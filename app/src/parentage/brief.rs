//! "What's my next career?": everything the player needs to run it, in one place. For a parent
//! step: the trainee, her two parents, the scenario, a deck with a borrowed MLB friend card, the
//! skills to buy (for rank, which sets spark stars), the stats to aim for, the G1s to win, and
//! what makes her a keeper. For a runner (a CM or Team Trials team's): her deck and skills, the
//! stats a viable run reaches (stamina is a hard floor), and when racing helps or hurts.
//!
//! Race advice follows `knowledge/mechanics.md` and `knowledge/modes.md`: a G1 her parents also
//! won adds 3 affinity per parent; a unique levels up at Senior-year fan counts (Turf 60,000 by
//! Valentine's, 70,000 by Fan Appreciation, 120,000 by Christmas; Dirt 40,000 / 60,000 / 80,000);
//! a third race in a row risks her mood and a fourth costs stats (umareference, Optional Races,
//! 2026-08-26; Game8, Unique Skill Level, read 2026-10-28).

use super::jobs::Run;
use super::{Goal, Inventory, STATS, uma_name};
use crate::App;
use crate::knowledge::{Knowledge, Style, grade_rank};
use crate::tab_planner::{Plan, Purpose};

/// One career, ready to run.
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct Brief {
    /// What it's for, in a line.
    pub why: String,
    pub trainee: u32,
    pub style: Style,
    pub scenario: String,
    /// Her two parents, in words.
    pub parents: String,
    pub deck: Vec<u32>,
    pub friend: Option<u32>,
    pub skills: Vec<u32>,
    /// The stats to train toward.
    pub targets: [u32; 5],
    /// What a viable run must reach (stamina for her race), or what only shapes the odds.
    pub stat_notes: Vec<String>,
    pub races: Vec<String>,
    /// Keep her if…
    pub keep: Vec<String>,
    /// Independent Training (the 50-minute background career) or a manual career, and why.
    pub mode: String,
    /// What to set before an Independent Training run: focus, up to 10 prioritized skills, agenda.
    pub setup: Vec<String>,
    /// The Independent Training agenda: each race, her distance/surface grades ("A/B") and her
    /// win chance in percent before any streak penalty.
    pub agenda: Vec<(String, String, u32)>,
    /// A runner's own Independent Training runs the player marked: (reached the stamina floor,
    /// marked). None until there's one; no guess stands in for it.
    pub it_floor: Option<(u32, u32)>,
    /// For the page: the step's must-haves and nice-to-haves as sparks, her two legacies (id and
    /// outfit; none for a starter step or a runner), one career's chance of the must-haves, and
    /// Independent Training's training focus.
    pub must: Vec<super::jobs::Want>,
    pub nice: Vec<super::jobs::Want>,
    pub legacies: Vec<(u64, u32)>,
    pub p: Option<f64>,
    pub focus: Option<String>,
    /// A parent step's number (from 1) and the runner it's for.
    pub step: Option<usize>,
    pub for_runner: Option<u32>,
    /// The runner this career's uma would replace on the team, if any.
    pub replaces: Option<u32>,
    /// Why the borrowed card, and the next best borrows if no friend has it MLB (card, why).
    pub friend_why: Option<String>,
    pub friend_alts: Vec<(u32, String)>,
    /// A runner's own career: the Stamina she must reach (below it she runs dry).
    #[serde(default)]
    pub stat_floor: Option<u32>,
    /// A parent career: the stats (0 Speed .. 4 Wit) worth 1100 for her blue spark's stars.
    #[serde(default)]
    pub stat_aims: Vec<usize>,
    /// Team Trials: how she gets the community's inherited unique (564 Escapades) when her
    /// parents above don't carry it.
    #[serde(default)]
    pub inherit: Option<String>,
    /// A runner's own career: her career objectives (when, race, goal), the race she must run.
    #[serde(default)]
    pub objectives: Vec<(String, String, String)>,
    /// Extra G1s that fit her for fans (name, "1,600 m turf", her distance/surface grades),
    /// the plan's distance first.
    #[serde(default)]
    pub extra_races: Vec<(String, String, String)>,
    /// A runner's career plan as the planner made it (course, targets, deck, skills), for the
    /// skill shop helper at the end of her career.
    #[serde(default)]
    pub plan: Option<Plan>,
    /// A parent to borrow from a friend in place of one of `legacies` (outfit): Team Trials'
    /// inherited unique. `legacies` then holds the player's own parent to keep beside her.
    #[serde(default)]
    pub borrow_parent: Option<u32>,
}

