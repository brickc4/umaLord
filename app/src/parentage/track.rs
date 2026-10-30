//! The saved parent plan (plans/parents.json, one goal at a time) and its progress.
//!
//! The player runs the current step until a finished uma has its must-haves. Each import is
//! checked against that step: the umas of its trainee finished since the step began, with what
//! they have. Keeping one ends the step: with its must-haves met, or settled for without them.
//! Either way the plan is worked out again from everything the account now has; if the goal still
//! fits under the cap the new steps replace the old ones, otherwise the plan waits for the player
//! to pick another route.

use super::jobs::{Pin, Proposal, Run};
use super::{Goal, Inventory, Parent};
use crate::knowledge::Knowledge;
use crate::veterans::Veteran;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "plans/parents.json";
/// Cancelled plans, kept to look back on.
pub const PAST: &str = "plans/past-parent-plans";
/// Ids for parents the player checked off by hand, until an import brings the real one.
const DECLARED: u64 = u64::MAX - 1000;
/// Undo levels kept.
const UNDO: usize = 20;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ParentPlan {
    pub goal: Goal,
    /// The day it was made (local).
    pub made: String,
    /// Steps finished, then the ones to run.
    pub steps: Vec<Step>,
    /// The finished uma the last step's product pairs with, for a one-step route.
    pub partner: Option<u64>,
    /// Why the plan is waiting for the player's choice, when the goal no longer fits the cap.
    pub blocked: Option<String>,
    /// The player accepted a route with a step past the cap.
    pub over_cap: bool,
    /// What happened, oldest first.
    pub log: Vec<String>,
    /// Paused: imports aren't checked against it.
    pub paused: bool,
    /// The player's pinned choice for the next step.
    pub pin: Pin,
    /// Friends' legacies the player can borrow, which the plan may use.
    pub borrows: Vec<super::Borrow>,
    /// How to raise it (`ceiling.rs`), measured on a worker after the plan is made or changed.
    pub ceiling: Option<Vec<super::ceiling::Lever>>,
    pub tried: super::ceiling::Tried,
    /// The plan before its last change, for Undo (each holds its own, up to `UNDO` deep).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Box<ParentPlan>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Step {
    pub run: Run,
    /// When the newest finished uma was made as the step began (the game's create_time): only
    /// umas finished after it count for the step.
    pub after: String,
    /// The finished uma kept for it.
    pub done: Option<u64>,
    /// Kept without all its must-haves.
    pub settled: bool,
    /// Checked off by hand before the import: the sparks the player says she got. The step
    /// counts as done, with her as a parent the plan can use, until an import confirms her.
    pub declared: Option<Vec<u32>>,
}

impl Step {
    /// Done, or checked off by hand.
    pub fn finished(&self) -> bool {
        self.done.is_some() || self.declared.is_some()
    }
}

/// How the player marks a step done.
pub enum Mark {
    /// A finished uma the import brought.
    Finished(u64),
    /// What she got, before the import: spark ids.
    Declared(Vec<u32>),
}

/// A finished uma that counts for the current step.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub id: u64,
    pub must: bool,
    /// The nice-to-haves it has.
    pub nice: Vec<usize>,
    /// Whether it ran with the step's legacies.
    pub same_parents: bool,
    /// Must-have stars it's missing, all must-haves together.
    pub short: u32,
}

impl ParentPlan {
    /// A plan from a proposal, its steps starting after the newest finished uma.
    pub fn new(goal: Goal, p: &Proposal, vets: &[Veteran], today: &str) -> Self {
        let after = newest(vets);
        let steps = p.steps.iter().map(|r| Step { run: r.clone(), after: after.clone(), ..Default::default() }).collect();
        ParentPlan { goal, made: today.into(), steps, partner: p.partner, over_cap: p.over_cap, log: vec![format!("{today}: planned.")], ..Default::default() }
    }

    pub fn current(&self) -> Option<usize> {
        self.steps.iter().position(|s| !s.finished())
    }

    /// Keep the plan as it is now for Undo.
    fn snapshot(&mut self) {
        fn trim(p: &mut ParentPlan, depth: usize) {
            match p.before.as_mut() {
                Some(_) if depth == 0 => p.before = None,
                Some(b) => trim(b, depth - 1),
                None => {}
            }
        }
        let mut prev = self.clone();
        trim(&mut prev, UNDO - 1);
        self.before = Some(Box::new(prev));
    }

    /// Put the plan back as it was before its last change.
    pub fn undo(&mut self, today: &str) -> Result<String, String> {
        let prev = self.before.take().ok_or("Nothing to undo.")?;
        let log = self.log.clone();
        *self = *prev;
        self.log = log;
        self.log.push(format!("{today}: undid the last change."));
        Ok("Undone: the plan is as it was before the last change.".into())
    }

    /// The account's parents plus the ones checked off by hand, which the plan can already use.
    pub fn inventory(&self, kb: &Knowledge, inv: &Inventory) -> Inventory {
        let mut out = inv.clone();
        for b in &self.borrows {
            out.borrow(kb, b);
        }
        for (i, s) in self.steps.iter().enumerate() {
            let (Some(own), None) = (&s.declared, s.done) else { continue };
            let legs: Vec<Parent> = s.run.legacies.iter().filter_map(|id| out.by_id(*id).cloned()).collect();
            if let [a, b] = &legs[..] {
                let mut p = Parent::planned(kb, s.run.trainee, own.clone(), a, b);
                p.id = DECLARED + i as u64;
                out.parents.push(p);
            }
        }
        out
    }

