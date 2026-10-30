//! Settings: one window with its sections listed on the left (Profile, Appearance, Data and
//! updates, AI assistant, About). Every control saves straight away to settings.json or the
//! account files; there is no Save button.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontFamily, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, vec2};

use crate::theme::{self, Look, pal, size};
use crate::ui_util::UiExt;
use crate::ui_util::{ToastKind, chibi, semibold, web_link};
use crate::{App, settings};

/// The sections, in the order the list shows them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum SettingsPage {
    #[default]
    Profile,
    Appearance,
    Data,
    Ai,
    About,
}

impl SettingsPage {
    pub const ALL: [SettingsPage; 5] = [SettingsPage::Profile, SettingsPage::Appearance, SettingsPage::Data, SettingsPage::Ai, SettingsPage::About];

    /// Icon, name, and one line on what the section holds.
    pub fn info(self) -> (&'static str, &'static str, &'static str) {
        match self {
            SettingsPage::Profile => ("👤", "Profile", "Whose account this is, and saving, loading or emptying it."),
            SettingsPage::Appearance => ("🎨", "Appearance", "Dark or light, the colour theme, Seiun Sky, and the size of everything."),
            SettingsPage::Data => ("🗂", "Data and updates", "Where the game data comes from, how old it is, and how to refresh it."),
            SettingsPage::Ai => ("💬", "AI assistant", "Who answers on the Ask tab: a free model through OpenCode, or Claude with your Claude account or an API key."),
            SettingsPage::About => ("ℹ", "About", "Version, credits, license and keyboard shortcuts."),
        }
    }

    /// The name `UMALORD_SETTINGS_PAGE` takes.
    pub fn key(self) -> &'static str {
        match self {
            SettingsPage::Profile => "profile",
            SettingsPage::Appearance => "appearance",
            SettingsPage::Data => "data",
            SettingsPage::Ai => "ai",
            SettingsPage::About => "about",
        }
    }

    pub fn from_key(key: &str) -> Option<SettingsPage> {
        SettingsPage::ALL.into_iter().find(|p| p.key() == key)
    }
}

#[derive(Default)]
pub(crate) struct SettingsState {
    pub open: bool,
    pub page: SettingsPage,
    /// The trainer name being edited (saved when the box loses focus or on Enter).
    name: String,
    name_loaded: bool,
}

/// A rounded rectangle as a polygon, for painting gradients with rounded corners.
fn rounded_poly(rect: Rect, r: f32) -> Vec<Pos2> {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let corners = [
        (rect.right_bottom() - vec2(r, r), 0.0),
        (rect.left_bottom() + vec2(r, -r), 0.5),
        (rect.left_top() + vec2(r, r), 1.0),
        (rect.right_top() + vec2(-r, r), 1.5),
    ];
    let mut pts = Vec::new();
    for (c, start) in corners {
        for i in 0..=6 {
            let a = std::f32::consts::PI * (start + 0.5 * i as f32 / 6.0);
            pts.push(c + vec2(a.cos() * r, a.sin() * r));
        }
    }
    pts
}

