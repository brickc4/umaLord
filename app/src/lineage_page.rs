//! The Lineage page: "what's the next career I should run, and how?" for a Champions Meeting
//! team, a Team Trials team, or one runner. The planner (`parentage/`) does the work on a
//! worker thread with its own copy of the app's data (`agent::mcp::headless_app`), since a team
//! takes seconds to race; the page shows the Next career card first, then today's team, the
//! planned runners and the parent queue.

use crate::App;
use crate::parentage::brief::Brief;
use crate::parentage::saved::Built;
use crate::parentage::tt::TtTeam;
use crate::theme::{pal, size};
use crate::ui_util::semibold;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};

/// The two-pane breakpoint, the action pane's width and the gap (see `panes`).
const WIDE: f32 = 1200.0;
const ACTION_W: f32 = 480.0;
const PANE_GAP: f32 = 24.0;
/// The overview pane's content stops here; past it the extra width goes to the action pane
/// (the card the player works from), up to `ACTION_MAX`.
const RIGHT_CAP: f32 = 1240.0;
const ACTION_MAX: f32 = 1200.0;
/// Prose stops at about 80 characters a line (WCAG 1.4.8).
pub(crate) const PROSE_W: f32 = 580.0;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum LineageView {
    #[default]
    Overview,
    Cm,
    Tt,
    Runner,
}

impl LineageView {
    fn info(self) -> (&'static str, &'static str, &'static str) {
        match self {
            LineageView::Overview => ("🧭", "Overview", "Where each plan stands: Champions Meeting, Team Trials and one runner."),
            LineageView::Cm => ("🏆", "Champions Meeting", "Three runners for the next Champions Meeting, and the parent careers they need."),
            LineageView::Tt => ("🏟", "Team Trials", "Fifteen runners across the five races, an Ace for each, and the career that raises the team most."),
            LineageView::Runner => ("🌱", "One runner", "One runner's parent plan, step by step."),
        }
    }
}

#[derive(Default)]
pub(crate) struct LineageState {
    pub view: LineageView,
    /// Which upcoming CM, by position in the schedule.
    pub cm: usize,
    /// The saved team plans, a build in flight, and what's open (`lineage_plans.rs`).
    pub p: crate::lineage_plans::PlansUi,
    /// When the last build arrived (screenshots give its art a moment to load).
    pub done_at: Option<f64>,
    /// Which borrowed card's "What to look for" is open (a key per place).
    pub look_for: Option<String>,
    pub runner: crate::lineage_runner::RunnerState,
}

