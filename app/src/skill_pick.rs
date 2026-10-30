//! Planner Step 5, skills to buy: the rows, the auto-pick optimiser's background job
//! (`skill_opt.rs`), skill shop mode, and Step 2's aptitude warning.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::time::Instant;

use eframe::egui::{self, CornerRadius, RichText, Stroke, vec2};
use egui_extras::Column;

use crate::App;
use crate::ui_util::UiExt;
use crate::knowledge::grade_rank;
use crate::sim::run::Progress;
use crate::skill_opt::{self, Candidate, Input, MetaInfluence, Outcome, Phase, PickObjective, Tune};
use crate::tab_planner::Fit;
use crate::theme::{pal, size};
use crate::ui_util::{DataTable, SortKey, TableCol, badge, semibold, sort_rows};

/// Conditions the simulator treats as always met (sim/conditions.rs, `keep(true)`): they depend on
/// where other umas are, so a skill that needs them looks better in the simulator than in a race.
const POSITION_CONDITIONS: [&str; 18] = [
    "is_overtake",
    "near_count",
    "bashin_diff_behind",
    "bashin_diff_infront",
    "blocked_front",
    "blocked_front_continuetime",
    "blocked_side_continuetime",
    "blocked_all_continuetime",
    "is_surrounded",
    "overtake_target_time",
    "overtake_target_no_order_up_time",
    "change_order_onetime",
    "infront_near_lane_time",
    "behind_near_lane_time",
    "is_behind_in",
    "visiblehorse",
    "is_temptation",
    "temptation_count",
];

/// Whether `text` uses the condition `name` (not just a longer name that starts with it).
fn uses_condition(text: &str, name: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(name).any(|(i, m)| {
        let before = text[..i].chars().next_back();
        let after = text[i + m.len()..].chars().next();
        before.is_none_or(|c| !ident(c)) && after.is_none_or(|c| !ident(c))
    })
}

/// The optimiser running in the background.
pub(crate) struct PickJob {
    pub key: u64,
    pub progress: Arc<Progress>,
    pub phase: Arc<Phase>,
    rx: mpsc::Receiver<Outcome>,
    pub started: Instant,
}

impl PickJob {
    pub fn fraction(&self) -> f32 {
        let done = self.progress.done.load(Ordering::Relaxed) as f32;
        let total = self.progress.total.load(Ordering::Relaxed).max(1) as f32;
        (done / total).min(1.0)
    }

    pub fn cancel(&self) {
        self.progress.cancel.store(true, Ordering::Relaxed);
    }
}

/// What every row needs, worked out once per frame.
pub(crate) struct PickCtx {
    /// Skills on offer for the plan, with where each comes from.
    pub rows: BTreeMap<u32, Vec<String>>,
    /// Skills only the parents' white sparks offer.
    pub parent_only: Vec<u32>,
    pub disabled: Vec<u32>,
    pub cup: String,
}

