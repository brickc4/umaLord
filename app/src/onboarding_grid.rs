//! Setup's grids of umas and support cards, made to be quick: a "New picks start as" brush so
//! one click adds an item at the right stars or limit break; editing right on a hovered tile
//! (stars and potential, or the LB diamonds); Ctrl- and Shift-click to pick several, with a bulk
//! bar; sections with "Add all"; keys; and the selected tile's editor docked on the right.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, vec2};

use crate::onboarding::Edit;
use crate::ui_util::UiExt;
use crate::theme::{pal, size};
use crate::ui_util::{ToastAction, ToastKind, draw_lb, draw_stars, placeholder, semibold, tile_grid, today, truncate, type_color_str, type_icon};
use crate::{App, Jump};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GridKind {
    Umas,
    Cards,
}

const ICON: f32 = 84.0;
const UMA_TILE: egui::Vec2 = egui::vec2(ICON, ICON + 44.0);
const CARD_W: f32 = 78.0;
const CARD_H: f32 = CARD_W * 346.0 / 260.0;
const CARD_TILE: egui::Vec2 = egui::vec2(CARD_W, CARD_W * 346.0 / 260.0 + 32.0);
const TYPES: [&str; 7] = ["Speed", "Stamina", "Power", "Guts", "Wit", "Pal", "Group"];

/// Where on a tile a click landed.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    Body,
    Star(u8),
    Pot(u8),
    Lb(u8),
}

/// What a tile's right-click menu asked for.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Menu {
    Add,
    Remove,
    Edit,
    Details,
}

/// A group of tiles under a heading.
struct Section {
    title: String,
    color: Option<Color32>,
    ids: Vec<u32>,
}

fn order_id() -> egui::Id {
    egui::Id::unique("setup_grid_order")
}

fn positions_id() -> egui::Id {
    egui::Id::unique("setup_grid_positions")
}

impl App {
    /// The tiles shown, in sections: all umas by character (outfits side by side), or cards by
    /// rarity and type.
    fn setup_sections(&self, kind: GridKind) -> Vec<Section> {
        let o = &self.onboarding;
        let today = today();
        let q = o.search.trim().to_lowercase();
        match kind {
            GridKind::Umas => {
                let mut ids: Vec<u32> = self
                    .kb
                    .umas
                    .values()
                    .filter(|u| u.release.as_str() <= today.as_str())
                    .filter(|u| q.is_empty() || crate::ui_util::matches_query(&format!("{} {}", u.name, u.outfit), &q))
                    .filter(|u| !o.only_owned || self.owned_uma(u.id).is_some())
                    .map(|u| u.id)
                    .collect();
                let kb = &self.kb;
                if o.newest_first {
                    ids.sort_by(|a, b| kb.umas[b].release.cmp(&kb.umas[a].release).then(kb.umas[a].name.cmp(&kb.umas[b].name)).then(a.cmp(b)));
                } else {
                    ids.sort_by(|a, b| kb.umas[a].name.cmp(&kb.umas[b].name).then(a.cmp(b)));
                }
                vec![Section { title: String::new(), color: None, ids }]
            }
            GridKind::Cards => {
                let mut out = Vec::new();
                for rarity in ["SSR", "SR", "R"] {
                    for t in TYPES {
                        if o.kind.as_deref().is_some_and(|k| k != t) {
                            continue;
                        }
                        let mut ids: Vec<u32> = self
                            .kb
                            .cards
                            .values()
                            .filter(|c| c.rarity == rarity && c.kind == t && c.released(&today))
                            .filter(|c| o.show_r || c.rarity != "R" || self.owned_card(c.id).is_some())
                            .filter(|c| q.is_empty() || crate::ui_util::matches_query(&format!("{} {} {} {}", c.uma, c.title, c.kind, c.rarity), &q))
                            .filter(|c| !o.only_owned || self.owned_card(c.id).is_some())
                            .map(|c| c.id)
                            .collect();
                        if ids.is_empty() {
                            continue;
                        }
                        let kb = &self.kb;
                        if o.newest_first {
                            ids.sort_by(|a, b| kb.cards[b].release.cmp(&kb.cards[a].release).then(a.cmp(b)));
                        } else {
                            ids.sort_by(|a, b| kb.cards[a].uma.cmp(&kb.cards[b].uma).then(a.cmp(b)));
                        }
                        out.push(Section { title: format!("{rarity} · {} {t}", type_icon(t)), color: Some(type_color_str(t)), ids });
                    }
                }
                out
            }
        }
    }

    /// Whether the side editor has an owned tile to show.
    pub(crate) fn setup_selection_owned(&self, kind: GridKind) -> bool {
        self.onboarding.selected.is_some_and(|id| match kind {
            GridKind::Umas => self.owned_uma(id).is_some(),
            GridKind::Cards => self.owned_card(id).is_some(),
        })
    }

    /// The edit that adds or sets `id` with the brush's values (or the given stars/LB).
    fn brush_edit(&self, kind: GridKind, id: u32) -> Edit {
        let o = &self.onboarding;
        match kind {
            GridKind::Umas => Edit::Uma { id, stars: o.brush_stars, potential: o.brush_pot },
            GridKind::Cards => Edit::Card { id, lb: o.brush_lb, level: None },
        }
    }

    fn remove_edit(kind: GridKind, id: u32) -> Edit {
        match kind {
            GridKind::Umas => Edit::RemoveUma(id),
            GridKind::Cards => Edit::RemoveCard(id),
        }
    }

    /// The tiles a key or bulk action applies to: the picked ones, else the focused one.
    fn setup_targets(&self) -> Vec<u32> {
        if self.onboarding.multi.is_empty() { self.onboarding.focus.into_iter().collect() } else { self.onboarding.multi.iter().copied().collect() }
    }