impl App {
    pub(crate) fn lineage_page(&mut self, ui: &mut egui::Ui) {
        self.plan_poll(ui.ctx());
        // UMALORD_PLAN=create (or building, to screenshot the log mid-way) makes the view's plan
        // on the first frame, as Create plan would; with a saved plan, building shows the plan
        // set aside while it's made again.
        if !self.lineage.p.env_done {
            self.lineage.p.env_done = true;
            if matches!(std::env::var("UMALORD_PLAN").as_deref(), Ok("create" | "building")) {
                if let Some(which) = self.view_which() {
                    let ctx = ui.ctx().clone();
                    self.plan_start(&ctx, which, Default::default());
                }
            }
            // UMALORD_PLAN=replan opens Re-plan's confirmation on the saved plan.
            if std::env::var("UMALORD_PLAN").as_deref() == Ok("replan") {
                self.lineage.p.confirm_replan = self.view_which();
            }
        }
        egui::Panel::top("lineage_bar")
            .frame(egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 18, top: 12, bottom: 10 }))
            .show(ui, |ui| self.lineage_bar(ui));
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 4, bottom: 12 })).show(ui, |ui| match self.lineage.view {
            LineageView::Overview => self.lineage_overview(ui),
            LineageView::Cm => self.lineage_cm(ui),
            LineageView::Tt => self.lineage_tt(ui),
            LineageView::Runner => self.lineage_runner(ui),
        });
    }

    fn lineage_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.set_min_height(34.0);
            ui.label(semibold("🌱 Lineage", 22.0));
            ui.add_space(14.0);
            for v in [LineageView::Overview, LineageView::Cm, LineageView::Tt, LineageView::Runner] {
                let (icon, label, what) = v.info();
                if ui.add(egui::Button::selectable(self.lineage.view == v, RichText::new(format!("{icon} {label}")).size(14.0)).min_size(vec2(0.0, 30.0))).on_hover_text(what).clicked() {
                    self.lineage.view = v;
                }
            }
        });
    }

    /// Whether the page is still building (screenshots wait for it).
    pub(crate) fn lineage_busy(&self, now: f64) -> bool {
        if std::env::var("UMALORD_PLAN").as_deref() == Ok("building") {
            return self.lineage.p.job.as_ref().is_some_and(|j| j.stages.len() < 3);
        }
        self.lineage.p.job.is_some() || self.lineage.done_at.is_some_and(|t| now - t < 0.5)
    }

    /// The page's two layouts, chosen once from the page's width:
    /// at 1200 px and wider, a fixed 480 px action pane (the Next career card) beside the
    /// overview, each scrolling on its own so the setup stays in view; narrower, one column,
    /// the card first.
    pub(crate) fn panes(&mut self, ui: &mut egui::Ui, head: impl FnOnce(&mut Self, &mut egui::Ui), action: impl FnOnce(&mut Self, &mut egui::Ui), overview: impl FnOnce(&mut Self, &mut egui::Ui)) {
        self.panes_with(ui, ACTION_W, head, action, overview);
    }

    /// `panes` with the action pane's width: fixed (the team views) or, at 1.0 or less, a share
    /// of the page (One runner, where the step is the main content and the overview is short).
    pub(crate) fn panes_with(&mut self, ui: &mut egui::Ui, action_w: f32, head: impl FnOnce(&mut Self, &mut egui::Ui), action: impl FnOnce(&mut Self, &mut egui::Ui), overview: impl FnOnce(&mut Self, &mut egui::Ui)) {
        let wide = ui.available_width() >= WIDE;
        let full = ui.available_width();
        let action_w = if action_w <= 1.0 {
            (full - PANE_GAP) * action_w
        } else {
            // The card is the detail the player works from, the overview the list they glance at
            // (list-detail layouts): the card takes 40% and every pixel past the overview's cap,
            // between 480 and 1,200; a width dragged by hand wins, until double-clicked away.
            let auto = (full * 0.40).max(full - RIGHT_CAP - PANE_GAP).clamp(action_w, ACTION_MAX);
            let dragged: Option<f32> = crate::collection_kit::setting::<Option<f32>>(ui.ctx(), "lineage_split");
            dragged.map_or(auto, |d| d.clamp(action_w, (full - 600.0).max(action_w)))
        };
        head(self, ui);
        ui.add_space(8.0);
        if wide {
            let h = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(action_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(action_w);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    let mut sa = egui::ScrollArea::vertical().id_salt("lineage_action").auto_shrink([false, false]);
                    // UMALORD_SCROLL holds the action pane scrolled down, for screenshots.
                    if let Some(y) = std::env::var("UMALORD_SCROLL").ok().and_then(|v| v.parse::<f32>().ok()) {
                        sa = sa.vertical_scroll_offset(y);
                    }
                    sa.show(ui, |ui| action(self, ui));
                });
                // The gap is a handle: drag to resize, double-click to go back to automatic.
                let (gap, resp) = ui.allocate_exact_size(vec2(PANE_GAP, h), Sense::click_and_drag());
                let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeHorizontal).on_hover_text("Drag to resize; double-click to reset");
                if resp.hovered() || resp.dragged() {
                    ui.painter().line_segment([pos2(gap.center().x, gap.top() + 4.0), pos2(gap.center().x, gap.bottom() - 4.0)], Stroke::new(2.0, pal(ui).accent.gamma_multiply(0.6)));
                }
                if resp.dragged() {
                    let w = (action_w + resp.drag_delta().x).clamp(320.0, (full - 600.0).max(320.0));
                    crate::collection_kit::set_setting(ui.ctx(), "lineage_split", Some(w));
                }
                if resp.double_clicked() {
                    crate::collection_kit::set_setting(ui.ctx(), "lineage_split", None::<f32>);
                }
                let rest = ui.available_width();
                ui.allocate_ui_with_layout(vec2(rest, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(rest);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    let mut sa = egui::ScrollArea::vertical().id_salt("lineage_overview").auto_shrink([false, false]);
                    // UMALORD_SCROLL_OVERVIEW holds the overview pane scrolled down, for screenshots.
                    if let Some(y) = std::env::var("UMALORD_SCROLL_OVERVIEW").ok().and_then(|v| v.parse::<f32>().ok()) {
                        sa = sa.vertical_scroll_offset(y);
                    }
                    sa.show(ui, |ui| {
                        ui.set_max_width(RIGHT_CAP.min(rest));
                        overview(self, ui)
                    });
                });
            });
        } else {
            egui::ScrollArea::vertical().id_salt("lineage_scroll").auto_shrink([false, false]).show(ui, |ui| {
                action(self, ui);
                ui.add_space(12.0);
                overview(self, ui);
            });
        }
    }

    fn lineage_cm(&mut self, ui: &mut egui::Ui) {
        let Some(which) = self.view_which() else {
            prose(ui, "No upcoming Champions Meeting is in the schedule yet.", 14.0, pal(ui).weak);
            return;
        };
        self.plan_view(ui, which);
    }

    fn lineage_tt(&mut self, ui: &mut egui::Ui) {
        self.plan_view(ui, crate::parentage::saved::Which::Tt);
    }

    /// A saved plan laid out: the open step's card in the action pane; the plan bar, what it's
    /// for, the steps, the 564 note, the team before and after and how it was chosen beside it.
    /// In one column the plan bar comes first and a step's card opens under its row.
    fn plan_view(&mut self, ui: &mut egui::Ui, which: crate::parentage::saved::Which) {
        // While the plan is made again, the old one is set aside: only the build log shows.
        let Some(plan) = self.team_plans().get(which).cloned().filter(|_| !self.plan_building(which)) else {
            self.plan_alone(ui, which);
            return;
        };
        let wide = ui.available_width() >= WIDE;
        let p = pal(ui);
        self.panes(
            ui,
            |this, ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.link(RichText::new("Overview").size(12.5)).clicked() {
                        this.lineage.view = LineageView::Overview;
                    }
                    ui.label(RichText::new("›").size(12.5).color(p.weak));
                    match &plan.built {
                        Built::Cm { team, .. } => {
                            let e = &team.event;
                            ui.label(semibold(format!("CM {} {} team", e.id, e.name), size::LG));
                            pill(ui, &e.course_name, p.accent);
                            ui.label(RichText::new(format!("Round 1 on {} · {} days away{}", crate::lineage_plans::when(&e.date), crate::tab_planner::days_apart(&e.date, &crate::ui_util::today()).max(0), if e.announced { "" } else { " (estimated)" })).size(12.5).color(p.weak));
                        }
                        Built::Tt { team, .. } => {
                            let runners: usize = team.rows.iter().map(|r| r.slots.len()).sum();
                            ui.label(semibold(format!("Team Trials · {runners} runners"), size::LG));
                            pill(ui, &format!("{} races × 3", team.rows.len()), p.accent);
                        }
                    }
                });
            },
            |this, ui| {
                if wide {
                    this.open_step_card(ui, which, &plan);
                } else {
                    this.plan_lead(ui, which);
                }
            },
            |this, ui| {
                if wide {
                    this.plan_lead(ui, which);
                }
                this.plan_summary(ui, &plan);
                ui.add_space(12.0);
                if this.ceiling_card(ui, &crate::parentage::saved::label(which), plan.live_ceiling().as_deref(), &plan.tried, plan.basis.vets.is_empty(), false) {
                    let ctx = ui.ctx().clone();
                    this.plan_start(&ctx, which, plan.choices_after());
                }
                ui.add_space(12.0);
                this.plan_steps(ui, which, &plan, wide);
                ui.add_space(12.0);
                if let Built::Tt { team, .. } = &plan.built {
                    this.tt_inherit_card(ui, team);
                }
                this.before_after(ui, &plan);
                let notes: Vec<String> = match &plan.built {
                    Built::Cm { team, .. } => team.notes.clone(),
                    Built::Tt { team, .. } => team.notes.clone(),
                };
                for n in &notes {
                    ui.add(egui::Label::new(RichText::new(format!("⚠ {n}")).size(12.5).color(pal(ui).warn)).wrap());
                }
                this.plan_chosen(ui, which);
            },
        );
    }

    /// Team Trials: 564 Escapades, which every runner should inherit, and who needs a friend's
    /// Summer Gold Ship for it.
    pub(crate) fn tt_inherit_card(&mut self, ui: &mut egui::Ui, team: &TtTeam) {
        let p = pal(ui);
        // 564 Escapades (the community's inherit for every runner): who needs a friend's Summer
        // Gold Ship as a parent, and on request what to look for and who to pair her with.
        let needs: Vec<(u32, String)> = team.rows.iter().flat_map(|r| r.slots.iter()).filter_map(|s| s.notes.iter().find(|n| n.contains("Summer Gold Ship")).map(|n| (s.trainee, n.split("pair her with ").nth(1).and_then(|x| x.split(" instead").next()).unwrap_or("").to_string()))).collect();
        if !needs.is_empty() {
            let key = "tt-564".to_string();
            let open = self.lineage.look_for.as_ref() == Some(&key);
            let mut toggle = false;
            crate::ui_util::card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("ONE SKILL EVERY RUNNER WANTS").size(12.0).strong().color(p.weak));
                ui.add_space(4.0);
                let runners: usize = team.rows.iter().map(|r| r.slots.len()).sum();
                ui.horizontal_wrapped(|ui| {
                    crate::lineage_kit::spark_chip(ui, crate::lineage_kit::Spark::Green, "564 Escapades", 0, Some(1), false, true);
                });
                ui.add_space(4.0);
                crate::lineage_steps::rich(
                    ui,
                    &[
                        crate::lineage_steps::Span::t("Most Team Trials players give every runner the skill "),
                        crate::lineage_steps::Span::b("564 Escapades"),
                        crate::lineage_steps::Span::t(". A runner can only learn it by "),
                        crate::lineage_steps::Span::term("inheriting", "inherit"),
                        crate::lineage_steps::Span::t(" it from "),
                        crate::lineage_steps::Span::b("Summer Gold Ship"),
                        crate::lineage_steps::Span::t(&format!(" as a parent. Borrow a friend's Summer Gold Ship for {} of your {runners} runners.", needs.len())),
                    ],
                    14.0,
                    PROSE_W,
                );
                if ui.link(if open { "Hide" } else { "What to look for" }).clicked() {
                    toggle = true;
                }
                if open {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        self.avatar(ui, 100702, 32.0);
                        ui.label(semibold(crate::parentage::uma_name(&self.kb, 100702), 13.5));
                        ui.label(RichText::new("must carry her unique, 564 Escapades, as a green spark").size(12.5));
                    });
                    ui.label(RichText::new("PAIR HER WITH, FOR EACH RUNNER").size(11.5).strong().color(p.weak));
                    for (t, partner) in &needs {
                        ui.horizontal_wrapped(|ui| {
                            self.avatar(ui, *t, 20.0);
                            ui.label(RichText::new(short_name(&self.kb, *t)).size(12.5));
                            ui.label(RichText::new(format!("→ {}", if partner.is_empty() { "a parent that keeps her targets at A" } else { partner.as_str() })).size(12.5).color(p.weak));
                        });
                    }
                    prose(ui, "Need a new friend? Look for a trainer whose legacies include Summer Gold Ship with 564 Escapades as a green spark. Borrow her again for every career that uses her.", 12.0, p.weak);
                }
            });
            if toggle {
                self.lineage.look_for = if open { None } else { Some(key) };
            }
            ui.add_space(10.0);
        }
    }

    /// The plan bar, and the build log under it while the plan is being made again.
    fn plan_lead(&mut self, ui: &mut egui::Ui, which: crate::parentage::saved::Which) {
        self.plan_bar(ui, which);
        ui.add_space(12.0);
        if self.plan_building(which) {
            self.build_log(ui, which);
            ui.add_space(12.0);
        }
    }

    /// A view with no saved plan: the build log while one is being made, else the empty state.
    fn plan_alone(&mut self, ui: &mut egui::Ui, which: crate::parentage::saved::Which) {
        // The same header a saved plan has, so the card sits the same distance below the bar.
        let p = pal(ui);
        ui.horizontal_wrapped(|ui| {
            if ui.link(RichText::new("Overview").size(12.5)).clicked() {
                self.lineage.view = LineageView::Overview;
            }
            ui.label(RichText::new("›").size(12.5).color(p.weak));
            let title = match which {
                crate::parentage::saved::Which::Cm(id) => self.kb.upcoming_cms(&crate::ui_util::today()).into_iter().find(|e| e.id == id).map_or(format!("CM {id}"), |e| format!("CM {} {} team", e.id, e.name)),
                crate::parentage::saved::Which::Tt => "Team Trials".to_string(),
            };
            ui.label(semibold(title, size::LG));
        });
        ui.add_space(8.0);
        egui::ScrollArea::vertical().id_salt("lineage_alone").auto_shrink([false, false]).show(ui, |ui| {
            if self.plan_building(which) {
                self.build_log(ui, which);
            } else {
                self.plan_empty(ui, which);
            }
        });
    }

    /// The Next career card: who and why; How (Independent
    /// Training with its setup panel, or a manual career); then parents, scenario, deck and
    /// skills beside stats, races and keep-if; the odds strip last.
    pub(crate) fn next_card(&mut self, ui: &mut egui::Ui, b: &Brief) {
        self.career_card(ui, b, None);
    }

    /// A career's card. As a plan's step it has a header tying it to the list ("Step 4 of 12 ·
    /// Special Week's own career · Medium"), and reads without "Next:" unless it's the next one.
    pub(crate) fn career_card(&mut self, ui: &mut egui::Ui, b: &Brief, head: Option<&crate::lineage_steps::StepHead>) {
        let p = pal(ui);
        let it = b.mode != "Manual career";
        // Two columns once the card is wide enough for two 360 px columns: setup, then the run.
        let wide = ui.available_width() >= 760.0;
        egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.5, p.accent)).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut shop = false;
            if let Some(h) = head {
                ui.label(semibold(&h.line, 13.0).color(p.accent));
                ui.horizontal_wrapped(|ui| {
                    crate::lineage_steps::kind_chip(ui, h.own);
                    ui.label(RichText::new(&h.context).size(12.0).color(p.weak));
                });
                ui.add_space(8.0);
            }
            let next = head.is_none_or(|h| h.next);
            // Who and why.
            ui.horizontal_top(|ui| {
                self.avatar(ui, b.trainee, 64.0);
                // Leave the button its room: the text wraps before it.
                let text_w = ui.available_width();
                ui.allocate_ui_with_layout(vec2(text_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_max_width(text_w);
                    // A plan's step already says what kind of career it is and who it's for.
                    if head.is_none() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(if next { "NEXT CAREER" } else { "CAREER" }).size(11.5).strong().color(p.accent));
                        match b.step {
                            Some(n) => pill(ui, &format!("Parent · step {n}"), p.accent),
                            None => pill(ui, "Runner", p.accent),
                        }
                        if let Some(r) = b.for_runner {
                            ui.label(RichText::new("for").size(12.5).color(p.weak));
                            self.avatar(ui, r, 18.0);
                            ui.label(RichText::new(crate::parentage::uma_name(&self.kb, r)).size(12.5));
                        }
                    });
                    }
                    let full = crate::parentage::uma_name(&self.kb, b.trainee);
                    let (name, outfit) = full.split_once(" [").map_or((full.clone(), String::new()), |(n, o)| (n.to_string(), format!("[{o}")));
                    // One left-aligned text that wraps as a whole, never mid-word.
                    let mut job = egui::text::LayoutJob::default();
                    let bold = |sz: f32| FontId::new(sz, egui::FontFamily::Name(crate::theme::SEMIBOLD.into()));
                    if next {
                        job.append("Next: ", 0.0, egui::TextFormat { font_id: bold(19.0), color: p.weak, ..Default::default() });
                    }
                    job.append(&name, 0.0, egui::TextFormat { font_id: bold(19.0), color: p.text, ..Default::default() });
                    job.append(&format!(" {outfit} "), 0.0, egui::TextFormat { font_id: FontId::proportional(17.0), color: p.weak, ..Default::default() });
                    job.append(&format!("as {}", style_article(b.style)), 0.0, egui::TextFormat { font_id: bold(19.0), color: p.text, ..Default::default() });
                    ui.add(egui::Label::new(job).wrap()).on_hover_text(crate::lineage_steps::style_hint(b.style.label()).unwrap_or_default());
                    ui.label(RichText::new(&b.why).size(13.0).color(p.weak));
                });
            });
            ui.add_space(10.0);
            // How.
            label_row(ui, "HOW", |ui| {
                ui.horizontal_wrapped(|ui| {
                    if it {
                        pill(ui, "⏳ Independent Training", p.weak);
                        ui.label(RichText::new("50 minutes in the background; sparks are what this career is for.").size(13.0));
                    } else {
                        pill(ui, "🏇 Manual career", p.accent);
                        ui.label(RichText::new("A runner races on her stats and skills; manual careers come out much stronger.").size(13.0));
                    }
                });
            });
            if it {
                self.it_setup(ui, b);
                ui.add_space(8.0);
            }
            // The body: two columns when there's room.
            let look_key = format!("card:{}", b.trainee);
            let look_open = self.lineage.look_for.as_ref() == Some(&look_key);
            let mut look_toggle = false;
            let mut left = |this: &Self, ui: &mut egui::Ui| {
                label_row(ui, "PARENTS", |ui| {
                    if b.legacies.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&b.parents).size(13.0)).wrap());
                    } else {
                        // Each parent says where she comes from: the player's own finished umas, or a
                        // friend's to borrow.
                        let mut items: Vec<Who> = b
                            .legacies
                            .iter()
                            .map(|(id, uma)| {
                                let borrowed = crate::parentage::is_borrowed(*id);
                                Who {
                                    uma: *uma,
                                    name: if borrowed { crate::parentage::uma_name(&this.kb, *uma) } else { format!("#{id} {}", crate::parentage::uma_name(&this.kb, *uma)) },
                                    note: if borrowed { "a friend's · borrow".into() } else { "yours · Veteran List".into() },
                                    note_color: if borrowed { p.accent } else { p.weak },
                                    hover: Some(if borrowed { "A friend's finished uma: pick her as a borrowed parent when the career starts.".to_string() } else { crate::lineage_steps::explain("vet").to_string() }),
                                }
                            })
                            .collect();
                        if let Some(u) = b.borrow_parent {
                            items.push(Who { uma: u, name: crate::parentage::uma_name(&this.kb, u), note: "a friend's · borrow".into(), note_color: p.accent, hover: Some("A friend's finished uma: pick her as a borrowed parent when the career starts.".into()) });
                        }
                        this.who_flow(ui, &items);
                    }
                    if let Some(n) = &b.inherit {
                        // A headline, then what it means (older plans have the headline only).
                        let (head, body) = n.split_once('\n').unwrap_or((n.as_str(), ""));
                        ui.add(egui::Label::new(semibold(format!("⚠ {head}"), 12.5).color(p.warn)).wrap());
                        if !body.is_empty() {
                            prose(ui, body, 12.5, p.weak);
                        }
                    }
                });
                label_row(ui, "SCENARIO", |ui| {
                    ui.label(semibold(&b.scenario, 13.5));
                });
                label_row(ui, "DECK", |ui| {
                    // Six to a row, as large as the row allows (72 px at most, the design's size).
                    let art = ((ui.available_width() - 5.0 * 8.0) / 6.0).clamp(40.0, 72.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        for c in &b.deck {
                            this.card_art(ui, *c, art, false);
                        }
                        if let Some(f) = b.friend {
                            this.card_art(ui, f, art, true);
                        }
                    });
                    // The borrowed card, and on request what to look for.
                    if let Some(f) = b.friend.and_then(|f| this.kb.cards.get(&f)) {
                        ui.add(egui::Label::new(RichText::new(format!("Borrow: {} [{}] (MLB)", f.uma, f.title)).size(12.5)).wrap()).on_hover_text(crate::lineage_steps::explain("mlb"));
                        if ui.link(if look_open { "Hide what to look for" } else { "What to look for" }).clicked() {
                            look_toggle = true;
                        }
                        if look_open {
                            this.borrow_look_for(ui, f.id, b.friend_why.as_deref(), &b.friend_alts);
                        }
                    }
                });
                label_row(ui, "SKILLS TO TARGET", |ui| {
                    let items: Vec<String> = b.skills.iter().take(10).enumerate().map(|(i, s)| format!("{} {}", i + 1, this.kb.skills.get(s).map_or(s.to_string(), |k| k.name.clone()))).collect();
                    pills(ui, &items, p.weak);
                    ui.label(RichText::new("In buying order: the first matter most.").size(12.0).color(p.weak));
                });
            };
            let mut right = |this: &Self, ui: &mut egui::Ui| {
                label_row(ui, "STATS", |ui| {
                    stat_table(ui, b);
                    for n in &b.stat_notes {
                        prose(ui, n, 12.5, p.text);
                    }
                });
                // A runner's races: the objectives she must run, then G1s that fit her for fans.
                if !b.objectives.is_empty() || !b.extra_races.is_empty() {
                    label_row(ui, "RACES", |ui| {
                        if !b.objectives.is_empty() {
                            ui.label(RichText::new("Her objectives (the career ends if she misses one):").size(12.5).color(p.weak));
                            for (when, race, goal) in &b.objectives {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(when).size(12.5).color(p.weak));
                                    ui.label(semibold(race, 13.0));
                                    ui.label(RichText::new(goal).size(12.5));
                                });
                            }
                        }
                        if !b.extra_races.is_empty() {
                            ui.add_space(4.0);
                            ui.label(RichText::new("Extra G1s that fit her, for fans (on turns without a strong friendship training):").size(12.5).color(p.weak));
                            let items: Vec<String> = b.extra_races.iter().map(|(n, d, g)| format!("{n} · {d} · {g}")).collect();
                            pills(ui, &items, p.weak);
                        }
                    });
                }
                if !b.agenda.is_empty() {
                    label_row(ui, "G1S TO WIN", |ui| {
                        for ((name, grades, chance), (_, links)) in b.agenda.iter().zip(this.g1_links(b)) {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(name).size(13.0));
                                pill(ui, &format!("{grades} · ~{chance}%"), p.good);
                                if links > 0 {
                                    ui.label(RichText::new(format!("+{} affinity", 3 * links)).size(12.0).color(p.weak));
                                }
                            });
                        }
                    });
                }
                label_row(ui, "KEEP HER IF", |ui| {
                    if b.must.is_empty() && b.nice.is_empty() {
                        for k in &b.keep {
                            ui.add(egui::Label::new(RichText::new(k).size(12.5)).wrap());
                        }
                    } else {
                        for (label, list) in [("must", &b.must), ("nice to have", &b.nice)] {
                            if list.is_empty() {
                                continue;
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.add_sized(vec2(78.0, 20.0), egui::Label::new(RichText::new(label).size(12.0).color(p.weak)));
                                for w in list.iter() {
                                    let (kind, name, stars) = w.chip(&this.kb);
                                    crate::lineage_kit::spark_chip(ui, crate::lineage_kit::Spark::of(kind), &name, 0, Some(stars), false, true);
                                }
                            });
                        }
                        ui.label(RichText::new("Close but short? Use the 30 TP spark reroll on that run before you settle.").size(12.0).color(p.weak));
                    }
                });
                // The Plan page's skill shop mode is what helps at the end of her career.
                if b.plan.is_some() {
                    label_row(ui, "AT THE END", |ui| {
                        prose(ui, "At the skill shop, enter her final stats, SP and the skills on offer: the helper races the options and picks the best set for her course.", 12.5, p.text);
                        if ui.button("Open the skill shop helper").on_hover_text("Career → Plan with her plan and its course, in skill shop mode").clicked() {
                            shop = true;
                        }
                    });
                }
            };
            if wide {
                // Two fixed columns (egui's `columns` let one column's content push into the other).
                let w = ui.available_width();
                let colw = ((w - 24.0) / 2.0).max(300.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(vec2(colw, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(colw);
                        ui.set_max_width(colw);
                        left(self, ui)
                    });
                    ui.add_space(24.0 - ui.spacing().item_spacing.x);
                    ui.allocate_ui_with_layout(vec2(colw, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(colw);
                        ui.set_max_width(colw);
                        right(self, ui)
                    });
                });
            } else {
                left(self, ui);
                right(self, ui);
            }
            if look_toggle {
                self.lineage.look_for = if look_open { None } else { Some(look_key.clone()) };
            }
            if shop {
                self.open_shop_helper(b);
            }
            // How races work, for when it's needed: collapsed, remembered.
            ui.separator();
            remembered_section(ui, "races_help", "How races work in this career", false, |ui| {
                ui.set_max_width(640.0);
                for r in b.races.iter().filter(|r| !r.starts_with("Win ")) {
                    ui.add(egui::Label::new(RichText::new(r).size(12.5)).wrap());
                }
                if !b.agenda.is_empty() {
                    ui.add(egui::Label::new(RichText::new("Each G1 her parents also won adds 3 affinity to that link; a G1 win may leave a race spark.").size(12.5)).wrap());
                    ui.add(egui::Label::new(RichText::new("Independent Training wins races on surface and distance grades alone: A/A starts at 110%, 10 less per grade below A, shown capped at 100%; a 3rd / 4th / 5th / 6th race in a row costs 10 / 25 / 35 / 50. In a manual career a 3rd race in a row risks her mood, a 4th her stats.").size(12.5)).wrap());
                }
            });
            // Odds.
            if let Some(pp) = b.p {
                ui.separator();
                label_row(ui, "ODDS", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        crate::lineage_kit::odds_dots(ui, pp, p.accent, p.border);
                        let (typical, unlucky) = crate::parentage::verify::step_spread(pp);
                        ui.label(RichText::new(format!("{} careers hit ({:.1}%). With the spark reroll, usually {typical}; 1 in 10 need {unlucky} or more.", capitalize(&crate::parentage::verify::one_in(pp)), pp * 100.0)).size(13.0));
                    });
                });
            }
        });
    }

    /// The Independent Training setup, laid out as the game's screen: focus, prioritized skills
    /// (ticked off as they're entered), the agenda with each race's win chance.
    fn it_setup(&mut self, ui: &mut egui::Ui, b: &Brief) {
        let p = pal(ui);
        egui::Frame::new().fill(p.raised).stroke(Stroke::new(1.0, p.border)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(semibold("⏳ Independent Training setup", 13.5));
                ui.label(RichText::new("Same order as the game's screen: copy it across, top to bottom.").size(12.0).color(p.weak));
            });
            ui.add_space(6.0);
            // Ticks last through the 50-minute run and app restarts, per plan.
            let plan_key = format!("{}:{:?}", b.trainee, &b.skills[..b.skills.len().min(10)]);
            let mut all: std::collections::BTreeMap<String, Vec<bool>> = crate::collection_kit::setting(ui.ctx(), "lineage_ticks");
            let mut ticks: Vec<bool> = all.get(&plan_key).cloned().filter(|t| t.len() == b.skills.len().min(10)).unwrap_or_else(|| vec![false; b.skills.len().min(10)]);
            let before = ticks.clone();
            let focus = b.focus.clone().unwrap_or_else(|| "Balanced".into());
            let cols = if ui.available_width() >= 900.0 { 3 } else { 1 };
            let mut draw = |ui: &mut egui::Ui, part: usize| match part {
                0 => {
                    ui.label(RichText::new("TRAINING FOCUS").size(11.5).strong().color(p.weak));
                    crate::lineage_kit::segmented(ui, &["Balanced", "Stamina", "Sprint"], &focus, p.accent, p.on_accent, p.weak, p.input);
                    let why = if b.step.is_some() { "Parent careers train Balanced, or Stamina when the step's stat aim is Stamina. Never Sprint: speed isn't what a parent run is for." } else { "Stamina when her floor is 900 or more, Sprint at 650 or less, otherwise Balanced." };
                    prose(ui, why, 12.5, p.weak);
                    ui.add_space(6.0);
                    prose(ui, "You still buy the skills at the end, and the spark reroll works as usual.", 12.5, p.weak);
                }
                1 => {
                    ui.label(RichText::new(format!("PRIORITIZED SKILLS · {} of {} entered", ticks.iter().filter(|t| **t).count(), ticks.len())).size(11.5).strong().color(p.weak));
                    let two = ui.available_width() >= 520.0;
                    let colw = if two { (ui.available_width() - 12.0) / 2.0 } else { ui.available_width() };
                    egui::Grid::new(("it_skills", b.trainee)).num_columns(if two { 2 } else { 1 }).spacing(vec2(12.0, 4.0)).max_col_width(colw).show(ui, |ui| {
                        for (i, s) in b.skills.iter().take(10).enumerate() {
                            let name = self.kb.skills.get(s).map_or(s.to_string(), |k| k.name.clone());
                            let text = RichText::new(format!("{} {name}", i + 1)).size(12.5);
                            let done = ticks[i];
                            crate::ui_util::UiExt::tick(ui, &mut ticks[i], if done { text.strikethrough().color(p.weak) } else { text });
                            if !two || i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                    ui.label(RichText::new("They become the runner's white sparks, so each fits her.").size(12.0).color(p.weak));
                }
                _ => {
                    ui.label(RichText::new("AGENDA · extra races, win chance").size(11.5).strong().color(p.weak));
                    if b.agenda.is_empty() {
                        ui.label(RichText::new("No extra races needed: leave the agenda empty.").size(12.5));
                    }
                    for (name, grades, chance) in &b.agenda {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).size(12.5));
                            pill(ui, &format!("{grades} · ~{chance}%"), if *chance >= 90 { p.good } else { p.warn });
                        });
                    }
                    ui.label(RichText::new("Won on her grades alone; see How races work, below.").size(12.0).color(p.weak));
                }
            };
            if cols == 3 {
                ui.columns(3, |c| {
                    for (k, col) in c.iter_mut().enumerate() {
                        // ui.columns justifies its children; prose reads better ragged.
                        col.with_layout(egui::Layout::top_down(egui::Align::Min), |col| draw(col, k));
                    }
                });
            } else {
                for k in 0..3 {
                    draw(ui, k);
                    ui.add_space(6.0);
                }
            }
            if ticks != before {
                // Keep only this plan's ticks: a new plan starts fresh.
                all.clear();
                all.insert(plan_key, ticks);
                crate::collection_kit::set_setting(ui.ctx(), "lineage_ticks", all);
            }
        });
    }

    /// How many of each agenda race's links won it (for the +affinity), from the brief's lines.
    fn g1_links(&self, b: &Brief) -> Vec<(String, u32)> {
        b.agenda
            .iter()
            .map(|(name, _, _)| {
                let n = b.races.iter().find(|r| r.starts_with(&format!("Win {name} ("))).and_then(|r| r.split("(+").nth(1)?.split(' ').next()?.parse::<u32>().ok()).map_or(0, |a| a / 3);
                (name.clone(), n)
            })
            .collect()
    }

    /// Fill Career → Plan from a brief and open it.
    /// The skill shop helper for a runner's career: Career → Plan with her plan as the planner
    /// made it (course, targets, deck, skills) and skill shop mode on, her targets as the stats to
    /// replace with her real ones.
    fn open_shop_helper(&mut self, b: &Brief) {
        let Some(mut plan) = b.plan.clone() else { return };
        plan.shop.on = true;
        plan.shop.stats = plan.target;
        self.planner.plan = plan;
        self.tab = crate::Tab::Career;
        self.career_tab = crate::CareerTab::Plan;
        self.toast("Skill shop helper: replace the stats with hers, enter her SP and the skills on offer, then Auto-pick.", crate::ui_util::ToastKind::Info);
    }

    /// What to look for when borrowing a friend's support card: the card at MLB, why it's the
    /// pick, text to paste into the game's friend search, and the next best if no friend has it.
    pub(crate) fn borrow_look_for(&self, ui: &mut egui::Ui, id: u32, why: Option<&str>, alts: &[(u32, String)]) {
        let p = pal(ui);
        let Some(c) = self.kb.cards.get(&id) else { return };
        egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.5, p.accent.gamma_multiply(0.7))).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("LOOK FOR · A FRIEND'S SUPPORT CARD").size(11.5).strong().color(crate::lineage_kit::ink(p.accent, p.surface)));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                self.card_art(ui, id, 72.0, true);
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(semibold(format!("{} [{}]", c.uma, c.title), 14.5)).wrap());
                    ui.label(RichText::new(format!("Fully limit broken (MLB) · {}", c.kind)).size(12.5));
                    if let Some(w) = why {
                        // The ranking's reason is dense: say it plainly, the detail on hover.
                        let plain = if w.contains("meta sheet") { "Why her: the community meta sheet's pick for this race." } else { "Why her: she adds the most to this deck in simulated careers." };
                        ui.add(egui::Label::new(RichText::new(plain).size(12.5)).wrap()).on_hover_text(w);
                    }
                });
            });
            ui.add_space(6.0);
            ui.label(RichText::new("SEARCH TEXT · PASTE INTO THE GAME'S FRIEND SEARCH").size(11.5).strong().color(p.weak));
            let search = format!("{} {}", c.uma, c.title);
            ui.horizontal(|ui| {
                let field_w = (ui.available_width() - 70.0).max(80.0);
                egui::Frame::new().fill(p.input).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                    ui.set_max_width(field_w);
                    ui.add(egui::Label::new(RichText::new(&search).monospace().size(12.5)).truncate());
                });
                if ui.add(egui::Button::new(RichText::new("Copy").size(12.5)).wrap_mode(egui::TextWrapMode::Extend).min_size(vec2(52.0, 24.0))).clicked() {
                    ui.ctx().copy_text(search.clone());
                }
            });
            if !alts.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("NO FRIEND HAS IT MLB? NEXT BEST").size(11.5).strong().color(p.weak));
                for (k, (a, why)) in alts.iter().enumerate() {
                    let ac = self.kb.cards.get(a);
                    ui.horizontal(|ui| {
                        self.card_art(ui, *a, 30.0, true);
                        ui.vertical(|ui| {
                            ui.add(egui::Label::new(RichText::new(format!("{}. {}", k + 2, ac.map_or(a.to_string(), |c| format!("{} [{}]", c.uma, c.title)))).size(12.5)).wrap());
                            let plain = if why.contains("meta sheet") { "Also on the meta sheet for this race." } else { "Next best in simulated careers." };
                            ui.add(egui::Label::new(RichText::new(plain).size(12.0).color(p.weak)).wrap()).on_hover_text(why);
                        });
                    });
                }
            }
            ui.add_space(6.0);
            prose(ui, "Need a new friend? Look for a trainer whose support card list shows this card at MLB.", 12.0, p.weak);
        });
    }

    /// An uma's round avatar, `size` across, with a thin ring; initials if the art is missing.
    pub(crate) fn avatar(&self, ui: &mut egui::Ui, outfit: u32, size: f32) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
        if ui.is_rect_visible(rect) {
            let p = pal(ui);
            if resp.hovered() {
                ui.painter().circle_filled(rect.center(), size / 2.0 + 3.0, p.accent.gamma_multiply(0.35));
            }
            match self.art_uri("umas", outfit) {
                Some(uri) => crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same((size / 2.0) as u8)), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))),
                None => {
                    ui.painter().circle_filled(rect.center(), size / 2.0, p.hover);
                    let name = crate::parentage::uma_name(&self.kb, outfit);
                    let init: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, init, FontId::proportional(size * 0.38), p.weak);
                }
            }
            ui.painter().circle_stroke(rect.center(), size / 2.0, Stroke::new(1.0, p.border));
        }
        if resp.clicked() {
            self.open_detail(crate::detail_modal::Detail::Uma(outfit), None);
        }
        resp.on_hover_text(format!("{} · click for details", crate::parentage::uma_name(&self.kb, outfit))).on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    /// A support card's art, `w` wide at 3:4; a borrowed card gets a dashed-look ring and an MLB tag.
    pub(crate) fn card_art(&self, ui: &mut egui::Ui, id: u32, w: f32, borrowed: bool) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(vec2(w, w * 4.0 / 3.0), Sense::click());
        if ui.is_rect_visible(rect) {
            let p = pal(ui);
            if resp.hovered() {
                ui.painter().rect_filled(rect.expand(3.0), 6.0, p.accent.gamma_multiply(0.35));
            }
            match self.art_uri("cards", id) {
                Some(uri) => crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same(4)), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))),
                None => _ = ui.painter().rect_filled(rect, 4.0, p.hover),
            }
            if borrowed {
                // A dashed ring, and a "Borrow · MLB" band across the art (just "MLB" when small).
                let ring = rect.expand(2.0);
                let pts = [ring.left_top(), ring.right_top(), ring.right_bottom(), ring.left_bottom(), ring.left_top()];
                ui.painter().extend(egui::Shape::dashed_line(&pts, Stroke::new(1.5, p.accent), 4.0, 3.0));
                let label = if w >= 72.0 { "Borrow · MLB" } else { "MLB" };
                let band_h = if w >= 72.0 { 15.0 } else { 12.0 };
                let band = Rect::from_min_size(pos2(rect.left(), rect.top() + rect.height() * 0.42), vec2(w, band_h));
                ui.painter().rect_filled(band, 0.0, p.accent.gamma_multiply(0.92));
                ui.painter().text(band.center(), Align2::CENTER_CENTER, label, FontId::new(if w >= 72.0 { 10.5 } else { 9.0 }, egui::FontFamily::Name(crate::theme::SEMIBOLD.into())), p.on_accent);
            }
        }
        let name = self.kb.cards.get(&id).map_or(id.to_string(), |c| format!("{} [{}]", c.uma, c.title));
        if resp.clicked() {
            let why = borrowed.then(|| match self.roster.cards.iter().find(|c| c.id == id) {
                Some(c) if !c.is_mlb() => format!("Borrow a friend's fully limit broken (MLB) copy for the friend slot: yours is only LB{}.", c.lb),
                _ => "Borrow a friend's fully limit broken (MLB) copy for the friend slot.".to_string(),
            });
            self.open_detail(crate::detail_modal::Detail::Card(id), why);
        }
        resp.on_hover_text(format!("{} · click for details", if borrowed { format!("Borrow: {name}, fully limit broken (MLB)") } else { name })).on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

