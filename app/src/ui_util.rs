//! Drawing helpers and small components shared by the tabs.

use std::collections::HashSet;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Margin, Painter, Pos2, Rect, Response, RichText, Sense, Shape, Stroke,
    StrokeKind, Vec2, vec2,
};

use crate::data::CardType;
use crate::knowledge::grade_rank;
use crate::theme::{SEMIBOLD, pal, size};

/// Star fill. For gold *text*, use `pal(ui).gold`, which is darker in light mode.
pub const GOLD: Color32 = Color32::from_rgb(250, 204, 21);
pub const DIM: Color32 = Color32::from_gray(85);
pub const LB_ON: Color32 = Color32::from_rgb(56, 189, 248);

/// A card type's colour, for fills (bars, swatches, placeholders). For text use `type_text`.
pub fn type_color(t: CardType) -> Color32 {
    type_color_str(t.label())
}

/// A card type's colour for text: brighter on dark, darker on light, so it reads in both.
pub fn type_text(ui: &egui::Ui, t: &str) -> Color32 {
    let c = Color32::from_rgb;
    if ui.visuals().dark_mode {
        match t {
            "Speed" => c(96, 165, 250),
            "Stamina" => c(248, 113, 113),
            "Guts" => c(244, 114, 182),
            _ => type_color_str(t),
        }
    } else {
        match t {
            "Speed" => c(29, 78, 216),
            "Stamina" => c(185, 28, 28),
            "Power" => c(180, 83, 9),
            "Guts" => c(190, 24, 93),
            "Wit" => c(21, 128, 61),
            "Pal" => c(146, 100, 0),
            "Group" => c(15, 118, 110),
            _ => c(93, 101, 114),
        }
    }
}

pub fn type_color_str(t: &str) -> Color32 {
    match t {
        "Speed" => Color32::from_rgb(59, 130, 246),
        "Stamina" => Color32::from_rgb(239, 68, 68),
        "Power" => Color32::from_rgb(245, 158, 11),
        "Guts" => Color32::from_rgb(236, 72, 153),
        "Wit" => Color32::from_rgb(34, 197, 94),
        "Pal" => Color32::from_rgb(234, 179, 8),
        "Group" => Color32::from_rgb(20, 184, 166),
        _ => Color32::from_gray(160),
    }
}

pub fn type_icon(t: &str) -> &'static str {
    match t {
        "Speed" => "👟",
        "Stamina" => "♥",
        "Power" => "💪",
        "Guts" => "🔥",
        "Wit" => "🎓",
        "Pal" => "😊",
        "Group" => "👥",
        _ => "•",
    }
}

pub fn grade_color(g: &str) -> Color32 {
    match grade_rank(g) {
        8 => Color32::from_rgb(250, 204, 21),
        7 => Color32::from_rgb(251, 146, 60),
        6 => Color32::from_rgb(244, 114, 182),
        5 => Color32::from_rgb(74, 222, 128),
        4 => Color32::from_rgb(56, 189, 248),
        3 => Color32::from_rgb(167, 139, 250),
        2 => Color32::from_rgb(148, 163, 184),
        _ => Color32::from_rgb(100, 116, 139),
    }
}

/// Text in the semibold heading face.
pub fn semibold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).family(FontFamily::Name(SEMIBOLD.into())).size(size)
}

/// The title block every page starts with: the title, actions on the right, and one plain line
/// on what the page is for. Same size and spacing on every tab, so pages line up.
pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str, actions: impl FnOnce(&mut egui::Ui)) {
    let weak = pal(ui).weak;
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        ui.label(semibold(title, size::XL));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            header_chibi(ui);
            actions(ui);
        });
    });
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).size(13.0).color(weak));
    }
    ui.add_space(10.0);
}

/// A rounded rectangle shading from `from` (top left) to `to` (bottom right).
pub fn gradient_rect(painter: &Painter, rect: Rect, radius: f32, from: Color32, to: Color32) {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let corners = [(rect.right_bottom() - vec2(r, r), 0.0), (rect.left_bottom() + vec2(r, -r), 0.5), (rect.left_top() + vec2(r, r), 1.0), (rect.right_top() + vec2(-r, r), 1.5)];
    let shade = |q: Pos2| {
        let t = (((q.x - rect.left()) / rect.width() + (q.y - rect.top()) / rect.height()) / 2.0).clamp(0.0, 1.0);
        from.lerp_to_gamma(to, t)
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), shade(rect.center()));
    let mut n = 0u32;
    for (c, start) in corners {
        for i in 0..=6 {
            let a = std::f32::consts::PI * (start + 0.5 * i as f32 / 6.0);
            let q = c + vec2(a.cos() * r, a.sin() * r);
            mesh.colored_vertex(q, shade(q));
            n += 1;
        }
    }
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
}

/// The six chibi Seiun Sky poses, embedded.
pub fn chibi_image(pose: u8) -> egui::ImageSource<'static> {
    match pose {
        0 => egui::include_image!("../assets/chibi/seiun_102001_0011.png"),
        1 => egui::include_image!("../assets/chibi/seiun_102001_0010.png"),
        2 => egui::include_image!("../assets/chibi/seiun_102002_0010.png"),
        3 => egui::include_image!("../assets/chibi/seiun_102003_0011.png"),
        4 => egui::include_image!("../assets/chibi/seiun_102002_0011.png"),
        _ => egui::include_image!("../assets/chibi/seiun_102003_0010.png"),
    }
}

static POSE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The pose for the page being drawn; `App::frame` sets it per tab.
pub fn set_pose(pose: u8) {
    POSE.store(pose, std::sync::atomic::Ordering::Relaxed);
}

pub fn pose() -> u8 {
    POSE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Chibi Seiun Sky laid out at `size` (she takes her own space, so she never covers anything).
pub fn chibi(ui: &mut egui::Ui, pose: u8, size: f32) -> Response {
    ui.add(egui::Image::new(chibi_image(pose)).fit_to_exact_size(vec2(size, size)))
}

/// Small chibi at the right of a page header, when the mascot is set to show everywhere.
/// Call it first inside the header's right-to-left section.
pub fn header_chibi(ui: &mut egui::Ui) {
    if crate::theme::chibi() == crate::theme::Chibi::Everywhere {
        chibi(ui, pose(), 44.0).on_hover_text("Seiun Sky. ⚙ Settings moves or hides her.");
    }
}

/// A row of mutually exclusive choices in one rounded track, like sub-tabs.
pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, String, &str)]) {
    let p = pal(ui);
    egui::Frame::new().fill(p.input).stroke(Stroke::new(1.0, p.border)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(3)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            // In a right-to-left row the first one added sits rightmost: add them backwards.
            let ordered: Vec<&(T, String, &str)> = if ui.layout().prefer_right_to_left() { options.iter().rev().collect() } else { options.iter().collect() };
            for (v, label, hover) in ordered {
                let on = *value == *v;
                let text = RichText::new(label).size(14.0).color(if on { p.text } else { p.weak });
                let b = egui::Button::new(text)
                    .fill(if on { p.raised } else { Color32::TRANSPARENT })
                    .stroke(if on { Stroke::new(1.0, p.accent.gamma_multiply(0.7)) } else { Stroke::NONE })
                    .corner_radius(CornerRadius::same(8))
                    .min_size(vec2(0.0, 28.0));
                let mut r = ui.add(b);
                if !hover.is_empty() {
                    r = r.on_hover_text(*hover);
                }
                if r.clicked() {
                    *value = *v;
                }
            }
        });
    });
}

/// A small uppercase heading for a group of controls, with an ⓘ that explains it on hover.
pub fn section_label(ui: &mut egui::Ui, text: &str, help: &str) {
    let weak = pal(ui).weak;
    ui.add_space(12.0);
    let r = ui.label(RichText::new(if help.is_empty() { text.to_uppercase() } else { format!("{}  ⓘ", text.to_uppercase()) }).size(11.5).color(weak));
    if !help.is_empty() {
        r.on_hover_text(help);
    }
    ui.add_space(2.0);
}