    /// Mark the current step done: with a finished uma the import brought, or with what she got
    /// before the import. Then the plan is worked out again.
    pub fn check_off(&mut self, kb: &Knowledge, inv: &Inventory, vets: &[Veteran], mark: Mark, today: &str) -> Result<String, String> {
        match mark {
            Mark::Finished(id) => self.settle(kb, inv, vets, id, today),
            Mark::Declared(own) => {
                if let Some(why) = &self.blocked {
                    return Err(format!("The plan is waiting for your choice ({why}): plan another route first."));
                }
                let i = self.current().ok_or("Every step is done: the runner's lineage is ready.")?;
                self.snapshot();
                let step = &mut self.steps[i];
                let met = step.run.must.iter().all(|w| w.met(&own));
                let mut msg = if met { format!("Step {} checked off; the next import confirms her.", i + 1) } else { format!("Step {} checked off short of {}; the next import confirms her.", i + 1, missing(kb, &step.run, &own)) };
                step.settled = !met;
                step.declared = Some(own);
                self.log.push(format!("{today}: {msg}"));
                msg += " ";
                msg += &self.revalidate(kb, inv, vets, today);
                Ok(msg)
            }
        }
    }

    /// After an import: link each step checked off by hand to the finished uma that matches what
    /// was declared, or say none does. Returns a note per step.
    pub fn confirm(&mut self, kb: &Knowledge, vets: &[Veteran], today: &str) -> Vec<String> {
        let mut notes = Vec::new();
        for i in 0..self.steps.len() {
            let s = &self.steps[i];
            let (Some(own), None) = (s.declared.clone(), s.done) else { continue };
            let since: Vec<&Veteran> = vets.iter().filter(|v| v.card_id == s.run.trainee && created(v) > s.after).collect();
            if let Some(v) = since.iter().find(|v| own.iter().all(|f| v.factors().contains(f))) {
                let id = v.trained_chara_id;
                let virt = DECLARED + i as u64;
                self.steps[i].done = Some(id);
                self.steps[i].declared = None;
                // A later step or the partner may already use her by her stand-in id.
                for later in &mut self.steps {
                    for l in &mut later.run.legacies {
                        if *l == virt {
                            *l = id;
                        }
                    }
                }
                if self.partner == Some(virt) {
                    self.partner = Some(id);
                }
                for a in &mut self.pin.avoid {
                    for l in &mut a.legacies {
                        if *l == virt {
                            *l = id;
                        }
                    }
                }
                if self.pin.legacy == Some(virt) {
                    self.pin.legacy = Some(id);
                }
                notes.push(format!("Step {}: the import confirms #{id}.", i + 1));
            } else if !since.is_empty() {
                notes.push(format!(
                    "Step {}: no imported {} has what you checked off ({}). Undo the check-off, or keep one of the imported ones.",
                    i + 1,
                    super::uma_name(kb, s.run.trainee),
                    own.iter().filter_map(|f| kb.sparks.get(f)).map(|x| format!("{} {}★", x.name, x.stars)).collect::<Vec<_>>().join(", ")
                ));
            }
        }
        for n in &notes {
            self.log.push(format!("{today}: {n}"));
        }
        notes
    }

    /// Pause (imports aren't checked) or resume.
    pub fn set_paused(&mut self, paused: bool, today: &str) {
        self.paused = paused;
        self.log.push(format!("{today}: {}.", if paused { "paused" } else { "resumed" }));
    }

    /// What a new goal would do to the plan, without changing it: the proposal and a summary.
    pub fn preview_goal(&self, kb: &Knowledge, inv: &Inventory, goal: &Goal) -> (Proposal, String) {
        let p = super::jobs::propose(kb, goal, &self.inventory(kb, inv));
        let summary = changes(kb, &self.remaining(), &p.steps);
        (p, summary)
    }

    /// Change the goal and work the plan out again (the pin goes: it was for the old goal).
    pub fn set_goal(&mut self, kb: &Knowledge, inv: &Inventory, vets: &[Veteran], goal: Goal, today: &str) -> String {
        self.snapshot();
        self.goal = goal;
        self.pin = Pin::default();
        self.log.push(format!("{today}: changed the goal."));
        self.revalidate(kb, inv, vets, today)
    }

    /// Keep a choice for the next step (a trainee, one of her parents, or the runner's partner)
    /// and plan the rest around it.
    pub fn set_pin(&mut self, kb: &Knowledge, inv: &Inventory, vets: &[Veteran], pin: Pin, today: &str) -> String {
        self.snapshot();
        let avoid = std::mem::take(&mut self.pin.avoid);
        self.pin = if pin.avoid.is_empty() { Pin { avoid, ..pin } } else { pin };
        self.log.push(format!("{today}: {}.", if self.pin.is_empty() { "cleared the pinned choice" } else { "pinned a choice for the next step" }));
        self.revalidate(kb, inv, vets, today)
    }