/// An uma in a sentence: avatar, name and a small note, kept together when lines wrap.
pub(crate) struct Who {
    pub uma: u32,
    pub name: String,
    pub note: String,
    pub note_color: Color32,
    /// Shown when the pointer is over her name (e.g. the card she borrows).
    pub hover: Option<String>,
}

impl App {
    /// A row of umas that wraps between umas, never inside one (avatar, name and note stay together).
    pub(crate) fn who_flow(&self, ui: &mut egui::Ui, items: &[Who]) {
        let gap = 12.0;
        let max = ui.available_width();
        let w = |ui: &egui::Ui, it: &Who| {
            let n = ui.painter().layout_no_wrap(it.name.clone(), FontId::proportional(13.0), Color32::WHITE).size().x;
            let m = if it.note.is_empty() { 0.0 } else { ui.painter().layout_no_wrap(it.note.clone(), FontId::proportional(12.0), Color32::WHITE).size().x + 6.0 };
            24.0 + 6.0 + n + m
        };
        let widths: Vec<f32> = items.iter().map(|it| w(ui, it)).collect();
        let mut start = 0;
        while start < items.len() {
            let mut end = start;
            let mut used = 0.0;
            while end < items.len() && (end == start || used + widths[end] <= max) {
                used += widths[end] + gap;
                end += 1;
            }
            ui.horizontal(|ui| {
                for it in &items[start..end] {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    self.avatar(ui, it.uma, 24.0);
                    let name = ui.add(egui::Label::new(RichText::new(&it.name).size(13.0)).wrap_mode(egui::TextWrapMode::Extend));
                    if let Some(h) = &it.hover {
                        name.on_hover_text(h);
                    }
                    if !it.note.is_empty() {
                        let ink = crate::lineage_kit::ink(it.note_color, pal(ui).raised);
                        ui.add(egui::Label::new(RichText::new(&it.note).size(12.0).color(ink)).wrap_mode(egui::TextWrapMode::Extend));
                    }
                    ui.add_space(gap - 6.0);
                }
            });
            start = end;
        }
    }
}

