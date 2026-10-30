//! Saved team plans (`plans/lineage-plans.json`): one per Champions Meeting and one for Team
//! Trials, each with the careers it asks for in order, which of them are done, what it was made
//! from, how it was chosen, and edits waiting for a re-plan. Nothing here simulates: making a
//! plan (`make_cm_plan`, `make_tt_plan`) is the only step that does, and the page runs it only
//! when the player asks (Create plan, Re-plan, Reset).

use super::brief::Brief;
use super::jobs::Run;
use super::progress::Stage;
use super::team::{CmTeam, TeamOpts};
use super::tt::{TtPins, TtTeam};
use super::{Inventory, uma_name};
use crate::App;
use crate::knowledge::{EventPreset, Knowledge};
use crate::veterans::Veteran;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "plans/lineage-plans.json";
/// Past plans kept (replaced by a re-plan or a reset), newest first.
const PAST: usize = 10;
/// Recent actions kept for the overview.
const RECENT: usize = 8;

/// A team the planner built, with everything the page shows already worked out.
#[derive(Serialize, Deserialize, Clone)]
pub(crate) enum Built {
    Cm { team: CmTeam, next: Option<Brief>, queue: Vec<QueueRow>, borrows: Vec<(Option<String>, Vec<(u32, String)>)> },
    Tt { team: TtTeam, next: Option<Brief>, lengths: Vec<String> },
}

/// One row of the parent queue, in words.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct QueueRow {
    pub trainee: u32,
    pub for_runner: u32,
    pub what: String,
    pub legacies: Vec<(u64, u32)>,
    pub careers: String,
    pub style: String,
    /// The step's deck and its borrowed card, with why and fallbacks (from its brief).
    pub deck: Vec<u32>,
    pub friend: Option<u32>,
    pub friend_why: Option<String>,
    pub friend_alts: Vec<(u32, String)>,
}

/// Which plan: a Champions Meeting's (by number) or Team Trials.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Cm(u32),
    Tt,
}

/// What a plan was made from, to tell when it may be out of date: the finished umas imported,
/// when the meta sheet and the Team Trials meta were read.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Basis {
    pub vets: Vec<u64>,
    pub meta: String,
    pub tt_meta: String,
}

/// One career the plan asks for, and whether it's done.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Career {
    /// Stable within the plan: "q2" (the CM queue's third parent step), "r<outfit>" (a CM
    /// runner's own career), "tt:<race>:<outfit>" or "tt:<race>:<outfit>:p<step>".
    pub key: String,
    /// One line: "Gold Ship's parent career for Agnes Digital".
    pub label: String,
    pub brief: Brief,
    pub done: Option<Done>,
    /// Her own career (she races for the team), else a parent career.
    #[serde(default)]
    pub own: bool,
    /// The Team Trials race it helps.
    #[serde(default)]
    pub race: Option<String>,
    /// What it's for, in one plain sentence.
    #[serde(default)]
    pub what_for: String,
    /// How many careers it usually takes: "usually 3 careers; 1 in 10 need 8 or more".
    #[serde(default)]
    pub usually: String,
}

/// How many careers a step usually takes, from one career's chance of its must-haves (none for
/// a runner's own career, which is kept or run again by its keep-if).
pub fn usually(p: Option<f64>) -> String {
    let Some(p) = p else { return "1 career".into() };
    let (typical, nine) = super::verify::step_spread(p);
    let n = |k: u32| format!("{k} career{}", if k == 1 { "" } else { "s" });
    if typical == u32::MAX {
        "it may never come: plan another route".into()
    } else if nine <= typical {
        format!("usually {}", n(typical))
    } else {
        format!("usually {}; 1 in 10 need {nine} or more", n(typical))
    }
}

/// A parent career's reason: who it's for and what she passes on.
fn parent_for(kb: &Knowledge, b: &Brief, runner: u32, race: Option<&str>) -> String {
    let at = race.map_or(String::new(), |r| format!(" ({r})"));
    let (me, who) = (super::short_name(kb, b.trainee), super::short_name(kb, runner));
    if b.must.is_empty() {
        format!("Make {me} a parent for {who}{at}: her career only has to finish.")
    } else {
        let what: Vec<String> = b.must.iter().map(|w| w.chip(kb).1).collect();
        format!("Raise {me}'s chance to pass {} on to {who}{at}.", what.join(" and "))
    }
}

/// A career marked done: the finished uma that ran it (if the player picked one) and when.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Done {
    pub vet: Option<u64>,
    pub at: String,
}

/// The player's choices the planner keeps: runners pinned (CM: outfits; Team Trials: an outfit to
/// a race) and characters left out.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Choices {
    pub pin: Vec<u32>,
    pub tt_pin: Vec<(u32, String)>,
    pub exclude: Vec<u32>,
}

/// An edit waiting for the next re-plan.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Edit {
    /// Keep this outfit on the team (in this race, for Team Trials).
    Pin { uma: u32, race: Option<String> },
    /// Leave this character out.
    Exclude { chara: u32 },
    /// Undo a pin or an exclusion made earlier.
    Unpin { uma: u32 },
    Include { chara: u32 },
}

impl Edit {
    pub fn describe(&self, kb: &Knowledge) -> String {
        match self {
            Edit::Pin { uma, race: Some(r) } => format!("pins {} to {r}", uma_name(kb, *uma)),
            Edit::Pin { uma, race: None } => format!("keeps {}", uma_name(kb, *uma)),
            Edit::Exclude { chara } => format!("drops {}", chara_name(kb, *chara)),
            Edit::Unpin { uma } => format!("unpins {}", uma_name(kb, *uma)),
            Edit::Include { chara } => format!("lets {} back in", chara_name(kb, *chara)),
        }
    }
}