    /// Give up on the current step (no career comes close) and plan the rest around it: the
    /// finished steps stay, and the routes left steer around this trainee with these legacies.
    /// When no other route reaches the goal the plan stays as it was and the answer says what's
    /// left: keep going, settle for the closest uma, or borrow a friend's legacy.
    pub fn another_route(&mut self, kb: &Knowledge, inv: &Inventory, vets: &[Veteran], today: &str) -> Result<String, String> {
        if let Some(why) = &self.blocked {
            return Err(format!("The plan is waiting for your choice ({why}): pick another runner or goal, add a friend's legacy, or accept the cheapest route past the cap."));
        }
        let i = self.current().ok_or("Every step is done.")?;
        let run = self.steps[i].run.clone();
        let mut trial = self.clone();
        trial.snapshot();
        trial.pin.avoid.push(super::jobs::Avoid::of(&run));
        // Pins on the step given up on would bring it straight back.
        if trial.pin.trainee == Some(run.trainee) {
            trial.pin.trainee = None;
        }
        if trial.pin.legacy.is_some_and(|l| run.legacies.contains(&l)) {
            trial.pin.legacy = None;
        }
        let msg = trial.revalidate(kb, inv, vets, today);
        if trial.blocked.is_some() {
            return Err(format!(
                "No other route reaches the goal without step {} ({}'s career): keep going, settle for her closest uma, or add a friend's legacy to borrow.",
                i + 1,
                super::uma_name(kb, run.trainee)
            ));
        }
        trial.log.push(format!("{today}: gave up on step {} ({}'s career) for another route.", i + 1, super::uma_name(kb, run.trainee)));
        *self = trial;
        Ok(format!("Step {} set aside. {msg}", i + 1))
    }

    /// Add a friend's legacy the plan may use, and plan again with it.
    pub fn add_borrow(&mut self, kb: &Knowledge, inv: &Inventory, vets: &[Veteran], b: super::Borrow, today: &str) -> String {
        self.snapshot();
        self.log.push(format!("{today}: added a friend's {} to borrow.", super::uma_name(kb, b.uma)));
        self.borrows.push(b);
        self.revalidate(kb, inv, vets, today)
    }

    /// The steps still to run.
    pub fn remaining(&self) -> Vec<Run> {
        self.steps.iter().filter(|s| !s.finished()).map(|s| s.run.clone()).collect()
    }

    /// How the current step's careers so far were run: (Independent Training, manual). A parent
    /// career unmarked counts as Independent Training, the mode the brief asks for.
    pub fn step_modes(&self, vets: &[Veteran], log: &super::modes::ModeLog) -> (u32, u32) {
        let hits = self.hits(vets);
        let manual = hits.iter().filter(|h| log.of(h.id, super::modes::Mode::Independent) == super::modes::Mode::Manual).count() as u32;
        (hits.len() as u32 - manual, manual)
    }

    /// The current step's finished umas, closest to its must-haves first: the ones to settle with.
    pub fn closest(&self, vets: &[Veteran]) -> Vec<Hit> {
        let mut out = self.hits(vets);
        out.sort_by_key(|h| (!h.must, h.short, !h.same_parents, std::cmp::Reverse(h.nice.len())));
        out
    }

    /// What settling with `id` would do, without doing it: the steps left after, and the message.
    pub fn preview_settle(&self, kb: &Knowledge, inv: &Inventory, vets: &[Veteran], id: u64, today: &str) -> Result<(Vec<Run>, String), String> {
        let mut trial = self.clone();
        let msg = trial.settle(kb, inv, vets, id, today)?;
        Ok((trial.remaining(), msg))
    }

    /// What another route would be, without taking it: the steps left after, and the message.
    pub fn preview_another_route(&self, kb: &Knowledge, inv: &Inventory, vets: &[Veteran], today: &str) -> Result<(Vec<Run>, String), String> {
        let mut trial = self.clone();
        let msg = trial.another_route(kb, inv, vets, today)?;
        Ok((trial.remaining(), msg))
    }

    /// The current step's finished umas, newest first.
    pub fn hits(&self, vets: &[Veteran]) -> Vec<Hit> {
        let Some(step) = self.current().map(|i| &self.steps[i]) else { return Vec::new() };
        let mut out: Vec<(String, Hit)> = vets
            .iter()
            .filter(|v| v.card_id == step.run.trainee && created(v) > step.after)
            .map(|v| {
                let own = v.factors();
                let same = same_parents(step, v, vets);
                // The route counts the step's legacies' own pinks too, so other legacies don't meet it.
                let hit = Hit {
                    id: v.trained_chara_id,
                    must: same && step.run.must.iter().all(|w| w.met(&own)),
                    nice: step.run.nice.iter().enumerate().filter(|(_, w)| w.met(&own)).map(|(i, _)| i).collect(),
                    same_parents: same,
                    short: step.run.must.iter().map(|w| u32::from(w.short(&own))).sum(),
                };
                (created(v), hit)
            })
            .collect();
        out.sort_by(|a, b| b.0.cmp(&a.0));
        out.into_iter().map(|h| h.1).collect()
    }

    /// Whether finished uma `id` can be kept for the current step: the step, whether it ran
    /// with the step's legacies, and whether it meets the must-haves.
    pub fn judge(&self, kb: &Knowledge, vets: &[Veteran], id: u64) -> Result<(usize, bool, bool), String> {
        if let Some(why) = &self.blocked {
            return Err(format!("The plan is waiting for your choice ({why}): plan another route first."));
        }
        let i = self.current().ok_or("Every step is done: the runner's lineage is ready.")?;
        let v = vets.iter().find(|v| v.trained_chara_id == id).ok_or(format!("No finished uma #{id}; import it first."))?;
        let step = &self.steps[i];
        if v.card_id != step.run.trainee {
            return Err(format!("#{id} is {}, but step {} is a career of {}.", super::uma_name(kb, v.card_id), i + 1, super::uma_name(kb, step.run.trainee)));
        }
        if created(v) <= step.after {
            return Err(format!("#{id} finished before step {} began; keep one finished since.", i + 1));
        }
        let same = same_parents(step, v, vets);
        Ok((i, same, same && step.run.must.iter().all(|w| w.met(&v.factors()))))
    }

