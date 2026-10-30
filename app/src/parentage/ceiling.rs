//! How to raise a plan's ceiling: after a plan is made, each change the player could make (an uma
//! they don't own, a card they don't own or haven't limit broken, a better parent to borrow) is
//! tried on its own and measured the way the plan was: the same races, the same scoring. The
//! results are ranked by gain. Each is a separate change; their gains don't simply add up.
//!
//! The trials run on the build's headless copy of the app: an uma or card "owned" for a trial is
//! added to that copy's roster and taken out again, never saved.

use super::tt::{ROWS, TtMeta, TtTeam};
use super::{Goal, Inventory, uma_name};
use crate::App;
use crate::data::{CardType, Rarity, SupportCard, Uma};
use crate::knowledge::EventPreset;
use crate::sim::run::UmaBuild;
use crate::tab_planner::Plan;
use serde::{Deserialize, Serialize};

/// What the player would change.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub enum LeverKind {
    /// Scout an uma they don't own.
    #[default]
    GetUma,
    /// Get a support card they don't own.
    GetCard,
    /// Limit-break a card they own to MLB.
    LimitBreak,
    /// Borrow (or breed) a parent with better sparks.
    Parent,
}

/// What a gain is counted in.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub enum Unit {
    /// Team Trials points a round.
    #[default]
    Points,
    /// Lengths gained a race, head to head (CM: a near-equal build wins most races by a
    /// little, so a share would overstate it).
    Lengths,
    /// Careers saved.
    Careers,
    /// Expected stat points at the end of the career.
    Stats,
}

/// One way to raise the plan, measured.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Lever {
    pub kind: LeverKind,
    /// The uma to get, or whose career the change is for.
    pub uma: Option<u32>,
    /// The card to get or limit-break, and its LB now (None: not owned).
    pub card: Option<u32>,
    pub lb: Option<u8>,
    /// For a parent: the spark wanted ("End Closer pink 3★").
    pub spark: Option<String>,
    /// One line: "Get Kitasan Black [Get to Winning!] for Medium".
    pub title: String,
    /// Why, in one line.
    pub why: String,
    pub gain: f64,
    pub unit: Unit,
    /// How it was measured: "simulated · 200 races".
    pub measured: String,
    /// Other runners the same change helps (a card for several decks).
    pub helps: Vec<u32>,
    /// What was compared, for the tag's hover.
    pub compared: String,
    /// The title in a few words, for the Overview: "Get Hishi Amazon for Medium".
    pub short: String,
    /// A parent: the characters to look for among friends' legacies (best first), and the
    /// finished uma she pairs with.
    pub look: Vec<u32>,
    pub partner: Option<u64>,
}

/// What a ceiling search tried, for when nothing was worth listing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Tried {
    pub umas: usize,
    pub cards: usize,
    pub limit_breaks: usize,
    pub parents: usize,
}

impl Tried {
    /// "9 umas you don't own, 6 cards, 2 limit breaks and a better parent for 3 runners".
    pub fn text(&self) -> String {
        let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
        let mut parts = Vec::new();
        if self.umas > 0 {
            parts.push(n(self.umas, "uma you don't own", "umas you don't own"));
        }
        if self.cards > 0 {
            parts.push(n(self.cards, "card", "cards"));
        }
        if self.limit_breaks > 0 {
            parts.push(n(self.limit_breaks, "limit break", "limit breaks"));
        }
        if self.parents > 0 {
            parts.push(format!("a better parent for {}", n(self.parents, "runner", "runners")));
        }
        match parts.len() {
            0 => "nothing".into(),
            1 => parts.remove(0),
            _ => {
                let last = parts.pop().unwrap_or_default();
                format!("{} and {last}", parts.join(", "))
            }
        }
    }

    fn count(&mut self, lb: Option<u8>) {
        if lb.is_some() {
            self.limit_breaks += 1;
        } else {
            self.cards += 1;
        }
    }
}

impl Lever {
    /// "+8,400 pts a round", "+12% win share", "−3 careers", "+140 stats".
    pub fn gain_text(&self) -> String {
        let n = crate::lineage_kit::thousands(self.gain.abs());
        match self.unit {
            Unit::Points => format!("+{n} pts a round"),
            Unit::Lengths => format!("+{:.1} lengths a race", self.gain),
            Unit::Careers => format!("{:.0} fewer career{}", self.gain, if self.gain.round() == 1.0 { "" } else { "s" }),
            Unit::Stats => format!("+{n} stats"),
        }
    }
}

