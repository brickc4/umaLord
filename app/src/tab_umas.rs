//! Umas: owned trainees with aptitudes, skills, evolutions, objectives and events.

use std::collections::{BTreeMap, BTreeSet};

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, pos2, vec2};
use egui_extras::Column;
use serde::{Deserialize, Serialize};

use crate::collection_kit::{self as kit, SortOption, TileGroup};
use crate::ui_util::UiExt;
use crate::data::Uma;
use crate::knowledge::{Aptitude, grade_rank};
use crate::theme::{pal, size};
use crate::ui_util::{
    DataTable, FilterBarAction, FilterSection, SortKey, TableCol, active_filter_bar, badge, card, card_explained, card_titled, draw_stars,
    empty_state, grade_badge, grade_color, grade_tile, placeholder, potential_badge, section_label, semibold, sort_rows, stars_widget, tile_ring,
    truncate, web_link,
};
use crate::{App, Jump, View};

const ICON: f32 = 96.0;

/// One aptitude: a surface, a distance or a running style.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Apt {
    Turf,
    Dirt,
    Sprint,
    Mile,
    Medium,
    Long,
    Front,
    Pace,
    Late,
    End,
}

impl Apt {
    pub const ALL: [Apt; 10] = [Apt::Turf, Apt::Dirt, Apt::Sprint, Apt::Mile, Apt::Medium, Apt::Long, Apt::Front, Apt::Pace, Apt::Late, Apt::End];

    /// The table column and sort key.
    pub fn key(self) -> &'static str {
        match self {
            Apt::Turf => "turf",
            Apt::Dirt => "dirt",
            Apt::Sprint => "sprint",
            Apt::Mile => "mile",
            Apt::Medium => "medium",
            Apt::Long => "long",
            Apt::Front => "front",
            Apt::Pace => "pace",
            Apt::Late => "late",
            Apt::End => "end",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Apt::Turf => "Turf",
            Apt::Dirt => "Dirt",
            Apt::Sprint => "Spr",
            Apt::Mile => "Mile",
            Apt::Medium => "Med",
            Apt::Long => "Long",
            Apt::Front => "Front",
            Apt::Pace => "Pace",
            Apt::Late => "Late",
            Apt::End => "End",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Apt::Turf => "Turf",
            Apt::Dirt => "Dirt",
            Apt::Sprint => "Sprint",
            Apt::Mile => "Mile",
            Apt::Medium => "Medium",
            Apt::Long => "Long",
            Apt::Front => "Front Runner",
            Apt::Pace => "Pace Chaser",
            Apt::Late => "Late Surger",
            Apt::End => "End Closer",
        }
    }

    pub fn group(self) -> &'static str {
        match self {
            Apt::Turf | Apt::Dirt => "Surface",
            Apt::Sprint | Apt::Mile | Apt::Medium | Apt::Long => "Distance",
            _ => "Style",
        }
    }

    pub fn grade(self, a: &Aptitude) -> &str {
        match self {
            Apt::Turf => &a.turf,
            Apt::Dirt => &a.dirt,
            Apt::Sprint => &a.sprint,
            Apt::Mile => &a.mile,
            Apt::Medium => &a.medium,
            Apt::Long => &a.long,
            Apt::Front => &a.front,
            Apt::Pace => &a.pace,
            Apt::Late => &a.late,
            Apt::End => &a.end,
        }
    }
}

/// The filters on the Umas sub-tab. Saved views store this.
#[derive(Clone, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct UmaFilter {
    pub stars: BTreeSet<u8>,
    pub potential: BTreeSet<u8>,
    /// The lowest grade rank allowed per aptitude (7 = A, 8 = S).
    pub apt: BTreeMap<Apt, u8>,
    pub evolved: bool,
    pub recent: bool,
}

impl UmaFilter {
    pub fn count(&self) -> usize {
        self.stars.len() + self.potential.len() + self.apt.len() + usize::from(self.evolved) + usize::from(self.recent)
    }
}

pub(crate) struct UmasState {
    pub search: String,
    pub filter: UmaFilter,
    pub sort: crate::ui_util::TableSort,
    pub selected: Option<u32>,
}

impl Default for UmasState {
    fn default() -> Self {
        Self { search: String::new(), filter: UmaFilter::default(), sort: crate::ui_util::TableSort { key: "stars", desc: true }, selected: None }
    }
}

/// A filter that's on, as the active-filter bar lists it.
#[derive(Clone, Copy)]
enum UmaChip {
    Search,
    Stars(u8),
    Potential(u8),
    Apt(Apt),
    Evolved,
    Recent,
}

const GROWTH: [(&str, &str, &str); 5] =
    [("g_spd", "Spd %", "Speed"), ("g_sta", "Sta %", "Stamina"), ("g_pow", "Pow %", "Power"), ("g_gut", "Gut %", "Guts"), ("g_wit", "Wit %", "Wit")];

/// The Umas sort dropdown: every sortable column, plus "Best fit".
const UMA_SORTS: [SortOption; 22] = [
    ("stars", "Stars", true),
    ("potential", "Potential", true),
    ("name", "Name", false),
    ("outfit", "Outfit", false),
    ("release", "Release date", true),
    ("fit", "Best fit for the aptitude filter", true),
    ("turf", "Turf aptitude", true),
    ("dirt", "Dirt aptitude", true),
    ("sprint", "Sprint aptitude", true),
    ("mile", "Mile aptitude", true),
    ("medium", "Medium aptitude", true),
    ("long", "Long aptitude", true),
    ("front", "Front Runner aptitude", true),
    ("pace", "Pace Chaser aptitude", true),
    ("late", "Late Surger aptitude", true),
    ("end", "End Closer aptitude", true),
    ("g_spd", "Speed growth", true),
    ("g_sta", "Stamina growth", true),
    ("g_pow", "Power growth", true),
    ("g_gut", "Guts growth", true),
    ("g_wit", "Wit growth", true),
    ("unique", "Unique skill", false),
];