impl App {
    /// Debuffs, and skills that hinge on other umas' positions: one simulated uma can't value them.
    pub(crate) fn skill_unsimulated(&self, id: u32) -> Option<&'static str> {
        let k = self.kb.skills.get(&id)?;
        // The simulator's own data says whether a skill hits rivals (uma.guide's category also
        // files a few of the trainee's own skills, like Focus, under Debuff).
        let debuff = self.sim_data.as_ref().filter(|d| d.skills.contains_key(&id)).map_or(k.category == "Debuff", |d| d.is_debuff(id));
        if debuff {
            return Some("A debuff: it slows rivals, which one simulated uma can't show.");
        }
        let text = format!("{} {} {}", k.conditions, k.pre, k.alt.as_ref().map_or("", |a| a.conditions.as_str()));
        POSITION_CONDITIONS
            .iter()
            .any(|c| uses_condition(&text, c))
            .then_some("It depends on where other umas are, which the simulator assumes always works out.")
    }

    /// White skills from the plan's parents' sparks (the rows of account/parents.md the plan
    /// names): skill → "Gold Ship's spark (2★)".
    pub(crate) fn parent_spark_skills(&self) -> BTreeMap<u32, String> {
        let parents = &self.planner.plan.parents;
        if let Some((key, map)) = self.planner.parent_sparks.borrow().as_ref()
            && key == parents
        {
            return map.clone();
        }
        let map = self.read_parent_sparks();
        *self.planner.parent_sparks.borrow_mut() = Some((parents.clone(), map.clone()));
        map
    }

    fn read_parent_sparks(&self) -> BTreeMap<u32, String> {
        let mut out = BTreeMap::new();
        let Some(root) = &self.root else { return out };
        let text = std::fs::read_to_string(root.join("account").join("parents.md")).unwrap_or_default();
        let Some(table) = crate::data::tables(&text).into_iter().next() else { return out };
        let col = |prefix: &str| table.headers.iter().position(|h| h.starts_with(prefix));
        let (Some(uma), Some(white)) = (col("Uma"), col("White")) else { return out };
        let finished = self.veterans();
        for parent in &self.planner.plan.parents {
            let name = parent.split(" — ").next().unwrap_or("").trim();
            if name.is_empty() {
                continue;
            }
            // An imported finished uma: its sparks from the import, since skill names can hold
            // the commas the cell is split on ("Forward, March!").
            if let Some(v) = crate::veterans::row_id(name).and_then(|id| finished.iter().find(|v| v.trained_chara_id == id)) {
                for s in v.factors().iter().filter_map(|f| self.kb.sparks.get(f)).filter(|s| s.kind == "white") {
                    if let Some(k) = self.kb.skills.values().find(|k| k.sp > 0 && k.tier() == "White" && k.name.eq_ignore_ascii_case(&s.name)) {
                        out.entry(k.id).or_insert_with(|| format!("{name}'s spark ({}★)", s.stars));
                    }
                }
                continue;
            }
            let Some(row) = table.rows.iter().find(|r| r.get(uma).is_some_and(|u| u.trim() == name)) else { continue };
            for part in row.get(white).map_or("", String::as_str).split([',', ';', '·', '\n']) {
                let stars: String = part.chars().filter(char::is_ascii_digit).collect();
                let skill = part.trim().trim_end_matches(|c: char| c.is_ascii_digit() || c == '★' || c == '*' || c.is_whitespace());
                if skill.is_empty() {
                    continue;
                }
                if let Some(k) = self.kb.skills.values().find(|k| k.sp > 0 && k.tier() == "White" && k.name.eq_ignore_ascii_case(skill)) {
                    let stars = if stars.is_empty() { String::new() } else { format!(" ({stars}★)") };
                    out.entry(k.id).or_insert_with(|| format!("{name}'s spark{stars}"));
                }
            }
        }
        out
    }

    /// Every skill Step 5 lists: what the trainee and deck offer, the parents' sparks, what the
    /// shop offers, and anything pinned.
    pub(crate) fn pick_rows(&self) -> BTreeMap<u32, Vec<String>> {
        let p = &self.planner.plan;
        let mut rows = self.plan_skill_candidates();
        for (id, label) in self.parent_spark_skills() {
            rows.entry(id).or_default().push(label);
        }
        for id in p.shop.offered.keys() {
            rows.entry(*id).or_default().push("on offer at the shop".into());
        }
        for id in &p.pinned_skills {
            rows.entry(*id).or_default();
        }
        rows.retain(|id, _| self.kb.skills.get(id).is_some_and(|k| k.sp > 0 && k.category != "Unique"));
        rows
    }

    pub(crate) fn pick_context(&self) -> PickCtx {
        let cands = self.plan_skill_candidates();
        let parent_only = self.parent_spark_skills().into_keys().filter(|id| !cands.contains_key(id)).collect();
        let (disabled, cup) = self.plan_cm().map(|e| (e.disabled_skills.clone(), e.name.clone())).unwrap_or_default();
        PickCtx { rows: self.pick_rows(), parent_only, disabled, cup }
    }

    /// Why auto-pick won't buy a skill unless it's pinned, if it won't.
    pub(crate) fn pick_excluded_in(&self, ctx: &PickCtx, id: u32) -> Option<String> {
        let p = &self.planner.plan;
        if p.banned_skills.contains(&id) {
            return Some("You banned it (🚫): auto-pick never buys it.".into());
        }
        if p.pinned_skills.contains(&id) {
            return None;
        }
        if ctx.disabled.contains(&id) {
            return Some(format!("Turned off in the {}: it won't activate there.", ctx.cup));
        }
        if p.shop.on && !p.shop.offered.contains_key(&id) {
            return Some("Not ticked as on offer at the shop.".into());
        }
        if self.can_activate_on_plan(id) == Some(false) {
            return Some("Can't trigger on this course, by the simulator's check.".into());
        }
        if let Some(why) = self.skill_unsimulated(id) {
            return Some(format!("{why} Pin it (📌) to buy it."));
        }
        if !p.shop.on && ctx.parent_only.contains(&id) {
            return Some("Only if a parent's spark passes it on (3–9% a time at 0 affinity, uma.guide). Pin it (📌) to count on it.".into());
        }
        if self.sim_data.as_ref().is_some_and(|d| !d.skills.contains_key(&id)) {
            return Some("The simulator doesn't have it (most are scenario skills). Pin it (📌) to buy it anyway.".into());
        }
        None
    }

    /// The SP to spend: the shop's in skill shop mode, else the plan's budget.
    pub(crate) fn pick_budget(&self) -> Option<f64> {
        let p = &self.planner.plan;
        if p.shop.on && p.shop.sp > 0 {
            return Some(f64::from(p.shop.sp));
        }
        self.sp_budget()
    }

    /// What an auto-pick result depends on, beyond `plan_key`.
    pub(crate) fn pick_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let p = &self.planner.plan;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.plan_key(), p.deck, p.friend, p.friend_lb, &p.pinned_skills, &p.banned_skills, p.fast_learner, p.meta_influence, p.pick_objective, &p.parents).hash(&mut h);
        (p.shop.on, p.shop.stats, p.shop.sp, p.shop.offered.iter().collect::<Vec<_>>(), self.pick_budget().map(|b| b.round() as i64)).hash(&mut h);
        h.finish()
    }

    /// The auto-pick result for the plan as it is now, if one ran.
    pub(crate) fn current_pick(&self) -> Option<&Outcome> {
        let key = self.pick_key();
        self.planner.pick.as_ref().filter(|(k, _)| *k == key).map(|(_, o)| o)
    }

    /// Everything the optimiser needs, and the skills' names for its log.
    pub(crate) fn skill_opt_input(&self) -> Result<(Input, BTreeMap<u32, String>), String> {
        let p = &self.planner.plan;
        if p.pick_objective == PickObjective::Rating {
            return Err("Rating / parent needs each skill's evaluation points, which aren't in knowledge/ yet.".into());
        }
        let data = self.sim_data.clone().ok_or("No simulator data; run tools/update_knowledge.py --only umalator.")?;
        let target = if p.shop.on {
            if p.shop.stats.iter().all(|v| *v == 0) {
                return Err("Enter the final stats from the skill shop screen first.".into());
            }
            p.shop.stats
        } else {
            p.target
        };
        let base = self.plan_sim_build_for(false, target)?;
        let setup = self.plan_race_setup();
        let course_id = setup.course.ok_or("Pick a course or Champions Meeting for the plan first.")?;
        let course = Arc::new(data.courses.get(&course_id).cloned().ok_or("That course isn't in the simulator's data.")?);
        let race = crate::sim::run::race_params(
            i32::from(setup.ground.unwrap_or(1)),
            i32::from(setup.weather.unwrap_or(1)),
            i32::from(setup.season.unwrap_or(1)),
            i32::from(setup.time.unwrap_or(2)),
            Some(base.desc.strategy),
        );
        let budget = self.pick_budget().ok_or("Set an SP budget first.")?;
        let career = self.plan_career();
        let ctx = self.pick_context();
        let meta = self.plan_meta_skills();
        let mut names = BTreeMap::new();
        let mut candidates = Vec::new();
        for &id in ctx.rows.keys() {
            let pinned = p.pinned_skills.contains(&id);
            if !pinned && self.pick_excluded_in(&ctx, id).is_some() {
                continue;
            }
            let lower = self.lower_version(id).map(|l| (l, self.own_price(l, career.as_ref())));
            let backing = meta.get(&id).filter(|_| self.can_activate_on_plan(id) != Some(false)).map_or(0.0, |(w, _)| f64::from(*w));
            candidates.push(Candidate { id, own: self.own_price(id, career.as_ref()), lower, meta: backing, pinned });
            names.insert(id, self.kb.skill_name(id));
            if let Some((l, _)) = lower {
                names.insert(l, self.kb.skill_name(l));
            }
        }
        if candidates.is_empty() {
            return Err("No skills to choose from: pick a trainee and cards, or tick what the shop offers.".into());
        }
        let tune = if cfg!(test) { Tune::quick() } else { Tune::default() };
        let input = Input { data, course, race, base, candidates, budget, meta: p.meta_influence, objective: p.pick_objective, seed: 2026.0, tune };
        Ok((input, names))
    }

    /// Start the optimiser in the background.
    pub(crate) fn start_pick(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.planner.pick_job.is_some() {
            return Ok(());
        }
        let (input, names) = self.skill_opt_input()?;
        let key = self.pick_key();
        let progress = Arc::new(Progress::default());
        let phase = Arc::new(Phase::default());
        let (tx, rx) = mpsc::channel();
        let (p2, ph2, repaint) = (progress.clone(), phase.clone(), ctx.clone());
        std::thread::spawn(move || {
            let out = skill_opt::optimise(&input, &names, &p2, &ph2);
            let _ = tx.send(out);
            repaint.request_repaint();
        });
        self.planner.pick_job = Some(PickJob { key, progress, phase, rx, started: Instant::now() });
        Ok(())
    }

    /// Collect the optimiser's result, and apply it while the plan hasn't changed.
    pub(crate) fn pick_poll(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.planner.pick_job else { return };
        match job.rx.try_recv() {
            Ok(out) => {
                let job = self.planner.pick_job.take().expect("job present");
                let current = job.key == self.pick_key();
                let msg = if out.cancelled {
                    ("Stopped. The quick pick stays.".to_string(), true)
                } else if !current {
                    ("The plan changed while auto-pick ran; press ✨ Auto-pick again.".to_string(), true)
                } else {
                    self.planner.plan.skills = out.chosen.clone();
                    // The numbers are in the summary line under it.
                    match &out.verify {
                        Some(_) => (format!("Picked {} skill{} in {:.0} s.", out.chosen.len(), if out.chosen.len() == 1 { "" } else { "s" }, out.seconds), false),
                        None => ("Nothing on offer gains enough on this course to be worth buying.".to_string(), true),
                    }
                };
                // SP left over: what else is on offer is worth nothing in simulation, or is left to the user.
                let left = self.pick_budget().map_or(0.0, |b| b - out.spent);
                let msg = if current && !out.cancelled && left > 150.0 {
                    let ctx = self.pick_context();
                    let unsim = ctx.rows.keys().filter(|id| self.skill_unsimulated(**id).is_some() && !self.planner.plan.pinned_skills.contains(id)).count();
                    let hint = if unsim > 0 { format!(" Pin skills from \"Not simulated\" below ({unsim}) to spend it.") } else { " Nothing else on offer gains in simulation.".to_string() };
                    (format!("{} {left:.0} SP left.{hint}", msg.0), msg.1)
                } else {
                    msg
                };
                self.planner.pick = Some((job.key, out));
                self.planner.skill_status = Some(msg);
            }
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.planner.pick_job = None;
                self.planner.skill_status = Some(("Auto-pick stopped unexpectedly.".into(), true));
            }
        }
    }

    /// ✨ Auto-pick: a quick pick at once, then the optimiser in the background.
    pub(crate) fn auto_pick(&mut self, ctx: &egui::Context) {
        if self.planner.plan.pick_objective == PickObjective::Rating {
            let msg = self.rating_auto_pick();
            self.planner.skill_status = Some((msg, false));
            return;
        }
        let mut msg = self.auto_pick_skills();
        let mut warn = false;
        match self.start_pick(ctx) {
            Ok(()) => msg = "Quick pick done. Now racing the skills to find the best set (about 20 seconds)…".into(),
            Err(e) => {
                msg += &format!(" These come from single gains or estimates: {e}");
                warn = true;
            }
        }
        self.planner.skill_status = Some((msg, warn));
    }

    /// Auto-pick for Rating / parent: the points knapsack, at once. Returns the status line.
    pub(crate) fn rating_auto_pick(&mut self) -> String {
        let (chosen, points) = self.rating_pick();
        let cost = self.chosen_cost(&chosen, self.plan_career().as_ref());
        let n = chosen.len();
        self.planner.plan.skills = chosen;
        format!(
            "Picked {n} skill{} worth {points} evaluation points for about {cost:.0} SP (uma.guide's skill data; aptitude scaling of distance and style skills isn't modelled).",
            if n == 1 { "" } else { "s" }
        )
    }

    /// Step 5.
    pub(crate) fn skill_picker(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let ctx = ui.ctx().clone();
        let pick = self.pick_context();
        let title = semibold("Step 5 · Skills to buy", size::MD);
        if pick.rows.is_empty() && !self.planner.plan.shop.on {
            ui.horizontal(|ui| {
                ui.label(title);
                ui.label(RichText::new("pick a trainee and cards to see the skills on offer").size(size::SM).color(p.weak));
            });
            return;
        }
        let career = self.plan_career();
        let budget = self.pick_budget();
        let spent = self.chosen_cost(&self.planner.plan.skills, career.as_ref()).abs();
        let mut auto = false;
        let mut start_chart = false;

        // Title, counts, and the main actions.
        ui.horizontal(|ui| {
            ui.label(title);
            ui.add_space(4.0);
            badge(ui, format!("{} chosen", self.planner.plan.skills.len()), p.accent);
            let over = budget.is_some_and(|b| spent > b + 0.5);
            let text = match budget {
                Some(b) => format!("{spent:.0} / {b:.0} SP"),
                None => format!("{spent:.0} SP"),
            };
            badge(ui, text, if over { p.warn } else { p.gold }).on_hover_text("What the chosen skills cost after discounts, against the SP budget");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✖ Clear").on_hover_text("Unpick every skill").clicked() {
                    self.planner.plan.skills.clear();
                    self.planner.skill_status = None;
                }
                match &self.planner.pick_job {
                    Some(job) => {
                        if ui.small_btn("✖").on_hover_text("Stop auto-pick").clicked() {
                            job.cancel();
                        }
                        let secs = job.started.elapsed().as_secs();
                        ui.add(egui::ProgressBar::new(job.fraction()).desired_width(170.0).text(format!("{} s", secs)));
                        ui.label(RichText::new(job.phase.get()).size(size::SM).color(p.weak));
                    }
                    None => {
                        let hover = "Choose the skills to buy within the SP budget. It picks at once from single gains, then races the candidates \
                                     to find the set that gains the most together: each step adds the skill whose measured gain on top of the \
                                     skills already chosen is best per SP, then it tries swaps and checks the set in 1000 races.";
                        if ui.add(crate::ui_util::primary_button(ui, "✨ Auto-pick", size::BODY)).on_hover_text(hover).clicked() {
                            auto = true;
                        }
                        let chart_running = self.planner.skill_job.is_some();
                        if !chart_running
                            && ui
                                .button("🏁 Simulate skills")
                                .on_hover_text("Race each skill alone on the plan's course, in realistic races, for the Gain alone column and the card rankings")
                                .clicked()
                        {
                            start_chart = true;
                        }
                        if let Some(job) = &self.planner.skill_job {
                            if ui.small_btn("✖").on_hover_text("Stop simulating").clicked() {
                                job.cancel();
                            }
                            ui.add(egui::ProgressBar::new(job.fraction()).desired_width(120.0).text(format!("{} skills", job.count())));
                        }
                    }
                }
            });
        });
        if auto {
            self.auto_pick(&ctx);
        }
        if start_chart && let Err(e) = self.start_skill_values(&ctx, false) {
            self.planner.skill_status = Some((e, true));
        }

        // How it picks.
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            let pl = &mut self.planner.plan;
            ui.label(RichText::new("Aim").color(p.weak));
            egui::ComboBox::from_id_salt("pick_objective").selected_text(pl.pick_objective.label()).show_ui(ui, |ui| {
                for o in PickObjective::ALL {
                    ui.selectable_value(&mut pl.pick_objective, o, o.label()).on_hover_text(o.explain());
                }
            })
            .response
            .on_hover_text(pl.pick_objective.explain());
            ui.label(RichText::new("Meta sheet").color(p.weak)).on_hover_text("How much the community meta sheet's skill advice counts. Community opinion, not game data.");
            egui::ComboBox::from_id_salt("pick_meta").selected_text(pl.meta_influence.label()).show_ui(ui, |ui| {
                for m in MetaInfluence::ALL {
                    ui.selectable_value(&mut pl.meta_influence, m, m.label()).on_hover_text(m.explain());
                }
            })
            .response
            .on_hover_text(pl.meta_influence.explain());
            ui.tick(&mut pl.fast_learner, "Fast Learner (−10%)").on_hover_text("The Fast Learner condition: every skill costs 10% of its base price less (uma.guide glossary)");
            ui.tick(&mut pl.shop.on, "I'm at the skill shop now").on_hover_text("Use the career's real final stats and SP, and only the skills on offer with their hint levels");
            ui.tick(&mut self.planner.only_fitting, "Hide skills that don't fit");
        });
        if self.planner.plan.shop.on {
            self.shop_panel(ui);
        }
        if let Some((msg, warn)) = &self.planner.skill_status {
            ui.label(RichText::new(msg).size(size::SM).color(if *warn { p.warn } else { p.good }));
        }
        self.pick_summary(ui);
        self.budget_line(ui, career.as_ref(), budget);
        ui.add_space(4.0);
        self.pick_table(ui, &pick);
        self.unsimulated_list(ui, &pick);
    }

    /// "13 skills · 2497/2547 SP · +3.5 L in 1000 races (95%: …) · singles summed: 7.1 L", and the
    /// decision log.
    fn pick_summary(&self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let Some(out) = self.current_pick() else { return };
        let Some(v) = &out.verify else { return };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(semibold(format!("{} skills · {:.0} SP", out.chosen.len(), out.spent), size::SM));
            ui.label(RichText::new("·").color(p.weak));
            ui.label(semibold(format!("{:+.2} L in {} races", v.mean, v.runs), size::SM).color(p.good))
                .on_hover_text("The chosen set against the trainee with only her unique, in realistic races (wit checks and rushing on). Simulator output.");
            ui.label(RichText::new(format!("(95%: {:+.2} to {:+.2})", v.lo, v.hi)).size(size::SM).color(p.weak));
            ui.label(RichText::new("·").color(p.weak));
            ui.label(RichText::new(format!("their single gains add up to {:.2} L", v.singles_sum)).size(size::SM).color(p.weak))
                .on_hover_text("Each skill alone, added up. Skills overlap and fire in the same stretches, so together they gain less than the sum.");
        });
        // UMALORD_EXPLAIN=1 opens it, for screenshots.
        let open = std::env::var_os("UMALORD_EXPLAIN").is_some();
        egui::CollapsingHeader::new(RichText::new("Explain: how auto-pick chose").size(size::SM)).id_salt("pick_explain").default_open(open).show(ui, |ui| {
            for line in &out.log {
                ui.label(RichText::new(line).size(size::SM));
            }
        });
    }

    /// Where the SP budget comes from, and setting it by hand.
    fn budget_line(&mut self, ui: &mut egui::Ui, career: Option<&crate::career::CareerResult>, budget: Option<f64>) {
        let p = pal(ui);
        if self.planner.plan.shop.on {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            match (career, self.planner.plan.sp_budget) {
                (_, Some(_)) => {
                    ui.label(RichText::new("SP budget set by hand:").size(size::SM).color(p.weak));
                }
                (Some(c), None) => {
                    let sources: Vec<String> = c.sp_sources.iter().filter(|(_, v)| *v >= 1.0).map(|(k, v)| format!("{k} {v:.0}")).collect();
                    ui.label(RichText::new(format!(
                        "SP budget ≈ {:.0}: the mean of {} simulated {} careers with this deck (10–90%: {:.0}–{:.0}; {}).",
                        c.sp_mean,
                        c.runs,
                        self.plan_scenario_name(),
                        c.sp_p10,
                        c.sp_p90,
                        sources.join(", ")
                    ))
                    .size(size::SM)
                    .color(p.weak))
                    .on_hover_text(self.career_explainer(c));
                }
                (None, None) => {
                    ui.label(RichText::new("No career simulation (knowledge/scenarios.json missing); set a budget by hand:").size(size::SM).color(p.warn));
                }
            }
            let mut manual = self.planner.plan.sp_budget.is_some();
            if ui.tick(&mut manual, "set by hand").changed() {
                self.planner.plan.sp_budget = manual.then(|| budget.unwrap_or(1500.0).round() as u32);
            }
            if let Some(b) = &mut self.planner.plan.sp_budget {
                ui.add(egui::DragValue::new(b).range(0..=6000).speed(10).suffix(" SP"));
            }
        });
    }

    /// Skill shop mode: the real final stats and SP, and pasting the skills on offer.
    fn shop_panel(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        egui::Frame::new()
            .fill(p.accent.gamma_multiply(if ui.visuals().dark_mode { 0.12 } else { 0.07 }))
            .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.5)))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(semibold("At the skill shop", size::BODY));
                ui.label(
                    RichText::new("Enter the stats and SP the career ended with, then tick the skills on offer below and set their hint levels. Auto-pick then works on exactly those.")
                        .size(size::SM)
                        .color(p.weak),
                );
                let shop = &mut self.planner.plan.shop;
                ui.horizontal_wrapped(|ui| {
                    for (i, name) in ["Speed", "Stamina", "Power", "Guts", "Wit"].iter().enumerate() {
                        ui.label(RichText::new(*name).color(p.weak));
                        ui.add(egui::DragValue::new(&mut shop.stats[i]).range(0..=2000).speed(5));
                    }
                    ui.label(RichText::new("SP").color(p.weak));
                    ui.add(egui::DragValue::new(&mut shop.sp).range(0..=9999).speed(5));
                });
                ui.horizontal(|ui| {
                    if ui.button("📋 Paste the list").on_hover_text("Paste or type the skills on offer, one per line, with a hint level if any: \"Corner Adept ○ Lv3\"").clicked() {
                        self.planner.shop_paste = Some(self.planner.shop_paste.take().unwrap_or_default());
                    }
                    if !shop.offered.is_empty() && ui.button("Untick all").clicked() {
                        shop.offered.clear();
                    }
                    ui.label(RichText::new(format!("{} on offer", shop.offered.len())).size(size::SM).color(p.weak));
                });
                if let Some(text) = &mut self.planner.shop_paste {
                    ui.add(egui::TextEdit::multiline(text).desired_rows(4).desired_width(f32::INFINITY).hint_text("One skill per line, e.g.\nCorner Adept ○ Lv3\nGroundwork 2"));
                    let mut done = false;
                    ui.horizontal(|ui| {
                        if ui.add(crate::ui_util::primary_button(ui, "Tick these", size::SM)).clicked() {
                            done = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.planner.shop_paste = None;
                        }
                    });
                    if done {
                        let text = self.planner.shop_paste.take().unwrap_or_default();
                        let found = match_skill_lines(&self.kb, &text);
                        let n = found.len();
                        let missed = text.lines().filter(|l| !l.trim().is_empty()).count().saturating_sub(n);
                        for (id, lv) in found {
                            self.planner.plan.shop.offered.insert(id, lv);
                        }
                        let note = if missed > 0 { format!(" {missed} line{} matched no skill.", if missed == 1 { "" } else { "s" }) } else { String::new() };
                        self.toast(format!("Ticked {n} skill{} as on offer.{note}", if n == 1 { "" } else { "s" }), crate::ui_util::ToastKind::Success);
                    }
                }
            });
    }

    /// The skills table.
    fn pick_table(&mut self, ui: &mut egui::Ui, pick: &PickCtx) {
        let p = pal(ui);
        let career = self.plan_career();
        let meta = self.plan_meta_skills();
        let out = self.current_pick().cloned();
        let chart = self.planner.current_skill_values(self.plan_key()).map(|v| v.rows.iter().map(|(k, r)| (*k, r.mean)).collect::<HashMap<u32, f64>>());
        let shop = self.planner.plan.shop.on;
        let chosen = self.planner.plan.skills.clone();
        let pinned = self.planner.plan.pinned_skills.clone();
        // Gain alone: the optimiser's realistic screen, else the skill chart, else an estimate.
        let single = |id: u32| -> (f64, bool) {
            if let Some(g) = out.as_ref().and_then(|o| o.singles.get(&id)) {
                return (*g, true);
            }
            if let Some(g) = chart.as_ref().and_then(|c| c.get(&id)) {
                return (*g, true);
            }
            (self.skill_lengths(id).0, false)
        };
        struct Row {
            id: u32,
            from: String,
            single: f64,
            measured: bool,
            cost: f64,
            excluded: Option<String>,
        }
        let mut rows: Vec<Row> = pick
            .rows
            .iter()
            .filter(|(id, _)| self.skill_unsimulated(**id).is_none() || pinned.contains(id))
            .map(|(id, src)| {
                let (single, measured) = single(*id);
                Row { id: *id, from: src.join(", "), single, measured, cost: self.skill_cost(*id, career.as_ref()), excluded: self.pick_excluded_in(pick, *id) }
            })
            .collect();
        if self.planner.only_fitting {
            rows.retain(|r| {
                chosen.contains(&r.id) || pinned.contains(&r.id) || meta.contains_key(&r.id) || (shop && self.planner.plan.shop.offered.contains_key(&r.id)) || r.single > 0.05 || self.skill_fit(r.id) != Fit::Other
            });
        }
        let added = |id: u32| out.as_ref().and_then(|o| o.added.get(&id)).copied();
        let sort = self.planner.pick_sort;
        rows.sort_by(|a, b| b.single.total_cmp(&a.single).then(a.id.cmp(&b.id)));
        sort_rows(&mut rows, sort.desc, |r| match sort.key {
            "pick" => SortKey::Num(if chosen.contains(&r.id) { 1000.0 - added(r.id).map_or(0.0, |(s, _)| s as f64) } else { r.single.max(-1.0) }),
            "name" => SortKey::Text(self.kb.skill_name(r.id)),
            "single" => SortKey::Num(r.single),
            "inset" => SortKey::Num(out.as_ref().and_then(|o| o.marginal.get(&r.id)).copied().unwrap_or(f64::NEG_INFINITY)),
            "cost" => SortKey::Num(r.cost),
            "eff" => SortKey::Num(if r.cost > 0.0 { r.single / r.cost } else { 0.0 }),
            "act" => SortKey::Num(out.as_ref().and_then(|o| o.activation.get(&r.id)).copied().unwrap_or(-1.0)),
            "hint" => SortKey::Num(f64::from(self.planner.plan.shop.offered.get(&r.id).copied().unwrap_or(0))),
            _ => SortKey::Text(String::new()),
        });
        let max_eff = rows.iter().filter(|r| r.cost > 0.0).map(|r| r.single / r.cost).fold(0.0f64, f64::max);
        let mut cols = vec![
            TableCol::new("pick", "").width(Column::exact(30.0)).center().sort_desc().hover("Bought: chosen skills first"),
            TableCol::new("pin", "").width(Column::exact(32.0)).center().hover("📌 always buy · 🚫 never buy. Click to cycle."),
        ];
        if shop {
            cols.push(TableCol::new("offer", "Offer").width(Column::exact(52.0)).center().hover("On offer at the shop"));
            cols.push(TableCol::new("hint", "Hint").width(Column::exact(88.0)).center().sort_desc().hover("Its hint level at the shop: each level takes 10/20/30/35/40% off (uma.guide)"));
        }
        cols.extend([
            TableCol::new("name", "Skill").width(Column::initial(200.0).at_least(140.0).clip(true)).sort_asc(),
            TableCol::new("single", "Gain alone").width(Column::exact(86.0)).right().sort_desc().hover("Lengths it gains on its own on the plan's course, in realistic races (~ means an estimate). Simulator output."),
            TableCol::new("inset", "In this set").width(Column::exact(86.0)).right().sort_desc().hover("What it adds to the chosen set: the set against the set without it. After auto-pick runs."),
            TableCol::new("cost", "Cost").width(Column::exact(58.0)).right().sort_asc().hover("SP after discounts, including the ○/white a ◎/gold needs"),
            TableCol::new("eff", "L / 100 SP").width(Column::exact(104.0)).sort_desc().hover("Gain alone per 100 SP"),
            TableCol::new("act", "Triggers").width(Column::exact(70.0)).right().sort_desc().hover("How often it triggered in the 1000-race check"),
            TableCol::new("why", "Why").width(Column::remainder().at_least(180.0).clip(true)).hover("Why it was or wasn't bought"),
            TableCol::new("effect", "Effect").width(Column::initial(200.0).clip(true)).optional(true),
            TableCol::new("from", "From").width(Column::initial(180.0).clip(true)).optional(true).hover("Where the skill comes from: the trainee, a card, a parent's spark"),
        ]);
        let height = (rows.len() as f32 * 32.0 + 40.0).clamp(140.0, 380.0);
        let mut toggles: Vec<u32> = Vec::new();
        let mut pins: Vec<u32> = Vec::new();
        let mut offers: Vec<u32> = Vec::new();
        let mut hints: Vec<(u32, u8)> = Vec::new();
        let mut sort_state = self.planner.pick_sort;
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} skills", rows.len())).size(size::SM).color(p.weak));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| crate::ui_util::columns_menu(ui, "pick", &cols));
        });
        DataTable::new("pick", &cols).sort(&mut sort_state).row_height(32.0).max_height(height).show(ui, rows.len(), |ui, i, key| {
            let r = &rows[i];
            let is_chosen = chosen.contains(&r.id);
            match key {
                "pick" => {
                    let mut on = is_chosen;
                    if ui.tick(&mut on, "").on_hover_text(if is_chosen { "Bought. Click to unpick." } else { "Click to buy" }).changed() {
                        toggles.push(r.id);
                    }
                }
                "pin" => {
                    let (icon, hover, color) = if pinned.contains(&r.id) {
                        ("📌", "Pinned: auto-pick always buys it. Click to ban it instead.", p.text)
                    } else if self.planner.plan.banned_skills.contains(&r.id) {
                        ("🚫", "Banned: auto-pick never buys it. Click to clear.", p.bad)
                    } else {
                        ("📌", "Click to pin it (always buy); click again to ban it (never buy)", p.weak.gamma_multiply(0.45))
                    };
                    if ui.add(egui::Button::new(RichText::new(icon).color(color)).frame(false)).on_hover_text(hover).clicked() {
                        pins.push(r.id);
                    }
                }
                "offer" => {
                    let mut on = self.planner.plan.shop.offered.contains_key(&r.id);
                    if ui.tick(&mut on, "").on_hover_text("On offer at the shop").changed() {
                        offers.push(r.id);
                    }
                }
                "hint" => {
                    if let Some(&lv) = self.planner.plan.shop.offered.get(&r.id) {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if ui.small_btn("−").on_hover_text("One hint level lower").clicked() && lv > 0 {
                            hints.push((r.id, lv - 1));
                        }
                        ui.label(semibold(format!("Lv{lv}"), size::SM)).on_hover_text("The hint level the shop shows: each level takes 10% off (35% at Lv4, 40% at Lv5)");
                        if ui.small_btn("+").on_hover_text("One hint level higher").clicked() && lv < 5 {
                            hints.push((r.id, lv + 1));
                        }
                    }
                }
                "name" => {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if let Some((w, why)) = meta.get(&r.id).filter(|(w, _)| *w > 0.0) {
                        let strength = if *w >= 2.0 { "the skill advice" } else if *w >= 1.0 { "a recommended card's note" } else { "a recommended gold's ○/white" };
                        ui.label(RichText::new("★").color(p.gold)).on_hover_text(format!("Recommended by the meta sheet ({strength}): {why}. Community opinion, not game data."));
                    }
                    if pick.disabled.contains(&r.id) {
                        ui.label(RichText::new(self.kb.skill_name(r.id)).size(size::SM).strikethrough().color(p.weak));
                    } else {
                        self.skill_link(ui, r.id, r.excluded.is_some() && !is_chosen);
                    }
                }
                "single" => {
                    let text = if r.measured { format!("{:+.2} L", r.single) } else { format!("~{:.2} L", r.single) };
                    ui.label(RichText::new(text).color(if r.measured && r.single > 0.05 { p.good } else { p.weak }));
                }
                "inset" => match out.as_ref().and_then(|o| o.marginal.get(&r.id)) {
                    Some(g) => {
                        ui.label(semibold(format!("{g:+.2} L"), size::BODY).color(if *g > 0.02 { p.good } else { p.warn }));
                    }
                    None => {
                        ui.label(RichText::new("—").color(p.weak));
                    }
                },
                "cost" => {
                    ui.label(format!("{:.0}", r.cost));
                }
                "eff" => {
                    let e = if r.cost > 0.0 { r.single / r.cost * 100.0 } else { 0.0 };
                    let (rect, _) = ui.allocate_exact_size(vec2(56.0, 8.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, CornerRadius::same(3), p.border);
                    if max_eff > 0.0 && e > 0.0 {
                        let t = (e / (max_eff * 100.0)).clamp(0.0, 1.0) as f32;
                        let fill = egui::Rect::from_min_size(rect.min, vec2(rect.width() * t, rect.height()));
                        ui.painter().rect_filled(fill, CornerRadius::same(3), crate::charts::sequential(0.35 + 0.65 * t, ui.visuals().dark_mode));
                    }
                    ui.label(RichText::new(format!("{e:.2}")).size(size::SM));
                }
                "act" => match out.as_ref().and_then(|o| o.activation.get(&r.id)) {
                    // Passives and gate skills: the simulator applies them without logging a trigger.
                    Some(a) if *a <= 0.0 && self.skill_not_logged(r.id).is_some() => {
                        let how = self.skill_not_logged(r.id).unwrap_or_default();
                        let word = if how.starts_with("Passive") { "passive" } else { "at gate" };
                        ui.label(RichText::new(word).size(size::SM).color(p.weak)).on_hover_text(format!("{how}: the simulator doesn't log it as triggering"));
                    }
                    Some(a) => {
                        ui.label(format!("{:.0}%", a * 100.0));
                    }
                    None => {
                        ui.label(RichText::new("—").color(p.weak));
                    }
                },
                "why" => {
                    let why = self.pick_why(r.id, is_chosen, r.single, r.measured, r.cost, r.excluded.as_deref(), out.as_ref(), &meta);
                    let full = if r.from.is_empty() { why.clone() } else { format!("{why}\nFrom: {}", r.from) };
                    ui.add(egui::Label::new(RichText::new(&why).size(size::SM).color(p.weak)).truncate()).on_hover_text(full);
                }
                "effect" => {
                    ui.add(egui::Label::new(RichText::new(self.kb.skills.get(&r.id).map_or(String::new(), |k| k.effect_text())).size(size::SM)).truncate());
                }
                "from" => {
                    ui.add(egui::Label::new(RichText::new(&r.from).size(size::SM).color(p.weak)).truncate());
                }
                _ => {}
            }
        });
        self.planner.pick_sort = sort_state;
        let pl = &mut self.planner.plan;
        for id in toggles {
            if pl.skills.contains(&id) {
                pl.skills.retain(|s| *s != id);
            } else {
                pl.skills.push(id);
            }
        }
        for id in pins {
            if pl.pinned_skills.contains(&id) {
                pl.pinned_skills.retain(|s| *s != id);
                pl.banned_skills.push(id);
                pl.skills.retain(|s| *s != id);
            } else if pl.banned_skills.contains(&id) {
                pl.banned_skills.retain(|s| *s != id);
            } else {
                pl.pinned_skills.push(id);
                if !pl.skills.contains(&id) {
                    pl.skills.push(id);
                }
            }
        }
        for id in offers {
            if pl.shop.offered.remove(&id).is_none() {
                pl.shop.offered.insert(id, 0);
            }
        }
        for (id, lv) in hints {
            pl.shop.offered.insert(id, lv);
        }
    }

    /// One row's "Why".
    #[allow(clippy::too_many_arguments)]
    fn pick_why(&self, id: u32, chosen: bool, single: f64, measured: bool, cost: f64, excluded: Option<&str>, out: Option<&Outcome>, meta: &HashMap<u32, (f32, String)>) -> String {
        let p = &self.planner.plan;
        if chosen {
            if p.pinned_skills.contains(&id) {
                return "Pinned (📌): always bought.".into();
            }
            return match out.and_then(|o| o.added.get(&id)) {
                Some((0, g)) => format!("Swapped in: the set gained {g:+.2} L."),
                Some((step, g)) => {
                    let upgrade = self.lower_version(id).map_or(String::new(), |l| format!(" (with {} first)", self.kb.skill_name(l)));
                    format!("Added at step {step}: {g:+.2} L on top of the skills before it{upgrade}.")
                }
                None if out.is_some() => "Chosen by you.".into(),
                None => "Chosen (quick pick, from single gains).".into(),
            };
        }
        if let Some(why) = excluded {
            return why.to_string();
        }
        let how = if measured { "in simulation" } else { "by estimate" };
        if meta.get(&id).is_some_and(|(w, _)| *w > 0.0) {
            return format!("The meta sheet recommends it, but it's worth {single:+.2} L for {cost:.0} SP on this course {how}.");
        }
        if single <= 0.02 {
            return format!("Gains little on this course ({single:+.2} L {how}).");
        }
        if out.is_some() {
            return "Others gave more per SP once the set was chosen.".into();
        }
        String::new()
    }

    /// Debuffs and position skills: listed apart, with the meta sheet's advice and a pin button.
    fn unsimulated_list(&mut self, ui: &mut egui::Ui, pick: &PickCtx) {
        let p = pal(ui);
        let pinned = self.planner.plan.pinned_skills.clone();
        let mut items: Vec<(u32, &'static str)> = pick.rows.keys().filter(|id| !pinned.contains(id)).filter_map(|id| self.skill_unsimulated(*id).map(|why| (*id, why))).collect();
        if items.is_empty() {
            return;
        }
        // The community tier list's CM tier first (it rates what the simulator can't), then name.
        let rank = |id: u32| self.kb.skill_tiers.as_ref().and_then(|t| t.get(id)).map_or(9, |t| crate::notes::tier_rank(&t.cm));
        items.sort_by_key(|(id, _)| (rank(*id), self.kb.skill_name(*id)));
        let meta = self.plan_meta_skills();
        let career = self.plan_career();
        let mut pin = None;
        egui::CollapsingHeader::new(RichText::new(format!("Not simulated ({}): debuffs and skills that depend on other umas", items.len())).size(size::SM))
            .id_salt("pick_unsimulated")
            // UMALORD_EXPLAIN=1 opens it too, for screenshots.
            .default_open(std::env::var_os("UMALORD_EXPLAIN").is_some())
            .show(ui, |ui| {
                ui.label(RichText::new("One simulated uma can't value these, so auto-pick leaves them to you. Pin one to buy it.").size(size::SM).color(p.weak));
                for (id, why) in items {
                    ui.horizontal(|ui| {
                        if ui.small_btn("📌").on_hover_text("Pin it: auto-pick always buys it").clicked() {
                            pin = Some(id);
                        }
                        if let Some((_, advice)) = meta.get(&id) {
                            ui.label(RichText::new("★").color(p.gold)).on_hover_text(format!("The meta sheet: {advice}. Community opinion."));
                        }
                        self.skill_link(ui, id, false);
                        ui.label(RichText::new(format!("{:.0} SP", self.skill_cost(id, career.as_ref()))).size(size::SM).color(p.weak));
                        // The community tier list rates what the simulator can't.
                        if let Some((tiers, t)) = self.kb.skill_tiers.as_ref().and_then(|x| Some((x, x.get(id)?))) {
                            badge(ui, format!("CM {} · TT {}", t.cm, t.tt), p.gold).on_hover_text(tiers.explain(t));
                        }
                        ui.label(RichText::new(why).size(size::SM).color(p.weak));
                    });
                }
            });
        if let Some(id) = pin {
            let pl = &mut self.planner.plan;
            pl.pinned_skills.push(id);
            pl.banned_skills.retain(|s| *s != id);
            if !pl.skills.contains(&id) {
                pl.skills.push(id);
            }
        }
    }

    /// Step 2: a warning when the trainee's grade for the goal is below B, with what it costs her
    /// by the simulator's aptitude coefficients.
    pub(crate) fn aptitude_warning(&self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let pl = &self.planner.plan;
        let Some(info) = pl.trainee.and_then(|t| self.kb.umas.get(&t)) else { return };
        let a = &info.aptitude;
        let (dg, sg, yg) = (a.distance(pl.distance), a.surface(pl.surface), a.style(pl.style));
        if [dg, sg, yg].iter().all(|g| grade_rank(g) >= 6) {
            return;
        }
        use crate::sim::params::parse_aptitude;
        let stats = if pl.target.iter().any(|v| *v > 0) { pl.target.map(f64::from) } else { [1000.0; 5] };
        let desc = crate::sim::params::HorseDesc {
            speed: stats[0],
            stamina: stats[1],
            power: stats[2],
            guts: stats[3],
            wisdom: stats[4],
            strategy: crate::tab_sim::strategy_of(pl.style),
            distance_apt: parse_aptitude(dg),
            surface_apt: parse_aptitude(sg),
            strategy_apt: parse_aptitude(yg),
            mood: 2,
        };
        let metres = self
            .plan_race_setup()
            .course
            .and_then(|c| self.sim_data.as_ref()?.courses.get(&c).map(|x| x.distance))
            .unwrap_or(match pl.distance {
                crate::knowledge::Distance::Sprint => 1200.0,
                crate::knowledge::Distance::Mile => 1600.0,
                crate::knowledge::Distance::Medium => 2000.0,
                crate::knowledge::Distance::Long => 2500.0,
            });
        let loss = crate::sim::solver::aptitude_losses(&desc, metres, desc.distance_apt, desc.surface_apt, desc.strategy_apt);
        let mut lines = Vec::new();
        if grade_rank(dg) < 6 {
            lines.push(format!("{} {dg}: in the last spurt she runs about {:.1}% slower than at A ({metres:.0} m).", pl.distance.label(), loss[0] * 100.0));
        }
        if grade_rank(sg) < 6 {
            lines.push(format!("{} {sg}: she accelerates about {:.0}% slower than at A.", pl.surface.label(), loss[1] * 100.0));
        }
        if grade_rank(yg) < 6 {
            lines.push(format!("{} {yg}: her Wit counts {:.0}% less, so skills trigger less often and she rushes more.", pl.style.label(), loss[2] * 100.0));
        }
        let pink = self.pink_sparks_wanted();
        egui::Frame::new()
            .fill(p.warn.gamma_multiply(if ui.visuals().dark_mode { 0.12 } else { 0.08 }))
            .stroke(Stroke::new(1.0, p.warn.gamma_multiply(0.6)))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(semibold("⚠ Below B for this goal", size::BODY).color(p.warn));
                for l in lines {
                    ui.label(RichText::new(l).size(size::SM));
                }
                let fix = if pink.is_empty() { String::new() } else { format!("Pink sparks from parents can raise it ({}). ", pink.join(", ")) };
                ui.label(RichText::new(format!("{fix}Or pick a trainee who fits better. From the race simulator's aptitude coefficients.")).size(size::XS).color(p.weak));
            });
    }
}