/// Races per trial: enough to tell a real gain from noise at the planner's own scale.
const RACES: usize = 200;
/// Card trials per runner, and uma trials per race (or per CM style).
const CARDS_EACH: usize = 2;
const UMAS_EACH: usize = 1;
/// Gains below these aren't worth listing.
const MIN_POINTS: f64 = 300.0;
const MIN_LENGTHS: f64 = 0.2;
const MIN_STATS: f64 = 20.0;

impl App {
    /// Run `f` with outfit `id` in the roster (3★, potential 1, as a new pull), then take her out.
    fn with_uma<T>(&mut self, id: u32, f: impl FnOnce(&mut Self) -> T) -> T {
        let added = self.owned_uma(id).is_none();
        if added && let Some(info) = self.kb.umas.get(&id) {
            self.roster.umas.push(Uma { id, name: info.name.clone(), outfit: info.outfit.clone(), stars: 3, potential: 1, notes: String::new(), url: String::new() });
        }
        let out = f(self);
        if added {
            self.roster.umas.retain(|u| u.id != id);
        }
        out
    }

    /// Run `f` with card `id` owned at MLB and its top level, then put the roster back.
    fn with_mlb<T>(&mut self, id: u32, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.roster.cards.iter().position(|c| c.id == id).map(|i| (i, self.roster.cards[i].lb, self.roster.cards[i].level));
        let rarity = Rarity::from_id(id).unwrap_or(Rarity::SSR);
        match saved {
            Some((i, _, _)) => {
                self.roster.cards[i].lb = 4;
                self.roster.cards[i].level = rarity.max_level(4);
            }
            None => {
                if let Some(ci) = self.kb.cards.get(&id) {
                    let card_type = CardType::parse(&ci.kind).unwrap_or(CardType::Speed);
                    self.roster.cards.push(SupportCard { id, title: ci.title.clone(), uma: ci.uma.clone(), card_type, rarity, lb: 4, level: rarity.max_level(4), notes: String::new(), url: String::new() });
                }
            }
        }
        let out = f(self);
        match saved {
            Some((i, lb, level)) => {
                self.roster.cards[i].lb = lb;
                self.roster.cards[i].level = level;
            }
            None => self.roster.cards.retain(|c| c.id != id),
        }
        out
    }

    /// A plan's runner as her career would end with this deck: the career simulation's expected
    /// stats (not the targets, so a better deck shows), the skills picked again for it.
    fn career_build(&mut self, plan: &Plan) -> Option<(UmaBuild, [f64; 5])> {
        let saved = std::mem::replace(&mut self.planner.plan, plan.clone());
        self.auto_pick_skills();
        let out = self.plan_career().and_then(|c| {
            let stats = c.stats_mean.map(|v| v.round().max(1.0) as u32);
            self.plan_sim_build_for(true, stats).ok().map(|b| (b, c.stats_mean))
        });
        self.planner.plan = saved;
        out
    }

    /// The best cards for a plan's deck that the player doesn't have at MLB: for each of her own
    /// five slots, the top card if every released SSR were owned at MLB, when it isn't the card
    /// there now (a new card), or the card there isn't MLB (a limit break). The most promising
    /// first, by the ranking's own score.
    fn card_candidates(&mut self, plan: &Plan) -> Vec<(usize, u32, Option<u8>)> {
        let saved = std::mem::replace(&mut self.planner.plan, plan.clone());
        let mut ctx = self.rank_context();
        let mlb = ctx.friend.clone();
        ctx.owned = mlb;
        let deck = [plan.deck[0], plan.deck[1], plan.deck[2], plan.deck[3], plan.deck[4], plan.friend];
        let lb = |app: &App, id: u32| app.owned_card(id).map(|c| c.lb);
        let mut out: Vec<(f32, usize, u32, Option<u8>)> = Vec::new();
        for slot in 0..5 {
            let ranked = crate::recommend::rank_slot(&ctx, &deck, slot);
            let Some(best) = ranked.first() else { continue };
            let now = deck[slot].and_then(|id| ranked.iter().find(|c| c.id == id)).map_or(0.0, |c| c.score);
            if Some(best.id) != deck[slot] && lb(self, best.id) != Some(4) {
                out.push((best.score - now, slot, best.id, lb(self, best.id)));
            } else if let Some(id) = deck[slot].filter(|id| lb(self, *id).is_some_and(|l| l < 4)) {
                out.push((best.score.max(now) * 0.25, slot, id, lb(self, id)));
            }
        }
        self.planner.plan = saved;
        out.sort_by(|a, b| b.0.total_cmp(&a.0));
        // One trial per card: the slot where it ranked best.
        let mut seen = Vec::new();
        out.retain(|c| {
            let new = !seen.contains(&c.2);
            seen.push(c.2);
            new
        });
        out.into_iter().take(CARDS_EACH).map(|(_, s, id, l)| (s, id, l)).collect()
    }

