//! The Lineage page's saved team plans: the Overview (where each plan stands), the plan bar
//! (Mark done, Edit, Re-plan, Reset, and what changed since the plan was made), the empty state,
//! and the build log while a plan is being made. Opening a view shows the saved plan; only
//! Create plan, Re-plan and Reset run the planner (`parentage::saved`), on a worker thread with
//! its own copy of the app's data, reporting each stage as it goes.

use crate::App;
use crate::lineage_page::{LineageView, pill, prose, remembered_section};
use crate::parentage::progress::{CANCELLED, Progress, Stage};
use crate::parentage::saved::{Act, Built, Choices, Edit, Notice, Plans, TeamPlan, Which, label, stamp};
use crate::theme::{pal, size};
use crate::ui_util::{ToastKind, semibold};
use eframe::egui::{self, RichText, vec2};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

/// A plan being made on a worker.
pub(crate) struct Job {
    pub which: Which,
    rx: Receiver<Result<TeamPlan, String>>,
    stages_rx: Receiver<Vec<Stage>>,
    pub stages: Vec<Stage>,
    cancel: Arc<AtomicBool>,
    pub started: f64,
}

/// How the last build ended, when it didn't make a plan.
#[derive(Clone)]
pub(crate) enum Ended {
    Cancelled(Which),
    Failed(Which, String, Vec<Stage>),
}

/// The page's plan state: the saved plans, a build in flight, and what's open.
#[derive(Default)]
pub(crate) struct PlansUi {
    pub plans: Option<Plans>,
    pub job: Option<Job>,
    pub ended: Option<Ended>,
    pub marking: Option<Which>,
    /// The career picked in Mark done.
    pub mark_key: Option<String>,
    pub editing: Option<Which>,
    pub confirm_reset: Option<Which>,
    /// Re-plan asks first, as Reset does.
    pub confirm_replan: Option<Which>,
    /// The UMALORD_PLAN screenshot hook ran.
    pub env_done: bool,
    /// Each plan's notices and when they were worked out: reading the account takes
    /// milliseconds, so it's done every couple of seconds, and at once after a change.
    notices: Vec<(Which, std::time::Instant, Vec<Notice>)>,
    /// The one-runner plan as last read, for its Overview card (same rule).
    runner: Option<(std::time::Instant, Option<crate::parentage::track::ParentPlan>)>,
    /// The Overview cards' header-and-body heights last frame, and whether they sit in a row
    /// (then each pads to the tallest, so the cards and their footers line up).
    card_h: [f32; 3],
    level: bool,
    /// The step whose card is open (else the next one), and a plan whose open card was hidden
    /// (one column).
    pub open_step: Option<(Which, String)>,
    pub steps_folded: Option<Which>,
    /// The account's umas, support cards and finished umas, and when they were counted.
    ready: Option<(std::time::Instant, [usize; 3])>,
    /// The plan whose "How to raise the ceiling" shows every row.
    pub ceiling_all: Option<String>,
}

/// How long worked-out notices stand (an import elsewhere shows within this).
const NOTICES_FOR: std::time::Duration = std::time::Duration::from_secs(2);

/// The notices' and pending edits' amber, the same in both themes (a light theme's darker warn
/// colour reads as an error when tinted).
const AMBER: egui::Color32 = egui::Color32::from_rgb(0xE8, 0xA0, 0x20);

fn amber_frame() -> egui::Frame {
    egui::Frame::new().fill(AMBER.gamma_multiply(0.16)).stroke(egui::Stroke::new(1.0, AMBER.gamma_multiply(0.7))).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 8))
}

/// A plan's time as people say it: "today 1:19 PM", "Oct 27, 9:05 AM".
pub(crate) fn when(stamp: &str) -> String {
    let (date, time) = stamp.split_once(' ').unwrap_or((stamp, ""));
    let clock = time.split_once(':').and_then(|(h, m)| Some((h.parse::<u32>().ok()?, m))).map(|(h, m)| format!("{}:{m} {}", if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" }));
    let day = if date == crate::ui_util::local_today() {
        "today".to_string()
    } else {
        let mut it = date.split('-').skip(1);
        let month = it.next().and_then(|m| m.parse::<usize>().ok()).and_then(|m| ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].get(m.wrapping_sub(1)).copied());
        match (month, it.next().and_then(|d| d.parse::<u32>().ok())) {
            (Some(m), Some(d)) => format!("{m} {d}"),
            _ => date.to_string(),
        }
    };
    match clock {
        Some(c) if day == "today" => format!("today {c}"),
        Some(c) => format!("{day}, {c}"),
        None => day,
    }
}

/// How long making a plan takes, in words.
/// "a, b and c".
fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn how_long(which: Which) -> &'static str {
    match which {
        Which::Cm(_) => "a few seconds",
        Which::Tt => "about 20 seconds",
    }
}

impl App {
    /// The saved team plans, read once.
    pub(crate) fn team_plans(&mut self) -> &mut Plans {
        if self.lineage.p.plans.is_none() {
            self.lineage.p.plans = Some(self.root.as_deref().map(Plans::load).unwrap_or_default());
        }
        self.lineage.p.plans.get_or_insert_with(Plans::default)
    }

    fn save_team_plans(&mut self) {
        self.lineage.p.notices.clear();
        let (Some(root), Some(plans)) = (self.root.clone(), self.lineage.p.plans.as_ref()) else { return };
        if let Err(e) = plans.save(&root) {
            self.toast(e, ToastKind::Error);
        }
    }

    /// The plan a CM or Team Trials view shows: the CM it's set to, or Team Trials.
    pub(crate) fn view_which(&self) -> Option<Which> {
        match self.lineage.view {
            LineageView::Cm => self.kb.upcoming_cms(&crate::ui_util::today()).get(self.lineage.cm).map(|e| Which::Cm(e.id)),
            LineageView::Tt => Some(Which::Tt),
            _ => None,
        }
    }

    /// The account's umas, support cards and finished umas (counted every couple of seconds:
    /// the finished umas are read from disk).
    pub(crate) fn plan_ready(&mut self) -> [usize; 3] {
        if let Some((t, n)) = self.lineage.p.ready.filter(|(t, _)| t.elapsed() < NOTICES_FOR) {
            let _ = t;
            return n;
        }
        let n = [self.roster.umas.len(), self.roster.cards.len(), self.veterans().len()];
        self.lineage.p.ready = Some((std::time::Instant::now(), n));
        n
    }