/// Independent Training's win chance for a race, in percent (uma.guide, Independent Training):
/// 110 at A or better in both distance and surface, 10 less for each grade below A in either,
/// less the streak penalty (a third race in a row 10, a fourth 25, a fifth 35, six or more 50).
/// Grades are `grade_rank`'s (A = 7). An A/A race absorbs a three-race streak.
pub fn it_win(distance: u8, surface: u8, in_a_row: u32) -> u32 {
    let below = |r: u8| u32::from(7u8.saturating_sub(r.min(7)));
    let streak = match in_a_row {
        0..=2 => 0,
        3 => 10,
        4 => 25,
        5 => 35,
        _ => 50,
    };
    110u32.saturating_sub(10 * (below(distance) + below(surface)) + streak).min(100)
}

/// Independent Training's training focus for a career's target stamina: Stamina for long
/// races, Sprint for short ones, Balanced between (uma.guide, Independent Training).
fn focus(stamina: u32) -> &'static str {
    if stamina >= 900 {
        "Stamina"
    } else if stamina <= 650 {
        "Sprint"
    } else {
        "Balanced"
    }
}

/// The unique's fan counts for her, by whether she's a Dirt uma (higher Dirt grade than Turf).
fn fan_line(kb: &Knowledge, trainee: u32, scenario: &str) -> String {
    let dirt = kb.umas.get(&trainee).is_some_and(|u| grade_rank(&u.aptitude.dirt) > grade_rank(&u.aptitude.turf));
    if scenario == "Trackblazer" {
        return "Trackblazer levels her unique at each year's end with the Director's bond at 15% / 30% / 50% and 5,000 / 60,000 / 120,000 fans: her scheduled and rival races get most of it.".into();
    }
    let (a, b, c) = if dirt { ("40,000", "60,000", "80,000") } else { ("60,000", "70,000", "120,000") };
    let bond = if scenario == "Unity Cup" { "" } else { " (with the Director's bond green, 60%)" };
    format!("Her unique levels up in the Senior year at {a} fans by Valentine's, {b} by Fan Appreciation{bond} and {c} by Christmas: run extra races only to reach those, on turns without a strong friendship training.")
}

/// A career objective in words: when, the race (its proper name, a debut's track, or a fan
/// count), and the goal.
fn objective(kb: &Knowledge, o: &crate::knowledge::Objective) -> (String, String, String) {
    let cap = |s: &str| {
        let mut c = s.chars();
        c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
    };
    let months = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];
    let when = cap(&o.when.split(' ').map(|w| if months.contains(&w) { cap(w) } else { w.to_string() }).collect::<Vec<_>>().join(" "));
    if o.kind == "Fan_Count" || o.race.chars().all(|c| c.is_ascii_digit()) {
        let fans = o.race.parse::<f64>().map_or(o.race.clone(), crate::lineage_kit::thousands);
        return (when, format!("Reach {fans} fans"), String::new());
    }
    let race = match o.race_id.and_then(|id| kb.races.iter().find(|r| r.id == id)) {
        Some(r) => r.name.clone(),
        None => match o.race.split('-').collect::<Vec<_>>().as_slice() {
            // "hanshin-turf-1200": a debut on that track.
            [track, surface, metres] if metres.chars().all(|c| c.is_ascii_digit()) => format!("A race at {} ({surface} {} m)", cap(track), crate::lineage_kit::thousands(metres.parse().unwrap_or(0.0))),
            _ => o.race.split(' ').map(cap).collect::<Vec<_>>().join(" "),
        },
    };
    let goal = match o.goal.as_str() {
        "Enter" => "run it".to_string(),
        "None" | "" => String::new(),
        g => g.to_lowercase(),
    };
    (when, race, goal)
}

const RACING: &str = "Don't run three races in a row if you can help it (a third risks her mood; a fourth costs stats).";

impl App {
    /// Why a plan's borrowed card, and up to two fallbacks, from the deck ranking's friend slot.
    pub(crate) fn friend_picks(&mut self, plan: &Plan) -> (Option<String>, Vec<(u32, String)>) {
        let saved = std::mem::replace(&mut self.planner.plan, plan.clone());
        let deck = [plan.deck[0], plan.deck[1], plan.deck[2], plan.deck[3], plan.deck[4], plan.friend];
        let picks = crate::recommend::rank_slot(&self.rank_context(), &deck, 5);
        self.planner.plan = saved;
        let why = picks.iter().find(|c| Some(c.id) == plan.friend).map(|c| c.reason.clone()).filter(|r| !r.is_empty());
        let alts = picks.iter().filter(|c| Some(c.id) != plan.friend).take(2).map(|c| (c.id, c.reason.clone())).collect();
        (why, alts)
    }