    /// A card trial's plan: the card in the slot, the rest of the deck as it was.
    fn with_card_in(plan: &Plan, slot: usize, card: u32) -> Plan {
        let mut p = plan.clone();
        p.deck[slot] = Some(card);
        p
    }

    /// A card lever's kind, title and short title.
    fn card_title(&self, card: u32, lb: Option<u8>, runner: u32) -> (LeverKind, String, String) {
        let c = self.kb.cards.get(&card);
        let name = c.map_or(format!("card {card}"), |c| format!("{} [{}]", c.uma, c.title));
        let uma = c.map_or(String::new(), |c| c.uma.clone());
        let who = super::short_name(&self.kb, runner);
        match lb {
            Some(l) => (LeverKind::LimitBreak, format!("Your {name} card from LB{l} to MLB, for {who}"), format!("Limit-break {uma} to MLB")),
            None => (LeverKind::GetCard, format!("Get {name}{}", c.map_or(String::new(), |c| format!(" ({} {})", c.rarity, c.kind))), format!("Get the {uma} card")),
        }
    }

    /// Team Trials: the community's runners the player doesn't own, one per race, raced against
    /// that race's team; and better cards for each race's Ace, raced against her as planned.
    pub(crate) fn tt_ceiling(&mut self, team: &TtTeam, meta: &TtMeta) -> (Vec<Lever>, Tried) {
        let mut tried = Tried::default();
        let Some(data) = self.sim_data.clone() else { return (Vec::new(), tried) };
        let saved = std::mem::take(&mut self.planner.plan);
        let today = crate::ui_util::today();
        let race = crate::sim::run::race_params(1, 1, 1, 2, None);
        let opts = crate::sim::run::SimOptions { samples: RACES, seed: 12345.0, ..Default::default() };
        let measured = format!("simulated · {RACES} races");
        let mut out = Vec::new();
        let on_team: Vec<u32> = team.rows.iter().flat_map(|r| r.slots.iter().map(|s| crate::affinity::chara(s.trainee))).collect();
        for row in &team.rows {
            if self.check_cancel().is_err() {
                break;
            }
            let i = ROWS.iter().position(|r| r.0 == row.name).unwrap_or(0);
            let Some(course) = self.tt_course(meta, i).and_then(|c| data.courses.get(&c).cloned()) else { continue };
            let course = std::sync::Arc::new(course);
            let mut builds: Vec<Option<UmaBuild>> = Vec::new();
            for s in &row.slots {
                self.planner.plan = s.plan.clone();
                builds.push(self.plan_sim_build(true).ok());
            }
            let n = builds.len();
            let stars_of = |app: &App, t: u32| app.owned_uma(t).map_or(3, |u| u.stars);
            // Today's points for the race's team, in this pool.
            let score = |app: &App, raced: &[(Option<f64>, std::collections::BTreeMap<u32, f64>)], who: &[(u32, crate::knowledge::Style)]| -> Vec<(f64, crate::knowledge::Style)> {
                who.iter().zip(raced).map(|((t, st), (share, rates))| (super::tt::placement(*share) + super::tt::skills_score(&app.kb, meta, *t, stars_of(app, *t), rates), *st)).collect()
            };
            // An uma the player doesn't own: the meta's runners for this race, in its order.
            let picks: Vec<(u32, crate::knowledge::Style)> = meta
                .roles
                .list
                .iter()
                .filter(|r| r.races.iter().any(|x| x == row.name) && self.owned_uma(r.id).is_none() && !on_team.contains(&crate::affinity::chara(r.id)))
                .filter(|r| self.kb.umas.get(&r.id).is_some_and(|u| u.release.as_str() <= today.as_str()))
                .filter_map(|r| {
                    let a = &self.kb.umas.get(&r.id)?.aptitude;
                    let st = r.styles.iter().filter_map(|x| super::tt::parse_style(x)).find(|s| crate::knowledge::grade_rank(a.style(*s)) >= 6).unwrap_or_else(|| a.best_style());
                    Some((r.id, st))
                })
                .take(UMAS_EACH)
                .collect();
            for (t, style) in picks {
                let built = self.with_uma(t, |app| {
                    let plan = app.tt_runner_plan(meta, row.name, t, style);
                    app.planner.plan = plan;
                    app.plan_sim_build(true).ok()
                });
                let Some(b) = built else { continue };
                tried.umas += 1;
                let mut pool = builds.clone();
                pool.push(Some(b));
                let raced = super::baseline::race_pool(&data, &course, &race, &pool, &opts);
                let mut who: Vec<(u32, crate::knowledge::Style)> = row.slots.iter().map(|s| (s.trainee, s.style)).collect();
                who.push((t, style));
                let pts = self.with_uma(t, |app| score(app, &raced, &who));
                let now = super::tt::team_points(&pts[..n]);
                // She takes the weakest place, if she beats it.
                let weakest = (0..n).min_by(|a, b| pts[*a].0.total_cmp(&pts[*b].0));
                let with = match weakest {
                    Some(w) if n == 3 => {
                        let mut v: Vec<(f64, crate::knowledge::Style)> = pts[..n].to_vec();
                        v[w] = pts[n];
                        super::tt::team_points(&v)
                    }
                    _ => super::tt::team_points(&pts),
                };
                let gain = with - now;
                self.note(format!("{}: {} would score about {} a round; the race {}.", row.name, uma_name(&self.kb, t), crate::lineage_kit::thousands(pts[n].0), if gain > 0.0 { format!("gains {}", crate::lineage_kit::thousands(gain)) } else { "doesn't gain".into() }));
                if gain >= MIN_POINTS {
                    let replaced = weakest.map(|w| super::short_name(&self.kb, row.slots[w].trainee)).unwrap_or_default();
                    out.push(Lever {
                        kind: LeverKind::GetUma,
                        uma: Some(t),
                        title: format!("Get {} for {}", uma_name(&self.kb, t), row.name),
                        short: format!("Get {} for {}", super::short_name(&self.kb, t), row.name),
                        why: if n == 3 { format!("The community runs her in {}; in head-to-heads she'd take {replaced}'s place.", row.name) } else { format!("The community runs her in {}, and the race has a free place.", row.name) },
                        compared: format!("The plan's {race} three with {} in {replaced}'s place, against the plan's three as they are: {RACES} races each on a {race} course.", super::short_name(&self.kb, t), race = row.name),
                        gain,
                        unit: Unit::Points,
                        measured: measured.clone(),
                        ..Default::default()
                    });
                }
            }
            // Better cards for the race's Ace: her career with the card against her as planned.
            let Some(ace) = row.slots.first() else { continue };
            let cands = self.card_candidates(&ace.plan);
            let Some((old, _)) = self.career_build(&ace.plan) else { continue };
            for (slot, card, lb) in cands {
                if self.check_cancel().is_err() {
                    break;
                }
                let plan = Self::with_card_in(&ace.plan, slot, card);
                let Some((new, _)) = self.with_mlb(card, |app| app.career_build(&plan)) else { continue };
                tried.count(lb);
                let mut pool: Vec<Option<UmaBuild>> = vec![Some(old.clone()), Some(new)];
                pool.extend(builds[1..].iter().cloned());
                let raced = super::baseline::race_pool(&data, &course, &race, &pool, &opts);
                let stars = stars_of(self, ace.trainee);
                let pts = |k: usize| super::tt::placement(raced[k].0) + super::tt::skills_score(&self.kb, meta, ace.trainee, stars, &raced[k].1);
                // The Ace scores 10% more.
                let gain = (pts(1) - pts(0)) * 1.1;
                let (kind, title, short) = self.card_title(card, lb, ace.trainee);
                self.note(format!("{} ({} Ace) with {}: {} a round.", super::short_name(&self.kb, ace.trainee), row.name, self.kb.cards.get(&card).map_or(String::new(), |c| c.uma.clone()), if gain >= 0.0 { format!("+{}", crate::lineage_kit::thousands(gain)) } else { format!("−{}", crate::lineage_kit::thousands(-gain)) }));
                if gain >= MIN_POINTS {
                    out.push(Lever {
                        kind,
                        uma: Some(ace.trainee),
                        card: Some(card),
                        lb,
                        title,
                        short,
                        why: format!("For {}, {}'s Ace: her simulated career ends stronger with it in her deck.", super::short_name(&self.kb, ace.trainee), row.name),
                        compared: format!("{who}'s career simulated with the card {} and as planned, then each {who} raced with the plan's other {race} runners: {RACES} races each.", if lb.is_some() { "at MLB" } else { "in her deck" }, who = super::short_name(&self.kb, ace.trainee), race = row.name),
                        gain,
                        unit: Unit::Points,
                        measured: format!("career simulated, then {RACES} races"),
                        ..Default::default()
                    });
                }
            }
        }
        self.planner.plan = saved;
        (rank(out), tried)
    }