    /// Set stars (or potential) on umas, adding any that aren't owned with the brush's other value.
    fn set_uma_values(&mut self, ids: &[u32], stars: Option<u8>, potential: Option<u8>) {
        let edits: Vec<Edit> = ids
            .iter()
            .map(|id| {
                let cur = self.owned_uma(*id).map_or((self.onboarding.brush_stars, self.onboarding.brush_pot), |u| (u.stars, u.potential));
                Edit::Uma { id: *id, stars: stars.unwrap_or(cur.0), potential: potential.unwrap_or(cur.1) }
            })
            .collect();
        self.edit_roster(&edits);
    }

    fn set_card_lb(&mut self, ids: &[u32], lb: u8) {
        let edits: Vec<Edit> = ids.iter().map(|id| Edit::Card { id: *id, lb, level: None }).collect();
        self.edit_roster(&edits);
    }

    /// The title, search, filters, the brush, Paste a list, and the bulk bar when tiles are picked.
    pub(crate) fn setup_toolbar(&mut self, ui: &mut egui::Ui, kind: GridKind) {
        let p = pal(ui);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let title = if kind == GridKind::Umas { "Which umas do you have?" } else { "Which support cards do you have?" };
            ui.label(semibold(title, size::LG));
            let how = if kind == GridKind::Umas {
                "Click one to add it. Hover an added one to set its stars and potential right on the tile."
            } else {
                "Click one to add it. Hover an added one and click a diamond to set its limit break."
            };
            ui.label(RichText::new(how).size(size::SM).color(p.weak)).on_hover_text(
                "Ctrl-click or Shift-click picks several for a bulk change. Keys: type to search (Enter adds the first match), \
                 arrows move, Space adds or removes, 1–5 sets stars (Shift for potential) or 0–4 sets LB, Delete removes, Ctrl+Z undoes.",
            );
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let id = egui::Id::unique("setup_search");
            let hint = if kind == GridKind::Umas { "🔍 Search name or outfit" } else { "🔍 Search uma or card title" };
            let search = ui.add(egui::TextEdit::singleline(&mut self.onboarding.search).id(id).hint_text(hint).desired_width(220.0));
            if std::mem::take(&mut self.onboarding.focus_search) || self.focus_search.take() {
                search.request_focus();
                // Keep typing where the first letter left off.
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                    let end = egui::text::CCursor::new(self.onboarding.search.chars().count());
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                    state.store(ui.ctx(), id);
                }
            }
            if search.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.add_first_match(kind);
            }
            ui.tick(&mut self.onboarding.only_owned, "Only mine");
            ui.tick(&mut self.onboarding.newest_first, "Newest first").on_hover_text("Recent releases first: new players own the recent banners");
            if kind == GridKind::Cards {
                ui.tick(&mut self.onboarding.show_r, "R cards").on_hover_text("R cards are rarely worth a deck slot, so they're hidden unless you have them");
            }
            ui.separator();
            ui.label(RichText::new("New picks start as").color(p.weak)).on_hover_text("Set this first, then one click adds each item with these values. Set it to MLB, click every maxed card, then set it back for the rest.");
            match kind {
                GridKind::Umas => {
                    egui::ComboBox::from_id_salt("brush_stars").width(56.0).selected_text(format!("{}★", self.onboarding.brush_stars)).show_ui(ui, |ui| {
                        for s in 1..=5u8 {
                            ui.selectable_value(&mut self.onboarding.brush_stars, s, format!("{s}★"));
                        }
                    });
                    egui::ComboBox::from_id_salt("brush_pot").width(56.0).selected_text(format!("P{}", self.onboarding.brush_pot)).show_ui(ui, |ui| {
                        for l in 1..=5u8 {
                            ui.selectable_value(&mut self.onboarding.brush_pot, l, format!("P{l}"));
                        }
                    });
                }
                GridKind::Cards => {
                    for lb in 0..=4u8 {
                        let on = self.onboarding.brush_lb == lb;
                        if lb_button(ui, lb, on).on_hover_text(format!("New cards start at LB{lb}{}", if lb == 4 { " (maxed, MLB)" } else { "" })).clicked() {
                            self.onboarding.brush_lb = lb;
                        }
                    }
                }
            }
            ui.separator();
            if ui.button("📋 Paste a list").on_hover_text("Type or paste what you own, one per line, like \"Kitasan Black MLB\"").clicked() {
                self.onboarding.paste.open = true;
            }
        });
        if kind == GridKind::Cards {
            ui.add_space(4.0);
            let mut options: Vec<(Option<String>, String, &str)> = vec![(None, "All types".into(), "")];
            options.extend(TYPES.iter().map(|t| (Some(t.to_string()), format!("{} {t}", type_icon(t)), "")));
            let mut kind_filter = self.onboarding.kind.clone();
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for (v, label, _) in &options {
                    if crate::ui_util::chip(ui, kind_filter == *v, label, None).clicked() && kind_filter != *v {
                        kind_filter = v.clone();
                        changed = true;
                    }
                }
            });
            if changed {
                self.onboarding.kind = kind_filter;
            }
        }
        if !self.onboarding.multi.is_empty() {
            ui.add_space(6.0);
            self.bulk_bar(ui, kind);
        }
    }

    /// "12 picked — set LB / stars / potential, remove, or clear."
    fn bulk_bar(&mut self, ui: &mut egui::Ui, kind: GridKind) {
        let p = pal(ui);
        let ids: Vec<u32> = self.onboarding.multi.iter().copied().collect();
        let owned = ids.iter().filter(|id| if kind == GridKind::Umas { self.owned_uma(**id).is_some() } else { self.owned_card(**id).is_some() }).count();
        egui::Frame::new()
            .fill(p.accent.gamma_multiply(if ui.visuals().dark_mode { 0.18 } else { 0.1 }))
            .stroke(Stroke::new(1.0, p.accent_strong))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(semibold(format!("{} picked", ids.len()), size::BODY));
                    ui.separator();
                    match kind {
                        GridKind::Cards => {
                            ui.label(RichText::new("Set limit break").color(p.weak));
                            for lb in 0..=4u8 {
                                if lb_button(ui, lb, false).on_hover_text(format!("Set all {} to LB{lb} (adds any not owned)", ids.len())).clicked() {
                                    self.set_card_lb(&ids, lb);
                                }
                            }
                        }
                        GridKind::Umas => {
                            ui.label(RichText::new("Stars").color(p.weak));
                            for s in 1..=5u8 {
                                if ui.small_btn(format!("{s}★")).clicked() {
                                    self.set_uma_values(&ids, Some(s), None);
                                }
                            }
                            ui.label(RichText::new("Potential").color(p.weak));
                            for l in 1..=5u8 {
                                if ui.small_btn(format!("P{l}")).clicked() {
                                    self.set_uma_values(&ids, None, Some(l));
                                }
                            }
                        }
                    }
                    ui.separator();
                    if ui.add_enabled(owned > 0, egui::Button::new(format!("✖ Remove {owned}"))).clicked() {
                        let edits: Vec<Edit> = ids.iter().map(|id| Self::remove_edit(kind, *id)).collect();
                        self.edit_roster(&edits);
                        self.toast_action(format!("Removed {owned}."), ToastKind::Info, "Undo", ToastAction::Undo);
                    }
                    if ui.button("Clear").on_hover_text("Unpick them all (Esc)").clicked() {
                        self.onboarding.multi.clear();
                    }
                });
            });
    }

    /// Enter in the search box: add the first match that isn't owned yet.
    fn add_first_match(&mut self, kind: GridKind) {
        let first = self.setup_sections(kind).into_iter().flat_map(|s| s.ids).find(|id| match kind {
            GridKind::Umas => self.owned_uma(*id).is_none(),
            GridKind::Cards => self.owned_card(*id).is_none(),
        });
        if let Some(id) = first {
            let e = self.brush_edit(kind, id);
            self.edit_roster(&[e]);
            self.onboarding.focus = Some(id);
            self.onboarding.scroll_to_focus = true;
            self.onboarding.search.clear();
            self.onboarding.focus_search = true;
        }
    }

    /// The grid itself.
    pub(crate) fn setup_grid(&mut self, ui: &mut egui::Ui, kind: GridKind) {
        let p = pal(ui);
        let sections = self.setup_sections(kind);
        let order: Vec<u32> = sections.iter().flat_map(|s| s.ids.iter().copied()).collect();
        ui.data_mut(|d| d.insert_temp(order_id(), order.clone()));
        if order.is_empty() {
            crate::ui_util::empty_state(ui, "🔍", "Nothing matches", "Clear the search or the filters above.");
            return;
        }
        let mut clicks: Vec<(u32, Hit, Modifiers)> = Vec::new();
        let mut menus: Vec<(u32, Menu)> = Vec::new();
        let mut add_all: Option<Vec<u32>> = None;
        let mut positions: Vec<(u32, Rect)> = Vec::new();
        let tile = if kind == GridKind::Umas { UMA_TILE } else { CARD_TILE };
        for section in &sections {
            if !section.title.is_empty() {
                let owned = section.ids.iter().filter(|id| self.owned_card(**id).is_some()).count();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if let Some(c) = section.color {
                        let (r, _) = ui.allocate_exact_size(vec2(4.0, 18.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(2), c);
                    }
                    ui.label(semibold(&section.title, size::MD));
                    ui.label(RichText::new(format!("{owned} of {} owned", section.ids.len())).color(p.weak));
                    let missing: Vec<u32> = section.ids.iter().copied().filter(|id| self.owned_card(*id).is_none()).collect();
                    if !missing.is_empty() {
                        let label = format!("＋ Add all {} at LB{}", missing.len(), self.onboarding.brush_lb);
                        if ui.small_btn(label).on_hover_text("Adds every card in this group you haven't added, at the brush's limit break. Ctrl+Z undoes it.").clicked() {
                            add_all = Some(missing);
                        }
                    }
                });
                ui.add_space(4.0);
            }
            let ids = &section.ids;
            tile_grid(ui, ids.len(), tile, 10.0, 10.0, |ui, k| {
                let id = ids[k];
                let (resp, hit, menu) = match kind {
                    GridKind::Umas => self.setup_uma_tile(ui, id),
                    GridKind::Cards => self.setup_card_tile(ui, id),
                };
                positions.push((id, resp.rect));
                if self.onboarding.scroll_to_focus && self.onboarding.focus == Some(id) {
                    ui.scroll_to_rect(resp.rect, Some(egui::Align::Center));
                }
                if let Some(h) = hit {
                    clicks.push((id, h, ui.input(|i| i.modifiers)));
                }
                if let Some(m) = menu {
                    menus.push((id, m));
                }
            });
        }
        self.onboarding.scroll_to_focus = false;
        ui.data_mut(|d| d.insert_temp(positions_id(), positions));
        if let Some(ids) = add_all {
            let edits: Vec<Edit> = ids.iter().map(|id| self.brush_edit(kind, *id)).collect();
            let n = edits.len();
            self.edit_roster(&edits);
            self.toast_action(format!("Added {n} cards at LB{}.", self.onboarding.brush_lb), ToastKind::Success, "Undo", ToastAction::Undo);
        }
        for (id, hit, mods) in clicks {
            self.tile_clicked(kind, id, hit, mods, &order);
        }
        for (id, m) in menus {
            match m {
                Menu::Add => {
                    let e = self.brush_edit(kind, id);
                    self.edit_roster(&[e]);
                }
                Menu::Remove => self.edit_roster(&[Self::remove_edit(kind, id)]),
                Menu::Edit => self.onboarding.selected = Some(id),
                Menu::Details => {
                    self.jump.set(Some(if kind == GridKind::Umas { Jump::Uma(id) } else { Jump::Card(id) }));
                    self.close_setup();
                }
            }
        }
    }

    /// A click on a tile: on its stars, potential or diamonds, or on the tile itself.
    fn tile_clicked(&mut self, kind: GridKind, id: u32, hit: Hit, mods: Modifiers, order: &[u32]) {
        let owned = match kind {
            GridKind::Umas => self.owned_uma(id).is_some(),
            GridKind::Cards => self.owned_card(id).is_some(),
        };
        self.onboarding.focus = Some(id);
        match hit {
            Hit::Star(s) => self.set_uma_values(&[id], Some(s), None),
            Hit::Pot(l) => self.set_uma_values(&[id], None, Some(l)),
            Hit::Lb(k) => {
                // Clicking the top lit diamond again takes one off (the first one: back to LB0).
                let cur = self.owned_card(id).map_or(0, |c| c.lb);
                self.set_card_lb(&[id], if cur == k { k - 1 } else { k });
            }
            Hit::Body if mods.command => {
                if !self.onboarding.multi.remove(&id) {
                    self.onboarding.multi.insert(id);
                }
                self.onboarding.anchor = Some(id);
            }
            Hit::Body if mods.shift => {
                let from = self.onboarding.anchor.or(self.onboarding.selected).unwrap_or(id);
                let (a, b) = (order.iter().position(|x| *x == from).unwrap_or(0), order.iter().position(|x| *x == id).unwrap_or(0));
                self.onboarding.multi.extend(order[a.min(b)..=a.max(b)].iter().copied());
            }
            Hit::Body => {
                self.onboarding.multi.clear();
                self.onboarding.anchor = Some(id);
                if owned {
                    self.onboarding.selected = (self.onboarding.selected != Some(id)).then_some(id);
                } else {
                    let e = self.brush_edit(kind, id);
                    self.edit_roster(&[e]);
                }
            }
        }
    }

    /// An uma's tile. Added: full colour, ✔, stars and potential; hovered, a strip to set them.
    /// Not added: faded, with ＋ on hover.
    fn setup_uma_tile(&self, ui: &mut egui::Ui, id: u32) -> (egui::Response, Option<Hit>, Option<Menu>) {
        let p = pal(ui);
        let info = &self.kb.umas[&id];
        let owned = self.owned_uma(id).map(|u| (u.stars, u.potential));
        let (rect, resp) = ui.allocate_exact_size(UMA_TILE, Sense::click());
        let img = Rect::from_min_size(rect.min, vec2(ICON, ICON));
        let band = Rect::from_min_max(egui::pos2(img.left(), img.bottom() - 34.0), img.right_bottom());
        let step = 14.0;
        let star_x0 = img.center().x - 2.5 * step;
        let mut hit = None;
        if resp.clicked() {
            hit = Some(match (owned, resp.interact_pointer_pos()) {
                (Some(_), Some(pos)) if band.contains(pos) && pos.y < band.top() + 18.0 => Hit::Star((((pos.x - star_x0) / step).floor().clamp(0.0, 4.0)) as u8 + 1),
                (Some(_), Some(pos)) if band.contains(pos) => Hit::Pot((((pos.x - img.left()) / (ICON / 5.0)).floor().clamp(0.0, 4.0)) as u8 + 1),
                _ => Hit::Body,
            });
        }
        if ui.is_rect_visible(rect) {
            let hovered = resp.hovered() || self.debug_hover == Some(id);
            let painter = ui.painter();
            let t = ui.ctx().animate_bool_with_time(resp.id.with("owned"), owned.is_some(), 0.15);
            if hovered {
                painter.add(egui::Shadow { offset: [0, 3], blur: 12, spread: 0, color: Color32::from_black_alpha(70) }.as_shape(img, CornerRadius::same(10)));
            }
            let alpha = (90.0 + 165.0 * t) as u8;
            match self.art_uri("umas", id) {
                Some(uri) => crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same(10)).tint(Color32::from_white_alpha(alpha)), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))),
                None => placeholder(ui.painter(), img, Color32::from_gray(70), &info.name),
            }
            let painter = ui.painter();
            match owned {
                Some((stars, pot)) => {
                    check_badge(painter, img.left_top() + vec2(12.0, 12.0), p.good);
                    if hovered {
                        painter.rect_filled(band, CornerRadius { nw: 0, ne: 0, sw: 10, se: 10 }, Color32::from_black_alpha(175));
                        for k in 0..5u8 {
                            let c = egui::pos2(star_x0 + step * (f32::from(k) + 0.5), band.top() + 10.0);
                            draw_stars_one(painter, c, 5.5, k < stars);
                        }
                        let w = ICON / 5.0;
                        for k in 0..5u8 {
                            let r = Rect::from_min_size(egui::pos2(img.left() + w * f32::from(k) + 1.0, band.top() + 19.0), vec2(w - 2.0, 13.0));
                            if k + 1 == pot {
                                painter.rect_filled(r, CornerRadius::same(3), p.accent);
                            }
                            painter.text(r.center(), Align2::CENTER_CENTER, format!("P{}", k + 1), FontId::proportional(9.5), if k + 1 == pot { p.on_accent } else { Color32::from_gray(225) });
                        }
                    } else {
                        crate::ui_util::potential_badge(painter, img.right_bottom() - vec2(4.0, 4.0), pot);
                    }
                    draw_stars(painter, egui::pos2(img.center().x, img.bottom() + 9.0), stars, 5.0);
                }
                None if hovered => {
                    painter.circle_filled(img.center(), 17.0, p.accent.gamma_multiply(0.9));
                    painter.text(img.center(), Align2::CENTER_CENTER, "＋", FontId::proportional(20.0), p.on_accent);
                    let o = &self.onboarding;
                    painter.text(egui::pos2(img.center().x, img.bottom() + 9.0), Align2::CENTER_CENTER, format!("adds {}★ P{}", o.brush_stars, o.brush_pot), FontId::proportional(10.5), p.weak);
                }
                None => {}
            }
            let color = if owned.is_some() { p.text } else { p.weak };
            painter.text(egui::pos2(img.center().x, img.bottom() + 23.0), Align2::CENTER_CENTER, truncate(&info.name, 14), FontId::proportional(12.0), color);
            painter.text(egui::pos2(img.center().x, img.bottom() + 37.0), Align2::CENTER_CENTER, truncate(&info.outfit, 16), FontId::proportional(10.5), p.weak);
            self.tile_marks(ui, img, id, 12);
        }
        let menu = tile_menu(&resp, owned.is_some(), "Set stars and potential…");
        let action = if owned.is_some() { "Click: details on the right · stars and P on the tile set them" } else { "Click to add" };
        (resp.on_hover_text(format!("{} [{}]\n{action}\nRight-click for more", info.name, info.outfit)), hit, menu)
    }

    /// A card's tile. Added: full colour, ✔ and LB diamonds; hovered, diamonds to click.
    fn setup_card_tile(&self, ui: &mut egui::Ui, id: u32) -> (egui::Response, Option<Hit>, Option<Menu>) {
        let p = pal(ui);
        let info = &self.kb.cards[&id];
        let owned = self.owned_card(id).map(|c| c.lb);
        let (rect, resp) = ui.allocate_exact_size(CARD_TILE, Sense::click());
        let img = Rect::from_min_size(rect.min, vec2(CARD_W, CARD_H));
        let band = Rect::from_min_max(egui::pos2(img.left(), img.bottom() - 24.0), img.right_bottom());
        let slot = (CARD_W - 8.0) / 4.0;
        let mut hit = None;
        if resp.clicked() {
            hit = Some(match (owned, resp.interact_pointer_pos()) {
                (Some(_), Some(pos)) if band.contains(pos) => Hit::Lb((((pos.x - img.left() - 4.0) / slot).floor().clamp(0.0, 3.0)) as u8 + 1),
                _ => Hit::Body,
            });
        }
        if ui.is_rect_visible(rect) {
            let hovered = resp.hovered() || self.debug_hover == Some(id);
            let t = ui.ctx().animate_bool_with_time(resp.id.with("owned"), owned.is_some(), 0.15);
            if hovered {
                ui.painter().add(egui::Shadow { offset: [0, 3], blur: 12, spread: 0, color: Color32::from_black_alpha(70) }.as_shape(img, CornerRadius::same(6)));
            }
            let alpha = (90.0 + 165.0 * t) as u8;
            match self.art_uri("cards", id) {
                Some(uri) => crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same(6)).tint(Color32::from_white_alpha(alpha)), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))),
                None => placeholder(ui.painter(), img, type_color_str(&info.kind), &info.uma),
            }
            let painter = ui.painter();
            match owned {
                Some(lb) => {
                    check_badge(painter, img.left_top() + vec2(11.0, 11.0), p.good);
                    if hovered {
                        painter.rect_filled(band, CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 }, Color32::from_black_alpha(175));
                        draw_lb(painter, egui::pos2(img.left() + 4.0 + slot / 2.0 - 6.5, band.center().y), lb, 6.5);
                    }
                    draw_lb(painter, egui::pos2(rect.left() + 2.0, img.bottom() + 9.0), lb, 5.0);
                }
                None if hovered => {
                    painter.circle_filled(img.center(), 17.0, p.accent.gamma_multiply(0.9));
                    painter.text(img.center(), Align2::CENTER_CENTER, "＋", FontId::proportional(20.0), p.on_accent);
                    painter.text(egui::pos2(img.center().x, img.bottom() + 9.0), Align2::CENTER_CENTER, format!("adds LB{}", self.onboarding.brush_lb), FontId::proportional(10.5), p.weak);
                }
                None => {}
            }
            let color = if owned.is_some() { p.text } else { p.weak };
            painter.text(egui::pos2(img.center().x, img.bottom() + 24.0), Align2::CENTER_CENTER, truncate(&info.uma, 13), FontId::proportional(11.5), color);
            self.tile_marks(ui, img, id, 8);
        }
        let menu = tile_menu(&resp, owned.is_some(), "Set limit break and level…");
        let action = if owned.is_some() { "Click: details on the right · click a diamond on the tile to set the LB" } else { "Click to add" };
        (resp.on_hover_text(format!("{} {} [{}]\n{} card\n{action}\nRight-click for more", info.rarity, info.uma, info.title, info.kind)), hit, menu)
    }

    /// Rings for the selected, picked and keyboard-focused tile.
    fn tile_marks(&self, ui: &egui::Ui, img: Rect, id: u32, radius: u8) {
        let p = pal(ui);
        let painter = ui.painter();
        let o = &self.onboarding;
        if o.multi.contains(&id) {
            painter.rect_stroke(img.expand(2.5), CornerRadius::same(radius), Stroke::new(2.5, p.accent), StrokeKind::Outside);
            let r = Rect::from_center_size(img.right_top() + vec2(-11.0, 11.0), vec2(16.0, 16.0));
            painter.rect_filled(r, CornerRadius::same(4), p.accent);
            painter.text(r.center(), Align2::CENTER_CENTER, "✔", FontId::proportional(11.0), p.on_accent);
        } else if o.selected == Some(id) {
            painter.rect_stroke(img.expand(2.5), CornerRadius::same(radius), Stroke::new(2.5, p.accent), StrokeKind::Outside);
        }
        if o.focus == Some(id) {
            painter.rect_stroke(img.expand(5.5), CornerRadius::same(radius + 2), Stroke::new(1.5, p.accent_strong), StrokeKind::Outside);
        }
    }

    /// The selected tile's editor, docked on the right so it never scrolls away.
    pub(crate) fn setup_side_editor(&mut self, ui: &mut egui::Ui, kind: GridKind) {
        let p = pal(ui);
        let Some(id) = self.onboarding.selected else { return };
        ui.horizontal(|ui| {
            ui.label(semibold("Edit", size::LG));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new("✕").frame(false)).on_hover_text("Close (Esc)").clicked() {
                    self.onboarding.selected = None;
                }
            });
        });
        ui.add_space(6.0);
        match kind {
            GridKind::Umas => {
                let Some(u) = self.owned_uma(id).cloned() else { return };
                ui.horizontal(|ui| {
                    if let Some(uri) = self.art_uri("umas", id) {
                        ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(64.0, 64.0)).corner_radius(CornerRadius::same(10)));
                    }
                    ui.vertical(|ui| {
                        ui.label(semibold(&u.name, size::MD));
                        ui.label(RichText::new(&u.outfit).color(p.weak));
                    });
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Stars").color(p.weak));
                ui.horizontal_wrapped(|ui| {
                    for s in 1..=5u8 {
                        if ui.add(egui::Button::selectable(u.stars == s, format!("{s}★")).min_size(vec2(40.0, 30.0))).clicked() {
                            self.set_uma_values(&[id], Some(s), None);
                        }
                    }
                });
                ui.label(RichText::new("Potential").color(p.weak)).on_hover_text("Each level unlocks more of her own skills. P5 is maxed.");
                ui.horizontal_wrapped(|ui| {
                    for l in 1..=5u8 {
                        if ui.add(egui::Button::selectable(u.potential == l, format!("P{l}")).min_size(vec2(40.0, 30.0))).clicked() {
                            self.set_uma_values(&[id], None, Some(l));
                        }
                    }
                });
            }
            GridKind::Cards => {
                let Some(c) = self.owned_card(id).cloned() else { return };
                ui.horizontal(|ui| {
                    if let Some(uri) = self.art_uri("cards", id) {
                        ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(54.0, 72.0)).corner_radius(CornerRadius::same(6)));
                    }
                    ui.vertical(|ui| {
                        ui.label(semibold(format!("{} {}", c.rarity.label(), c.uma), size::MD));
                        ui.add(egui::Label::new(RichText::new(&c.title).color(p.weak)).wrap());
                    });
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Limit break").color(p.weak)).on_hover_text("The diamonds under the card in the game. All four is maxed (MLB).");
                ui.horizontal_wrapped(|ui| {
                    for lb in 0..=4u8 {
                        if lb_button(ui, lb, c.lb == lb).clicked() && c.lb != lb {
                            self.set_card_lb(&[id], lb);
                        }
                    }
                });
                let cap = c.rarity.max_level(c.lb);
                ui.add_space(4.0);
                ui.label(RichText::new(format!("Level (up to {cap} at LB{})", c.lb)).color(p.weak));
                let mut level = c.level;
                let r = ui.add(egui::Slider::new(&mut level, 1..=cap));
                if (r.drag_stopped() || (r.changed() && !r.dragged())) && level != c.level {
                    self.edit_roster(&[Edit::Card { id, lb: c.lb, level: Some(level) }]);
                }
            }
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button("✖ Remove").clicked() {
                self.edit_roster(&[Self::remove_edit(kind, id)]);
                self.onboarding.selected = None;
            }
            if ui.button("🔍 Open details").on_hover_text("See it in Collection (closes setup)").clicked() {
                self.jump.set(Some(if kind == GridKind::Umas { Jump::Uma(id) } else { Jump::Card(id) }));
                self.close_setup();
            }
        });
    }

    /// Keys on the grid: letters search, arrows move, Space adds or removes, digits set stars, potential
    /// or LB, Delete removes, Enter adds or opens, Ctrl+Z undoes.
    pub(crate) fn setup_keys(&mut self, ctx: &egui::Context, kind: GridKind) {
        if self.onboarding.paste.open || self.settings.open || self.help_open || self.onboarding.confirm_start_over {
            return;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::Z))) {
            self.undo_setup();
            return;
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let order: Vec<u32> = ctx.data(|d| d.get_temp(order_id())).unwrap_or_default();
        let positions: Vec<(u32, Rect)> = ctx.data(|d| d.get_temp(positions_id())).unwrap_or_default();
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            match e {
                egui::Event::Text(t) if !t.is_empty() && t.chars().all(char::is_alphabetic) => {
                    self.onboarding.search.push_str(&t);
                    self.onboarding.focus_search = true;
                }
                egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } if !modifiers.command && !modifiers.alt => {
                    let k = physical_key.unwrap_or(key);
                    self.grid_key(kind, k, modifiers.shift, &order, &positions);
                }
                _ => {}
            }
        }
    }

    fn grid_key(&mut self, kind: GridKind, k: Key, shift: bool, order: &[u32], positions: &[(u32, Rect)]) {
        let digit = [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5].iter().position(|d| *d == k).map(|d| d as u8);
        let owned = |a: &App, id: u32| match kind {
            GridKind::Umas => a.owned_uma(id).is_some(),
            GridKind::Cards => a.owned_card(id).is_some(),
        };
        match k {
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                self.onboarding.focus = move_focus(self.onboarding.focus, k, order, positions);
                self.onboarding.scroll_to_focus = true;
            }
            Key::Space | Key::Enter => {
                let Some(id) = self.onboarding.focus else { return };
                if !owned(self, id) {
                    let e = self.brush_edit(kind, id);
                    self.edit_roster(&[e]);
                } else if k == Key::Space {
                    self.edit_roster(&[Self::remove_edit(kind, id)]);
                } else {
                    self.onboarding.selected = Some(id);
                }
            }
            Key::Delete | Key::Backspace => {
                let ids: Vec<u32> = self.setup_targets().into_iter().filter(|id| owned(self, *id)).collect();
                let edits: Vec<Edit> = ids.iter().map(|id| Self::remove_edit(kind, *id)).collect();
                self.edit_roster(&edits);
            }
            _ => {
                let Some(d) = digit else { return };
                let ids = self.setup_targets();
                if ids.is_empty() {
                    return;
                }
                match kind {
                    GridKind::Cards if d <= 4 => self.set_card_lb(&ids, d),
                    GridKind::Umas if (1..=5).contains(&d) && shift => self.set_uma_values(&ids, None, Some(d)),
                    GridKind::Umas if (1..=5).contains(&d) => self.set_uma_values(&ids, Some(d), None),
                    _ => {}
                }
            }
        }
    }
}

