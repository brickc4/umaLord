//! The detail pop-up (design round 14): a click on any support card's art or uma's avatar opens
//! everything about it in a modal over the page. It says first why it's shown here (the
//! context strip), then the same sections Collection's detail panel draws. Clicking outside, Esc
//! or ✕ closes it; a card opened from inside it stacks, with ← Back to return.

use crate::theme::{pal, size};
use crate::ui_util::{badge, lb_widget, semibold, stars_widget, type_color_str, type_icon};
use crate::{App, CareerTab, Jump};
use eframe::egui::{self, vec2, CornerRadius, RichText};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Detail {
    Card(u32),
    Uma(u32),
}

/// One finished uma of hers, for the uma pop-up: id, rank, and its blue, pink and green sparks.
#[derive(Clone)]
pub(crate) struct Finished {
    id: u64,
    /// The outfit she ran in: veterans of another outfit of hers are listed apart.
    outfit: u32,
    rank: String,
    sparks: Vec<(crate::lineage_kit::Spark, String, u8)>,
    whites: usize,
}

/// A pop-up on the stack: what it shows, why (one line, or none), and her finished umas.
#[derive(Clone)]
pub(crate) struct Open {
    what: Detail,
    context: Option<String>,
    finished: Vec<Finished>,
}

/// Where her full-body art will be, until the picture is ready: a soft glow that slowly
/// breathes, with a small spinner in it and one quiet word.
fn art_loading(ui: &egui::Ui, rect: egui::Rect) {
    let p = pal(ui);
    let t = ui.input(|i| i.time) as f32;
    let breathe = 0.5 + 0.5 * (t * 2.0).sin();
    let c = rect.center() - vec2(0.0, rect.height() * 0.06);
    // Many faint stacked ellipses blend into a smooth radial glow, brightest at the middle.
    // The outer layers are the faintest, so the glow fades to nothing at its edge.
    let steps = 32;
    for i in 0..steps {
        let f = i as f32 / steps as f32;
        let k = 1.0 - f * 0.9;
        let r = vec2(rect.width() * 0.5 * k, rect.height() * 0.3 * k);
        ui.painter().add(egui::Shape::ellipse_filled(c, r, p.accent.gamma_multiply((0.002 + 0.01 * f) * (0.55 + 0.9 * breathe))));
    }
    // A short arc going round, as egui's spinner draws.
    let r = 10.0;
    let start = t * 4.0;
    let pts: Vec<egui::Pos2> = (0..=24).map(|i| {
        let a = start + i as f32 / 24.0 * std::f32::consts::PI * 1.4;
        c + r * vec2(a.cos(), a.sin())
    }).collect();
    ui.painter().add(egui::Shape::line(pts, egui::Stroke::new(2.0, p.accent.gamma_multiply(0.85))));
    ui.painter().text(c + vec2(0.0, r + 14.0), egui::Align2::CENTER_CENTER, "Loading…", egui::FontId::proportional(11.5), p.weak);
    ui.ctx().request_repaint();
}

/// What the uma header shows beside her art, so the body leaves it out.
#[derive(Clone, Copy, Default)]
pub(crate) struct Glance {
    aptitudes: bool,
    stats: bool,
}

/// Wider windows get the large art; the pop-up is 880 px at most, 92% of a narrow window.
const MAX_W: f32 = 880.0;

impl App {
    /// Asks for a pop-up; it opens at the end of the frame (thumbnails draw from `&self`).
    pub(crate) fn open_detail(&self, what: Detail, context: Option<String>) {
        *self.detail_ask.borrow_mut() = Some((what, context));
    }

    pub(crate) fn take_detail_ask(&mut self, stack: bool) {
        let Some((what, context)) = self.detail_ask.borrow_mut().take() else { return };
        if !stack {
            self.details.clear();
        }
        if self.details.last().is_some_and(|o| o.what == what) {
            return;
        }
        let finished = match what {
            Detail::Uma(outfit) => {
                self.queue_art_of(crate::tab_cards::Art::Full, [outfit]);
                self.finished_of(outfit)
            }
            Detail::Card(_) => Vec::new(),
        };
        self.details.push(Open { what, context, finished });
    }