/// A section that opens and closes, remembered across launches in settings.json
/// (`lineage_open`), since eframe's persistence is off.
pub(crate) fn remembered_section(ui: &mut egui::Ui, key: &str, title: &str, default_open: bool, body: impl FnOnce(&mut egui::Ui)) {
    use egui::collapsing_header::{CollapsingState, paint_default_icon};
    let saved: std::collections::BTreeMap<String, bool> = crate::collection_kit::setting(ui.ctx(), "lineage_open");
    let open = saved.get(key).copied().unwrap_or(default_open);
    let mut st = CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(("lineage_open", key)), open);
    let was = st.is_open();
    let weak = pal(ui).weak;
    let header = ui.horizontal(|ui| {
        st.show_toggle_button(ui, paint_default_icon);
        ui.add(egui::Label::new(RichText::new(title).size(13.0).color(weak)).sense(Sense::click()))
    });
    if header.inner.clicked() {
        st.toggle(ui);
    }
    if st.is_open() != was {
        let mut all = saved;
        all.insert(key.to_string(), st.is_open());
        crate::collection_kit::set_setting(ui.ctx(), "lineage_open", all);
    }
    st.show_body_unindented(ui, body);
}

/// A paragraph, ragged right and no wider than about 80 characters.
pub(crate) fn prose(ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
    let w = ui.available_width().min(PROSE_W);
    ui.allocate_ui_with_layout(vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_max_width(w);
        ui.add(egui::Label::new(RichText::new(text).size(size).color(color)).wrap());
    });
}