/// Lay `n` equal tiles out in rows that fill the width exactly: the gaps stretch so the left
/// and right edges of every row line up. `add` draws tile `i`.
pub fn tile_grid(ui: &mut egui::Ui, n: usize, tile: Vec2, min_gap: f32, row_gap: f32, mut add: impl FnMut(&mut egui::Ui, usize)) {
    if n == 0 {
        return;
    }
    let w = ui.available_width();
    let cols = (((w + min_gap) / (tile.x + min_gap)).floor() as usize).max(1);
    let gap = if cols > 1 { ((w - cols as f32 * tile.x) / (cols - 1) as f32).clamp(min_gap, min_gap * 2.5) } else { 0.0 };
    for row in 0..n.div_ceil(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for i in row * cols..((row + 1) * cols).min(n) {
                add(ui, i);
            }
        });
        ui.add_space(row_gap);
    }
}

/// The main action on a page: filled with the accent colour, with text that reads on it.
/// `small_btn`: egui's small button, but at least 24×24 px, so it's easy to hit.
/// `tick`: a checkbox whose empty box has an outline in dark mode too, where buttons have none.
pub trait UiExt {
    fn small_btn(&mut self, text: impl Into<egui::WidgetText>) -> egui::Response;
    fn tick(&mut self, checked: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response;
}

impl UiExt for egui::Ui {
    fn small_btn(&mut self, text: impl Into<egui::WidgetText>) -> egui::Response {
        self.add(egui::Button::new(text).small().min_size(egui::vec2(24.0, 24.0)))
    }

    fn tick(&mut self, checked: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
        let outline = egui::Stroke::new(1.0, pal(self).weak.gamma_multiply(0.7));
        let saved = self.visuals().widgets.inactive.bg_stroke;
        self.visuals_mut().widgets.inactive.bg_stroke = outline;
        let r = self.checkbox(checked, text);
        self.visuals_mut().widgets.inactive.bg_stroke = saved;
        r
    }
}

pub fn primary_button(ui: &egui::Ui, text: impl Into<String>, size: f32) -> egui::Button<'static> {
    let p = pal(ui);
    egui::Button::new(semibold(text, size).color(p.on_accent)).fill(p.accent)
}

/// A rounded surface that groups related content, full width of the parent.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A card with a title line.
pub fn card_titled<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_explained(ui, title, "", add)
}

/// A card with a title line and a short plain-language line under it.
pub fn card_explained<R>(ui: &mut egui::Ui, title: &str, explain: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let r = card(ui, |ui| {
        ui.label(semibold(title, 15.5));
        if !explain.is_empty() {
            ui.label(RichText::new(explain).size(12.5).color(pal(ui).weak));
        }
        ui.add_space(4.0);
        add(ui)
    });
    ui.add_space(10.0);
    r
}

/// Columns whose last cards end at the same height. `add` draws column `i`, stretching its last
/// card down to the given y with `card_titled_to`, and returns where it would end on its own.
/// The tallest column from the last frame sets the line.
pub fn equal_columns(ui: &mut egui::Ui, id: &str, n: usize, mut add: impl FnMut(&mut egui::Ui, usize, f32) -> f32) {
    let key = ui.make_persistent_id(id);
    let height: f32 = ui.data(|d| d.get_temp(key)).unwrap_or(0.0);
    let top = ui.cursor().top();
    let mut tallest = 0.0f32;
    ui.columns(n, |cols| {
        for (i, col) in cols.iter_mut().enumerate() {
            tallest = tallest.max(add(col, i, top + height) - top);
        }
    });
    if (tallest - height).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(key, tallest));
        ui.ctx().request_repaint();
    }
}

/// A titled card whose frame reaches down to `bottom` (a y on screen) when its content is
/// shorter. Returns the y where it would end on its own, for `equal_columns`.
pub fn card_titled_to(ui: &mut egui::Ui, title: &str, bottom: f32, add: impl FnOnce(&mut egui::Ui)) -> f32 {
    let p = pal(ui);
    let mut natural = 0.0;
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semibold(title, 15.5));
            ui.add_space(4.0);
            add(ui);
            // Inner margin and stroke below the content.
            natural = ui.min_rect().bottom() + 15.0;
            // Pad the content down so the frame ends at `bottom`.
            let extra = bottom - natural - ui.spacing().item_spacing.y;
            if extra > 0.5 {
                ui.allocate_space(vec2(0.0, extra));
            }
        });
    ui.add_space(10.0);
    natural
}

/// Small tinted pill with coloured text.
/// Drawn as one widget, so it keeps its size in any layout (a table cell's included).
pub fn badge(ui: &mut egui::Ui, text: impl Into<String>, color: Color32) -> Response {
    let text = RichText::new(text.into()).color(readable(ui, color)).size(size::SM);
    pill_padded(ui, text, color.gamma_multiply(0.16), Stroke::new(1.0, color.gamma_multiply(0.45)), vec2(16.0, 4.0))
}

/// `color` as text: unchanged in dark mode; in light mode darkened until it reads on a card
/// (WCAG AA, 4.5:1), so bright accent colours still work as labels.
pub fn readable(ui: &egui::Ui, color: Color32) -> Color32 {
    let p = pal(ui);
    if ui.visuals().dark_mode {
        return color;
    }
    let mut c = color;
    for _ in 0..16 {
        if crate::theme::contrast(c, p.surface) >= 4.5 && crate::theme::contrast(c, p.bg) >= 4.5 {
            break;
        }
        c = Color32::from_rgb((f32::from(c.r()) * 0.88) as u8, (f32::from(c.g()) * 0.88) as u8, (f32::from(c.b()) * 0.88) as u8);
    }
    c
}

/// A rounded pill with text, as one widget, so it wraps whole in a wrapping row (a Frame with a
/// label inside can't, since its size is only known after it's placed).
pub fn pill(ui: &mut egui::Ui, text: RichText, fill: Color32, stroke: Stroke) -> Response {
    pill_padded(ui, text, fill, stroke, vec2(16.0, 6.0))
}

pub fn pill_padded(ui: &mut egui::Ui, text: RichText, fill: Color32, stroke: Stroke, pad: Vec2) -> Response {
    let galley = egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
    let size = galley.size() + pad;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(rect, CornerRadius::same(10), fill, stroke, StrokeKind::Inside);
        ui.painter().galley(rect.center() - galley.size() / 2.0, galley, pal(ui).text);
    }
    resp
}

/// A small label over a coloured letter tile, like the game's aptitude grades.
pub fn grade_badge(ui: &mut egui::Ui, label: &str, grade: &str) {
    let weak = pal(ui).weak;
    ui.vertical(|ui| {
        ui.set_width(52.0);
        ui.label(RichText::new(label).size(11.5).color(weak));
        let (rect, _) = ui.allocate_exact_size(vec2(30.0, 24.0), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(6), grade_color(grade));
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            grade,
            FontId::new(15.0, FontFamily::Name(SEMIBOLD.into())),
            Color32::from_rgb(20, 22, 28),
        );
    });
}

/// A grade as a small coloured tile, the same as the detail panel's, for table cells.
pub fn grade_tile(ui: &mut egui::Ui, grade: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 22.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        if grade.is_empty() || grade == "-" {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "—", FontId::proportional(size::SM), pal(ui).weak);
        } else {
            ui.painter().rect_filled(rect, CornerRadius::same(5), grade_color(grade));
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, grade, FontId::new(size::BODY, FontFamily::Name(SEMIBOLD.into())), Color32::from_rgb(20, 22, 28));
        }
    }
    resp
}

/// Colour for a skill tier, readable in both themes.
pub fn tier_color(ui: &egui::Ui, tier: &str) -> Color32 {
    let p = pal(ui);
    match tier {
        "Gold" => p.gold,
        "Unique" | "Evolved" | "Inherited unique" => p.pink,
        _ => p.text,
    }
}