    /// The skills on a runner's plan that can't help her in its race: tied to another style,
    /// distance, surface or racecourse, or turned off there. Checked from each skill's own
    /// conditions, apart from the pickers that chose them.
    pub(crate) fn runner_skill_misfits(&mut self, plan: &Plan, disabled: &[u32]) -> Vec<String> {
        let saved = std::mem::replace(&mut self.planner.plan, plan.clone());
        let track = self.plan_race_setup().course.and_then(|c| self.sim_data.as_ref()?.courses.get(&c).map(|c| c.race_track_id));
        self.planner.plan = saved;
        plan.skills
            .iter()
            .filter_map(|id| {
                let k = self.kb.skills.get(id)?;
                let t = k.tags();
                let why = if disabled.contains(id) {
                    "turned off in this CM"
                } else if !t.fits(Some(plan.style), Some(plan.distance), Some(plan.surface)) {
                    "tied to another style, distance or surface"
                } else if !t.tracks.is_empty() && track.is_some_and(|tr| !t.tracks.contains(&tr)) {
                    "tied to another racecourse"
                } else {
                    return None;
                };
                Some(format!("{} for {} ({} {} {}): {why}", k.name, uma_name(&self.kb, plan.trainee.unwrap_or(0)), plan.surface.label(), plan.distance.label(), plan.style.label()))
            })
            .collect()
    }

    /// Whether a parent's skill is worth having as the runner's white spark: not a debuff, and
    /// not tied to a style, distance or surface her goal isn't, or to one racecourse.
    pub(crate) fn white_fits(&self, id: u32, style: Option<Style>, distance: Option<crate::knowledge::Distance>, surface: Option<crate::knowledge::Surface>) -> bool {
        let Some(k) = self.kb.skills.get(&id) else { return false };
        let t = k.tags();
        !self.is_debuff(id) && t.tracks.is_empty() && t.fits(style, distance, surface)
    }

