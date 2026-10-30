//! One button, a whole Team Trials team: five races (Sprint, Mile, Medium, Long, Dirt) with up
//! to three runners each, from the account's umas, each scored by an estimate of her Team
//! Trials points, the top slot of each race its Ace, and the career that would raise the team
//! most.
//!
//! The score follows `knowledge/mechanics.md` (Team Trials): placement points, estimated from
//! head-to-head races against the other candidates for that race in the simulator (labelled an
//! estimate: the real race has twelve runners); skill points, each skill's points (gold 1,200;
//! white or green 500; an inherited unique 500; her own unique by her stars) times the share of
//! races it fired in; and the positioning bonus for each running style on the race's team.

use super::jobs::Proposal;
use super::{Goal, Inventory};
use crate::App;
use crate::knowledge::{Distance, Style, Surface, grade_rank};
use crate::tab_planner::Plan;
use std::collections::BTreeMap;

/// The five races, with the stamina the playbook's guide asks for on random tracks.
pub const ROWS: [(&str, Surface, Distance, u32); 5] = [
    ("Sprint", Surface::Turf, Distance::Sprint, 450),
    ("Mile", Surface::Turf, Distance::Mile, 650),
    ("Medium", Surface::Turf, Distance::Medium, 750),
    ("Long", Surface::Turf, Distance::Long, 900),
    ("Dirt", Surface::Dirt, Distance::Mile, 650),
];

/// The community Team Trials meta (`knowledge/data/tt_meta.json`, hand-curated and sourced; how
/// to refresh it is in `knowledge/UPDATING.md`): each outfit's usual races and styles, track
/// lengths, stamina targets, the scenario, the inherit and the borrows.
#[derive(serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TtMeta {
    pub read: String,
    pub sources: Vec<MetaSource>,
    pub tracks: BTreeMap<String, serde_json::Value>,
    pub stamina: BTreeMap<String, serde_json::Value>,
    pub scenario: MetaScenario,
    pub inherit: MetaInherit,
    pub borrows: BTreeMap<String, serde_json::Value>,
    pub roles: MetaRoles,
}

#[derive(serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct MetaSource {
    pub name: String,
    pub url: String,
    pub updated: String,
}

/// The meta is community opinion that moves with every release: older than this, say so.
pub const STALE_DAYS: i64 = 45;

#[derive(serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct MetaScenario {
    pub name: String,
}

#[derive(serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct MetaInherit {
    pub uma: u32,
    pub skill: String,
}

#[derive(serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct MetaRoles {
    pub list: Vec<Role>,
}

#[derive(serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Role {
    pub id: u32,
    pub races: Vec<String>,
    pub styles: Vec<String>,
    pub unique: Option<String>,
    pub note: Option<String>,
}

impl TtMeta {
    /// Days since the meta was read, and whether that's past `STALE_DAYS`.
    pub fn age(&self, today: &str) -> (i64, bool) {
        let days = crate::tab_planner::days_apart(today, &self.read);
        (days, days > STALE_DAYS)
    }

    pub fn role(&self, uma: u32) -> Option<&Role> {
        self.roles.list.iter().find(|r| r.id == uma)
    }

    /// The race's most common track length.
    fn length(&self, race: &str) -> Option<u32> {
        self.tracks.get(race)?.as_object()?.iter().filter_map(|(k, v)| Some((k.parse::<u32>().ok()?, v.as_f64()?))).max_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0)
    }

    fn stamina(&self, race: &str) -> Option<u32> {
        self.stamina.get(race)?.as_u64().map(|v| v as u32)
    }

    fn borrows(&self, key: &str) -> Vec<u32> {
        self.borrows.get(key).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as u32)).collect()).unwrap_or_default()
    }
}

fn style_name(s: Style) -> &'static str {
    match s {
        Style::Front => "Front",
        Style::Pace => "Pace",
        Style::Late => "Late",
        Style::End => "End",
    }
}

pub(crate) fn parse_style(s: &str) -> Option<Style> {
    Some(match s {
        "Front" => Style::Front,
        "Pace" => Style::Pace,
        "Late" => Style::Late,
        "End" => Style::End,
        _ => return None,
    })
}

/// The player's choices for a Team Trials team: an outfit pinned to a race, and characters left
/// out. Re-planning keeps them.
#[derive(Clone, Debug, Default)]
pub struct TtPins {
    pub pin: Vec<(u32, String)>,
    pub exclude: Vec<u32>,
}

/// Positioning: 1,000 in each of the early, mid and late race per running style on the team.
const POSITIONING: f64 = 3000.0;

/// One runner in a race's team.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Slot {
    pub trainee: u32,
    pub style: Style,
    pub ace: bool,
    /// Her estimated points: placement, skills, positioning (shared by style), and the total
    /// with the Ace's +10%.
    pub placement: f64,
    pub skills: f64,
    pub total: f64,
    /// Her surface and distance grades are A or better without help.
    pub natural: bool,
    pub parents: Proposal,
    pub plan: Plan,
    /// Why she's here, and what to watch.
    pub notes: Vec<String>,
    /// Her own finished umas already start her race's grades at A (the inherit aside): her
    /// next career can be her own; otherwise a parent step comes first.
    pub ready: bool,
}