pub fn toggle<T: std::hash::Hash + Eq>(set: &mut HashSet<T>, value: T, on: bool) {
    if on {
        set.insert(value);
    } else {
        set.remove(&value);
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

/// A link that opens `url` in the default browser. eframe's own `links` feature pulls in
/// `webbrowser`, which links shlwapi, and the minimal MinGW in the GNU toolchain lacks it.
pub fn web_link(ui: &mut egui::Ui, label: &str, url: &str) {
    if ui.link(format!("{label} ↗")).on_hover_text(url).clicked() {
        open_url(url);
    }
}

pub fn open_url(url: &str) {
    #[cfg(windows)]
    let result = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(err) = result {
        eprintln!("couldn't open {url}: {err}");
    }
}

/// Open a folder in the file manager, or show a file selected in its folder (Explorer's /select,
/// Finder's -R). xdg-open has no "select", so on Linux a file opens its folder.
pub fn show_in_folder(path: &std::path::Path) -> std::io::Result<std::process::Child> {
    #[cfg(windows)]
    {
        if path.is_dir() {
            std::process::Command::new("explorer").arg(path).spawn()
        } else {
            std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn()
        }
    }
    #[cfg(target_os = "macos")]
    {
        if path.is_dir() { std::process::Command::new("open").arg(path).spawn() } else { std::process::Command::new("open").arg("-R").arg(path).spawn() }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
        std::process::Command::new("xdg-open").arg(dir).spawn()
    }
}

/// Stat tiles in equal columns that fill the row, so their edges line up with the cards below.
pub fn stat_row(ui: &mut egui::Ui, tiles: &[(&str, &str, String, String, Color32)]) {
    ui.columns(tiles.len().max(1), |cols| {
        for (col, (icon, label, value, sub, accent)) in cols.iter_mut().zip(tiles) {
            let w = col.available_width() - 34.0;
            stat_tile_w(col, w, icon, label, value, sub, *accent);
        }
    });
}

/// A headline number with a caption, its content at least `width` wide.
pub fn stat_tile_w(ui: &mut egui::Ui, width: f32, icon: &str, label: &str, value: &str, sub: &str, accent: Color32) {
    let p = pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_min_width(width);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon).size(18.0).color(accent));
                    ui.label(RichText::new(label).color(p.weak));
                });
                ui.label(semibold(value, 28.0));
                if !sub.is_empty() {
                    ui.label(RichText::new(sub).size(12.0).color(p.weak));
                }
            });
        });
}

/// Empty-state message centred in the available space: chibi Seiun Sky (or the icon, with the
/// mascot off), a title, and what to do next.
pub fn empty_state(ui: &mut egui::Ui, icon: &str, title: &str, body: &str) {
    empty_state_action(ui, icon, title, body, None);
}

/// An empty state with a button for the obvious next step. Returns true when it's clicked.
pub fn empty_state_action(ui: &mut egui::Ui, icon: &str, title: &str, body: &str, action: Option<&str>) -> bool {
    let weak = pal(ui).weak;
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        if crate::theme::chibi() == crate::theme::Chibi::Off {
            ui.label(RichText::new(icon).size(40.0).color(weak));
        } else {
            chibi(ui, 5, 120.0);
        }
        ui.label(semibold(title, size::LG));
        ui.label(RichText::new(body).color(weak));
        if let Some(text) = action {
            ui.add_space(6.0);
            clicked = ui.add(primary_button(ui, text, size::BODY)).clicked();
        }
    });
    clicked
}

/// A grey placeholder bar that pulses gently while background work runs.
pub fn skeleton(ui: &mut egui::Ui, width: f32, height: f32) -> Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(width.min(ui.available_width()), height), Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    let pulse = 0.55 + 0.25 * (t * 2.4).sin();
    ui.painter().rect_filled(rect, CornerRadius::same(6), p.raised.gamma_multiply(pulse + 0.2));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    resp
}

/// A few skeleton lines of decreasing width, like a paragraph loading.
pub fn skeleton_lines(ui: &mut egui::Ui, lines: usize) {
    let w = ui.available_width();
    for i in 0..lines {
        skeleton(ui, w * [1.0, 0.86, 0.64, 0.78][i % 4], 12.0);
        ui.add_space(4.0);
    }
}



/// Paints an image into `rect` without ever stretching it: when the rect's shape differs from
/// the image's (or from `uv`, the part of it to show), the image is cropped evenly at the edges
/// to fit, like CSS `object-fit: cover`. While the texture loads, it paints as `paint_at` would.
pub fn paint_cover(ui: &egui::Ui, image: egui::Image<'_>, rect: Rect, uv: Rect) {
    let size = image.load_for_size(ui.ctx(), rect.size()).ok().and_then(|t| t.size());
    let uv = match size {
        Some(px) if rect.width() > 0.0 && rect.height() > 0.0 => cover_uv(px, uv, rect.size()),
        _ => uv,
    };
    image.uv(uv).paint_at(ui, rect);
}

/// The part of `uv` (in an image of `px` pixels) with `target`'s aspect ratio, centred.
pub fn cover_uv(px: egui::Vec2, uv: Rect, target: egui::Vec2) -> Rect {
    let src = egui::vec2(px.x * uv.width(), px.y * uv.height());
    let (want, have) = (target.x / target.y, src.x / src.y);
    if (want - have).abs() < 1e-4 {
        return uv;
    }
    let c = uv.center();
    if have > want {
        // The image is wider: trim its sides.
        let w = uv.width() * want / have;
        Rect::from_center_size(c, egui::vec2(w, uv.height()))
    } else {
        let h = uv.height() * have / want;
        Rect::from_center_size(c, egui::vec2(uv.width(), h))
    }
}
pub fn placeholder(p: &Painter, rect: Rect, color: Color32, text: &str) {
    p.rect_filled(rect, CornerRadius::same(6), color.gamma_multiply(0.35));
    p.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(12.0), Color32::WHITE);
}

/// Hover/selection ring around a tile image.
pub fn tile_ring(ui: &egui::Ui, rect: Rect, selected: bool, hovered: bool, radius: u8) {
    if !(selected || hovered) {
        return;
    }
    let p = pal(ui);
    let (w, c) = if selected { (2.5, p.accent) } else { (1.5, p.weak) };
    ui.painter().rect_stroke(rect.expand(2.5), CornerRadius::same(radius), Stroke::new(w, c), StrokeKind::Outside);
}

/// Unlit stars and diamonds: dark grey on dark, light grey on light.
fn dim(p: &Painter) -> Color32 {
    if p.ctx().global_style().visuals.dark_mode { DIM } else { Color32::from_gray(200) }
}

/// Four LB diamonds starting at `left_center`, filled up to `lb`.
pub fn draw_lb(p: &Painter, left_center: Pos2, lb: u8, r: f32) {
    for i in 0..4u8 {
        let c = Pos2::new(left_center.x + r + f32::from(i) * (2.0 * r + 2.5), left_center.y);
        let pts = vec![c + vec2(0.0, -r), c + vec2(r * 0.8, 0.0), c + vec2(0.0, r), c + vec2(-r * 0.8, 0.0)];
        let fill = if i < lb { LB_ON } else { dim(p) };
        p.add(Shape::convex_polygon(pts, fill, Stroke::NONE));
    }
}

/// LB diamonds as a widget.
pub fn lb_widget(ui: &mut egui::Ui, lb: u8, r: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(4.0 * (2.0 * r + 2.5), 2.0 * r + 2.0), Sense::hover());
    draw_lb(ui.painter(), rect.left_center(), lb, r);
    resp
}

/// Five stars centred on `center`, the first `n` gold.
pub fn draw_stars(p: &Painter, center: Pos2, n: u8, r: f32) {
    let step = 2.0 * r + 1.5;
    let x0 = center.x - 2.0 * step;
    for i in 0..5u8 {
        let c = Pos2::new(x0 + f32::from(i) * step, center.y);
        draw_star(p, c, r, if i < n { GOLD } else { dim(p) });
    }
}

pub fn stars_widget(ui: &mut egui::Ui, n: u8, r: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(5.0 * (2.0 * r + 1.5), 2.0 * r + 2.0), Sense::hover());
    draw_stars(ui.painter(), rect.center(), n, r);
    resp
}

/// One star centred on `c`.
pub fn draw_star_at(p: &Painter, c: Pos2, r: f32, color: Color32) {
    draw_star(p, c, r, color);
}