    /// Her veterans in any outfit of her character, best rank first; the pop-up splits this
    /// outfit's from the others'.
    fn finished_of(&self, outfit: u32) -> Vec<Finished> {
        let chara = outfit / 100;
        let mut v: Vec<_> = self.veterans().into_iter().filter(|v| v.card_id / 100 == chara).collect();
        v.sort_by_key(|v| std::cmp::Reverse(v.rank_score));
        v.iter()
            .map(|v| {
                let mut sparks = Vec::new();
                let mut whites = 0;
                for id in v.factors() {
                    let Some(s) = self.kb.sparks.get(&id) else { continue };
                    let kind = match s.kind.as_str() {
                        "blue" => crate::lineage_kit::Spark::Blue,
                        "pink" => crate::lineage_kit::Spark::Pink,
                        "green" => crate::lineage_kit::Spark::Green,
                        _ => {
                            whites += 1;
                            continue;
                        }
                    };
                    sparks.push((kind, s.name.clone(), s.stars));
                }
                Finished { id: v.trained_chara_id, outfit: v.card_id, rank: v.rank_label(), sparks, whites }
            })
            .collect()
    }

    /// Draws the pop-up on top of everything, if one is open.
    pub(crate) fn detail_modal(&mut self, ctx: &egui::Context) {
        self.take_detail_ask(false);
        let Some(open) = self.details.last().cloned() else { return };
        let screen = ctx.content_rect();
        let w = (screen.width() * 0.92).min(MAX_W);
        let max_h = screen.height() * 0.9;
        // The large art on a big window; a short one keeps the header small so the body has room.
        // The art's scale: full size on a big window, smaller on a narrow one, and never taller
        // than about 38% of the window, so the body keeps room to scroll. Below 0.6 the uma's
        // full-body art gives way to her round icon.
        let by_width: f32 = if screen.width() >= 1200.0 { 1.0 } else { 0.75 };
        let by_height = (screen.height() * 0.38 / 400.0).min(1.0);
        let art = by_width.min(by_height);
        let mut close = false;
        let mut back = false;
        let mut glanced = Glance::default();
        let modal = egui::Modal::new(egui::Id::unique("detail_modal"))
            .backdrop_color(egui::Color32::from_black_alpha(200))
            .frame(egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(18)).corner_radius(CornerRadius::same(14)).stroke(egui::Stroke::new(1.0, crate::theme::palette(ctx.global_style().visuals.dark_mode).border)))
            .show(ctx, |ui| {
                ui.set_width(w - 36.0);
                ui.set_max_height(max_h);
                let p = pal(ui);
                // ✕ sits in the top right corner; ← Back, when one pop-up opened another, above the header.
                let start = ui.min_rect().top();
                let corner = egui::Rect::from_min_size(egui::pos2(ui.max_rect().right() - 34.0, start - 6.0), vec2(40.0, 40.0));
                let x = egui::Button::new(RichText::new("✕").size(18.0)).corner_radius(CornerRadius::same(20)).fill(p.hover);
                // Child uis, so the buttons take no row of their own and the header starts at the top.
                if ui.new_child(egui::UiBuilder::new().max_rect(corner)).add(x).on_hover_text("Close (Esc, or click outside)").clicked() {
                    close = true;
                }
                if self.details.len() > 1 {
                    // ← Back on the ✕'s row, left of the header.
                    let left = egui::Rect::from_min_size(egui::pos2(ui.max_rect().right() - 130.0, start - 2.0), vec2(86.0, 32.0));
                    if ui.new_child(egui::UiBuilder::new().max_rect(left)).add(egui::Button::new(RichText::new("← Back").size(size::BODY))).on_hover_text("Back to the previous pop-up").clicked() {
                        back = true;
                    }
                }
                match open.what {
                    Detail::Card(id) => self.card_head(ui, id, art),
                    Detail::Uma(outfit) => glanced = self.uma_head(ui, outfit, art),
                }
                if let Some(c) = &open.context {
                    ui.add_space(10.0);
                    egui::Frame::new().fill(p.accent.gamma_multiply(0.16)).stroke(egui::Stroke::new(1.0, p.accent.gamma_multiply(0.5))).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(egui::Label::new(RichText::new(c).size(size::SM + 1.0).color(p.text)).wrap());
                    });
                }
                ui.add_space(8.0);
                let used = ui.min_rect().bottom() - start;
                let body_h = (max_h - used - 60.0).max(100.0);
                egui::ScrollArea::vertical().scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible).id_salt(("detail_body", self.details.len())).max_height(body_h).auto_shrink([false, true]).show(ui, |ui| match open.what {
                    Detail::Card(id) => {
                        let level = self.roster.cards.iter().find(|c| c.id == id).map(|c| u32::from(c.level));
                        self.card_sections(ui, id, level);
                    }
                    Detail::Uma(outfit) => self.uma_body(ui, outfit, &open.finished, glanced),
                });
                ui.separator();
                if self.detail_footer(ui, open.what) {
                    close = true;
                }
            });
        // A pop-up opened from inside this one stacks on it.
        self.take_detail_ask(true);
        if back {
            self.details.pop();
        } else if close || modal.should_close() {
            self.details.clear();
        }
    }

    fn card_head(&mut self, ui: &mut egui::Ui, id: u32, art: f32) {
        let p = pal(ui);
        let Some(info) = self.kb.cards.get(&id).cloned() else {
            ui.label(RichText::new(format!("Card {id}: no data for it yet.")).color(p.warn));
            return;
        };
        let owned = self.roster.cards.iter().find(|c| c.id == id).cloned();
        let h = 240.0 * art;
        ui.horizontal_top(|ui| {
            match self.art_uri("cards", id) {
                Some(uri) => _ = ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(h * 260.0 / 346.0, h)).corner_radius(CornerRadius::same(10))),
                None => _ = ui.allocate_exact_size(vec2(h * 260.0 / 346.0, h), egui::Sense::hover()),
            }
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.add(egui::Label::new(semibold(&info.uma, size::XL)).wrap());
                ui.add(egui::Label::new(RichText::new(format!("[{}]", info.title)).size(size::MD).color(p.weak)).wrap());
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    badge(ui, &info.rarity, p.gold);
                    badge(ui, format!("{} {}", type_icon(&info.kind), info.kind), type_color_str(&info.kind));
                });
                ui.add_space(8.0);
                match &owned {
                    Some(c) => {
                        let cap = c.rarity.max_level(c.lb);
                        ui.horizontal(|ui| {
                            lb_widget(ui, c.lb, 7.0);
                            let lb = if c.is_mlb() { "MLB".to_string() } else { format!("LB{}", c.lb) };
                            badge(ui, format!("Yours · {lb} · level {} of {cap}", c.level), p.accent);
                        });
                    }
                    None => _ = badge(ui, "Not in your collection", p.weak),
                }
                ui.add_space(4.0);
                if !info.release.is_empty() && info.release < "2030".to_string() {
                    ui.label(RichText::new(format!("On Global since {}", info.release)).size(size::SM).color(p.weak));
                }
            });
        });
    }

    fn uma_head(&mut self, ui: &mut egui::Ui, outfit: u32, art: f32) -> Glance {
        let p = pal(ui);
        let full = crate::parentage::uma_name(&self.kb, outfit);
        let (name, title) = full.split_once(" [").map_or((full.clone(), String::new()), |(n, t)| (n.to_string(), format!("[{t}")));
        let owned = self.roster.umas.iter().find(|u| u.id == outfit).cloned();
        let s = (160.0 * art).max(80.0);
        // Her full-body render once it's downloaded (a short window keeps the round icon, so the
        // body has room); the render's sides are empty, so only its middle half is drawn.
        // Her full-body art takes its own space while it downloads or loads from the cache (a
        // quiet loading animation there, never the icon first); a failed download falls back to
        // the round icon. UMALORD_ART=loading holds the animation for screenshots.
        let ready = self.art_uri("umas-full", outfit);
        let coming = self.art_coming(crate::tab_cards::Art::Full, outfit) || std::env::var("UMALORD_ART").as_deref() == Ok("loading");
        let slot = art >= 0.6 && (ready.is_some() || coming);
        // Her grades and stats join the header only when the window is tall enough to keep the
        // body's room; otherwise they stay in the body.
        let tall = ui.ctx().content_rect().height();
        let glance = Glance { aptitudes: slot, stats: slot && tall >= 760.0 };
        ui.horizontal_top(|ui| {
            if slot {
                let h = 400.0 * art;
                // Her box takes the render's own shape (768×1024 for all of them today), cut to
                // the middle half, so she's never stretched whatever the window.
                let crop = egui::Rect::from_min_max(egui::pos2(0.24, 0.05), egui::pos2(0.76, 0.97));
                let image = ready.filter(|_| std::env::var("UMALORD_ART").as_deref() != Ok("loading")).map(egui::Image::new);
                let poll = image.as_ref().and_then(|i| i.load_for_size(ui.ctx(), vec2(768.0, 1024.0)).ok());
                let px = poll.as_ref().and_then(|t| t.size()).unwrap_or(vec2(768.0, 1024.0));
                let aspect = (px.x * crop.width()) / (px.y * crop.height());
                let (rect, _) = ui.allocate_exact_size(vec2(h * aspect, h), egui::Sense::hover());
                match (image, poll) {
                    (Some(image), Some(egui::load::TexturePoll::Ready { .. })) => {
                        // A soft floor shadow under her feet: many faint ellipses blend smoothly.
                        let floor = rect.center_bottom() - vec2(0.0, h * 0.045);
                        let a = if ui.visuals().dark_mode { 9 } else { 4 };
                        for i in 0..12 {
                            let k = 1.0 - i as f32 / 12.0 * 0.7;
                            ui.painter().add(egui::Shape::ellipse_filled(floor, vec2(rect.width() * 0.36 * k, h * 0.02 * k), egui::Color32::from_black_alpha(a)));
                        }
                        crate::ui_util::paint_cover(ui, image, rect, crop);
                    }
                    _ => art_loading(ui, rect),
                }
            } else {
                match self.art_uri("umas", outfit) {
                    Some(uri) => _ = ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(s, s)).corner_radius(CornerRadius::same((s / 2.0) as u8))),
                    None => _ = ui.allocate_exact_size(vec2(s, s), egui::Sense::hover()),
                }
            }
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.add(egui::Label::new(semibold(&name, size::XL)).wrap());
                if !title.is_empty() {
                    ui.add(egui::Label::new(RichText::new(title).size(size::MD).color(p.weak)).wrap());
                }
                ui.add_space(8.0);
                match &owned {
                    Some(u) => {
                        ui.horizontal(|ui| {
                            stars_widget(ui, u.stars, 7.5);
                            badge(ui, format!("Yours · potential {} of 5", u.potential), p.accent);
                        });
                    }
                    None => _ = badge(ui, "Not in your collection", p.weak),
                }
                ui.add_space(4.0);
                if let Some(i) = self.kb.umas.get(&outfit).cloned() {
                    let when = if i.release == crate::knowledge::GLOBAL_LAUNCH { "On Global since launch".to_string() } else { format!("On Global since {}", i.release) };
                    ui.label(RichText::new(when).size(size::SM).color(p.weak));
                    // Beside her full-body art, her grades and stats at a glance.
                    if glance.aptitudes {
                        ui.add_space(10.0);
                        crate::tab_umas::aptitude_rows(ui, &i);
                    }
                    if glance.stats {
                        ui.add_space(6.0);
                        crate::tab_umas::stats_grid(ui, &i);
                    }
                }
            });
        });
        glance
    }

    /// The uma pop-up's body: her finished umas, her support cards, then Collection's sections.
    fn uma_body(&mut self, ui: &mut egui::Ui, outfit: u32, finished: &[Finished], glance: Glance) {
        let p = pal(ui);
        let potential = self.roster.umas.iter().find(|u| u.id == outfit).map(|u| u.potential);
        // The game's own word: finished careers go to the Veteran List, and become parents.
        let (this, other): (Vec<&Finished>, Vec<&Finished>) = finished.iter().partition(|f| f.outfit == outfit);
        let owned = self.roster.umas.iter().any(|u| u.id == outfit);
        crate::ui_util::card_explained(ui, "Your veterans", "Her finished careers on your Veteran List: parents you can breed other umas from. Import them in Settings → Profile.", |ui| {
            if this.is_empty() {
                let none = if owned { "None in this outfit yet." } else { "None yet: you don't have this outfit." };
                ui.label(RichText::new(none).color(p.weak));
            }
            self.veteran_rows(ui, &this);
            // Her other outfits' veterans: the same character, so the same affinity as a parent.
            let mut outfits: Vec<u32> = other.iter().map(|f| f.outfit).collect();
            outfits.sort_unstable();
            outfits.dedup();
            for o in outfits {
                let rows: Vec<&Finished> = other.iter().copied().filter(|f| f.outfit == o).collect();
                let title = crate::parentage::uma_name(&self.kb, o);
                let title = title.split_once(" [").map_or(title.clone(), |(_, t)| format!("[{t}"));
                ui.add_space(6.0);
                ui.label(semibold(format!("Veterans of her other outfit, {title}:"), size::SM).color(p.weak))
                    .on_hover_text("The same character, so as a parent she brings the same affinity; her sparks are her own.");
                self.veteran_rows(ui, &rows);
            }
        });
        // Her support cards: a click opens one on top, with ← Back.
        let name = self.kb.umas.get(&outfit).map(|u| u.name.clone()).unwrap_or_default();
        let today = crate::ui_util::today();
        let mut cards: Vec<u32> = self.kb.cards.values().filter(|c| c.uma == name && c.released(&today)).map(|c| c.id).collect();
        cards.sort_unstable_by(|a, b| b.cmp(a));
        if !cards.is_empty() {
            crate::ui_util::card_explained(ui, "Her support cards", "Click one for its details. A card can't go in her own deck.", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for id in cards {
                        self.card_art(ui, id, 54.0, false);
                    }
                });
            });
        }
        self.uma_sections(ui, outfit, potential, !glance.aptitudes, !glance.stats);
    }

    /// Veteran rows: id, rank, blue, pink and green sparks, and how many whites.
    fn veteran_rows(&self, ui: &mut egui::Ui, rows: &[&Finished]) {
        let p = pal(ui);
        for f in rows.iter().take(6) {
            ui.horizontal_wrapped(|ui| {
                ui.label(semibold(format!("#{}", f.id), size::BODY));
                ui.label(RichText::new(&f.rank).size(size::SM).color(p.weak)).on_hover_text("Her rank when she finished: G is lowest, then up through A, S and SS to UG.");
                for (kind, name, stars) in &f.sparks {
                    crate::lineage_kit::spark_chip(ui, *kind, name, *stars, None, false, true);
                }
                if f.whites > 0 {
                    ui.label(RichText::new(format!("+{} white spark{}", f.whites, if f.whites == 1 { "" } else { "s" })).size(size::SM).color(p.weak));
                }
            });
        }
        if rows.len() > 6 {
            ui.label(RichText::new(format!("And {} more.", rows.len() - 6)).size(size::SM).color(p.weak));
        }
    }

    /// The buttons under the pop-up; true when one of them leaves it.
    fn detail_footer(&mut self, ui: &mut egui::Ui, what: Detail) -> bool {
        let mut leave = false;
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| match what {
            Detail::Card(id) => {
                let owned = self.roster.cards.iter().find(|c| c.id == id).map(|c| c.is_mlb());
                let add = match owned {
                    None => Some("＋ Add to my collection"),
                    Some(false) => Some("Update LB"),
                    Some(true) => None,
                };
                if let Some(label) = add {
                    if ui.add(crate::ui_util::primary_button(ui, label, 14.0).min_size(vec2(0.0, 34.0))).clicked() {
                        self.tab = crate::Tab::Collection;
                        self.collection_tab = crate::CollectionTab::Cards;
                        let mut a = std::mem::take(&mut self.cards.add);
                        a.open = true;
                        self.pick_add_card(&mut a, id);
                        self.cards.add = a;
                        leave = true;
                    }
                }
                if owned.is_some() && ui.add(egui::Button::new("Open in Collection").min_size(vec2(0.0, 34.0))).clicked() {
                    self.jump.set(Some(Jump::Card(id)));
                    leave = true;
                }
            }
            Detail::Uma(outfit) => {
                let owned = self.roster.umas.iter().any(|u| u.id == outfit);
                if owned {
                    if ui.add(crate::ui_util::primary_button(ui, "📋 Plan a career with her", 14.0).min_size(vec2(0.0, 34.0))).clicked() {
                        self.planner.plan.trainee = Some(outfit);
                        if let Some(i) = self.kb.umas.get(&outfit) {
                            self.planner.plan.style = i.aptitude.best_style();
                        }
                        self.go_career(CareerTab::Plan);
                        leave = true;
                    }
                    if ui.add(egui::Button::new("Open in Collection").min_size(vec2(0.0, 34.0))).clicked() {
                        self.jump.set(Some(Jump::Uma(outfit)));
                        leave = true;
                    }
                } else if ui.add(crate::ui_util::primary_button(ui, "＋ Add to my collection", 14.0).min_size(vec2(0.0, 34.0))).clicked() {
                    self.open_setup(crate::onboarding::Step::Umas);
                    leave = true;
                }
            }
        });
        leave
    }
}