fn chara_name(kb: &Knowledge, chara: u32) -> String {
    kb.umas.keys().filter(|u| crate::affinity::chara(**u) == chara).min().map_or_else(|| format!("character {chara}"), |u| uma_name(kb, *u))
}

impl Choices {
    /// The choices with these edits made.
    pub fn with(&self, edits: &[Edit]) -> Choices {
        let mut c = self.clone();
        for e in edits {
            match e {
                Edit::Pin { uma, race } => {
                    let ch = crate::affinity::chara(*uma);
                    c.exclude.retain(|x| *x != ch);
                    match race {
                        Some(r) => {
                            c.tt_pin.retain(|(u, _)| crate::affinity::chara(*u) != ch);
                            c.tt_pin.push((*uma, r.clone()));
                        }
                        None if !c.pin.contains(uma) => c.pin.push(*uma),
                        None => {}
                    }
                }
                Edit::Exclude { chara } => {
                    c.pin.retain(|u| crate::affinity::chara(*u) != *chara);
                    c.tt_pin.retain(|(u, _)| crate::affinity::chara(*u) != *chara);
                    if !c.exclude.contains(chara) {
                        c.exclude.push(*chara);
                    }
                }
                Edit::Unpin { uma } => {
                    c.pin.retain(|u| u != uma);
                    c.tt_pin.retain(|(u, _)| u != uma);
                }
                Edit::Include { chara } => c.exclude.retain(|x| x != chara),
            }
        }
        c
    }
}

/// A saved team plan.
#[derive(Serialize, Deserialize, Clone)]
pub struct TeamPlan {
    pub which: Which,
    /// When it was made, on this computer's clock ("2026-10-28 18:12").
    pub made: String,
    pub basis: Basis,
    pub built: Built,
    pub careers: Vec<Career>,
    /// How it was chosen: the build's stages.
    pub log: Vec<Stage>,
    pub choices: Choices,
    #[serde(default)]
    pub pending: Vec<Edit>,
    /// Notices the player answered "Not now" to.
    #[serde(default)]
    pub dismissed: Vec<String>,
    /// How to raise the plan, measured while it was made (`ceiling.rs`), strongest first.
    /// None for a plan made before the ceiling was measured.
    #[serde(default)]
    pub ceiling: Option<Vec<super::ceiling::Lever>>,
    /// What the ceiling search tried.
    #[serde(default)]
    pub tried: super::ceiling::Tried,
}

/// Something that changed under a plan, and what the player can do about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub id: String,
    pub text: String,
    pub act: Act,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    MarkDone { key: String, vet: u64 },
    Replan,
    /// Plan for this newly announced CM.
    PlanCm(u32),
}

impl TeamPlan {
    /// How to raise the plan, without what finished careers made moot: a parent for a runner
    /// whose parent careers are all done saves nothing now.
    pub fn live_ceiling(&self) -> Option<Vec<super::ceiling::Lever>> {
        let v = self.ceiling.as_ref()?;
        Some(
            v.iter()
                .filter(|l| {
                    l.kind != super::ceiling::LeverKind::Parent || {
                        let mut parents = self.careers.iter().filter(|c| !c.own && c.brief.for_runner == l.uma).peekable();
                        parents.peek().is_none() || parents.any(|c| c.done.is_none())
                    }
                })
                .cloned()
                .collect(),
        )
    }

    /// The next career: the first not done.
    pub fn next(&self) -> Option<&Career> {
        self.careers.iter().find(|c| c.done.is_none())
    }

    /// Careers done, and in all.
    pub fn progress(&self) -> (usize, usize) {
        (self.careers.iter().filter(|c| c.done.is_some()).count(), self.careers.len())
    }

    pub fn mark_done(&mut self, key: &str, vet: Option<u64>, at: &str) -> Result<String, String> {
        let c = self.careers.iter_mut().find(|c| c.key == key).ok_or("That career isn't in the plan.")?;
        if c.done.is_some() {
            return Err(format!("{} is already marked done.", c.label));
        }
        c.done = Some(Done { vet, at: at.into() });
        Ok(match vet {
            Some(v) => format!("Marked #{v} done: {}", c.label),
            None => format!("Marked done: {}", c.label),
        })
    }

    pub fn unmark(&mut self, key: &str) -> bool {
        self.careers.iter_mut().find(|c| c.key == key).is_some_and(|c| c.done.take().is_some())
    }

    /// The edits waiting, with the choices they'd make.
    pub fn choices_after(&self) -> Choices {
        self.choices.with(&self.pending)
    }