/// A five-point star as a convex pentagon plus five convex tips.
fn draw_star(p: &Painter, c: Pos2, r: f32, color: Color32) {
    let inner = r * 0.45;
    let point = |radius: f32, k: f32| {
        let a = -std::f32::consts::FRAC_PI_2 + k * std::f32::consts::PI / 5.0;
        c + Vec2::angled(a) * radius
    };
    let inner_pts: Vec<Pos2> = (0..5).map(|k| point(inner, (2 * k + 1) as f32)).collect();
    p.add(Shape::convex_polygon(inner_pts.clone(), color, Stroke::NONE));
    for k in 0..5 {
        let tip = point(r, (2 * k) as f32);
        p.add(Shape::convex_polygon(vec![tip, inner_pts[k], inner_pts[(k + 4) % 5]], color, Stroke::NONE));
    }
}

pub fn potential_badge(p: &Painter, bottom_right: Pos2, potential: u8) {
    let rect = Rect::from_min_max(bottom_right - vec2(26.0, 16.0), bottom_right);
    let (fill, text) = if potential >= 5 {
        (Color32::from_rgb(180, 120, 20), Color32::WHITE)
    } else {
        (Color32::from_gray(60), Color32::from_gray(220))
    };
    p.rect_filled(rect, CornerRadius::same(4), fill);
    p.text(rect.center(), Align2::CENTER_CENTER, format!("P{potential}"), FontId::proportional(11.0), text);
}

/// Whether every word of `query` appears in `text`, ignoring case.
pub fn matches_query(text: &str, query: &str) -> bool {
    let t = text.to_lowercase();
    query.split_whitespace().all(|w| t.contains(&w.to_lowercase()))
}

/// A dropdown row: (key, label, hover); a key of `None` is a heading row.
pub type ComboItems<K> = Vec<(Option<K>, String, String)>;

/// A dropdown with a search box at the top, for long lists. `items` builds the rows, and only
/// runs while the dropdown is open. Returns the key picked this frame.
pub fn search_combo<K: Clone + PartialEq>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    selected_text: impl Into<egui::WidgetText>,
    width: f32,
    items: impl FnOnce() -> ComboItems<K>,
    current: Option<&K>,
) -> Option<K> {
    let id = ui.make_persistent_id(id_salt);
    let query_id = id.with("query");
    let mut query: String = ui.data(|d| d.get_temp(query_id)).unwrap_or_default();
    let mut picked = None;
    let weak = pal(ui).weak;
    let resp = egui::ComboBox::from_id_salt(id)
        .selected_text(selected_text.into().text())
        .width(width)
        .height(420.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            ui.set_min_width(width.max(260.0));
            // Keep the popup as tall as the whole list. If it shrank while filtering, egui
            // wouldn't grow it back when the search is cleared, and the list looked gone.
            let full_h = id.with("full_height");
            let start = ui.cursor().top();
            if let Some(h) = ui.data(|d| d.get_temp::<f32>(full_h)) {
                ui.set_min_height(h);
            }
            let edit = ui.add(egui::TextEdit::singleline(&mut query).hint_text("🔍 Type to search…").desired_width(f32::INFINITY));
            let opened = id.with("opened");
            if ui.data(|d| d.get_temp::<bool>(opened)).is_none() {
                edit.request_focus();
                ui.data_mut(|d| d.insert_temp(opened, true));
            }
            let mut shown = 0;
            let mut heading: Option<&str> = None;
            let items = items();
            for (key, label, hover) in &items {
                let Some(k) = key else {
                    heading = Some(label);
                    continue;
                };
                if !query.is_empty() && !matches_query(label, &query) {
                    continue;
                }
                if let Some(h) = heading.take() {
                    ui.label(RichText::new(h).size(11.5).color(weak));
                }
                shown += 1;
                if shown > 400 {
                    ui.label(RichText::new("…more; keep typing to narrow it down").size(11.5).color(weak));
                    break;
                }
                let mut r = ui.selectable_label(current == Some(k), label);
                if !hover.is_empty() {
                    r = r.on_hover_text(hover);
                }
                if r.clicked() {
                    picked = Some(k.clone());
                }
            }
            if shown == 0 {
                ui.label(RichText::new("No match.").color(weak));
            }
            if query.is_empty() {
                let natural = ui.cursor().top() - start;
                ui.data_mut(|d| d.insert_temp(full_h, natural));
            }
            if picked.is_some() {
                ui.close();
            }
        });
    if resp.inner.is_none() || picked.is_some() {
        ui.data_mut(|d| d.remove::<bool>(id.with("opened")));
        query.clear();
    }
    ui.data_mut(|d| d.insert_temp(query_id, query));
    picked
}

// ---------------------------------------------------------------------------
// Filters: chips, sort, collapsible sections and the active-filter bar

/// A pill toggle for filters, with an optional count. The caller flips its state on click.
pub fn chip(ui: &mut egui::Ui, on: bool, label: &str, count: Option<usize>) -> Response {
    let p = pal(ui);
    let text_color = if on { p.text } else { p.weak };
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), text_color);
    let count_galley = count.map(|n| ui.painter().layout_no_wrap(n.to_string(), FontId::proportional(size::XS), p.weak));
    let count_w = count_galley.as_ref().map_or(0.0, |g| g.size().x + 12.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + count_w + 22.0, 26.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = CornerRadius::same(13);
        if on {
            painter.rect_filled(rect, radius, p.accent.gamma_multiply(if ui.visuals().dark_mode { 0.28 } else { 0.16 }));
            painter.rect_stroke(rect, radius, Stroke::new(1.0, p.accent_strong), StrokeKind::Inside);
        } else {
            painter.rect_filled(rect, radius, if resp.hovered() { p.hover } else { Color32::TRANSPARENT });
            painter.rect_stroke(rect, radius, Stroke::new(1.0, p.border), StrokeKind::Inside);
        }
        painter.galley(egui::pos2(rect.left() + 11.0, rect.center().y - galley.size().y / 2.0), galley, text_color);
        if let Some(g) = count_galley {
            let x = rect.right() - 11.0 - g.size().x;
            let pill = Rect::from_min_size(egui::pos2(x - 5.0, rect.center().y - 8.0), vec2(g.size().x + 10.0, 16.0));
            painter.rect_filled(pill, CornerRadius::same(8), p.raised);
            painter.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, p.weak);
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A collapsible group of filters in a sidebar: its title with an ⓘ hover, a badge with how many
/// of its filters are on, and a Clear link. Remembers whether it's open for the session.
pub struct FilterSection<'a> {
    title: &'a str,
    help: &'a str,
    active: usize,
    default_open: bool,
}

impl<'a> FilterSection<'a> {
    pub fn new(title: &'a str) -> Self {
        Self { title, help: "", active: 0, default_open: true }
    }

    pub fn help(mut self, help: &'a str) -> Self {
        self.help = help;
        self
    }

    /// How many of this section's filters are on.
    pub fn active(mut self, n: usize) -> Self {
        self.active = n;
        self
    }

    /// Draws the section. Returns true when its Clear was clicked.
    pub fn show(self, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) -> bool {
        let p = pal(ui);
        let id = ui.make_persistent_id(("filter_section", self.title));
        let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(self.default_open);
        let mut clear = false;
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let title = format!("{} {}", if open { "▾" } else { "▸" }, self.title.to_uppercase());
            let r = ui.add(egui::Label::new(semibold(title, size::XS).color(p.weak)).sense(Sense::click()).selectable(false));
            if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                open = !open;
            }
            if !self.help.is_empty() {
                ui.add(egui::Label::new(RichText::new("ⓘ").size(size::XS).color(p.weak)).selectable(false)).on_hover_text(self.help);
            }
            if self.active > 0 {
                count_badge(ui, self.active);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let b = egui::Button::new(RichText::new("Clear").size(size::XS).color(p.link)).frame(false);
                    if ui.add(b).on_hover_text("Turn off this section's filters").clicked() {
                        clear = true;
                    }
                });
            }
        });
        ui.data_mut(|d| d.insert_temp(id, open));
        if open {
            ui.add_space(2.0);
            add(ui);
        }
        clear
    }
}

/// A small round number in the accent colour, e.g. how many filters are on.
pub fn count_badge(ui: &mut egui::Ui, n: usize) -> Response {
    let p = pal(ui);
    let text = n.to_string();
    let galley = ui.painter().layout_no_wrap(text, FontId::new(size::XS, FontFamily::Name(SEMIBOLD.into())), p.on_accent);
    let (rect, resp) = ui.allocate_exact_size(vec2((galley.size().x + 10.0).max(18.0), 18.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(9), p.accent);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.on_accent);
    resp
}