/// The next tile for an arrow key: along the order for left and right; for up and down, the
/// nearest tile in the row above or below.
fn move_focus(focus: Option<u32>, k: Key, order: &[u32], positions: &[(u32, Rect)]) -> Option<u32> {
    let Some(cur) = focus.filter(|f| order.contains(f)) else { return order.first().copied() };
    let i = order.iter().position(|x| *x == cur)?;
    match k {
        Key::ArrowLeft => Some(order[i.saturating_sub(1)]),
        Key::ArrowRight => Some(order[(i + 1).min(order.len() - 1)]),
        _ => {
            let here = positions.iter().find(|(id, _)| *id == cur)?.1.center();
            let down = k == Key::ArrowDown;
            let row: Vec<&(u32, Rect)> = positions.iter().filter(|(_, r)| if down { r.center().y > here.y + 4.0 } else { r.center().y < here.y - 4.0 }).collect();
            let dy = row.iter().map(|(_, r)| (r.center().y - here.y).abs()).fold(f32::INFINITY, f32::min);
            row.into_iter()
                .filter(|(_, r)| ((r.center().y - here.y).abs() - dy).abs() < 4.0)
                .min_by(|a, b| (a.1.center().x - here.x).abs().total_cmp(&(b.1.center().x - here.x).abs()))
                .map(|(id, _)| *id)
                .or(Some(cur))
        }
    }
}