    /// The brief for a parent step of a goal's plan.
    pub(crate) fn parent_brief(&mut self, goal: &Goal, run: &Run, inv: &Inventory, step: usize) -> Brief {
        let kb = &self.kb;
        let style = kb.umas.get(&run.trainee).map(|u| u.aptitude.best_style()).unwrap_or_default();
        let parents = if run.legacies == [0, 0] {
            "any two legacies (yours or a friend's), best without pinks that raise her other aptitudes to A".to_string()
        } else {
            run.legacies.iter().map(|id| inv.by_id(*id).map_or(format!("#{id}"), |p| super::parent_name(kb, p))).collect::<Vec<_>>().join(" and ")
        };
        // Parents: rank decides green and white stars, so skills are bought for rating; a
        // wanted blue stat should finish at 1100 or more.
        let mut targets = [1200, 900, 1000, 500, 900];
        for i in &run.stat_aims {
            targets[*i] = targets[*i].max(1100);
        }
        // The runner's goal as the skill filter sees it: targets 0-1 surfaces, 2-5 distances, 6-9 styles.
        let goal_surface = goal.targets.iter().find(|t| **t < 2).map(|t| crate::knowledge::Surface::ALL[*t]);
        let goal_distance = goal.targets.iter().find(|t| (2..6).contains(*t)).map(|t| crate::knowledge::Distance::ALL[*t - 2]);
        let goal_style = goal.targets.iter().find(|t| (6..10).contains(*t)).map(|t| Style::ALL[*t - 6]);
        let saved = std::mem::take(&mut self.planner.plan);
        self.planner.plan = Plan {
            purpose: Purpose::Parent,
            trainee: Some(run.trainee),
            style,
            target: targets,
            friend_lb: 4,
            pick_objective: crate::skill_opt::PickObjective::Rating,
            scenario: "Trackblazer".into(),
            // Her skills become white sparks her runner may inherit: no debuffs, and none tied
            // to a style, distance or surface the runner's goal isn't, or to one racecourse.
            banned_skills: self.kb.skills.values().filter(|k| !self.white_fits(k.id, goal_style, goal_distance, goal_surface)).map(|k| k.id).collect(),
            ..Plan::default()
        };
        self.recommend_deck(true);
        self.rating_auto_pick();
        let plan = std::mem::replace(&mut self.planner.plan, saved);
        let (friend_why, friend_alts) = self.friend_picks(&plan);
        let kb = &self.kb;
        let mut races: Vec<String> = run.g1s.iter().map(|(n, c)| format!("Win {n} (+{} affinity)", 3 * u32::from(*c))).collect();
        races.push("Trackblazer is built on racing (30 to 40 races a career): pick the G1s above among them.".into());
        races.push(RACING.into());
        let mut stat_notes = vec!["Stats don't pass or fail a parent run: they set her rank (SS or better makes green and white sparks 2★ or better 80% of the time) and her blue spark's stars.".into()];
        for i in &run.stat_aims {
            stat_notes.push(format!("{} at 1100 or more: her blue spark, if it's {}, comes 2★ or better 80% of the time.", STATS[*i], STATS[*i]));
        }
        let mut keep: Vec<String> = run.must.iter().map(|w| format!("Must have: {}", w.label(kb))).collect();
        if keep.is_empty() {
            keep.push("Any finished career counts: her parents bring what the plan needs.".into());
        }
        keep.extend(run.nice.iter().map(|w| format!("Nice to have: {}", w.label(kb))));
        keep.push("Use the 30 TP spark reroll on a run that's close.".into());
        // Parents: sparks are what count, so Independent Training (Global since 2026-07-22) does
        // the grind; its races are won on distance and surface grades alone.
        let mut setup = vec![
            format!("Training focus: {} (a Stamina aim wants Stamina; otherwise Balanced).", if run.stat_aims.contains(&1) { "Stamina" } else { "Balanced" }),
            format!("Prioritized skills (up to 10): {}.", plan.skills.iter().take(10).filter_map(|s| kb.skills.get(s).map(|k| k.name.clone())).collect::<Vec<_>>().join(", ")),
        ];
        // Its races are won on her distance and surface grades alone: her own, before her
        // parents' pinks (the cautious reading).
        let agenda: Vec<(String, String, u32)> = run
            .g1s
            .iter()
            .filter_map(|(name, _)| {
                let race = kb.races.iter().find(|x| &x.name == name && x.grade == "G1")?;
                let a = &kb.umas.get(&run.trainee)?.aptitude;
                let surface = if race.surface == "Dirt" { crate::knowledge::Surface::Dirt } else { crate::knowledge::Surface::Turf };
                let (d, sf) = (a.distance(crate::knowledge::Distance::of_metres(race.distance)), a.surface(surface));
                Some((name.clone(), format!("{d}/{sf}"), it_win(grade_rank(d), grade_rank(sf), 0)))
            })
            .collect();
        if !agenda.is_empty() {
            setup.push(format!(
                "Agenda: {}. Her distance/surface grades set the win chance: an A/A race starts at 110%, so a third race in a row (−10) still wins; a fourth costs 25, a fifth 35, a sixth on 50.",
                agenda.iter().map(|(n, g, c)| format!("{n} ({g}, ~{c}%)")).collect::<Vec<_>>().join(", ")
            ));
        }
        setup.push("You still buy the skills at the end, and the spark reroll works as in a manual career.".into());
        Brief {
            mode: "Independent Training".into(),
            setup,
            agenda,
            it_floor: None,
            stat_floor: None,
            stat_aims: run.stat_aims.clone(),
            must: run.must.clone(),
            nice: run.nice.clone(),
            legacies: if run.legacies == [0, 0] { Vec::new() } else { run.legacies.iter().zip(run.legacy_umas).map(|(id, uma)| (*id, if uma == 0 { inv.by_id(*id).map_or(0, |p| p.uma) } else { uma })).collect() },
            p: Some(run.p),
            focus: Some(if run.stat_aims.contains(&1) { "Stamina" } else { "Balanced" }.to_string()),
            step: Some(step + 1),
            for_runner: Some(goal.runner),
            replaces: None,
            friend_why,
            friend_alts,
            inherit: None,
            objectives: Vec::new(),
            extra_races: Vec::new(),
            plan: None,
            borrow_parent: None,
            why: format!("Step {} of the parent plan for {}.", step + 1, uma_name(kb, goal.runner)),
            trainee: run.trainee,
            style,
            scenario: plan.scenario.clone(),
            parents,
            deck: plan.deck.iter().flatten().copied().collect(),
            friend: plan.friend,
            skills: plan.skills.clone(),
            targets: plan.target,
            stat_notes,
            races,
            keep,
        }
    }