/// What the active-filter bar asks for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FilterBarAction {
    /// Turn off the filter at this index of the list passed in.
    Remove(usize),
    ResetAll,
}

/// The filters that are on, as removable chips ("Long ≥ A ✕"), then "Reset all (n)". Draws
/// nothing when none is on.
pub fn active_filter_bar(ui: &mut egui::Ui, active: &[String]) -> Option<FilterBarAction> {
    if active.is_empty() {
        return None;
    }
    let p = pal(ui);
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (i, label) in active.iter().enumerate() {
            if chip(ui, true, &format!("{label}  ✕"), None).on_hover_text("Remove this filter").clicked() {
                out = Some(FilterBarAction::Remove(i));
            }
        }
        let reset = egui::Button::new(RichText::new(format!("Reset all ({})", active.len())).size(size::SM).color(p.link)).frame(false);
        if ui.add(reset).on_hover_text("Turn off every filter").clicked() {
            out = Some(FilterBarAction::ResetAll);
        }
    });
    out
}

// ---------------------------------------------------------------------------
// Tables

/// One column of a `DataTable`.
#[derive(Clone)]
pub struct TableCol {
    /// Stable name: for sorting, the Columns menu and settings.json.
    pub key: &'static str,
    pub title: String,
    /// The full name or an explanation, shown when the header is hovered.
    pub hover: String,
    pub width: egui_extras::Column,
    pub align: egui::Align,
    pub sortable: bool,
    /// Sort highest first the first time this column is clicked (numbers, grades).
    pub desc_first: bool,
    /// Listed in the Columns menu, so it can be hidden.
    pub optional: bool,
    /// Hidden until the user turns it on.
    pub hidden: bool,
    /// A super-header over this and its neighbours with the same group, e.g. "Distance".
    pub group: Option<&'static str>,
}

impl TableCol {
    pub fn new(key: &'static str, title: impl Into<String>) -> Self {
        Self {
            key,
            title: title.into(),
            hover: String::new(),
            width: egui_extras::Column::auto(),
            align: egui::Align::Min,
            sortable: false,
            desc_first: false,
            optional: false,
            hidden: false,
            group: None,
        }
    }

    /// Put the column under a super-header shared with its neighbours.
    pub fn group(mut self, group: &'static str) -> Self {
        self.group = Some(group);
        self
    }

    pub fn width(mut self, width: egui_extras::Column) -> Self {
        self.width = width;
        self
    }

    /// Right-aligned, for numbers.
    pub fn right(mut self) -> Self {
        self.align = egui::Align::Max;
        self
    }

    /// Centred, for grades, stars and badges.
    pub fn center(mut self) -> Self {
        self.align = egui::Align::Center;
        self
    }

    pub fn hover(mut self, text: impl Into<String>) -> Self {
        self.hover = text.into();
        self
    }

    /// Sortable, lowest first on the first click (names).
    pub fn sort_asc(mut self) -> Self {
        self.sortable = true;
        self.desc_first = false;
        self
    }

    /// Sortable, highest first on the first click (numbers, grades).
    pub fn sort_desc(mut self) -> Self {
        self.sortable = true;
        self.desc_first = true;
        self
    }

    /// Can be hidden from the Columns menu; `hidden` starts it hidden.
    pub fn optional(mut self, hidden: bool) -> Self {
        self.optional = true;
        self.hidden = hidden;
        self
    }
}

/// Which column a table is sorted by, and which way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableSort {
    pub key: &'static str,
    pub desc: bool,
}

impl TableSort {
    /// A header was clicked: the same column flips direction, another starts its own way.
    pub fn click(&mut self, col: &TableCol) {
        if self.key == col.key {
            self.desc = !self.desc;
        } else {
            self.key = col.key;
            self.desc = col.desc_first;
        }
    }
}

/// What happened in a `DataTable` this frame.
#[derive(Default, Debug)]
pub struct TableOut {
    /// A row was clicked.
    pub clicked: Option<usize>,
    /// A header was clicked and the sort changed: re-sort the rows.
    pub sorted: bool,
}

/// The app's one table style: semibold headers in the weak colour, sortable headers with ▲/▼,
/// per-column alignment, rows with every cell vertically centred, a hover highlight, and an
/// accent bar on the selected row. Optional columns can be hidden with `columns_menu`.
pub struct DataTable<'a> {
    id: &'a str,
    cols: &'a [TableCol],
    sort: Option<&'a mut TableSort>,
    row_height: f32,
    selected: Option<usize>,
    scroll_to: Option<usize>,
    max_height: Option<f32>,
}

impl<'a> DataTable<'a> {
    pub fn new(id: &'a str, cols: &'a [TableCol]) -> Self {
        Self { id, cols, sort: None, row_height: 34.0, selected: None, scroll_to: None, max_height: None }
    }

    /// Scroll inside this height (for a table in a card) rather than taking the rest of the page.
    pub fn max_height(mut self, h: f32) -> Self {
        self.max_height = Some(h);
        self
    }

    /// Scroll so this row is in view (centred), this frame.
    pub fn scroll_to(mut self, row: Option<usize>) -> Self {
        self.scroll_to = row;
        self
    }

    pub fn sort(mut self, sort: &'a mut TableSort) -> Self {
        self.sort = Some(sort);
        self
    }

    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    /// The row to mark as selected.
    pub fn selected(mut self, row: Option<usize>) -> Self {
        self.selected = row;
        self
    }

    /// Draws the table; `cell(ui, row, key)` draws one cell.
    pub fn show(self, ui: &mut egui::Ui, rows: usize, mut cell: impl FnMut(&mut egui::Ui, usize, &str)) -> TableOut {
        let p = pal(ui);
        let hidden = hidden_columns(ui.ctx(), self.id, self.cols);
        let visible: Vec<&TableCol> = self.cols.iter().filter(|c| !hidden.contains(c.key)).collect();
        let current = self.sort.as_deref().copied();
        let mut clicked_col: Option<&TableCol> = None;
        let mut out = TableOut::default();
        let mut builder = egui_extras::TableBuilder::new(ui)
            .id_salt(self.id)
            .striped(true)
            .sense(Sense::click())
            .auto_shrink([false, false])
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        for c in &visible {
            builder = builder.column(c.width);
        }
        if let Some(r) = self.scroll_to {
            builder = builder.scroll_to_row(r, Some(egui::Align::Center));
        }
        if let Some(h) = self.max_height {
            builder = builder.max_scroll_height(h).min_scrolled_height(h).auto_shrink([false, true]);
        }
        let selected = self.selected;
        let row_height = self.row_height;
        let grouped = visible.iter().any(|c| c.group.is_some());
        // Header cells so far, for drawing each group's label across its columns.
        let mut cells: Vec<Rect> = Vec::new();
        builder
            .header(if grouped { 50.0 } else { 30.0 }, |mut header| {
                for (n, c) in visible.iter().enumerate() {
                    header.col(|ui| {
                        let cell = ui.max_rect();
                        cells.push(cell);
                        if let Some(g) = c.group
                            && visible.get(n + 1).is_none_or(|next| next.group != Some(g))
                        {
                            let first = (0..=n).rev().take_while(|&i| visible[i].group == Some(g)).last().unwrap_or(n);
                            let span = Rect::from_min_max(cells[first].left_top(), egui::pos2(cell.right(), cell.top() + 20.0));
                            // A painter of its own: the cell's would clip the label to this one column.
                            let painter = egui::Painter::new(ui.ctx().clone(), ui.layer_id(), span.expand(1.0));
                            painter.text(span.center(), Align2::CENTER_CENTER, g.to_uppercase(), FontId::new(size::XS, FontFamily::Name(SEMIBOLD.into())), p.weak);
                            let y = span.bottom() - 1.0;
                            painter.line_segment([egui::pos2(span.left() + 6.0, y), egui::pos2(span.right() - 6.0, y)], Stroke::new(1.0, p.border));
                        }
                        // With groups, every title sits on the lower line so the baselines match.
                        let lower = if grouped { Rect::from_min_max(egui::pos2(cell.left(), cell.bottom() - 28.0), cell.right_bottom()) } else { cell };
                        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(lower).layout(egui::Layout::left_to_right(egui::Align::Center)));
                        let ui = &mut ui;
                        with_align(ui, c.align, |ui| {
                            let on = current.is_some_and(|s| s.key == c.key);
                            let arrow = match current {
                                Some(s) if s.key == c.key => {
                                    if s.desc {
                                        " ▼"
                                    } else {
                                        " ▲"
                                    }
                                }
                                _ => "",
                            };
                            let text = semibold(format!("{}{arrow}", c.title), size::SM).color(if on { p.text } else { p.weak });
                            let label = egui::Label::new(text).selectable(false).truncate();
                            if c.sortable {
                                let mut hover = c.hover.clone();
                                if !hover.is_empty() {
                                    hover.push_str("\n\n");
                                }
                                hover.push_str(&format!("Click to sort by {}.", c.title.to_lowercase()));
                                let r = ui.add(label.sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hover);
                                if r.clicked() {
                                    clicked_col = Some(*c);
                                }
                            } else {
                                let r = ui.add(label);
                                if !c.hover.is_empty() {
                                    r.on_hover_text(&c.hover);
                                }
                            }
                        });
                    });
                }
            })
            .body(|body| {
                body.rows(row_height, rows, |mut row| {
                    let i = row.index();
                    let is_selected = selected == Some(i);
                    row.set_selected(is_selected);
                    for (n, c) in visible.iter().enumerate() {
                        row.col(|ui| {
                            if is_selected && n == 0 {
                                let r = ui.max_rect();
                                let bar = Rect::from_min_max(egui::pos2(r.left() - 4.0, r.top() + 4.0), egui::pos2(r.left() - 1.0, r.bottom() - 4.0));
                                ui.painter().rect_filled(bar, CornerRadius::same(2), p.accent);
                            }
                            with_align(ui, c.align, |ui| cell(ui, i, c.key));
                        });
                    }
                    if row.response().clicked() {
                        out.clicked = Some(i);
                    }
                });
            });
        if let (Some(col), Some(sort)) = (clicked_col, self.sort) {
            sort.click(col);
            out.sorted = true;
        }
        out
    }
}

