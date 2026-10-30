//! Home, the hub: a greeting with quick actions, the next Champions Meeting as a command centre,
//! the next three weeks of events, where you left off, your stable at a glance, and a few daily
//! picks. Its data comes from `home.rs`, rebuilt only when something changes.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::charts;
use crate::ui_util::UiExt;
use crate::data::{CardType, Rarity, Uma};
use crate::home::{Cat, HomeCache};
use crate::knowledge::{Distance, EventPreset, Style, grade_rank};
use crate::tab_planner::Plan;
use crate::theme::{pal, size};
use crate::ui_util::{badge, semibold, today, type_color, type_icon};
use crate::{App, CareerTab, CollectionTab, Tab};

/// Segments of a type's bar: SSR MLB, other SSR, SR MLB, other SR.
const SEGMENTS: [(&str, f32); 4] = [("SSR, maxed", 1.0), ("SSR", 0.6), ("SR, maxed", 0.38), ("SR", 0.2)];

/// The widgets below the timeline, in pairs on wide windows.
#[derive(Clone, Copy, PartialEq)]
enum Widget {
    Continue,
    Stable,
    Strength,
    Aptitude,
    UmaOfDay,
    Trainer,
    Showcase,
    Wishlist,
    Tip,
}

impl Widget {
    fn title(self) -> &'static str {
        match self {
            Widget::Continue => "Continue where you left off",
            Widget::Stable => "Your stable",
            Widget::Strength => "Support cards by type",
            Widget::Aptitude => "Who can run where",
            Widget::UmaOfDay => "Uma of the day",
            Widget::Trainer => "Trainer card",
            Widget::Showcase => "Your best cards for the next CM",
            Widget::Wishlist => "Missing pieces",
            Widget::Tip => "Did you know?",
        }
    }

    fn explain(self) -> &'static str {
        match self {
            Widget::Continue => "Your latest plans, race and results.",
            Widget::Stable => "How much of the game you own, and who still has potential to unlock.",
            Widget::Strength => "Cards for each stat; brighter segments are stronger, maxed (MLB) SSRs first.",
            Widget::Aptitude => "Owned umas with A or better at a distance and a running style. Gaps show at a glance.",
            Widget::UmaOfDay => "A different one of yours each day.",
            Widget::Trainer => "Badges for what you've done here.",
            Widget::Showcase => "Owned cards the community meta sheet backs most for the next CM's top style. Community meta: opinion, not game data.",
            Widget::Wishlist => "Cards in the community meta sheet's decks for the next CM that you don't own yet. Community meta: opinion, not game data.",
            Widget::Tip => "A game fact or term, with where it comes from. A new one each day.",
        }
    }
}

/// A titled card that lifts (shadow and accent border) while the pointer is over it, and with
/// `Some(bottom)` reaches down to that y (for `equal_columns`). Returns where it would end on its own.
fn widget_card(ui: &mut egui::Ui, title: &str, explain: &str, bottom: Option<f32>, add: impl FnOnce(&mut egui::Ui)) -> f32 {
    let p = pal(ui);
    let id = ui.make_persistent_id(("lift", title));
    let hovered = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let shadow = if hovered { egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(55) } } else { egui::Shadow::NONE };
    let mut natural = 0.0;
    let resp = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, if hovered { p.accent.gamma_multiply(0.6) } else { p.border }))
        .corner_radius(CornerRadius::same(crate::theme::RADIUS))
        .inner_margin(egui::Margin::same(14))
        .shadow(shadow)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // `ui.columns` lays out justified, which would also justify wrapped text; cards read
            // the same in columns as stacked.
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                if !title.is_empty() {
                    ui.label(semibold(title, size::MD));
                }
                if !explain.is_empty() {
                    ui.add(egui::Label::new(RichText::new(explain).size(size::SM).color(p.weak)).wrap());
                }
                if !title.is_empty() {
                    ui.add_space(6.0);
                }
                add(ui);
                natural = ui.min_rect().bottom() + 15.0;
                // Screen coordinates: a card scrolled above the window has a negative bottom, so only a
                // real target stretches it.
                if let Some(bottom) = bottom {
                    let extra = bottom - natural - ui.spacing().item_spacing.y;
                    if extra > 0.5 {
                        ui.allocate_space(vec2(0.0, extra));
                    }
                }
            });
        });
    let now = resp.response.contains_pointer();
    if now != hovered {
        ui.data_mut(|d| d.insert_temp(id, now));
        ui.ctx().request_repaint();
    }
    ui.add_space(12.0);
    natural
}