#[cfg(test)]
mod tests {
    use super::Detail;
    use crate::App;

    fn frames(app: &mut App, n: usize) {
        let ctx = eframe::egui::Context::default();
        crate::theme::install(&ctx, crate::theme::Mode::Dark);
        for f in 0..n {
            let input = eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(960.0, 600.0))),
                time: Some(f as f64 * 0.1),
                ..Default::default()
            };
            let _ = crate::ui_util::test_frame(&ctx, input, |ui| app.frame(ui));
        }
    }

    /// A pop-up from the page replaces what's open; one from inside a pop-up stacks on it, and
    /// both kinds draw, owned or not.
    #[test]
    fn pop_ups_open_stack_and_draw() {
        let mut app = App::for_tests();
        let card = app.roster.cards[0].id;
        let uma = app.roster.umas[0].id;
        let unowned = *app.kb.cards.keys().find(|id| !app.roster.cards.iter().any(|c| c.id == **id)).unwrap();
        app.open_detail(Detail::Uma(uma), Some("Why it's here".into()));
        frames(&mut app, 2);
        assert_eq!(app.details.len(), 1);
        app.open_detail(Detail::Card(card), None);
        app.take_detail_ask(true);
        app.open_detail(Detail::Card(unowned), None);
        app.take_detail_ask(true);
        frames(&mut app, 2);
        assert_eq!(app.details.len(), 3);
        app.open_detail(Detail::Card(card), None);
        frames(&mut app, 1);
        assert_eq!(app.details.len(), 1, "a pop-up opened from the page starts a new stack");
    }
}