/// Lay a cell's contents out left, centred or right, always vertically centred.
fn with_align(ui: &mut egui::Ui, align: egui::Align, add: impl FnOnce(&mut egui::Ui)) {
    let layout = match align {
        egui::Align::Min => egui::Layout::left_to_right(egui::Align::Center),
        egui::Align::Center => egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Center),
        egui::Align::Max => egui::Layout::right_to_left(egui::Align::Center),
    };
    ui.with_layout(layout, add);
}

/// The optional columns a table has hidden: settings.json's `columns.<table>`, read once a
/// session, else the columns that start hidden.
pub fn hidden_columns(ctx: &egui::Context, table: &str, cols: &[TableCol]) -> HashSet<String> {
    let id = egui::Id::unique(("table_hidden", table));
    if let Some(h) = ctx.data(|d| d.get_temp::<HashSet<String>>(id)) {
        return h;
    }
    let saved = crate::settings::get("columns").and_then(|v| v.get(table).cloned()).and_then(|v| serde_json::from_value::<Vec<String>>(v).ok());
    let hidden: HashSet<String> = match saved {
        Some(keys) => keys.into_iter().filter(|k| cols.iter().any(|c| c.optional && c.key == k)).collect(),
        None => cols.iter().filter(|c| c.optional && c.hidden).map(|c| c.key.to_string()).collect(),
    };
    ctx.data_mut(|d| d.insert_temp(id, hidden.clone()));
    hidden
}

fn set_hidden_columns(ctx: &egui::Context, table: &str, hidden: HashSet<String>) {
    let mut keys: Vec<&String> = hidden.iter().collect();
    keys.sort();
    let mut all = crate::settings::get("columns").filter(serde_json::Value::is_object).unwrap_or_else(|| serde_json::json!({}));
    all[table] = serde_json::json!(keys);
    crate::settings::set("columns", all);
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(("table_hidden", table)), hidden));
}

/// "☰ Columns": tick which optional columns a table shows. Saved in settings.json.
pub fn columns_menu(ui: &mut egui::Ui, table: &str, cols: &[TableCol]) {
    let mut hidden = hidden_columns(ui.ctx(), table, cols);
    let mut changed = false;
    ui.menu_button("☰ Columns", |ui| {
        for c in cols.iter().filter(|c| c.optional) {
            let mut on = !hidden.contains(c.key);
            let mut r = ui.tick(&mut on, &c.title);
            if !c.hover.is_empty() {
                r = r.on_hover_text(&c.hover);
            }
            if r.changed() {
                toggle(&mut hidden, c.key.to_string(), !on);
                changed = true;
            }
        }
    })
    .response
    .on_hover_text("Show or hide columns");
    if changed {
        set_hidden_columns(ui.ctx(), table, hidden);
    }
}

/// Rows as CSV (RFC 4180): a field with a comma, quote or line break is quoted.
pub fn to_csv(rows: &[Vec<String>]) -> String {
    let field = |f: &String| {
        if f.contains([',', '"', '\n', '\r']) { format!("\"{}\"", f.replace('"', "\"\"")) } else { f.clone() }
    };
    rows.iter().map(|r| r.iter().map(field).collect::<Vec<_>>().join(",")).map(|l| l + "\r\n").collect()
}

/// A table's visible columns as CSV, headers first; `text(row, key)` gives a cell's text.
pub fn table_csv(ctx: &egui::Context, table: &str, cols: &[TableCol], rows: usize, text: impl Fn(usize, &str) -> String) -> String {
    let hidden = hidden_columns(ctx, table, cols);
    let visible: Vec<&TableCol> = cols.iter().filter(|c| !hidden.contains(c.key) && !c.title.is_empty()).collect();
    let mut out = vec![visible.iter().map(|c| c.title.clone()).collect::<Vec<_>>()];
    out.extend((0..rows).map(|r| visible.iter().map(|c| text(r, c.key)).collect()));
    to_csv(&out)
}

/// A value to sort table rows by: numbers numerically, text ignoring case, with runs of digits
/// compared as numbers ("Lv 9" before "Lv 10").
#[derive(Clone, Debug)]
pub enum SortKey {
    Num(f64),
    Text(String),
}

impl SortKey {
    pub fn cmp(&self, other: &SortKey) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (SortKey::Num(a), SortKey::Num(b)) => a.total_cmp(b),
            (SortKey::Text(a), SortKey::Text(b)) => natural_cmp(a, b),
            (SortKey::Num(_), SortKey::Text(_)) => Ordering::Less,
            (SortKey::Text(_), SortKey::Num(_)) => Ordering::Greater,
        }
    }
}