/// The width of every Collection side panel, so the lists line up between sub-tabs.
pub(crate) const FILTER_W: f32 = 240.0;

/// A filter heading in a side panel.
pub(crate) fn filter_heading(ui: &mut egui::Ui, text: &str) {
    section_label(ui, text, "");
}

/// "Any", "A+" or "S" for a minimum grade rank.
fn min_label(min: Option<u8>) -> &'static str {
    match min {
        Some(8) => "S",
        Some(_) => "A+",
        None => "any",
    }
}

impl App {
    pub(crate) fn umas_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut collapsed = kit::rail_collapsed(&ctx, "umas");
        if collapsed && self.focus_search.get() {
            collapsed = false;
            kit::set_rail(&ctx, "umas", false);
        }
        // A narrow window with the details open, or with a table: fold the filters so the list
        // keeps its room.
        let table_fold = self.view == View::Table && !kit::opened_by_hand(&ctx, "umas");
        let auto_fold = !collapsed && ui.available_width() < 1100.0 && !self.focus_search.get() && (self.umas.selected.is_some() || table_fold);
        collapsed |= auto_fold;
        let active = self.umas.filter.count() + usize::from(!self.umas.search.trim().is_empty());
        if collapsed {
            egui::Panel::left("uma_filters_rail").resizable(false).exact_size(kit::RAIL_W).frame(kit::rail_frame()).show(ui, |ui| {
                if kit::rail(ui, active, &[("🔍", "Search"), ("★", "Stars"), ("P", "Potential"), ("A", "Aptitudes")]) {
                    if auto_fold {
                        self.umas.selected = None;
                        kit::open_by_hand(&ctx, "umas");
                    } else {
                        kit::set_rail(&ctx, "umas", false);
                    }
                }
            });
        } else {
            egui::Panel::left("uma_filters").resizable(false).exact_size(FILTER_W).frame(crate::side_frame()).show(ui, |ui| {
                if kit::sidebar_head(ui, active) {
                    kit::set_rail(&ctx, "umas", true);
                }
                egui::ScrollArea::vertical().show(ui, |ui| self.uma_filters(ui));
            });
        }
        if let Some(uma) = self.umas.selected.and_then(|id| self.owned_uma(id)).cloned() {
            egui::Panel::right("uma_detail").resizable(true).default_size(440.0).min_size(340.0).frame(crate::side_frame()).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.uma_detail(ui, &uma));
            });
        }
        let list = self.filtered_umas();
        egui::CentralPanel::default().frame(crate::page_frame()).show(ui, |ui| {
            self.uma_results_header(ui, &list);
            if list.is_empty() {
                empty_state(ui, "🔍", "No umas match", "Loosen or clear the filters.");
                return;
            }
            match self.view {
                View::Grid => self.uma_grid(ui, &list),
                View::Table => self.uma_table(ui, &list),
            }
        });
    }

    /// "Showing 12 of 57 umas · Sort […] · S M L · Pictures | Table", then the filters that are on.
    fn uma_results_header(&mut self, ui: &mut egui::Ui, list: &[usize]) {
        let cols = self.uma_cols();
        let shown = list.len();
        // Too narrow for one line: the view tools go on a second.
        let narrow = ui.available_width() < if self.view == View::Grid { 640.0 } else { 730.0 };
        let tools = |app: &mut App, ui: &mut egui::Ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                app.view_switch(ui);
                match app.view {
                    View::Grid => kit::tile_size_switch(ui),
                    View::Table => app.table_tools(ui, "umas", &cols, shown, |app, (r, k)| app.uma_text(&app.roster.umas[list[r]], k)),
                }
            });
        };
        ui.horizontal(|ui| {
            ui.set_min_height(30.0);
            kit::showing(ui, shown, self.roster.umas.len(), "umas");
            ui.add_space(8.0);
            kit::sort_picker(ui, "uma_sort", &mut self.umas.sort, &UMA_SORTS);
            if !narrow {
                tools(self, ui);
            }
        });
        if narrow {
            ui.horizontal(|ui| tools(self, ui));
        }
        let chips = self.uma_chips();
        let labels: Vec<String> = chips.iter().map(|(l, _)| l.clone()).collect();
        match active_filter_bar(ui, &labels) {
            Some(FilterBarAction::Remove(i)) => self.remove_uma_chip(chips[i].1),
            Some(FilterBarAction::ResetAll) => {
                self.umas.filter = UmaFilter::default();
                self.umas.search.clear();
            }
            None => {}
        }
        ui.add_space(6.0);
    }

    fn uma_chips(&self) -> Vec<(String, UmaChip)> {
        let f = &self.umas.filter;
        let mut out = Vec::new();
        if !self.umas.search.trim().is_empty() {
            out.push((format!("“{}”", self.umas.search.trim()), UmaChip::Search));
        }
        out.extend(f.stars.iter().rev().map(|s| (format!("{s}★"), UmaChip::Stars(*s))));
        out.extend(f.potential.iter().map(|s| (format!("P{s}"), UmaChip::Potential(*s))));
        out.extend(f.apt.iter().map(|(a, m)| (format!("{} {}", a.label(), if *m >= 8 { "S" } else { "A or better" }), UmaChip::Apt(*a))));
        if f.evolved {
            out.push(("Has an evolved skill".into(), UmaChip::Evolved));
        }
        if f.recent {
            out.push(("Released in the last 60 days".into(), UmaChip::Recent));
        }
        out
    }

    fn remove_uma_chip(&mut self, c: UmaChip) {
        let f = &mut self.umas.filter;
        match c {
            UmaChip::Search => self.umas.search.clear(),
            UmaChip::Stars(s) => {
                f.stars.remove(&s);
            }
            UmaChip::Potential(s) => {
                f.potential.remove(&s);
            }
            UmaChip::Apt(a) => {
                f.apt.remove(&a);
            }
            UmaChip::Evolved => f.evolved = false,
            UmaChip::Recent => f.recent = false,
        }
    }

    fn uma_filters(&mut self, ui: &mut egui::Ui) {
        let focus = self.focus_search.take();
        kit::search_box(ui, &mut self.umas.search, "🔍 Search name, outfit or note (Ctrl+F)", focus);
        let builtins = [
            ("Maxed potential", "Umas at P5: every skill of their own unlocked", UmaFilter { potential: [5].into(), ..Default::default() }),
            ("Needs potential", "Umas below P5: potential still to raise", UmaFilter { potential: [1, 2, 3, 4].into(), ..Default::default() }),
            ("Long A+", "Umas with A or better at Long distance", UmaFilter { apt: [(Apt::Long, 7)].into(), ..Default::default() }),
            ("Dirt A+", "Umas with A or better on dirt", UmaFilter { apt: [(Apt::Dirt, 7)].into(), ..Default::default() }),
        ];
        kit::quick_views(ui, "umas", &mut self.umas.filter, &builtins);

        let umas = &self.roster.umas;
        let f = &mut self.umas.filter;
        let stars: Vec<_> = (1..=5u8)
            .rev()
            .filter_map(|n| {
                let count = umas.iter().filter(|u| u.stars == n).count();
                (count > 0 || f.stars.contains(&n)).then(|| (n, format!("{n}★"), Some(count), Some(crate::ui_util::GOLD), String::new()))
            })
            .collect();
        if FilterSection::new("Stars").help("Star rank, 1–5★. Higher stars make an uma stronger, including her unique skill.").active(f.stars.len()).show(ui, |ui| {
            kit::chip_set(ui, &mut f.stars, &stars);
        }) {
            f.stars.clear();
        }
        let pots: Vec<_> = (1..=5u8)
            .map(|n| (n, umas.iter().filter(|u| u.potential == n).count()))
            .filter(|(n, count)| *count > 0 || f.potential.contains(n))
            .map(|(n, count)| (n, format!("P{n}"), Some(count), None, if n == 5 { "Maxed: every potential skill unlocked".to_string() } else { String::new() }))
            .collect();
        if FilterSection::new("Potential").help("Potential unlocks an uma's own skills, P1 to P5. At P5 all of them can be bought.").active(f.potential.len()).show(ui, |ui| {
            kit::chip_set(ui, &mut f.potential, &pots);
        }) {
            f.potential.clear();
        }
        if FilterSection::new("Aptitudes")
            .help("Click a cell to keep umas graded A or better there; click again for S only; once more to clear. Grades go from S (best) through A, B, C… down to G.")
            .active(f.apt.len())
            .show(ui, |ui| apt_matrix(ui, &mut f.apt))
        {
            f.apt.clear();
        }
        let more = usize::from(f.evolved) + usize::from(f.recent);
        if FilterSection::new("More").active(more).show(ui, |ui| {
            kit::toggle_row(ui, &mut f.evolved, "Has an evolved skill", "Umas whose skills can evolve into stronger versions during a career");
            kit::toggle_row(ui, &mut f.recent, "Released in the last 60 days", "Outfits that came out on Global in the last two months");
        }) {
            f.evolved = false;
            f.recent = false;
        }
        ui.add_space(12.0);
    }

    /// Whether an owned uma passes the filters and search.
    fn uma_passes(&self, u: &Uma, q: &str, today: i64) -> bool {
        let f = &self.umas.filter;
        let info = self.kb.umas.get(&u.id);
        (f.stars.is_empty() || f.stars.contains(&u.stars))
            && (f.potential.is_empty() || f.potential.contains(&u.potential))
            && f.apt.iter().all(|(a, min)| info.is_some_and(|i| grade_rank(a.grade(&i.aptitude)) >= *min))
            && (!f.evolved || info.is_some_and(|i| !i.evolved.is_empty()))
            && (!f.recent || info.is_some_and(|i| (0..=60).contains(&(today - crate::tab_planner::day_number(&i.release)))))
            && (q.is_empty() || [u.name.as_str(), u.outfit.as_str(), u.notes.as_str()].iter().any(|t| t.to_lowercase().contains(q)))
    }

    /// What a column sorts by.
    pub(crate) fn uma_sort_key(&self, u: &Uma, key: &str) -> SortKey {
        let info = self.kb.umas.get(&u.id);
        let num = |v: f64| SortKey::Num(v);
        match key {
            "name" => SortKey::Text(u.name.clone()),
            "outfit" => SortKey::Text(u.outfit.clone()),
            "stars" => num(f64::from(u.stars) * 10.0 + f64::from(u.potential)),
            "potential" => num(f64::from(u.potential) * 10.0 + f64::from(u.stars)),
            "release" => SortKey::Text(info.map(|i| i.release.clone()).unwrap_or_default()),
            "unique" => SortKey::Text(info.and_then(|i| i.unique_skill).map(|s| self.kb.skill_name(s)).unwrap_or_default()),
            "notes" => SortKey::Text(u.notes.clone()),
            "fit" => num(self.umas.filter.apt.keys().map(|a| info.map_or(0, |i| grade_rank(a.grade(&i.aptitude)) as u32)).sum::<u32>().into()),
            k => {
                if let Some(a) = Apt::ALL.iter().find(|a| a.key() == k) {
                    num(info.map_or(0.0, |i| f64::from(grade_rank(a.grade(&i.aptitude)))))
                } else if let Some(n) = GROWTH.iter().position(|(g, _, _)| *g == k) {
                    num(info.map_or(0.0, |i| f64::from(i.growth[n])))
                } else {
                    SortKey::Text(String::new())
                }
            }
        }
    }

    /// Indexes into the roster's umas that pass the filters, in the chosen order.
    pub(crate) fn filtered_umas(&self) -> Vec<usize> {
        let q = self.umas.search.trim().to_lowercase();
        let today = crate::tab_planner::day_number(&crate::ui_util::today());
        let umas = &self.roster.umas;
        let mut idx: Vec<usize> = (0..umas.len()).filter(|&i| self.uma_passes(&umas[i], &q, today)).collect();
        // Ties keep name order, then id.
        idx.sort_by(|&a, &b| umas[a].name.cmp(&umas[b].name).then(umas[a].id.cmp(&umas[b].id)));
        let sort = self.umas.sort;
        sort_rows(&mut idx, sort.desc, |&i| self.uma_sort_key(&umas[i], sort.key));
        idx
    }

    fn uma_grid(&mut self, ui: &mut egui::Ui, list: &[usize]) {
        let scale = kit::tile_size(ui.ctx()).scale();
        let icon = ICON * scale;
        let extra = if self.umas.filter.apt.is_empty() { 0.0 } else { 14.0 };
        let tile = vec2(icon, icon + 38.0 + extra);
        // Groups by stars or potential when sorted by them.
        let group_of = |u: &Uma| match self.umas.sort.key {
            "stars" => Some(u.stars),
            "potential" => Some(u.potential),
            _ => None,
        };
        let mut groups: Vec<(Option<u8>, Vec<usize>)> = Vec::new();
        for &i in list {
            let key = group_of(&self.roster.umas[i]);
            match groups.last_mut() {
                Some((k, v)) if *k == key => v.push(i),
                _ => groups.push((key, vec![i])),
            }
        }
        let p = pal(ui);
        let heads: Vec<TileGroup> = groups
            .iter()
            .map(|(k, v)| TileGroup {
                title: match (self.umas.sort.key, k) {
                    ("stars", Some(n)) => format!("{n}★"),
                    ("potential", Some(5)) => "P5 · maxed".to_string(),
                    (_, Some(n)) => format!("P{n}"),
                    _ => String::new(),
                },
                color: if self.umas.sort.key == "stars" { p.gold } else { p.text },
                note: format!("{} uma{}", v.len(), if v.len() == 1 { "" } else { "s" }),
                len: v.len(),
            })
            .collect();
        let mut clicked = None;
        let mut plan = None;
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if let Some(y) = self.debug_scroll {
            scroll = scroll.vertical_scroll_offset(y);
        }
        scroll.show(ui, |ui| {
            kit::grouped_tiles(ui, &heads, tile, |ui, g, k| {
                let uma = &self.roster.umas[groups[g].1[k]];
                let (resp, action) = self.uma_tile(ui, uma, icon, tile);
                if action {
                    plan = Some(uma.id);
                } else if resp.clicked() {
                    clicked = Some(uma.id);
                }
            });
            ui.add_space(24.0);
        });
        if let Some(id) = clicked {
            self.umas.selected = (self.umas.selected != Some(id)).then_some(id);
        }
        if let Some(id) = plan {
            self.plan_with_uma(id);
        }
    }

    /// Open the planner with this uma as the trainee.
    pub(crate) fn plan_with_uma(&mut self, id: u32) {
        self.planner.plan.trainee = Some(id);
        if let Some(i) = self.kb.umas.get(&id) {
            self.planner.plan.style = i.aptitude.best_style();
        }
        self.go_career(crate::CareerTab::Plan);
    }

    /// The aptitude letters for the active aptitude filters, e.g. "Long A · Pace A".
    fn filter_grades(&self, id: u32) -> String {
        let Some(info) = self.kb.umas.get(&id) else { return String::new() };
        self.umas.filter.apt.keys().map(|a| format!("{} {}", a.short(), a.grade(&info.aptitude))).collect::<Vec<_>>().join(" · ")
    }

    /// One picture tile. Returns its response, and true when its "Plan with her" button was clicked.
    fn uma_tile(&self, ui: &mut egui::Ui, uma: &Uma, icon: f32, tile: egui::Vec2) -> (Response, bool) {
        let grades = self.filter_grades(uma.id);
        let (rect, resp) = ui.allocate_exact_size(tile, Sense::click());
        let img = Rect::from_min_size(rect.min, vec2(icon, icon));
        let plan_at = img.right_top() + vec2(-14.0, 14.0);
        let mut action = false;
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            match self.art_uri("umas", uma.id) {
                Some(uri) => crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same(10)), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))),
                None => placeholder(p, img, Color32::from_gray(70), &uma.name),
            }
            potential_badge(p, img.right_bottom() - vec2(4.0, 4.0), uma.potential);
            draw_stars(p, Pos2::new(img.center().x, img.bottom() + 11.0), uma.stars, 6.0);
            p.text(Pos2::new(img.center().x, img.bottom() + 28.0), Align2::CENTER_CENTER, truncate(&uma.name, (15.0 * icon / ICON) as usize), FontId::proportional(12.5), ui.visuals().text_color());
            if !grades.is_empty() {
                p.text(Pos2::new(img.center().x, img.bottom() + 43.0), Align2::CENTER_CENTER, &grades, FontId::proportional(11.0), pal(ui).gold);
            }
            tile_ring(ui, img, self.umas.selected == Some(uma.id), resp.hovered(), 12);
            if resp.hovered() {
                let hot = kit::over(ui, plan_at);
                kit::corner_button(ui, plan_at, "📋", hot);
                action = hot && resp.clicked();
            }
        }
        let resp = if resp.hovered() && kit::over(ui, plan_at) {
            resp.on_hover_text("Plan a career with her")
        } else {
            resp.on_hover_ui(|ui| self.uma_hover_card(ui, uma))
        };
        let mut menu_plan = false;
        resp.context_menu(|ui| {
            if ui.button("📋 Plan a career with her").clicked() {
                menu_plan = true;
                ui.close();
            }
            if ui.button("🔍 Open details").clicked() {
                self.jump.set(Some(Jump::Uma(uma.id)));
                ui.close();
            }
        });
        (resp, action || menu_plan)
    }

    /// The hover card on a picture tile: the numbers that matter at a glance.
    fn uma_hover_card(&self, ui: &mut egui::Ui, uma: &Uma) {
        let p = pal(ui);
        ui.set_max_width(300.0);
        ui.label(semibold(&uma.name, size::MD));
        ui.label(RichText::new(format!("[{}]", uma.outfit)).italics().color(p.weak));
        ui.label(format!("{}★ · Potential {}{}", uma.stars, uma.potential, if uma.potential >= 5 { " (maxed)" } else { "" }));
        if let Some(info) = self.kb.umas.get(&uma.id) {
            let best: Vec<String> = Apt::ALL.iter().filter(|a| grade_rank(a.grade(&info.aptitude)) >= 7).map(|a| format!("{} {}", a.label(), a.grade(&info.aptitude))).collect();
            if !best.is_empty() {
                ui.label(RichText::new(format!("A or better: {}", best.join(" · "))).size(size::SM));
            }
            let growth: Vec<String> = GROWTH.iter().zip(info.growth).filter(|(_, v)| *v > 0).map(|((_, _, n), v)| format!("+{v}% {n}")).collect();
            if !growth.is_empty() {
                ui.label(RichText::new(format!("Growth: {}", growth.join(" · "))).size(size::SM).color(p.weak));
            }
        }
        ui.label(RichText::new("Click for details · right-click for more").size(size::XS).color(p.weak));
    }

    fn uma_cols(&self) -> Vec<TableCol> {
        let mut cols = vec![
            TableCol::new("art", "").width(Column::exact(34.0)),
            TableCol::new("name", "Uma").width(Column::initial(150.0).at_least(110.0).clip(true)).sort_asc(),
            TableCol::new("outfit", "Outfit").width(Column::initial(170.0).at_least(100.0).clip(true)).sort_asc(),
            TableCol::new("stars", "Stars").width(Column::exact(86.0)).center().sort_desc().hover("Star rank, 1–5★"),
            TableCol::new("potential", "Potential").width(Column::exact(76.0)).center().sort_desc().hover("P1 to P5: how many of her own skills are unlocked"),
        ];
        for a in Apt::ALL {
            cols.push(
                TableCol::new(a.key(), a.short())
                    .width(Column::exact(48.0))
                    .center()
                    .sort_desc()
                    .group(a.group())
                    .hover(format!("{} aptitude: S is best, then A, B, C… down to G", a.label())),
            );
        }
        for (key, title, stat) in GROWTH {
            cols.push(
                TableCol::new(key, title)
                    .width(Column::exact(62.0))
                    .right()
                    .sort_desc()
                    .group("Growth")
                    .optional(true)
                    .hover(format!("Growth: a bonus to what training adds to {stat}")),
            );
        }
        cols.push(TableCol::new("unique", "Unique skill").width(Column::initial(190.0).clip(true)).sort_asc().optional(true).hover("Her own skill, from 1★"));
        cols.push(TableCol::new("release", "Released").width(Column::exact(96.0)).sort_desc().optional(true).hover("When this outfit came out on Global"));
        if self.roster.umas.iter().any(|u| !u.notes.trim().is_empty()) {
            cols.push(TableCol::new("notes", "Notes").width(Column::remainder().at_least(100.0).clip(true)).sort_asc());
        }
        cols
    }

    /// A cell as text, for Copy as CSV.
    fn uma_text(&self, u: &Uma, key: &str) -> String {
        let info = self.kb.umas.get(&u.id);
        match key {
            "name" => u.name.clone(),
            "outfit" => u.outfit.clone(),
            "stars" => u.stars.to_string(),
            "potential" => format!("P{}", u.potential),
            "unique" => info.and_then(|i| i.unique_skill).map(|s| self.kb.skill_name(s)).unwrap_or_default(),
            "release" => info.map(|i| i.release.clone()).unwrap_or_default(),
            "notes" => u.notes.clone(),
            k => {
                if let Some(a) = Apt::ALL.iter().find(|a| a.key() == k) {
                    info.map(|i| a.grade(&i.aptitude).to_string()).unwrap_or_default()
                } else if let Some(n) = GROWTH.iter().position(|(g, _, _)| *g == k) {
                    info.map(|i| i.growth[n].to_string()).unwrap_or_default()
                } else {
                    String::new()
                }
            }
        }
    }

    fn uma_table(&mut self, ui: &mut egui::Ui, list: &[usize]) {
        let cols = self.uma_cols();
        let selected = self.umas.selected.and_then(|id| list.iter().position(|&i| self.roster.umas[i].id == id));
        let mut sort = self.umas.sort;
        let out = egui::ScrollArea::horizontal()
            .id_salt("uma_table_h")
            .show(ui, |ui| {
                DataTable::new("umas", &cols).sort(&mut sort).row_height(36.0).selected(selected).show(ui, list.len(), |ui, r, key| {
                    let uma = &self.roster.umas[list[r]];
                    let info = self.kb.umas.get(&uma.id);
                    match key {
                        "art" => {
                            if let Some(uri) = self.art_uri("umas", uma.id) {
                                ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(28.0, 28.0)).corner_radius(CornerRadius::same(6)));
                            }
                        }
                        "name" => {
                            ui.add(egui::Label::new(&uma.name).truncate());
                        }
                        "outfit" => {
                            ui.add(egui::Label::new(RichText::new(&uma.outfit).color(pal(ui).weak)).truncate());
                        }
                        "stars" => {
                            stars_widget(ui, uma.stars, 5.5);
                        }
                        "potential" => {
                            let text = semibold(format!("P{}", uma.potential), size::BODY);
                            ui.label(if uma.potential >= 5 { text.color(pal(ui).gold) } else { text });
                        }
                        "unique" => {
                            if let Some(s) = info.and_then(|i| i.unique_skill) {
                                ui.add(egui::Label::new(self.kb.skill_name(s)).truncate());
                            }
                        }
                        "release" => {
                            ui.label(RichText::new(info.map(|i| i.release.as_str()).unwrap_or("")).color(pal(ui).weak));
                        }
                        "notes" => {
                            ui.add(egui::Label::new(RichText::new(&uma.notes).color(pal(ui).weak)).truncate());
                        }
                        k => {
                            if let Some(a) = Apt::ALL.iter().find(|a| a.key() == k) {
                                grade_tile(ui, info.map_or("", |i| a.grade(&i.aptitude)));
                            } else if let Some(n) = GROWTH.iter().position(|(g, _, _)| *g == k) {
                                let v = info.map_or(0, |i| i.growth[n]);
                                if v == 0 {
                                    ui.label(RichText::new("—").color(pal(ui).weak));
                                } else {
                                    ui.label(semibold(format!("+{v}%"), size::BODY).color(pal(ui).gold));
                                }
                            }
                        }
                    }
                })
            })
            .inner;
        self.umas.sort = sort;
        if let Some(r) = out.clicked {
            let id = self.roster.umas[list[r]].id;
            self.umas.selected = (self.umas.selected != Some(id)).then_some(id);
        }
    }

    /// The Pictures | Table switch.
    pub(crate) fn view_switch(&mut self, ui: &mut egui::Ui) {
        let views = [(View::Grid, "▦ Pictures".to_string(), "Show art tiles"), (View::Table, "☰ Table".to_string(), "Show a sortable table with more columns")];
        crate::ui_util::segmented(ui, &mut self.view, &views);
    }

    /// In table view: "☰ Columns" and "⧉ Copy as CSV". `text(app, (row, key))` gives a cell's text.
    pub(crate) fn table_tools(&mut self, ui: &mut egui::Ui, table: &str, cols: &[TableCol], rows: usize, text: impl Fn(&App, (usize, &str)) -> String) {
        let compact = ui.available_width() < 520.0;
        if ui.button(if compact { "⧉" } else { "⧉ Copy as CSV" }).on_hover_text("Copy the rows shown, with the columns shown, to paste into a spreadsheet").clicked() {
            let csv = crate::ui_util::table_csv(ui.ctx(), table, cols, rows, |r, k| text(self, (r, k)));
            ui.ctx().copy_text(csv);
            self.toast(format!("Copied {rows} rows. Paste them into a spreadsheet."), crate::ui_util::ToastKind::Success);
        }
        if cols.iter().any(|c| c.optional) {
            crate::ui_util::columns_menu(ui, table, cols);
        }
    }

    fn uma_detail(&mut self, ui: &mut egui::Ui, uma: &Uma) {
        let p = pal(ui);
        ui.add_space(10.0);
        let info = self.kb.umas.get(&uma.id).cloned();

        // Header
        card(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(uri) = self.art_uri("umas", uma.id) {
                    ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(108.0, 108.0)).corner_radius(CornerRadius::same(14)));
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(semibold(&uma.name, 22.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if ui.small_btn("✖").on_hover_text("Close").clicked() {
                                self.umas.selected = None;
                            }
                        });
                    });
                    ui.label(RichText::new(format!("[{}]", uma.outfit)).italics().color(p.weak));
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        stars_widget(ui, uma.stars, 7.5);
                        if uma.stars < 5 && ui.small_btn(RichText::new("＋").color(p.weak)).on_hover_text(format!("Pulled another copy? Raise her to {}★ and save it. Shortcut: +", uma.stars + 1)).clicked() {
                            self.raise_uma(uma.id);
                        }
                    });
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        if uma.potential >= 5 {
                            badge(ui, "Potential 5 (maxed)", p.accent).on_hover_text("All her potential skills are unlocked");
                        } else {
                            badge(ui, format!("Potential {} of 5", uma.potential), pal(ui).warn).on_hover_text("Some of her potential skills are still locked");
                        }
                        if let Some(i) = &info {
                            if i.release == crate::knowledge::GLOBAL_LAUNCH {
                                badge(ui, "On Global since launch", p.weak).on_hover_text(format!("She was there when the Global server opened, {}.", i.release));
                            } else {
                                badge(ui, format!("Released {}", i.release), p.weak).on_hover_text("When she arrived on the Global server.");
                            }
                        }
                    });
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("📋 Plan a career with her").on_hover_text("Open the Planner with this uma as the trainee").clicked() {
                    self.planner.plan.trainee = Some(uma.id);
                    if let Some(i) = &info {
                        self.planner.plan.style = i.aptitude.best_style();
                    }
                    self.go_career(crate::CareerTab::Plan);
                }
                if !uma.url.is_empty() {
                    web_link(ui, "uma.guide", &uma.url);
                }
            });
            if !uma.notes.is_empty() {
                ui.label(RichText::new(&uma.notes).color(p.weak));
            }
        });
        ui.add_space(8.0);

        if info.is_none() {
            ui.label(RichText::new("No knowledge data for this outfit yet. Rebuild knowledge/data.").color(pal(ui).warn));
            return;
        }
        self.uma_sections(ui, uma.id, Some(uma.potential), true, true);

        let outfits: Vec<&Uma> = self.roster.umas.iter().filter(|u| u.name == uma.name && u.id != uma.id).collect();
        let own_cards: Vec<_> = self.roster.cards.iter().filter(|c| c.uma == uma.name).collect();
        if !outfits.is_empty() || !own_cards.is_empty() {
            card_titled(ui, "Related", |ui| {
                for u in outfits {
                    if ui.link(format!("🐴 [{}] {}★ P{}", u.outfit, u.stars, u.potential)).clicked() {
                        self.jump.set(Some(Jump::Uma(u.id)));
                    }
                }
                if !own_cards.is_empty() {
                    ui.label(RichText::new(format!("Your {} cards can't go in her own deck.", uma.name)).size(12.0).color(p.weak));
                    for c in own_cards {
                        if ui.link(format!("🃏 {} {} [{}] LB{}", c.rarity.label(), c.card_type.label(), c.title, c.lb)).clicked() {
                            self.jump.set(Some(Jump::Card(c.id)));
                        }
                    }
                }
            });
        }
        ui.add_space(10.0);
    }

    /// Everything known about an outfit, below its header: aptitudes and stats (each unless the
    /// header already shows it), skills (locked above `potential` when she's owned),
    /// evolutions, objectives and events. Collection's detail panel and the detail pop-up both
    /// draw it.
    pub(crate) fn uma_sections(&mut self, ui: &mut egui::Ui, id: u32, potential: Option<u8>, aptitudes: bool, stats: bool) {
        let p = pal(ui);
        let Some(info) = self.kb.umas.get(&id).cloned() else { return };

        if aptitudes {
            card_explained(ui, "Aptitudes", "S and A are ideal. Below A she runs slower there, unless parents' pink sparks raise the grade.", |ui| aptitude_rows(ui, &info));
        }
        if stats {
            card_explained(ui, "Stats", "Base: her starting stats. Growth: a bonus to what training adds to that stat.", |ui| stats_grid(ui, &info));
        }

        card_explained(ui, "Skills", "Click a skill to see what it does. Potential skills above her current potential are locked.", |ui| {
            if let Some(u) = info.unique_skill {
                ui.label(RichText::new("UNIQUE").size(11.0).color(p.weak));
                self.skill_link(ui, u, false);
                if let Some(s) = self.kb.skills.get(&u) {
                    ui.label(RichText::new(&s.summary).size(12.0).color(p.weak));
                }
                ui.add_space(6.0);
            }
            let mut levels: Vec<u8> = info.skills.iter().map(|s| s.potential).collect();
            levels.dedup();
            for lvl in levels {
                let locked = potential.is_some_and(|p| lvl > p);
                let title = if lvl <= 1 { "INNATE".to_string() } else { format!("POTENTIAL {lvl}{}", if locked { "  🔒 locked" } else { "" }) };
                ui.label(RichText::new(title).size(11.0).color(if locked { pal(ui).warn } else { p.weak }));
                ui.horizontal_wrapped(|ui| {
                    for s in info.skills.iter().filter(|s| s.potential == lvl) {
                        self.skill_link(ui, s.id, locked);
                    }
                });
                ui.add_space(4.0);
            }
            if !info.event_skills.is_empty() {
                ui.label(RichText::new("FROM HER EVENTS").size(11.0).color(p.weak));
                ui.horizontal_wrapped(|ui| {
                    for &s in &info.event_skills {
                        self.skill_link(ui, s, false);
                    }
                });
            }
        });

        if !info.evolved.is_empty() {
            card_explained(ui, "Evolved skills", "Stronger versions of her skills, unlocked in a career when the conditions are met.", |ui| {
                for e in &info.evolved {
                    ui.horizontal_wrapped(|ui| {
                        self.skill_link(ui, e.base, false);
                        ui.label(RichText::new("➜").color(p.weak));
                        self.skill_link(ui, e.evolved, false);
                    });
                    let alts: Vec<String> = e.conditions.iter().map(|all| all.join(" + ")).collect();
                    if !alts.is_empty() {
                        ui.label(RichText::new(format!("Needs: {}", alts.join("  OR  "))).size(12.0).color(p.weak));
                    }
                    ui.add_space(4.0);
                }
            });
        }

        if !info.objectives.is_empty() {
            card_explained(ui, "Career objectives", "Goals she must meet during her career. Missing one ends the career early.", |ui| {
                egui::Grid::new("uma_obj").num_columns(3).spacing([12.0, 5.0]).striped(true).show(ui, |ui| {
                    for o in &info.objectives {
                        ui.label(RichText::new(&o.when).color(p.weak));
                        ui.label(title_case(&o.race));
                        ui.label(semibold(&o.goal, 13.5));
                        ui.end_row();
                    }
                });
            });
        }

        if !info.events.is_empty() {
            card_explained(ui, &format!("Events ({})", info.events.len()), "Story events in her career and what each choice gives. Click one to open it.", |ui| {
                for (i, e) in info.events.iter().enumerate() {
                    egui::CollapsingHeader::new(RichText::new(&e.name)).id_salt(("uma_ev", id, i)).show(ui, |ui| {
                        ui.label(RichText::new(&e.kind).size(11.5).color(p.weak));
                        for (n, o) in e.options.iter().enumerate() {
                            let label = if e.options.len() > 1 { format!("Option {}", n + 1) } else { "Outcome".to_string() };
                            ui.label(RichText::new(label).strong()).on_hover_text(&o.label);
                            for out in &o.outcomes {
                                ui.label(format!("  • {}", out.replace('\n', ", ")));
                            }
                        }
                    });
                }
            });
        }

    }
}

