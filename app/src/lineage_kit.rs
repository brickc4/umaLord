//! The Lineage page's drawn pieces: spark chips drawn like the game's (colours sampled from the
//! game's own chips; stars centred below, wanted stars as hollow targets), the odds strip, and
//! contrast-safe text colours.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, pos2, vec2};

/// A spark's colour, as the game draws it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Spark {
    Blue,
    Pink,
    Green,
    White,
}

impl Spark {
    /// From `Want::chip`'s colour number.
    pub fn of(kind: u8) -> Self {
        match kind {
            0 => Spark::Blue,
            1 => Spark::Pink,
            2 => Spark::Green,
            _ => Spark::White,
        }
    }

    /// Fill left, fill right, border, text.
    fn colors(self, dark: bool) -> (Color32, Color32, Color32, Color32) {
        let rgb = Color32::from_rgb;
        match self {
            Spark::Blue => (rgb(0x34, 0xB6, 0xF4), rgb(0x68, 0xCB, 0xFA), rgb(0x21, 0x9A, 0xF0), Color32::WHITE),
            Spark::Pink => (rgb(0xFF, 0x75, 0xB0), rgb(0xFF, 0x95, 0xC3), rgb(0xF0, 0x5E, 0x9E), Color32::WHITE),
            Spark::Green => (rgb(0x91, 0xD0, 0x2E), rgb(0x9A, 0xDA, 0x37), rgb(0x78, 0xBA, 0x22), Color32::WHITE),
            Spark::White if dark => (rgb(0xCF, 0xCF, 0xD2), rgb(0xCF, 0xCF, 0xD2), rgb(0xB8, 0xB8, 0xBC), rgb(0x5A, 0x46, 0x36)),
            Spark::White => (rgb(0xE1, 0xE0, 0xE1), rgb(0xE1, 0xE0, 0xE1), rgb(0xCE, 0xCF, 0xCE), rgb(0x5A, 0x46, 0x36)),
        }
    }
}

const GOLD: Color32 = Color32::from_rgb(0xFD, 0xCE, 0x31);
const GOLD_EDGE: Color32 = Color32::from_rgb(0xD3, 0x9F, 0x2C);
const EMPTY: Color32 = Color32::from_rgb(0xE3, 0xE2, 0xE3);
const EMPTY_EDGE: Color32 = Color32::from_rgb(0xA9, 0xA9, 0xAD);
const MISS: Color32 = Color32::from_rgb(0xE5, 0x3E, 0x3E);

/// A five-pointed star of outer radius `r`.
pub fn star(painter: &egui::Painter, c: Pos2, r: f32, fill: Color32, edge: Stroke) {
    let pts: Vec<Pos2> = (0..10)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let rr = if i % 2 == 0 { r } else { r * 0.45 };
            c + vec2(a.cos() * rr, a.sin() * rr)
        })
        .collect();
    if fill != Color32::TRANSPARENT {
        let mut mesh = Mesh::default();
        mesh.colored_vertex(c, fill);
        for p in &pts {
            mesh.colored_vertex(*p, fill);
        }
        for i in 0..10u32 {
            mesh.add_triangle(0, 1 + i, 1 + (i + 1) % 10);
        }
        painter.add(Shape::mesh(mesh));
    }
    let mut closed = pts.clone();
    closed.push(pts[0]);
    painter.add(Shape::line(closed, edge));
}