    /// What changed under the plan since it was made, checked against the account as it is now
    /// (no simulation): finished umas that meet a career's must-haves, a newer meta sheet, other
    /// imports, and a newer Champions Meeting announced. Dismissed ones are left out.
    pub fn notices(&self, kb: &Knowledge, vets: &[Veteran], now: &Basis, next_cm: Option<(u32, &str)>) -> Vec<Notice> {
        let mut out = Vec::new();
        let new: Vec<&Veteran> = vets.iter().filter(|v| !self.basis.vets.contains(&v.trained_chara_id)).collect();
        let mut matched = Vec::new();
        for c in self.careers.iter().filter(|c| c.done.is_none()) {
            for v in &new {
                if v.card_id != c.brief.trainee || matched.contains(&v.trained_chara_id) {
                    continue;
                }
                let own = v.factors();
                let parents_ok = c.brief.legacies.is_empty() || {
                    let mut want: Vec<u32> = c.brief.legacies.iter().map(|l| l.1).collect();
                    let mut got: Vec<u32> = [10, 20].iter().filter_map(|p| v.ancestor(*p).map(|a| a.card_id)).collect();
                    want.sort_unstable();
                    got.sort_unstable();
                    want == got
                };
                if parents_ok && c.brief.must.iter().all(|w| w.met(&own)) {
                    matched.push(v.trained_chara_id);
                    let what = if c.brief.must.is_empty() { "finished".to_string() } else { "meets the must-haves of".to_string() };
                    out.push(Notice {
                        id: format!("done:{}:{}", c.key, v.trained_chara_id),
                        text: format!("#{} {} {what} {}.", v.trained_chara_id, uma_name(kb, v.card_id), c.label),
                        act: Act::MarkDone { key: c.key.clone(), vet: v.trained_chara_id },
                    });
                }
            }
        }
        if !now.meta.is_empty() && now.meta != self.basis.meta && matches!(self.which, Which::Cm(_)) {
            out.push(Notice { id: format!("meta:{}", now.meta), text: format!("The meta sheet was read again ({}); this plan used the one from {}.", now.meta, or_unknown(&self.basis.meta)), act: Act::Replan });
        }
        if !now.tt_meta.is_empty() && now.tt_meta != self.basis.tt_meta && self.which == Which::Tt {
            out.push(Notice { id: format!("ttmeta:{}", now.tt_meta), text: format!("The Team Trials meta was read again ({}); this plan used the one from {}.", now.tt_meta, or_unknown(&self.basis.tt_meta)), act: Act::Replan });
        }
        let others = new.len() - matched.len();
        if others > 0 {
            out.push(Notice {
                id: format!("vets:{}", new.iter().map(|v| v.trained_chara_id).max().unwrap_or(0)),
                text: format!("{others} finished uma{} imported since this plan was made; a re-plan can use {}.", if others == 1 { "" } else { "s" }, if others == 1 { "her" } else { "them" }),
                act: Act::Replan,
            });
        }
        if let (Which::Cm(id), Some((next, name))) = (self.which, next_cm) {
            if next > id {
                out.push(Notice { id: format!("cm:{next}"), text: format!("CM {next} ({name}) is next; this plan is for CM {id}."), act: Act::PlanCm(next) });
            }
        }
        out.retain(|n| !self.dismissed.contains(&n.id));
        out
    }
}

fn or_unknown(s: &str) -> &str {
    if s.is_empty() { "an earlier read" } else { s }
}

/// Something the player did, for the overview's Recent list.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Recent {
    pub at: String,
    pub text: String,
    /// A mark done that Undo can take back: the plan and the career.
    pub undo: Option<(Which, String)>,
}

/// Every saved team plan.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Plans {
    pub cm: Vec<TeamPlan>,
    pub tt: Option<TeamPlan>,
    pub past: Vec<TeamPlan>,
    pub recent: Vec<Recent>,
}

impl Plans {
    pub fn load(root: &Path) -> Plans {
        std::fs::read_to_string(root.join(FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> Result<(), String> {
        let path = root.join(FILE);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("Couldn't save {}: {e}", path.display()))
    }

    pub fn get(&self, which: Which) -> Option<&TeamPlan> {
        match which {
            Which::Tt => self.tt.as_ref(),
            Which::Cm(id) => self.cm.iter().find(|p| p.which == Which::Cm(id)),
        }
    }

    pub fn get_mut(&mut self, which: Which) -> Option<&mut TeamPlan> {
        match which {
            Which::Tt => self.tt.as_mut(),
            Which::Cm(id) => self.cm.iter_mut().find(|p| p.which == Which::Cm(id)),
        }
    }

    /// Keep a newly made plan; the one it replaces goes to the past plans. Its finished careers
    /// carry over where the new plan asks for the same career.
    pub fn put(&mut self, mut plan: TeamPlan, at: &str) {
        let old = self.take(plan.which);
        if let Some(old) = old {
            for c in &mut plan.careers {
                if let Some(d) = old.careers.iter().find(|o| o.key == c.key && o.brief.trainee == c.brief.trainee).and_then(|o| o.done.clone()) {
                    c.done = Some(d);
                }
            }
            self.retire(old);
        }
        let text = match plan.which {
            Which::Cm(id) => format!("Made the CM {id} plan: {} careers", plan.careers.len()),
            Which::Tt => format!("Made the Team Trials plan: {} careers", plan.careers.len()),
        };
        match plan.which {
            Which::Tt => self.tt = Some(plan),
            Which::Cm(_) => {
                self.cm.push(plan);
                self.cm.sort_by_key(|p| match p.which {
                    Which::Cm(id) => id,
                    Which::Tt => 0,
                });
            }
        }
        self.log(at, text, None);
    }

    /// Throw a plan away (it's kept with the past plans).
    pub fn reset(&mut self, which: Which, at: &str) -> bool {
        let Some(old) = self.take(which) else { return false };
        self.retire(old);
        self.log(at, format!("Reset the {} plan", label(which)), None);
        true
    }

    fn take(&mut self, which: Which) -> Option<TeamPlan> {
        match which {
            Which::Tt => self.tt.take(),
            Which::Cm(_) => self.cm.iter().position(|p| p.which == which).map(|i| self.cm.remove(i)),
        }
    }

    fn retire(&mut self, old: TeamPlan) {
        self.past.insert(0, old);
        self.past.truncate(PAST);
    }

    pub fn mark_done(&mut self, which: Which, key: &str, vet: Option<u64>, at: &str) -> Result<String, String> {
        let msg = self.get_mut(which).ok_or("No plan to mark.")?.mark_done(key, vet, at)?;
        self.log(at, msg.clone(), Some((which, key.to_string())));
        Ok(msg)
    }

    /// Undo a Recent entry's mark done.
    pub fn undo(&mut self, i: usize) -> bool {
        let Some((which, key)) = self.recent.get(i).and_then(|r| r.undo.clone()) else { return false };
        let ok = self.get_mut(which).is_some_and(|p| p.unmark(&key));
        if ok {
            self.recent.remove(i);
        }
        ok
    }

    pub fn log(&mut self, at: &str, text: String, undo: Option<(Which, String)>) {
        self.recent.insert(0, Recent { at: at.into(), text, undo });
        self.recent.truncate(RECENT);
    }
}

pub fn label(which: Which) -> String {
    match which {
        Which::Cm(id) => format!("CM {id}"),
        Which::Tt => "Team Trials".into(),
    }
}

/// The time on this computer's clock, as plans record it.
pub fn stamp() -> String {
    let (y, m, d, h, mi, _) = crate::live::local(crate::live::now());
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}")
}

/// Why a plan can't be made yet: the account has no umas or no support cards in umaLord.
pub fn not_set_up(umas: usize, cards: usize) -> Option<String> {
    match (umas, cards) {
        (0, 0) => Some("Add the umas and support cards you own first (Settings → Profile → Set up my collection, or load a profile): a plan is built from them.".into()),
        (0, _) => Some("Add the umas you own first (Settings → Profile → Set up my collection): a plan picks its runners and parents from them.".into()),
        (_, 0) => Some("Add the support cards you own first (Settings → Profile → Set up my collection): every career's deck is built from them.".into()),
        _ => None,
    }
}

/// What a plan says when the account has no finished umas: it's planned as a fresh account.
pub const FRESH: &str = "No finished umas are imported, so this is planned as a fresh account: every runner starts from scratch and every parent career is planned from nothing.";

impl App {
    /// What a plan made now would be made from.
    pub(crate) fn plan_basis(&self) -> Basis {
        let mut vets: Vec<u64> = self.veterans().iter().map(|v| v.trained_chara_id).collect();
        vets.sort_unstable();
        Basis { vets, meta: self.kb.pvp_meta.as_ref().map(|m| m.fetched.clone()).unwrap_or_default(), tt_meta: self.tt_meta().read.clone() }
    }