impl App {
    pub(crate) fn overview(&mut self, ui: &mut egui::Ui) {
        let cache = self.home_cache();
        let now_t = ui.input(|i| i.time);
        let shown = *self.home.shown_at.get_or_insert(now_t);
        // Numbers count up once a session, over 0.9 s.
        let t = (((now_t - shown) / 0.9) as f32).clamp(0.0, 1.0);
        let ease = 1.0 - (1.0 - t).powi(3);
        if t < 1.0 {
            ui.ctx().request_repaint();
        }
        // Countdowns tick while Home is open.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(30));
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        // UMALORD_SCROLL holds it scrolled down, for screenshots of the lower widgets.
        if let Some(y) = self.debug_scroll {
            scroll = scroll.vertical_scroll_offset(y);
        }
        scroll.show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(1240.0));
            self.home_hero(ui);
            if self.roster.is_empty() {
                self.home_empty(ui);
            }
            self.home_cm(ui, &cache);
            self.home_timeline(ui, &cache);
            let wide = ui.available_width() >= 1100.0;
            let rows = [(Widget::Continue, Widget::Stable), (Widget::Strength, Widget::Aptitude), (Widget::UmaOfDay, Widget::Trainer), (Widget::Showcase, Widget::Wishlist)];
            for (k, (a, b)) in rows.into_iter().enumerate() {
                if wide {
                    crate::ui_util::equal_columns(ui, &format!("home_row_{k}"), 2, |col, i, bottom| {
                        let w = if i == 0 { a } else { b };
                        widget_card(col, w.title(), w.explain(), Some(bottom), |ui| self.home_widget(ui, w, &cache, ease))
                    });
                } else {
                    for w in [a, b] {
                        widget_card(ui, w.title(), w.explain(), None, |ui| self.home_widget(ui, w, &cache, ease));
                    }
                }
            }
            widget_card(ui, Widget::Tip.title(), Widget::Tip.explain(), None, |ui| self.home_widget(ui, Widget::Tip, &cache, ease));
        });
    }

    fn home_widget(&mut self, ui: &mut egui::Ui, w: Widget, c: &HomeCache, ease: f32) {
        match w {
            Widget::Continue => self.home_continue(ui, c),
            Widget::Stable => self.home_stable(ui, c, ease),
            Widget::Strength => self.cards_by_type(ui),
            Widget::Aptitude => self.home_aptitude(ui, c),
            Widget::UmaOfDay => self.home_uma_of_day(ui),
            Widget::Trainer => self.home_trainer(ui, c),
            Widget::Showcase => self.home_showcase(ui, c),
            Widget::Wishlist => self.home_wishlist(ui, c),
            Widget::Tip => self.home_tip(ui, c),
        }
    }

    /// "Good evening, Sam", today's date, quick actions, and Seiun Sky in a pose for the hour.
    fn home_hero(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let (_, month, day, hour, _, weekday) = crate::live::local(crate::live::now());
        let (greeting, pose) = match hour {
            5..=11 => ("Good morning", 0),
            12..=16 => ("Good afternoon", 1),
            17..=21 => ("Good evening", 3),
            _ => ("Hello, night owl", 5),
        };
        const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
        const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        let date = format!("{}, {} {day}", DAYS[weekday as usize % 7], MONTHS[(month as usize).clamp(1, 12) - 1]);
        let name = self.roster.profile.clone().unwrap_or_else(|| "Trainer".into());
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(w, 150.0), Sense::hover());
        crate::ui_util::gradient_rect(ui.painter(), rect, 16.0, p.surface, p.sky);
        ui.painter().rect_stroke(rect, CornerRadius::same(16), Stroke::new(1.0, p.border), StrokeKind::Inside);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(22.0, 16.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let mut go = None;
        child.vertical(|ui| {
            ui.set_max_width(rect.width() - 190.0);
            ui.label(semibold(format!("{greeting}, {name}"), size::XXL));
            ui.label(RichText::new(format!("{date} · {} umas · {} support cards", self.roster.umas.len(), self.roster.cards.len())).color(p.weak));
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                for (k, (icon, label, hover)) in [
                    ("📋", "Plan a career", "Career → Plan"),
                    ("🏁", "Race sim", "Race two builds against each other"),
                    ("＋", "Add pulls", "Add new cards, or a card's new limit break"),
                    ("🔍", "Look up a skill", "Collection → Skills"),
                    ("💬", "Ask", "Questions about your account in plain English"),
                ]
                .into_iter()
                .enumerate()
                {
                    let b = egui::Button::new(RichText::new(format!("{icon} {label}")).size(size::BODY)).fill(p.surface).corner_radius(CornerRadius::same(10)).min_size(vec2(0.0, 32.0));
                    if ui.add(b).on_hover_text(hover).clicked() {
                        go = Some(k);
                    }
                }
            });
        });
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::theme::chibi() != crate::theme::Chibi::Off {
                crate::ui_util::chibi(ui, pose, 118.0).on_hover_text("Seiun Sky. ⚙ Settings moves or hides her.");
            }
        });
        match go {
            Some(0) => self.go_career(CareerTab::Plan),
            Some(1) => self.go_career(CareerTab::Race),
            Some(2) => {
                self.tab = Tab::Collection;
                self.collection_tab = CollectionTab::Cards;
                self.cards.add.open = true;
            }
            Some(3) => {
                self.tab = Tab::Collection;
                self.collection_tab = CollectionTab::Skills;
                self.focus_search.set(true);
            }
            Some(4) => self.tab = Tab::Ask,
            _ => {}
        }
        ui.add_space(14.0);
    }

    fn home_empty(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        crate::ui_util::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(semibold("Your collection is empty", 16.0));
                ui.label(RichText::new("Load a saved profile, or click the umas and cards you own. Plans and picks need them.").color(p.weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(crate::ui_util::primary_button(ui, "✨ Set up", 14.0)).clicked() {
                        self.open_setup(crate::onboarding::Step::Welcome);
                    }
                });
            });
        });
        ui.add_space(12.0);
    }

    /// The next Champions Meeting: dates and countdowns, the course strip, conditions, the skills
    /// it turns off, the community meta, your best trainees for it, and your plan's progress.
    fn home_cm(&mut self, ui: &mut egui::Ui, c: &HomeCache) {
        let Some(cm) = c.cm.clone() else { return };
        let p = pal(ui);
        let now = crate::live::now();
        let mut action: Option<CmAction> = None;
        widget_card(ui, "", "", None, |ui| {
            // Header: name, status, the buttons.
            ui.horizontal(|ui| {
                let running = cm.round1.is_some_and(|t| t <= now) && cm.ends.is_none_or(|e| e > now);
                ui.label(RichText::new("🏆").size(22.0));
                ui.label(semibold(format!("{}: {}", if running { "Champions Meeting on now" } else { "Next Champions Meeting" }, cm.name), size::LG + 1.0));
                if cm.announced {
                    badge(ui, "✔ Announced", p.good).on_hover_text(cm.status_explained());
                } else {
                    badge(ui, "≈ Estimated", p.warn).on_hover_text(cm.status_explained());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(plan) = &c.plan {
                        if ui.add(egui::Button::new("▶ Simulate").min_size(vec2(0.0, 32.0))).on_hover_text("Race your plan's trainee with and without her skills to buy").clicked() {
                            action = Some(CmAction::Simulate(plan.path.clone()));
                        }
                        if ui.add(crate::ui_util::primary_button(ui, "📋 Continue plan", size::BODY).min_size(vec2(0.0, 32.0))).on_hover_text(&plan.title).clicked() {
                            action = Some(CmAction::Open(plan.path.clone()));
                        }
                    } else if ui.add(crate::ui_util::primary_button(ui, "📋 Plan for it", size::BODY).min_size(vec2(0.0, 32.0))).on_hover_text("Start a new plan with this course and its conditions filled in").clicked() {
                        action = Some(CmAction::New);
                    }
                    if let Some(id) = cm.article {
                        crate::ui_util::web_link(ui, "Announcement", &crate::live::article_url(id));
                    }
                });
            });
            ui.horizontal_wrapped(|ui| {
                let course = ui.label(RichText::new(&cm.course_name).strong());
                if let Some(text) = &cm.official_conditions {
                    course.on_hover_text(format!("As announced: {text}"));
                }
                ui.label(RichText::new(format!("· {}", cm.conditions)).color(p.weak));
                if let Some(sc) = self.kb.cm_scenario(&cm) {
                    ui.label(RichText::new(format!("· trained in {sc}")).color(p.weak));
                }
            });
            ui.add_space(6.0);
            self.cm_milestones(ui, &cm, now);
            if let Some(course) = self.sim_data.as_ref().and_then(|d| d.courses.get(&cm.course)) {
                ui.add_space(8.0);
                charts::course_strip(ui, course, 76.0, &[]);
            }
            if !cm.disabled_skills.is_empty() {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new("Won't activate:").size(12.5).color(p.weak))
                        .on_hover_text("The official announcement turns these skills off for this Champions Meeting. Auto-pick never buys them for it.");
                    for (base, marks) in disabled_groups(&self.kb, &cm.disabled_skills) {
                        crate::ui_util::pill(ui, RichText::new(format!("{base} {marks}")).size(12.5).strikethrough().color(p.weak), p.raised, Stroke::NONE);
                    }
                });
            }
            ui.add_space(10.0);
            let cols = if ui.available_width() >= 860.0 { 3 } else { 1 };
            let mut parts = |ui: &mut egui::Ui, k: usize| match k {
                0 => self.cm_meta_part(ui, c),
                1 => {
                    if let Some(a) = self.cm_best_part(ui, c, &cm) {
                        action = Some(a);
                    }
                }
                _ => self.cm_plan_part(ui, c),
            };
            if cols == 3 {
                ui.columns(3, |cols| {
                    for (k, col) in cols.iter_mut().enumerate() {
                        parts(col, k);
                    }
                });
            } else {
                for k in 0..3 {
                    parts(ui, k);
                    ui.add_space(8.0);
                }
            }
        });
        match action {
            Some(CmAction::New) => self.plan_for_cm(&cm),
            Some(CmAction::Open(path)) => {
                if self.open_plan_file(&path) {
                    self.go_career(CareerTab::Plan);
                }
            }
            Some(CmAction::Simulate(path)) => {
                if self.open_plan_file(&path) {
                    let ctx = ui.ctx().clone();
                    if let Err(e) = self.simulate_plan(&ctx) {
                        self.toast(e, crate::ui_util::ToastKind::Warn);
                        self.go_career(CareerTab::Plan);
                    }
                }
            }
            Some(CmAction::Trainee(id)) => {
                self.plan_for_cm(&cm);
                self.planner.plan.trainee = Some(id);
                if let Some(m) = &c.meta {
                    self.planner.plan.style = m.top;
                }
            }
            None => {}
        }
    }

    /// League selection, Round 1 and the end, as countdown chips (local time; UTC on hover).
    fn cm_milestones(&self, ui: &mut egui::Ui, cm: &EventPreset, now: i64) {
        let p = pal(ui);
        let milestones: Vec<(&str, i64)> = [("League selection", cm.league_selection), ("Round 1", cm.round1), ("Ends", cm.ends)].into_iter().filter_map(|(l, t)| Some((l, t?))).collect();
        if milestones.is_empty() {
            let est = crate::live::date_epoch(&cm.date).map(|t| format!(" ({})", crate::live::fmt_until(t - now))).unwrap_or_default();
            ui.label(RichText::new(format!("Round 1 around {}{est}, by Umalator's estimate. The official dates come with the announcement.", crate::live::short_day(&cm.date))).color(p.weak));
            return;
        }
        let next = milestones.iter().position(|(_, t)| *t > now);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for (i, (label, t)) in milestones.iter().enumerate() {
                let past = *t <= now;
                let on = Some(i) == next;
                egui::Frame::new()
                    .fill(if on { p.accent.gamma_multiply(if ui.visuals().dark_mode { 0.22 } else { 0.12 }) } else { p.raised })
                    .stroke(Stroke::new(1.0, if on { p.accent_strong } else { p.border }))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(10, 5))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(*label).size(11.5).color(p.weak));
                            ui.label(semibold(crate::live::fmt_local(*t), 14.0).color(if past { p.weak } else { p.text }));
                            let when = if past { format!("✔ {}", crate::live::fmt_until(t - now)) } else { crate::live::fmt_until(t - now) };
                            ui.label(RichText::new(when).size(12.0).color(if on { p.accent_strong } else { p.weak }));
                        });
                    })
                    .response
                    .on_hover_text(format!("{}\nShown in your computer's time zone.", crate::live::fmt_utc(*t)));
            }
        });
    }

    /// The community meta's style ranking and recommended stats.
    fn cm_meta_part(&self, ui: &mut egui::Ui, c: &HomeCache) {
        let p = pal(ui);
        ui.label(semibold("Community meta", size::BODY)).on_hover_text(
            self.kb.pvp_meta.as_ref().map_or(String::new(), |m| format!("{}\n{}\nOpinion, not game data. Read {}.", m.source, m.authors, m.fetched)),
        );
        let Some(m) = &c.meta else {
            ui.label(RichText::new("The community sheet has nothing for this CM yet.").size(size::SM).color(p.weak));
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, s) in m.ranking.iter().enumerate() {
                if i > 0 {
                    ui.label(RichText::new("›").color(p.weak));
                }
                let label = Style::ALL.into_iter().find(|st| crate::meta::style_key(*st) == s).map_or(s.as_str(), |st| st.label());
                if i == 0 {
                    badge(ui, label, p.accent_strong);
                } else {
                    ui.label(RichText::new(label).size(size::SM));
                }
            }
        });
        if m.stats.iter().any(|s| !s.is_empty()) {
            ui.add_space(4.0);
            ui.label(RichText::new("Recommended stats").size(size::SM).color(p.weak));
            egui::Grid::new("home_meta_stats").spacing([10.0, 2.0]).show(ui, |ui| {
                for name in ["Spd", "Sta", "Pow", "Gut", "Wit"] {
                    ui.label(RichText::new(name).size(size::XS).color(p.weak));
                }
                ui.end_row();
                for v in m.stats.iter().take(5) {
                    ui.label(semibold(if v.is_empty() { "—" } else { v.as_str() }, size::SM));
                }
                ui.end_row();
            });
        }
    }

    /// Your three best trainees for the CM's top style; a click starts a plan with her.
    fn cm_best_part(&self, ui: &mut egui::Ui, c: &HomeCache, cm: &EventPreset) -> Option<CmAction> {
        let p = pal(ui);
        let style = c.meta.as_ref().map_or(Style::Pace, |m| m.top);
        ui.label(semibold(format!("Your best {}s for it", style.label()), size::BODY))
            .on_hover_text("The community sheet's picks you own first (★), then your umas with the best aptitudes for this course and style.");
        if c.best.is_empty() {
            ui.label(RichText::new("Add umas in Collection to see who fits.").size(size::SM).color(p.weak));
            return None;
        }
        let meta_ids: Vec<u32> = self
            .kb
            .pvp_meta
            .as_ref()
            .and_then(|m| m.events.iter().find(|e| e.series == "CM" && e.number == cm.id))
            .and_then(|e| e.styles.get(crate::meta::style_key(style)))
            .map(|s| s.umas.iter().filter_map(|u| u.id).collect())
            .unwrap_or_default();
        let course = self.kb.umalator_courses.iter().find(|u| u.id == cm.course);
        let distance = course.map_or(Distance::Medium, |u| Distance::of_metres(u.distance));
        let mut picked = None;
        ui.horizontal(|ui| {
            for id in &c.best {
                let Some(info) = self.kb.umas.get(id) else { continue };
                let (rect, resp) = ui.allocate_exact_size(vec2(84.0, 96.0), Sense::click());
                let img = Rect::from_min_size(pos2(rect.center().x - 26.0, rect.top()), vec2(52.0, 52.0));
                if resp.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(8), p.hover);
                }
                if let Some(uri) = self.art_uri("umas", *id) {
                    crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same(8)), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)));
                }
                let star = if meta_ids.contains(id) { "★ " } else { "" };
                ui.painter().text(pos2(rect.center().x, img.bottom() + 11.0), Align2::CENTER_CENTER, format!("{star}{}", crate::ui_util::truncate(&info.name, 12)), FontId::proportional(12.0), p.text);
                let a = &info.aptitude;
                let (dg, sg) = (a.distance(distance), a.style(style));
                let weak = grade_rank(dg) < 6 || grade_rank(sg) < 6;
                ui.painter().text(
                    pos2(rect.center().x, img.bottom() + 26.0),
                    Align2::CENTER_CENTER,
                    format!("{} {dg} · {} {sg}", distance.label(), style.label().split(' ').next().unwrap_or("")),
                    FontId::proportional(10.5),
                    if weak { p.warn } else { p.weak },
                );
                let warn = if weak { "\n⚠ Below B here: she'd be slow without pink sparks from parents." } else { "" };
                if resp.secondary_clicked() {
                    self.open_detail(crate::detail_modal::Detail::Uma(*id), None);
                }
                if resp.on_hover_text(format!("{} [{}]{warn}\nClick to plan the CM with her; right-click for her details", info.name, info.outfit)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    picked = Some(CmAction::Trainee(*id));
                }
            }
        });
        picked
    }

    /// How far your plan for this CM has got.
    fn cm_plan_part(&self, ui: &mut egui::Ui, c: &HomeCache) {
        let p = pal(ui);
        ui.label(semibold("Your plan", size::BODY));
        let Some(plan) = &c.plan else {
            ui.label(RichText::new("No plan for this CM yet. Plan for it fills in the course and conditions.").size(size::SM).color(p.weak));
            return;
        };
        ui.label(RichText::new(crate::ui_util::truncate(&plan.title, 40)).size(size::SM).color(p.weak)).on_hover_text(&plan.title);
        let trainee = plan.trainee.and_then(|t| self.kb.umas.get(&t)).map(|u| u.name.clone());
        for (done, text) in [
            (trainee.is_some(), trainee.map_or("Pick a trainee".to_string(), |n| format!("Trainee: {n}"))),
            (plan.deck >= 6, format!("Deck: {} of 6 cards", plan.deck)),
            (plan.skills > 0, if plan.skills > 0 { format!("{} skills to buy", plan.skills) } else { "Pick skills to buy".into() }),
            (plan.simulated, if plan.simulated { "Simulated".into() } else { "Not simulated yet".into() }),
        ] {
            ui.horizontal(|ui| {
                ui.label(RichText::new(if done { "✔" } else { "○" }).color(if done { p.good } else { p.weak }));
                ui.label(RichText::new(text).size(size::SM).color(if done { p.text } else { p.weak }));
            });
        }
    }

    /// The next three weeks as bars on a time axis: CMs, Legend Races, events, scouts and campaigns.
    fn home_timeline(&mut self, ui: &mut egui::Ui, c: &HomeCache) {
        let p = pal(ui);
        let dark = ui.visuals().dark_mode;
        let loading = self.live.rx.is_some() && self.live.data.is_none();
        widget_card(ui, "Coming up", "The next three weeks from the game's news. Click a bar to read its announcement.", None, |ui| {
            if loading {
                crate::ui_util::skeleton_lines(ui, 4);
                return;
            }
            if c.timeline.is_empty() {
                let why = if self.live.data.is_some() { "Nothing announced for the next three weeks yet." } else { "The news hasn't been fetched yet (Settings → Data and updates)." };
                ui.label(RichText::new(why).color(p.weak));
                return;
            }
            let now = crate::live::now();
            let days = 21i64;
            let w = ui.available_width();
            let lane_h = 26.0;
            let font = FontId::proportional(12.0);
            // Pack bars into lanes, leaving room for labels that don't fit inside.
            let x_of = |t: i64, left: f32, width: f32| left + width * ((t - now) as f32 / (days * 86_400) as f32).clamp(0.0, 1.0);
            let mut lanes: Vec<f32> = Vec::new();
            let mut placed: Vec<(usize, usize)> = Vec::new();
            for (k, item) in c.timeline.iter().enumerate() {
                let (a, b) = (x_of(item.start, 0.0, w), x_of(item.end, 0.0, w));
                let label_w = ui.painter().layout_no_wrap(item.name.clone(), font.clone(), p.text).size().x + 14.0;
                let occupied_to = if b - a >= label_w { b } else { b + label_w };
                let lane = lanes.iter().position(|end| *end + 6.0 <= a).unwrap_or_else(|| {
                    lanes.push(f32::NEG_INFINITY);
                    lanes.len() - 1
                });
                lanes[lane] = occupied_to;
                placed.push((k, lane));
            }
            let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0 + lanes.len() as f32 * lane_h), Sense::click());
            let painter = ui.painter_at(rect);
            let x = |t: i64| x_of(t, rect.left(), rect.width());
            // Day ticks, labels every few days, and today.
            const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            for d in 0..=days {
                let tx = x(now + d * 86_400);
                painter.line_segment([pos2(tx, rect.top() + 16.0), pos2(tx, rect.bottom())], Stroke::new(1.0, p.border.gamma_multiply(if d % 7 == 0 { 1.0 } else { 0.45 })));
                if d % 3 == 0 && d < days {
                    let (_, m, day, _, _, _) = crate::live::local(now + d * 86_400);
                    let label = if d == 0 { "Today".to_string() } else { format!("{} {day}", MONTHS[(m as usize).clamp(1, 12) - 1]) };
                    painter.text(pos2(tx + 3.0, rect.top()), Align2::LEFT_TOP, label, FontId::proportional(11.0), p.weak);
                }
            }
            painter.line_segment([pos2(rect.left() + 1.0, rect.top() + 14.0), pos2(rect.left() + 1.0, rect.bottom())], Stroke::new(2.0, p.accent));
            let mut hovered = None;
            for (k, lane) in &placed {
                let item = &c.timeline[*k];
                let top = rect.top() + 22.0 + *lane as f32 * lane_h;
                let bar = Rect::from_min_max(pos2(x(item.start).max(rect.left()), top), pos2(x(item.end).max(x(item.start) + 6.0), top + lane_h - 6.0));
                let color = charts::series(item.cat.slot(), dark);
                let fill = if item.estimated { color.gamma_multiply(0.45) } else { color };
                let r = CornerRadius::same(4);
                painter.rect_filled(bar, r, fill);
                if item.estimated {
                    painter.rect_stroke(bar, r, Stroke::new(1.0, color), StrokeKind::Inside);
                }
                let galley = painter.layout_no_wrap(item.name.clone(), font.clone(), p.text);
                if galley.size().x + 12.0 <= bar.width() {
                    painter.text(pos2(bar.left() + 6.0, bar.center().y), Align2::LEFT_CENTER, &item.name, font.clone(), charts::ink_on(fill));
                } else {
                    painter.text(pos2(bar.right() + 6.0, bar.center().y), Align2::LEFT_CENTER, &item.name, font.clone(), p.text);
                }
                if resp.hover_pos().is_some_and(|h| bar.expand2(vec2(0.0, 3.0)).contains(h)) {
                    painter.rect_stroke(bar, r, Stroke::new(1.5, p.text), StrokeKind::Outside);
                    hovered = Some(*k);
                }
            }
            if let Some(k) = hovered {
                let item = &c.timeline[k];
                let mut text = format!("{}\n{} → {}", item.name, crate::live::fmt_local(item.start), crate::live::fmt_local(item.end));
                if !item.detail.is_empty() {
                    text += &format!("\n{}", item.detail);
                }
                if item.article.is_some() {
                    text += "\nClick to read the announcement.";
                }
                let clicked = resp.clicked();
                resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(text);
                if clicked && let Some(id) = item.article {
                    crate::ui_util::open_url(&crate::live::article_url(id));
                }
            }
            ui.add_space(6.0);
            let present: Vec<(Color32, &str)> = Cat::ALL.into_iter().filter(|cat| c.timeline.iter().any(|i| i.cat == *cat)).map(|cat| (charts::series(cat.slot(), dark), cat.label())).collect();
            charts::legend(ui, &present);
        });
    }

    fn home_continue(&mut self, ui: &mut egui::Ui, c: &HomeCache) {
        let p = pal(ui);
        let now = crate::live::now();
        let mut open = None;
        if c.recent_plans.is_empty() && self.sim.compare.is_none() && c.recent_results.is_empty() {
            ui.label(RichText::new("Nothing to pick up yet. Plans you save, races you simulate and results you log show up here.").size(size::SM).color(p.weak));
            ui.add_space(6.0);
            if ui.button("📋 Plan a career").clicked() {
                self.go_career(CareerTab::Plan);
            }
        } else if c.recent_plans.is_empty() {
            ui.label(RichText::new("No saved plans yet: Career → Plan makes one.").size(size::SM).color(p.weak));
        }
        for (path, title, modified) in c.recent_plans.iter().take(3) {
            ui.horizontal(|ui| {
                if ui.link(format!("📋 {}", crate::ui_util::truncate(title, 44))).on_hover_text(title).clicked() {
                    open = Some(path.clone());
                }
                ui.label(RichText::new(format!("saved {}", crate::live::fmt_ago(now - modified))).size(size::XS).color(p.weak));
            });
        }
        if let Some(path) = open
            && self.open_plan_file(&path)
        {
            self.go_career(CareerTab::Plan);
        }
        if let Some(r) = &self.sim.compare {
            ui.add_space(6.0);
            let lead = r.mean.abs();
            let who = if r.mean >= 0.0 { "Uma 2" } else { "Uma 1" };
            let runs = r.gaps.len();
            let mut open_sim = false;
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("🏁 Last race:").color(p.weak));
                ui.label(format!("{who} won by {lead:.2} L on average over {runs} runs."));
                open_sim = ui.link("Open").clicked();
            });
            if open_sim {
                self.go_career(CareerTab::Race);
            }
        }
        if !c.recent_results.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Latest results").size(size::SM).color(p.weak));
            for (log, date, summary) in c.recent_results.iter().take(3) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(semibold(*log, size::SM));
                    ui.label(RichText::new(format!("{date} · {summary}")).size(size::SM).color(p.weak));
                });
            }
        }
    }

    fn home_stable(&mut self, ui: &mut egui::Ui, c: &HomeCache, ease: f32) {
        let p = pal(ui);
        let dark = ui.visuals().dark_mode;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            for (k, (label, (owned, total))) in ["Umas", "SSR cards", "SR cards"].into_iter().zip(c.completion).enumerate() {
                let shown = (owned as f32 * ease).round();
                charts::ring(ui, 92.0, shown, total as f32, charts::series([0, 4, 2][k], dark), &format!("{shown:.0}"), &format!("{label} of {total}"))
                    .on_hover_text(format!("You own {owned} of the {total} {label} released on Global ({:.0}%).", owned as f32 / total.max(1) as f32 * 100.0));
            }
        });
        let mut below: Vec<Uma> = self.roster.umas.iter().filter(|u| u.potential < 5).cloned().collect();
        below.sort_by(|a, b| b.stars.cmp(&a.stars).then(a.potential.cmp(&b.potential)).then(a.name.cmp(&b.name)));
        ui.add_space(8.0);
        ui.label(semibold(format!("Potential to raise ({})", below.len()), size::SM)).on_hover_text("Umas with skills still locked behind potential, highest stars first. Click one to see which.");
        if below.is_empty() {
            ui.label(RichText::new("Every uma is at P5. Nice!").color(p.good));
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for u in below.iter().take(5) {
                let (rect, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
                if let Some(uri) = self.art_uri("umas", u.id) {
                    crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same(8)), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)));
                }
                crate::ui_util::potential_badge(ui.painter(), rect.right_bottom(), u.potential);
                if resp.on_hover_text(format!("{} {}★ P{} · click for details", u.display_name(), u.stars, u.potential)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.open_detail(crate::detail_modal::Detail::Uma(u.id), None);
                }
            }
            if below.len() > 5 {
                ui.label(RichText::new(format!("+{} more", below.len() - 5)).color(p.weak));
            }
        });
    }

    fn cards_by_type(&self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let cards = &self.roster.cards;
        let rows: Vec<(CardType, [usize; 4])> = CardType::ALL
            .into_iter()
            .map(|t| {
                let of = |r: Rarity, mlb: bool| cards.iter().filter(|c| c.card_type == t && c.rarity == r && c.is_mlb() == mlb).count();
                (t, [of(Rarity::SSR, true), of(Rarity::SSR, false), of(Rarity::SR, true), of(Rarity::SR, false)])
            })
            .filter(|(_, n)| n.iter().sum::<usize>() > 0)
            .collect();
        if rows.is_empty() {
            ui.label(RichText::new("No support cards yet.").color(p.weak));
            return;
        }
        let max = rows.iter().map(|(_, n)| n.iter().sum::<usize>()).max().unwrap_or(1).max(1);
        for (t, n) in &rows {
            ui.horizontal(|ui| {
                ui.add_sized([86.0, 18.0], egui::Label::new(RichText::new(format!("{} {}", type_icon(t.label()), t.label())).size(size::SM).color(crate::ui_util::type_text(ui, t.label()))));
                let width = (ui.available_width() - 52.0).max(100.0);
                let (rect, resp) = ui.allocate_exact_size(vec2(width, 14.0), Sense::hover());
                let painter = ui.painter();
                painter.rect_filled(rect, CornerRadius::same(4), p.raised);
                let color = type_color(*t);
                let mut x = rect.left();
                for (i, &k) in n.iter().enumerate() {
                    if k == 0 {
                        continue;
                    }
                    let w = width * k as f32 / max as f32;
                    let seg = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height()));
                    let fill = color.gamma_multiply(SEGMENTS[i].1);
                    // A 2 px gap between segments.
                    painter.rect_filled(Rect::from_min_max(seg.min, pos2(seg.right() - 2.0, seg.bottom())), CornerRadius::same(3), fill);
                    if w > 18.0 {
                        painter.text(seg.center() - vec2(1.0, 0.0), Align2::CENTER_CENTER, k.to_string(), FontId::proportional(10.5), charts::ink_on(fill));
                    }
                    x += w;
                }
                resp.on_hover_text(format!("{}: SSR maxed {} · other SSR {} · SR maxed {} · other SR {}", t.label(), n[0], n[1], n[2], n[3]));
                ui.label(RichText::new(n.iter().sum::<usize>().to_string()).size(size::SM).color(p.weak));
            });
        }
        ui.add_space(4.0);
        // The legend: each segment's shade, on one neutral colour.
        let items: Vec<(Color32, &str)> = SEGMENTS.iter().map(|(label, a)| (p.accent.gamma_multiply(*a), *label)).collect();
        charts::legend(ui, &items);
    }

    fn home_aptitude(&self, ui: &mut egui::Ui, c: &HomeCache) {
        let p = pal(ui);
        let rows: Vec<&str> = Distance::ALL.iter().map(|d| d.label()).collect();
        let cols = ["Front", "Pace", "Late", "End"];
        let w = ((ui.available_width() - 70.0) / 4.0).clamp(48.0, 110.0);
        charts::heat_grid(ui, &rows, &cols, &c.aptitude, vec2(w, 28.0), |i, j| {
            let names = &c.aptitude_names[i][j];
            format!("{} {}: {}", Distance::ALL[i].label(), Style::ALL[j].label(), if names.is_empty() { "nobody with A or better yet".to_string() } else { names.clone() })
        });
        let gaps: Vec<String> = (0..4).flat_map(|i| (0..4).map(move |j| (i, j))).filter(|(i, j)| c.aptitude[*i][*j] == 0).map(|(i, j)| format!("{} {}", Distance::ALL[i].label(), Style::ALL[j].label())).collect();
        if !gaps.is_empty() {
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(format!("No one yet for: {}. Pink sparks from parents can raise an aptitude.", gaps.join(", "))).size(size::SM).color(p.weak)).wrap());
        }
    }

    /// Today's uma: seeded by the date, ↻ shows another.
    fn home_uma_of_day(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let mut umas: Vec<Uma> = self.roster.umas.clone();
        if umas.is_empty() {
            ui.label(RichText::new("Add umas to meet one each day.").color(p.weak));
            return;
        }
        umas.sort_by_key(|u| u.id);
        let day = crate::tab_planner::day_number(&today()).max(0) as usize;
        let u = umas[(day.wrapping_mul(2_654_435_761) + self.home.uma_offset) % umas.len()].clone();
        let Some(info) = self.kb.umas.get(&u.id).cloned() else { return };
        let mut plan = false;
        ui.horizontal(|ui| {
            if let Some(uri) = self.art_uri("umas", u.id) {
                let art = egui::Image::new(uri).fit_to_exact_size(vec2(84.0, 84.0)).corner_radius(CornerRadius::same(12)).sense(Sense::click());
                if ui.add(art).on_hover_text("Click for her details").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.open_detail(crate::detail_modal::Detail::Uma(u.id), None);
                }
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(semibold(&u.name, size::LG));
                    if ui.small_btn("↻").on_hover_text("Another one").clicked() {
                        self.home.uma_offset += 1;
                    }
                });
                ui.label(RichText::new(format!("{} · {}★ P{}", u.outfit, u.stars, u.potential)).color(p.weak));
                // Her best aptitudes.
                let a = &info.aptitude;
                let mut best: Vec<(String, String)> = Distance::ALL
                    .iter()
                    .map(|d| (d.label().to_string(), a.distance(*d).to_string()))
                    .chain(Style::ALL.iter().map(|s| (s.label().to_string(), a.style(*s).to_string())))
                    .collect();
                best.sort_by_key(|(_, g)| std::cmp::Reverse(grade_rank(g)));
                let top: Vec<String> = best.iter().take(2).map(|(n, g)| format!("{n} {g}")).collect();
                ui.label(RichText::new(format!("Best at {}", top.join(" · "))).size(size::SM));
                if let Some(unique) = info.skills.iter().find(|s| self.kb.skills.get(&s.id).is_some_and(|k| k.tier() == "Unique")) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Unique").size(size::SM).color(p.weak));
                        self.skill_link(ui, unique.id, false);
                    });
                }
                plan = ui.add(egui::Button::new("📋 Plan with her")).clicked();
            });
        });
        if plan {
            match self.kb.upcoming_cms(&today()).first().copied().cloned() {
                Some(cm) => self.plan_for_cm(&cm),
                None => {
                    self.planner.plan = Plan::default();
                    self.planner.open_file = None;
                    self.go_career(CareerTab::Plan);
                }
            }
            self.planner.plan.trainee = Some(u.id);
        }
    }

    fn home_trainer(&self, ui: &mut egui::Ui, c: &HomeCache) {
        let p = pal(ui);
        ui.horizontal(|ui| {
            ui.label(semibold(self.roster.profile.clone().unwrap_or_else(|| "Trainer".into()), size::LG));
            if let Some(since) = &c.trainer_since {
                ui.label(RichText::new(format!("trainer since {since}")).size(size::SM).color(p.weak));
            }
        });
        let earned = c.badges.iter().filter(|b| b.3).count();
        ui.label(RichText::new(format!("{earned} of {} badges", c.badges.len())).size(size::SM).color(p.weak));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for (icon, name, how, got) in &c.badges {
                let fill = if *got { p.accent.gamma_multiply(if ui.visuals().dark_mode { 0.22 } else { 0.12 }) } else { p.raised };
                let stroke = Stroke::new(1.0, if *got { p.accent_strong } else { p.border });
                let text = RichText::new(format!("{icon} {name}")).size(size::SM).color(if *got { p.text } else { p.weak });
                crate::ui_util::pill(ui, text, fill, stroke).on_hover_text(if *got { format!("Earned: {how}") } else { format!("To earn it: {how}") });
            }
        });
    }

    /// Card art tiles with a caption; a click opens the card.
    fn art_tiles(&self, ui: &mut egui::Ui, ids: &[(u32, String)]) {
        let p = pal(ui);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for (id, caption) in ids {
                let Some(info) = self.kb.cards.get(id) else { continue };
                let (rect, resp) = ui.allocate_exact_size(vec2(60.0, 98.0), Sense::click());
                let img = Rect::from_min_size(rect.min, vec2(60.0, 80.0));
                if resp.hovered() {
                    ui.painter().add(egui::Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_black_alpha(60) }.as_shape(img, CornerRadius::same(6)));
                }
                match self.art_uri("cards", *id) {
                    Some(uri) => crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same(6)), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))),
                    None => crate::ui_util::placeholder(ui.painter(), img, crate::ui_util::type_color_str(&info.kind), &info.uma),
                }
                ui.painter().text(pos2(img.center().x, img.bottom() + 9.0), Align2::CENTER_CENTER, caption, FontId::proportional(10.5), p.weak);
                if resp.on_hover_text(format!("{} {} [{}] ({}) · click for details", info.rarity, info.uma, info.title, info.kind)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.open_detail(crate::detail_modal::Detail::Card(*id), Some(caption.to_string()).filter(|c| !c.is_empty()));
                }
            }
        });
    }

    fn home_showcase(&self, ui: &mut egui::Ui, c: &HomeCache) {
        let p = pal(ui);
        let (Some(cm), Some(m)) = (&c.cm, &c.meta) else {
            ui.label(RichText::new("The community sheet has nothing for the next CM yet.").color(p.weak));
            return;
        };
        ui.label(RichText::new(format!("For the {} as a {}", cm.name, m.top.label())).size(size::SM).color(p.weak));
        if c.showcase.is_empty() {
            ui.label(RichText::new("None of the sheet's core cards for it are in your collection yet: see Missing pieces.").color(p.weak));
            return;
        }
        let ids: Vec<(u32, String)> = c.showcase.iter().take(8).map(|(id, _)| (*id, "★★★".to_string())).collect();
        self.art_tiles(ui, &ids);
    }

    fn home_wishlist(&self, ui: &mut egui::Ui, c: &HomeCache) {
        let p = pal(ui);
        let Some(cm) = &c.cm else { return };
        if c.meta.is_none() {
            ui.label(RichText::new("The community sheet has nothing for the next CM yet.").color(p.weak));
            return;
        }
        if c.wishlist.is_empty() {
            ui.label(RichText::new(format!("You own every card in the sheet's decks for the {}.", cm.name)).color(p.good));
            return;
        }
        let ids: Vec<(u32, String)> = c.wishlist.iter().take(8).map(|(id, tier)| (*id, tier.to_string())).collect();
        self.art_tiles(ui, &ids);
        if c.wishlist.len() > 8 {
            ui.label(RichText::new(format!("+{} more", c.wishlist.len() - 8)).size(size::SM).color(p.weak));
        }
    }

    fn home_tip(&mut self, ui: &mut egui::Ui, c: &HomeCache) {
        let p = pal(ui);
        if c.tips.is_empty() {
            return;
        }
        let day = crate::tab_planner::day_number(&today()).max(0) as usize;
        let (term, text, source) = &c.tips[(day.wrapping_mul(40_503) + self.home.tip_offset) % c.tips.len()];
        ui.horizontal(|ui| {
            // A fact from knowledge/mechanics.md has no term: it reads as the headline itself.
            if term.is_empty() {
                ui.label(RichText::new("💡").size(size::MD));
            } else {
                ui.label(semibold(term, size::MD));
            }
            if ui.small_btn("↻").on_hover_text("Another one").clicked() {
                self.home.tip_offset += 1;
            }
        });
        ui.add(egui::Label::new(if term.is_empty() { semibold(text, size::BODY) } else { RichText::new(text) }).wrap());
        ui.label(RichText::new(format!("Source: {source}")).size(size::XS).color(p.weak));
    }

    /// A new plan for a Champions Meeting, opened in the Planner.
    pub(crate) fn plan_for_cm(&mut self, cm: &EventPreset) {
        self.planner.plan = Plan::default();
        self.planner.open_file = None;
        let msg = match self.planner.plan.apply_cm(&self.kb, cm) {
            Some(note) => (format!("New plan for the {}; {note}.", cm.name), true),
            None => (format!("New plan for the {} ({}): course and conditions filled in. Next, pick a trainee.", cm.name, cm.date), false),
        };
        self.toast_flag(msg);
        self.go_career(CareerTab::Plan);
    }
}

