//! The Lineage page's One runner view: one runner's parent plan (`plans/parents.json`), step by
//! step. The current step's Next career card and its progress against the odds come first; past
//! the step's bad-luck mark the Stuck panel opens (settle with the closest uma, try another
//! route, or keep going), each choice previewed before it changes anything.
//! The step list, the plan's choices (pins and steps set aside) and its history fill the
//! overview pane.

use crate::App;
use crate::parentage::brief::Brief;
use crate::parentage::jobs::{Avoid, Run};
use crate::parentage::track::{self, ParentPlan};
use crate::parentage::{Goal, uma_name};
use crate::theme::{pal, size};
use crate::ui_util::semibold;
use eframe::egui::{self, CornerRadius, RichText, Stroke, vec2};

/// What a Stuck panel preview would do, waiting for the player's go-ahead.
#[derive(Clone)]
pub(crate) enum Pending {
    Settle(u64),
    Route,
}

#[derive(Default)]
pub(crate) struct RunnerState {
    pub loaded: bool,
    pub plan: Option<ParentPlan>,
    /// The goal picker's choices: runner, surface (0 Turf, 1 Dirt), distance (0-3), style (0-3).
    pub pick: (Option<u32>, usize, usize, usize),
    /// The current step's brief, keyed by the step it was made for.
    pub brief: Option<(String, Brief)>,
    /// A previewed choice: what it is, the steps after, and the planner's words.
    pub preview: Option<(Pending, Vec<Run>, String)>,
    pub stuck_open: Option<bool>,
    pub message: Option<String>,
    /// The "What to look for" panel: shown, and its Add form (uma, pink stars per wanted
    /// aptitude, whether she carries the wanted unique).
    pub show_borrow: bool,
    pub add_uma: Option<u32>,
    pub add_stars: Vec<u8>,
    pub add_green: bool,
    /// Each settle candidate's cost: careers left if kept, by (step, finished uma id).
    pub settle_costs: std::collections::HashMap<(usize, u64), f64>,
    /// How to raise the plan: measured on a worker after the plan is made or changed.
    pub ceiling_due: bool,
    pub ceiling_job: Option<std::sync::mpsc::Receiver<(Vec<crate::parentage::ceiling::Lever>, crate::parentage::ceiling::Tried)>>,
}