/// A painted thumbnail of the app in a theme: the page with its sky, the top bar with a selected
/// tab, a card with two lines of text, and a main button.
fn look_preview(painter: &egui::Painter, rect: Rect, look: Look, dark: bool) {
    let painter = &painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let p = theme::palette_for(look, dark);
    // The page, shading toward the theme's sky at the bottom right (Ink stays plain).
    let poly = rounded_poly(rect, 8.0);
    let mut mesh = egui::Mesh::default();
    let center = rect.center();
    let shade = |q: Pos2| {
        if look == Look::Ink {
            return p.bg;
        }
        let t = (((q.x - rect.left()) / rect.width() + (q.y - rect.top()) / rect.height()) / 2.0).clamp(0.0, 1.0);
        p.bg.lerp_to_gamma(p.sky, t * t)
    };
    mesh.colored_vertex(center, shade(center));
    for q in &poly {
        mesh.colored_vertex(*q, shade(*q));
    }
    let n = poly.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(egui::Shape::mesh(mesh));
    // Top bar with three tabs, the first selected.
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 18.0));
    painter.rect_filled(bar, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, p.surface);
    painter.circle_filled(bar.left_center() + vec2(10.0, 0.0), 4.0, p.accent);
    for i in 0..3 {
        let tab = Rect::from_min_size(bar.left_top() + vec2(22.0 + i as f32 * 30.0, 5.0), vec2(24.0, 8.0));
        painter.rect_filled(tab, CornerRadius::same(3), if i == 0 { p.accent.gamma_multiply(if dark { 0.45 } else { 0.3 }) } else { p.weak.gamma_multiply(0.35) });
    }
    // A card with a title line, a text line and a main button.
    let card = Rect::from_min_size(rect.left_top() + vec2(10.0, 28.0), vec2(rect.width() * 0.62, rect.height() - 40.0));
    painter.rect_filled(card, CornerRadius::same(6), p.surface);
    painter.rect_stroke(card, CornerRadius::same(6), Stroke::new(1.0, p.border), StrokeKind::Inside);
    painter.rect_filled(Rect::from_min_size(card.left_top() + vec2(8.0, 8.0), vec2(card.width() * 0.6, 6.0)), CornerRadius::same(3), p.text.gamma_multiply(0.85));
    painter.rect_filled(Rect::from_min_size(card.left_top() + vec2(8.0, 20.0), vec2(card.width() * 0.75, 4.0)), CornerRadius::same(2), p.weak.gamma_multiply(0.7));
    painter.rect_filled(Rect::from_min_size(card.left_top() + vec2(8.0, 28.0), vec2(card.width() * 0.5, 4.0)), CornerRadius::same(2), p.weak.gamma_multiply(0.7));
    let button = Rect::from_min_size(card.left_bottom() + vec2(8.0, -18.0), vec2(42.0, 12.0));
    painter.rect_filled(button, CornerRadius::same(4), p.accent);
    painter.rect_filled(button.shrink2(vec2(10.0, 4.5)), CornerRadius::same(1), p.on_accent);
    // A little of each theme's decoration.
    let corner = rect.right_bottom() - vec2(24.0, 22.0);
    match look {
        Look::Sky | Look::Sakura => {
            let cloud = Color32::from_white_alpha(if dark { 30 } else { 170 });
            for (dx, dy, r) in [(-8.0, 2.0, 8.0), (2.0, -2.0, 10.0), (12.0, 3.0, 7.0)] {
                painter.circle_filled(corner + vec2(dx, dy), r, cloud);
            }
            if look == Look::Sakura {
                for (dx, dy) in [(-40.0, -40.0), (-18.0, -56.0), (-60.0, -16.0)] {
                    painter.add(egui::Shape::ellipse_filled(corner + vec2(dx, dy), vec2(4.0, 2.5), p.pink.gamma_multiply(0.6)));
                }
            }
        }
        Look::Sunset => {
            painter.circle_filled(corner + vec2(-4.0, -6.0), 13.0, Color32::from_rgb(251, 146, 60).gamma_multiply(if dark { 0.45 } else { 0.7 }));
        }
        Look::Night => {
            painter.circle_filled(corner + vec2(-4.0, -12.0), 8.0, Color32::from_rgb(240, 236, 210).gamma_multiply(if dark { 0.7 } else { 0.95 }));
            for (dx, dy) in [(-40.0, -46.0), (-20.0, -30.0), (-56.0, -24.0), (-10.0, -56.0)] {
                painter.circle_filled(corner + vec2(dx, dy), 1.4, p.accent.gamma_multiply(0.8));
            }
        }
        Look::Track => {
            let rail = if dark { Color32::from_white_alpha(70) } else { Color32::from_rgb(146, 100, 0).gamma_multiply(0.5) };
            let c = rect.right_bottom() + vec2(0.0, 40.0);
            let arc: Vec<Pos2> = (0..=16).map(|i| {
                let a = std::f32::consts::PI * (1.0 + 0.5 * i as f32 / 16.0);
                c + vec2(a.cos() * 70.0, a.sin() * 50.0)
            }).collect();
            painter.add(egui::Shape::line(arc, Stroke::new(1.5, rail)));
        }
        Look::Ink => {}
    }
    painter.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, p.border), StrokeKind::Inside);
}

/// A row with a label in a fixed column on the left, and the control after it.
fn setting_row(ui: &mut egui::Ui, label: &str, help: &str, add: impl FnOnce(&mut egui::Ui)) {
    let p = pal(ui);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(150.0, 28.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(150.0);
            let r = ui.label(semibold(label, size::BODY));
            if !help.is_empty() {
                r.on_hover_text(help);
            }
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            add(ui);
            let _ = p;
        });
    });
    ui.add_space(theme::space(theme::gap::MD));
}

/// A section heading inside a page.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(theme::space(theme::gap::SM));
    ui.label(semibold(text, size::MD));
    ui.add_space(theme::space(theme::gap::XS));
}