/// A labelled row: the label in a fixed column, the content beside it in the rest of the width
/// (a fixed width, so wrapped content wraps instead of squeezing).
fn label_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let weak = pal(ui).weak;
    // Narrower labels in a narrow card, so the deck's six cards fit on one row.
    let label_w = if ui.available_width() < 560.0 { 84.0 } else { 118.0 };
    let w = ui.available_width();
    ui.horizontal_top(|ui| {
        // The label wraps within its column ("SKILLS TO / TARGET") rather than running into the value.
        let galley = ui.painter().layout(label.to_string(), FontId::new(11.5, egui::FontFamily::Name(crate::theme::SEMIBOLD.into())), weak, label_w - 6.0);
        let (r, _) = ui.allocate_exact_size(vec2(label_w, galley.size().y.max(18.0)), Sense::hover());
        ui.painter().galley(pos2(r.left(), r.top() + 2.0), galley, weak);
        let rest = (w - label_w - ui.spacing().item_spacing.x).max(120.0);
        ui.allocate_ui_with_layout(vec2(rest, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_max_width(rest);
            add(ui)
        });
    });
    ui.add_space(8.0);
}

/// The career's stat goals as a table: what to aim for, the least that keeps the run, and the
/// training order (a runner), or which stats her blue spark wants (a parent).
fn stat_table(ui: &mut egui::Ui, b: &Brief) {
    let p = pal(ui);
    let parent = b.p.is_some();
    let order = ["1st", "2nd", "3rd, with Wit", "last", "3rd, with Power"];
    egui::Grid::new(("stat-table", b.trainee, b.step)).spacing(vec2(22.0, 6.0)).min_col_width(56.0).show(ui, |ui| {
        let head = |ui: &mut egui::Ui, t: &str| ui.add(egui::Label::new(RichText::new(t).size(11.5).color(p.weak)).extend());
        head(ui, "Stat");
        head(ui, "Aim for");
        head(ui, if parent { "Why" } else { "Train order" });
        ui.end_row();
        for (i, name) in crate::parentage::STATS.iter().enumerate() {
            ui.add(egui::Label::new(RichText::new(*name).size(13.0)).extend());
            // A runner's stamina floor is the one number she must reach.
            match b.stat_floor.filter(|_| i == 1 && !parent) {
                Some(f) => ui.add(egui::Label::new(semibold(format!("{} minimum", crate::lineage_kit::thousands(f64::from(f.max(b.targets[i])))), 13.5).color(p.warn)).extend()),
                None => ui.add(egui::Label::new(semibold(crate::lineage_kit::thousands(f64::from(b.targets[i])), 13.5)).extend()),
            };
            let note = if parent { if b.stat_aims.contains(&i) { "1,100 or more for a 2★ blue spark" } else { "" } } else { order[i] };
            ui.add(egui::Label::new(RichText::new(note).size(12.5).color(p.weak)).extend());
            ui.end_row();
        }
    });
    ui.add_space(4.0);
}