impl App {
    pub(crate) fn lineage_runner(&mut self, ui: &mut egui::Ui) {
        self.runner_ceiling_poll(ui.ctx());
        // A runner's plan is built from the account too: set it up first.
        if self.lineage.runner.plan.is_none() && self.setup_gate(ui) {
            return;
        }
        if !self.lineage.runner.loaded {
            self.lineage.runner.loaded = true;
            self.lineage.runner.plan = self.root.as_deref().and_then(track::load);
            // UMALORD_RUNNER=<outfit>[:<target>,…]: a plan in memory, for screenshots (targets are
            // APTITUDES indices; her best surface, distance and style when left out).
            if self.lineage.runner.plan.is_none()
                && let Ok(v) = std::env::var("UMALORD_RUNNER")
            {
                let (id, targets) = v.split_once(':').map_or((v.as_str(), None), |(a, b)| (a, Some(b.split(',').filter_map(|t| t.parse().ok()).collect())));
                if let Ok(id) = id.parse::<u32>() {
                    self.lineage.runner.plan = self.plan_for(id, targets).ok();
                    // UMALORD_AFTER=<date>: the steps began then, so older finished umas count.
                    if let (Some(plan), Ok(after)) = (&mut self.lineage.runner.plan, std::env::var("UMALORD_AFTER")) {
                        for s in &mut plan.steps {
                            s.after = after.clone();
                        }
                    }
                }
            }
        }
        if self.lineage.runner.plan.is_none() {
            self.goal_picker(ui);
            return;
        }
        // UMALORD_BORROW=1: open the What to look for panel, for screenshots.
        if std::env::var("UMALORD_BORROW").is_ok() && self.lineage.runner.message.is_none() {
            self.lineage.runner.show_borrow = true;
        }
        // UMALORD_PREVIEW=route: open the another-route preview, for screenshots.
        if std::env::var("UMALORD_PREVIEW").is_ok_and(|v| v == "route") && self.lineage.runner.preview.is_none() && self.lineage.runner.message.is_none() {
            let (inv, vets) = (self.parent_inventory(), self.veterans());
            if let Some(plan) = &self.lineage.runner.plan {
                match plan.preview_another_route(&self.kb, &inv, &vets, &crate::ui_util::today()) {
                    Ok((after, msg)) => self.lineage.runner.preview = Some((Pending::Route, after, msg)),
                    Err(e) => self.lineage.runner.message = Some(e),
                }
            }
        }
        let plan = self.lineage.runner.plan.clone().unwrap_or_else(|| unreachable!());
        let vets = self.veterans();
        let inv = self.parent_inventory();
        // The current step's brief, made once per step.
        let current = plan.current();
        if let Some(i) = current {
            let key = format!("{i}:{:?}", plan.steps[i].run);
            if self.lineage.runner.brief.as_ref().is_none_or(|(k, _)| *k != key) {
                let b = self.parent_brief(&plan.goal, &plan.steps[i].run, &plan.inventory(&self.kb, &inv), i);
                self.lineage.runner.brief = Some((key, b));
            }
        }
        let brief = self.lineage.runner.brief.as_ref().map(|(_, b)| b.clone()).filter(|_| current.is_some());
        let p = pal(ui);
        self.panes_with(
            ui,
            0.62,
            |this, ui| {
                ui.horizontal_wrapped(|ui| {
                    this.avatar(ui, plan.goal.runner, 32.0);
                    ui.label(semibold(format!("Parent plan for {}", uma_name(&this.kb, plan.goal.runner)), size::XL));
                    ui.label(RichText::new(format!("{} at A at the start", plan.goal.targets.iter().map(|t| crate::parentage::APTITUDES[*t]).collect::<Vec<_>>().join(", "))).size(13.0).color(p.weak));
                    if ui.small_button("Start a new plan").on_hover_text("The current plan moves to plans/past-parent-plans").clicked() {
                        if let Some(root) = this.root.clone() {
                            let _ = track::cancel(&root, &crate::ui_util::today());
                        }
                        this.lineage.runner = RunnerState { loaded: true, ..Default::default() };
                    }
                });
                this.constraints_strip(ui, &plan);
            },
            |this, ui| {
                if let Some(m) = this.lineage.runner.message.clone() {
                    crate::ui_util::card(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(m).size(13.0));
                            if this.lineage.runner.plan.as_ref().is_some_and(|p| p.before.is_some()) && ui.button("↶ Undo").clicked() {
                                this.runner_do(|plan, _, _, _, today| plan.undo(today));
                            }
                        });
                    });
                    ui.add_space(8.0);
                }
                if let Some(why) = &plan.blocked {
                    crate::ui_util::card(ui, |ui| {
                        ui.label(semibold("The plan is waiting for your choice", 15.0));
                        ui.label(RichText::new(why).size(13.0).color(crate::lineage_kit::ink(p.warn, p.surface)));
                    });
                    ui.add_space(8.0);
                    this.look_for_legacy(ui, &plan);
                    ui.add_space(8.0);
                }
                match (current, &brief) {
                    (Some(i), Some(b)) => {
                        this.step_card(ui, &plan, i, &vets);
                        ui.add_space(12.0);
                        this.next_card(ui, b);
                    }
                    _ if plan.blocked.is_none() => {
                        crate::ui_util::card(ui, |ui| {
                            ui.label(semibold("Every step is done: her lineage is ready.", 15.0));
                            ui.label(RichText::new(format!("Train {} with her planned parents.", uma_name(&this.kb, plan.goal.runner))).size(13.0).color(p.weak));
                        });
                    }
                    _ => {}
                }
            },
            |this, ui| {
                let working = this.lineage.runner.ceiling_job.is_some() || this.lineage.runner.ceiling_due;
                this.runner_summary(ui, &plan);
                ui.add_space(12.0);
                let fresh = this.plan_ready()[2] == 0;
                if this.ceiling_card(ui, "runner", plan.ceiling.as_deref(), &plan.tried, fresh, working) {
                    this.lineage.runner.ceiling_due = true;
                }
                ui.add_space(12.0);
                this.step_list(ui, &plan);
                ui.add_space(12.0);
                crate::ui_util::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(semibold("History", 15.0));
                        if plan.before.is_some() && ui.small_button("↶ Undo the last change").clicked() {
                            this.runner_do(|plan, _, _, _, today| plan.undo(today));
                        }
                    });
                    for l in plan.log.iter().rev().take(8) {
                        ui.add(egui::Label::new(RichText::new(l).size(12.5).color(p.weak)).wrap());
                    }
                });
            },
        );
    }

    /// A plan for a runner: her best surface, distance and style unless given.
    fn plan_for(&mut self, runner: u32, targets: Option<Vec<usize>>) -> Result<ParentPlan, String> {
        let info = self.kb.umas.get(&runner).ok_or("unknown uma")?;
        let a = &info.aptitude;
        let targets = targets.unwrap_or_else(|| {
            use crate::knowledge::{Distance, Surface, grade_rank};
            let s = Surface::ALL.iter().position(|s| *s == *Surface::ALL.iter().max_by_key(|s| grade_rank(a.surface(**s))).unwrap_or(&Surface::Turf)).unwrap_or(0);
            let d = Distance::ALL.iter().enumerate().max_by_key(|(_, d)| grade_rank(a.distance(**d))).map_or(2, |x| x.0);
            vec![s, 2 + d, 6 + a.best_style() as usize]
        });
        let goal = Goal { runner, targets, label: format!("One runner: {}", uma_name(&self.kb, runner)), ..Default::default() };
        let inv = self.parent_inventory();
        let p = crate::parentage::jobs::propose(&self.kb, &goal, &inv);
        if !p.impossible.is_empty() {
            return Err("Her grades are too far below A for parents to lift.".into());
        }
        Ok(ParentPlan::new(goal, &p, &self.veterans(), &crate::ui_util::today()))
    }

    fn goal_picker(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        crate::ui_util::card(ui, |ui| {
            ui.label(semibold("Plan one runner's parents", 17.0));
            ui.label(RichText::new("Pick a runner and what must start at A. The planner works out the parent careers, cheapest first, and tracks them as you import finished umas.").size(13.0).color(p.weak));
            ui.add_space(8.0);
            let (mut runner, mut surface, mut distance, mut style) = self.lineage.runner.pick;
            let picked_before = runner;
            let owned: Vec<u32> = self.roster.umas.iter().map(|u| u.id).collect();
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("runner_pick").width(260.0).selected_text(runner.map_or("Runner…".into(), |r| uma_name(&self.kb, r))).show_ui(ui, |ui| {
                    for id in &owned {
                        ui.selectable_value(&mut runner, Some(*id), uma_name(&self.kb, *id));
                    }
                });
                for (label, value, options) in [("Surface", &mut surface, &["Turf", "Dirt"][..]), ("Distance", &mut distance, &["Sprint", "Mile", "Medium", "Long"][..]), ("Style", &mut style, &["Front Runner", "Pace Chaser", "Late Surger", "End Closer"][..])] {
                    ui.label(RichText::new(label).size(12.5).color(p.weak));
                    egui::ComboBox::from_id_salt(label).selected_text(options[*value]).show_ui(ui, |ui| {
                        for (k, o) in options.iter().enumerate() {
                            ui.selectable_value(value, k, *o);
                        }
                    });
                }
            });
            // A newly chosen runner starts at her own best grades.
            if runner != picked_before
                && let Some(a) = runner.and_then(|r| self.kb.umas.get(&r)).map(|u| u.aptitude.clone())
            {
                use crate::knowledge::{Distance, Surface, grade_rank};
                surface = Surface::ALL.iter().enumerate().max_by_key(|(_, s)| grade_rank(a.surface(**s))).map_or(0, |x| x.0);
                distance = Distance::ALL.iter().enumerate().max_by_key(|(_, d)| grade_rank(a.distance(**d))).map_or(2, |x| x.0);
                style = a.best_style() as usize;
            }
            self.lineage.runner.pick = (runner, surface, distance, style);
            ui.add_space(8.0);
            let go = ui.add_enabled(runner.is_some(), crate::ui_util::primary_button(ui, "Plan her parents", 14.0)).on_disabled_hover_text("Pick a runner first");
            if let Some(r) = runner.filter(|_| go.clicked()) {
                match self.plan_for(r, Some(vec![surface, 2 + distance, 6 + style])) {
                    Ok(plan) => {
                        if let Some(root) = &self.root {
                            let _ = track::save(root, &plan);
                        }
                        self.lineage.runner.plan = Some(plan);
                        self.lineage.runner.ceiling_due = true;
                    }
                    Err(e) => self.lineage.runner.message = Some(e),
                }
            }
            if let Some(m) = &self.lineage.runner.message {
                ui.label(RichText::new(m).color(p.warn));
            }
        });
    }

    /// Run an action on the plan, save it, and show what it said.
    /// Measure how to raise the runner's plan on a worker, after it was made or changed (the
    /// only time this simulates), and keep the result with the plan.
    fn runner_ceiling_poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.lineage.runner.ceiling_job {
            match rx.try_recv() {
                Ok((v, tried)) => {
                    self.lineage.runner.ceiling_job = None;
                    if let Some(plan) = &mut self.lineage.runner.plan {
                        plan.ceiling = Some(v);
                        plan.tried = tried;
                        if let Some(root) = &self.root {
                            let _ = track::save(root, plan);
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
                Err(_) => self.lineage.runner.ceiling_job = None,
            }
        }
        if self.lineage.runner.ceiling_due && self.lineage.runner.ceiling_job.is_none() {
            self.lineage.runner.ceiling_due = false;
            let Some(plan) = self.lineage.runner.plan.clone() else { return };
            let root = self.root.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx2 = ctx.clone();
            std::thread::spawn(move || {
                let mut app = crate::agent::mcp::headless_app(root);
                let _ = tx.send(app.runner_plan_ceiling(&plan));
                ctx2.request_repaint();
            });
            self.lineage.runner.ceiling_job = Some(rx);
        }
    }

    fn runner_do(&mut self, f: impl FnOnce(&mut ParentPlan, &crate::knowledge::Knowledge, &crate::parentage::Inventory, &[crate::veterans::Veteran], &str) -> Result<String, String>) {
        let Some(mut plan) = self.lineage.runner.plan.take() else { return };
        let inv = self.parent_inventory();
        let vets = self.veterans();
        let msg = f(&mut plan, &self.kb, &inv, &vets, &crate::ui_util::today());
        if let Some(root) = &self.root {
            let _ = track::save(root, &plan);
        }
        self.lineage.runner.plan = Some(plan);
        self.lineage.runner.preview = None;
        self.lineage.runner.ceiling_due = true;
        self.lineage.runner.message = Some(match msg {
            Ok(m) | Err(m) => m,
        });
    }

    /// The pins and steps set aside, as removable chips.
    fn constraints_strip(&mut self, ui: &mut egui::Ui, plan: &ParentPlan) {
        if plan.pin.avoid.is_empty() && plan.pin.is_empty() {
            return;
        }
        let p = pal(ui);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Your choices:").size(12.5).color(p.weak));
            let mut remove: Option<usize> = None;
            for (i, a) in plan.pin.avoid.iter().enumerate() {
                let text = format!("✕ {} with #{} and #{} set aside", uma_name(&self.kb, a.trainee), a.legacies[0], a.legacies[1]);
                if ui.add(egui::Button::new(RichText::new(text).size(12.0)).corner_radius(CornerRadius::same(12))).on_hover_text("Bring this step back").clicked() {
                    remove = Some(i);
                }
            }
            if let Some(t) = plan.pin.trainee {
                ui.label(RichText::new(format!("📌 {} as the next trainee", uma_name(&self.kb, t))).size(12.0));
            }
            if let Some(i) = remove {
                self.runner_do(|plan, kb, inv, vets, today| {
                    let mut pin = plan.pin.clone();
                    let gone: Avoid = pin.avoid.remove(i);
                    let back = format!("Brought back {}'s step. ", uma_name(kb, gone.trainee));
                    plan.pin.avoid.clear();
                    Ok(back + &plan.set_pin(kb, inv, vets, pin, today))
                });
            }
        });
        ui.add_space(4.0);
    }

    /// The current step: what it needs, careers run against the odds, and the Stuck panel.
    fn step_card(&mut self, ui: &mut egui::Ui, plan: &ParentPlan, i: usize, vets: &[crate::veterans::Veteran]) {
        let p = pal(ui);
        let step = &plan.steps[i];
        let run = &step.run;
        let hits = plan.hits(vets);
        // UMALORD_CAREERS=<n> shows the card n careers in, for screenshots of the later states.
        let n = std::env::var("UMALORD_CAREERS").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(hits.len() as u32);
        let (typical, unlucky) = crate::parentage::verify::step_spread(run.p);
        let done_by_now = ((1.0 - crate::parentage::verify::still_going(run.p, n)) * 20.0).round() as u32;
        let past_mark = n >= unlucky;
        egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                crate::ui_util::pill(ui, RichText::new(format!("▶ Step {} of {}", i + 1, plan.steps.len())).size(12.0).color(p.on_accent), p.accent, Stroke::NONE);
                self.avatar(ui, run.trainee, 32.0);
                ui.label(semibold(format!("Train {}", uma_name(&self.kb, run.trainee)), 15.0));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Must have:").size(12.5).color(p.weak));
                if run.must.is_empty() {
                    ui.label(RichText::new("nothing: just finish her").size(12.5));
                }
                for w in &run.must {
                    let (kind, name, stars) = w.chip(&self.kb);
                    crate::lineage_kit::spark_chip(ui, crate::lineage_kit::Spark::of(kind), &name, 0, Some(stars), false, true);
                }
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                crate::lineage_kit::odds_dots(ui, run.p, p.accent, p.border);
                ui.label(RichText::new(format!("{} a career ({:.1}%). With the spark reroll, usually {typical}; 1 in 10 need {unlucky} or more.", crate::parentage::verify::one_in(run.p), run.p * 100.0)).size(13.0));
            });
            // The careers run so far as dots, a ring at the 9-in-10 mark.
            ui.horizontal(|ui| {
                let total = n.max(unlucky) + 1;
                let d = 10.0;
                let (rect, _) = ui.allocate_exact_size(vec2(total as f32 * (d + 3.0), d + 4.0), egui::Sense::hover());
                for k in 0..total {
                    let c = egui::pos2(rect.left() + d / 2.0 + k as f32 * (d + 3.0), rect.center().y);
                    if k < n {
                        ui.painter().circle_filled(c, d / 2.0, if k + 1 >= unlucky { crate::lineage_kit::ink(p.warn, p.surface) } else { p.weak });
                    } else {
                        ui.painter().circle_stroke(c, d / 2.0 - 0.5, Stroke::new(1.0, p.border));
                    }
                    if k + 1 == unlucky {
                        ui.painter().circle_stroke(c, d / 2.0 + 2.5, Stroke::new(1.5, crate::lineage_kit::ink(p.warn, p.surface)));
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                let mark = if past_mark { format!("Career {} · past the 9-in-10 mark", n + 1) } else { format!("Career {}", n + 1) };
                ui.label(semibold(mark, 13.0).color(if past_mark { crate::lineage_kit::ink(p.warn, p.surface) } else { p.text }));
                ui.label(RichText::new(format!("{done_by_now} of 20 players are done by now{}", if past_mark { "; you're one of the few still going" } else { "" })).size(13.0).color(p.weak));
                let open = self.lineage.runner.stuck_open.unwrap_or(past_mark);
                if ui.link(if open { "Hide" } else { "Stuck?" }).clicked() {
                    self.lineage.runner.stuck_open = Some(!open);
                }
            });
            if self.lineage.runner.stuck_open.unwrap_or(past_mark) {
                ui.add_space(8.0);
                self.stuck_panel(ui, plan, n, unlucky, vets);
            }
            if self.lineage.runner.show_borrow && plan.blocked.is_none() {
                ui.add_space(8.0);
                self.look_for_legacy(ui, plan);
            }
        });
    }

    /// Settle with the closest, try another route, or keep going: each previewed first.
    fn stuck_panel(&mut self, ui: &mut egui::Ui, plan: &ParentPlan, n: u32, unlucky: u32, vets: &[crate::veterans::Veteran]) {
        let p = pal(ui);
        let i = plan.current().unwrap_or(0);
        let run = plan.steps[i].run.clone();
        egui::Frame::new().fill(crate::lineage_kit::tint(p.warn, p.surface, 0.08)).stroke(Stroke::new(1.0, p.warn.gamma_multiply(0.5))).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semibold(format!("Stuck on step {}?", i + 1), 14.5));
            let panel_bg = crate::lineage_kit::tint(p.warn, p.surface, 0.08);
            ui.label(RichText::new(format!("You've run {n} careers. About 9 in 10 players are done by career {unlucky}; the other 1 in 10, like you, are still going.")).size(12.5).color(crate::lineage_kit::ink(p.weak, panel_bg)));
            ui.add_space(6.0);
            // A preview waiting for the go-ahead replaces the three choices.
            if let Some((what, after, msg)) = self.lineage.runner.preview.clone() {
                let before: f64 = plan.remaining().iter().map(Run::careers).sum();
                let now: f64 = after.iter().map(Run::careers).sum();
                ui.label(semibold(match &what { Pending::Settle(id) => format!("Settle step {} with #{id}", i + 1), Pending::Route => "Try another route".into() }, 13.5));
                // The step that would come next, as the planner has it.
                if let Some(first) = after.first() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("New next step:").size(12.5).color(p.weak));
                        self.avatar(ui, first.trainee, 24.0);
                        ui.label(semibold(uma_name(&self.kb, first.trainee), 13.0));
                        if first.legacies != [0, 0] {
                            ui.label(RichText::new("with").size(12.5).color(p.weak));
                            for (id, uma) in first.legacies.iter().zip(first.legacy_umas) {
                                self.avatar(ui, uma, 20.0);
                                ui.label(RichText::new(format!("#{id}")).size(12.5));
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for w in &first.must {
                            let (kind, name, stars) = w.chip(&self.kb);
                            crate::lineage_kit::spark_chip(ui, crate::lineage_kit::Spark::of(kind), &name, 0, Some(stars), false, true);
                        }
                        crate::lineage_kit::odds_dots(ui, first.p, p.accent, p.border);
                        ui.label(RichText::new(crate::parentage::verify::one_in(first.p)).size(12.5));
                    });
                } else {
                    ui.label(RichText::new("No steps left after this: her lineage would be ready.").size(12.5));
                }
                ui.add(egui::Label::new(RichText::new(&msg).size(12.5).color(p.weak)).wrap());
                // The steps left, before and after, side by side.
                let steps_list = |this: &Self, ui: &mut egui::Ui, title: &str, steps: &[Run]| {
                    ui.label(RichText::new(title).size(12.0).strong().color(p.weak));
                    if steps.is_empty() {
                        ui.label(RichText::new("none: her lineage is ready").size(12.5));
                    }
                    for r in steps {
                        ui.horizontal_wrapped(|ui| {
                            this.avatar(ui, r.trainee, 20.0);
                            ui.label(RichText::new(crate::parentage::uma_name(&this.kb, r.trainee)).size(12.5));
                            let (typical, _) = crate::parentage::verify::step_spread(r.p);
                            ui.label(RichText::new(format!("usually {typical}")).size(12.0).color(p.weak));
                        });
                    }
                };
                let old_steps = plan.remaining();
                ui.columns(2, |c| {
                    c[0].with_layout(egui::Layout::top_down(egui::Align::Min), |ui| steps_list(self, ui, "BEFORE", &old_steps));
                    c[1].with_layout(egui::Layout::top_down(egui::Align::Min), |ui| steps_list(self, ui, "AFTER", &after));
                });
                let saving = before - now;
                ui.label(RichText::new(if saving.abs() < 0.5 { format!("No faster: about {before:.0} careers left either way, with a fresh chance of luck.") } else if saving > 0.0 { format!("Careers left: about {before:.0} → about {now:.0} ({saving:.0} fewer).") } else { format!("Careers left: about {before:.0} → about {now:.0} ({:.0} more).", -saving) }).size(13.0));
                ui.label(RichText::new("Nothing changes until you press the button.").size(12.0).color(p.weak));
                ui.horizontal(|ui| {
                    let label = match &what { Pending::Settle(_) => "Settle with her", Pending::Route => "Use this route" };
                    if ui.add(crate::ui_util::primary_button(ui, label, 13.0)).clicked() {
                        match what {
                            Pending::Settle(id) => self.runner_do(move |plan, kb, inv, vets, today| plan.settle(kb, inv, vets, id, today)),
                            Pending::Route => self.runner_do(|plan, kb, inv, vets, today| plan.another_route(kb, inv, vets, today)),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.lineage.runner.preview = None;
                    }
                });
                return;
            }
            let cols = if ui.available_width() >= 720.0 { 3 } else { 1 };
            let settle_ui = |this: &mut Self, ui: &mut egui::Ui| {
                ui.label(semibold("1 · Settle with your closest", 13.5));
                prose(ui, "Keep the one that came closest. The steps before stay done; the rest is planned again from her actual sparks.");
                let closest = plan.closest(vets);
                if closest.is_empty() {
                    ui.label(RichText::new(format!("No {} finished since this step began.", uma_name(&this.kb, run.trainee))).size(12.5).color(crate::lineage_kit::ink(p.weak, crate::lineage_kit::tint(p.warn, p.surface, 0.08))));
                }
                // What keeping each would leave to do, worked out once per candidate.
                for h in closest.iter().take(3) {
                    if !this.lineage.runner.settle_costs.contains_key(&(i, h.id)) {
                        let inv = this.parent_inventory();
                        let left = plan.preview_settle(&this.kb, &inv, vets, h.id, &crate::ui_util::today()).map_or(f64::NAN, |(after, _)| after.iter().map(Run::careers).sum());
                        this.lineage.runner.settle_costs.insert((i, h.id), left);
                    }
                }
                for h in closest.iter().take(3) {
                    let own = vets.iter().find(|v| v.trained_chara_id == h.id).map(|v| v.factors()).unwrap_or_default();
                    egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            this.avatar(ui, run.trainee, 24.0);
                            ui.label(semibold(format!("#{} {}", h.id, uma_name(&this.kb, run.trainee)), 13.0));
                        });
                        // Her sparks against the must-haves: filled what she has, targets for the
                        // rest, red where she's short.
                        ui.horizontal_wrapped(|ui| {
                            for w in &run.must {
                                let (kind, name, wanted) = w.chip(&this.kb);
                                let short = w.short(&own);
                                let have = wanted.saturating_sub(short);
                                crate::lineage_kit::spark_chip(ui, crate::lineage_kit::Spark::of(kind), &name, have, Some(wanted), short > 0, true);
                                if short > 0 {
                                    ui.add(egui::Label::new(RichText::new(format!("{name} {have}★, wanted {wanted}★")).size(12.0).color(crate::lineage_kit::ink(p.warn, p.surface))).wrap_mode(egui::TextWrapMode::Extend));
                                }
                            }
                        });
                        ui.label(RichText::new(if h.same_parents { "✔ ran with the step's legacies" } else { "✖ ran with other legacies" }).size(12.0).color(p.weak));
                        ui.horizontal_wrapped(|ui| {
                            if let Some(left) = this.lineage.runner.settle_costs.get(&(i, h.id)).filter(|l| l.is_finite()) {
                                let txt = if left.abs() < 0.5 { "Keep her: then her lineage is ready".to_string() } else { format!("Keep #{}: about {left:.0} careers left after", h.id) };
                                ui.label(RichText::new(txt).size(12.5));
                            }
                            if ui.small_button("Preview").clicked() {
                                let inv = this.parent_inventory();
                                match plan.preview_settle(&this.kb, &inv, vets, h.id, &crate::ui_util::today()) {
                                    Ok((after, msg)) => this.lineage.runner.preview = Some((Pending::Settle(h.id), after, msg)),
                                    Err(e) => this.lineage.runner.message = Some(e),
                                }
                            }
                        });
                    });
                    ui.add_space(4.0);
                }
            };
            let route_ui = |this: &mut Self, ui: &mut egui::Ui| {
                ui.label(semibold("2 · Try another route", 13.5));
                prose(ui, &format!("Set this step aside: the same goal, without {} and these two legacies. The steps before stay done.", uma_name(&this.kb, run.trainee)));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Preview the route").clicked() {
                        let inv = this.parent_inventory();
                        match plan.preview_another_route(&this.kb, &inv, vets, &crate::ui_util::today()) {
                            Ok((after, msg)) => this.lineage.runner.preview = Some((Pending::Route, after, msg)),
                            Err(e) => {
                                // No route of her own: say what a friend's legacy would need.
                                this.lineage.runner.message = Some(e);
                                this.lineage.runner.show_borrow = true;
                            }
                        }
                    }
                    let inv = this.parent_inventory();
                    let who = crate::parentage::jobs::borrow_wish(&this.kb, &plan.goal, &plan.inventory(&this.kb, &inv)).filter(|w| w.possible).and_then(|w| {
                        let n = w.charas.len();
                        w.charas.first().and_then(|(c, _)| this.kb.umas.values().filter(|u| crate::affinity::chara(u.id) == *c).map(|u| u.id).min()).map(|u| (u, n))
                    });
                    if let Some((u, _)) = who {
                        this.avatar(ui, u, 20.0);
                    }
                    let label = match who {
                        Some((u, n)) if n > 1 => format!("Borrow a friend's legacy: {}, or {} others", this.kb.umas[&u].name, n.min(6) - 1),
                        Some((u, _)) => format!("Borrow a friend's legacy: {}", this.kb.umas[&u].name),
                        None => "Borrow a friend's legacy".to_string(),
                    };
                    if ui.button(label).on_hover_text("What to look for among your friends' legacies").clicked() {
                        this.lineage.runner.show_borrow = !this.lineage.runner.show_borrow;
                    }
                });
            };
            let keep_ui = |this: &mut Self, ui: &mut egui::Ui| {
                ui.label(semibold("3 · Keep going", 13.5));
                let (_, tail) = crate::parentage::verify::step_spread(run.p);
                prose(ui, &format!("Each career still hits {}: bad luck doesn't change the odds. About 1 in 10 players need {tail} or more careers; that's luck, not a mistake.", crate::parentage::verify::one_in(run.p)));
                prose(ui, "Costs nothing to decide; this closes until you open it again.");
                if ui.add(crate::ui_util::primary_button(ui, "Keep going", 13.0)).clicked() {
                    this.lineage.runner.stuck_open = Some(false);
                }
            };
            if cols == 3 {
                ui.columns(3, |c| {
                    c[0].with_layout(egui::Layout::top_down(egui::Align::Min), |ui| settle_ui(self, ui));
                    c[1].with_layout(egui::Layout::top_down(egui::Align::Min), |ui| route_ui(self, ui));
                    c[2].with_layout(egui::Layout::top_down(egui::Align::Min), |ui| keep_ui(self, ui));
                });
            } else {
                settle_ui(self, ui);
                ui.separator();
                route_ui(self, ui);
                ui.separator();
                keep_ui(self, ui);
            }
        });
    }

    /// What a friend's legacy must carry for the runner to start at A today, and a form to add
    /// the one the player finds.
    fn look_for_legacy(&mut self, ui: &mut egui::Ui, plan: &ParentPlan) {
        let p = pal(ui);
        let inv = plan.inventory(&self.kb, &self.parent_inventory());
        let Some(w) = crate::parentage::jobs::borrow_wish(&self.kb, &plan.goal, &inv) else { return };
        egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.5, p.accent.gamma_multiply(0.7))).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(semibold("What to look for in a friend's legacy", 15.0));
                ui.label(RichText::new("The planner can't see your friends' umas: here's the one that would do it.").size(12.5).color(p.weak));
            });
            ui.add_space(6.0);
            if !w.possible {
                ui.add(egui::Label::new(RichText::new("No single friend's legacy can bring it: it needs more than three pinks, and a legacy line has three at most (hers and her two parents', 3★ each).").size(13.0).color(crate::lineage_kit::ink(p.warn, p.surface))).wrap());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("The step needs").size(12.5).color(p.weak));
                    for (i, n) in &w.stars {
                        crate::lineage_kit::spark_chip(ui, crate::lineage_kit::Spark::Pink, crate::parentage::APTITUDES[*i], 0, Some((*n).min(3) as u8), false, true);
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("INSTEAD").size(11.5).strong().color(p.weak));
                ui.horizontal_wrapped(|ui| {
                    if ui.add(crate::ui_util::primary_button(ui, "Keep going with the plan", 13.0)).clicked() {
                        self.lineage.runner.show_borrow = false;
                    }
                    if ui.button("Change the goal").on_hover_text("Start a new plan: another runner, or fewer aptitudes at A").clicked() {
                        if let Some(root) = self.root.clone() {
                            let _ = track::cancel(&root, &crate::ui_util::today());
                        }
                        self.lineage.runner = RunnerState { loaded: true, ..Default::default() };
                    }
                });
                return;
            }
            let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                ui.horizontal_wrapped(|ui| {
                    ui.add_sized(vec2(150.0, 20.0), egui::Label::new(RichText::new(label).size(12.5).color(p.weak)));
                    add(ui);
                });
                ui.add_space(4.0);
            };
            let partner = inv.by_id(w.partner).map(|x| x.uma);
            row(ui, "She pairs with", &mut |ui| {
                if let Some(u) = partner {
                    self.avatar(ui, u, 24.0);
                    ui.label(RichText::new(format!("your #{} {}", w.partner, uma_name(&self.kb, u))).size(13.0));
                }
            });
            row(ui, "She must carry", &mut |ui| {
                for (i, n) in &w.stars {
                    crate::lineage_kit::spark_chip(ui, crate::lineage_kit::Spark::Pink, crate::parentage::APTITUDES[*i], 0, Some((*n).min(3) as u8), false, true);
                    if *n > 3 {
                        ui.label(RichText::new(format!("{n}★ in all, across her and her parents")).size(12.0).color(p.weak));
                    }
                }
                if let Some(u) = w.green {
                    let name = self.kb.sparks.get(&crate::parentage::green_spark(u, 1)).map_or_else(|| uma_name(&self.kb, u), |s| s.name.clone());
                    crate::lineage_kit::spark_chip(ui, crate::lineage_kit::Spark::Green, &name, 0, Some(1), false, true);
                }
                if w.stars.is_empty() && w.green.is_none() {
                    ui.label(RichText::new("nothing more: any legacy works").size(12.5));
                }
            });
            let words: Vec<String> = w.stars.iter().map(|(i, n)| format!("{} {n}★ or more", crate::parentage::APTITUDES[*i])).chain(w.green.map(|_| "her line's unique".to_string())).collect();
            if !words.is_empty() {
                ui.label(RichText::new(format!("{}, counting her and her two parents.", words.join(" and "))).size(12.5));
            }
            ui.add_space(4.0);
            let mut pick: Option<u32> = None;
            // Ranked characters as whole chips, wrapped by measuring (a Frame can't wrap whole).
            let chips: Vec<(usize, u32, String, u32)> = w.charas.iter().take(6).enumerate().filter_map(|(rank, (c, aff))| self.kb.umas.values().filter(|u| crate::affinity::chara(u.id) == *c).map(|u| u.id).min().map(|u| (rank, u, self.kb.umas[&u].name.clone(), *aff))).collect();
            ui.label(RichText::new("Best characters").size(12.5).color(p.weak));
            let max = ui.available_width();
            let widths: Vec<f32> = chips.iter().map(|(_, _, name, _)| ui.painter().layout_no_wrap(name.clone(), egui::FontId::proportional(12.5), p.text).size().x + 64.0).collect();
            let mut start = 0;
            while start < chips.len() {
                let (mut end, mut used) = (start, 0.0);
                while end < chips.len() && (end == start || used + widths[end] <= max) {
                    used += widths[end] + 8.0;
                    end += 1;
                }
                ui.horizontal(|ui| {
                    for (rank, u, name, aff) in &chips[start..end] {
                        let chosen = self.lineage.runner.add_uma.is_some_and(|a| crate::affinity::chara(a) == crate::affinity::chara(*u));
                        let r = egui::Frame::new().fill(if chosen { crate::lineage_kit::tint(p.accent, p.surface, 0.18) } else { p.raised }).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin { left: 4, right: 10, top: 2, bottom: 2 }).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{}", rank + 1)).size(11.5).color(p.weak));
                                self.avatar(ui, *u, 22.0);
                                ui.add(egui::Label::new(RichText::new(name).size(12.5)).wrap_mode(egui::TextWrapMode::Extend));
                            });
                        });
                        if r.response.interact(egui::Sense::click()).on_hover_text(format!("Affinity {aff} with your runner and her partner. Click to fill the form below.")).clicked() {
                            pick = Some(*u);
                        }
                    }
                });
                start = end;
            }
            if pick.is_some() {
                self.lineage.runner.add_uma = pick;
            }
            ui.label(RichText::new("Ranked by affinity; any outfit counts. Click one to fill the form below.").size(12.0).color(p.weak));
            ui.separator();
            // The Add form.
            ui.label(semibold("Found one? Add her", 13.5));
            let mut uma = self.lineage.runner.add_uma;
            let mut stars = self.lineage.runner.add_stars.clone();
            stars.resize(w.stars.len(), 0);
            let mut green = self.lineage.runner.add_green;
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("borrow_uma").width(240.0).selected_text(uma.map_or("Her uma…".into(), |u| uma_name(&self.kb, u))).show_ui(ui, |ui| {
                    let mut all: Vec<(String, u32)> = self.kb.umas.values().map(|u| (uma_name(&self.kb, u.id), u.id)).collect();
                    all.sort();
                    for (name, id) in all {
                        ui.selectable_value(&mut uma, Some(id), name);
                    }
                });
            });
            for (k, (i, need)) in w.stars.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{} ★ in her line", crate::parentage::APTITUDES[*i])).size(12.5));
                    for v in 0..=3u8 {
                        if ui.add(egui::Button::selectable(stars[k] == v, RichText::new(format!("{v}★")).size(12.5))).clicked() {
                            stars[k] = v;
                        }
                    }
                    if u32::from(stars[k]) >= *need {
                        ui.label(RichText::new("✔ enough").size(12.0).color(crate::lineage_kit::ink(p.good, p.surface)));
                    } else {
                        ui.label(RichText::new(format!("needs {need}★")).size(12.0).color(p.weak));
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                if w.green.is_some() {
                    crate::ui_util::UiExt::tick(ui, &mut green, "carries the unique");
                }
            });
            ui.label(RichText::new("Stars across her and her two parents. Borrow her again for every career that uses her; a lineage takes one friend's legacy at most.").size(12.0).color(p.weak));
            self.lineage.runner.add_uma = uma;
            self.lineage.runner.add_stars = stars.clone();
            self.lineage.runner.add_green = green;
            let go = ui.add_enabled(uma.is_some(), crate::ui_util::primary_button(ui, "Add her and plan again", 13.0)).on_disabled_hover_text("Pick her uma first");
            if let Some(u) = uma.filter(|_| go.clicked()) {
                // Her pinks as sparks of up to 3★ each; the planner counts her line's total.
                let mut own: Vec<u32> = Vec::new();
                for ((i, _), n) in w.stars.iter().zip(&stars) {
                    let mut left = *n;
                    while left > 0 {
                        let k = left.min(3);
                        own.push(crate::parentage::pink_spark(*i, k));
                        left -= k;
                    }
                }
                if let (Some(g), true) = (w.green, green) {
                    own.push(crate::parentage::green_spark(g, 1));
                }
                let b = crate::parentage::Borrow { uma: u, own, parents: [(0, vec![]), (0, vec![])] };
                self.runner_do(move |plan, kb, inv, vets, today| Ok(plan.add_borrow(kb, inv, vets, b, today)));
                self.lineage.runner.show_borrow = false;
            }
        });
    }

    /// Every step: done, settled, current or next.
    fn step_list(&mut self, ui: &mut egui::Ui, plan: &ParentPlan) {
        let p = pal(ui);
        let current = plan.current();
        crate::ui_util::card(ui, |ui| {
            ui.label(semibold(format!("Steps · {}", plan.steps.len()), 15.0));
            for (k, s) in plan.steps.iter().enumerate() {
                let (status, color) = match (s.finished(), s.settled, Some(k) == current) {
                    (true, true, _) => ("settled", crate::lineage_kit::ink(p.warn, p.raised)),
                    (true, false, _) => ("✔ done", crate::lineage_kit::ink(p.good, p.raised)),
                    (false, _, true) => ("▶ current", crate::lineage_kit::ink(p.accent, p.raised)),
                    _ => ("next", p.weak),
                };
                egui::Frame::new().fill(p.raised).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(semibold(format!("Step {}", k + 1), 13.0));
                        ui.label(RichText::new(status).size(12.5).color(color));
                        self.avatar(ui, s.run.trainee, 24.0);
                        ui.label(RichText::new(uma_name(&self.kb, s.run.trainee)).size(13.0));
                        for w in &s.run.must {
                            let (kind, name, stars) = w.chip(&self.kb);
                            crate::lineage_kit::spark_chip(ui, crate::lineage_kit::Spark::of(kind), &name, 0, Some(stars), false, true);
                        }
                        let (typical, _) = crate::parentage::verify::step_spread(s.run.p);
                        if !s.finished() {
                            ui.label(RichText::new(format!("usually {typical}")).size(12.5).color(p.weak));
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });
    }
}

/// A paragraph, ragged right.
fn prose(ui: &mut egui::Ui, text: &str) {
    // About 80 characters a line at most.
    let w = ui.available_width().min(580.0);
    ui.allocate_ui_with_layout(vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_max_width(w);
        ui.add(egui::Label::new(RichText::new(text).size(12.5)).wrap());
    });
}