    /// Keep finished uma `id` for the current step, then work the plan out again from the
    /// account as it is now. Returns what changed.
    pub fn settle(&mut self, kb: &Knowledge, inv: &Inventory, vets: &[Veteran], id: u64, today: &str) -> Result<String, String> {
        let (i, same, met) = self.judge(kb, vets, id)?;
        self.snapshot();
        let own = vets.iter().find(|v| v.trained_chara_id == id).map(|v| v.factors()).unwrap_or_default();
        let step = &mut self.steps[i];
        step.done = Some(id);
        step.settled = !met;
        let mut msg = if met {
            format!("Step {} done with #{id}.", i + 1)
        } else if !same {
            format!("Step {} settled with #{id}, which ran with other legacies than the step's.", i + 1)
        } else {
            format!("Step {} settled with #{id}, short of {}.", i + 1, missing(kb, &step.run, &own))
        };
        self.log.push(format!("{today}: {msg}"));
        msg += " ";
        msg += &self.revalidate(kb, inv, vets, today);
        Ok(msg)
    }

    /// Work the remaining steps out again from what the account has now.
    pub fn revalidate(&mut self, kb: &Knowledge, inv: &Inventory, vets: &[Veteran], today: &str) -> String {
        let inv2 = self.inventory(kb, inv);
        let mut p = super::jobs::propose_pinned(kb, &self.goal, &inv2, &self.pin);
        // Keep the step already planned when it's still as cheap: a re-plan shouldn't swap the
        // player's next career for a costlier one with a slightly better final lineage.
        let next = self.steps.iter().find(|s| !s.finished()).map(|s| s.run.clone());
        if let Some(next) = next.filter(|n| self.pin.trainee.is_none() && !self.pin.avoid.iter().any(|a| a.matches(n.trainee, n.legacies)))
            && !p.now.as_ref().is_some_and(super::score::Lineage::meets)
            && p.steps.first().is_some_and(|s| s.trainee != next.trainee)
        {
            let keep = super::jobs::propose_pinned(kb, &self.goal, &inv2, &Pin { trainee: Some(next.trainee), ..self.pin.clone() });
            let cost = |q: &Proposal| q.steps.iter().map(Run::careers).sum::<f64>();
            if !keep.steps.is_empty() && !keep.over_cap && keep.impossible.is_empty() && cost(&keep) <= cost(&p) {
                p = keep;
            }
        }
        let old = self.remaining();
        let done: Vec<Step> = self.steps.iter().filter(|s| s.finished()).cloned().collect();
        let after = newest(vets);
        let msg = if p.now.as_ref().is_some_and(super::score::Lineage::meets) {
            self.blocked = None;
            self.steps = done;
            self.partner = None;
            "The account's finished umas now meet the goal: the runner can train.".to_string()
        } else if !p.impossible.is_empty() || p.steps.is_empty() || (p.over_cap && !self.over_cap) {
            let why = if p.steps.is_empty() { "no route reaches the goal from what the account has" } else { "every route left has a step past the cap" };
            self.blocked = Some(why.to_string());
            self.steps = done;
            self.partner = None;
            format!("The goal no longer fits: {why}. Pick another route: another runner or goal, a friend's legacy, or the cheapest route past the cap if you accept it.")
        } else {
            self.blocked = None;
            self.partner = p.partner;
            // A step that stays the same keeps when it began, so careers already run still count.
            let kept: Vec<Step> = p
                .steps
                .iter()
                .map(|r| match self.steps.iter().find(|s| !s.finished() && s.run.trainee == r.trainee && s.run.legacies == r.legacies && s.run.must == r.must) {
                    Some(s) => Step { run: r.clone(), after: s.after.clone(), ..Default::default() },
                    None => Step { run: r.clone(), after: after.clone(), ..Default::default() },
                })
                .collect();
            self.steps = done.into_iter().chain(kept).collect();
            changes(kb, &old, &p.steps)
        };
        self.log.push(format!("{today}: {msg}"));
        msg
    }
}

/// Whether a finished uma ran with the step's two legacies. The game keeps a parent's outfit,
/// not its id, so outfits are compared.
fn same_parents(step: &Step, v: &Veteran, vets: &[Veteran]) -> bool {
    // A starter step takes any two legacies.
    if step.run.legacies == [0, 0] {
        return true;
    }
    v.ancestor(10).zip(v.ancestor(20)).is_some_and(|(a, b)| {
        // The outfits the step recorded, else (an older plan) the legacies still imported.
        let mut want: Vec<u32> = if step.run.legacy_umas.iter().all(|u| *u > 0) {
            step.run.legacy_umas.to_vec()
        } else {
            step.run.legacies.iter().filter_map(|id| vets.iter().find(|x| x.trained_chara_id == *id)).map(|x| x.card_id).collect()
        };
        let mut got = vec![a.card_id, b.card_id];
        got.sort_unstable();
        want.sort_unstable();
        got == want
    })
}