/// Three stars: `have` filled gold; up to `wanted`, the rest drawn as targets (gold outlines on
/// a light backing) or, when the uma is short, red slots; the rest empty grey.
pub fn stars(painter: &egui::Painter, center: Pos2, r: f32, have: u8, wanted: Option<u8>, near_miss: bool) {
    let gap = r * 2.15;
    // Wanted stars sit on a light backing, so hollow targets read on any chip colour.
    if wanted.is_some() {
        let back = Rect::from_center_size(center, vec2(gap * 2.0 + r * 2.6, r * 2.6));
        painter.rect_filled(back, r * 1.3, Color32::from_white_alpha(225));
    }
    for i in 0..3u8 {
        let c = center + vec2((i as f32 - 1.0) * gap, 0.0);
        if i < have {
            star(painter, c, r, GOLD, Stroke::new(1.0, GOLD_EDGE));
        } else if wanted.is_some_and(|w| i < w) {
            // A target: hollow, with a firm outline; red when she came up short.
            star(painter, c, r, Color32::TRANSPARENT, Stroke::new(1.6, if near_miss { MISS } else { GOLD_EDGE }));
        } else {
            star(painter, c, r * 0.85, EMPTY, Stroke::new(1.0, EMPTY_EDGE));
        }
    }
}

/// A spark chip like the game's: a rounded bar shading left to right, a sheen at the top right,
/// a glossy sphere at the left (gold on green chips), the name in white (dark on white chips),
/// and three stars, centred under the chip (`compact`: inside it, at the right). `wanted` draws
/// target stars; `have` below `wanted` with `near_miss` marks the missing ones red.
pub fn spark_chip(ui: &mut egui::Ui, kind: Spark, name: &str, have: u8, wanted: Option<u8>, near_miss: bool, compact: bool) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let (left, right, border, text) = kind.colors(dark);
    let h = if compact { 26.0 } else { 34.0 };
    let font = FontId::new(if compact { 12.0 } else { 14.5 }, FontFamily::Name(crate::theme::SEMIBOLD.into()));
    let galley = ui.painter().layout_no_wrap(name.to_string(), font.clone(), text);
    let stars_w = if compact { 3.0 * 2.15 * 6.5 + 6.0 } else { 0.0 };
    let w = (h * 0.95 + galley.size().x + stars_w + if compact { 10.0 } else { 26.0 }).max(if compact { 90.0 } else { 150.0 });
    let below = if compact { 0.0 } else { 9.0 };
    let (outer, resp) = ui.allocate_exact_size(vec2(w, h + below), Sense::hover());
    let rect = Rect::from_min_size(outer.min, vec2(w, h));
    if !ui.is_rect_visible(outer) {
        return resp;
    }
    let p = ui.painter();
    let r = 5.0;
    // The fill: the right colour all over, the left colour at the left end, a gradient between.
    p.rect_filled(rect, r, right);
    p.rect_filled(Rect::from_min_size(rect.min, vec2(2.0 * r, h)), r, left);
    let mut mesh = Mesh::default();
    let (x0, x1) = (rect.left() + r, rect.right() - r);
    for (x, c) in [(x0, left), (x1, right)] {
        mesh.colored_vertex(pos2(x, rect.top()), c);
        mesh.colored_vertex(pos2(x, rect.bottom()), c);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    p.add(Shape::mesh(mesh));
    // The sheen: two slanted light bands near the right.
    let sheen = Color32::from_white_alpha(if kind == Spark::White { 50 } else { 42 });
    for (from, width) in [(0.70, 0.07), (0.80, 0.03)] {
        let xa = rect.left() + w * from;
        let slant = h * 0.55;
        p.add(Shape::convex_polygon(vec![pos2(xa + slant, rect.top() + 1.0), pos2(xa + slant + w * width, rect.top() + 1.0), pos2(xa + w * width, rect.bottom() - 1.0), pos2(xa, rect.bottom() - 1.0)], sheen, Stroke::NONE));
    }
    p.rect_stroke(rect, r, Stroke::new(1.0, border), StrokeKind::Inside);
    // The sphere.
    let sr = h * 0.24;
    let sc = pos2(rect.left() + h * 0.48, rect.center().y);
    let (ball, ball_edge) = if kind == Spark::Green { (Color32::from_rgb(0xF5, 0xC5, 0x42), Color32::from_rgb(0xC9, 0x93, 0x1C)) } else { (Color32::from_rgb(0xC3, 0xCF, 0xEE), Color32::from_rgb(0x8E, 0x9C, 0xC8)) };
    p.circle(sc, sr, ball, Stroke::new(1.0, ball_edge));
    p.circle_filled(sc + vec2(-sr * 0.3, -sr * 0.3), sr * 0.35, Color32::from_white_alpha(200));
    let at = pos2(sc.x + sr + 6.0, rect.center().y - galley.size().y / 2.0);
    p.galley(at + vec2(0.45, 0.0), galley.clone(), text);
    p.galley(at, galley, text);
    if compact {
        stars(p, pos2(rect.right() - 6.0 - 2.15 * 6.5 - 6.5, rect.center().y), 6.5, have, wanted, near_miss);
    } else {
        stars(p, pos2(rect.center().x, rect.bottom() + 1.0), 7.0, have, wanted, near_miss);
    }
    resp
}