    /// Champions Meeting: the sheet's picks the player doesn't own, raced head to head against
    /// the runner of their style; better cards for each runner; and a better parent where a
    /// runner still needs parent careers.
    pub(crate) fn cm_ceiling(&mut self, team: &super::team::CmTeam, preset: &EventPreset, inv: &Inventory) -> (Vec<Lever>, Tried) {
        let mut tried = Tried::default();
        let Some(data) = self.sim_data.clone() else { return (Vec::new(), tried) };
        let saved = std::mem::take(&mut self.planner.plan);
        let mut out = Vec::new();
        let opts = crate::sim::run::SimOptions { samples: RACES, seed: 12345.0, ..Default::default() };
        let today = crate::ui_util::today();
        let on_team: Vec<u32> = team.runners.iter().map(|r| crate::affinity::chara(r.trainee)).collect();
        for r in &team.runners {
            if self.check_cancel().is_err() {
                break;
            }
            self.planner.plan = r.plan.clone();
            let setup = self.plan_race_setup();
            let Some(course) = setup.course.and_then(|c| data.courses.get(&c).cloned()) else { continue };
            let course = std::sync::Arc::new(course);
            let race = crate::sim::run::race_params(i32::from(setup.ground.unwrap_or(1)), i32::from(setup.weather.unwrap_or(1)), i32::from(setup.season.unwrap_or(1)), i32::from(setup.time.unwrap_or(2)), None);
            // The second build's mean lead in lengths, and the share of races it wins.
            let head = |a: &UmaBuild, b: &UmaBuild| -> (f64, f64) {
                let res = crate::sim::run::run_comparison(&data, &course, &race, a, b, &opts, true, false, None);
                let runs = res.gaps.len().max(1) as f64;
                (res.mean, res.gaps.iter().filter(|g| **g > 0.0).count() as f64 / runs)
            };
            let mut planned = match self.plan_sim_build(true) {
                Ok(b) => b,
                Err(_) => continue,
            };
            planned.skills.retain(|s| !preset.disabled_skills.contains(s));
            // The sheet's picks for her style that the player doesn't own.
            let picks: Vec<u32> = self.plan_meta_trainees().into_iter().filter(|t| self.owned_uma(*t).is_none() && !on_team.contains(&crate::affinity::chara(*t)) && self.kb.umas.get(t).is_some_and(|u| u.release.as_str() <= today.as_str())).take(UMAS_EACH).collect();
            for t in picks {
                let built = self.with_uma(t, |app| {
                    let mut plan = r.plan.clone();
                    plan.trainee = Some(t);
                    app.planner.plan = plan;
                    app.recommend_deck(true);
                    app.auto_pick_skills();
                    app.plan_sim_build(true).ok()
                });
                let Some(mut b) = built else { continue };
                tried.umas += 1;
                b.skills.retain(|s| !preset.disabled_skills.contains(s));
                let (gain, share) = head(&planned, &b);
                self.note(format!("{} against {} ({}): {:+.1} lengths a race.", uma_name(&self.kb, t), super::short_name(&self.kb, r.trainee), r.style.label(), gain));
                if gain >= MIN_LENGTHS {
                    out.push(Lever {
                        kind: LeverKind::GetUma,
                        uma: Some(t),
                        title: format!("Get {} as the team's {}", uma_name(&self.kb, t), r.style.label()),
                        short: format!("Get {}", super::short_name(&self.kb, t)),
                        compared: format!("{} against {}, head to head: {RACES} races on {}.", super::short_name(&self.kb, t), super::short_name(&self.kb, r.trainee), preset.course_name),
                        why: format!("The meta sheet recommends her for CM {}; she finishes ahead of {} in {:.0}% of head-to-heads.", preset.id, super::short_name(&self.kb, r.trainee), share * 100.0),
                        gain,
                        unit: Unit::Lengths,
                        measured: format!("simulated · {RACES} races"),
                        ..Default::default()
                    });
                }
            }
            // Better cards: her career with the card against her career as planned.
            let cands = self.card_candidates(&r.plan);
            let Some((mut old, _)) = self.career_build(&r.plan) else { continue };
            old.skills.retain(|s| !preset.disabled_skills.contains(s));
            for (slot, card, lb) in cands {
                if self.check_cancel().is_err() {
                    break;
                }
                let plan = Self::with_card_in(&r.plan, slot, card);
                let Some((mut new, _)) = self.with_mlb(card, |app| app.career_build(&plan)) else { continue };
                tried.count(lb);
                new.skills.retain(|s| !preset.disabled_skills.contains(s));
                let (gain, share) = head(&old, &new);
                let (kind, title, short) = self.card_title(card, lb, r.trainee);
                self.note(format!("{} with {}: {:+.1} lengths a race against her career as planned.", super::short_name(&self.kb, r.trainee), self.kb.cards.get(&card).map_or(String::new(), |c| c.uma.clone()), gain));
                if gain >= MIN_LENGTHS {
                    out.push(Lever {
                        kind,
                        uma: Some(r.trainee),
                        card: Some(card),
                        lb,
                        title,
                        short,
                        compared: format!("{who}'s career simulated with the card {} and as planned, then the two raced head to head: {RACES} races on {}.", if lb.is_some() { "at MLB" } else { "in her deck" }, preset.course_name, who = super::short_name(&self.kb, r.trainee)),
                        why: format!("For {}: her simulated career ends stronger with it, and finishes ahead of her career as planned in {:.0}% of races.", super::short_name(&self.kb, r.trainee), share * 100.0),
                        gain,
                        unit: Unit::Lengths,
                        measured: format!("career simulated, then {RACES} races"),
                        ..Default::default()
                    });
                }
            }
            if let Some(l) = self.parent_lever(&r.goal, inv, &r.parents, &mut tried) {
                out.push(l);
            }
        }
        self.planner.plan = saved;
        (rank(out), tried)
    }