/// Compare text ignoring case, with digit runs compared by value.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s.trim_start_matches('0').to_string()
                };
                let (na, nb) = (take(&mut a), take(&mut b));
                let o = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
                if o.is_ne() {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o.is_ne() {
                    return o;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// Sort rows by a key, highest or lowest first. Stable: rows with equal keys keep their order.
pub fn sort_rows<T>(rows: &mut [T], desc: bool, key: impl Fn(&T) -> SortKey) {
    rows.sort_by(|a, b| {
        let o = key(a).cmp(&key(b));
        if desc { o.reverse() } else { o }
    });
}

// ---------------------------------------------------------------------------
// Toasts: short messages that confirm or warn, stacked at the bottom centre.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
}

/// What a toast's button does; the app carries it out (`App::toast_action`).
#[derive(Clone, Debug, PartialEq)]
pub enum ToastAction {
    /// Open a file or folder with Windows.
    Open(std::path::PathBuf),
    /// Undo the change the toast reported.
    Undo,
    /// Open a saved plan in the Planner.
    OpenPlan(std::path::PathBuf),
}

#[derive(Clone)]
struct Toast {
    text: String,
    kind: ToastKind,
    action: Option<(String, ToastAction)>,
    /// Seconds it has been shown (hovering pauses it).
    shown: f64,
    last: Option<f64>,
}

fn toast_id() -> egui::Id {
    egui::Id::unique("umalord_toasts")
}

/// Show a short message at the bottom of the window for a few seconds.
pub fn toast(ctx: &egui::Context, text: impl Into<String>, kind: ToastKind) {
    push_toast(ctx, Toast { text: text.into(), kind, action: None, shown: 0.0, last: None });
}

/// A toast with a button, e.g. "Open" or "Undo".
pub fn toast_action(ctx: &egui::Context, text: impl Into<String>, kind: ToastKind, label: &str, action: ToastAction) {
    push_toast(ctx, Toast { text: text.into(), kind, action: Some((label.to_string(), action)), shown: 0.0, last: None });
}

fn push_toast(ctx: &egui::Context, t: Toast) {
    ctx.data_mut(|d| {
        let list = d.get_temp_mut_or_default::<Vec<Toast>>(toast_id());
        // The same message again just restarts its timer.
        list.retain(|x| x.text != t.text);
        list.push(t);
        let excess = list.len().saturating_sub(4);
        list.drain(..excess);
    });
    ctx.request_repaint();
}

/// One frame in a test. Its texture uploads are dropped: debug builds check they were handled.
#[cfg(test)]
pub fn test_frame(ctx: &egui::Context, input: egui::RawInput, run_ui: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
    let mut out = ctx.run_ui(input, run_ui);
    out.textures_delta.clear();
    out
}

/// The messages showing now, oldest first (for tests).
#[cfg(test)]
pub fn toasts_now(ctx: &egui::Context) -> Vec<(String, ToastKind)> {
    ctx.data(|d| d.get_temp::<Vec<Toast>>(toast_id())).unwrap_or_default().into_iter().map(|t| (t.text, t.kind)).collect()
}

/// Draw the toasts over everything; returns the action of a toast button clicked this frame.
pub fn show_toasts(ctx: &egui::Context) -> Option<ToastAction> {
    let mut list: Vec<Toast> = ctx.data(|d| d.get_temp(toast_id())).unwrap_or_default();
    if list.is_empty() {
        return None;
    }
    let now = ctx.input(|i| i.time);
    let mut picked = None;
    let mut closed = Vec::new();
    let resp = egui::Area::new(egui::Id::unique("toast_area"))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -44.0))
        .interactable(true)
        .show(ctx, |ui| {
            let p = pal(ui);
            ui.spacing_mut().item_spacing.y = 8.0;
            for (i, t) in list.iter().enumerate() {
                let life = toast_life(t);
                let fade = ((life - t.shown) / 0.35).clamp(0.0, 1.0) as f32;
                ui.scope(|ui| {
                    ui.multiply_opacity(fade);
                    let (color, icon) = match t.kind {
                        ToastKind::Info => (p.accent_strong, "ℹ"),
                        ToastKind::Success => (p.good, "✔"),
                        ToastKind::Warn => (p.warn, "⚠"),
                        ToastKind::Error => (p.bad, "✖"),
                    };
                    egui::Frame::new()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0, color.gamma_multiply(0.7)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(14, 9))
                        .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(60) })
                        .show(ui, |ui| {
                            ui.set_max_width(560.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(icon).size(size::MD).color(color));
                                ui.add(egui::Label::new(RichText::new(&t.text).color(p.text)).wrap());
                                if let Some((label, action)) = &t.action
                                    && ui.add(egui::Button::new(semibold(label.as_str(), size::BODY).color(p.link)).frame(false)).clicked()
                                {
                                    picked = Some(action.clone());
                                    closed.push(i);
                                }
                                if ui.add(egui::Button::new(RichText::new("✕").color(p.weak)).frame(false)).on_hover_text("Dismiss").clicked() {
                                    closed.push(i);
                                }
                            });
                        });
                });
            }
        });
    let paused = resp.response.contains_pointer();
    for (i, t) in list.iter_mut().enumerate() {
        if !paused && let Some(last) = t.last {
            t.shown += (now - last).clamp(0.0, 0.25);
        }
        t.last = Some(now);
        if t.shown >= toast_life(t) {
            closed.push(i);
        }
    }
    let mut i = 0;
    list.retain(|_| {
        i += 1;
        !closed.contains(&(i - 1))
    });
    ctx.data_mut(|d| d.insert_temp(toast_id(), list));
    ctx.request_repaint_after(std::time::Duration::from_millis(60));
    picked
}

/// How long a toast stays: longer for problems and for ones with a button.
fn toast_life(t: &Toast) -> f64 {
    match (t.kind, t.action.is_some()) {
        (_, true) => 8.0,
        (ToastKind::Warn | ToastKind::Error, _) => 6.0,
        _ => 4.0,
    }
}

// ---------------------------------------------------------------------------
// Stepper