/// A limit-break choice as diamonds: ◇◇◇◇ to ◆◆◆◆ (MLB).
pub(crate) fn lb_button(ui: &mut egui::Ui, lb: u8, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(if lb == 4 { 82.0 } else { 54.0 }, 24.0), Sense::click());
    let p = pal(ui);
    let painter = ui.painter();
    let fill = if on {
        p.accent.gamma_multiply(if ui.visuals().dark_mode { 0.3 } else { 0.18 })
    } else if resp.hovered() {
        p.hover
    } else {
        p.raised
    };
    painter.rect_filled(rect, CornerRadius::same(6), fill);
    painter.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, if on { p.accent_strong } else { p.border }), StrokeKind::Inside);
    draw_lb(painter, egui::pos2(rect.left() + 6.0, rect.center().y), lb, 4.5);
    if lb == 4 {
        painter.text(egui::pos2(rect.right() - 6.0, rect.center().y), Align2::RIGHT_CENTER, "MLB", FontId::proportional(11.0), p.text);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A round ✔ on an added tile.
fn check_badge(painter: &egui::Painter, at: Pos2, color: Color32) {
    painter.circle_filled(at, 9.0, color);
    painter.text(at, Align2::CENTER_CENTER, "✔", FontId::proportional(11.0), Color32::WHITE);
}

/// One star, gold or dim.
fn draw_stars_one(painter: &egui::Painter, c: Pos2, r: f32, on: bool) {
    // `draw_stars` centres five; draw one by drawing a single lit or unlit star.
    crate::ui_util::draw_star_at(painter, c, r, if on { crate::ui_util::GOLD } else { Color32::from_gray(110) });
}

/// A tile's right-click menu.
fn tile_menu(resp: &egui::Response, owned: bool, edit: &str) -> Option<Menu> {
    let mut picked = None;
    resp.context_menu(|ui| {
        if owned {
            if ui.button(format!("✏ {edit}")).clicked() {
                picked = Some(Menu::Edit);
                ui.close();
            }
            if ui.button("✖ Remove").clicked() {
                picked = Some(Menu::Remove);
                ui.close();
            }
        } else if ui.button("＋ Add").clicked() {
            picked = Some(Menu::Add);
            ui.close();
        }
        if ui.button("🔍 Open details").clicked() {
            picked = Some(Menu::Details);
            ui.close();
        }
    });
    picked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onboarding::Step;

    fn frame(a: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))), events, ..Default::default() };
        let _ = crate::ui_util::test_frame(&ctx, input, |ui| a.frame(ui));
    }

    fn key(k: Key, shift: bool) -> egui::Event {
        egui::Event::Key { key: k, physical_key: Some(k), pressed: true, repeat: false, modifiers: Modifiers { shift, ..Default::default() } }
    }

    /// With the brush at MLB, a click adds a card at LB4; digits set the LB of the focused card,
    /// stars and Shift+potential of an uma; Delete removes; Ctrl+Z undoes.
    #[test]
    fn brush_and_keys() {
        let (mut a, dir) = crate::onboarding::tests::scratch_app("keys");
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, crate::theme::Mode::Dark);
        a.open_setup(Step::Cards);
        a.onboarding.brush_lb = 4;
        a.tile_clicked(GridKind::Cards, 30076, Hit::Body, Modifiers::default(), &[30076]);
        assert_eq!(a.owned_card(30076).map(|c| (c.lb, c.level)), Some((4, 50)), "added at the brush's MLB");
        a.tile_clicked(GridKind::Cards, 30076, Hit::Lb(4), Modifiers::default(), &[30076]);
        assert_eq!(a.owned_card(30076).map(|c| c.lb), Some(3), "clicking the top lit diamond takes one off");
        frame(&mut a, &ctx, vec![]);
        a.onboarding.focus = Some(30076);
        frame(&mut a, &ctx, vec![key(Key::Num2, false)]);
        assert_eq!(a.owned_card(30076).map(|c| c.lb), Some(2), "2 sets LB2");
        frame(&mut a, &ctx, vec![key(Key::Delete, false)]);
        assert!(a.owned_card(30076).is_none(), "Delete removes it");
        frame(&mut a, &ctx, vec![egui::Event::Key { key: Key::Z, physical_key: Some(Key::Z), pressed: true, repeat: false, modifiers: Modifiers::COMMAND }]);
        assert_eq!(a.owned_card(30076).map(|c| c.lb), Some(2), "Ctrl+Z brings it back");

        a.open_setup(Step::Umas);
        a.onboarding.brush_stars = 4;
        a.onboarding.brush_pot = 2;
        a.tile_clicked(GridKind::Umas, 100601, Hit::Body, Modifiers::default(), &[100601]);
        assert_eq!(a.owned_uma(100601).map(|u| (u.stars, u.potential)), Some((4, 2)));
        frame(&mut a, &ctx, vec![]);
        a.onboarding.focus = Some(100601);
        frame(&mut a, &ctx, vec![key(Key::Num5, false)]);
        frame(&mut a, &ctx, vec![key(Key::Num3, true)]);
        assert_eq!(a.owned_uma(100601).map(|u| (u.stars, u.potential)), Some((5, 3)), "5 sets stars, Shift+3 potential");
        a.tile_clicked(GridKind::Umas, 100601, Hit::Pot(5), Modifiers::default(), &[100601]);
        assert_eq!(a.owned_uma(100601).map(|u| u.potential), Some(5), "P5 on the tile");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Shift-click picks a range, and a bulk LB adds them all in one undoable action.
    #[test]
    fn multi_select_and_bulk() {
        let (mut a, dir) = crate::onboarding::tests::scratch_app("bulk");
        a.open_setup(Step::Cards);
        let order: Vec<u32> = a.setup_sections(GridKind::Cards).into_iter().flat_map(|s| s.ids).take(6).collect();
        a.tile_clicked(GridKind::Cards, order[1], Hit::Body, Modifiers::COMMAND, &order);
        a.tile_clicked(GridKind::Cards, order[4], Hit::Body, Modifiers { shift: true, ..Default::default() }, &order);
        assert_eq!(a.onboarding.multi.len(), 4, "items 1 to 4");
        let ids: Vec<u32> = a.onboarding.multi.iter().copied().collect();
        a.set_card_lb(&ids, 4);
        assert!(ids.iter().all(|id| a.owned_card(*id).is_some_and(|c| c.lb == 4)));
        a.undo_setup();
        assert!(ids.iter().all(|id| a.owned_card(*id).is_none()), "one undo takes the whole bulk change back");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The goal: 50 cards with the right LBs in a few minutes. Count the clicks a new user needs
    /// with the brush: set MLB, click the maxed ones, set LB0, click the rest, then one diamond
    /// click for each card in between.
    #[test]
    fn fifty_cards_take_few_clicks() {
        let (mut a, dir) = crate::onboarding::tests::scratch_app("fifty");
        a.open_setup(Step::Cards);
        let order: Vec<u32> = a.setup_sections(GridKind::Cards).into_iter().flat_map(|s| s.ids).take(50).collect();
        let want: Vec<(u32, u8)> = order.iter().enumerate().map(|(i, id)| (*id, [4, 4, 0, 2, 4][i % 5])).collect();
        let mut clicks = 0;
        for brush in [4u8, 0] {
            a.onboarding.brush_lb = brush;
            clicks += 1;
            for (id, lb) in &want {
                if (*lb == 4) == (brush == 4) {
                    a.tile_clicked(GridKind::Cards, *id, Hit::Body, Modifiers::default(), &order);
                    clicks += 1;
                }
            }
        }
        for (id, lb) in &want {
            if *lb != 4 && *lb != 0 {
                a.tile_clicked(GridKind::Cards, *id, Hit::Lb(*lb), Modifiers::default(), &order);
                clicks += 1;
            }
        }
        for (id, lb) in &want {
            assert_eq!(a.owned_card(*id).map(|c| c.lb), Some(*lb));
        }
        eprintln!("50 cards in {clicks} clicks");
        // About two seconds a click (find the card, click) is under two minutes.
        assert!(clicks <= 62, "{clicks}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