    /// A friend's legacy (or one the player breeds) carrying what the runner's lineage still
    /// lacks, and the parent careers it saves.
    pub(crate) fn parent_lever(&mut self, goal: &Goal, inv: &Inventory, now: &super::jobs::Proposal, tried: &mut Tried) -> Option<Lever> {
        if now.steps.is_empty() {
            return None;
        }
        tried.parents += 1;
        let wish = super::jobs::borrow_wish(&self.kb, goal, inv).filter(|w| w.possible)?;
        let chara = wish.charas.first()?.0;
        let uma = *self.kb.umas.keys().filter(|u| crate::affinity::chara(**u) == chara).min()?;
        // One pink each for her and her two parents, at most 3★.
        let mut pinks: Vec<u32> = Vec::new();
        for (i, n) in &wish.stars {
            let mut left = *n;
            while left > 0 {
                let s = left.min(3);
                pinks.push(super::pink_spark(*i, s as u8));
                left -= s;
            }
        }
        let mut own: Vec<u32> = pinks.first().copied().into_iter().collect();
        if let Some(g) = wish.green {
            own.push(super::green_spark(g, 1));
        }
        let parent = |k: usize| (0, pinks.get(k).copied().into_iter().collect::<Vec<u32>>());
        let borrow = super::Borrow { uma, own, parents: [parent(1), parent(2)] };
        let mut inv2 = inv.clone();
        inv2.borrow(&self.kb, &borrow);
        let after = super::jobs::propose(&self.kb, goal, &inv2);
        // Counted as the steps list counts them: each step's usual number of careers.
        // (An empty sum is -0.0, which prints as "-0".)
        let usual = |p: &super::jobs::Proposal| -> f64 { p.steps.iter().map(|r| f64::from(super::verify::step_spread(r.p).0.min(50))).sum::<f64>() + 0.0 };
        let saved = usual(now) - usual(&after);
        let spark = wish.stars.iter().map(|(i, n)| format!("{} pink {n}★", super::APTITUDES[*i])).collect::<Vec<_>>().join(" and ");
        self.note(format!("A friend's legacy with {spark} for {}: usually {:.0} parent careers instead of {:.0}.", super::short_name(&self.kb, goal.runner), usual(&after), usual(now)));
        (saved >= 1.0).then(|| Lever {
            kind: LeverKind::Parent,
            uma: Some(goal.runner),
            spark: Some(spark.clone()),
            title: format!("Borrow a friend's legacy with {spark} for {}", super::short_name(&self.kb, goal.runner)),
            short: format!("Borrow a legacy with {spark}"),
            compared: format!("The parent plan made again with a friend's legacy carrying {spark} as one of {}'s parents, against the plan as it is. Everything else is planned the same way.", super::short_name(&self.kb, goal.runner)),
            why: if usual(&after) < 0.5 {
                format!("Paired with your #{}, she starts {} at A without the parent careers.", wish.partner, super::short_name(&self.kb, goal.runner))
            } else {
                format!("Paired with your #{}, she starts {} at A: usually {:.0} parent careers instead of {:.0}.", wish.partner, super::short_name(&self.kb, goal.runner), usual(&after), usual(now))
            },
            look: wish.charas.iter().filter_map(|(c, _)| self.kb.umas.keys().filter(|u| crate::affinity::chara(**u) == *c).min().copied()).collect(),
            partner: Some(wish.partner),
            gain: saved,
            unit: Unit::Careers,
            measured: "planned · the same routes".into(),
            ..Default::default()
        })
    }