/// Her ten aptitude grades in three rows: track, distance, style.
pub(crate) fn aptitude_rows(ui: &mut egui::Ui, info: &crate::knowledge::UmaInfo) {
    let p = pal(ui);
    let a = &info.aptitude;
    let rows: [(&str, Vec<(&str, &str)>); 3] = [
        ("Track", vec![("Turf", &a.turf), ("Dirt", &a.dirt)]),
        ("Distance", vec![("Sprint", &a.sprint), ("Mile", &a.mile), ("Medium", &a.medium), ("Long", &a.long)]),
        ("Style", vec![("Front", &a.front), ("Pace", &a.pace), ("Late", &a.late), ("End", &a.end)]),
    ];
    for (title, cells) in rows {
        ui.horizontal(|ui| {
            ui.add_sized([70.0, 40.0], egui::Label::new(RichText::new(title).color(p.weak)));
            for (label, g) in cells {
                grade_badge(ui, label, g);
            }
        });
    }
}

/// Her base stats and growth bonuses, a row each, in the same 52 px columns as the aptitude
/// grades above them.
pub(crate) fn stats_grid(ui: &mut egui::Ui, info: &crate::knowledge::UmaInfo) {
    let p = pal(ui);
    let cell = |ui: &mut egui::Ui, text: RichText| {
        ui.allocate_ui_with_layout(vec2(52.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(52.0);
            ui.label(text);
        });
    };
    let row = |ui: &mut egui::Ui, title: &str, hover: &str, cells: Vec<RichText>| {
        ui.horizontal(|ui| {
            ui.add_sized([70.0, 22.0], egui::Label::new(RichText::new(title).color(p.weak))).on_hover_text(hover);
            for c in cells {
                cell(ui, c);
            }
        });
    };
    let heads = ["Speed", "Stamina", "Power", "Guts", "Wit"].map(|h| RichText::new(h).size(11.5).color(p.weak)).to_vec();
    row(ui, "", "", heads);
    row(ui, "Base", "Her stats at the start of a career.", info.base.iter().map(|v| semibold(v.to_string(), 15.0)).collect());
    let growth = info.growth.iter().map(|v| if *v == 0 { RichText::new("–").color(p.weak) } else { semibold(format!("+{v}%"), 14.0).color(p.gold) }).collect();
    row(ui, "Growth", "A bonus to what training adds to that stat.", growth);
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The aptitude filter: a cell per surface, distance and running style. A click cycles a cell
/// through any grade → A or better → S only; right-click clears it.
fn apt_matrix(ui: &mut egui::Ui, apt: &mut BTreeMap<Apt, u8>) {
    let p = pal(ui);
    let gap = 4.0;
    let cell_w = ((ui.available_width() - 3.0 * gap) / 4.0).floor();
    for (title, row) in [("Surface", &Apt::ALL[0..2]), ("Distance", &Apt::ALL[2..6]), ("Style", &Apt::ALL[6..10])] {
        ui.label(RichText::new(title).size(size::XS).color(p.weak));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &a in row {
                let min = apt.get(&a).copied();
                let (rect, resp) = ui.allocate_exact_size(vec2(cell_w, 38.0), Sense::click());
                let on_fill = min.map(|m| grade_color(if m >= 8 { "S" } else { "A" }));
                let fill = on_fill.unwrap_or(if resp.hovered() { p.hover } else { p.input });
                let ink = if on_fill.is_some() { Color32::from_rgb(20, 22, 28) } else { p.text };
                let painter = ui.painter();
                painter.rect(rect, CornerRadius::same(7), fill, Stroke::new(1.0, on_fill.unwrap_or(p.border)), StrokeKind::Inside);
                painter.text(pos2(rect.center().x, rect.top() + 12.0), Align2::CENTER_CENTER, a.short(), FontId::proportional(size::SM), ink);
                painter.text(pos2(rect.center().x, rect.bottom() - 11.0), Align2::CENTER_CENTER, min_label(min), FontId::proportional(size::XS), if on_fill.is_some() { ink } else { p.weak });
                let now = match min {
                    Some(8) => "S only",
                    Some(_) => "A or better",
                    None => "any grade",
                };
                let resp = resp.on_hover_text(format!("{}: {now}. Click to cycle any → A or better → S only; right-click to clear.", a.label())).on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
                    match min {
                        None => {
                            apt.insert(a, 7);
                        }
                        Some(7) => {
                            apt.insert(a, 8);
                        }
                        _ => {
                            apt.remove(&a);
                        }
                    }
                }
                if resp.secondary_clicked() {
                    apt.remove(&a);
                }
            }
        });
        ui.add_space(2.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_util::TableSort;

    fn long(app: &App, i: usize) -> u8 {
        grade_rank(&app.kb.umas[&app.roster.umas[i].id].aptitude.long)
    }

    /// Every way the Umas table sorts: aptitude grades, names, stars then potential, and a header
    /// click flipping the direction.
    #[test]
    fn umas_sort_by_any_column() {
        let mut app = App::for_tests();
        app.umas.sort = TableSort { key: "long", desc: true };
        let list = app.filtered_umas();
        assert_eq!(list.len(), app.roster.umas.len());
        assert!(list.windows(2).all(|w| long(&app, w[0]) >= long(&app, w[1])));
        let col = app.uma_cols().into_iter().find(|c| c.key == "long").unwrap();
        app.umas.sort.click(&col);
        assert!(!app.umas.sort.desc);
        let list = app.filtered_umas();
        assert!(list.windows(2).all(|w| long(&app, w[0]) <= long(&app, w[1])));

        app.umas.sort = TableSort { key: "name", desc: false };
        let names: Vec<String> = app.filtered_umas().iter().map(|&i| app.roster.umas[i].name.to_lowercase()).collect();
        assert!(names.windows(2).all(|w| w[0] <= w[1]), "{names:?}");

        app.umas.sort = TableSort { key: "stars", desc: true };
        let key = |i: usize| (app.roster.umas[i].stars, app.roster.umas[i].potential);
        assert!(app.filtered_umas().windows(2).all(|w| key(w[0]) >= key(w[1])));

        // Another column clicked starts its own way: names A to Z.
        let name = app.uma_cols().into_iter().find(|c| c.key == "name").unwrap();
        let mut sort = TableSort { key: "stars", desc: true };
        sort.click(&name);
        assert_eq!((sort.key, sort.desc), ("name", false));
    }

    /// The aptitude matrix keeps umas at or above each cell's grade, and a saved view keeps it.
    #[test]
    fn aptitude_filter_and_views() {
        let mut app = App::for_tests();
        app.umas.filter.apt.insert(Apt::Long, 7);
        let a_or_better = app.filtered_umas();
        assert!(!a_or_better.is_empty());
        assert!(a_or_better.iter().all(|&i| long(&app, i) >= 7));
        app.umas.filter.apt.insert(Apt::Long, 8);
        assert!(app.filtered_umas().iter().all(|&i| long(&app, i) >= 8));
        app.umas.filter.potential = [5].into();
        assert!(app.filtered_umas().iter().all(|&i| app.roster.umas[i].potential == 5));
        assert_eq!(app.umas.filter.count(), 2);

        let json = serde_json::to_value(&app.umas.filter).unwrap();
        assert_eq!(json["apt"]["long"], 8);
        let back: UmaFilter = serde_json::from_value(json).unwrap();
        assert_eq!(back, app.umas.filter);
    }
}