    /// Why a plan can't be made yet, if it can't.
    pub(crate) fn plan_blocked(&mut self) -> Option<String> {
        let [umas, cards, _] = self.plan_ready();
        crate::parentage::saved::not_set_up(umas, cards)
    }

    /// Before any plan: the account must be set up (umas and cards), else a card says what to
    /// do and planning stays off; with no finished umas, a note says plans are for a fresh
    /// account. Returns whether planning is blocked.
    pub(crate) fn setup_gate(&mut self, ui: &mut egui::Ui) -> bool {
        let [umas, cards, vets] = self.plan_ready();
        let p = pal(ui);
        if let Some(why) = crate::parentage::saved::not_set_up(umas, cards) {
            crate::ui_util::card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(semibold("Set up your account first", size::LG));
                ui.add_space(4.0);
                prose(ui, &format!("{why} umaLord has {umas} uma{} and {cards} support card{} for this account so far.", if umas == 1 { "" } else { "s" }, if cards == 1 { "" } else { "s" }), 13.5, p.text);
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add(crate::ui_util::primary_button(ui, "📝 Set up my collection", 13.5)).clicked() {
                        self.open_setup(crate::onboarding::Step::Umas);
                    }
                    if ui.button("📥 Load a profile").on_hover_text("Switch to a saved account (a .umalord.json file).").clicked() {
                        self.open_setup(crate::onboarding::Step::Import);
                    }
                });
            });
            ui.add_space(12.0);
            return true;
        }
        if vets == 0 {
            amber_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(semibold("No finished umas imported: plans will be for a fresh account", 14.0));
                prose(ui, "Finished umas are the umas whose careers you've completed; they race for you today and become parents. Without them every runner starts from scratch and every parent is planned from nothing, which only fits a brand-new account. If you have finished umas, import them first.", 13.0, p.text);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("🌱 Import finished umas").on_hover_text("Takes UmaExtractor's data.json from your Downloads folder (or drop it on the window).").clicked() {
                        self.import_finished_from_downloads();
                    }
                });
            });
            ui.add_space(12.0);
        }
        false
    }

    /// Settings' "Import finished umas", from the Lineage page.
    pub(crate) fn import_finished_from_downloads(&mut self) {
        match crate::settings::user_dir("Downloads").map(|d| d.join("data.json")).filter(|p| p.is_file()) {
            Some(path) => {
                let r = self.import_veterans(&path);
                self.lineage.p.ready = None;
                self.lineage.p.notices.clear();
                self.toast_flag(r.map_or_else(|e| (e, true), |m| (m, false)));
            }
            None => self.toast("No data.json in Downloads. Run UmaExtractor (or tools/read_veterans.py) on the game's Veteran List, then drop its data.json on the window.", ToastKind::Info),
        }
    }

    /// Make a plan (Create plan, Re-plan, Reset) on a worker. The only thing on the page that
    /// simulates. Refused until the account has its umas and support cards.
    pub(crate) fn plan_start(&mut self, ctx: &egui::Context, which: Which, choices: Choices) {
        if self.lineage.p.job.is_some() {
            return;
        }
        if let Some(why) = self.plan_blocked() {
            self.toast(why, ToastKind::Warn);
            return;
        }
        let root = self.root.clone();
        let preset = match which {
            Which::Cm(id) => self.kb.upcoming_cms(&crate::ui_util::today()).into_iter().find(|e| e.id == id).cloned(),
            Which::Tt => None,
        };
        let (tx, rx) = channel();
        let (stx, stages_rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let mut app = crate::agent::mcp::headless_app(root);
            app.progress = Some(Progress::new(Some(stx), flag));
            let out = match which {
                Which::Cm(id) => preset.ok_or_else(|| format!("CM {id} isn't in the schedule any more.")).and_then(|p| app.make_cm_plan(&p, &choices)),
                Which::Tt => app.make_tt_plan(&choices),
            };
            let log = app.progress.take().map(Progress::finish).unwrap_or_default();
            let _ = tx.send(out.map(|mut p| {
                p.log = log;
                p
            }));
            ctx2.request_repaint();
        });
        self.lineage.p.ended = None;
        self.lineage.p.job = Some(Job { which, rx, stages_rx, stages: Vec::new(), cancel, started: ctx.input(|i| i.time) });
    }

    pub(crate) fn plan_poll(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.lineage.p.job else { return };
        while let Ok(s) = job.stages_rx.try_recv() {
            job.stages = s;
        }
        let Ok(out) = job.rx.try_recv() else {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
            return;
        };
        let (which, stages) = (job.which, job.stages.clone());
        self.lineage.p.job = None;
        self.lineage.done_at = Some(ctx.input(|i| i.time));
        match out {
            Ok(plan) => {
                self.lineage.p.notices.clear();
                let n = plan.careers.len();
                self.team_plans().put(plan, &stamp());
                self.save_team_plans();
                self.toast(format!("The {} plan is ready: {n} careers. Its log is under \"How this plan was chosen\".", label(which)), ToastKind::Success);
            }
            Err(e) if e == CANCELLED => self.lineage.p.ended = Some(Ended::Cancelled(which)),
            Err(e) => self.lineage.p.ended = Some(Ended::Failed(which, e, stages)),
        }
    }

    pub(crate) fn plan_building(&self, which: Which) -> bool {
        self.lineage.p.job.as_ref().is_some_and(|j| j.which == which)
    }

    /// What changed under a plan, checked against the account now (no simulation).
    fn plan_notices(&mut self, which: Which) -> Vec<Notice> {
        if let Some((_, _, n)) = self.lineage.p.notices.iter().find(|(w, t, _)| *w == which && t.elapsed() < NOTICES_FOR) {
            return n.clone();
        }
        let n = self.work_out_notices(which);
        self.lineage.p.notices.retain(|(w, ..)| *w != which);
        self.lineage.p.notices.push((which, std::time::Instant::now(), n.clone()));
        n
    }

    fn work_out_notices(&mut self, which: Which) -> Vec<Notice> {
        let vets = self.veterans();
        let now = self.plan_basis();
        let next = self.kb.upcoming_cms(&crate::ui_util::today()).first().map(|e| (e.id, e.name.clone()));
        self.team_plans();
        let plan = self.lineage.p.plans.as_ref().and_then(|ps| ps.get(which));
        plan.map(|p| p.notices(&self.kb, &vets, &now, next.as_ref().map(|(i, n)| (*i, n.as_str())))).unwrap_or_default()
    }

    fn notice_act(&mut self, which: Which, n: &Notice) {
        match &n.act {
            Act::MarkDone { key, vet } => {
                let r = self.team_plans().mark_done(which, key, Some(*vet), &stamp());
                self.save_team_plans();
                self.toast_result(r);
            }
            Act::Replan => self.lineage.p.confirm_replan = Some(which),
            Act::PlanCm(id) => {
                let i = self.kb.upcoming_cms(&crate::ui_util::today()).iter().position(|e| e.id == *id).unwrap_or(0);
                self.lineage.cm = i;
                self.lineage.view = LineageView::Cm;
            }
        }
    }

    fn dismiss(&mut self, which: Which, id: &str) {
        if let Some(p) = self.team_plans().get_mut(which) {
            p.dismissed.push(id.to_string());
        }
        self.save_team_plans();
    }

    /// A notice as an amber box with its action and Not now.
    fn notice_box(&mut self, ui: &mut egui::Ui, which: Which, n: &Notice) {
        let p = pal(ui);
        amber_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            prose(ui, &format!("⚠ {}", n.text), 13.0, p.text);
            ui.horizontal(|ui| {
                let act = match n.act {
                    Act::MarkDone { .. } => "✔ Mark done",
                    Act::Replan => "↻ Re-plan",
                    Act::PlanCm(_) => "Plan for it",
                };
                let busy = self.lineage.p.job.is_some() && n.act == Act::Replan;
                if ui.add_enabled(!busy, crate::ui_util::primary_button(ui, act, 13.0)).on_disabled_hover_text("A plan is being made; wait for it to finish.").clicked() {
                    self.notice_act(which, n);
                }
                if ui.button("Not now").clicked() {
                    self.dismiss(which, &n.id);
                }
            });
        });
    }

    // ── Overview ────────────────────────────────────────────────────────────────────────────

    /// Where each plan stands, one card each, then what was done lately.
    pub(crate) fn lineage_overview(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        egui::ScrollArea::vertical().id_salt("lineage_home").auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(semibold("Where each plan stands", size::XL));
                ui.add_space(8.0);
                ui.label(RichText::new("Opening a plan shows it as saved. Nothing simulates until you choose Create plan, Re-plan or Reset.").size(13.0).color(p.weak));
            });
            ui.add_space(10.0);
            self.setup_gate(ui);
            let wide = ui.available_width() >= 1000.0;
            self.lineage.p.level = wide;
            let cards: [fn(&mut App, &mut egui::Ui); 3] = [App::overview_cm, App::overview_tt, App::overview_runner];
            if wide {
                let w = (ui.available_width() - 2.0 * 14.0) / 3.0;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    for f in cards {
                        ui.allocate_ui_with_layout(vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(w);
                            f(self, ui);
                        });
                    }
                });
            } else {
                for f in cards {
                    f(self, ui);
                    ui.add_space(10.0);
                }
            }
            ui.add_space(14.0);
            self.overview_recent(ui);
        });
    }

    /// A card: the title row with its state pill, the body, then a footer row with the card's
    /// action. In a row of cards each body pads to the tallest, so the cards are one height and
    /// their footers line up.
    fn overview_card(&mut self, ui: &mut egui::Ui, slot: usize, icon: &str, title: &str, state: (&str, egui::Color32), body: impl FnOnce(&mut Self, &mut egui::Ui), footer: impl FnOnce(&mut Self, &mut egui::Ui)) {
        crate::ui_util::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let top = ui.cursor().top();
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                ui.label(semibold(format!("{icon} {title}"), size::LG));
                pill(ui, state.0, state.1);
            });
            ui.add_space(6.0);
            body(self, ui);
            let own = ui.cursor().top() - top;
            if self.lineage.p.level {
                let tallest = self.lineage.p.card_h.iter().copied().fold(own, f32::max);
                ui.add_space(tallest - own);
            }
            if (self.lineage.p.card_h[slot] - own).abs() > 0.5 {
                self.lineage.p.card_h[slot] = own;
                ui.ctx().request_repaint();
            }
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.set_min_height(32.0);
                footer(self, ui);
            });
        });
    }

    fn overview_cm(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let today = crate::ui_util::today();
        let Some(ev) = self.kb.upcoming_cms(&today).first().map(|e| (*e).clone()) else {
            self.overview_card(ui, 0, "🏆", "Champions Meeting", ("No CM scheduled", p.weak), |_, ui| prose(ui, "No upcoming Champions Meeting is in the schedule yet.", 13.0, pal(ui).weak), |_, _| {});
            return;
        };
        // The plan for the next CM, else the newest plan for a later one.
        let which = Which::Cm(ev.id);
        let plan = self.team_plans().get(which).cloned();
        let notices = self.plan_notices(which);
        let building = self.plan_building(which);
        let state = if building {
            ("Being made", p.accent)
        } else if plan.is_none() {
            ("No plan", p.weak)
        } else if !notices.is_empty() {
            ("Needs attention", p.warn)
        } else {
            ("In progress", p.accent)
        };
        let course = format!("CM {} · {} · {}", ev.id, ev.name, ev.course_name);
        let has = plan.is_some();
        self.overview_card(ui, 0, "🏆", "Champions Meeting", state, |this, ui| {
            let p = pal(ui);
            match &plan {
                Some(plan) => {
                    let (done, total) = plan.progress();
                    let next = plan.next().map_or("every career is done".into(), |c| format!("next: {}", c.label));
                    prose(ui, &format!("{course} · career {} of {total} · {next}", (done + 1).min(total)), 13.5, p.text);
                    if let Built::Cm { team, .. } = &plan.built {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for r in &team.runners {
                                let own = plan.careers.iter().find(|c| c.key == format!("r{}", r.trainee));
                                let finished = own.is_some_and(|c| c.done.is_some());
                                ui.vertical(|ui| {
                                    this.avatar(ui, r.trainee, 34.0);
                                    let tick = if finished { "✔ " } else { "" };
                                    ui.label(RichText::new(format!("{tick}{}", crate::parentage::short_name(&this.kb, r.trainee))).size(11.5).color(if finished { p.accent } else { p.weak }));
                                });
                            }
                            let days = crate::tab_planner::days_apart(&ev.date, &today).max(0);
                            ui.label(RichText::new(format!("Round 1 on {} · in {days} days", when(&ev.date))).size(12.5).color(p.weak));
                        });
                    }
                }
                None => {
                    prose(ui, &format!("{course}. Three runners for it, the parent careers they need, decks and skills."), 13.5, p.text);
                }
            }
            if let Some(plan) = &plan {
                ceiling_hint(ui, plan.live_ceiling().as_ref().and_then(|v| v.first()));
            }
            for n in notices.iter().take(1) {
                ui.add_space(6.0);
                this.notice_box(ui, which, n);
            }
            if notices.len() > 1 {
                ui.label(RichText::new(format!("and {} more in the plan", notices.len() - 1)).size(12.0).color(p.weak));
            }
        }, |this, ui| this.overview_footer(ui, which, has, LineageView::Cm));
    }

    fn overview_tt(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let which = Which::Tt;
        let plan = self.team_plans().get(which).cloned();
        let notices = self.plan_notices(which);
        let building = self.plan_building(which);
        let state = if building {
            ("Being made", p.accent)
        } else if plan.is_none() {
            ("No plan", p.weak)
        } else if !notices.is_empty() {
            ("Needs attention", p.warn)
        } else {
            ("In progress", p.accent)
        };
        let has = plan.is_some();
        self.overview_card(ui, 1, "🏟", "Team Trials", state, |this, ui| {
            let p = pal(ui);
            for n in notices.iter().take(1) {
                this.notice_box(ui, which, n);
                ui.add_space(6.0);
            }
            match &plan {
                Some(plan) => {
                    let (done, total) = plan.progress();
                    let next = plan.next().map_or("every career is done".into(), |c| format!("next: {}", c.label));
                    let gain = match &plan.built {
                        Built::Tt { team, .. } if team.today_total > 0.0 => format!("{}{} over today's team · ", if team.gain < 0.0 { "−" } else { "+" }, crate::lineage_kit::thousands(team.gain.abs())),
                        _ => String::new(),
                    };
                    prose(ui, &format!("{gain}{done} of {total} careers done · {next}"), 13.5, p.text);
                    if let Built::Tt { team, .. } = &plan.built {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            let mut finished = 0;
                            for r in &team.rows {
                                let prefix = format!("tt:{}:", r.name);
                                let race_done = plan.careers.iter().filter(|c| c.key.starts_with(&prefix)).all(|c| c.done.is_some());
                                finished += usize::from(race_done);
                                let (fill, ink) = if race_done { (p.accent, p.on_accent) } else { (egui::Color32::TRANSPARENT, p.weak) };
                                crate::ui_util::pill(ui, RichText::new(r.name).size(11.5).color(ink), fill, egui::Stroke::new(1.0, if race_done { p.accent } else { p.weak }));
                            }
                            ui.label(RichText::new(format!("{finished} of {} races done", team.rows.len())).size(12.0).color(p.weak));
                        });
                    }
                }
                None => prose(ui, "Fifteen runners over five races, an Ace for each, today's team to beat, and the career that raises it most.", 13.5, p.text),
            }
            if let Some(plan) = &plan {
                ceiling_hint(ui, plan.live_ceiling().as_ref().and_then(|v| v.first()));
            }
        }, |this, ui| this.overview_footer(ui, which, has, LineageView::Tt));
    }

    fn overview_runner(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let plan = match &self.lineage.p.runner {
            Some((t, p)) if t.elapsed() < NOTICES_FOR => p.clone(),
            _ => {
                let p = self.root.as_deref().and_then(crate::parentage::track::load);
                self.lineage.p.runner = Some((std::time::Instant::now(), p.clone()));
                p
            }
        };
        let state = if plan.is_some() { ("In progress", p.accent) } else { ("No plan", p.weak) };
        let has = plan.is_some();
        self.overview_card(ui, 2, "🌱", "One runner", state, |this, ui| {
            let p = pal(ui);
            match &plan {
                Some(plan) => {
                    let n = plan.steps.len();
                    let line = match plan.current() {
                        Some(i) => format!(
                            "{} · {} · step {} of {n} · next: {}'s parent career",
                            crate::parentage::uma_name(&this.kb, plan.goal.runner),
                            plan.goal.label,
                            i + 1,
                            crate::parentage::uma_name(&this.kb, plan.steps[i].run.trainee)
                        ),
                        None => format!("{} · {} · her lineage is ready: run her own career.", crate::parentage::uma_name(&this.kb, plan.goal.runner), plan.goal.label),
                    };
                    prose(ui, &line, 13.5, p.text);
                    ceiling_hint(ui, plan.ceiling.as_ref().and_then(|v| v.first()));
                }
                None => prose(ui, "One uma and a goal: each parent career it takes to get her there, in order.", 13.5, p.text),
            }
        }, |this, ui| {
            if has {
                if ui.button("Open ›").clicked() {
                    this.lineage.view = LineageView::Runner;
                }
            } else {
                let blocked = this.plan_blocked();
                if ui.add_enabled(blocked.is_none(), crate::ui_util::primary_button(ui, "Create plan", 13.5)).on_hover_text("Opens One runner, where you pick her and her goal.").on_disabled_hover_text(blocked.unwrap_or_default()).clicked() {
                    this.lineage.view = LineageView::Runner;
                }
                ui.label(RichText::new("pick her, then a few seconds").size(12.0).color(pal(ui).weak));
            }
        });
    }

    /// A team card's footer: the build's stage while one runs, Open when there's a plan, else
    /// Create plan.
    fn overview_footer(&mut self, ui: &mut egui::Ui, which: Which, has_plan: bool, view: LineageView) {
        let p = pal(ui);
        if let Some(job) = self.lineage.p.job.as_ref().filter(|j| j.which == which) {
            let stage = job.stages.last().map_or("Starting…".to_string(), |s| s.title.clone());
            ui.spinner();
            ui.label(RichText::new(format!("{stage} · stage {} of 7", job.stages.len().max(1))).size(12.5).color(p.weak));
            if ui.link("Watch").clicked() {
                self.lineage.view = view;
            }
        } else if has_plan {
            if ui.button("Open ›").clicked() {
                self.lineage.view = view;
            }
        } else {
            let ctx = ui.ctx().clone();
            let blocked = self.plan_blocked();
            let busy = self.lineage.p.job.is_some() || blocked.is_some();
            if ui.add_enabled(!busy, crate::ui_util::primary_button(ui, "Create plan", 13.5)).on_disabled_hover_text(blocked.unwrap_or_else(|| "Another plan is being made; wait for it to finish.".into())).clicked() {
                self.lineage.view = view;
                self.plan_start(&ctx, which, Choices::default());
            }
            ui.label(RichText::new(format!("takes {}", how_long(which))).size(12.0).color(p.weak));
        }
    }

    fn overview_recent(&mut self, ui: &mut egui::Ui) {
        let recent = self.team_plans().recent.clone();
        if recent.is_empty() {
            return;
        }
        let p = pal(ui);
        let mut undo = None;
        crate::ui_util::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(semibold("Recent", size::MD));
                ui.label(RichText::new("the last few things done, newest first").size(12.5).color(p.weak));
            });
            for (i, r) in recent.iter().enumerate() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&r.text).size(13.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if r.undo.is_some() && ui.link("Undo").clicked() {
                            undo = Some(i);
                        }
                        ui.label(RichText::new(when(&r.at)).size(13.0).color(p.weak));
                    });
                });
            }
        });
        if let Some(i) = undo {
            if self.team_plans().undo(i) {
                self.save_team_plans();
                self.toast("Undone.", ToastKind::Success);
            }
        }
    }

    // ── A plan's view ───────────────────────────────────────────────────────────────────────

    /// The view when there's no plan: what Create plan makes, from what, and how long it takes.
    pub(crate) fn plan_empty(&mut self, ui: &mut egui::Ui, which: Which) {
        let p = pal(ui);
        let ctx = ui.ctx().clone();
        self.plan_ended(ui, which);
        let blocked = self.setup_gate(ui);
        crate::ui_util::card(ui, |ui| {
            ui.set_max_width(760.0);
            let (title, makes) = match which {
                Which::Cm(id) => {
                    let ev = self.kb.upcoming_cms(&crate::ui_util::today()).into_iter().find(|e| e.id == id).cloned();
                    (
                        format!("No plan for CM {id}{} yet", ev.as_ref().map_or(String::new(), |e| format!(" ({}, {})", e.name, e.course_name))),
                        "Three runners, one for each style the meta sheet ranks best, raced against each other and against what you can field today; the parent careers each needs, hardest first; and a deck, borrowed card and skills for every career.",
                    )
                }
                Which::Tt => ("No Team Trials plan yet".to_string(), "Fifteen runners over the five races, an Ace for each, raced against what you can field today; and every career that takes, the one that raises the team most first."),
            };
            ui.label(semibold(title, size::LG));
            ui.add_space(6.0);
            prose(ui, makes, 13.5, p.text);
            ui.add_space(4.0);
            prose(ui, "It reads your umas, finished umas, support cards and the community meta. Nothing runs until you press Create plan.", 13.0, p.weak);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let busy = self.lineage.p.job.is_some() || blocked;
                if ui.add_enabled(!busy, crate::ui_util::primary_button(ui, "Create plan", 14.0)).on_disabled_hover_text(if blocked { "Set up your account first: see above." } else { "Another plan is being made; wait for it to finish." }).clicked() {
                    self.plan_start(&ctx, which, Choices::default());
                }
                ui.label(RichText::new(format!("takes {}", how_long(which))).size(12.5).color(p.weak));
            });
        });
    }

    /// A cancelled or failed build's message, above the plan or the empty state.
    fn plan_ended(&mut self, ui: &mut egui::Ui, which: Which) {
        let Some(ended) = self.lineage.p.ended.clone() else { return };
        let p = pal(ui);
        let ctx = ui.ctx().clone();
        match ended {
            Ended::Cancelled(w) if w == which => {
                crate::ui_util::card(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(CANCELLED).size(13.5));
                        if ui.link("Dismiss").clicked() {
                            self.lineage.p.ended = None;
                        }
                    });
                });
                ui.add_space(10.0);
            }
            Ended::Failed(w, why, stages) if w == which => {
                crate::ui_util::card(ui, |ui| {
                    ui.label(semibold("Making the plan stopped", size::MD).color(p.warn));
                    prose(ui, &why, 13.5, p.text);
                    if let Some(s) = stages.last() {
                        ui.label(RichText::new(format!("It stopped at stage {}: {}.", stages.len(), s.title)).size(12.5).color(p.weak));
                    }
                    ui.horizontal(|ui| {
                        if ui.add(crate::ui_util::primary_button(ui, format!("Try again ({})", how_long(which)), 13.5)).clicked() {
                            let choices = self.team_plans().get(which).map(TeamPlan::choices_after).unwrap_or_default();
                            self.plan_start(&ctx, which, choices);
                        }
                        if ui.link("Dismiss").clicked() {
                            self.lineage.p.ended = None;
                        }
                        if ui.link("Copy details").clicked() {
                            let text = stages.iter().map(|s| format!("{}\n{}", s.title, s.lines.join("\n"))).collect::<Vec<_>>().join("\n\n");
                            ui.ctx().copy_text(format!("{why}\n\n{text}"));
                        }
                    });
                });
                ui.add_space(10.0);
            }
            _ => {}
        }
    }

    /// The plan bar: name and age, what changed, and Mark done, Edit, Re-plan and Reset; the
    /// edits waiting; the Mark done and Edit panels when open.
    pub(crate) fn plan_bar(&mut self, ui: &mut egui::Ui, which: Which) {
        let Some(plan) = self.team_plans().get(which).cloned() else { return };
        let notices = self.plan_notices(which);
        let building = self.plan_building(which);
        let busy = self.lineage.p.job.is_some();
        let p = pal(ui);
        let ctx = ui.ctx().clone();
        self.plan_ended(ui, which);
        crate::ui_util::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let one_row = ui.available_width() >= 760.0;
            let head = |ui: &mut egui::Ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(semibold(format!("{} plan", label(which)), size::MD));
                        if !notices.is_empty() {
                            pill(ui, "⚠ Changed since", p.warn);
                        }
                        if plan.basis.vets.is_empty() {
                            crate::ui_util::pill(ui, RichText::new("Fresh account").size(12.0).color(p.text), AMBER.gamma_multiply(0.25), egui::Stroke::new(1.0, AMBER.gamma_multiply(0.7))).on_hover_text(crate::parentage::saved::FRESH);
                        }
                    });
                    let (done, total) = plan.progress();
                    ui.label(RichText::new(format!("Made {} · {done} of {total} careers done", when(&plan.made))).size(12.5).color(p.weak));
                });
            };
            // Mark done, Edit, Re-plan, …; drawn in reverse when laid out from the right.
            let mut buttons = |ui: &mut egui::Ui, reverse: bool| {
                let mut order = [0, 1, 2, 3];
                if reverse {
                    order.reverse();
                }
                for k in order {
                    match k {
                        0 => {
                            if ui.add_enabled(!building, egui::Button::new("✔ Mark done…")).clicked() {
                                self.lineage.p.marking = if self.lineage.p.marking == Some(which) { None } else { Some(which) };
                                self.lineage.p.mark_key = plan.next().map(|c| c.key.clone());
                            }
                        }
                        1 => {
                            if ui.add_enabled(!building, egui::Button::new("✏ Edit")).on_hover_text("Keep or leave out runners. Edits wait until you re-plan; nothing runs as you make them.").clicked() {
                                self.lineage.p.editing = if self.lineage.p.editing == Some(which) { None } else { Some(which) };
                            }
                        }
                        2 => {
                            if !building
                                && ui
                                    .add_enabled(!busy, egui::Button::new("↻ Re-plan"))
                                    .on_hover_text(format!("Make the plan again from your account as it is now, keeping the careers you marked done and the runners you kept or left out. Asks first; takes {}.", how_long(which)))
                                    .on_disabled_hover_text("Another plan is being made; wait for it to finish.")
                                    .clicked()
                            {
                                self.lineage.p.confirm_replan = Some(which);
                            }
                        }
                        _ => {
                            ui.menu_button("…", |ui| {
                                if ui.button("Reset plan…").on_hover_text("Start over: a new plan without the careers marked done or your choices.").clicked() {
                                    self.lineage.p.confirm_reset = Some(which);
                                    ui.close();
                                }
                            });
                        }
                    }
                }
            };
            if one_row {
                ui.horizontal(|ui| {
                    head(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| buttons(ui, true));
                });
            } else {
                head(ui);
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| buttons(ui, false));
            }
            for n in &notices {
                ui.add_space(6.0);
                self.notice_box(ui, which, n);
            }
            if !plan.pending.is_empty() {
                ui.add_space(8.0);
                self.pending_bar(ui, which, &plan);
            }
            if self.lineage.p.marking == Some(which) {
                ui.add_space(8.0);
                self.mark_panel(ui, which, &plan);
            }
            if self.lineage.p.editing == Some(which) {
                ui.add_space(8.0);
                self.edit_panel(ui, which, &plan);
            }
        });
        self.reset_dialog(&ctx);
        self.replan_dialog(&ctx);
    }

    fn pending_bar(&mut self, ui: &mut egui::Ui, which: Which, plan: &TeamPlan) {
        let p = pal(ui);
        let lines: Vec<String> = plan.pending.iter().map(|e| crate::lineage_page::capitalize(&e.describe(&self.kb))).collect();
        amber_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let n = plan.pending.len();
            ui.label(semibold(format!("{n} change{} waiting", if n == 1 { "" } else { "s" }), 13.5));
            let mut drop = None;
            for (i, line) in lines.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("· {line}")).size(13.0));
                    if ui.small_button("Undo").clicked() {
                        drop = Some(i);
                    }
                });
            }
            ui.label(RichText::new("Nothing runs until you press Re-plan.").size(12.0).color(p.weak));
            ui.horizontal(|ui| {
                let busy = self.lineage.p.job.is_some();
                if ui.add_enabled(!busy, crate::ui_util::primary_button(ui, format!("↻ Re-plan with these changes ({})", how_long(which)), 13.0)).clicked() {
                    self.lineage.p.confirm_replan = Some(which);
                }
                if ui.button("Discard changes").clicked() {
                    if let Some(pl) = self.team_plans().get_mut(which) {
                        pl.pending.clear();
                    }
                    self.save_team_plans();
                }
            });
            if let Some(i) = drop {
                if let Some(pl) = self.team_plans().get_mut(which) {
                    pl.pending.remove(i);
                }
                self.save_team_plans();
            }
        });
    }

    /// Mark a career done: which one (the next by default) and the finished uma that ran it.
    fn mark_panel(&mut self, ui: &mut egui::Ui, which: Which, plan: &TeamPlan) {
        let p = pal(ui);
        let open: Vec<(String, String, u32)> = plan.careers.iter().filter(|c| c.done.is_none()).map(|c| (c.key.clone(), c.label.clone(), c.brief.trainee)).collect();
        if open.is_empty() {
            ui.label("Every career in this plan is done.");
            return;
        }
        let key = self.lineage.p.mark_key.clone().filter(|k| open.iter().any(|o| &o.0 == k)).unwrap_or_else(|| open[0].0.clone());
        let (_, label_now, trainee) = open.iter().find(|o| o.0 == key).cloned().unwrap_or_default();
        egui::Frame::new().stroke(egui::Stroke::new(1.0, p.accent.gamma_multiply(0.6))).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Mark done:").size(13.0));
                egui::ComboBox::from_id_salt("mark_career").selected_text(&label_now).width(320.0).show_ui(ui, |ui| {
                    for (k, l, _) in &open {
                        if ui.selectable_label(*k == key, l).clicked() {
                            self.lineage.p.mark_key = Some(k.clone());
                        }
                    }
                });
            });
            // Her finished umas the plan hasn't used yet, newest first.
            let used: Vec<u64> = plan.careers.iter().filter_map(|c| c.done.as_ref().and_then(|d| d.vet)).collect();
            let mut vets: Vec<crate::veterans::Veteran> = self.veterans().into_iter().filter(|v| v.card_id == trainee && !used.contains(&v.trained_chara_id)).collect();
            vets.sort_by_key(|v| std::cmp::Reverse(v.trained_chara_id));
            let mut pick: Option<Option<u64>> = None;
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("The finished uma that ran it:").size(12.5).color(p.weak));
                for v in vets.iter().take(6) {
                    if ui.button(format!("#{}", v.trained_chara_id)).clicked() {
                        pick = Some(Some(v.trained_chara_id));
                    }
                }
                if vets.is_empty() {
                    ui.label(RichText::new("none imported yet").size(12.5).color(p.weak));
                }
                if ui.button("Done, without one").on_hover_text("Mark it done now; import her later.").clicked() {
                    pick = Some(None);
                }
                if ui.link("Close").clicked() {
                    self.lineage.p.marking = None;
                }
            });
            if let Some(vet) = pick {
                let r = self.team_plans().mark_done(which, &key, vet, &stamp());
                self.save_team_plans();
                self.toast_result(r);
                self.lineage.p.marking = None;
            }
        });
    }

    /// Keep or leave out runners; each edit waits for the next re-plan.
    fn edit_panel(&mut self, ui: &mut egui::Ui, which: Which, plan: &TeamPlan) {
        let p = pal(ui);
        let runners: Vec<(u32, Option<String>)> = match &plan.built {
            Built::Cm { team, .. } => team.runners.iter().map(|r| (r.trainee, None)).collect(),
            Built::Tt { team, .. } => team.rows.iter().flat_map(|r| r.slots.iter().map(move |s| (s.trainee, Some(r.name.to_string())))).collect(),
        };
        let after = plan.choices_after();
        let mut add: Option<Edit> = None;
        egui::Frame::new().stroke(egui::Stroke::new(1.0, p.accent.gamma_multiply(0.6))).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            prose(ui, "Keep a runner on the team, or leave a character out. Each change waits until you re-plan.", 12.5, p.weak);
            egui::Grid::new("plan_edit").num_columns(3).spacing(vec2(10.0, 4.0)).max_col_width(260.0).show(ui, |ui| {
                for (uma, race) in &runners {
                    let name = crate::parentage::uma_name(&self.kb, *uma);
                    ui.label(RichText::new(match race {
                        Some(r) => format!("{name} ({r})"),
                        None => name,
                    }).size(13.0));
                    let chara = crate::affinity::chara(*uma);
                    let kept = after.pin.contains(uma) || after.tt_pin.iter().any(|(u, _)| u == uma);
                    if kept {
                        if ui.small_button("Unpin").clicked() {
                            add = Some(Edit::Unpin { uma: *uma });
                        }
                    } else if ui.small_button("📌 Keep").on_hover_text("Keep her on the team when it's re-planned.").clicked() {
                        add = Some(Edit::Pin { uma: *uma, race: race.clone() });
                    }
                    if after.exclude.contains(&chara) {
                        if ui.small_button("Let back in").clicked() {
                            add = Some(Edit::Include { chara });
                        }
                    } else if ui.small_button("✖ Leave out").on_hover_text("Plan without this character.").clicked() {
                        add = Some(Edit::Exclude { chara });
                    }
                    ui.end_row();
                }
            });
            if ui.link("Close").clicked() {
                self.lineage.p.editing = None;
            }
        });
        if let Some(e) = add {
            if let Some(pl) = self.team_plans().get_mut(which) {
                pl.pending.push(e);
            }
            self.save_team_plans();
        }
    }

    /// Re-plan's confirmation: what it keeps, and that the plan on screen is set aside while
    /// the new one is made.
    fn replan_dialog(&mut self, ctx: &egui::Context) {
        let Some(which) = self.lineage.p.confirm_replan else { return };
        let Some(plan) = self.team_plans().get(which).cloned() else {
            self.lineage.p.confirm_replan = None;
            return;
        };
        let (done, _) = plan.progress();
        let mut keeps = Vec::new();
        if done > 0 {
            keeps.push(format!("the {done} career{} you marked done", if done == 1 { "" } else { "s" }));
        }
        let kept = plan.choices.pin.len() + plan.choices.tt_pin.len() + plan.choices.exclude.len();
        if kept > 0 {
            keeps.push(format!("the {kept} runner{} you kept or left out", if kept == 1 { "" } else { "s" }));
        }
        if !plan.pending.is_empty() {
            keeps.push(format!("your {} edit{} waiting", plan.pending.len(), if plan.pending.len() == 1 { "" } else { "s" }));
        }
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("lineage_replan")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.label(semibold(format!("Re-plan the {} plan?", label(which)), size::LG));
            ui.add_space(6.0);
            let keeps = if keeps.is_empty() { " It keeps what you mark done and choose; there's nothing yet.".to_string() } else { format!(" It keeps {}.", join_and(&keeps)) };
            prose(ui, &format!("This makes the plan again from your account as it is now ({}).{keeps} The current plan is set aside while the new one is made, and a copy is kept with the past plans.", how_long(which)), 13.5, pal(ui).text);
            ui.add_space(4.0);
            prose(ui, "To start over without what you've marked and chosen, use Reset plan in the … menu instead.", 12.5, pal(ui).weak);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Keep the plan").clicked() {
                    close = true;
                }
                if ui.add(crate::ui_util::primary_button(ui, "↻ Re-plan", 14.0)).clicked() {
                    self.plan_start(ctx, which, plan.choices_after());
                    close = true;
                }
            });
        });
        if close || modal.should_close() {
            self.lineage.p.confirm_replan = None;
        }
    }

    fn reset_dialog(&mut self, ctx: &egui::Context) {
        let Some(which) = self.lineage.p.confirm_reset else { return };
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("lineage_reset")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.label(semibold(format!("Reset the {} plan?", label(which)), size::LG));
            ui.add_space(6.0);
            prose(ui, &format!("This throws the plan away, with the careers marked done, the runners you kept or left out, and the edits waiting, and makes a new one from your account as it is now ({}). A copy is kept with the past plans.", how_long(which)), 13.5, pal(ui).text);
            ui.add_space(4.0);
            prose(ui, "To keep what you've marked and chosen, use Re-plan instead.", 12.5, pal(ui).weak);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Keep the plan").clicked() {
                    close = true;
                }
                if ui.add(egui::Button::new(RichText::new("Reset plan").color(egui::Color32::WHITE)).fill(pal(ui).bad)).clicked() {
                    self.team_plans().reset(which, &stamp());
                    self.save_team_plans();
                    self.plan_start(ctx, which, Choices::default());
                    close = true;
                }
            });
        });
        if close || modal.should_close() {
            self.lineage.p.confirm_reset = None;
        }
    }

    /// The build log while a plan is being made: each stage with its time and what it found,
    /// and Cancel.
    pub(crate) fn build_log(&mut self, ui: &mut egui::Ui, which: Which) {
        let Some(job) = self.lineage.p.job.as_ref().filter(|j| j.which == which) else { return };
        let p = pal(ui);
        let elapsed = ui.ctx().input(|i| i.time) - job.started;
        let stages = job.stages.clone();
        let has_plan = self.lineage.p.plans.as_ref().and_then(|ps| ps.get(which)).is_some();
        let mut cancel = false;
        crate::ui_util::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(semibold(format!("Making the {} plan", label(which)), size::MD));
                ui.label(RichText::new(format!("{elapsed:.0} s so far")).size(12.5).color(p.weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
            if has_plan {
                prose(ui, "Your current plan is set aside while this runs; Cancel brings it back unchanged.", 12.5, p.weak);
            }
            let all: &[&str] = match which {
                Which::Cm(_) => &crate::parentage::progress::CM_STAGES,
                Which::Tt => &crate::parentage::progress::TT_STAGES,
            };
            let done = stages.iter().filter(|s| s.secs.is_some()).count();
            let typical = match which {
                Which::Cm(_) => 3.0,
                Which::Tt => 18.0,
            };
            let left = typical - elapsed;
            ui.add_space(6.0);
            ui.add(egui::ProgressBar::new(done as f32 / all.len() as f32).desired_height(6.0).desired_width(ui.available_width()));
            ui.label(
                RichText::new(format!("Stage {} of {} · {}", stages.len().max(1), all.len(), if left >= 1.5 { format!("about {left:.0} s left") } else { "almost done".into() }))
                    .size(12.5)
                    .color(p.weak),
            );
            ui.add_space(12.0);
            stage_list(ui, &stages, true, all);
        });
        if cancel {
            if let Some(j) = &self.lineage.p.job {
                j.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    /// "How this plan was chosen": the finished build's log, folded.
    pub(crate) fn plan_chosen(&mut self, ui: &mut egui::Ui, which: Which) {
        let Some(plan) = self.team_plans().get(which).cloned() else { return };
        if plan.log.is_empty() {
            return;
        }
        ui.add_space(12.0);
        remembered_section(ui, &format!("chosen-{}", label(which)), "How this plan was chosen", false, |ui| {
            stage_list(ui, &plan.log, false, &[]);
        });
    }
}

/// An Overview card's one line on the plan's best way to raise it, under a dashed rule.
fn ceiling_hint(ui: &mut egui::Ui, l: Option<&crate::parentage::ceiling::Lever>) {
    let Some(l) = l else { return };
    let p = pal(ui);
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
    let mut x = r.left();
    while x < r.right() {
        ui.painter().line_segment([egui::pos2(x, r.top()), egui::pos2((x + 4.0).min(r.right()), r.top())], egui::Stroke::new(1.0, p.weak.gamma_multiply(0.5)));
        x += 8.0;
    }
    ui.add_space(6.0);
    let mut job = egui::text::LayoutJob::default();
    let font = |bold: bool| if bold { egui::FontId::new(13.0, egui::FontFamily::Name(crate::theme::SEMIBOLD.into())) } else { egui::FontId::proportional(13.0) };
    job.append("↑ Raise the ceiling: ", 0.0, egui::TextFormat { font_id: font(false), color: p.weak, ..Default::default() });
    job.append(&l.gain_text(), 0.0, egui::TextFormat { font_id: font(true), color: p.good, ..Default::default() });
    job.append(" · ", 0.0, egui::TextFormat { font_id: font(false), color: p.weak, ..Default::default() });
    job.append(if l.short.is_empty() { &l.title } else { &l.short }, 0.0, egui::TextFormat { font_id: font(false), color: p.text, ..Default::default() });
    job.wrap.max_rows = 1;
    job.wrap.max_width = ui.available_width();
    job.wrap.break_anywhere = true;
    ui.label(job).on_hover_text(&l.title);
}

/// Stages with a tick (or a spinner for the running one), time and lines. Wide, each stage is a
/// row: its number, title and time on the left, what it found on the right; narrow, the lines
/// sit under the title. The stages still to come follow, greyed, when `to_come` names them.
fn stage_list(ui: &mut egui::Ui, stages: &[Stage], live: bool, to_come: &[&str]) {
    let p = pal(ui);
    let wide = ui.available_width() >= 900.0;
    let left_w = 300.0;
    let mut rows: Vec<(usize, &str, Option<&Stage>)> = stages.iter().enumerate().map(|(i, s)| (i, s.title.as_str(), Some(s))).collect();
    rows.extend(to_come.iter().enumerate().filter(|(_, t)| !stages.iter().any(|s| s.title == **t)).map(|(i, t)| (i, *t, None)));
    for (k, (i, title, stage)) in rows.into_iter().enumerate() {
        if k > 0 {
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
        }
        let running = live && stage.is_some_and(|s| s.secs.is_none());
        let head = |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                match stage {
                    _ if running => {
                        ui.spinner();
                    }
                    Some(_) => {
                        ui.label(RichText::new("✔").color(p.accent).strong().size(15.0));
                    }
                    None => {
                        ui.label(RichText::new("○").color(p.weak).size(15.0));
                    }
                }
                let ink = if stage.is_some() { p.text } else { p.weak };
                ui.label(semibold(format!("{}. {title}", i + 1), 14.5).color(ink));
            });
            if let Some(t) = stage.and_then(|s| s.secs) {
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    ui.label(RichText::new(if t < 0.05 { "under 0.1 s".to_string() } else { format!("{t:.1} s") }).size(12.5).color(p.weak));
                });
            } else if running {
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    ui.label(RichText::new("running…").size(12.5).color(p.weak));
                });
            }
        };
        let lines = |ui: &mut egui::Ui| {
            for l in stage.map(|s| s.lines.as_slice()).unwrap_or_default() {
                ui.label(RichText::new(l).size(13.5).color(p.text.gamma_multiply(0.92)));
                ui.add_space(2.0);
            }
        };
        if wide {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(left_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(left_w);
                    head(ui);
                });
                ui.allocate_ui_with_layout(vec2(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_max_width(ui.available_width().min(900.0));
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    lines(ui);
                });
            });
        } else {
            head(ui);
            ui.horizontal_top(|ui| {
                ui.add_space(26.0);
                ui.vertical(|ui| lines(ui));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A brand-new account can't start a plan: nothing runs, and a toast says to set it up.
    #[test]
    fn planning_waits_for_the_account() {
        let (mut a, dir) = crate::onboarding::tests::scratch_app("plan-gate");
        let ctx = egui::Context::default();
        a.plan_start(&ctx, Which::Tt, Choices::default());
        assert!(a.lineage.p.job.is_none(), "a plan started on an empty account");
        assert!(a.pending_toasts.borrow().iter().any(|(t, ..)| t.starts_with("Add the umas and support cards")));
        // The planner refuses too, whoever calls it.
        assert!(a.make_tt_plan(&Choices::default()).is_err_and(|e| e.starts_with("Add the umas and support cards")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_up_means_umas_and_cards() {
        use crate::parentage::saved::not_set_up;
        assert!(not_set_up(0, 0).is_some_and(|w| w.contains("umas and support cards")));
        assert!(not_set_up(0, 5).is_some_and(|w| w.starts_with("Add the umas you own")));
        assert!(not_set_up(5, 0).is_some_and(|w| w.starts_with("Add the support cards")));
        // No finished umas doesn't block: it's a fresh account.
        assert_eq!(not_set_up(5, 5), None);
    }
}