/// Pills that wrap: each is measured first, and a row ends before one would overflow.
fn pills(ui: &mut egui::Ui, items: &[String], color: Color32) {
    let font = FontId::proportional(12.0);
    let gap = ui.spacing().item_spacing.x;
    let max = ui.available_width();
    let widths: Vec<f32> = items.iter().map(|t| ui.painter().layout_no_wrap(t.clone(), font.clone(), color).size().x + 18.0).collect();
    let mut start = 0;
    while start < items.len() {
        let mut end = start;
        let mut used = 0.0;
        while end < items.len() && (end == start || used + widths[end] <= max) {
            used += widths[end] + gap;
            end += 1;
        }
        ui.horizontal(|ui| {
            for t in &items[start..end] {
                pill(ui, t, color);
            }
        });
        start = end;
    }
}

/// A small rounded label: one widget, so it wraps whole in a wrapping row.
pub(crate) fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    // Drawn on its own tint over the raised surface (the darkest it sits on); text reaches 4.5:1 there.
    let fill = crate::lineage_kit::tint(color, pal(ui).raised, 0.16);
    let ink = crate::lineage_kit::ink(color, fill);
    crate::ui_util::pill(ui, RichText::new(text).size(12.0).color(ink), fill, Stroke::new(1.0, color.gamma_multiply(0.55)));
}

/// An uma's name without the outfit ("Curren Chan"); the outfit shows on hover of her avatar.
fn short_name(kb: &crate::knowledge::Knowledge, outfit: u32) -> String {
    kb.umas.get(&outfit).map_or_else(|| crate::parentage::uma_name(kb, outfit), |u| u.name.clone())
}

/// "an End Closer", "a Pace Chaser".
fn style_article(s: crate::knowledge::Style) -> String {
    s.article()
}

pub(crate) fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}