    /// The brief for a runner's own career (a CM or Team Trials team's), from her plan.
    pub(crate) fn runner_brief(&self, why: String, plan: &Plan, stamina_floor: Option<(u32, String)>, parents: String) -> Brief {
        let kb = &self.kb;
        let trainee = plan.trainee.unwrap_or_default();
        let mut stat_notes = Vec::new();
        if let Some((floor, race)) = &stamina_floor {
            stat_notes.push(format!("Stamina {floor} or more is the floor for {race}: below it she runs dry on longer tracks and loses placement and skill points."));
        }
        stat_notes.push("Train Speed first, then Stamina to its floor, then Power and Wit (Wit makes skills fire and keeps her calm).".into());
        // Her career objectives, and the G1s that fit her (A or better in distance and surface) for
        // fans: the plan's own distance first, then the others.
        let info = kb.umas.get(&trainee);
        let objectives: Vec<(String, String, String)> = info.map(|u| u.objectives.iter().map(|o| objective(kb, o)).collect()).unwrap_or_default();
        let mut extra: Vec<(bool, String, String, String)> = Vec::new();
        if let Some(u) = info {
            let a = &u.aptitude;
            // Career races only: ids under 5000 (above are events, scenario finals and matches).
            for r in kb.races.iter().filter(|r| r.grade == "G1" && r.id < 5000) {
                if objectives.iter().any(|o| o.1 == r.name) || extra.iter().any(|e| e.1 == r.name) {
                    continue;
                }
                let surface = if r.surface == "Dirt" { crate::knowledge::Surface::Dirt } else { crate::knowledge::Surface::Turf };
                let distance = crate::knowledge::Distance::of_metres(r.distance);
                let (d, sf) = (a.distance(distance), a.surface(surface));
                if grade_rank(d) >= 7 && grade_rank(sf) >= 7 {
                    extra.push((distance == plan.distance && surface == plan.surface, r.name.clone(), format!("{} m {}", crate::lineage_kit::thousands(f64::from(r.distance)), r.surface.to_lowercase()), format!("{d}/{sf}")));
                }
            }
        }
        extra.sort_by_key(|e| !e.0);
        let extra_races: Vec<(String, String, String)> = extra.into_iter().take(8).map(|e| (e.1, e.2, e.3)).collect();
        let mut races = vec![fan_line(kb, trainee, &plan.scenario)];
        if plan.scenario == "Grand Concert" {
            races.push("Grand Concert's races give no Performance Points: run her objective races and the fan races above, and train the rest.".into());
        }
        races.push(RACING.into());
        let keep = vec![
            format!("Keep her if she reaches the stats above{} and learns the skills listed.", stamina_floor.as_ref().map_or(String::new(), |(f, _)| format!(" (Stamina {f} at the least)"))),
            "Skills you can't afford by the end: the ones listed first matter most.".into(),
        ];
        // Runners race on their stats and skills: a manual career (Cygames: manual runs are
        // generally much stronger than Independent Training).
        let setup = vec![format!("If you grind her with Independent Training anyway: focus {}, prioritize the first ten skills above, and keep only a run that reaches the stamina floor.", focus(plan.target[1]))];
        let it_floor = stamina_floor.as_ref().zip(plan.trainee).zip(self.root.as_ref()).and_then(|(((f, _), t), root)| super::modes::load(root).floor_rate(&self.veterans(), t, *f));
        Brief {
            agenda: Vec::new(),
            it_floor,
            stat_floor: stamina_floor.as_ref().map(|(f, _)| *f),
            stat_aims: Vec::new(),
            must: Vec::new(),
            nice: Vec::new(),
            legacies: Vec::new(),
            p: None,
            focus: Some(focus(plan.target[1]).to_string()),
            step: None,
            for_runner: None,
            replaces: None,
            friend_why: None,
            friend_alts: Vec::new(),
            inherit: None,
            objectives,
            extra_races,
            plan: Some(plan.clone()),
            borrow_parent: None,
            mode: "Manual career".into(),
            setup,
            why,
            trainee,
            style: plan.style,
            scenario: if plan.scenario.is_empty() { "the scenario of your choice".into() } else { plan.scenario.clone() },
            parents,
            deck: plan.deck.iter().flatten().copied().collect(),
            friend: plan.friend,
            skills: plan.skills.clone(),
            targets: plan.target,
            stat_notes,
            races,
            keep,
        }
    }
}

/// The brief as text: "What to do next".
/// A lineage's two legacies as (finished uma id, outfit), for the page's avatar chips.
pub(crate) fn legacies_of(l: Option<&super::score::Lineage>, inv: &Inventory) -> Vec<(u64, u32)> {
    l.map(|l| l.legacies.iter().filter_map(|i| inv.parents.get(*i)).map(|p| (p.id, p.uma)).collect()).unwrap_or_default()
}