/// The odds strip: 20 dots, one per player of 20, filled for those whose career hits; and the
/// plain line beside it.
pub fn odds_dots(ui: &mut egui::Ui, p: f64, color: Color32, empty: Color32) {
    let n = 20;
    let filled = (p * n as f64).round().max(if p > 0.0 { 1.0 } else { 0.0 }) as usize;
    let d = 11.0;
    let (rect, _) = ui.allocate_exact_size(vec2(n as f32 * (d + 3.0), d), Sense::hover());
    for i in 0..n {
        let c = pos2(rect.left() + d / 2.0 + i as f32 * (d + 3.0), rect.center().y);
        if i < filled {
            ui.painter().circle_filled(c, d / 2.0, color);
        } else {
            ui.painter().circle_stroke(c, d / 2.0 - 0.5, Stroke::new(1.0, empty));
        }
    }
}

/// A segmented choice, the picked one highlighted (display only).
pub fn segmented(ui: &mut egui::Ui, options: &[&str], picked: &str, accent: Color32, on_accent: Color32, weak: Color32, frame: Color32) {
    egui::Frame::new().fill(frame).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(3)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for o in options {
                let on = *o == picked;
                let text = egui::RichText::new(*o).size(12.5).color(if on { on_accent } else { weak });
                let _ = ui.add(egui::Button::new(text).fill(if on { accent } else { Color32::TRANSPARENT }).corner_radius(CornerRadius::same(6)));
            }
        });
    });
}

/// `color` as small text on `bg`, adjusted until it reaches WCAG's 4.5:1 (darker on light
/// backgrounds, lighter on dark ones), so tinted pills and raised panels stay legible in both
/// themes (WCAG 2.2, 1.4.3).
pub fn ink(color: Color32, bg: Color32) -> Color32 {
    let light_bg = crate::theme::contrast(bg, Color32::BLACK) > crate::theme::contrast(bg, Color32::WHITE);
    let mut c = color;
    for _ in 0..24 {
        if crate::theme::contrast(c, bg) >= 4.5 {
            break;
        }
        c = if light_bg {
            Color32::from_rgb((f32::from(c.r()) * 0.9) as u8, (f32::from(c.g()) * 0.9) as u8, (f32::from(c.b()) * 0.9) as u8)
        } else {
            c.lerp_to_gamma(Color32::WHITE, 0.1)
        };
    }
    c
}

/// A tint of `color` over `base`, as a pill's fill is drawn.
pub fn tint(color: Color32, base: Color32, a: f32) -> Color32 {
    base.lerp_to_gamma(Color32::from_rgb(color.r(), color.g(), color.b()), a)
}

/// A whole number with thousands separators: 23513 → "23,513".
pub fn thousands(v: f64) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ink_reaches_aa_on_every_surface() {
        for dark in [false, true] {
            let p = crate::theme::palette(dark);
            for fg in [p.weak, p.gold, p.good, p.warn, p.accent] {
                for bg in [p.surface, p.raised, p.hover, tint(fg, p.raised, 0.16)] {
                    let c = ink(fg, bg);
                    assert!(crate::theme::contrast(c, bg) >= 4.5, "{fg:?} on {bg:?} ({}): {}", if dark { "dark" } else { "light" }, crate::theme::contrast(c, bg));
                }
            }
        }
    }
}