    /// One runner: better cards for her own career, measured by the career simulation's expected
    /// final stats; and a better parent.
    pub(crate) fn runner_ceiling(&mut self, goal: &Goal, plan: &Plan, inv: &Inventory, now: &super::jobs::Proposal) -> (Vec<Lever>, Tried) {
        let mut tried = Tried::default();
        let saved = std::mem::take(&mut self.planner.plan);
        let mut out = Vec::new();
        let cands = self.card_candidates(plan);
        if let Some((_, old)) = self.career_build(plan) {
            for (slot, card, lb) in cands {
                let p = Self::with_card_in(plan, slot, card);
                let Some((_, new)) = self.with_mlb(card, |app| app.career_build(&p)) else { continue };
                tried.count(lb);
                let gain: f64 = (0..5).map(|i| new[i] - old[i]).sum();
                let (kind, title, short) = self.card_title(card, lb, goal.runner);
                if gain >= MIN_STATS {
                    let best = (0..5).max_by(|a, b| (new[*a] - old[*a]).total_cmp(&(new[*b] - old[*b]))).unwrap_or(0);
                    out.push(Lever {
                        kind,
                        uma: plan.trainee,
                        card: Some(card),
                        lb,
                        title,
                        short,
                        compared: format!("{who}'s career simulated 400 times with the card {} and 400 times as planned; the gain is the average difference in her five stats at the end.", if lb.is_some() { "at MLB" } else { "in her deck" }, who = super::short_name(&self.kb, goal.runner)),
                        why: format!("Her simulated career ends with about {} more stats, most in {}.", crate::lineage_kit::thousands(gain), super::STATS[best]),
                        gain,
                        unit: Unit::Stats,
                        measured: "400 careers simulated".into(),
                        ..Default::default()
                    });
                }
            }
        }
        self.planner.plan = saved;
        if let Some(l) = self.parent_lever(goal, inv, now, &mut tried) {
            out.push(l);
        }
        (rank(out), tried)
    }
}