pub fn text(kb: &Knowledge, b: &Brief) -> String {
    let deck: Vec<String> = b.deck.iter().filter_map(|c| kb.cards.get(c).map(|c| format!("{} [{}]", c.uma, c.title))).collect();
    let friend = b.friend.and_then(|c| kb.cards.get(&c)).map_or("none".into(), |c| format!("{} [{}], fully limit broken (MLB)", c.uma, c.title));
    let skills: Vec<String> = b.skills.iter().filter_map(|s| kb.skills.get(s).map(|k| k.name.clone())).collect();
    let article = if b.style == Style::End { "an" } else { "a" };
    let mut out = format!("Next career: {} as {article} {}. {}\n", uma_name(kb, b.trainee), b.style.label(), b.why);
    out += &format!("- How: {}{}\n", b.mode, if b.mode == "Manual career" { " (a runner races on her stats and skills; manual careers are generally much stronger)" } else { " (50 minutes in the background; sparks are what this career is for)" });
    out += &format!("- Parents: {}.\n- Scenario: {}.\n", b.parents, b.scenario);
    out += &format!("- Deck: {}; borrow {friend}.\n", deck.join(", "));
    out += &format!("- Skills to target: {}.\n", if skills.is_empty() { "none picked yet".to_string() } else { skills.join(", ") });
    out += &format!("- Stats: aim for {}. {}\n", b.targets.iter().enumerate().map(|(i, v)| format!("{} {v}", STATS[i])).collect::<Vec<_>>().join(", "), b.stat_notes.join(" "));
    out += &format!("- Races: {}\n", b.races.join(" "));
    out += &format!("- Keep: {}\n", b.keep.join(" "));
    out += &format!("- Set up: {}\n", b.setup.join(" "));
    if let Some((reached, runs)) = b.it_floor {
        out += &format!("- Your Independent Training runs of her: {reached} of {runs} reached the stamina floor.\n");
    }
    out
}

impl App {
    /// A CM team's next career: the queue's first parent step, else the ace's own career (the
    /// runner who wins most head to head).
    pub(crate) fn cm_next(&mut self, team: &super::team::CmTeam, inv: &Inventory) -> Option<Brief> {
        if let Some((i, run)) = team.queue.first() {
            let r = &team.runners[*i];
            let step = r.parents.steps.iter().position(|s| s == run).unwrap_or(0);
            return Some(self.parent_brief(&r.goal, run, inv, step));
        }
        let ace = team.runners.iter().max_by(|a, b| a.sim.map_or(0.0, |s| s.0).total_cmp(&b.sim.map_or(0.0, |s| s.0)))?;
        let parents = ace.parents.now.as_ref().map_or("her best two finished umas".into(), |l| l.legacies.iter().map(|i| super::parent_name(&self.kb, &inv.parents[*i])).collect::<Vec<_>>().join(" and "));
        let mut b = self.runner_brief(format!("Her own career for CM {} ({}): she wins most head to head, so she's the ace.", team.event.id, team.event.name), &ace.plan, Some((ace.plan.target[1], team.event.course_name.clone())), parents);
        b.legacies = legacies_of(ace.parents.now.as_ref(), inv);
        (b.friend_why, b.friend_alts) = self.friend_picks(&ace.plan);
        Some(b)
    }