/// What changed between the steps to run before and after a re-plan, in plain words.
pub fn changes(kb: &Knowledge, old: &[Run], new: &[Run]) -> String {
    let same = |a: &Run, b: &Run| a.trainee == b.trainee && a.legacies == b.legacies && a.must == b.must;
    if old.len() == new.len() && old.iter().zip(new).all(|(a, b)| same(a, b)) {
        return "The rest of the plan stands.".into();
    }
    let musts = |r: &Run| if r.must.is_empty() { "just finishing her".to_string() } else { r.must.iter().map(|w| w.label(kb)).collect::<Vec<_>>().join(", ") };
    let name = |u: u32| super::uma_name(kb, u);
    let mut out = Vec::new();
    for i in 0..old.len().max(new.len()) {
        match (old.get(i), new.get(i)) {
            (Some(a), Some(b)) if same(a, b) => {}
            (Some(a), Some(b)) if a.trainee != b.trainee => out.push(format!("the next step {} is now a career of {} ({}) instead of {}", i + 1, name(b.trainee), musts(b), name(a.trainee))),
            (Some(a), Some(b)) if a.must != b.must => out.push(format!("step {} of {} now needs {} instead of {}", i + 1, name(b.trainee), musts(b), musts(a))),
            (Some(_), Some(b)) => out.push(format!("step {} of {} now runs with other parents", i + 1, name(b.trainee))),
            (None, Some(b)) => out.push(format!("a step is added: a career of {} ({})", name(b.trainee), musts(b))),
            (Some(a), None) => out.push(format!("the step of {} isn't needed any more", name(a.trainee))),
            (None, None) => {}
        }
    }
    let mut s = out.join("; ");
    if let Some(c) = s.get(..1) {
        s = c.to_uppercase() + &s[1..];
    }
    format!("{s}.")
}

/// Cancel the saved plan: it moves to the past plans, with how far it got.
pub fn cancel(root: &Path, today: &str) -> Result<String, String> {
    let mut plan = load(root).ok_or("No parent plan to cancel.")?;
    plan.log.push(format!("{today}: cancelled."));
    plan.before = None;
    let dir = root.join(PAST);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = format!("{}-{}-{}.json", plan.made, plan.goal.runner, crate::profile::stamp().rsplit('-').next().unwrap_or_default());
    std::fs::write(dir.join(&name), serde_json::to_string_pretty(&plan).map_err(|e| e.to_string())? + "\n").map_err(|e| e.to_string())?;
    std::fs::remove_file(root.join(FILE)).map_err(|e| e.to_string())?;
    Ok(format!("Cancelled; it's kept under {PAST}/{name}."))
}

/// Cancelled plans, newest first.
pub fn past(root: &Path) -> Vec<ParentPlan> {
    let mut out: Vec<(String, ParentPlan)> = std::fs::read_dir(root.join(PAST))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let plan: ParentPlan = serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            Some((name, plan))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|p| p.1).collect()
}

/// The must-haves a finished uma lacks.
fn missing(kb: &Knowledge, run: &Run, own: &[u32]) -> String {
    run.must.iter().filter(|w| !w.met(own)).map(|w| w.label(kb)).collect::<Vec<_>>().join(", ")
}

fn created(v: &Veteran) -> String {
    v.create_time.as_str().unwrap_or_default().to_string()
}

fn newest(vets: &[Veteran]) -> String {
    vets.iter().map(created).max().unwrap_or_default()
}

pub fn load(root: &Path) -> Option<ParentPlan> {
    serde_json::from_str(&std::fs::read_to_string(root.join(FILE)).ok()?).ok()
}