impl App {
    /// The plan for a runner's own career toward her goal: her goal's surface, distance and style,
    /// the decks and skills the planner would pick. The cards are measured against it.
    pub(crate) fn runner_own_plan(&mut self, goal: &Goal) -> Plan {
        use crate::knowledge::{Distance, Style, Surface};
        let mut plan = Plan { trainee: Some(goal.runner), friend_lb: 4, ..Plan::default() };
        for t in &goal.targets {
            match *t {
                0 | 1 => plan.surface = Surface::ALL[*t],
                2..=5 => plan.distance = Distance::ALL[*t - 2],
                6..=9 => plan.style = [Style::Front, Style::Pace, Style::Late, Style::End][*t - 6],
                _ => {}
            }
        }
        plan.scenario = self.kb.current_scenario(&crate::ui_util::today()).unwrap_or_default();
        plan.target = [1200, 900, 1000, 600, 1000];
        let saved = std::mem::replace(&mut self.planner.plan, plan);
        self.recommend_deck(true);
        self.auto_pick_skills();
        std::mem::replace(&mut self.planner.plan, saved)
    }

    /// One runner's ceiling, as a worker makes it: her own career's cards and a better parent.
    pub(crate) fn runner_plan_ceiling(&mut self, plan: &super::track::ParentPlan) -> (Vec<Lever>, Tried) {
        let inv = plan.inventory(&self.kb, &self.parent_inventory());
        let now = super::jobs::propose(&self.kb, &plan.goal, &inv);
        let own = self.runner_own_plan(&plan.goal);
        self.runner_ceiling(&plan.goal, &own, &inv, &now)
    }
}