/// A race's name, one of `ROWS`. An alias so serde doesn't take the `&'static str` as borrowed
/// from the saved text.
pub type RowName = &'static str;

/// A saved race's name, as the `ROWS` entry it names (an unknown name reads as the first).
fn row_name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<&'static str, D::Error> {
    let name = <String as serde::Deserialize>::deserialize(d)?;
    Ok(ROWS.iter().find(|r| r.0 == name).map_or(ROWS[0].0, |r| r.0))
}

/// A race's surface and distance.
pub fn race_grades(race: &str) -> (Surface, Distance) {
    let i = ROWS.iter().position(|r| r.0 == race).unwrap_or(0);
    (ROWS[i].1, ROWS[i].2)
}

impl Slot {
    /// The goal her lineage plan was made for (her race's surface and distance, her style).
    pub fn parents_goal(&self, race: &str) -> Goal {
        let i = ROWS.iter().position(|r| r.0 == race).unwrap_or(0);
        let (_, surface, distance, _) = ROWS[i];
        Goal { runner: self.trainee, targets: vec![usize::from(surface == Surface::Dirt), 2 + Distance::ALL.iter().position(|d| *d == distance).unwrap_or(1), 6 + self.style as usize], label: format!("Team Trials {race}"), ..Default::default() }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Row {
    #[serde(deserialize_with = "row_name")]
    pub name: RowName,
    pub slots: Vec<Slot>,
    /// Styles on the team (each adds the positioning bonus).
    pub styles: usize,
    /// What the player can field today here: finished umas as they are, with their points, and
    /// the planned runners' gain over them, raced against each other on the same course.
    pub today: Vec<(super::baseline::Today, f64)>,
    pub gain: Option<f64>,
    pub notes: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct TtTeam {
    pub rows: Vec<Row>,
    pub total: f64,
    /// Today's team's points and the plan's gain over it, summed over the races both ran.
    pub today_total: f64,
    pub gain: f64,
    /// When the community meta was read, its days old, and whether it may be stale.
    pub meta_read: String,
    pub meta_age: (i64, bool),
    /// The career that raises the team most: the weakest slot's next step, in words.
    pub next: String,
    pub notes: Vec<String>,
}

/// Points for one skill she fires.
fn skill_points(kb: &crate::knowledge::Knowledge, id: u32, own_unique: Option<u32>, stars: u8) -> f64 {
    if Some(id) == own_unique {
        // Game8: a 3★+ unique scores 2,000 (Lv1) to 2,600 (Lv6), 1★-2★ 1,500 to 2,000; her
        // stars set the level she starts at (3★ Lv1, then +1 a star), and a career's fan
        // counts add up to three more: assume one.
        return if stars >= 3 { 2000.0 + 100.0 * f64::from((stars - 3 + 2).min(6)) } else { 1700.0 };
    }
    if id >= 900_000 && id < 1_000_000 {
        return 500.0;
    }
    match kb.skills.get(&id).map(|s| s.rarity) {
        Some(2) | Some(0) => 1200.0,
        Some(3..=5) => 500.0,
        Some(_) => 500.0,
        None => 0.0,
    }
}

/// Her skill points: each skill's points times the share of races it fired in. A unique the
/// community finds unreliable fires far less in a full field than in head-to-head races: 40%.
pub(crate) fn skills_score(kb: &crate::knowledge::Knowledge, meta: &TtMeta, outfit: u32, stars: u8, rates: &BTreeMap<u32, f64>) -> f64 {
    let unique = kb.umas.get(&outfit).and_then(|i| i.unique_skill);
    let shaky = meta.role(outfit).is_some_and(|r| r.unique.as_deref() == Some("inconsistent"));
    rates.iter().map(|(id, rate)| skill_points(kb, *id, unique, stars) * rate * if shaky && Some(*id) == unique { 0.4 } else { 1.0 }).sum()
}

/// Placement between about 6th (4,000) and 1st (10,000) in a twelve-runner field, from her win
/// share in head-to-head races: an estimate.
pub(crate) fn placement(share: Option<f64>) -> f64 {
    4000.0 + 6000.0 * share.unwrap_or(0.5)
}

/// A race's team points from each runner's placement plus skills and style: positioning for
/// each style on the team, and the Ace (the top one) +10%.
pub(crate) fn team_points(runners: &[(f64, Style)]) -> f64 {
    let mut styles: Vec<Style> = runners.iter().map(|r| r.1).collect();
    styles.sort_by_key(|s| *s as u8);
    styles.dedup();
    let top = runners.iter().map(|r| r.0).fold(f64::MIN, f64::max);
    let base: f64 = runners.iter().map(|r| r.0 + POSITIONING * styles.len() as f64 / 3.0).sum();
    if runners.is_empty() { 0.0 } else { base + 0.1 * (top + POSITIONING * styles.len() as f64 / 3.0) }
}

impl App {
    /// A Team Trials plan for one race and trainee, ready for the deck and skill pickers.
    fn tt_plan(&self, meta: &TtMeta, row: usize, trainee: u32, style: Style, course: Option<u32>) -> Plan {
        let (name, surface, distance, stamina) = ROWS[row];
        let stamina = meta.stamina(name).unwrap_or(stamina);
        let mut plan = Plan { purpose: crate::tab_planner::Purpose::TeamTrials, ..Plan::default() };
        plan.surface = surface;
        plan.distance = distance;
        plan.umalator_course = course;
        plan.style = style;
        plan.trainee = Some(trainee);
        plan.friend_lb = 4;
        plan.target = [1200, stamina, 1000, 600, 1000];
        plan.pick_objective = crate::skill_opt::PickObjective::Consistency;
        // The meta's scenario only if Global has it.
        if self.kb.scenarios.as_ref().is_some_and(|g| g.scenarios.iter().any(|x| x.global && x.matches(&meta.scenario.name))) {
            plan.scenario = meta.scenario.name.clone();
        }
        plan
    }

    /// A runner's Team Trials plan for a race, with the meta's borrow and skills: what a brief needs.
    pub(crate) fn tt_runner_plan(&mut self, meta: &TtMeta, race: &str, trainee: u32, style: Style) -> Plan {
        let row = ROWS.iter().position(|r| r.0 == race).unwrap_or(0);
        let course = self.tt_course(meta, row);
        let saved = std::mem::take(&mut self.planner.plan);
        self.planner.plan = self.tt_plan(meta, row, trainee, style, course);
        self.recommend_deck(true);
        if let Some(f) = self.tt_friend(meta, row, style, &self.planner.plan.clone()) {
            self.planner.plan.friend = Some(f);
            self.planner.plan.friend_lb = 4;
        }
        self.auto_pick_skills();
        std::mem::replace(&mut self.planner.plan, saved)
    }

    /// With a friend's `uma` (carrying her unique) borrowed as one parent, which of the player's
    /// own finished umas, as the other, still starts every target of `goal` at A. The goal's
    /// green is the unique wanted.
    pub(crate) fn inherit_partner(&self, goal: &Goal, inv: &Inventory, uma: u32) -> Option<u64> {
        let mut goal = goal.clone();
        goal.green = Some(uma);
        let mut inv2 = inv.clone();
        let borrowed = inv2.borrow(&self.kb, &super::Borrow { uma, own: vec![super::green_spark(uma, 1)], parents: [(0, vec![]), (0, vec![])] });
        let with_friend = super::jobs::propose(&self.kb, &goal, &inv2);
        with_friend.now.as_ref().filter(|l| l.meets()).and_then(|l| l.legacies.iter().map(|i| &inv2.parents[*i]).find(|x| x.id != borrowed).map(|x| x.id))
    }

    /// The meta file from the data folder, or the copy built in.
    pub(crate) fn tt_meta(&self) -> TtMeta {
        let path = self.root.as_ref().map(|r| r.join("knowledge/data/tt_meta.json"));
        let text = path.and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_else(|| include_str!("../../../knowledge/data/tt_meta.json").to_string());
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// The friend card to borrow for a runner: the meta's general pick, then the race's, then the
    /// style's, skipping her own character and characters already in the deck.
    fn tt_friend(&self, meta: &TtMeta, row: usize, style: Style, plan: &Plan) -> Option<u32> {
        // A deck holds one card per character, and never the trainee's own.
        let mut taken: Vec<&str> = plan.deck.iter().flatten().filter_map(|id| self.kb.cards.get(id).map(|c| c.uma.as_str())).collect();
        taken.extend(plan.trainee.and_then(|t| self.kb.umas.get(&t)).map(|u| u.name.as_str()));
        let today = crate::ui_util::today();
        ["general", ROWS[row].0, style_name(style)]
            .iter()
            .flat_map(|k| meta.borrows(k))
            .find(|id| self.kb.cards.get(id).is_some_and(|c| c.released(&today) && !taken.contains(&c.uma.as_str())))
    }

    /// A representative course for each race: one an event uses, else the first that fits.
    pub(crate) fn tt_course(&self, meta: &TtMeta, row: usize) -> Option<u32> {
        let (name, surface, distance, _) = ROWS[row];
        let length = meta.length(name);
        let mut fits: Vec<&crate::knowledge::UmalatorCourse> = self.kb.umalator_courses.iter().filter(|c| c.surface == surface.label() && length.map_or(Distance::of_metres(c.distance) == distance, |l| c.distance == l)).collect();
        if fits.is_empty() {
            fits = self.kb.umalator_courses.iter().filter(|c| c.surface == surface.label() && Distance::of_metres(c.distance) == distance).collect();
        }
        fits.iter().find(|c| self.kb.umalator_presets.iter().any(|e| e.course == c.id)).or(fits.first()).map(|c| c.id)
    }

    /// The pins that can hold: an owned outfit, a race that exists, a character not left out, and
    /// one pin per character (the first). The others are dropped, each with why.
    fn tt_valid_pins(&self, pins: &TtPins) -> (TtPins, Vec<String>) {
        let mut out = TtPins { pin: Vec::new(), exclude: pins.exclude.clone() };
        let mut dropped = Vec::new();
        for (t, race) in &pins.pin {
            let name = super::uma_name(&self.kb, *t);
            let c = crate::affinity::chara(*t);
            let why = if self.owned_uma(*t).is_none() {
                Some("you don't own her")
            } else if !ROWS.iter().any(|r| r.0 == race) {
                Some("no such race")
            } else if pins.exclude.contains(&c) {
                Some("her character is left out")
            } else if out.pin.iter().any(|(o, _)| crate::affinity::chara(*o) == c) {
                Some("her character is already pinned: each character runs once")
            } else {
                None
            };
            match why {
                Some(w) => dropped.push(format!("Your pin of {name} to {race} was dropped: {w}.")),
                None => out.pin.push((*t, race.clone())),
            }
        }
        (out, dropped)
    }

    /// The Team Trials team. Builds and races up to six candidates per race, so it takes a few
    /// seconds.
    pub(crate) fn tt_team(&mut self, races: usize, pins: &TtPins) -> Result<TtTeam, String> {
        let inv = self.parent_inventory();
        let (pins, dropped) = self.tt_valid_pins(pins);
        let saved = std::mem::take(&mut self.planner.plan);
        let mut out = self.tt_team_inner(&inv, races, &pins);
        if let Ok(t) = &mut out {
            t.notes.extend(dropped);
        }
        self.planner.plan = saved;
        out
    }

    fn tt_team_inner(&mut self, inv: &Inventory, races: usize, pins: &TtPins) -> Result<TtTeam, String> {
        let data = self.sim_data.clone().ok_or("The simulator's data isn't loaded.")?;
        let mut notes = Vec::new();
        let meta = self.tt_meta();
        self.stage("Finding who can run each race")?;
        self.note(format!("The community's Team Trials picks (read {}) first, then your umas with A or B in each race.", meta.read));
        // Candidates per race: the meta's usual runners for it first (in its order), then A in
        // its surface and distance, then B (parents can lift a B). Her style is the meta's
        // preferred one where her grade there is B or better, else her best.
        let mut cands: Vec<Vec<(u32, Style, bool)>> = Vec::new();
        for &(name, surface, distance, _) in &ROWS {
            let mut list: Vec<(u32, Style, bool, (u8, bool, u8, u8, u8))> = self
                .roster
                .umas
                .iter()
                .filter_map(|u| {
                    let a = &self.kb.umas.get(&u.id)?.aptitude;
                    let (gs, gd) = (grade_rank(a.surface(surface)), grade_rank(a.distance(distance)));
                    let pinned_here = pins.pin.iter().any(|(t, r)| *t == u.id && r == name);
                    let pinned_elsewhere = pins.pin.iter().any(|(t, r)| crate::affinity::chara(*t) == crate::affinity::chara(u.id) && r != name);
                    if pins.exclude.contains(&crate::affinity::chara(u.id)) || pinned_elsewhere || (!pinned_here && (gs < 6 || gd < 6)) {
                        return None;
                    }
                    let role = meta.role(u.id);
                    let fit = role.and_then(|r| r.races.iter().position(|x| x == name)).map_or(9, |i| i as u8);
                    let style = role.and_then(|r| r.styles.iter().filter_map(|s| parse_style(s)).find(|s| grade_rank(a.style(*s)) >= 6)).unwrap_or_else(|| a.best_style());
                    let steady = role.is_none_or(|r| r.unique.as_deref() != Some("inconsistent"));
                    Some((u.id, style, gs >= 7 && gd >= 7, (fit, steady, gs.min(7) + gd.min(7), u.stars, u.potential)))
                })
                .collect();
            // Pinned runners first, whatever the meta says.
            let pinned = |id: u32| pins.pin.iter().any(|(t, r)| *t == id && r == name);
            list.sort_by(|a, b| pinned(b.0).cmp(&pinned(a.0)).then(a.3.0.cmp(&b.3.0)).then(b.3.1.cmp(&a.3.1)).then(b.2.cmp(&a.2)).then((b.3.2, b.3.3, b.3.4).cmp(&(a.3.2, a.3.3, a.3.4))));
            let ids: Vec<u32> = list.iter().take(6).map(|c| c.0).collect();
            let names = super::short_names(&self.kb, &ids);
            self.note(format!("{name}: {}.", if names.is_empty() { "none of your umas can run it".to_string() } else { names.join(", ") }));
            cands.push(list.into_iter().take(6).map(|c| (c.0, c.1, c.2)).collect());
        }
        self.stage("Racing the candidates head to head")?;
        // Build and race each race's candidates head to head; score each.
        let mut scored: Vec<Vec<Slot>> = Vec::new();
        for (row, list) in cands.iter().enumerate() {
            let course_id = self.tt_course(&meta, row);
            let mut builds = Vec::new();
            let mut slots = Vec::new();
            for &(t, style, natural) in list {
                self.check_cancel()?;
                self.planner.plan = self.tt_plan(&meta, row, t, style, course_id);
                self.recommend_deck(true);
                // The meta's borrow for this race and style, when one fits the deck.
                if let Some(f) = self.tt_friend(&meta, row, style, &self.planner.plan.clone()) {
                    self.planner.plan.friend = Some(f);
                    self.planner.plan.friend_lb = 4;
                }
                self.auto_pick_skills();
                builds.push(self.plan_sim_build(true).ok());
                let mut snotes = Vec::new();
                if let Some(r) = meta.role(t) {
                    if r.races.iter().any(|x| x == ROWS[row].0) {
                        let article = if style == Style::End { "an" } else { "a" };
                        snotes.push(format!("Because: the community runs her in {} as {article} {} (umareference, read {}).", ROWS[row].0, style.label(), meta.read));
                    }
                    if let Some(n) = &r.note {
                        snotes.push(format!("Note: {n}."));
                    }
                }
                slots.push(Slot { trainee: t, style, ace: false, placement: 0.0, skills: 0.0, total: 0.0, natural, parents: Proposal::default(), plan: self.planner.plan.clone(), notes: snotes, ready: false });
            }
            let Some(course) = course_id.and_then(|c| data.courses.get(&c).cloned()) else {
                notes.push(format!("No simulator course for {}: its runners weren't raced.", ROWS[row].0));
                scored.push(slots);
                continue;
            };
            let course = std::sync::Arc::new(course);
            let race = crate::sim::run::race_params(1, 1, 1, 2, None);
            let opts = crate::sim::run::SimOptions { samples: races, seed: 12345.0, ..Default::default() };
            let raced = super::baseline::race_pool(&data, &course, &race, &builds, &opts);
            let mut by_share: Vec<(f64, u32)> = slots.iter().zip(&raced).map(|(s, r)| (r.0.unwrap_or(0.0), s.trainee)).collect();
            by_share.sort_by(|a, b| b.0.total_cmp(&a.0));
            if by_share.len() > 1 {
                let names = super::short_names(&self.kb, &by_share.iter().map(|x| x.1).collect::<Vec<_>>());
                self.note(format!("{}: {} wins {:.0}% of {races} races against the others; {} next at {:.0}%.", ROWS[row].0, names[0], by_share[0].0 * 100.0, names[1], by_share[1].0 * 100.0));
            }
            for (s, (share, rates)) in slots.iter_mut().zip(&raced) {
                s.placement = placement(*share);
                let stars = self.roster.umas.iter().find(|u| u.id == s.trainee).map_or(3, |u| u.stars);
                s.skills = skills_score(&self.kb, &meta, s.trainee, stars, rates);
                if meta.role(s.trainee).is_some_and(|r| r.unique.as_deref() == Some("inconsistent")) {
                    s.notes.push("Her unique fires unreliably in Team Trials (community), so it's counted at 40%.".into());
                }
            }
            scored.push(slots);
        }
        // Fill the races with the fewest candidates first; each character once in the team.
        self.stage("Picking three runners per race")?;
        self.note("Races with the fewest candidates pick first; each character runs once in the team, and a new running style adds the positioning bonus.");
        let mut order: Vec<usize> = (0..ROWS.len()).collect();
        order.sort_by_key(|r| scored[*r].len());
        // Pinned runners hold their characters from the start.
        let held: Vec<u32> = pins.pin.iter().map(|(t, _)| crate::affinity::chara(*t)).collect();
        let mut used: Vec<u32> = Vec::new();
        let mut rows: Vec<Option<Row>> = vec![None; ROWS.len()];
        for r in order {
            let mut team: Vec<Slot> = scored[r].iter().filter(|s| pins.pin.iter().any(|(t, x)| *t == s.trainee && x == ROWS[r].0)).take(3).cloned().collect();
            for s in &mut team {
                s.notes.insert(0, "Pinned by you.".into());
            }
            let mut pool: Vec<Slot> = scored[r].iter().filter(|s| !used.contains(&crate::affinity::chara(s.trainee)) && !held.contains(&crate::affinity::chara(s.trainee))).cloned().collect();
            // Greedy: each pick maximises her points plus the positioning bonus for a new style.
            while team.len() < 3 && !pool.is_empty() {
                let gain = |s: &Slot, team: &[Slot]| s.placement + s.skills + if team.iter().any(|t| t.style == s.style) { 0.0 } else { POSITIONING };
                let (i, _) = pool.iter().enumerate().max_by(|a, b| gain(a.1, &team).total_cmp(&gain(b.1, &team))).ok_or("no candidate")?;
                let s = pool.remove(i);
                pool.retain(|p| crate::affinity::chara(p.trainee) != crate::affinity::chara(s.trainee));
                team.push(s);
            }
            for s in &team {
                used.push(crate::affinity::chara(s.trainee));
            }
            team.sort_by(|a, b| (b.placement + b.skills).total_cmp(&(a.placement + a.skills)));
            let styles = {
                let mut v: Vec<Style> = team.iter().map(|s| s.style).collect();
                v.sort_by_key(|s| *s as u8);
                v.dedup();
                v.len()
            };
            let mut rnotes = Vec::new();
            if team.len() < 3 {
                rnotes.push(format!("Only {} of your umas can run {} (A or B in it): an empty slot scores nothing.", team.len(), ROWS[r].0));
            }
            for (k, s) in team.iter_mut().enumerate() {
                s.ace = k == 0;
                let base = s.placement + s.skills + POSITIONING * styles as f64 / 3.0;
                s.total = if s.ace { base * 1.1 } else { base };
            }
            if let Some(ace) = team.first() {
                let names = super::short_names(&self.kb, &team.iter().map(|s| s.trainee).collect::<Vec<_>>());
                let rest = names[1..].join(" and ");
                self.note(format!("{}: {} is the Ace (about {} points){}.", ROWS[r].0, names[0], crate::lineage_kit::thousands(ace.total), if rest.is_empty() { String::new() } else { format!(", with {rest}") }));
            }
            rows[r] = Some(Row { name: ROWS[r].0, slots: team, styles, today: Vec::new(), gain: None, notes: rnotes });
        }
        let mut rows: Vec<Row> = rows.into_iter().flatten().collect();
        // Each runner's lineage: her race's surface and distance and her style at A, inheriting
        // the meta's unique (564 Escapades) when a route reaches it, else without.
        let inherit = (meta.inherit.uma > 0 && self.kb.umas.contains_key(&meta.inherit.uma)).then_some(meta.inherit.uma);
        self.stage("Planning parents")?;
        for row in &mut rows {
            let i = ROWS.iter().position(|r| r.0 == row.name).unwrap_or(0);
            let (_, surface, distance, _) = ROWS[i];
            for s in &mut row.slots {
                let mut goal = Goal { runner: s.trainee, targets: vec![usize::from(surface == Surface::Dirt), 2 + Distance::ALL.iter().position(|d| *d == distance).unwrap_or(1), 6 + s.style as usize], label: format!("Team Trials {}", row.name), ..Default::default() };
                let base = super::jobs::propose(&self.kb, &goal, inv);
                s.ready = base.now.as_ref().is_some_and(super::score::Lineage::meets);
                let mut p = None;
                if let Some(g) = inherit.filter(|g| crate::affinity::chara(*g) != crate::affinity::chara(s.trainee)) {
                    goal.green = Some(g);
                    let with = super::jobs::propose(&self.kb, &goal, inv);
                    let reach = with.now.as_ref().is_some_and(super::score::Lineage::meets) || (!with.steps.is_empty() && !with.over_cap);
                    if reach {
                        s.notes.push(format!("Her lineage carries {} (the community's inherit for every Team Trials runner).", meta.inherit.skill));
                        p = Some(with);
                    } else {
                        // The community's way: borrow a friend's finished Summer Gold Ship as one
                        // parent; her green comes with her. Does one of the player's own finished
                        // umas still give the rest of the start?
                        let partner = self.inherit_partner(&goal, inv, g).and_then(|id| inv.by_id(id)).map(|x| super::parent_name(&self.kb, x));
                        goal.green = None;
                        match partner {
                            Some(w) => s.notes.push(format!("For {}: borrow a friend's Summer Gold Ship as one parent and pair her with {w} instead of the parents below; every target still starts at A.", meta.inherit.skill)),
                            None => s.notes.push(format!("{} needs Summer Gold Ship as a parent: borrow a friend's, and plan the other parent so her targets still start at A.", meta.inherit.skill)),
                        }
                    }
                }
                s.parents = p.unwrap_or(base);
                let what = if s.parents.need.iter().all(|n| n.1 == 0) {
                    "she starts at A on her own".to_string()
                } else if s.ready {
                    "her finished umas already start her at A".to_string()
                } else if s.parents.steps.is_empty() {
                    "no parent route under the cap".to_string()
                } else {
                    format!("{} parent career{} first, about {:.0} careers", s.parents.steps.len(), if s.parents.steps.len() == 1 { "" } else { "s" }, s.parents.careers())
                };
                if !what.starts_with("she starts at A") {
                    let line = format!("{} ({}): {what}.", super::short_name(&self.kb, s.trainee), row.name);
                    self.note(line);
                }
            }
            self.check_cancel()?;
        }
        let natural = rows.iter().flat_map(|r| &r.slots).filter(|s| s.parents.need.iter().all(|n| n.1 == 0)).count();
        if natural > 0 {
            self.note(format!("{natural} of the runners start at A in their race on their own: no parents needed."));
        }
        // What the player can field today: per race, up to three finished umas as they are (each
        // character once in the team), raced against the planned runners on the same course and
        // scored the same way.
        self.stage("Comparing with today's team")?;
        let vets = self.veterans();
        if vets.is_empty() {
            self.note("No finished umas imported yet, so there's no team today to compare with.");
        }
        let mut taken: Vec<u32> = Vec::new();
        let (mut today_total, mut gain) = (0.0, 0.0);
        let race = crate::sim::run::race_params(1, 1, 1, 2, None);
        let opts = crate::sim::run::SimOptions { samples: races, seed: 12345.0, ..Default::default() };
        for row in &mut rows {
            let i = ROWS.iter().position(|r| r.0 == row.name).unwrap_or(0);
            let (_, surface, distance, _) = ROWS[i];
            let picks = super::baseline::candidates(&vets, surface, distance, &taken, 3);
            taken.extend(picks.iter().map(|v| crate::affinity::chara(v.card_id)));
            let Some(course) = self.tt_course(&meta, i).and_then(|c| data.courses.get(&c).cloned()) else { continue };
            if picks.is_empty() {
                continue;
            }
            let course = std::sync::Arc::new(course);
            let mut builds: Vec<Option<crate::sim::run::UmaBuild>> = Vec::new();
            for s in &row.slots {
                self.planner.plan = s.plan.clone();
                builds.push(self.plan_sim_build(true).ok());
            }
            let vb: Vec<(Option<crate::sim::run::UmaBuild>, Style)> = picks.iter().map(|v| super::baseline::build(&data, v, surface, distance).map_or((None, super::baseline::best_style(v)), |(b, st)| (Some(b), st))).collect();
            builds.extend(vb.iter().map(|x| x.0.clone()));
            let raced = super::baseline::race_pool(&data, &course, &race, &builds, &opts);
            let n = row.slots.len();
            let planned: Vec<(f64, Style)> = row.slots.iter().zip(&raced).map(|(s, (share, rates))| {
                let stars = self.roster.umas.iter().find(|u| u.id == s.trainee).map_or(3, |u| u.stars);
                (placement(*share) + skills_score(&self.kb, &meta, s.trainee, stars, rates), s.style)
            }).collect();
            row.today = picks.iter().zip(&vb).zip(&raced[n..]).map(|((v, (_, st)), (share, rates))| {
                let pts = placement(*share) + skills_score(&self.kb, &meta, v.card_id, v.rarity, rates);
                (super::baseline::Today { vet: v.trained_chara_id, outfit: v.card_id, style: *st, share: *share }, pts)
            }).collect();
            row.today.sort_by(|a, b| b.1.total_cmp(&a.1));
            let today_pts = team_points(&row.today.iter().map(|t| (t.1, t.0.style)).collect::<Vec<_>>());
            let g = team_points(&planned) - today_pts;
            row.gain = Some(g);
            self.note(format!("{}: today about {} points; the plan gains {}{}.", row.name, crate::lineage_kit::thousands(today_pts), if g < 0.0 { "−" } else { "+" }, crate::lineage_kit::thousands(g.abs())));
            self.check_cancel()?;
            today_total += today_pts;
            gain += g;
        }
        let total: f64 = rows.iter().flat_map(|r| r.slots.iter().map(|s| s.total)).sum();
        // The career that raises the team most: fill an empty slot, else lift the weakest one.
        let next = if let Some(row) = rows.iter().find(|r| r.slots.len() < 3) {
            format!("Fill {}: no more of your umas can run it. Look at a uma with B in it whose parents can lift her to A.", row.name)
        } else if let Some((row, s)) = rows.iter().flat_map(|r| r.slots.iter().map(move |s| (r, s))).min_by(|a, b| a.1.total.total_cmp(&b.1.total)) {
            let name = super::uma_name(&self.kb, s.trainee);
            let shaky = meta.role(s.trainee).is_some_and(|r| r.unique.as_deref() == Some("inconsistent"));
            let on_team: Vec<u32> = rows.iter().flat_map(|r| r.slots.iter().map(|x| crate::affinity::chara(x.trainee))).collect();
            let others: Vec<String> = meta.roles.list.iter().filter(|r| r.races.iter().any(|x| x == row.name) && r.unique.is_none() && !on_team.contains(&crate::affinity::chara(r.id))).map(|r| {
                let owned = self.roster.umas.iter().any(|u| u.id == r.id);
                format!("{}{}", super::uma_name(&self.kb, r.id), if owned { " (yours)" } else { "" })
            }).take(4).collect();
            // Never suggest replacing a runner the player pinned.
            let pinned = pins.pin.iter().any(|(t, r)| *t == s.trainee && r == row.name);
            if shaky && !pinned && !others.is_empty() {
                format!("{name} ({}) scores lowest and her unique fires unreliably: a steadier {} runner would score more, such as {}.", row.name, row.name, others.join(", "))
            } else {
            match s.parents.steps.first() {
                Some(step) if !s.ready => format!("{name} ({}): her lineage needs {} parent step(s) first; start with a career of {}.", row.name, s.parents.steps.len(), super::uma_name(&self.kb, step.trainee)),
                _ => format!("{name} ({}) scores lowest: train her career with the deck and skills shown; it raises the team most.", row.name),
            }
            }
        } else {
            "Nothing to train.".into()
        };
        notes.push("Scores are estimates: placement from head-to-head races in the simulator (the real races have twelve runners), skills from how often each fired.".into());
        let meta_age = meta.age(&crate::ui_util::today());
        if meta_age.1 {
            notes.push(format!("The community meta was read {} days ago ({}): it may be stale. knowledge/UPDATING.md says how to refresh it.", meta_age.0, meta.read));
        }
        Ok(TtTeam { rows, total, today_total, gain, meta_read: meta.read.clone(), meta_age, next, notes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_hold_one_per_character() {
        let a = App::for_tests();
        let t = a.roster.umas[0].id;
        let other = a.roster.umas.iter().map(|u| u.id).find(|id| crate::affinity::chara(*id) != crate::affinity::chara(t)).unwrap();
        let pins = TtPins { pin: vec![(t, "Mile".into()), (t, "Sprint".into()), (other, "Dirt".into()), (1, "Long".into()), (t, "Moon".into())], exclude: vec![crate::affinity::chara(other)] };
        let (kept, dropped) = a.tt_valid_pins(&pins);
        assert_eq!(kept.pin, vec![(t, "Mile".to_string())]);
        assert_eq!(dropped.len(), 4, "{dropped:?}");
        assert!(dropped.iter().any(|d| d.contains("already pinned")) && dropped.iter().any(|d| d.contains("left out")) && dropped.iter().any(|d| d.contains("don't own")));
    }

    #[test]
    fn meta_turns_stale_after_45_days() {
        let m = TtMeta { read: "2026-09-01".into(), ..Default::default() };
        assert_eq!(m.age("2026-10-16"), (45, false));
        assert_eq!(m.age("2026-10-28"), (57, true));
        let a = App::for_tests();
        let shipped = a.tt_meta();
        assert!(!shipped.read.is_empty() && !shipped.sources.is_empty(), "the meta file says when and where it was read");
    }

    /// The Team Trials team on the real account:
    /// `UMALORD_ROOT=<data folder with knowledge/> cargo test --release tt_team_real_account -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn tt_team_real_account() {
        let root = std::path::PathBuf::from(std::env::var("UMALORD_ROOT").expect("UMALORD_ROOT"));
        let mut a = App::new(Some(root));
        let t = std::time::Instant::now();
        let team = a.tt_team(200, &TtPins::default()).unwrap();
        eprintln!("({:.1} s)\n{}", t.elapsed().as_secs_f64(), super::super::text::tt(&a.kb, &team));
        let inv = a.parent_inventory();
        let next = a.tt_next(&team, &inv).expect("a next career");
        eprintln!("{}", super::super::brief::text(&a.kb, &next));
        assert!(next.friend.is_some(), "the next career borrows a friend card");
        // Leave the first race's Ace out and pin the last race's weakest runner into the first race.
        if let (Some(first), Some(last)) = (team.rows.first(), team.rows.last()) {
            if let (Some(ace), Some(weak)) = (first.slots.first(), last.slots.last()) {
                let pins = TtPins { pin: vec![(weak.trainee, first.name.to_string())], exclude: vec![crate::affinity::chara(ace.trainee)] };
                let again = a.tt_team(200, &pins).unwrap();
                let row = again.rows.iter().find(|r| r.name == first.name).unwrap();
                assert!(row.slots.iter().any(|s| s.trainee == weak.trainee), "the pinned runner is in her race");
                assert!(again.rows.iter().flat_map(|r| &r.slots).all(|s| crate::affinity::chara(s.trainee) != crate::affinity::chara(ace.trainee)), "the excluded character is out");
                assert_eq!(again.rows.iter().flat_map(|r| &r.slots).filter(|s| crate::affinity::chara(s.trainee) == crate::affinity::chara(weak.trainee)).count(), 1, "pinned once");
            }
        }
        let mut charas = std::collections::BTreeSet::new();
        for r in &team.rows {
            assert!(r.slots.len() <= 3);
            assert_eq!(r.slots.iter().filter(|s| s.ace).count(), usize::from(!r.slots.is_empty()), "one Ace per race");
            for s in &r.slots {
                assert!(charas.insert(crate::affinity::chara(s.trainee)), "each character once");
                assert!(s.plan.friend.is_some() && s.plan.friend_lb == 4);
                let misfits = a.runner_skill_misfits(&s.plan, &[]);
                assert!(misfits.is_empty(), "skills that can't help her: {misfits:?}");
            }
        }
    }
}