    /// A Team Trials team's next career: the weakest runner's first parent step, else her own
    /// career.
    pub(crate) fn tt_next(&mut self, team: &super::tt::TtTeam, inv: &Inventory) -> Option<Brief> {
        let (row, s) = team.rows.iter().flat_map(|r| r.slots.iter().map(move |s| (r, s))).min_by(|a, b| a.1.total.total_cmp(&b.1.total))?;
        // An unreliable unique at the bottom: brief the community's steadier runner for that race
        // that the player owns and the team doesn't use yet.
        let meta = self.tt_meta();
        if meta.role(s.trainee).is_some_and(|r| r.unique.as_deref() == Some("inconsistent")) {
            // Everyone else on the team (another outfit of the replaced uma may take her place).
            let on_team: Vec<u32> = team.rows.iter().flat_map(|r| r.slots.iter().filter(|x| x.trainee != s.trainee).map(|x| crate::affinity::chara(x.trainee))).collect();
            let (surface, distance) = super::tt::race_grades(row.name);
            let fit = |id: u32| self.kb.umas.get(&id).map(|u| (grade_rank(u.aptitude.surface(surface)), grade_rank(u.aptitude.distance(distance))));
            // Owned, steady, B or better in the race; the meta's order, then natural A grades.
            let mut alts: Vec<&super::tt::Role> = meta
                .roles
                .list
                .iter()
                .filter(|r| r.id != s.trainee && r.races.iter().any(|x| x == row.name) && r.unique.is_none() && !on_team.contains(&crate::affinity::chara(r.id)) && self.roster.umas.iter().any(|u| u.id == r.id))
                .filter(|r| fit(r.id).is_some_and(|(gs, gd)| gs >= 6 && gd >= 6))
                .collect();
            alts.sort_by_key(|r| (r.races.iter().position(|x| x == row.name), fit(r.id).map(|(gs, gd)| std::cmp::Reverse(gs.min(7) + gd.min(7)))));
            if let Some(r) = alts.first() {
                let a = self.kb.umas.get(&r.id).map(|u| u.aptitude.clone()).unwrap_or_default();
                // Her preferred style, one the race's other runners don't use if she can.
                let used: Vec<Style> = row.slots.iter().filter(|x| x.trainee != s.trainee).map(|x| x.style).collect();
                let styles: Vec<Style> = r.styles.iter().filter_map(|x| super::tt::parse_style(x)).filter(|st| grade_rank(a.style(*st)) >= 6).collect();
                let style = styles.iter().find(|st| !used.contains(st)).or(styles.first()).copied().unwrap_or_else(|| a.best_style());
                let plan = self.tt_runner_plan(&meta, row.name, r.id, style);
                let replaced = uma_name(&self.kb, s.trainee);
                let goal = super::tt::Slot { trainee: r.id, style, ..s.clone() }.parents_goal(row.name);
                let best = super::score::rank(&self.kb, &goal, &inv.parents, 1).into_iter().next();
                let parents = best
                    .as_ref()
                    .map(|l| {
                        let names = l.legacies.iter().map(|i| super::parent_name(&self.kb, &inv.parents[*i])).collect::<Vec<_>>().join(" and ");
                        if l.meets() { names } else { format!("{names} (her best pair, though short of A somewhere: see her lineage plan)") }
                    })
                    .unwrap_or_else(|| "any two legacies (yours or a friend's)".into());
                let mut b = self.runner_brief(format!("Her own career for Team Trials ({}), to replace {replaced}, whose unique fires unreliably.", row.name), &plan, Some((plan.target[1], row.name.to_string())), parents);
                b.legacies = legacies_of(best.as_ref(), inv);
                b.replaces = Some(s.trainee);
                (b.friend_why, b.friend_alts) = self.friend_picks(&plan);
                return Some(b);
            }
        }
        // Parents first only when her finished umas don't yet start her race's grades at A.
        if let Some(run) = s.parents.steps.first().filter(|_| !s.ready) {
            return Some(self.parent_brief(&s.parents_goal(row.name), run, inv, 0));
        }
        let parents = s.parents.now.as_ref().map_or("her best two finished umas".into(), |l| l.legacies.iter().map(|i| super::parent_name(&self.kb, &inv.parents[*i])).collect::<Vec<_>>().join(" and "));
        let mut b = self.runner_brief(format!("Her own career for Team Trials ({}): she scores lowest on the team, so a better run of her raises it most.", row.name), &s.plan, Some((s.plan.target[1], row.name.to_string())), parents);
        b.legacies = legacies_of(s.parents.now.as_ref(), inv);
        (b.friend_why, b.friend_alts) = self.friend_picks(&s.plan);
        Some(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every skill a parent's brief recommends (her runner's future white sparks) must fit the
    /// runner's goal; runners' plans are checked in the team suites (`runner_skill_misfits`):
    /// `UMALORD_ROOT=<account> cargo test --release briefs_fit_their_runners -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn briefs_fit_their_runners() {
        let root = std::path::PathBuf::from(std::env::var("UMALORD_ROOT").expect("UMALORD_ROOT"));
        let mut a = App::new(Some(root));
        let inv = a.parent_inventory();
        let runners: Vec<u32> = a.roster.umas.iter().map(|u| u.id).collect();
        let (mut briefs, mut skills, mut bad) = (0, 0, Vec::new());
        // Every surface × distance × style goal that needs parent careers, each distinct step
        // once, up to BRIEFS (default 400) briefs.
        let cap: usize = std::env::var("BRIEFS").ok().and_then(|v| v.parse().ok()).unwrap_or(400);
        let mut seen = std::collections::BTreeSet::new();
        'all: for r in runners {
            for surface in crate::knowledge::Surface::ALL {
                for distance in crate::knowledge::Distance::ALL {
                    for style in Style::ALL {
                        let goal = Goal { runner: r, targets: vec![usize::from(surface == crate::knowledge::Surface::Dirt), 2 + distance as usize, 6 + style as usize], ..Default::default() };
                        let p = super::super::jobs::propose(&a.kb, &goal, &inv);
                        for (i, run) in p.steps.iter().enumerate() {
                            if !seen.insert((run.trainee, run.legacies, surface as u8, distance as u8, style as u8)) {
                                continue;
                            }
                            let b = a.parent_brief(&goal, run, &inv, i);
                            briefs += 1;
                            for id in &b.skills {
                                skills += 1;
                                let Some(k) = a.kb.skills.get(id) else { continue };
                                let t = k.tags();
                                if a.is_debuff(*id) || !t.tracks.is_empty() || !t.fits(Some(style), Some(distance), Some(surface)) {
                                    bad.push(format!("{} in {}'s career for {} ({} {} {})", k.name, uma_name(&a.kb, run.trainee), uma_name(&a.kb, r), surface.label(), distance.label(), style.label()));
                                }
                            }
                            if briefs >= cap {
                                break 'all;
                            }
                        }
                    }
                }
            }
        }
        eprintln!("{briefs} parent briefs, {skills} skills checked, {} misfits", bad.len());
        for m in bad.iter().take(30) {
            eprintln!("  {m}");
        }
        assert!(bad.is_empty());
    }

    #[test]
    fn independent_training_wins_on_grades() {
        // uma.guide's table: A/A 110 (shown as 100), A/B 100, B/B 90, B/C 80; the streak after.
        assert_eq!(it_win(7, 7, 0), 100);
        assert_eq!(it_win(8, 7, 3), 100, "an A/A race absorbs a third race in a row");
        assert_eq!(it_win(7, 7, 4), 85);
        assert_eq!(it_win(7, 6, 0), 100);
        assert_eq!(it_win(6, 6, 0), 90);
        assert_eq!(it_win(6, 5, 3), 70);
        assert_eq!(it_win(1, 1, 6), 0);
    }

    fn skill(a: &App, name: &str) -> u32 {
        a.kb.skills.values().find(|k| k.name == name).unwrap_or_else(|| panic!("no skill {name}")).id
    }

    #[test]
    fn white_sparks_must_fit_the_runner() {
        use crate::knowledge::{Distance, Surface};
        let a = App::for_tests();
        let end = (Some(Style::End), Some(Distance::Mile), Some(Surface::Dirt));
        let fits = |n: &str| a.white_fits(skill(&a, n), end.0, end.1, end.2);
        // The Gold Ship case: Front Runner and Pace Chaser skills for an End Closer.
        assert!(!fits("Front Runner Straightaways ○"));
        assert!(!fits("Pace Chaser Savvy ○"));
        assert!(!fits("Long Corners ○"), "another distance");
        assert!(!fits("Hanshin Racecourse ○"), "one racecourse");
        assert!(!fits("Trick (Front)"), "a debuff");
        assert!(fits("End Closer Savvy ○"));
        assert!(fits("Corner Recovery ○"), "a skill for any race");
    }

    /// Every parent brief, for every style, distance and surface a runner could want, recommends
    /// only skills that runner can use (her future white sparks). On the bundled sample roster,
    /// so plain `cargo test` covers it; `briefs_fit_their_runners` does the same on real accounts.
    #[test]
    fn parent_careers_skip_skills_the_runner_cant_use() {
        let mut a = App::for_tests();
        let trainee = a.roster.umas[0].id;
        let runner = a.roster.umas.iter().map(|u| u.id).find(|id| crate::affinity::chara(*id) != crate::affinity::chara(trainee)).unwrap();
        let inv = a.parent_inventory();
        let run = Run { trainee, legacies: [0, 0], ..Default::default() };
        let mut checked = 0;
        for surface in crate::knowledge::Surface::ALL {
            for distance in crate::knowledge::Distance::ALL {
                for style in Style::ALL {
                    let goal = Goal { runner, targets: vec![usize::from(surface == crate::knowledge::Surface::Dirt), 2 + distance as usize, 6 + style as usize], ..Default::default() };
                    let b = a.parent_brief(&goal, &run, &inv, 0);
                    assert!(!b.skills.is_empty(), "skills to target for {surface:?} {distance:?} {style:?}");
                    for id in &b.skills {
                        let k = &a.kb.skills[id];
                        let t = k.tags();
                        assert!(!a.is_debuff(*id) && t.tracks.is_empty() && t.fits(Some(style), Some(distance), Some(surface)), "{} for a {surface:?} {distance:?} {style:?} runner", k.name);
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 100, "{checked} skills checked");
    }

    #[test]
    fn runner_misfits_are_named() {
        use crate::knowledge::{Distance, Surface};
        let mut a = App::for_tests();
        let plan = Plan {
            trainee: Some(a.roster.umas[0].id),
            style: Style::End,
            distance: Distance::Long,
            surface: Surface::Turf,
            skills: vec![skill(&a, "Front Runner Straightaways ○"), skill(&a, "Mile Corners ○"), skill(&a, "Corner Recovery ○"), skill(&a, "Long Corners ○")],
            ..Plan::default()
        };
        let off = skill(&a, "Long Corners ○");
        let m = a.runner_skill_misfits(&plan, &[off]);
        assert_eq!(m.len(), 3, "{m:?}");
        assert!(m[0].contains("Front Runner") && m[1].contains("Mile Corners") && m[2].contains("turned off"), "{m:?}");
    }
}