/// Strongest first within each unit, parents and points before lengths and stats. A card that
/// helps several runners is one row: its best gain, and who else it helps.
fn rank(v: Vec<Lever>) -> Vec<Lever> {
    let mut merged: Vec<(Lever, Vec<u32>)> = Vec::new();
    for l in v {
        match merged.iter_mut().find(|(m, _)| l.card.is_some() && m.card == l.card && m.kind == l.kind) {
            Some((m, others)) => {
                let (best, other) = if l.gain > m.gain { (l, m.uma) } else { (m.clone(), l.uma) };
                if let Some(o) = other.filter(|o| Some(*o) != best.uma && !others.contains(o)) {
                    others.push(o);
                }
                *m = best;
            }
            None => merged.push((l, Vec::new())),
        }
    }
    let mut v: Vec<Lever> = merged
        .into_iter()
        .map(|(mut l, others)| {
            if !others.is_empty() {
                l.helps = others;
            }
            l
        })
        .collect();
    let order = |u: Unit| match u {
        Unit::Points => 0,
        Unit::Careers => 1,
        Unit::Lengths => 2,
        Unit::Stats => 3,
    };
    v.sort_by(|a, b| order(a.unit).cmp(&order(b.unit)).then(b.gain.total_cmp(&a.gain)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(uma: u32, card: u32, gain: f64) -> Lever {
        Lever { kind: LeverKind::GetCard, uma: Some(uma), card: Some(card), gain, unit: Unit::Points, ..Default::default() }
    }

    #[test]
    fn gains_read_in_their_units() {
        assert_eq!(Lever { gain: 8400.0, ..Default::default() }.gain_text(), "+8,400 pts a round");
        assert_eq!(Lever { gain: 2.43, unit: Unit::Lengths, ..Default::default() }.gain_text(), "+2.4 lengths a race");
        assert_eq!(Lever { gain: 1.0, unit: Unit::Careers, ..Default::default() }.gain_text(), "1 fewer career");
        assert_eq!(Lever { gain: 140.0, unit: Unit::Stats, ..Default::default() }.gain_text(), "+140 stats");
    }

    /// One card that helps several runners is one row: its best gain, and who else it helps.
    #[test]
    fn a_card_for_several_runners_is_one_row() {
        let v = rank(vec![card(1, 30001, 1200.0), card(2, 30001, 3000.0), card(3, 30002, 2000.0), card(1, 30001, 900.0)]);
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].card, v[0].uma, v[0].gain), (Some(30001), Some(2), 3000.0));
        assert_eq!(v[0].helps, vec![1]);
        assert_eq!(v[1].card, Some(30002));
    }

    /// One runner's ceiling on a real account:
    /// `UMALORD_ROOT=<account folder> cargo test --release runner_ceiling_real_account -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn runner_ceiling_real_account() {
        let root = std::path::PathBuf::from(std::env::var("UMALORD_ROOT").expect("UMALORD_ROOT"));
        let mut app = crate::agent::mcp::headless_app(Some(root));
        // Agnes Digital: Dirt, Mile, End Closer.
        let goal = Goal { runner: 101901, targets: vec![1, 3, 9], label: "One runner: Agnes Digital".into(), ..Default::default() };
        let inv = app.parent_inventory();
        let p = super::super::jobs::propose(&app.kb, &goal, &inv);
        let plan = super::super::track::ParentPlan::new(goal, &p, &app.veterans(), "2026-10-28");
        let t = std::time::Instant::now();
        let (v, _) = app.runner_plan_ceiling(&plan);
        eprintln!("{} levers in {:.1} s", v.len(), t.elapsed().as_secs_f64());
        for l in &v {
            eprintln!("  {} · {} · {} [{}]", l.gain_text(), l.title, l.why, l.measured);
        }
        assert!(v.iter().all(|l| l.gain > 0.0));
    }
}