/// Skills named in pasted lines, with the hint level a line gives ("Lv3", "hint 2", a trailing
/// number). Names match ignoring case and the ○/◎ marks, else the closest by words.
pub(crate) fn match_skill_lines(kb: &crate::knowledge::Knowledge, text: &str) -> Vec<(u32, u8)> {
    let norm = |s: &str| s.to_lowercase().replace(['○', '◎', '×', '☆', '★'], " ").split_whitespace().collect::<Vec<_>>().join(" ");
    let skills: Vec<(u32, String)> = kb.skills.values().filter(|k| k.sp > 0).map(|k| (k.id, k.name.clone())).collect();
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_lowercase();
        // The hint level: "lv 3", "lv3", "hint 2" or a number at the end.
        let mut level = 0u8;
        let mut name = lower.clone();
        for key in ["lv.", "lv", "hint", "level"] {
            if let Some(i) = name.rfind(key) {
                let digits: String = name[i + key.len()..].trim().chars().take_while(char::is_ascii_digit).collect();
                if let Ok(n) = digits.parse::<u8>() {
                    level = n.min(5);
                    name.truncate(i);
                    break;
                }
            }
        }
        if level == 0 {
            let trimmed = name.trim_end();
            let digits: String = trimmed.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
            if !digits.is_empty() && digits.len() < trimmed.len() && let Ok(n) = digits.parse::<u8>() {
                level = n.min(5);
                name = trimmed[..trimmed.len() - digits.len()].to_string();
            }
        }
        let want = norm(&name);
        // The line's own ○/◎ says which version.
        let mark = if line.contains('◎') { Some('◎') } else if line.contains('○') { Some('○') } else { None };
        let pick = |cands: Vec<&(u32, String)>| -> Option<u32> {
            match mark {
                Some(m) => cands.iter().find(|(_, n)| n.contains(m)).or(cands.first()).map(|(id, _)| *id),
                None => cands.iter().find(|(_, n)| !n.contains('◎')).or(cands.first()).map(|(id, _)| *id),
            }
        };
        let exact: Vec<&(u32, String)> = skills.iter().filter(|(_, n)| norm(n) == want).collect();
        let found = if !exact.is_empty() {
            pick(exact)
        } else {
            let words: Vec<&str> = want.split(' ').collect();
            skills
                .iter()
                .map(|(id, n)| {
                    let have = norm(n);
                    let hits = words.iter().filter(|w| have.split(' ').any(|h| h == **w)).count();
                    (*id, hits as f64 / words.len().max(have.split(' ').count()) as f64)
                })
                .filter(|(_, s)| *s >= 0.6)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(id, _)| id)
        };
        if let Some(id) = found {
            out.push((id, level));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_opt::{check_set, optimise};

    /// The demo plan on CM 21 (Capricorn Cup, Chukyo turf 1200 m): Oguri Cap, Pace.
    fn cm21() -> App {
        let mut a = App::for_tests();
        a.planner.demo(&a.roster, &a.kb, 100601);
        let cm = a.kb.umalator_presets.iter().find(|e| e.is_cm() && e.id == 21).cloned().expect("CM 21 preset");
        a.planner.plan.apply_cm(&a.kb, &cm);
        a.planner.plan.style = crate::knowledge::Style::Pace;
        a
    }

    fn small(input: &mut Input) {
        input.tune = Tune { keep: 6, step_runs: 15, verify_runs: 40, drop_runs: 15, endgame_sp: 250.0, min_gain: 0.02, swap_time: std::time::Duration::ZERO, swap_pool: 2 };
    }

    /// Rating / parent: the most evaluation points within the budget, pins in, bans out, one per
    /// group, and every pick worth points (skills.json's `grade`).
    #[test]
    fn rating_pick_maximises_points() {
        let mut a = cm21();
        assert!(a.kb.skills.values().filter(|s| s.grade > 0).count() > 500, "skills.json carries evaluation points");
        let rows: Vec<u32> = a.pick_rows().into_keys().collect();
        let best = *rows.iter().max_by_key(|id| a.kb.skills[id].grade).unwrap();
        let cheap = *rows.iter().filter(|id| **id != best && **id / 10 != best / 10).min_by_key(|id| a.kb.skills[id].sp).unwrap();
        a.planner.plan.pick_objective = PickObjective::Rating;
        a.planner.plan.banned_skills = vec![best];
        a.planner.plan.pinned_skills = vec![cheap];
        let ctx = egui::Context::default();
        a.auto_pick(&ctx);
        let (msg, warn) = a.planner.skill_status.clone().unwrap();
        assert!(!warn && msg.contains("evaluation points"), "{msg}");
        assert!(a.planner.pick_job.is_none(), "no racing for rating");
        let chosen = a.planner.plan.skills.clone();
        assert!(chosen.contains(&cheap) && !chosen.contains(&best));
        assert!(a.chosen_cost(&chosen, a.plan_career().as_ref()) <= a.pick_budget().unwrap() + 5.0);
        let groups: std::collections::HashSet<u32> = chosen.iter().map(|id| id / 10).collect();
        assert_eq!(groups.len(), chosen.len(), "one skill per group");
        // More points than the lengths pick spends the same SP on.
        let (_, points) = a.rating_pick();
        a.planner.plan.pick_objective = PickObjective::Lengths;
        a.planner.plan.banned_skills.clear();
        a.planner.plan.pinned_skills.clear();
        let lengths = a.recommend_skills(&rows);
        let lengths_points: i64 = lengths.iter().map(|id| i64::from(a.kb.skills[id].grade)).sum();
        assert!(points >= lengths_points, "{points} vs {lengths_points}");
    }

    /// Pinned skills are always bought, banned and CM-disabled ones never; the budget and the
    /// one-per-group rule hold.
    #[test]
    fn optimiser_rules() {
        let mut a = cm21();
        let rows: Vec<u32> = a.pick_rows().into_keys().collect();
        let ctx = a.pick_context();
        let free: Vec<u32> = rows.iter().copied().filter(|id| a.pick_excluded_in(&ctx, *id).is_none()).collect();
        let (pin, ban, off) = (free[0], free[1], free[2]);
        // Tests use the bundled presets, without the official announcement's turned-off skills.
        for e in a.kb.umalator_presets.iter_mut().filter(|e| e.is_cm() && e.id == 21) {
            e.disabled_skills = vec![off];
        }
        a.planner.plan.pinned_skills = vec![pin];
        a.planner.plan.banned_skills = vec![ban];
        a.planner.plan.sp_budget = Some(900);
        let (mut input, names) = a.skill_opt_input().unwrap();
        small(&mut input);
        assert!(input.candidates.iter().all(|c| c.id != ban), "banned skills aren't candidates");
        assert!(input.candidates.iter().any(|c| c.id == pin && c.pinned));
        let disabled = a.plan_cm().map(|e| e.disabled_skills.clone()).unwrap_or_default();
        assert_eq!(disabled, vec![off]);
        assert!(input.candidates.iter().all(|c| !disabled.contains(&c.id)), "CM-disabled skills aren't candidates");
        // Keep the run short: the pinned one and a dozen more.
        let keep: Vec<Candidate> = input.candidates.iter().filter(|c| c.pinned).chain(input.candidates.iter().filter(|c| !c.pinned).take(12)).cloned().collect();
        input.candidates = keep;
        let out = optimise(&input, &names, &Progress::default(), &Phase::default());
        assert!(out.chosen.iter().any(|id| *id == pin || a.lower_version(*id) == Some(pin)), "the pinned skill (or its gold) is bought");
        assert!(!out.chosen.contains(&ban));
        assert!(out.spent <= 900.5, "spent {}", out.spent);
        let mut groups: Vec<u32> = out.chosen.iter().map(|id| id / 10).collect();
        groups.sort_unstable();
        groups.dedup();
        assert_eq!(groups.len(), out.chosen.len(), "one skill per group");
        assert!(out.verify.is_some() && !out.log.is_empty());
    }

    /// With the meta influence Off, the sheet changes nothing: the same pick as with no backing.
    #[test]
    fn meta_off_is_simulation_only() {
        let mut a = cm21();
        a.planner.plan.meta_influence = MetaInfluence::Off;
        let (mut input, names) = a.skill_opt_input().unwrap();
        small(&mut input);
        input.candidates.truncate(12);
        let with_meta = optimise(&input, &names, &Progress::default(), &Phase::default());
        for c in &mut input.candidates {
            c.meta = 0.0;
        }
        let without = optimise(&input, &names, &Progress::default(), &Phase::default());
        assert_eq!(with_meta.chosen, without.chosen);
    }

    /// The old pick from the old chart (no wit checks or rushing), meta first.
    fn old_pick(a: &mut App, input: &Input) -> Vec<u32> {
        let pool: Vec<u32> = a.plan_skill_candidates().into_keys().filter(|id| input.data.skills.contains_key(id)).collect();
        let rows = crate::sim::run::run_chart(&input.data, &input.course, &input.race, &input.base, &pool, 1234.0, false, &Progress::default());
        a.planner.skill_values = Some(crate::tab_planner::SkillValues {
            key: a.plan_key(),
            rows: rows.into_iter().map(|r| (r.id, r)).collect(),
            stats_per_length: None,
            sp_value: None,
            label: "old chart".into(),
        });
        let ids: Vec<u32> = a.plan_skill_candidates().into_keys().collect();
        a.recommend_skills_v1(&ids)
    }

    /// Measure first: the old auto-pick's single gains added up, against what the set gains in
    /// realistic races (same seed). `cargo test --release auto_pick_baseline -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn auto_pick_baseline() {
        let mut a = cm21();
        let (mut input, _) = a.skill_opt_input().unwrap();
        input.tune = Tune::default();
        let old = old_pick(&mut a, &input);
        let singles: f64 = old.iter().filter_map(|id| a.planner.skill_values.as_ref()?.rows.get(id).map(|r| r.mean)).sum();
        let check = check_set(&input, &old);
        eprintln!("old pick: {} skills, {:.0} SP", old.len(), a.chosen_cost(&old, a.plan_career().as_ref()));
        for id in &old {
            eprintln!("  {}", a.kb.skill_name(*id));
        }
        eprintln!("single gains (old chart) add up to {singles:.2} L; in 1000 realistic races the set gains {:+.2} L (95%: {:+.2} to {:+.2})", check.mean, check.lo, check.hi);
    }

    /// The optimiser's verified gain on the CM 21 demo beats the old pick's, on the same seed, in
    /// under 60 s. `cargo test --release optimiser_beats_the_old_pick -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn optimiser_beats_the_old_pick() {
        let mut a = cm21();
        let (mut input, _) = a.skill_opt_input().unwrap();
        input.tune = Tune::default();
        let old = old_pick(&mut a, &input);
        let old_check = check_set(&input, &old);
        // Like for like: the old pick's skills the simulator can't value (debuffs, position skills)
        // are pinned, as a user who wants them would; auto-pick leaves those to the user.
        a.planner.plan.pinned_skills = old.iter().copied().filter(|id| a.skill_unsimulated(*id).is_some()).collect();
        eprintln!("pinned: {:?}", a.planner.plan.pinned_skills.iter().map(|id| a.kb.skill_name(*id)).collect::<Vec<_>>());
        let (mut input, names) = a.skill_opt_input().unwrap();
        input.tune = Tune::default();
        let start = Instant::now();
        let out = optimise(&input, &names, &Progress::default(), &Phase::default());
        let secs = start.elapsed().as_secs_f64();
        for line in &out.log {
            eprintln!("{line}");
        }
        let new = out.verify.clone().expect("a verified set");
        eprintln!("old: {} skills, {:+.2} L (95%: {:+.2} to {:+.2})", old.len(), old_check.mean, old_check.lo, old_check.hi);
        eprintln!(
            "new: {} skills for {:.0} SP, {:+.2} L (95%: {:+.2} to {:+.2}); singles add up to {:.2} L; {secs:.1} s",
            out.chosen.len(),
            out.spent,
            new.mean,
            new.lo,
            new.hi,
            new.singles_sum
        );
        assert!(new.mean >= old_check.mean, "new {:.2} L < old {:.2} L", new.mean, old_check.mean);
        assert!(secs < 60.0, "took {secs:.1} s");
    }

    #[test]
    fn conditions_match_whole_names() {
        assert!(uses_condition("is_overtake==1&phase>=2", "is_overtake"));
        assert!(!uses_condition("blocked_front_continuetime>=1", "blocked_front"));
        assert!(uses_condition("blocked_front_continuetime>=1", "blocked_front_continuetime"));
    }

    #[test]
    fn pasted_shop_lines() {
        let app = App::for_tests();
        let corner = app.kb.skills.values().find(|k| k.name == "Corner Adept ○").map(|k| k.id).expect("Corner Adept ○");
        let found = match_skill_lines(&app.kb, "Corner Adept ○ Lv3\n\nnot a skill at all\n");
        assert_eq!(found, vec![(corner, 3)]);
    }
}