    /// Make a Champions Meeting plan: the team, every career it asks for with its brief, in
    /// order (the parent queue, hardest first, then each runner's own career, the ace first).
    /// Runs the simulator; reports through `progress` when a build set it.
    pub(crate) fn make_cm_plan(&mut self, preset: &EventPreset, choices: &Choices) -> Result<TeamPlan, String> {
        self.stage("Reading your account")?;
        if let Some(why) = not_set_up(self.roster.umas.len(), self.roster.cards.len()) {
            return Err(why);
        }
        let basis = self.plan_basis();
        let inv = self.parent_inventory();
        if inv.parents.is_empty() {
            self.note(FRESH);
        }
        self.note(format!(
            "{} umas, {} finished umas, {} support cards; meta sheet read {}.",
            self.roster.umas.len(),
            inv.parents.len(),
            self.roster.cards.len(),
            if basis.meta.is_empty() { "never".into() } else { basis.meta.clone() }
        ));
        let opts = TeamOpts { pin: choices.pin.clone(), exclude: choices.exclude.clone(), ..Default::default() };
        let team = self.cm_team(preset, &opts)?;
        self.stage("Writing up each career")?;
        let mut careers = Vec::new();
        let mut queue = Vec::new();
        for (k, (i, run)) in team.queue.iter().enumerate() {
            let runner = team.runners[*i].trainee;
            let mut q = queue_row(self, &inv, runner, run);
            let step = team.runners[*i].parents.steps.iter().position(|s| s == run).unwrap_or(0);
            let b = self.parent_brief(&team.runners[*i].goal, run, &inv, step);
            (q.deck, q.friend, q.friend_why, q.friend_alts) = (b.deck.clone(), b.friend, b.friend_why.clone(), b.friend_alts.clone());
            queue.push(q);
            let what_for = parent_for(&self.kb, &b, runner, None);
            let usual = usually(b.p);
            careers.push(Career { key: format!("q{k}"), label: format!("{}'s parent career for {}", uma_name(&self.kb, run.trainee), uma_name(&self.kb, runner)), brief: b, what_for, usually: usual, ..Default::default() });
            self.check_cancel()?;
        }
        // Each runner's own career, the ace (most wins head to head) first.
        let mut order: Vec<usize> = (0..team.runners.len()).collect();
        order.sort_by(|a, b| team.runners[*b].sim.map_or(0.0, |s| s.0).total_cmp(&team.runners[*a].sim.map_or(0.0, |s| s.0)));
        for (n, i) in order.into_iter().enumerate() {
            let r = &team.runners[i];
            let parents = r.parents.now.as_ref().map_or("her best two finished umas".into(), |l| l.legacies.iter().map(|x| super::parent_name(&self.kb, &inv.parents[*x])).collect::<Vec<_>>().join(" and "));
            let why = if n == 0 {
                format!("Her own career for CM {} ({}): she wins most head to head, so she's the ace.", team.event.id, team.event.name)
            } else {
                format!("Her own career for CM {} ({}), as the team's {}.", team.event.id, team.event.name, r.style.label())
            };
            let mut b = self.runner_brief(why, &r.plan, Some((r.plan.target[1], team.event.course_name.clone())), parents);
            b.legacies = super::brief::legacies_of(r.parents.now.as_ref(), &inv);
            (b.friend_why, b.friend_alts) = self.friend_picks(&r.plan);
            let again = team.today.iter().any(|t| crate::affinity::chara(t.outfit) == crate::affinity::chara(r.trainee));
            let what_for = format!("Race as the team's {}{}{}.", r.style.label(), if n == 0 { " and its ace" } else { "" }, if again { ": a new career, built for this course, beats the one you have" } else { "" });
            careers.push(Career { key: format!("r{}", r.trainee), label: format!("{}'s own career", uma_name(&self.kb, r.trainee)), brief: b, own: true, what_for, usually: usually(None), ..Default::default() });
            self.check_cancel()?;
        }
        self.note(format!("{} careers in order: the parent careers first, hardest first, then each runner's own.", careers.len()));
        self.stage("Looking for ways to raise the ceiling")?;
        let (ceiling, tried) = self.cm_ceiling(&team, preset, &inv);
        self.check_cancel()?;
        self.note(ceiling_line(&ceiling));
        let borrows = team.runners.iter().map(|r| self.friend_picks(&r.plan)).collect();
        let next = careers.first().map(|c| c.brief.clone());
        Ok(TeamPlan { which: Which::Cm(preset.id), made: stamp(), basis, built: Built::Cm { team, next, queue, borrows }, careers, log: Vec::new(), choices: choices.clone(), pending: Vec::new(), dismissed: Vec::new(), ceiling: Some(ceiling), tried })
    }