pub fn save(root: &Path, plan: &ParentPlan) -> Result<(), String> {
    let path = root.join(FILE);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(plan).map_err(|e| e.to_string())? + "\n").map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{kb, spark};
    use super::*;
    use serde_json::json;

    fn vet(id: u64, card: u32, time: &str, factors: &[u32], parents: &[(u32, u32)]) -> Veteran {
        serde_json::from_value(json!({
            "trained_chara_id": id, "card_id": card, "create_time": time,
            "factor_info_array": factors.iter().map(|f| json!({"factor_id": f})).collect::<Vec<_>>(),
            "succession_chara_array": parents.iter().map(|(pos, c)| json!({"position_id": pos, "card_id": c})).collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    /// Oguri Cap needs one Dirt star; Haru Urara runs for it with the two finished umas there are.
    fn start() -> (Knowledge, Goal, Vec<Veteran>, ParentPlan) {
        let kb = kb();
        let goal = Goal { runner: 100601, targets: vec![1], ..Default::default() };
        let vets = vec![vet(1, 106801, "2026-10-01 10:00:00", &[spark(&kb, "pink", "Turf", 2)], &[(10, 100101), (20, 100201)]), vet(2, 100201, "2026-10-02 10:00:00", &[], &[(10, 100101), (20, 106801)])];
        let inv = Inventory::new(&kb, &vets, vec![(105201, 3)]);
        let p = super::super::jobs::propose(&kb, &goal, &inv);
        let plan = ParentPlan::new(goal.clone(), &p, &vets, "2026-10-27");
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].run.trainee, 105201);
        assert_eq!(plan.steps[0].after, "2026-10-02 10:00:00");
        (kb, goal, vets, plan)
    }

    #[test]
    fn import_meets_the_step() {
        let (kb, _, mut vets, mut plan) = start();
        // An older Haru Urara doesn't count; the new one has Dirt 2★.
        vets.push(vet(3, 105201, "2026-09-01 10:00:00", &[spark(&kb, "pink", "Dirt", 3)], &[]));
        vets.push(vet(4, 105201, "2026-10-20 10:00:00", &[spark(&kb, "pink", "Dirt", 2)], &[(10, 106801), (20, 100201)]));
        let hits = plan.hits(&vets);
        assert_eq!(hits, vec![Hit { id: 4, must: true, nice: vec![0], same_parents: true, short: 0 }]);
        assert!(super::super::text::import_note(&kb, &plan, &vets).is_some_and(|n| n.contains("#4")));
        let inv = Inventory::new(&kb, &vets, vec![(105201, 3)]);
        let msg = plan.settle(&kb, &inv, &vets, 4, "2026-10-21").unwrap();
        assert!(msg.contains("Step 1 done") && msg.contains("meet the goal"), "{msg}");
        assert_eq!(plan.current(), None);
        assert!(plan.settle(&kb, &inv, &vets, 4, "2026-10-21").is_err(), "nothing left to settle");
    }

    #[test]
    fn settling_short_replans() {
        let (kb, _, mut vets, mut plan) = start();
        vets.push(vet(4, 105201, "2026-10-20 10:00:00", &[spark(&kb, "pink", "Sprint", 2)], &[(10, 106801), (20, 100201)]));
        // One that meets the step, made before #4, and a newer one with other legacies: the one that
        // meets it comes first, whatever its age.
        vets.push(vet(6, 105201, "2026-10-19 10:00:00", &[spark(&kb, "pink", "Dirt", 1)], &[(10, 106801), (20, 100201)]));
        vets.push(vet(7, 105201, "2026-10-21 10:00:00", &[], &[(10, 100101), (20, 100201)]));
        let closest: Vec<u64> = plan.closest(&vets).iter().map(|h| h.id).collect();
        assert_eq!(closest[0], 6, "{closest:?}");
        vets.retain(|v| v.trained_chara_id != 6 && v.trained_chara_id != 7);
        let inv = Inventory::new(&kb, &vets, vec![(105201, 3)]);
        let before = plan.clone();
        let (after, msg) = plan.preview_settle(&kb, &inv, &vets, 4, "2026-10-21").unwrap();
        assert!(msg.contains("settled") && !after.is_empty(), "{msg}");
        assert_eq!(plan, before, "a preview changes nothing");
        assert!(!plan.hits(&vets)[0].must);
        assert!(plan.settle(&kb, &Inventory::new(&kb, &vets, vec![]), &vets, 1, "x").is_err(), "#1 isn't a Haru Urara");
        let inv = Inventory::new(&kb, &vets, vec![(105201, 3)]);
        let msg = plan.settle(&kb, &inv, &vets, 4, "2026-10-21").unwrap();
        assert!(msg.contains("settled") && msg.contains("Dirt pink 1★+"), "{msg}");
        assert!(plan.steps[0].settled && plan.blocked.is_none());
        // The goal still fits, so a new step follows, counting from the settled uma on.
        assert_eq!(plan.current(), Some(1));
        assert_eq!(plan.steps[1].after, "2026-10-20 10:00:00");
        // With nothing left to train, the plan waits for a choice, and says so rather than done.
        let none = Inventory::new(&kb, &vets, vec![]);
        let msg = plan.revalidate(&kb, &none, &vets, "2026-10-22");
        assert!(plan.blocked.is_some() && msg.contains("no longer fits"), "{msg}");
        assert_eq!(plan.steps.len(), 1, "only the step kept stays");
        let text = super::super::text::plan(&kb, &plan, &none, &vets);
        assert!(text.contains("Waiting for your choice") && !text.contains("Every step is done"), "{text}");
    }

    #[test]
    fn check_off_by_hand_then_import() {
        let (kb, _, mut vets, mut plan) = start();
        let inv = Inventory::new(&kb, &vets, vec![(105201, 3)]);
        let dirt = spark(&kb, "pink", "Dirt", 2);
        let msg = plan.check_off(&kb, &inv, &vets, Mark::Declared(vec![dirt]), "d1").unwrap();
        assert!(msg.contains("checked off") && msg.contains("meet the goal"), "{msg}");
        assert_eq!(plan.current(), None);
        // An import without her: nothing confirmed, nothing flagged.
        assert!(plan.confirm(&kb, &vets, "d2").is_empty());
        // One without what was declared: flagged.
        vets.push(vet(4, 105201, "2026-10-20 10:00:00", &[spark(&kb, "pink", "Sprint", 2)], &[(10, 106801), (20, 100201)]));
        let notes = plan.confirm(&kb, &vets, "d2");
        assert!(notes[0].contains("no imported") && notes[0].contains("Dirt 2★"), "{notes:?}");
        // The real one: confirmed.
        vets.push(vet(5, 105201, "2026-10-21 10:00:00", &[dirt, spark(&kb, "blue", "Speed", 1)], &[(10, 106801), (20, 100201)]));
        let notes = plan.confirm(&kb, &vets, "d3");
        assert!(notes[0].contains("#5"), "{notes:?}");
        assert_eq!((plan.steps[0].done, plan.steps[0].declared.is_none()), (Some(5), true));
    }

    #[test]
    fn undo_pin_goal_pause_and_cancel() {
        let (kb, goal, mut vets, mut plan) = start();
        let owned = vec![(105201, 3), (101901, 3)];
        let inv = Inventory::new(&kb, &vets, owned.clone());
        let plan0 = plan.clone();
        // Pin Agnes Digital as the next step's trainee: the step follows.
        let msg = plan.set_pin(&kb, &inv, &vets, Pin { trainee: Some(101901), ..Default::default() }, "d1");
        assert_eq!(plan.steps[0].run.trainee, 101901, "{msg}");
        assert!(msg.contains("Agnes Digital") && msg.contains("instead of"), "{msg}");
        // Undo puts it back.
        plan.undo("d2").unwrap();
        assert_eq!(plan.steps, plan0.steps);
        assert!(plan.undo("d2").is_err());
        // A settle, then undo reopens the step.
        vets.push(vet(4, 105201, "2026-10-20 10:00:00", &[spark(&kb, "pink", "Dirt", 2)], &[(10, 106801), (20, 100201)]));
        let inv = Inventory::new(&kb, &vets, owned.clone());
        plan.settle(&kb, &inv, &vets, 4, "d3").unwrap();
        assert_eq!(plan.current(), None);
        plan.undo("d4").unwrap();
        assert_eq!(plan.current(), Some(0));
        assert!(plan.log.iter().any(|l| l.contains("undid")));
        // A new goal, previewed first: Oguri for Long too (she's B there; Kitasan Black, A in
        // Long, can make the pink).
        let inv = Inventory::new(&kb, &vets, vec![(105201, 3), (101901, 3), (106801, 3)]);
        let longer = Goal { targets: vec![1, 5], ..goal.clone() };
        let (p, summary) = plan.preview_goal(&kb, &inv, &longer);
        assert!(!p.steps.is_empty() && !summary.is_empty());
        let before = plan.clone();
        assert_eq!(plan, before, "a preview changes nothing");
        plan.set_goal(&kb, &inv, &vets, longer.clone(), "d5");
        assert_eq!(plan.goal, longer);
        // Paused: no import note.
        plan.set_paused(true, "d6");
        assert!(super::super::text::import_note(&kb, &plan, &vets).is_none());
        // Cancel moves it to the past plans.
        let dir = std::env::temp_dir().join(format!("umalord-parent-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        save(&dir, &plan).unwrap();
        let msg = cancel(&dir, "d7").unwrap();
        assert!(load(&dir).is_none() && msg.contains(PAST));
        let past = past(&dir);
        assert_eq!(past.len(), 1);
        assert!(past[0].log.last().is_some_and(|l| l.contains("cancelled")) && past[0].before.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_steps_careers_split_by_mode() {
        let (kb, _, mut vets, plan) = start();
        vets.push(vet(4, 105201, "2026-10-20 10:00:00", &[spark(&kb, "pink", "Sprint", 2)], &[(10, 106801), (20, 100201)]));
        vets.push(vet(5, 105201, "2026-10-21 10:00:00", &[], &[(10, 106801), (20, 100201)]));
        let mut log = super::super::modes::ModeLog::default();
        assert_eq!(plan.step_modes(&vets, &log), (2, 0), "unmarked parent careers count as Independent Training");
        log.mark(5, super::super::modes::Mode::Manual);
        assert_eq!(plan.step_modes(&vets, &log), (1, 1));
    }

    #[test]
    fn stuck_steps_give_way_to_another_route() {
        let (kb, _, vets, mut plan) = start();
        // Only Haru Urara can run for the Dirt star: no other route, and the plan stays.
        let inv = Inventory::new(&kb, &vets, vec![(105201, 3)]);
        let before = plan.clone();
        let err = plan.another_route(&kb, &inv, &vets, "d1").unwrap_err();
        assert!(err.contains("No other route") && err.contains("settle"), "{err}");
        assert_eq!(plan, before);
        // With Agnes Digital owned too, the stuck Haru Urara step gives way to hers; the preview
        // shows it first and changes nothing.
        let inv = Inventory::new(&kb, &vets, vec![(105201, 3), (101901, 3)]);
        let (after, _) = plan.preview_another_route(&kb, &inv, &vets, "d2").unwrap();
        assert_eq!(after[0].trainee, 101901);
        assert_eq!(plan, before, "a preview changes nothing");
        let msg = plan.another_route(&kb, &inv, &vets, "d2").unwrap();
        assert!(msg.contains("Step 1 set aside"), "{msg}");
        assert_eq!(plan.steps[0].run.trainee, 101901, "{msg}");
        assert_eq!(plan.pin.avoid.len(), 1);
        // A later pin keeps the step set aside; a re-plan doesn't bring it back.
        plan.set_pin(&kb, &inv, &vets, Pin { legacy: Some(1), ..Default::default() }, "d3");
        assert_eq!(plan.pin.avoid.len(), 1);
        assert!(plan.steps.iter().all(|s| !(s.run.trainee == 105201 && !s.finished())));
        plan.undo("d4").unwrap();
        plan.undo("d4").unwrap();
        assert_eq!(plan.steps, before.steps, "undo brings the step back");
    }

    #[test]
    fn other_legacies_or_older_umas_dont_count() {
        let (kb, _, mut vets, mut plan) = start();
        // Dirt 2★, but run with other legacies: the route counted the step's legacies' pinks.
        vets.push(vet(4, 105201, "2026-10-20 10:00:00", &[spark(&kb, "pink", "Dirt", 2)], &[(10, 100101), (20, 100201)]));
        vets.push(vet(5, 105201, "2026-09-20 10:00:00", &[spark(&kb, "pink", "Dirt", 2)], &[(10, 106801), (20, 100201)]));
        let hits = plan.hits(&vets);
        assert_eq!(hits.len(), 1);
        assert!(!hits[0].must && !hits[0].same_parents);
        assert!(super::super::text::import_note(&kb, &plan, &vets).is_none());
        let inv = Inventory::new(&kb, &vets, vec![(105201, 3)]);
        assert!(plan.settle(&kb, &inv, &vets, 5, "x").is_err(), "#5 finished before the step began");
        let msg = plan.settle(&kb, &inv, &vets, 4, "x").unwrap();
        assert!(msg.contains("other legacies") && plan.steps[0].settled, "{msg}");
    }
}

/// A player following plans on the real account with the dice rolled for them: run the current
/// step, reroll the sparks once, keep the first uma with the must-haves, and let the plan work
/// itself out again. Every plan must finish (or say it no longer fits), and the careers it takes
/// should match what it predicted.
/// `cargo test --release parentage_play_through -- --ignored --nocapture`.
#[cfg(test)]
#[test]
#[ignore]
fn parentage_play_through() {
    use super::outcome::PINK_STARS;
    use super::{after_start, ranks};
    use serde_json::json;
    let kb = super::tests::kb();
    let root = std::env::var_os("UMALORD_ROOT").map(std::path::PathBuf::from).or_else(|| crate::data::find_root(None)).unwrap();
    let base_vets = crate::veterans::load(&root);
    let (roster, _) = crate::data::Roster::load(Some(&root));
    let owned: Vec<(u32, u8)> = roster.umas.iter().map(|u| (u.id, u.stars)).collect();
    let inv0 = Inventory::new(&kb, &base_vets, owned.clone());
    let mut goals = Vec::new();
    for &(runner, _) in &owned {
        for surface in [0, 1] {
            for distance in 2..6 {
                for style in 6..10 {
                    let goal = Goal { runner, targets: vec![surface, distance, style], ..Default::default() };
                    let p = super::jobs::propose(&kb, &goal, &inv0);
                    if !p.steps.is_empty() && !p.over_cap {
                        goals.push((goal, p.careers()));
                    }
                }
            }
        }
    }
    let filter = std::env::var("GOALS").unwrap_or_default();
    let goals: Vec<_> = if filter.is_empty() {
        goals.iter().step_by((goals.len() / 20).max(1)).cloned().collect()
    } else {
        goals.into_iter().filter(|(g, _)| filter.split(';').any(|f| super::text::goal(&kb, g).contains(f))).collect()
    };
    let mut rng = crate::sim::rng::Rng::new(12345.0);
    let mut pick = |weights: &[f64]| {
        let r = rng.random() * weights.iter().sum::<f64>();
        let mut acc = 0.0;
        weights.iter().position(|w| {
            acc += w;
            r < acc
        }).unwrap_or(weights.len() - 1)
    };
    let (mut total_pred, mut total_real, mut blocked) = (0.0, 0.0, 0);
    for (goal, predicted) in &goals {
        let trials = 12;
        let mut careers_sum = 0usize;
        for _ in 0..trials {
            let mut vets = base_vets.clone();
            let inv = Inventory::new(&kb, &vets, owned.clone());
            let mut plan = ParentPlan::new(goal.clone(), &super::jobs::propose(&kb, goal, &inv), &vets, "t");
            let mut careers = 0;
            let mut next_id = 900_000u64;
            while let Some(cur) = plan.current() {
                assert!(careers < 300, "{}: never finishes", super::text::goal(&kb, goal));
                careers += 1;
                let run = plan.steps[cur].run.clone();
                let legs: Vec<&crate::veterans::Veteran> = run.legacies.iter().filter_map(|id| vets.iter().find(|v| v.trained_chara_id == *id)).collect();
                let lp: Vec<super::Parent> = legs.iter().map(|v| super::Parent::from_veteran(&kb, v)).collect();
                let base = ranks(&kb.umas[&run.trainee].aptitude);
                let fin: Vec<u8> = (0..10).map(|i| after_start(base[i], lp.iter().map(|p| u32::from(p.pink_line[i])).sum())).collect();
                // The Wiki's grade weights, the rule the plan's odds use.
                let weights: Vec<f64> = fin.iter().map(|r| [0.0, 0.10, 0.25, 0.45, 0.65, 0.775, 0.875, 1.0, 1.075][*r as usize]).collect();
                // Two draws (the reroll); keep the one with the must-haves, else the first.
                let mut draw = || {
                    let apt = pick(&weights);
                    let stars = 1 + pick(&PINK_STARS) as u8;
                    let stat = pick(&[1.0; 5]);
                    let bstars = 1 + pick(&[0.45, 0.50, 0.05]) as u8;
                    let gstars = 1 + pick(&[0.45, 0.50, 0.05]) as u8;
                    let mut f = vec![super::pink_spark(apt, stars), (stat as u32 + 1) * 100 + u32::from(bstars)];
                    if owned.iter().any(|(u, s)| *u == run.trainee && *s >= 3) {
                        f.push(super::green_spark(run.trainee, gstars));
                    }
                    f
                };
                let a = draw();
                let b = draw();
                let own = if run.must.iter().all(|w| w.met(&a)) { a } else { b };
                if !run.must.iter().all(|w| w.met(&own)) {
                    continue;
                }
                next_id += 1;
                let anc: Vec<serde_json::Value> = legs.iter().zip([10, 20]).map(|(v, pos)| json!({"position_id": pos, "card_id": v.card_id, "factor_info_array": v.factors().iter().map(|f| json!({"factor_id": f})).collect::<Vec<_>>()})).collect();
                let v: crate::veterans::Veteran = serde_json::from_value(json!({"trained_chara_id": next_id, "card_id": run.trainee, "create_time": format!("2027-01-01 00:{:02}:{:02}", careers / 60, careers % 60), "factor_info_array": own.iter().map(|f| json!({"factor_id": f})).collect::<Vec<_>>(), "succession_chara_array": anc, "speed": 900, "stamina": 800, "power": 800, "guts": 400, "wiz": 600, "rank": 14})).unwrap();
                vets.push(v);
                let inv = Inventory::new(&kb, &vets, owned.clone());
                plan.settle(&kb, &inv, &vets, next_id, "t").unwrap();
                if plan.blocked.is_some() {
                    blocked += 1;
                    break;
                }
            }
            if !filter.is_empty() {
                eprintln!("  {careers} careers: {}", plan.log.join(" | "));
            }
            careers_sum += careers;
        }
        let real = careers_sum as f64 / f64::from(trials);
        total_pred += predicted;
        total_real += real;
        eprintln!("{:>5.1} predicted {:>5.1} played  {}", predicted, real, super::text::goal(&kb, goal));
    }
    eprintln!("all: {total_pred:.1} predicted, {total_real:.1} played, {blocked} blocked");
}