/// Big buttons in one row where exactly one is on, for choices like the mode.
fn big_choice<T: PartialEq + Copy>(ui: &mut egui::Ui, value: T, options: &[(T, &str, &str)]) -> Option<T> {
    let p = pal(ui);
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for (v, label, hover) in options {
            let on = *v == value;
            let text = semibold(*label, size::BODY).color(if on { p.text } else { p.weak });
            let b = egui::Button::new(text)
                .fill(if on { p.accent.gamma_multiply(if ui.visuals().dark_mode { 0.28 } else { 0.16 }) } else { p.raised })
                .stroke(Stroke::new(if on { 1.5 } else { 1.0 }, if on { p.accent_strong } else { p.border }))
                .corner_radius(CornerRadius::same(10))
                .min_size(vec2(132.0, 40.0));
            let mut r = ui.add(b);
            if !hover.is_empty() {
                r = r.on_hover_text(*hover);
            }
            if r.clicked() && !on {
                picked = Some(*v);
            }
        }
    });
    picked
}

/// Where the knowledge commands run, and the Python that runs them: Windows has `python` (and
/// PowerShell); elsewhere it's `python3` in a terminal.
const SHELL: &str = if cfg!(windows) { "PowerShell" } else { "a terminal" };
const PYTHON: &str = if cfg!(windows) { "python" } else { "python3" };

/// A command with a Copy button, for things to run in a terminal.
fn command_line(ui: &mut egui::Ui, cmd: &str) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        egui::Frame::new().fill(p.input).stroke(Stroke::new(1.0, p.border)).corner_radius(CornerRadius::same(6)).inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.label(RichText::new(cmd).monospace().size(size::SM));
        });
        if ui.small_btn("📋 Copy").on_hover_text(format!("Copy the command, to paste in {SHELL} in your data folder")).clicked() {
            ui.ctx().copy_text(cmd.to_string());
        }
    });
}

impl App {
    /// Open Settings at a section.
    pub(crate) fn open_settings(&mut self, page: SettingsPage) {
        self.settings.open = true;
        self.settings.page = page;
        self.settings.name_loaded = false;
    }