/// What the CM card's buttons ask for.
enum CmAction {
    New,
    Open(std::path::PathBuf),
    Simulate(std::path::PathBuf),
    Trainee(u32),
}

/// Disabled skills grouped by name: ("Night Races", "◎ ○ ×").
pub(crate) fn disabled_groups(kb: &crate::knowledge::Knowledge, ids: &[u32]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for id in ids {
        let name = kb.skill_name(*id);
        let (base, mark) = match name.trim_end().char_indices().last() {
            Some((i, c)) if matches!(c, '◎' | '○' | '×') => (name[..i].trim().to_string(), c.to_string()),
            _ => (name.clone(), String::new()),
        };
        match out.iter_mut().find(|(b, _)| *b == base) {
            Some((_, marks)) => {
                if !mark.is_empty() {
                    marks.push(' ');
                    marks.push_str(&mark);
                }
            }
            None => out.push((base, mark)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    /// Every widget title draws on a tall screen.
    #[test]
    fn every_home_widget_draws() {
        let mut a = crate::App::for_tests();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, crate::theme::Mode::Dark);
        let mut texts: Vec<String> = Vec::new();
        for _ in 0..4 {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 4000.0))), ..Default::default() };
            let out = crate::ui_util::test_frame(&ctx, input, |ui| a.frame(ui));
            texts = out.shapes.iter().filter_map(|s| if let egui::Shape::Text(t) = &s.shape { Some(t.galley.text().to_string()) } else { None }).collect();
        }
        for title in ["Coming up", "Continue where you left off", "Your stable", "Support cards by type", "Who can run where", "Uma of the day", "Trainer card", "Missing pieces", "Did you know?"] {
            assert!(texts.iter().any(|t| t == title), "{title} missing");
        }
    }

    /// Scrolling doesn't change the layout: a card scrolled above the window once padded itself
    /// by its negative screen position, so Home couldn't scroll to its last rows.
    #[test]
    fn layout_does_not_depend_on_scroll() {
        let place = |offset: f32| {
            let mut a = crate::App::for_tests();
            a.debug_scroll = Some(offset);
            let ctx = egui::Context::default();
            crate::theme::install(&ctx, crate::theme::Mode::Dark);
            let mut y = None;
            for f in 0..4 {
                let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1600.0))), time: Some(f64::from(f) * 0.1), ..Default::default() };
                let out = crate::ui_util::test_frame(&ctx, input, |ui| a.frame(ui));
                y = out.shapes.iter().find_map(|s| if let egui::Shape::Text(t) = &s.shape { (t.galley.text() == "Your stable").then_some(t.pos.y + offset) } else { None });
            }
            y.expect("Your stable on screen")
        };
        let (top, scrolled) = (place(0.0), place(700.0));
        assert!((top - scrolled).abs() < 1.0, "Your stable moved from {top} to {scrolled} when scrolling");
    }
}