/// Numbered steps joined by a line; finished steps show ✔. Returns a finished step that was
/// clicked, to go back to it.
pub fn stepper(ui: &mut egui::Ui, steps: &[&str], at: usize) -> Option<usize> {
    let p = pal(ui);
    let n = steps.len().max(1);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::hover());
    let seg = rect.width() / n as f32;
    let cy = rect.top() + 13.0;
    let center = |i: usize| egui::pos2(rect.left() + seg * (i as f32 + 0.5), cy);
    let painter = ui.painter();
    for i in 0..n.saturating_sub(1) {
        let color = if i < at { p.accent } else { p.border };
        painter.line_segment([center(i) + vec2(16.0, 0.0), center(i + 1) - vec2(16.0, 0.0)], Stroke::new(2.0, color));
    }
    let mut clicked = None;
    for (i, label) in steps.iter().enumerate() {
        let c = center(i);
        let bold = FontId::new(size::SM, FontFamily::Name(SEMIBOLD.into()));
        if i < at {
            painter.circle_filled(c, 12.0, p.accent);
            painter.text(c, Align2::CENTER_CENTER, "✔", FontId::proportional(size::SM), p.on_accent);
        } else if i == at {
            painter.circle(c, 12.0, p.surface, Stroke::new(2.0, p.accent));
            painter.text(c, Align2::CENTER_CENTER, (i + 1).to_string(), bold.clone(), p.accent_strong);
        } else {
            painter.circle(c, 12.0, p.surface, Stroke::new(1.0, p.border));
            painter.text(c, Align2::CENTER_CENTER, (i + 1).to_string(), bold.clone(), p.weak);
        }
        let text_color = if i <= at { p.text } else { p.weak };
        let font = if i == at { bold } else { FontId::proportional(size::SM) };
        painter.text(c + vec2(0.0, 26.0), Align2::CENTER_CENTER, *label, font, text_color);
        if i < at {
            let hit = Rect::from_center_size(c + vec2(0.0, 12.0), vec2(seg.min(140.0), 44.0));
            let r = ui.interact(hit, ui.make_persistent_id(("step", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("Back to {label}"));
            if r.clicked() {
                clicked = Some(i);
            }
        }
    }
    clicked
}

/// Today's date on this computer's clock, for dates the user writes down: a result, the day a
/// plan was made, a file name. Game schedules compare against `today()`, which is UTC.
pub fn local_today() -> String {
    let (y, m, d, ..) = crate::live::local(crate::live::now());
    format!("{y:04}-{m:02}-{d:02}")
}

/// Today's date (UTC) as YYYY-MM-DD, without a date crate.
pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {

    /// Art is cropped to its box's shape, never stretched: a card (260×346) in a 3:4 box, a
    /// square icon in a wide box, and the full-body render's middle half.
    #[test]
    fn cover_never_distorts() {
        let full = eframe::egui::Rect::from_min_max(eframe::egui::pos2(0.0, 0.0), eframe::egui::pos2(1.0, 1.0));
        for (px, uv, target) in [
            (eframe::egui::vec2(260.0, 346.0), full, eframe::egui::vec2(72.0, 96.0)),
            (eframe::egui::vec2(256.0, 256.0), full, eframe::egui::vec2(80.0, 40.0)),
            (eframe::egui::vec2(768.0, 1024.0), eframe::egui::Rect::from_min_max(eframe::egui::pos2(0.24, 0.05), eframe::egui::pos2(0.76, 0.97)), eframe::egui::vec2(127.0, 300.0)),
            (eframe::egui::vec2(260.0, 346.0), full, eframe::egui::vec2(260.0, 346.0)),
        ] {
            let c = crate::ui_util::cover_uv(px, uv, target);
            let shown = eframe::egui::vec2(px.x * c.width(), px.y * c.height());
            assert!((shown.x / shown.y - target.x / target.y).abs() < 1e-3, "{px:?} in {target:?} shows {shown:?}");
            assert!(c.min.x >= uv.min.x - 1e-6 && c.max.x <= uv.max.x + 1e-6 && c.min.y >= uv.min.y - 1e-6 && c.max.y <= uv.max.y + 1e-6);
        }
    }

    use eframe::egui;

    /// Open a search dropdown, type a filter, delete it again: every row comes back.
    #[test]
    fn search_combo_list_returns_after_clearing() {
        let ctx = egui::Context::default();
        let items: Vec<(Option<u32>, String, String)> = (0..40).map(|i| (Some(i), format!("Item {i:02}"), String::new())).collect();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 900.0));
        let frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
            let out = crate::ui_util::test_frame(&ctx, input, |ui| {
                super::search_combo(ui, "t", "Pick one", 200.0, || items.clone(), None);
            });
            // Rows drawn: text shapes that are item labels.
            out.shapes.iter().filter(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text().starts_with("Item "))).count()
        };
        frame(vec![]);
        let at = egui::pos2(60.0, 18.0);
        let click = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(vec![egui::Event::PointerMoved(at), click(true)]);
        frame(vec![click(false)]);
        let open = (0..3).map(|_| frame(vec![])).last().unwrap();
        frame(vec![egui::Event::Text("Item 1".into())]);
        let filtered = (0..3).map(|_| frame(vec![])).last().unwrap();
        let backspace = || egui::Event::Key { key: egui::Key::Backspace, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() };
        frame((0..6).map(|_| backspace()).collect());
        let cleared = (0..3).map(|_| frame(vec![])).last().unwrap();
        // A search that matches nothing shrinks the list to "No match."; it must still come back.
        frame(vec![egui::Event::Text("zzz".into())]);
        let none = (0..3).map(|_| frame(vec![])).last().unwrap();
        frame((0..3).map(|_| backspace()).collect());
        let back = (0..3).map(|_| frame(vec![])).last().unwrap();
        eprintln!("rows: open {open}, filtered {filtered}, cleared {cleared}, no match {none}, back {back}");
        assert!(open > 10, "the dropdown should open: {open}");
        assert!(filtered < open, "typing should filter: {filtered}");
        assert_eq!(cleared, open, "clearing the search should bring the rows back");
        assert_eq!(none, 0);
        assert_eq!(back, open, "clearing a search with no matches should bring the rows back");
    }

    #[test]
    fn csv_quotes_what_needs_it() {
        let rows = vec![
            vec!["Card".to_string(), "Note".to_string()],
            vec!["Kitasan Black".to_string(), "MLB, Lv 50".to_string()],
            vec!["Say \"hi\"".to_string(), "two\nlines".to_string()],
        ];
        assert_eq!(super::to_csv(&rows), "Card,Note\r\nKitasan Black,\"MLB, Lv 50\"\r\n\"Say \"\"hi\"\"\",\"two\nlines\"\r\n");
    }

    #[test]
    fn sorting_is_stable_both_ways() {
        use super::{SortKey, sort_rows};
        let mut rows = vec![("b", 2.0), ("a", 1.0), ("c", 2.0), ("d", 3.0)];
        sort_rows(&mut rows, true, |r| SortKey::Num(r.1));
        assert_eq!(rows.iter().map(|r| r.0).collect::<String>(), "dbca", "ties keep their order");
        sort_rows(&mut rows, false, |r| SortKey::Num(r.1));
        assert_eq!(rows.iter().map(|r| r.0).collect::<String>(), "abcd");
        let mut names = vec!["oguri Cap", "Lv 10", "Agnes", "Lv 9", "lv 9"];
        sort_rows(&mut names, false, |n| SortKey::Text(n.to_string()));
        assert_eq!(names, ["Agnes", "Lv 9", "lv 9", "Lv 10", "oguri Cap"], "case-insensitive, numbers by value");
    }

    #[test]
    fn table_sort_header_clicks() {
        let name = super::TableCol::new("name", "Uma").sort_asc();
        let stars = super::TableCol::new("stars", "Stars").sort_desc();
        let mut sort = super::TableSort { key: "name", desc: false };
        sort.click(&name);
        assert_eq!(sort, super::TableSort { key: "name", desc: true }, "same column flips");
        sort.click(&stars);
        assert_eq!(sort, super::TableSort { key: "stars", desc: true }, "a number column starts highest first");
        sort.click(&name);
        assert_eq!(sort, super::TableSort { key: "name", desc: false }, "a name column starts A to Z");
    }

    /// Toasts stack, the same message doesn't repeat, and they go away by themselves.
    #[test]
    fn toasts_stack_and_expire() {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let mut t = 0.0;
        let mut frame = |dt: f64| {
            t += dt;
            let input = egui::RawInput { screen_rect: Some(screen), time: Some(t), ..Default::default() };
            let _ = crate::ui_util::test_frame(&ctx, input, |ui| {
                super::show_toasts(ui.ctx());
            });
        };
        super::toast(&ctx, "Saved", super::ToastKind::Success);
        super::toast(&ctx, "Saved", super::ToastKind::Success);
        super::toast(&ctx, "Couldn't save", super::ToastKind::Error);
        frame(0.0);
        assert_eq!(super::toasts_now(&ctx).len(), 2);
        for _ in 0..35 {
            frame(0.2);
        }
        // After 6 s the success is gone; the error stays its 6 s, then goes too.
        assert!(super::toasts_now(&ctx).is_empty(), "{:?}", super::toasts_now(&ctx));
    }

    /// The new widgets draw without panicking, headless.
    #[test]
    fn widgets_draw() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, crate::theme::Mode::Light);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0));
        let cols = vec![
            super::TableCol::new("name", "Uma").sort_asc(),
            super::TableCol::new("lv", "Level").right().sort_desc(),
            super::TableCol::new("note", "Notes").optional(true),
        ];
        let mut sort = super::TableSort { key: "name", desc: false };
        let grouped = vec![super::TableCol::new("turf", "Turf").center().group("Surface"), super::TableCol::new("dirt", "Dirt").center().group("Surface")];
        let mut set = std::collections::BTreeSet::from([1u8]);
        let mut one = Some(0);
        let mut sort2 = super::TableSort { key: "stars", desc: true };
        for _ in 0..2 {
            let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            let _ = crate::ui_util::test_frame(&ctx, input, |ui| {
                crate::collection_kit::chip_set(ui, &mut set, &[(1, "5★".into(), Some(3), None, "Five stars".into()), (2, "4★".into(), None, None, String::new())]);
                crate::collection_kit::option_chips(ui, &mut one, &[(0, "Any"), (1, "P5")]);
                ui.horizontal(|ui| crate::collection_kit::sort_picker(ui, "s", &mut sort2, &[("stars", "Stars", true), ("name", "Name", false)]));
                super::DataTable::new("g", &grouped).show(ui, 2, |ui, _, key| _ = super::grade_tile(ui, if key == "turf" { "A" } else { "G" }));
                super::FilterSection::new("Stars").help("How many").active(1).show(ui, |ui| _ = ui.label("inside"));
                super::active_filter_bar(ui, &["Long ≥ A".to_string()]);
                super::stepper(ui, &["Welcome", "You", "Umas", "Cards", "Done"], 2);
                super::skeleton_lines(ui, 2);
                super::DataTable::new("t", &cols).sort(&mut sort).selected(Some(1)).show(ui, 3, |ui, r, key| _ = ui.label(format!("{key} {r}")));
            });
        }
        let csv = super::table_csv(&ctx, "t", &cols, 2, |r, k| format!("{k}{r}"));
        assert!(csv.starts_with("Uma,Level"), "{csv}");
    }

    #[test]
    fn today_is_a_date() {
        let t = super::today();
        assert_eq!(t.len(), 10);
        assert!(t.starts_with("20"));
    }
}