    /// The Settings window, while it's open.
    pub(crate) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings.open {
            return;
        }
        if !self.settings.name_loaded {
            self.settings.name = self.roster.profile.clone().unwrap_or_default();
            self.settings.name_loaded = true;
        }
        let screen = ctx.content_rect();
        let size = vec2((screen.width() - 48.0).min(900.0), (screen.height() - 56.0).min(640.0));
        let p = theme::palette(self.dark);
        let frame = egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(CornerRadius::same(theme::RADIUS + 2)).inner_margin(Margin::ZERO);
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("settings_window")).frame(frame).show(ctx, |ui| {
            ui.set_min_size(size);
            ui.set_max_size(size);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // The section list.
                egui::Frame::new().fill(p.raised).corner_radius(CornerRadius { nw: theme::RADIUS + 2, sw: theme::RADIUS + 2, ne: 0, se: 0 }).inner_margin(Margin::same(12)).show(ui, |ui| {
                    ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.set_width(176.0);
                    ui.set_height(size.y - 24.0);
                    ui.label(semibold("⚙ Settings", size::LG));
                    ui.add_space(10.0);
                    for page in SettingsPage::ALL {
                        let (icon, label, what) = page.info();
                        let on = self.settings.page == page;
                        let (rect, resp) = ui.allocate_exact_size(vec2(176.0, 34.0), Sense::click());
                        let painter = ui.painter();
                        if on {
                            painter.rect_filled(rect, CornerRadius::same(8), p.accent.gamma_multiply(if self.dark { 0.26 } else { 0.15 }));
                            painter.rect_filled(Rect::from_min_size(rect.left_top() + vec2(0.0, 7.0), vec2(3.0, 20.0)), CornerRadius::same(2), p.accent);
                        } else if resp.hovered() {
                            painter.rect_filled(rect, CornerRadius::same(8), p.hover);
                        }
                        let font = if on { FontId::new(size::BODY, FontFamily::Name(theme::SEMIBOLD.into())) } else { FontId::proportional(size::BODY) };
                        painter.text(rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, format!("{icon}  {label}"), font, if on { p.text } else { p.weak });
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(what).clicked() {
                            self.settings.page = page;
                        }
                    }
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        ui.label(RichText::new(format!("umaLord {}", env!("CARGO_PKG_VERSION"))).size(size::XS).color(p.weak));
                        ui.label(RichText::new("Changes save as you make them.").size(size::XS).color(p.weak));
                    });
                    });
                });
                // The open section.
                egui::Frame::new().inner_margin(Margin { left: 22, right: 18, top: 16, bottom: 12 }).show(ui, |ui| {
                    ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.set_width(size.x - 200.0 - 40.0);
                    ui.set_height(size.y - 28.0);
                    let (icon, label, what) = self.settings.page.info();
                    ui.horizontal(|ui| {
                        ui.label(semibold(format!("{icon} {label}"), size::XL));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("✕").size(size::LG)).frame(false)).on_hover_text("Close (Esc)").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.label(RichText::new(what).size(13.0).color(p.weak));
                    ui.add_space(8.0);
                    ui.separator();
                    let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
                    // UMALORD_SCROLL holds it scrolled down, for screenshots of the lower part.
                    if let Some(y) = self.debug_scroll {
                        scroll = scroll.vertical_scroll_offset(y);
                    }
                    scroll.show(ui, |ui| {
                        ui.set_max_width(ui.available_width() - 14.0);
                        ui.add_space(6.0);
                        match self.settings.page {
                            SettingsPage::Profile => self.settings_profile(ui, &mut close),
                            SettingsPage::Appearance => self.settings_appearance(ui),
                            SettingsPage::Data => self.settings_data(ui, &mut close),
                            SettingsPage::Ai => self.settings_ai(ui),
                            SettingsPage::About => self.settings_about(ui),
                        }
                        ui.add_space(12.0);
                    });
                    });
                });
            });
        });
        if close || modal.should_close() {
            self.save_trainer_name();
            self.settings.open = false;
        }
    }

    /// Write the edited trainer name to account/umas.md, if it changed.
    fn save_trainer_name(&mut self) {
        let name = self.settings.name.trim().to_string();
        if name.is_empty() || Some(&name) == self.roster.profile.as_ref() {
            return;
        }
        let Some(root) = self.root.clone() else { return };
        let path = root.join(crate::data::UMAS_FILE);
        let result = std::fs::read_to_string(&path).and_then(|text| std::fs::write(&path, crate::data::set_meta(&text, "Profile", &name)));
        match result {
            Ok(()) => {
                self.reload_roster();
                self.write_digest();
                self.toast(format!("Renamed the account to {name}."), ToastKind::Success);
            }
            Err(e) => self.toast(format!("Couldn't save the name: {e}"), ToastKind::Error),
        }
    }

    fn settings_profile(&mut self, ui: &mut egui::Ui, close: &mut bool) {
        let p = pal(ui);
        setting_row(ui, "Trainer name", "Whose account this is. It's saved in account/umas.md and in profiles you save.", |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.settings.name).hint_text("e.g. Sam").desired_width(260.0).font(FontId::proportional(size::MD)));
            if r.lost_focus() {
                self.save_trainer_name();
            }
            ui.label(RichText::new("Saved when you press Enter or click elsewhere. Home greets you by it.").size(size::SM).color(p.weak));
        });
        setting_row(ui, "Server", "Uma Musume runs separate servers with different content. The game data in this app is for Global.", |ui| {
            ui.label(self.roster.server.clone().unwrap_or_else(|| "Global (English)".into()).replace("**", ""));
            ui.label(RichText::new("The game data, meta sheet and simulator are all for Global.").size(size::SM).color(p.weak));
        });
        setting_row(ui, "Your collection", "", |ui| {
            ui.label(format!(
                "{} umas · {} support cards{}",
                self.roster.umas.len(),
                self.roster.cards.len(),
                self.roster.cards_updated.as_ref().map(|d| format!(" · last updated {d}")).unwrap_or_default()
            ));
        });
        setting_row(ui, "Data folder", "Everything the app saves lives here: your account, plans, results and profiles.", |ui| {
            match &self.root {
                Some(root) => {
                    ui.label(RichText::new(root.display().to_string()).monospace().size(size::SM));
                    if ui.button("📂 Open folder").clicked() {
                        let _ = crate::ui_util::show_in_folder(root);
                    }
                }
                None => _ = ui.label(RichText::new("The umaLord folder wasn't found.").color(p.warn)),
            }
        });

        heading(ui, "Your data");
        let action = |ui: &mut egui::Ui, text: &str, about: &str| {
            ui.horizontal(|ui| {
                let b = ui.add(egui::Button::new(RichText::new(text).size(size::BODY)).min_size(vec2(210.0, 32.0)));
                ui.add(egui::Label::new(RichText::new(about).size(size::SM).color(p.weak)).wrap());
                b.clicked()
            })
            .inner
        };
        if action(ui, "📤 Save my data", "Your umas, cards, parents, plans and results in one .umalord.json file, to keep or share.") {
            match self.export_profile() {
                Ok(path) => self.toast_action(format!("Saved {}.", path.display()), ToastKind::Success, "Show", crate::ui_util::ToastAction::Open(path)),
                Err(e) => self.toast(e, ToastKind::Error),
            }
        }
        if action(ui, "📥 Load a profile", "Switch to a saved account. What's here is backed up first.") {
            self.open_setup(crate::onboarding::Step::Import);
            *close = true;
        }
        let finished = self.veterans().len();
        let about = format!(
            "Your finished umas (parents) with their sparks and lineage, from UmaExtractor's data.json: takes it from Downloads, or drop it on the window. {}",
            if finished == 0 { "None imported yet.".to_string() } else { format!("{finished} imported.") }
        );
        if action(ui, "🌱 Import finished umas", &about) {
            match crate::settings::user_dir("Downloads").map(|d| d.join("data.json")).filter(|p| p.is_file()) {
                Some(path) => {
                    let r = self.import_veterans(&path);
                    self.toast_flag(r.map_or_else(|e| (e, true), |m| (m, false)));
                }
                None => self.toast("No data.json in Downloads. Run UmaExtractor (or tools/read_veterans.py) on the game's Veteran List, then drop its data.json on the window.", ToastKind::Info),
            }
        }
        if action(ui, "📝 Set up my collection", "Click the umas and cards you own, and set stars and limit breaks.") {
            self.open_setup(crate::onboarding::Step::Umas);
            *close = true;
        }

        heading(ui, "Danger zone");
        egui::Frame::new().stroke(Stroke::new(1.0, p.bad.gamma_multiply(0.7))).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let b = egui::Button::new(RichText::new("⚠ Start over…").color(Color32::WHITE)).fill(p.bad).min_size(vec2(0.0, 30.0));
                if ui.add(b).clicked() {
                    self.onboarding.confirm_start_over = true;
                    *close = true;
                }
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.label(semibold("Start over", size::BODY));
                    ui.add(
                        egui::Label::new(RichText::new("Back this account up to profiles/backups, then empty the app for a new one. You'll be asked to confirm.").size(size::SM).color(p.weak))
                            .wrap(),
                    );
                });
            });
        });
    }

    /// System, Light or Dark, as big buttons. Shared with setup's "Make it yours".
    pub(crate) fn mode_picker(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let options = [
            (theme::Mode::System, theme::Mode::System.label(), "Follow the system: dark when your computer is dark"),
            (theme::Mode::Light, theme::Mode::Light.label(), ""),
            (theme::Mode::Dark, theme::Mode::Dark.label(), ""),
        ];
        if let Some(m) = big_choice(ui, self.mode, &options) {
            self.set_mode(&ctx, m);
        }
    }

    /// The colour themes as cards with a painted preview each; a click applies one.
    pub(crate) fn theme_picker(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let ctx = ui.ctx().clone();
        let w = ui.available_width();
        let cols = if w >= 600.0 { 3 } else { 2 };
        let gap = 10.0;
        let tile = vec2((w - gap * (cols - 1) as f32) / cols as f32, 156.0);
        for row in Look::ALL.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for look in row {
                    let on = self.look == *look;
                    let (rect, resp) = ui.allocate_exact_size(tile, Sense::click());
                    let painter = ui.painter();
                    if resp.hovered() && !on {
                        painter.rect_filled(rect.expand(2.0), CornerRadius::same(10), p.hover);
                    }
                    let thumb = Rect::from_min_size(rect.min, vec2(rect.width(), 100.0));
                    look_preview(painter, thumb, *look, self.dark);
                    if on {
                        painter.rect_stroke(thumb.expand(1.5), CornerRadius::same(9), Stroke::new(2.5, p.accent_strong), StrokeKind::Outside);
                        let tick = thumb.right_top() + vec2(-14.0, 14.0);
                        painter.circle_filled(tick, 9.0, p.accent);
                        painter.text(tick, Align2::CENTER_CENTER, "✔", FontId::proportional(size::SM), p.on_accent);
                    }
                    painter.text(thumb.left_bottom() + vec2(2.0, 8.0), Align2::LEFT_TOP, look.label(), FontId::new(size::BODY, FontFamily::Name(theme::SEMIBOLD.into())), p.text);
                    let about = painter.layout(look.about().to_string(), FontId::proportional(size::XS), p.weak, rect.width() - 4.0);
                    painter.galley(thumb.left_bottom() + vec2(2.0, 27.0), about, p.weak);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on {
                        self.look = *look;
                        theme::set_look(&ctx, *look);
                        settings::set("look", serde_json::json!(look.key()));
                    }
                }
            });
            ui.add_space(gap);
        }
    }

    /// The sky-and-decorations switch.
    pub(crate) fn background_toggle(&mut self, ui: &mut egui::Ui) {
        if ui.tick(&mut self.mascot, "Sky and decorations behind the pages").changed() {
            settings::set("mascot", serde_json::json!(self.mascot));
        }
    }

    /// Where chibi Seiun Sky goes.
    pub(crate) fn chibi_picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            chibi(ui, 0, 64.0);
            ui.vertical(|ui| {
                let options = [
                    (theme::Chibi::Everywhere, "Everywhere", "On Home, on empty pages, and small beside every page title"),
                    (theme::Chibi::Home, "Home and empty pages", "Beside Home's welcome, and on pages with nothing to show yet"),
                    (theme::Chibi::Off, "Off", "Nowhere"),
                ];
                if let Some(c) = big_choice(ui, theme::chibi(), &options) {
                    theme::use_chibi(c);
                    settings::set("chibi", serde_json::json!(c.key()));
                }
            });
        });
    }

    fn settings_appearance(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let ctx = ui.ctx().clone();
        setting_row(ui, "Mode", "", |ui| self.mode_picker(ui));
        setting_row(ui, "Theme", "The accent colour of buttons and highlights, and the background. Click one to try it.", |ui| self.theme_picker(ui));
        setting_row(ui, "Background", "", |ui| {
            self.background_toggle(ui);
            ui.label(RichText::new("The theme's gradient, clouds, petals, stars or rails. Ink has none.").size(size::SM).color(p.weak));
        });
        setting_row(ui, "Seiun Sky", "Chibi Seiun Sky, the app's mascot. She's only drawn where nothing else is, so she never covers your data.", |ui| {
            self.chibi_picker(ui)
        });
        setting_row(ui, "Size", "Makes everything bigger or smaller, text and all. Ctrl+plus and Ctrl+minus do the same for this session.", |ui| {
            ui.horizontal(|ui| {
                let mut scale = self.ui_scale;
                ui.spacing_mut().slider_width = 240.0;
                let slider = egui::Slider::new(&mut scale, theme::UI_SCALE).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0));
                let r = ui.add(slider);
                // Apply when the drag ends, so the slider doesn't move under the mouse.
                if r.changed() {
                    self.ui_scale = scale;
                }
                if (r.drag_stopped() || (r.changed() && !r.dragged())) && (ctx.zoom_factor() - self.ui_scale).abs() > 0.001 {
                    ctx.set_zoom_factor(self.ui_scale);
                    settings::set("ui_scale", serde_json::json!(self.ui_scale));
                }
                if (self.ui_scale - 1.0).abs() > 0.001 && ui.button("Reset to 100%").clicked() {
                    self.ui_scale = 1.0;
                    ctx.set_zoom_factor(1.0);
                    settings::set("ui_scale", serde_json::json!(1.0));
                }
            });
        });
        setting_row(ui, "Density", "How much space there is between things.", |ui| {
            let options = [(false, "Comfortable", "Roomier, easier to read"), (true, "Compact", "Tighter spacing, more on screen")];
            if let Some(c) = big_choice(ui, self.compact, &options) {
                self.compact = c;
                theme::set_compact(&ctx, c);
                settings::set("density", serde_json::json!(if c { "compact" } else { "comfortable" }));
            }
        });
    }

    fn settings_data(&mut self, ui: &mut egui::Ui, close: &mut bool) {
        let p = pal(ui);
        let day = |s: &Option<String>| s.as_deref().map(|b| b[..10.min(b.len())].to_string()).unwrap_or_else(|| "unknown".into());
        heading(ui, "Champions Meeting schedule");
        ui.label(
            RichText::new(
                "Official dates, race conditions and the skills that won't activate come from the game's news on umamusume.com. \
                 Umalator (umalator.app) gives the exact course and estimates the events not announced yet.",
            )
            .size(size::SM)
            .color(p.weak),
        );
        ui.add_space(4.0);
        let mut on = crate::live::enabled();
        if ui.tick(&mut on, "Fetch the latest schedule when the app starts (at most every 6 hours)").changed() {
            crate::live::set_enabled(on);
        }
        let now = crate::live::now();
        let source_line = |ui: &mut egui::Ui, name: &str, s: Option<&crate::live::SourceStatus>| {
            ui.horizontal_wrapped(|ui| {
                ui.label(semibold(name, size::BODY));
                match s {
                    Some(s) if s.ok => {
                        ui.label(RichText::new(format!("✔ {}", s.message)).color(p.good));
                    }
                    Some(s) => {
                        ui.label(RichText::new(format!("✖ {}", s.message)).color(p.warn));
                    }
                    None => {
                        ui.label(RichText::new("not fetched yet").color(p.weak));
                    }
                }
                if let Some(t) = s.and_then(|s| s.fetched_at) {
                    ui.label(RichText::new(format!("· fetched {}", crate::live::fmt_ago(now - t))).color(p.weak));
                }
            });
        };
        let data = self.live.data.clone();
        source_line(ui, "umamusume.com", data.as_ref().map(|d| &d.news));
        source_line(ui, "umalator.app", data.as_ref().map(|d| &d.umalator));
        source_line(ui, "uma.guide (affinity, weekly)", data.as_ref().map(|d| &d.uma_guide).filter(|s| !s.message.is_empty()));
        ui.horizontal(|ui| {
            if self.live.rx.is_some() {
                ui.spinner();
                ui.label(RichText::new("Checking…").color(p.weak));
            } else if ui.add(egui::Button::new("🔄 Refresh now").min_size(vec2(0.0, 30.0))).on_hover_text("Fetch the news, Umalator's presets and the affinity tables again").clicked() {
                self.refresh_live(true);
            }
            if let Some(d) = &data {
                ui.label(RichText::new(format!("Last checked {}.", crate::live::fmt_ago(now - d.checked_at))).size(size::SM).color(p.weak));
            }
        });
        ui.add_space(6.0);
        heading(ui, "Game data");
        setting_row(ui, "uma.guide", "Every uma, support card, skill, race and event on the Global server.", |ui| {
            ui.label(format!(
                "Built {} · {} umas · {} support cards · {} skills · {} races",
                day(&self.kb.built),
                self.kb.umas.len(),
                self.kb.cards.len(),
                self.kb.skills.len(),
                self.kb.races.len()
            ));
            if let Some(e) = &self.kb.error {
                ui.label(RichText::new(format!("⚠ {e}")).color(p.warn));
            }
        });
        setting_row(ui, "Weekly pack", "New umas, cards and their art, refreshed from uma.guide and tested each week, then downloaded here (only what changed).", |ui| {
            ui.horizontal(|ui| {
                if self.pack.is_some() {
                    ui.spinner();
                    ui.label(RichText::new("Checking…").color(p.weak));
                } else if ui.button("Check now").clicked() {
                    let ctx = ui.ctx().clone();
                    self.pack_check(&ctx, true);
                }
                let checked = crate::settings::get("data_pack_checked").and_then(|v| v.as_str().map(String::from));
                ui.label(RichText::new(self.pack_status.clone().unwrap_or_else(|| checked.map_or("Not checked yet.".into(), |d| format!("Last checked {d}.")))).size(size::SM).color(p.weak));
            });
        });
        setting_row(ui, "Umalator", "The race simulator's skills and courses, and its Champions Meeting presets (Moomoolator, umalator.app).", |ui| {
            ui.label(format!("Built {} · {} courses · {} event presets", day(&self.kb.umalator_built), self.kb.umalator_courses.len(), self.kb.umalator_presets.len()));
        });
        setting_row(ui, "Meta sheet", "", |ui| match &self.kb.pvp_meta {
            Some(m) => {
                ui.label(format!("Read {} · community opinion, not game data", m.fetched));
                ui.label(RichText::new(&m.authors).size(size::SM).color(p.weak));
            }
            None => _ = ui.label(RichText::new("Not downloaded (knowledge/data/pvp_meta.json is missing).").color(p.warn)),
        });
        setting_row(ui, "Skill tier list", "The community skills spreadsheet's Team Trials and Champions Meeting tiers, shown on skills and used by the Rating auto-pick.", |ui| match &self.kb.skill_tiers {
            Some(t) => {
                ui.label(format!("Sheet updated {} · {} skills · community opinion, not game data", t.sheet_updated, t.len())).on_hover_text(format!("{}\n\n{}", t.caveat, t.source));
                ui.label(RichText::new(&t.authors).size(size::SM).color(p.weak));
            }
            None => _ = ui.label(RichText::new("Not downloaded (knowledge/data/skill_tiers.json is missing).").color(p.warn)),
        });
        heading(ui, "Refreshing it");
        let folder = self.root.as_ref().map_or_else(|| "the umaLord folder".to_string(), |r| r.display().to_string());
        ui.label(
            RichText::new(format!("Install Python 3, then run one of these in {SHELL} from your data folder ({folder}), and press 🔄 Reload (F5). The meta sheet changes most often."))
                .size(size::SM)
                .color(p.weak),
        );
        ui.add_space(4.0);
        command_line(ui, &format!("{PYTHON} -I -X utf8 tools/update_knowledge.py"));
        command_line(ui, &format!("{PYTHON} -I -X utf8 tools/update_knowledge.py --only meta"));
        heading(ui, "Advanced");
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new("📝 Browse notes and data files").min_size(vec2(0.0, 30.0))).on_hover_text("Read the markdown files in this folder: insights, plans, results, your roster files and the knowledge notes").clicked() {
                self.open_notes();
                *close = true;
            }
            if let Some(root) = &self.root
                && ui.add(egui::Button::new("📂 Open the game data folder").min_size(vec2(0.0, 30.0))).clicked()
            {
                let _ = crate::ui_util::show_in_folder(&root.join("knowledge").join("data"));
            }
        });
    }

    fn settings_ai(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let busy = self.ask_busy();
        setting_row(ui, "Answered by", "Who answers your questions on the Ask tab.", |ui| {
            let options = [
                (crate::ask::Backend::OpenCode, "🆓 OpenCode · free models", "Free models through OpenCode. No account or key; weaker than Claude."),
                (crate::ask::Backend::ClaudeApi, "✳ Claude · your account or key", "Claude, through Claude Code with your Claude account or a setup token, or with an Anthropic API key billed per use."),
            ];
            ui.add_enabled_ui(!busy, |ui| {
                if let Some(b) = big_choice(ui, self.ask.backend, &options)
                    && b != self.ask.backend
                {
                    // Each backend keeps its own sessions; the next question starts a fresh one.
                    self.ask.end_process();
                    self.ask.backend = b;
                    settings::set("ask_backend", serde_json::json!(if b == crate::ask::Backend::OpenCode { "opencode" } else { "claude_api" }));
                }
            });
            if busy {
                ui.label(RichText::new("A question is being answered; switch when it's done.").size(size::SM).color(p.weak));
            }
        });
        ui.separator();
        ui.add_space(6.0);
        ui.set_max_width(ui.available_width().min(560.0));
        if self.ask.backend == crate::ask::Backend::OpenCode && self.ask.oc.exe.is_none() {
            self.opencode_install_ui(ui);
        } else if self.ask.backend == crate::ask::Backend::OpenCode {
            App::opencode_settings(ui, &mut self.ask);
        } else {
            App::claude_api_settings(ui, &mut self.ask);
        }
        ui.add_space(8.0);
        App::agent_permissions(ui, &mut self.ask);
    }

    fn settings_about(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        ui.horizontal(|ui| {
            chibi(ui, 3, 84.0);
            ui.vertical(|ui| {
                ui.add_space(10.0);
                ui.label(semibold(format!("umaLord {}", env!("CARGO_PKG_VERSION")), size::XL));
                ui.label(RichText::new("Your Uma Musume: Pretty Derby (Global) companion. Plan careers on the community meta, race them in a real simulator, and keep your whole collection at hand.").color(p.weak));
            });
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            web_link(ui, "GitHub", "https://github.com/brickc4/umaLord");
            ui.label("·");
            web_link(ui, "Releases and downloads", "https://github.com/brickc4/umaLord/releases");
            ui.label("·");
            web_link(ui, "Report a bug or suggest an idea", "https://github.com/brickc4/umaLord/issues/new/choose");
        });
        heading(ui, "Credits");
        for (what, who, link) in [
            ("Game data", "uma.guide: umas, cards, skills, races and events", "https://uma.guide"),
            ("Race simulator", "a port of Moomoolator (TheCing/uma-tools), which builds on alpha123's Umalator and kachi-dev's VFalator", "https://umalator.app"),
            ("Meta picks", "the community \"PvP Deck & Uma Recommendation\" sheet by Lightning, Daptrius and Henry", ""),
            ("Skill tiers", "the community \"Uma Musume Skills Spreadsheet\" tier list by Dylan, formatted by sayaduck", ""),
            ("Glossary and tips", "GameTora, uma.guide and other community guides, each cited in knowledge/mechanics.md", ""),
            ("Chibi art", "GameTora", "https://gametora.com"),
        ] {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(semibold(format!("{what}:"), size::BODY));
                ui.label(RichText::new(who).color(p.weak));
                if !link.is_empty() {
                    web_link(ui, link.trim_start_matches("https://"), link);
                }
            });
        }
        heading(ui, "License");
        ui.label(RichText::new("GPL-3.0-or-later: the race simulator ports Moomoolator, which is GPL-3.0-or-later. The source is on GitHub and in the app folder.").color(p.weak));
        ui.label(RichText::new("Uma Musume: Pretty Derby is © Cygames, Inc. umaLord is an unofficial fan project, not affiliated with or endorsed by Cygames.").size(size::SM).color(p.weak));
        heading(ui, "Keyboard shortcuts");
        egui::Grid::new("about_shortcuts").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
            for (keys, what) in crate::SHORTCUTS {
                ui.label(RichText::new(*keys).monospace());
                ui.label(*what);
                ui.end_row();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_keys_round_trip() {
        for p in SettingsPage::ALL {
            assert_eq!(SettingsPage::from_key(p.key()), Some(p));
        }
    }

    /// Every section draws, in both modes, at the smallest window size.
    #[test]
    fn every_section_draws() {
        let mut a = App::for_tests();
        for mode in [theme::Mode::Dark, theme::Mode::Light] {
            let ctx = egui::Context::default();
            theme::install(&ctx, mode);
            for page in SettingsPage::ALL {
                a.open_settings(page);
                for _ in 0..2 {
                    let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, 600.0))), ..Default::default() };
                    let _ = crate::ui_util::test_frame(&ctx, input, |ui| a.frame(ui));
                }
                assert!(a.settings.open, "{page:?} closed itself");
            }
        }
    }
}