    /// Make a Team Trials plan: the team and every career it asks for, the one that raises the
    /// team most first, then the weakest runners' (parent careers before their own).
    pub(crate) fn make_tt_plan(&mut self, choices: &Choices) -> Result<TeamPlan, String> {
        self.stage("Reading your account")?;
        if let Some(why) = not_set_up(self.roster.umas.len(), self.roster.cards.len()) {
            return Err(why);
        }
        let basis = self.plan_basis();
        let inv = self.parent_inventory();
        if inv.parents.is_empty() {
            self.note(FRESH);
        }
        self.note(format!(
            "{} umas, {} finished umas, {} support cards; Team Trials meta read {}.",
            self.roster.umas.len(),
            inv.parents.len(),
            self.roster.cards.len(),
            if basis.tt_meta.is_empty() { "never".into() } else { basis.tt_meta.clone() }
        ));
        let team = self.tt_team(200, &TtPins { pin: choices.tt_pin.clone(), exclude: choices.exclude.clone() })?;
        let vets = self.veterans();
        let meta = self.tt_meta();
        self.stage("Writing up each career")?;
        let mut careers: Vec<Career> = Vec::new();
        let first = self.tt_next(&team, &inv);
        // Where the first career sits: a replacement takes its slot's race, and the runner she
        // replaces needs no career.
        let mut replaced: Vec<(String, u32)> = Vec::new();
        if let Some(b) = &first {
            let slot_of = |t: u32| team.rows.iter().find(|r| r.slots.iter().any(|s| s.trainee == t)).map(|r| r.name.to_string());
            let (race, runner) = match (b.replaces, b.for_runner) {
                (Some(old), _) => (slot_of(old), b.trainee),
                (None, Some(r)) => (slot_of(r), r),
                (None, None) => (slot_of(b.trainee), b.trainee),
            };
            let race = race.unwrap_or_default();
            if let Some(old) = b.replaces {
                replaced.push((race.clone(), old));
            }
            let key = match b.step {
                Some(_) => format!("tt:{race}:{runner}:p0"),
                None => format!("tt:{race}:{}", b.trainee),
            };
            let own = b.step.is_none();
            let mut b = b.clone();
            // A replacement's parents are her best pair, which doesn't carry the inherited unique.
            let meta = self.tt_meta();
            if own && b.replaces.is_some() && meta.inherit.uma > 0 && crate::affinity::chara(meta.inherit.uma) != crate::affinity::chara(b.trainee) {
                self.borrow_for_inherit(&mut b, &meta, &race, &inv);
            }
            let b = &b;
            let what_for = match (b.replaces, own) {
                (Some(old), _) => format!("Replace {old_name} in {race}: {old_name}'s unique skill fires unreliably.", old_name = super::short_name(&self.kb, old)),
                (None, false) => parent_for(&self.kb, b, runner, Some(&race)),
                // "Again" only when she's finished in this very outfit; another outfit is another career.
                (None, true) if vets.iter().any(|v| v.card_id == b.trainee) => format!("Run her again for {race}: she scores lowest on the team, so a better run raises it most."),
                (None, true) => format!("Train her first for {race}: she scores lowest on the team, so her career raises it most."),
            };
            careers.push(Career { key, label: tt_label(&self.kb, b, &race), brief: b.clone(), own, race: Some(race.clone()), what_for, usually: usually(if own { None } else { b.p }), ..Default::default() });
        }
        // The rest: runners who aren't on today's team as they are, weakest first.
        let mut slots: Vec<(&str, &super::tt::Slot)> = team
            .rows
            .iter()
            .flat_map(|r| r.slots.iter().map(move |s| (r.name, s)))
            .filter(|(race, s)| !team.rows.iter().any(|r| r.name == *race && r.today.iter().any(|t| t.0.outfit == s.trainee)))
            .collect();
        slots.sort_by(|a, b| a.1.total.total_cmp(&b.1.total));
        for (race, s) in slots {
            if replaced.iter().any(|(r, t)| r == race && *t == s.trainee) {
                continue;
            }
            let have = |careers: &[Career], key: &str| careers.iter().any(|c| c.key == key);
            let goal = s.parents_goal(race);
            if !s.ready {
                for (k, run) in s.parents.steps.iter().enumerate() {
                    if have(&careers, &format!("tt:{race}:{}:p{k}", s.trainee)) {
                        continue;
                    }
                    let b = self.parent_brief(&goal, run, &inv, k);
                    let what_for = parent_for(&self.kb, &b, s.trainee, Some(race));
                    let usual = usually(b.p);
                    careers.push(Career { key: format!("tt:{race}:{}:p{k}", s.trainee), label: tt_label(&self.kb, &b, race), brief: b, race: Some(race.to_string()), what_for, usually: usual, ..Default::default() });
                }
            }
            if have(&careers, &format!("tt:{race}:{}", s.trainee)) {
                continue;
            }
            let parents = s.parents.now.as_ref().map_or("her best two finished umas".into(), |l| l.legacies.iter().map(|i| super::parent_name(&self.kb, &inv.parents[*i])).collect::<Vec<_>>().join(" and "));
            let mut b = self.runner_brief(format!("Her own career for Team Trials ({race})."), &s.plan, Some((s.plan.target[1], race.to_string())), parents);
            b.legacies = super::brief::legacies_of(s.parents.now.as_ref(), &inv);
            (b.friend_why, b.friend_alts) = self.friend_picks(&s.plan);
            if s.notes.iter().any(|n| n.contains("borrow a friend's")) {
                self.borrow_for_inherit(&mut b, &meta, race, &inv);
            }
            // Where her character races today, if anywhere: here, or another race.
            let chara = crate::affinity::chara(s.trainee);
            let now_in = team.rows.iter().find(|r| r.today.iter().any(|t| crate::affinity::chara(t.0.outfit) == chara)).map(|r| r.name);
            let lead = if s.ace { format!("Lead {race} as its Ace") } else { format!("Join {race}") };
            let what_for = match now_in {
                Some(r) if r == race => format!("{lead}: a new career built for it scores more than the one racing now."),
                Some(r) => {
                    format!("{lead}: she moves over from {r}, with a new career built for {race}.")
                }
                None if vets.iter().any(|v| crate::affinity::chara(v.card_id) == chara) => {
                    format!("{lead}: none of your finished {}s can race it as they are.", super::short_name(&self.kb, s.trainee))
                }
                None => format!("{lead}: you have no finished {} yet.", super::short_name(&self.kb, s.trainee)),
            };
            careers.push(Career { key: format!("tt:{race}:{}", s.trainee), label: tt_label(&self.kb, &b, race), brief: b, own: true, race: Some(race.to_string()), what_for, usually: usually(None), ..Default::default() });
            self.check_cancel()?;
        }
        self.note(format!("{} careers in order: the one that raises the team most first, then the weakest runners', parent careers before their own.", careers.len()));
        let meta = self.tt_meta();
        self.stage("Looking for ways to raise the ceiling")?;
        let (ceiling, tried) = self.tt_ceiling(&team, &meta);
        self.check_cancel()?;
        self.note(ceiling_line(&ceiling));
        let lengths = team
            .rows
            .iter()
            .map(|r| {
                let ls: Vec<u32> = meta.tracks.get(r.name).and_then(|v| v.as_object()).map(|o| o.keys().filter_map(|k| k.parse().ok()).collect()).unwrap_or_default();
                match (ls.iter().min(), ls.iter().max()) {
                    (Some(a), Some(b)) if a != b => format!("{a}–{b} m"),
                    (Some(a), _) => format!("{a} m"),
                    _ => String::new(),
                }
            })
            .collect();
        Ok(TeamPlan { which: Which::Tt, made: stamp(), basis, built: Built::Tt { team, next: first, lengths }, careers, log: Vec::new(), choices: choices.clone(), pending: Vec::new(), dismissed: Vec::new(), ceiling: Some(ceiling), tried })
    }
}

impl App {
    /// A Team Trials runner whose parents don't carry the community's inherited unique: borrow
    /// a friend's uma whose unique it is, beside whichever of the player's own finished umas
    /// still starts her race at A (her parents become that one, and the borrow).
    fn borrow_for_inherit(&self, b: &mut Brief, meta: &super::tt::TtMeta, race: &str, inv: &Inventory) {
        let (surface, distance) = super::tt::race_grades(race);
        let goal = super::Goal {
            runner: b.trainee,
            targets: vec![usize::from(surface == crate::knowledge::Surface::Dirt), 2 + crate::knowledge::Distance::ALL.iter().position(|d| *d == distance).unwrap_or(1), 6 + b.style as usize],
            ..Default::default()
        };
        let partner = self.inherit_partner(&goal, inv, meta.inherit.uma);
        if let Some(p) = partner.and_then(|id| inv.by_id(id)) {
            b.legacies = vec![(p.id, p.uma)];
        }
        b.borrow_parent = Some(meta.inherit.uma);
        b.inherit = Some(inherit_text(&self.kb, meta, partner.is_some()));
    }
}

/// The community's inherited unique for a Team Trials runner, explained where it's asked for:
/// a headline, then (after a newline) what the skill is, why it needs that parent, and how.
fn inherit_text(kb: &Knowledge, meta: &super::tt::TtMeta, paired: bool) -> String {
    let parent = uma_name(kb, meta.inherit.uma);
    let keep = if paired {
        "Your parent above keeps the race's surface, distance and style at A."
    } else {
        "The other parent needs the pink sparks for the race's surface, distance and style."
    };
    format!("Why borrow {parent}: for her skill “{skill}”\nOnly a parent can pass on her unique skill, and you don't own her. {keep}", skill = meta.inherit.skill)
}

/// The ceiling stage's last line: what it found.
fn ceiling_line(v: &[super::ceiling::Lever]) -> String {
    match v.first() {
        Some(l) => format!("{} way{} to raise it; the most: {} ({}).", v.len(), if v.len() == 1 { "" } else { "s" }, l.title, l.gain_text()),
        None => "Nothing tried raised the plan by enough to list.".into(),
    }
}

fn tt_label(kb: &Knowledge, b: &Brief, race: &str) -> String {
    match (b.step, b.for_runner) {
        (Some(_), Some(r)) => format!("{}'s parent career for {} ({race})", uma_name(kb, b.trainee), uma_name(kb, r)),
        _ => format!("{}'s own career ({race})", uma_name(kb, b.trainee)),
    }
}

/// A queue row from a step.
pub(crate) fn queue_row(app: &App, inv: &Inventory, runner: u32, run: &Run) -> QueueRow {
    let legacies = run.legacies.iter().zip(run.legacy_umas).map(|(id, uma)| (*id, if uma == 0 { inv.by_id(*id).map_or(0, |p| p.uma) } else { uma })).collect();
    let what = if run.must.is_empty() { "just finishing her".to_string() } else { run.must.iter().map(|w| w.label(&app.kb)).collect::<Vec<_>>().join(", ") };
    let style = app.kb.umas.get(&run.trainee).map(|u| u.aptitude.best_style().label()).unwrap_or("").to_string();
    let (typical, _) = super::verify::step_spread(run.p);
    QueueRow { trainee: run.trainee, for_runner: runner, what, legacies, careers: format!("usually {typical} career{}", if typical == 1 { "" } else { "s" }), style, deck: Vec::new(), friend: None, friend_why: None, friend_alts: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::super::jobs::Want;
    use super::super::tests::{kb, spark};
    use super::*;
    use serde_json::json;

    fn vet(id: u64, card: u32, factors: &[u32], parents: &[(u32, u32)]) -> Veteran {
        serde_json::from_value(json!({
            "trained_chara_id": id, "card_id": card, "create_time": "2026-10-28 10:00:00",
            "factor_info_array": factors.iter().map(|f| json!({"factor_id": f})).collect::<Vec<_>>(),
            "succession_chara_array": parents.iter().map(|(pos, c)| json!({"position_id": pos, "card_id": c})).collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    fn career(key: &str, trainee: u32, must: Vec<Want>, legacies: Vec<(u64, u32)>) -> Career {
        Career { key: key.into(), label: format!("career {key}"), brief: Brief { trainee, must, legacies, ..Default::default() }, ..Default::default() }
    }

    /// A Team Trials plan made with finished umas 1 and 2: Haru Urara's parent career (Dirt 1★,
    /// with umas 100101 and 100201 as parents), then Oguri Cap's own.
    fn plan() -> TeamPlan {
        TeamPlan {
            which: Which::Tt,
            made: "2026-10-28 18:12".into(),
            basis: Basis { vets: vec![1, 2], meta: "2026-10-21".into(), tt_meta: "2026-10-01".into() },
            built: Built::Tt { team: TtTeam::default(), next: None, lengths: Vec::new() },
            careers: vec![career("tt:Dirt:100601:p0", 105201, vec![Want::Pink { aptitude: 1, stars: 1 }], vec![(1, 100101), (2, 100201)]), career("tt:Dirt:100601", 100601, Vec::new(), Vec::new())],
            log: Vec::new(),
            choices: Choices::default(),
            pending: Vec::new(),
            dismissed: Vec::new(),
            ceiling: None,
            tried: Default::default(),
        }
    }

    #[test]
    fn edits_wait_and_combine() {
        let c = Choices { pin: vec![100601], tt_pin: vec![], exclude: vec![1052] };
        let after = c.with(&[Edit::Pin { uma: 105201, race: None }, Edit::Exclude { chara: 1006 }]);
        // Pinning lets her character back in; leaving one out drops her pins.
        assert_eq!(after.exclude, vec![1006]);
        assert_eq!(after.pin, vec![105201]);
        let tt = Choices::default().with(&[Edit::Pin { uma: 100601, race: Some("Dirt".into()) }, Edit::Pin { uma: 100602, race: Some("Mile".into()) }]);
        // One outfit of a character per team.
        assert_eq!(tt.tt_pin, vec![(100602, "Mile".to_string())]);
        assert_eq!(tt.with(&[Edit::Unpin { uma: 100602 }]).tt_pin, vec![]);
    }

    #[test]
    fn mark_done_moves_next_and_undoes() {
        let mut plans = Plans::default();
        plans.put(plan(), "now");
        assert_eq!(plans.get(Which::Tt).and_then(TeamPlan::next).map(|c| c.key.as_str()), Some("tt:Dirt:100601:p0"));
        plans.mark_done(Which::Tt, "tt:Dirt:100601:p0", Some(3), "later").unwrap();
        let p = plans.get(Which::Tt).unwrap();
        assert_eq!(p.progress(), (1, 2));
        assert_eq!(p.next().map(|c| c.key.as_str()), Some("tt:Dirt:100601"));
        assert!(plans.mark_done(Which::Tt, "tt:Dirt:100601:p0", None, "later").is_err());
        assert!(plans.undo(0));
        assert_eq!(plans.get(Which::Tt).unwrap().progress(), (0, 2));
    }

    #[test]
    fn a_new_plan_keeps_finished_careers_and_retires_the_old() {
        let mut plans = Plans::default();
        plans.put(plan(), "now");
        plans.mark_done(Which::Tt, "tt:Dirt:100601:p0", Some(3), "later").unwrap();
        plans.put(plan(), "again");
        assert_eq!(plans.get(Which::Tt).unwrap().progress(), (1, 2));
        assert_eq!(plans.past.len(), 1);
        assert!(plans.reset(Which::Tt, "reset"));
        assert!(plans.get(Which::Tt).is_none());
        assert_eq!(plans.past.len(), 2);
    }

    /// A parent suggestion stops showing once that runner's parent careers are done.
    #[test]
    fn done_parent_careers_retire_their_suggestion() {
        use super::super::ceiling::{Lever, LeverKind, Unit};
        let mut p = plan();
        p.careers[0].brief.for_runner = Some(100601);
        p.ceiling = Some(vec![
            Lever { kind: LeverKind::Parent, uma: Some(100601), gain: 3.0, unit: Unit::Careers, ..Default::default() },
            Lever { kind: LeverKind::GetCard, uma: Some(100601), card: Some(30001), gain: 900.0, ..Default::default() },
        ]);
        assert_eq!(p.live_ceiling().map(|v| v.len()), Some(2));
        p.mark_done("tt:Dirt:100601:p0", Some(3), "now").unwrap();
        let live = p.live_ceiling().unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].kind, LeverKind::GetCard);
    }

    #[test]
    fn plans_round_trip_through_their_file() {
        let dir = std::env::temp_dir().join(format!("umalord-saved-{}", std::process::id()));
        let mut plans = Plans::default();
        plans.put(plan(), "now");
        plans.save(&dir).unwrap();
        let back = Plans::load(&dir);
        assert_eq!(back.get(Which::Tt).map(|p| p.careers.len()), Some(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn notices_say_what_changed_without_simulating() {
        let kb = kb();
        let p = plan();
        let dirt = spark(&kb, "pink", "Dirt", 1);
        let old = vec![vet(1, 100101, &[], &[]), vet(2, 100201, &[], &[])];
        let now = Basis { vets: vec![1, 2], meta: "2026-10-21".into(), tt_meta: "2026-10-01".into() };
        assert!(p.notices(&kb, &old, &now, None).is_empty());
        // A new Haru Urara with the must-have, from the step's parents: mark the step done.
        let mut vets = old.clone();
        vets.push(vet(3, 105201, &[dirt], &[(10, 100101), (20, 100201)]));
        // One without it, and one from other parents, only count as imports.
        vets.push(vet(4, 105201, &[], &[(10, 100101), (20, 100201)]));
        vets.push(vet(5, 105201, &[dirt], &[(10, 100101), (20, 100301)]));
        let n = p.notices(&kb, &vets, &now, None);
        assert_eq!(n[0].act, Act::MarkDone { key: "tt:Dirt:100601:p0".into(), vet: 3 });
        assert!(n.iter().any(|x| x.act == Act::Replan && x.text.starts_with("2 finished umas")));
        // A newer Team Trials meta asks for a re-plan; dismissing it hides it.
        let newer = Basis { tt_meta: "2026-11-01".into(), ..now.clone() };
        let n = p.notices(&kb, &old, &newer, None);
        assert_eq!(n.len(), 1);
        let mut q = p.clone();
        q.dismissed.push(n[0].id.clone());
        assert!(q.notices(&kb, &old, &newer, None).is_empty());
        // A CM plan hears when a later CM is next.
        let cm = TeamPlan { which: Which::Cm(21), ..p };
        assert_eq!(cm.notices(&kb, &old, &now, Some((22, "Aquarius Cup")))[0].act, Act::PlanCm(22));
    }

    /// Both team plans on a real account, made as Create plan makes them, with their logs:
    /// `UMALORD_ROOT=<account folder> cargo test --release team_plans_real_account -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn team_plans_real_account() {
        let root = std::path::PathBuf::from(std::env::var("UMALORD_ROOT").expect("UMALORD_ROOT"));
        let mut app = crate::agent::mcp::headless_app(Some(root));
        let cm = app.kb.upcoming_cms(&crate::ui_util::today()).first().map(|e| (*e).clone()).expect("an upcoming CM");
        for which in [Which::Cm(cm.id), Which::Tt] {
            let t = std::time::Instant::now();
            app.progress = Some(super::super::progress::Progress::new(None, Default::default()));
            let plan = match which {
                Which::Cm(_) => app.make_cm_plan(&cm, &Choices::default()),
                Which::Tt => app.make_tt_plan(&Choices::default()),
            };
            let log = app.progress.take().unwrap().finish();
            let mut plan = plan.unwrap_or_else(|e| panic!("{which:?}: {e}"));
            plan.log = log;
            eprintln!("\n== {} plan in {:.1} s ==", label(which), t.elapsed().as_secs_f64());
            for (i, s) in plan.log.iter().enumerate() {
                eprintln!("{}. {} ({:.1} s)", i + 1, s.title, s.secs.unwrap_or(0.0));
                for l in &s.lines {
                    eprintln!("     {l}");
                }
            }
            for c in &plan.careers {
                eprintln!("  [{}] {}", c.key, c.label);
            }
            for l in plan.ceiling.iter().flatten() {
                eprintln!("  ceiling: {} · {} · {} [{}]", l.gain_text(), l.title, l.why, l.measured);
            }
            assert!(plan.log.len() >= 5 && plan.log.iter().all(|s| s.secs.is_some()), "every stage logged and closed");
            // The page lists the stages to come from these.
            let all: &[&str] = if which == Which::Tt { &super::super::progress::TT_STAGES } else { &super::super::progress::CM_STAGES };
            assert!(plan.log.iter().all(|s| all.contains(&s.title.as_str())), "a stage the page doesn't list");
            assert!(!plan.careers.is_empty());
            let mut keys: Vec<&str> = plan.careers.iter().map(|c| c.key.as_str()).collect();
            keys.sort_unstable();
            keys.dedup();
            assert_eq!(keys.len(), plan.careers.len(), "career keys are unique");
            // It saves and reads back whole.
            let text = serde_json::to_string(&plan).unwrap();
            let back: TeamPlan = serde_json::from_str(&text).unwrap();
            assert_eq!(back.careers.len(), plan.careers.len());
            eprintln!("  saved: {} KB", text.len() / 1024);
        }
    }
}
