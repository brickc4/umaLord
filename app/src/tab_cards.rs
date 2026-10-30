//! Support cards: owned cards with effects at the owned level, hints and events.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::sync::mpsc;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, RichText, Sense, Stroke, pos2, vec2};
use egui_extras::Column;
use serde::{Deserialize, Serialize};

use crate::collection_kit::{self as kit, SortOption, TileGroup};
use crate::ui_util::UiExt;
use crate::data::{CardType, Rarity, SupportCard};
use crate::knowledge::{CardInfo, effect, effect_at};
use crate::tab_umas::FILTER_W;
use crate::theme::{pal, size};
use crate::ui_util::{
    DataTable, FilterBarAction, FilterSection, SortKey, TableCol, TableSort, active_filter_bar, badge, card_explained, card_titled, draw_lb,
    empty_state, lb_widget, placeholder, semibold, sort_rows, tile_ring, truncate, type_color, type_color_str, type_icon, type_text, web_link,
};
use crate::{App, Jump, View};

const CARD_W: f32 = 104.0;
const CARD_H: f32 = CARD_W * 346.0 / 260.0;

/// Effect columns: effect id, column key, short title, full name, unit, shown by default.
pub(crate) const FX: [(u32, &str, &str, &str, &str, bool); 12] = [
    (effect::FRIENDSHIP, "friendship", "Friendship", "Friendship Bonus", "%", true),
    (effect::TRAINING, "training", "Training", "Training Effectiveness", "%", true),
    (effect::SPECIALTY, "specialty", "Specialty", "Specialty Priority", "", true),
    (effect::RACE, "race", "Race bonus", "Race Bonus", "%", true),
    (effect::HINT_FREQ, "hint_rate", "Hint rate", "Hint Frequency", "%", true),
    (effect::MOOD, "mood", "Mood", "Mood Effect", "%", false),
    (effect::INIT_BOND, "init_bond", "Start bond", "Initial Friendship Gauge (uma.guide: Starting Bond)", "", false),
    (effect::HINT_LEVELS, "hint_levels", "Hint lv", "Hint Levels", "", false),
    (effect::FAIL_PROTECT, "fail_protect", "Fail guard", "Failure Protection", "%", false),
    (effect::ENERGY_COST, "energy", "Energy cut", "Energy Cost Reduction", "%", false),
    (effect::SKILL_POINTS, "sp_bonus", "SP bonus", "Skill Point Bonus", "", false),
    (effect::WIT_RECOVERY, "wit_recovery", "Wit recov.", "Wit Friendship Recovery", "", false),
];

fn fx_of(key: &str) -> Option<&'static (u32, &'static str, &'static str, &'static str, &'static str, bool)> {
    FX.iter().find(|f| f.1 == key)
}

fn fx_name(kind: u32) -> &'static str {
    FX.iter().find(|f| f.0 == kind).map_or("?", |f| f.3)
}

fn rarity_rank(r: Rarity) -> f64 {
    match r {
        Rarity::SSR => 3.0,
        Rarity::SR => 2.0,
        Rarity::R => 1.0,
    }
}

/// The filters on the Support cards sub-tab. Saved views store this.
#[derive(Clone, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CardFilter {
    /// Card type labels ("Speed", "Pal"…).
    pub types: BTreeSet<String>,
    pub rarities: BTreeSet<String>,
    /// Limit breaks 0–4 (4 is MLB).
    pub lbs: BTreeSet<u8>,
    pub below_cap: bool,
    /// Skill ids: cards that can teach any of them.
    pub teaches: Vec<u32>,
    /// Only cards that can teach a gold skill.
    pub gold_only: bool,
    /// (effect id, lowest value at the owned level).
    pub minimums: Vec<(u32, i32)>,
    /// A scenario's name: cards of its linked characters.
    pub linked: Option<String>,
}

impl CardFilter {
    pub fn count(&self) -> usize {
        self.types.len()
            + self.rarities.len()
            + self.lbs.len()
            + usize::from(self.below_cap)
            + self.teaches.len()
            + usize::from(self.gold_only)
            + self.minimums.len()
            + usize::from(self.linked.is_some())
    }
}

#[derive(Clone, Copy)]
enum CardChip {
    Search,
    Type(CardType),
    Rarity(Rarity),
    Lb(u8),
    BelowCap,
    Teaches(u32),
    GoldOnly,
    Minimum(usize),
    Linked,
}

pub(crate) struct CardsState {
    pub search: String,
    pub filter: CardFilter,
    pub sort: TableSort,
    pub selected: Option<u32>,
    pub add: AddCard,
    /// What's typed in the "Teaches…" box.
    pub teach_query: String,
    /// Cards picked to compare (two at most); the window opens at two.
    pub compare: Vec<u32>,
    pub compare_open: bool,
    pub art: ArtQueue,
}

/// The "Add cards" dialog: pick any Global card, set its LB and level, save it to the roster.
#[derive(Default)]
pub(crate) struct AddCard {
    pub open: bool,
    pub search: String,
    pub kind: Option<String>,
    pub pick: Option<u32>,
    pub lb: u8,
    pub level: u8,
    pub notes: String,
    /// Also list cards uma.guide shows ahead of their Global release.
    pub upcoming: bool,
    pub owned_only: bool,
}

/// Which art: a support card's, an uma outfit's icon, or her full-body render (the detail
/// pop-up's, fetched when one opens).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Art {
    Card,
    Uma,
    Full,
}

impl Art {
    /// Its folder under app/assets, its file type, and where it's served.
    pub(crate) fn dir(self) -> &'static str {
        match self {
            Art::Card => "cards",
            Art::Uma => "umas",
            Art::Full => "umas-full",
        }
    }

    pub(crate) fn ext(self) -> (&'static str, image::ImageFormat) {
        match self {
            Art::Full => ("png", image::ImageFormat::Png),
            _ => ("webp", image::ImageFormat::WebP),
        }
    }

    pub(crate) fn url(self, id: u32) -> String {
        match self {
            Art::Card => format!("https://uma.guide/img/card/composite/tex_support_card_{id}_thumb.webp"),
            // The character id is the outfit id's first four digits: 100101 is chr_icon_1001_100101.
            Art::Uma => format!("https://uma.guide/icon/uma/chr_icon_{}_{id}_01.webp", id / 100),
            // GameTora's trainee render: the outfit's 3D model standing, 768×1024 on transparency
            // (all 109 Global outfits there, checked 2026-10-29).
            Art::Full => format!("https://media.gametora.com/umamusume/characters/render/trainee/idle/{id}.png"),
        }
    }
}

/// Art to download from uma.guide, four at a time: cards and uma icons the app has no picture
/// for (on demand, and once per launch for everything in the knowledge data).
#[derive(Default)]
pub(crate) struct ArtQueue {
    pending: VecDeque<(Art, u32)>,
    seen: HashSet<(Art, u32)>,
    running: Vec<mpsc::Receiver<(Art, u32, bool)>>,
    /// Downloads that failed this session, so a picture stops showing as loading.
    failed: HashSet<(Art, u32)>,
    /// The card just saved: say so if its art fails.
    watch: Option<u32>,
    /// The launch sweep has been queued.
    swept: bool,
}

impl Default for CardsState {
    fn default() -> Self {
        Self {
            search: String::new(),
            filter: CardFilter::default(),
            sort: TableSort { key: "type", desc: false },
            selected: None,
            add: AddCard::default(),
            teach_query: String::new(),
            compare: Vec::new(),
            compare_open: false,
            art: ArtQueue::default(),
        }
    }
}

fn card_sorts() -> Vec<SortOption> {
    let mut v: Vec<SortOption> = vec![
        ("type", "Type, then rarity and LB", false),
        ("rarity", "Rarity", true),
        ("lb", "Limit break", true),
        ("level", "Level", true),
        ("uma", "Uma", false),
        ("title", "Card title", false),
        ("release", "Release date", true),
        ("hints", "Number of skill hints", true),
    ];
    v.extend(FX.iter().map(|f| (f.1, f.3, true)));
    v
}

impl App {
    pub(crate) fn cards_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut collapsed = kit::rail_collapsed(&ctx, "cards");
        if collapsed && self.focus_search.get() {
            collapsed = false;
            kit::set_rail(&ctx, "cards", false);
        }
        // A narrow window with the details open, or with a table: fold the filters so the list
        // keeps its room.
        let table_fold = self.view == View::Table && !kit::opened_by_hand(&ctx, "cards");
        let auto_fold = !collapsed && ui.available_width() < 1100.0 && !self.focus_search.get() && (self.cards.selected.is_some() || table_fold);
        collapsed |= auto_fold;
        let active = self.cards.filter.count() + usize::from(!self.cards.search.trim().is_empty());
        if collapsed {
            egui::Panel::left("card_filters_rail").resizable(false).exact_size(kit::RAIL_W).frame(kit::rail_frame()).show(ui, |ui| {
                if kit::rail(ui, active, &[("🔍", "Search"), ("🃏", "Type and rarity"), ("◆", "Limit break"), ("💡", "Teaches"), ("📈", "Effect minimums")]) {
                    if auto_fold {
                        self.cards.selected = None;
                        kit::open_by_hand(&ctx, "cards");
                    } else {
                        kit::set_rail(&ctx, "cards", false);
                    }
                }
            });
        } else {
            egui::Panel::left("card_filters").resizable(false).exact_size(FILTER_W).frame(crate::side_frame()).show(ui, |ui| {
                if kit::sidebar_head(ui, active) {
                    kit::set_rail(&ctx, "cards", true);
                }
                egui::ScrollArea::vertical().show(ui, |ui| self.card_filters(ui));
            });
        }
        if let Some(card) = self.cards.selected.and_then(|id| self.owned_card(id)).cloned() {
            egui::Panel::right("card_detail").resizable(true).default_size(440.0).min_size(340.0).frame(crate::side_frame()).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.card_detail(ui, &card));
            });
        }
        let list = self.filtered_cards();
        egui::CentralPanel::default().frame(crate::page_frame()).show(ui, |ui| {
            self.card_results_header(ui, &list);
            if !self.cards.compare.is_empty() {
                self.compare_tray(ui);
            }
            if list.is_empty() {
                empty_state(ui, "🔍", "No cards match", "Loosen or clear the filters.");
                return;
            }
            match self.view {
                View::Grid => self.card_grid(ui, &list),
                View::Table => self.card_table(ui, &list),
            }
        });
        if self.cards.compare_open && self.cards.compare.len() == 2 {
            self.compare_window(&ctx);
        }
        if self.cards.add.open {
            self.add_cards_dialog(ui);
        }
    }

    fn card_results_header(&mut self, ui: &mut egui::Ui, list: &[usize]) {
        let cols = self.card_cols();
        let narrow = ui.available_width() < if self.view == View::Grid { 720.0 } else { 800.0 };
        let tools = |app: &mut App, ui: &mut egui::Ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                app.view_switch(ui);
                match app.view {
                    View::Grid => kit::tile_size_switch(ui),
                    View::Table => app.table_tools(ui, "cards", &cols, list.len(), |app, (r, k)| app.card_text(&app.roster.cards[list[r]], k)),
                }
            });
        };
        ui.horizontal(|ui| {
            ui.set_min_height(30.0);
            kit::showing(ui, list.len(), self.roster.cards.len(), "support cards");
            ui.add_space(8.0);
            kit::sort_picker(ui, "card_sort", &mut self.cards.sort, &card_sorts());
            if !narrow {
                tools(self, ui);
            }
        });
        if narrow {
            ui.horizontal(|ui| tools(self, ui));
        }
        let chips = self.card_chips();
        let labels: Vec<String> = chips.iter().map(|(l, _)| l.clone()).collect();
        match active_filter_bar(ui, &labels) {
            Some(FilterBarAction::Remove(i)) => self.remove_card_chip(chips[i].1),
            Some(FilterBarAction::ResetAll) => {
                self.cards.filter = CardFilter::default();
                self.cards.search.clear();
            }
            None => {}
        }
        ui.add_space(6.0);
    }

    fn card_chips(&self) -> Vec<(String, CardChip)> {
        let f = &self.cards.filter;
        let mut out = Vec::new();
        if !self.cards.search.trim().is_empty() {
            out.push((format!("“{}”", self.cards.search.trim()), CardChip::Search));
        }
        for t in CardType::ALL.into_iter().filter(|t| f.types.contains(t.label())) {
            out.push((format!("{} {}", type_icon(t.label()), t.label()), CardChip::Type(t)));
        }
        for r in [Rarity::SSR, Rarity::SR, Rarity::R].into_iter().filter(|r| f.rarities.contains(r.label())) {
            out.push((r.label().to_string(), CardChip::Rarity(r)));
        }
        out.extend(f.lbs.iter().map(|lb| (if *lb >= 4 { "MLB".to_string() } else { format!("LB{lb}") }, CardChip::Lb(*lb))));
        if f.below_cap {
            out.push(("Below level cap".into(), CardChip::BelowCap));
        }
        out.extend(f.teaches.iter().map(|s| (format!("Teaches {}", self.kb.skill_name(*s)), CardChip::Teaches(*s))));
        if f.gold_only {
            out.push(("Teaches a gold".into(), CardChip::GoldOnly));
        }
        out.extend(f.minimums.iter().enumerate().map(|(i, (k, v))| (format!("{} ≥ {v}", short_effect(fx_name(*k))), CardChip::Minimum(i))));
        if let Some(s) = &f.linked {
            out.push((format!("Linked in {s}"), CardChip::Linked));
        }
        out
    }

    fn remove_card_chip(&mut self, c: CardChip) {
        let f = &mut self.cards.filter;
        match c {
            CardChip::Search => self.cards.search.clear(),
            CardChip::Type(t) => {
                f.types.remove(t.label());
            }
            CardChip::Rarity(r) => {
                f.rarities.remove(r.label());
            }
            CardChip::Lb(lb) => {
                f.lbs.remove(&lb);
            }
            CardChip::BelowCap => f.below_cap = false,
            CardChip::Teaches(s) => f.teaches.retain(|x| *x != s),
            CardChip::GoldOnly => f.gold_only = false,
            CardChip::Minimum(i) => {
                if i < f.minimums.len() {
                    f.minimums.remove(i);
                }
            }
            CardChip::Linked => f.linked = None,
        }
    }

    fn card_filters(&mut self, ui: &mut egui::Ui) {
        let focus = self.focus_search.take();
        kit::search_box(ui, &mut self.cards.search, "🔍 Search card, uma or note (Ctrl+F)", focus);
        let builtins = [
            ("Maxed SSRs", "SSR cards at max limit break (MLB)", CardFilter { rarities: ["SSR".to_string()].into(), lbs: [4].into(), ..Default::default() }),
            ("Not maxed", "Cards below MLB: more copies would make them stronger", CardFilter { lbs: [0, 1, 2, 3].into(), ..Default::default() }),
            ("Teaches a gold", "Cards that can teach a gold skill, through a hint or an event", CardFilter { gold_only: true, ..Default::default() }),
        ];
        kit::quick_views(ui, "cards", &mut self.cards.filter, &builtins);

        let cards = &self.roster.cards;
        let dark = ui.visuals().dark_mode;
        let _ = dark;
        let f = &mut self.cards.filter;
        let types: Vec<_> = CardType::ALL
            .iter()
            .map(|t| (t.label(), format!("{} {}", type_icon(t.label()), t.label()), cards.iter().filter(|c| c.card_type == *t).count()))
            .filter(|(l, _, n)| *n > 0 || f.types.contains(*l))
            .map(|(l, t, n)| (l, t, Some(n), None, String::new()))
            .collect();
        let rarities: Vec<_> = [Rarity::SSR, Rarity::SR, Rarity::R]
            .iter()
            .map(|r| (r.label(), r.label().to_string(), cards.iter().filter(|c| c.rarity == *r).count()))
            .filter(|(l, _, n)| *n > 0 || f.rarities.contains(*l))
            .map(|(l, t, n)| (l, t, Some(n), None, String::new()))
            .collect();
        let n = f.types.len() + f.rarities.len();
        if FilterSection::new("Type and rarity").help("Which stat the card trains (Pal and Group cards work differently), and its rarity: SSR is the strongest, then SR, then R.").active(n).show(ui, |ui| {
            kit::chip_set_str(ui, &mut f.types, &types);
            ui.add_space(4.0);
            kit::chip_set_str(ui, &mut f.rarities, &rarities);
        }) {
            f.types.clear();
            f.rarities.clear();
        }
        let lbs: Vec<_> = (0..=4u8)
            .map(|lb| {
                let label = if lb == 4 { "MLB".to_string() } else { format!("LB{lb}") };
                let hover = if lb == 4 { "Max limit break (LB4): the card at full strength".to_string() } else { format!("Limit break {lb} of 4") };
                (lb, label, Some(cards.iter().filter(|c| c.lb.min(4) == lb).count()), None, hover)
            })
            .collect();
        if FilterSection::new("Limit break")
            .help("Merging copies of a card raises its level cap: LB0 to LB4. LB4, called MLB (max limit break), is the card at full strength. The diamonds under each card show its LB.")
            .active(f.lbs.len() + usize::from(f.below_cap))
            .show(ui, |ui| {
                kit::chip_set(ui, &mut f.lbs, &lbs);
                ui.add_space(4.0);
                kit::toggle_row(ui, &mut f.below_cap, "Below level cap", "Cards you could still level up at their current limit break");
            })
        {
            f.lbs.clear();
            f.below_cap = false;
        }
        let teach_n = f.teaches.len() + usize::from(f.gold_only);
        let mut clear_teach = false;
        let mut add_skill = None;
        let mut remove_skill = None;
        clear_teach |= FilterSection::new("Teaches…").help("Cards that can teach any of these skills, through a hint or an event. Type part of a skill's name and pick it.").active(teach_n).show(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.cards.teach_query).hint_text("💡 Type a skill name").desired_width(f32::INFINITY));
            let q = self.cards.teach_query.trim().to_lowercase();
            if !q.is_empty() {
                let mut found: Vec<(u32, usize)> = Vec::new();
                for c in cards {
                    let Some(info) = self.kb.cards.get(&c.id) else { continue };
                    for s in info.hints.iter().chain(&info.event_skills) {
                        let Some(k) = self.kb.skills.get(s) else { continue };
                        if f.teaches.contains(s) || (f.gold_only && k.tier() != "Gold") || !k.name.to_lowercase().contains(&q) {
                            continue;
                        }
                        match found.iter_mut().find(|(id, _)| id == s) {
                            Some(e) => e.1 += 1,
                            None => found.push((*s, 1)),
                        }
                    }
                }
                found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| self.kb.skill_name(a.0).cmp(&self.kb.skill_name(b.0))));
                if found.is_empty() {
                    ui.label(RichText::new("None of your cards teach a skill like that.").size(size::SM).color(pal(ui).weak));
                }
                for (s, n) in found.iter().take(8) {
                    let color = crate::ui_util::tier_color(ui, self.kb.skills.get(s).map_or("White", |k| k.tier()));
                    let text = RichText::new(format!("＋ {}  ({n} card{})", self.kb.skill_name(*s), if *n == 1 { "" } else { "s" })).color(color);
                    if ui.add(egui::Button::new(text).frame(false)).clicked() {
                        add_skill = Some(*s);
                    }
                }
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for s in &f.teaches {
                    if crate::ui_util::chip(ui, true, &format!("{}  ✕", self.kb.skill_name(*s)), None).on_hover_text("Stop filtering by this skill").clicked() {
                        remove_skill = Some(*s);
                    }
                }
            });
            if f.teaches.len() > 1 {
                ui.label(RichText::new("Cards that teach any of these.").size(size::XS).color(pal(ui).weak));
            }
            kit::toggle_row(ui, &mut f.gold_only, "Gold skills only", "Only cards that can teach a gold skill; the box above then suggests gold skills only");
        });
        if let Some(s) = add_skill {
            f.teaches.push(s);
            self.cards.teach_query.clear();
        }
        if let Some(s) = remove_skill {
            f.teaches.retain(|x| *x != s);
        }
        if clear_teach {
            f.teaches.clear();
            f.gold_only = false;
        }
        let mut drop = None;
        if FilterSection::new("Effect minimums")
            .help("Cards whose effect, at the level you own them, is at least this much. For example Friendship ≥ 25.")
            .active(f.minimums.len())
            .show(ui, |ui| {
                for (i, (kind, min)) in f.minimums.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("fx_min", i)).width(112.0).selected_text(short_effect(fx_name(*kind))).show_ui(ui, |ui| {
                            for fx in FX {
                                ui.selectable_value(kind, fx.0, fx.3);
                            }
                        });
                        ui.label("≥");
                        ui.add(egui::DragValue::new(min).range(0..=200).speed(1.0));
                        if ui.small_btn("✕").on_hover_text("Remove this minimum").clicked() {
                            drop = Some(i);
                        }
                    });
                }
                let b = egui::Button::new(RichText::new("＋ Add a minimum").size(size::SM).color(pal(ui).link)).frame(false);
                if ui.add(b).clicked() {
                    f.minimums.push((effect::FRIENDSHIP, 20));
                }
            })
        {
            f.minimums.clear();
        }
        if let Some(i) = drop {
            f.minimums.remove(i);
        }
        let scenarios: Vec<(String, usize)> = self
            .kb
            .scenarios
            .as_ref()
            .map(|s| s.scenarios.iter().filter(|s| !s.link.is_empty()).map(|s| (s.name.clone(), s.link.len())).collect())
            .unwrap_or_default();
        if !scenarios.is_empty()
            && FilterSection::new("Scenario link")
                .help("In some training scenarios, cards of certain characters (linked characters) get a bonus, and some teach a gold skill through the scenario's events. From knowledge/scenarios.json.")
                .active(usize::from(f.linked.is_some()))
                .show(ui, |ui| {
                    let current = f.linked.clone().map_or("Any scenario".to_string(), |s| format!("Linked in {s}"));
                    egui::ComboBox::from_id_salt("linked").width(ui.available_width() - 8.0).selected_text(current).show_ui(ui, |ui| {
                        ui.selectable_value(&mut f.linked, None, "Any scenario");
                        for (name, n) in &scenarios {
                            ui.selectable_value(&mut f.linked, Some(name.clone()), format!("Linked in {name} ({n} umas)"));
                        }
                    });
                })
        {
            f.linked = None;
        }
        ui.add_space(12.0);
    }

    fn info(&self, id: u32) -> Option<&CardInfo> {
        self.kb.cards.get(&id)
    }

    fn effect_value(&self, c: &SupportCard, kind: u32) -> i32 {
        self.info(c.id).and_then(|i| i.effect(kind, u32::from(c.level))).unwrap_or(0)
    }

    /// The skills a card can teach, by hint or event.
    fn card_skills(&self, id: u32) -> Vec<u32> {
        self.info(id).map(|i| i.hints.iter().chain(&i.event_skills).copied().collect()).unwrap_or_default()
    }

    fn card_passes(&self, c: &SupportCard, q: &str) -> bool {
        let f = &self.cards.filter;
        let linked = |name: &str| {
            self.kb.scenarios.as_ref().and_then(|s| s.scenarios.iter().find(|s| s.name == name)).is_some_and(|s| s.link.iter().any(|u| *u == c.uma))
        };
        (f.types.is_empty() || f.types.contains(c.card_type.label()))
            && (f.rarities.is_empty() || f.rarities.contains(c.rarity.label()))
            && (f.lbs.is_empty() || f.lbs.contains(&c.lb.min(4)))
            && (!f.below_cap || c.level < c.rarity.max_level(c.lb))
            && (f.teaches.is_empty() || self.card_skills(c.id).iter().any(|s| f.teaches.contains(s)))
            && (!f.gold_only || self.card_skills(c.id).iter().any(|s| self.kb.skills.get(s).is_some_and(|k| k.tier() == "Gold")))
            && f.minimums.iter().all(|(k, min)| self.effect_value(c, *k) >= *min)
            && f.linked.as_deref().is_none_or(linked)
            && (q.is_empty() || [c.title.as_str(), c.uma.as_str(), c.notes.as_str(), c.card_type.label()].iter().any(|t| t.to_lowercase().contains(q)))
    }

    /// What a column sorts by.
    pub(crate) fn card_sort_key(&self, c: &SupportCard, key: &str) -> SortKey {
        let num = SortKey::Num;
        let (rr, lb) = (rarity_rank(c.rarity), f64::from(c.lb));
        match key {
            "type" => num(c.card_type as u8 as f64 * 100.0 + (3.0 - rr) * 10.0 + (4.0 - lb)),
            "rarity" => num(rr * 10.0 + lb),
            "lb" => num(lb * 10.0 + rr),
            "level" => num(f64::from(c.level) * 10.0 + rr),
            "uma" => SortKey::Text(c.uma.clone()),
            "title" => SortKey::Text(c.title.clone()),
            "release" => SortKey::Text(self.info(c.id).map(|i| i.release.clone()).unwrap_or_default()),
            "hints" => num(self.info(c.id).map_or(0, |i| i.hints.len()) as f64),
            "notes" => SortKey::Text(c.notes.clone()),
            k => match fx_of(k) {
                Some(fx) => num(f64::from(self.effect_value(c, fx.0))),
                None => SortKey::Text(String::new()),
            },
        }
    }

    pub(crate) fn filtered_cards(&self) -> Vec<usize> {
        let q = self.cards.search.trim().to_lowercase();
        let cards = &self.roster.cards;
        let mut idx: Vec<usize> = (0..cards.len()).filter(|&i| self.card_passes(&cards[i], &q)).collect();
        idx.sort_by(|&a, &b| cards[a].uma.cmp(&cards[b].uma).then(cards[a].id.cmp(&cards[b].id)));
        let sort = self.cards.sort;
        sort_rows(&mut idx, sort.desc, |&i| self.card_sort_key(&cards[i], sort.key));
        idx
    }

    fn card_grid(&mut self, ui: &mut egui::Ui, list: &[usize]) {
        let scale = kit::tile_size(ui.ctx()).scale();
        let (w, h) = (CARD_W * scale, CARD_H * scale);
        let fx = fx_of(self.cards.sort.key);
        let tile = vec2(w, h + if fx.is_some() { 50.0 } else { 36.0 });
        let key = self.cards.sort.key;
        let group_of = |c: &SupportCard| -> Option<u8> {
            match key {
                "type" => Some(c.card_type as u8),
                "rarity" => Some(rarity_rank(c.rarity) as u8),
                "lb" => Some(c.lb.min(4)),
                _ => None,
            }
        };
        let mut groups: Vec<(Option<u8>, Vec<usize>)> = Vec::new();
        for &i in list {
            let g = group_of(&self.roster.cards[i]);
            match groups.last_mut() {
                Some((k, v)) if *k == g => v.push(i),
                _ => groups.push((g, vec![i])),
            }
        }
        let p = pal(ui);
        let heads: Vec<TileGroup> = groups
            .iter()
            .map(|(g, v)| {
                let first = &self.roster.cards[v[0]];
                let (title, color) = match (key, g) {
                    ("type", Some(_)) => (format!("{} {}", type_icon(first.card_type.label()), first.card_type.label()), type_text(ui, first.card_type.label())),
                    ("rarity", Some(_)) => (first.rarity.label().to_string(), p.gold),
                    ("lb", Some(4)) => ("MLB · max limit break".to_string(), p.text),
                    ("lb", Some(lb)) => (format!("LB{lb}"), p.text),
                    _ => (String::new(), p.text),
                };
                TileGroup { title, color, note: format!("{} card{}", v.len(), if v.len() == 1 { "" } else { "s" }), len: v.len() }
            })
            .collect();
        let mut clicked = None;
        let mut action = None;
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if let Some(y) = self.debug_scroll {
            scroll = scroll.vertical_scroll_offset(y);
        }
        scroll.show(ui, |ui| {
            kit::grouped_tiles(ui, &heads, tile, |ui, g, k| {
                let card = &self.roster.cards[groups[g].1[k]];
                let (resp, act) = self.card_tile(ui, card, vec2(w, h), tile);
                match act {
                    Some(a) => action = Some((a, card.id)),
                    None if resp.clicked() => clicked = Some(card.id),
                    None => {}
                }
            });
            ui.add_space(24.0);
        });
        if let Some(id) = clicked {
            self.cards.selected = (self.cards.selected != Some(id)).then_some(id);
        }
        match action {
            Some((CardAction::UseInPlan, id)) => self.use_card_in_plan(id),
            Some((CardAction::Compare, id)) => self.toggle_compare(id),
            Some((CardAction::Details, id)) => self.cards.selected = Some(id),
            None => {}
        }
    }

    /// One picture tile. Returns its response and the quick action clicked, if any.
    fn card_tile(&self, ui: &mut egui::Ui, card: &SupportCard, art: egui::Vec2, tile: egui::Vec2) -> (Response, Option<CardAction>) {
        let fx = fx_of(self.cards.sort.key);
        let (rect, resp) = ui.allocate_exact_size(tile, Sense::click());
        let img = Rect::from_min_size(rect.min, art);
        let plan_at = img.right_top() + vec2(-14.0, 14.0);
        let compare_at = plan_at + vec2(0.0, 28.0);
        let mut action = None;
        let comparing = self.cards.compare.contains(&card.id);
        if ui.is_rect_visible(rect) {
            match self.art_uri("cards", card.id) {
                Some(uri) => crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same(8)), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))),
                None => placeholder(ui.painter(), img, type_color(card.card_type), &card.uma),
            }
            let p = pal(ui);
            let y = img.bottom() + 11.0;
            draw_lb(ui.painter(), Pos2::new(rect.left() + 2.0, y), card.lb, 6.0);
            ui.painter().text(Pos2::new(rect.right() - 2.0, y), Align2::RIGHT_CENTER, format!("Lv {}", card.level), FontId::proportional(12.0), p.weak);
            ui.painter().text(Pos2::new(rect.center().x, y + 16.0), Align2::CENTER_CENTER, truncate(&card.uma, (16.0 * art.x / CARD_W) as usize), FontId::proportional(12.5), p.text);
            if let Some(fx) = fx {
                let v = self.effect_value(card, fx.0);
                let text = if v == 0 { format!("{} —", fx.2) } else { format!("{} {v}{}", fx.2, fx.4) };
                ui.painter().text(Pos2::new(rect.center().x, y + 31.0), Align2::CENTER_CENTER, text, FontId::proportional(11.5), p.link);
            }
            tile_ring(ui, img, self.cards.selected == Some(card.id), resp.hovered(), 10);
            if comparing {
                kit::mark_ring(ui, img, p.link, 10);
                kit::corner_button(ui, compare_at, "⇄", true);
            }
            if resp.hovered() {
                let (hot_plan, hot_cmp) = (kit::over(ui, plan_at), kit::over(ui, compare_at));
                kit::corner_button(ui, plan_at, "📋", hot_plan);
                kit::corner_button(ui, compare_at, "⇄", hot_cmp || comparing);
                if resp.clicked() {
                    action = if hot_plan {
                        Some(CardAction::UseInPlan)
                    } else if hot_cmp {
                        Some(CardAction::Compare)
                    } else {
                        None
                    };
                }
            }
        }
        let resp = if resp.hovered() && kit::over(ui, plan_at) {
            resp.on_hover_text("Use in the current plan's deck")
        } else if resp.hovered() && kit::over(ui, compare_at) {
            resp.on_hover_text(if comparing { "Stop comparing this card" } else { "Compare: pick two cards to see their effects side by side" })
        } else {
            resp.on_hover_ui(|ui| self.card_hover_card(ui, card))
        };
        resp.context_menu(|ui| {
            if ui.button("📋 Use in the current plan").clicked() {
                action = Some(CardAction::UseInPlan);
                ui.close();
            }
            if ui.button(if comparing { "⇄ Stop comparing" } else { "⇄ Compare with another card" }).clicked() {
                action = Some(CardAction::Compare);
                ui.close();
            }
            if ui.button("🔍 Open details").clicked() {
                action = Some(CardAction::Details);
                ui.close();
            }
        });
        (resp, action)
    }

    /// The hover card on a picture tile: the numbers that matter at a glance.
    fn card_hover_card(&self, ui: &mut egui::Ui, card: &SupportCard) {
        let p = pal(ui);
        ui.set_max_width(310.0);
        ui.label(semibold(&card.uma, size::MD));
        ui.label(RichText::new(format!("[{}]", card.title)).italics().color(p.weak));
        let lb = if card.is_mlb() { "MLB".to_string() } else { format!("LB{}", card.lb) };
        ui.label(format!("{} {} · {lb} · level {} of {}", card.rarity.label(), card.card_type.label(), card.level, card.rarity.max_level(card.lb)));
        let parts: Vec<String> = FX
            .iter()
            .filter(|f| f.5)
            .filter_map(|f| {
                let v = self.effect_value(card, f.0);
                (v != 0).then(|| format!("{} {v}{}", f.2, f.4))
            })
            .collect();
        if !parts.is_empty() {
            ui.label(RichText::new(parts.join(" · ")).size(size::SM));
        }
        if let Some(info) = self.info(card.id) {
            let skills = self.card_skills(card.id);
            let golds = skills.iter().filter(|s| self.kb.skills.get(s).is_some_and(|k| k.tier() == "Gold")).count();
            ui.label(RichText::new(format!("Teaches {} skills{}", skills.len(), if golds > 0 { format!(" ({golds} gold)") } else { String::new() })).size(size::SM).color(p.weak));
            if let Some(u) = &info.unique
                && u32::from(card.level) >= u.level
            {
                let fx: Vec<String> = u.effects.iter().map(|e| format!("{} {}", e.name, e.value)).collect();
                ui.label(RichText::new(format!("Unique effect: {}", fx.join(", "))).size(size::SM).color(p.weak));
            }
        }
        ui.label(RichText::new("Click for details · right-click for more").size(size::XS).color(p.weak));
    }

    fn card_cols(&self) -> Vec<TableCol> {
        let mut cols = vec![
            TableCol::new("art", "").width(Column::exact(34.0)),
            TableCol::new("uma", "Uma").width(Column::initial(140.0).at_least(100.0).clip(true)).sort_asc(),
            TableCol::new("title", "Card").width(Column::initial(180.0).at_least(100.0).clip(true)).sort_asc().hover("The card's title"),
            TableCol::new("rarity", "Rarity").width(Column::exact(66.0)).center().sort_desc().hover("SSR is the strongest, then SR, then R"),
            TableCol::new("type", "Type").width(Column::exact(100.0)).sort_asc().hover("Which stat the card trains"),
            TableCol::new("lb", "LB").width(Column::exact(76.0)).center().sort_desc().hover("Limit break: LB0 to LB4 (MLB, full strength)"),
            TableCol::new("level", "Level").width(Column::exact(80.0)).center().sort_desc().hover("Level, of the cap at this limit break"),
        ];
        for f in FX {
            let mut c = TableCol::new(f.1, f.2)
                .width(Column::exact(if f.5 { 92.0 } else { 86.0 }))
                .right()
                .sort_desc()
                .hover(format!("{}{}, at the level you own the card", f.3, if f.4.is_empty() { String::new() } else { format!(" ({})", f.4) }));
            if !f.5 {
                c = c.optional(true);
            }
            cols.push(c);
        }
        cols.push(TableCol::new("hints", "Hints").width(Column::exact(64.0)).right().sort_desc().optional(true).hover("How many skills it can offer as hints"));
        cols.push(TableCol::new("unique", "Unique effect").width(Column::initial(200.0).clip(true)).optional(true).hover("The extra effect only this card has, and the level it starts at"));
        if self.roster.cards.iter().any(|c| !c.notes.trim().is_empty()) {
            cols.push(TableCol::new("notes", "Notes").width(Column::remainder().at_least(100.0).clip(true)).sort_asc());
        }
        cols
    }

    fn unique_text(&self, c: &SupportCard) -> String {
        self.info(c.id)
            .and_then(|i| i.unique.as_ref())
            .map(|u| format!("Lv {}: {}", u.level, u.effects.iter().map(|e| format!("{} {}", e.name, e.value)).collect::<Vec<_>>().join(", ")))
            .unwrap_or_default()
    }

    /// A cell as text, for Copy as CSV.
    fn card_text(&self, c: &SupportCard, key: &str) -> String {
        match key {
            "uma" => c.uma.clone(),
            "title" => c.title.clone(),
            "rarity" => c.rarity.label().to_string(),
            "type" => c.card_type.label().to_string(),
            "lb" => if c.is_mlb() { "MLB".to_string() } else { format!("LB{}", c.lb) },
            "level" => format!("{} / {}", c.level, c.rarity.max_level(c.lb)),
            "hints" => self.info(c.id).map_or(0, |i| i.hints.len()).to_string(),
            "unique" => self.unique_text(c),
            "notes" => c.notes.clone(),
            k => fx_of(k).map(|f| self.effect_value(c, f.0).to_string()).unwrap_or_default(),
        }
    }

    fn card_table(&mut self, ui: &mut egui::Ui, list: &[usize]) {
        let cols = self.card_cols();
        let selected = self.cards.selected.and_then(|id| list.iter().position(|&i| self.roster.cards[i].id == id));
        // Each effect column's highest value, for the heat tint.
        let max: Vec<i32> = FX.iter().map(|f| list.iter().map(|&i| self.effect_value(&self.roster.cards[i], f.0)).max().unwrap_or(0)).collect();
        let dark = ui.visuals().dark_mode;
        let mut sort = self.cards.sort;
        let out = egui::ScrollArea::horizontal()
            .id_salt("card_table_h")
            .show(ui, |ui| {
                DataTable::new("cards", &cols).sort(&mut sort).row_height(44.0).selected(selected).show(ui, list.len(), |ui, r, key| {
                    let card = &self.roster.cards[list[r]];
                    let p = pal(ui);
                    match key {
                        "art" => match self.art_uri("cards", card.id) {
                            Some(uri) => {
                                ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(28.0, 37.0)).corner_radius(CornerRadius::same(4)));
                            }
                            None => {
                                let (rect, _) = ui.allocate_exact_size(vec2(28.0, 37.0), Sense::hover());
                                placeholder(ui.painter(), rect, type_color(card.card_type), "");
                            }
                        },
                        "uma" => {
                            ui.add(egui::Label::new(&card.uma).truncate());
                        }
                        "title" => {
                            ui.add(egui::Label::new(RichText::new(&card.title).color(p.weak)).truncate()).on_hover_text(&card.title);
                        }
                        "rarity" => {
                            badge(ui, card.rarity.label(), if card.rarity == Rarity::SSR { p.gold } else { p.weak });
                        }
                        "type" => {
                            ui.label(RichText::new(format!("{} {}", type_icon(card.card_type.label()), card.card_type.label())).color(type_text(ui, card.card_type.label())));
                        }
                        "lb" => {
                            lb_widget(ui, card.lb, 5.0).on_hover_text(if card.is_mlb() { "Max limit break (MLB)".to_string() } else { format!("Limit break {} of 4", card.lb) });
                        }
                        "level" => {
                            let cap = card.rarity.max_level(card.lb);
                            let (rect, resp) = ui.allocate_exact_size(vec2(64.0, 30.0), Sense::hover());
                            let painter = ui.painter();
                            painter.text(rect.center() - vec2(0.0, 4.0), Align2::CENTER_CENTER, format!("{} / {cap}", card.level), FontId::proportional(size::BODY), p.text);
                            let bar = Rect::from_min_size(pos2(rect.left() + 4.0, rect.bottom() - 6.0), vec2(rect.width() - 8.0, 3.0));
                            painter.rect_filled(bar, CornerRadius::same(2), p.border);
                            let t = f32::from(card.level) / f32::from(cap.max(1));
                            painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * t.min(1.0), 3.0)), CornerRadius::same(2), if card.level >= cap { p.accent } else { p.warn });
                            if card.level < cap {
                                resp.on_hover_text(format!("{} levels below the cap at this limit break", cap - card.level));
                            }
                        }
                        "hints" => {
                            ui.label(self.info(card.id).map_or(0, |i| i.hints.len()).to_string());
                        }
                        "unique" => {
                            let t = self.unique_text(card);
                            let active = self.info(card.id).and_then(|i| i.unique.as_ref()).is_some_and(|u| u32::from(card.level) >= u.level);
                            ui.add(egui::Label::new(RichText::new(&t).size(size::SM).color(if active { p.text } else { p.weak })).truncate()).on_hover_text(if active { t.clone() } else { format!("{t}\nNot active yet at your level") });
                        }
                        "notes" => {
                            ui.add(egui::Label::new(RichText::new(&card.notes).color(p.weak)).truncate());
                        }
                        k => {
                            if let Some(n) = FX.iter().position(|f| f.1 == k) {
                                let f = FX[n];
                                let v = self.effect_value(card, f.0);
                                if v == 0 {
                                    ui.label(RichText::new("—").color(p.weak));
                                } else {
                                    let t = v as f32 / max[n].max(1) as f32;
                                    let cell = ui.max_rect().shrink2(vec2(4.0, 9.0));
                                    ui.painter().rect_filled(cell, CornerRadius::same(4), crate::charts::sequential(t, dark).gamma_multiply(0.06 + 0.24 * t));
                                    ui.label(semibold(format!("{v}{}", f.4), size::BODY));
                                }
                            }
                        }
                    }
                })
            })
            .inner;
        self.cards.sort = sort;
        if let Some(r) = out.clicked {
            let id = self.roster.cards[list[r]].id;
            self.cards.selected = (self.cards.selected != Some(id)).then_some(id);
        }
    }

    // -----------------------------------------------------------------------
    // Quick actions

    /// Put an owned card in the open plan's deck: the first empty slot.
    pub(crate) fn use_card_in_plan(&mut self, id: u32) {
        use crate::ui_util::ToastKind;
        let Some(card) = self.owned_card(id).cloned() else { return };
        let plan = &self.planner.plan;
        let trainee = plan.trainee.and_then(|t| self.kb.umas.get(&t)).map(|u| u.name.clone());
        if plan.deck.contains(&Some(id)) {
            self.toast(format!("{} is already in your plan's deck.", card.display_name()), ToastKind::Info);
        } else if trainee.as_deref() == Some(card.uma.as_str()) {
            self.toast(format!("Can't: {} is the trainee, and her own card can't go in her deck.", card.uma), ToastKind::Warn);
        } else if plan.deck.iter().flatten().any(|d| self.kb.cards.get(d).is_some_and(|i| i.uma == card.uma)) {
            self.toast(format!("Your plan's deck already has a {} card; a deck can't hold two cards of the same uma.", card.uma), ToastKind::Warn);
        } else if let Some(slot) = plan.deck.iter().position(Option::is_none) {
            self.planner.plan.deck[slot] = Some(id);
            self.toast(format!("Added {} to slot {} of your plan's deck. Career → Plan shows it.", card.display_name(), slot + 1), ToastKind::Success);
        } else {
            self.toast("Your plan's deck is full. Open Career → Plan to swap a card.", ToastKind::Warn);
        }
    }

    /// Pick or unpick a card to compare; the window opens at two.
    pub(crate) fn toggle_compare(&mut self, id: u32) {
        let c = &mut self.cards.compare;
        if let Some(i) = c.iter().position(|x| *x == id) {
            c.remove(i);
            self.cards.compare_open = false;
        } else {
            if c.len() >= 2 {
                c.truncate(1);
            }
            c.push(id);
            self.cards.compare_open = c.len() == 2;
        }
    }

    /// The bar above the cards while one or two are picked to compare.
    fn compare_tray(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let names: Vec<String> = self.cards.compare.iter().filter_map(|id| self.owned_card(*id)).map(|c| c.uma.clone()).collect();
        let mut cancel = false;
        let mut open = false;
        egui::Frame::new().fill(p.link.gamma_multiply(0.12)).stroke(Stroke::new(1.0, p.link.gamma_multiply(0.6))).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(semibold("⇄ Compare", size::BODY));
                if names.len() == 1 {
                    ui.label(format!("{} with… pick another card's ⇄ (hover a tile), or right-click it.", names[0]));
                } else {
                    ui.label(names.join("  vs  "));
                    if ui.button("Show").clicked() {
                        open = true;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✕ Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });
        ui.add_space(8.0);
        if cancel {
            self.cards.compare.clear();
            self.cards.compare_open = false;
        }
        if open {
            self.cards.compare_open = true;
        }
    }

    /// Two cards side by side: every effect at the level you own them, hints and unique effects.
    fn compare_window(&mut self, ctx: &egui::Context) {
        let ids = [self.cards.compare[0], self.cards.compare[1]];
        let (Some(a), Some(b)) = (self.owned_card(ids[0]).cloned(), self.owned_card(ids[1]).cloned()) else {
            self.cards.compare.clear();
            return;
        };
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("compare_cards")).show(ctx, |ui| {
            let p = pal(ui);
            ui.set_width(620.0);
            ui.horizontal(|ui| {
                ui.label(semibold("Compare cards", size::XL));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✖ Close").clicked() {
                        close = true;
                    }
                });
            });
            ui.label(RichText::new("Effects at the level you own each card. The higher value is bold.").size(size::SM).color(p.weak));
            ui.add_space(8.0);
            egui::Grid::new("compare_grid").num_columns(3).striped(true).spacing([18.0, 6.0]).min_col_width(150.0).show(ui, |ui| {
                ui.label("");
                for c in [&a, &b] {
                    ui.vertical(|ui| {
                        if let Some(uri) = self.art_uri("cards", c.id) {
                            ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(66.0, 88.0)).corner_radius(CornerRadius::same(6)));
                        }
                        ui.label(semibold(&c.uma, size::BODY));
                        ui.label(RichText::new(format!("[{}]", c.title)).size(size::SM).color(p.weak));
                        ui.label(RichText::new(format!("{} {} · {} · Lv {}", c.rarity.label(), c.card_type.label(), if c.is_mlb() { "MLB".to_string() } else { format!("LB{}", c.lb) }, c.level)).size(size::SM));
                    });
                }
                ui.end_row();
                let mut kinds: Vec<(u32, String)> = Vec::new();
                for c in [&a, &b] {
                    for e in self.info(c.id).map(|i| i.effects.clone()).unwrap_or_default() {
                        if !kinds.iter().any(|(k, _)| *k == e.kind) {
                            kinds.push((e.kind, e.name.clone()));
                        }
                    }
                }
                kinds.sort_by_key(|(k, _)| FX.iter().position(|f| f.0 == *k).unwrap_or(99));
                for (kind, name) in kinds {
                    let va = self.info(a.id).and_then(|i| i.effect(kind, u32::from(a.level))).unwrap_or(0);
                    let vb = self.info(b.id).and_then(|i| i.effect(kind, u32::from(b.level))).unwrap_or(0);
                    if va == 0 && vb == 0 {
                        continue;
                    }
                    let unit = FX.iter().find(|f| f.0 == kind).map_or("", |f| f.4);
                    ui.label(RichText::new(name).color(p.weak));
                    for (v, other) in [(va, vb), (vb, va)] {
                        let text = if v == 0 { "—".to_string() } else { format!("{v}{unit}") };
                        ui.label(if v > other { semibold(text, size::BODY) } else { RichText::new(text).color(if v == 0 { p.weak } else { p.text }) });
                    }
                    ui.end_row();
                }
                ui.label(RichText::new("Skill hints").color(p.weak));
                for c in [&a, &b] {
                    let skills = self.card_skills(c.id);
                    let golds: Vec<String> = skills.iter().filter(|s| self.kb.skills.get(s).is_some_and(|k| k.tier() == "Gold")).map(|s| self.kb.skill_name(*s)).collect();
                    ui.label(format!("{} skills{}", skills.len(), if golds.is_empty() { String::new() } else { format!("\nGold: {}", golds.join(", ")) }));
                }
                ui.end_row();
                ui.label(RichText::new("Unique effect").color(p.weak));
                for c in [&a, &b] {
                    let t = self.unique_text(c);
                    ui.label(RichText::new(if t.is_empty() { "—".to_string() } else { t }).size(size::SM));
                }
                ui.end_row();
            });
        });
        if close || modal.should_close() {
            self.cards.compare.clear();
            self.cards.compare_open = false;
        }
    }

    // -----------------------------------------------------------------------
    // Add cards

    /// Modal: every Global card as art tiles, newest first, searchable. Click one to set its LB and
    /// level on the right, or click a diamond on a tile to save it at that limit break at once.
    fn add_cards_dialog(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let (w, h) = ((screen.width() * 0.92).min(1180.0), (screen.height() * 0.8).max(360.0));
        let mut close = false;
        let mut save = false;
        let mut quick: Option<(u32, u8)> = None;
        let mut need_art: Vec<u32> = Vec::new();
        let mut a = std::mem::take(&mut self.cards.add);
        let modal = egui::Modal::new(egui::Id::unique("add_cards")).show(&ctx, |ui| {
            let p = pal(ui);
            ui.set_width(w);
            ui.horizontal(|ui| {
                ui.label(semibold("Add cards", size::XL));
                ui.label(RichText::new("Click a card to set its limit break and level, or click a diamond on a card to save it at that LB.").size(size::SM).color(p.weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✖ Close").clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                ui.add_sized([260.0, 24.0], egui::TextEdit::singleline(&mut a.search).hint_text("🔍 Search uma, title or skill hint"));
                if crate::ui_util::chip(ui, a.kind.is_none(), "All types", None).clicked() {
                    a.kind = None;
                }
                for t in ["Speed", "Stamina", "Power", "Guts", "Wit", "Pal", "Group"] {
                    let on = a.kind.as_deref() == Some(t);
                    if kit::dot_chip(ui, on, &format!("{} {t}", type_icon(t)), None, Some(type_color_str(t))).clicked() {
                        a.kind = if on { None } else { Some(t.to_string()) };
                    }
                }
                ui.tick(&mut a.owned_only, "Owned only").on_hover_text("Only cards you have, to raise their LB or level");
                ui.tick(&mut a.upcoming, "Not on Global yet").on_hover_text("uma.guide lists some cards ahead of their Global release");
            });
            ui.add_space(6.0);
            let q = a.search.trim().to_lowercase();
            let today = crate::ui_util::today();
            let mut items: Vec<&CardInfo> = self
                .kb
                .cards
                .values()
                .filter(|c| a.upcoming || c.released(&today))
                .filter(|c| a.kind.as_ref().is_none_or(|k| *k == c.kind))
                .filter(|c| !a.owned_only || self.owned_card(c.id).is_some())
                .filter(|c| {
                    q.is_empty()
                        || crate::ui_util::matches_query(&format!("{} {} {} {}", c.uma, c.title, c.rarity, c.kind), &q)
                        || c.hints.iter().any(|h| self.kb.skill_name(*h).to_lowercase().contains(&q))
                })
                .collect();
            items.sort_by(|x, y| y.release.cmp(&x.release).then(y.id.cmp(&x.id)));
            let mut picked = None;
            let grid_h = (h - 90.0).max(260.0);
            ui.horizontal_top(|ui| {
                let left_w = ui.available_width() - 300.0;
                ui.allocate_ui_with_layout(vec2(left_w, grid_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.label(RichText::new(format!("{} cards, newest first", items.len())).size(size::SM).color(p.weak));
                    egui::ScrollArea::vertical().id_salt("add_grid").max_height(grid_h - 20.0).auto_shrink([false, false]).show(ui, |ui| {
                        crate::ui_util::tile_grid(ui, items.len(), ADD_TILE, 10.0, 10.0, |ui, k| {
                            let c = items[k];
                            let (resp, lb) = self.add_tile(ui, c, a.pick == Some(c.id));
                            if ui.is_rect_visible(resp.rect) && !self.card_art.contains(&c.id) {
                                need_art.push(c.id);
                            }
                            match lb {
                                Some(lb) => quick = Some((c.id, lb)),
                                None if resp.clicked() => picked = Some(c.id),
                                None => {}
                            }
                        });
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(280.0);
                    self.add_editor(ui, &mut a, &mut save);
                });
            });
            if let Some(id) = picked {
                self.pick_add_card(&mut a, id);
            }
        });
        if let Some((id, lb)) = quick {
            a.pick = Some(id);
            a.lb = lb;
            a.level = Rarity::from_id(id).map_or(1, |r| r.max_level(lb));
            a.notes = self.owned_card(id).map(|c| c.notes.clone()).unwrap_or_default();
            save = true;
        }
        self.cards.add = a;
        self.queue_art(need_art);
        if save {
            let result = self.save_added_card();
            self.toast_flag(match result {
                Ok(msg) => (msg, false),
                Err(e) => (e, true),
            });
        }
        if close || (modal.should_close() && !save) {
            self.cards.add.open = false;
        }
    }

    /// Pick a card in the Add cards dialog: start from what's owned, or a fresh copy at LB0.
    pub(crate) fn pick_add_card(&self, a: &mut AddCard, id: u32) {
        a.pick = Some(id);
        a.notes.clear();
        match self.owned_card(id) {
            Some(o) => {
                a.lb = o.lb;
                a.level = o.level;
                a.notes = o.notes.clone();
            }
            None => {
                a.lb = 0;
                a.level = Rarity::from_id(id).map_or(1, |r| r.max_level(0));
            }
        }
    }

    /// A card tile in the Add cards dialog. Returns its response, and the LB of a diamond clicked.
    fn add_tile(&self, ui: &mut egui::Ui, c: &CardInfo, picked: bool) -> (Response, Option<u8>) {
        let p = pal(ui);
        let owned = self.owned_card(c.id);
        let (rect, resp) = ui.allocate_exact_size(ADD_TILE, Sense::click());
        let img = Rect::from_min_size(rect.min, vec2(ADD_W, ADD_H));
        let band = Rect::from_min_max(pos2(img.left(), img.bottom() - 24.0), img.right_bottom());
        let slot = (ADD_W - 8.0) / 4.0;
        let hit = |pos: Pos2| band.contains(pos).then(|| (((pos.x - img.left() - 4.0) / slot).floor().clamp(0.0, 3.0)) as u8 + 1);
        let mut lb = None;
        if resp.clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            lb = hit(pos);
        }
        if ui.is_rect_visible(rect) {
            let alpha = if owned.is_some() { 255 } else { 150 };
            match self.art_uri("cards", c.id) {
                Some(uri) => crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same(6)).tint(Color32::from_white_alpha(alpha)), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))),
                None => placeholder(ui.painter(), img, type_color_str(&c.kind), &c.uma),
            }
            let painter = ui.painter();
            if let Some(o) = owned {
                painter.circle_filled(img.left_top() + vec2(11.0, 11.0), 8.0, p.good);
                painter.text(img.left_top() + vec2(11.0, 11.0), Align2::CENTER_CENTER, "✔", FontId::proportional(10.0), Color32::WHITE);
                draw_lb(painter, pos2(rect.left() + 1.0, img.bottom() + 9.0), o.lb, 4.5);
                painter.text(pos2(rect.right() - 1.0, img.bottom() + 9.0), Align2::RIGHT_CENTER, format!("Lv{}", o.level), FontId::proportional(10.5), p.weak);
            } else {
                painter.text(pos2(rect.center().x, img.bottom() + 9.0), Align2::CENTER_CENTER, format!("{} · {}", c.rarity, c.release.get(..7).unwrap_or("")), FontId::proportional(10.5), p.weak);
            }
            painter.text(pos2(img.center().x, img.bottom() + 24.0), Align2::CENTER_CENTER, truncate(&c.uma, 13), FontId::proportional(11.5), if owned.is_some() { p.text } else { p.weak });
            if resp.hovered() {
                painter.rect_filled(band, CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 }, Color32::from_black_alpha(185));
                let over = ui.input(|i| i.pointer.hover_pos()).and_then(hit);
                draw_lb(painter, pos2(img.left() + 4.0 + slot / 2.0 - 5.5, band.center().y), over.unwrap_or(owned.map_or(0, |o| o.lb)), 5.5);
            }
            if picked {
                kit::mark_ring(ui, img, p.accent, 8);
            }
        }
        let have = owned.map_or("Not owned".to_string(), |o| format!("Owned: LB{} · Lv {}", o.lb, o.level));
        let resp = resp.on_hover_text(format!("{} {} [{}]\n{} card · released {}\n{have}\nClick to edit · click a diamond to save at that LB", c.rarity, c.uma, c.title, c.kind, c.release));
        (resp, lb)
    }

    /// The Add cards dialog's editor for the picked card.
    fn add_editor(&self, ui: &mut egui::Ui, a: &mut AddCard, save: &mut bool) {
        let p = pal(ui);
        let Some(id) = a.pick else {
            ui.add_space(30.0);
            ui.label(RichText::new("Pick a card on the left.").color(p.weak));
            return;
        };
        let Some(c) = self.kb.cards.get(&id) else { return };
        let rarity = Rarity::from_id(id).unwrap_or(Rarity::SSR);
        match self.art_uri("cards", id) {
            Some(uri) => {
                ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(108.0, 144.0)).corner_radius(CornerRadius::same(8)));
            }
            None => {
                let (r, _) = ui.allocate_exact_size(vec2(108.0, 144.0), Sense::hover());
                placeholder(ui.painter(), r, type_color_str(&c.kind), &c.uma);
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(semibold(&c.uma, size::LG));
            if let Some(o) = self.owned_card(id) {
                badge(ui, format!("Owned · LB{} Lv{}", o.lb, o.level), p.good);
            }
        });
        ui.label(RichText::new(format!("[{}]", c.title)).italics().color(p.weak));
        ui.label(RichText::new(format!("{} {} · released {}", c.rarity, c.kind, c.release)).size(size::SM));
        ui.add_space(8.0);
        ui.label(RichText::new("Limit break").color(p.weak));
        ui.horizontal_wrapped(|ui| {
            for lb in 0..=4u8 {
                if crate::onboarding_grid::lb_button(ui, lb, a.lb == lb).clicked() {
                    a.lb = lb;
                    a.level = rarity.max_level(lb);
                }
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Level").color(p.weak));
            ui.add(egui::Slider::new(&mut a.level, 1..=rarity.max_level(a.lb)));
        });
        ui.label(RichText::new(format!("The cap at LB{} is {}.", a.lb, rarity.max_level(a.lb))).size(size::XS).color(p.weak));
        ui.add_space(4.0);
        ui.add(egui::TextEdit::singleline(&mut a.notes).hint_text("Notes (optional)").desired_width(f32::INFINITY));
        ui.add_space(8.0);
        let verb = if self.owned_card(id).is_some() { "💾 Update card" } else { "＋ Add to roster" };
        if ui.add(crate::ui_util::primary_button(ui, verb, size::BODY).min_size(vec2(160.0, 32.0))).clicked() {
            *save = true;
        }
    }

    /// Write the picked card into account/support_cards.md, reload the roster, and fetch its art.
    fn save_added_card(&mut self) -> Result<String, String> {
        let root = self.root.clone().ok_or("umaLord folder not found.")?;
        let a = &self.cards.add;
        let id = a.pick.ok_or("Pick a card first.")?;
        let info = self.kb.cards.get(&id).ok_or("No data for that card.")?.clone();
        let card_type = CardType::parse(&info.kind).ok_or_else(|| format!("Unknown card type {}", info.kind))?;
        let row = crate::data::CardRow { id, title: info.title.clone(), uma: info.uma.clone(), card_type, lb: a.lb, level: a.level, notes: a.notes.trim().to_string() };
        let path = root.join(crate::data::CARDS_FILE);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
        let (new, updated) = crate::data::upsert_card(&text, &row, &crate::ui_util::local_today())?;
        std::fs::write(&path, new).map_err(|e| format!("Couldn't write {}: {e}", path.display()))?;
        self.reload_roster();
        self.write_digest();
        self.planner.rank_cache.borrow_mut().clear();
        self.cards.selected = Some(id);
        let lb = if row.lb >= 4 { "MLB".to_string() } else { format!("LB{}", row.lb) };
        let mut msg = format!("{} {} [{}] at {lb} Lv{}.", if updated { "Updated" } else { "Added" }, info.uma, info.title, row.level);
        if !self.card_art.contains(&id) {
            self.queue_art([id]);
            self.cards.art.watch = Some(id);
            msg += " Downloading its art from uma.guide…";
        }
        Ok(msg)
    }

    /// Queue card art to download from uma.guide (not in tests or screenshot runs).
    fn queue_art(&mut self, ids: impl IntoIterator<Item = u32>) {
        self.queue_art_of(Art::Card, ids);
    }

    /// Queue art of one kind to download, skipping what's already there or queued.
    pub(crate) fn queue_art_of(&mut self, kind: Art, ids: impl IntoIterator<Item = u32>) {
        if cfg!(test) || std::env::var_os("UMALORD_SCREENSHOT").is_some() || self.root.is_none() {
            return;
        }
        for id in ids {
            let have = match kind {
                Art::Card => self.card_art.contains(&id),
                Art::Uma => self.uma_art.contains(&id),
                Art::Full => self.full_art.contains(&id),
            };
            if !have && self.cards.art.seen.insert((kind, id)) {
                self.cards.art.pending.push_back((kind, id));
            }
        }
    }

    /// Whether a picture is still on its way: queued or downloading, not yet failed.
    pub(crate) fn art_coming(&self, kind: Art, id: u32) -> bool {
        self.cards.art.seen.contains(&(kind, id)) && !self.cards.art.failed.contains(&(kind, id))
    }

    /// Once per launch, when fetching is allowed (Settings → Data and updates): queue the art of
    /// every released uma and card the knowledge data has and the app doesn't, so nothing shows
    /// as a placeholder once a release reaches the data.
    pub(crate) fn sweep_art(&mut self) {
        if self.cards.art.swept {
            return;
        }
        self.cards.art.swept = true;
        if !crate::live::enabled() || std::env::var("UMALORD_LIVE").is_ok_and(|v| v != "fetch") {
            return;
        }
        let today = crate::ui_util::today();
        let umas: Vec<u32> = self.kb.umas.values().filter(|u| u.release.is_empty() || u.release.as_str() <= today.as_str()).map(|u| u.id).collect();
        let cards: Vec<u32> = self.kb.cards.values().filter(|c| c.released(&today)).map(|c| c.id).collect();
        self.queue_art_of(Art::Uma, umas);
        self.queue_art_of(Art::Card, cards);
    }

    /// Start queued downloads, four at a time, and collect finished ones.
    pub(crate) fn pump_art(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        self.cards.art.running.retain(|rx| match rx.try_recv() {
            Ok(r) => {
                done.push(r);
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        for (kind, id, ok) in done {
            if ok {
                match kind {
                    Art::Card => self.card_art.insert(id),
                    Art::Uma => self.uma_art.insert(id),
                    Art::Full => self.full_art.insert(id),
                };
            } else if {
                self.cards.art.failed.insert((kind, id));
                kind == Art::Card && self.cards.art.watch == Some(id)
            } {
                self.toast("Saved, but the card art couldn't be downloaded; it shows as a placeholder.", crate::ui_util::ToastKind::Error);
            }
            if kind == Art::Card && self.cards.art.watch == Some(id) {
                self.cards.art.watch = None;
            }
        }
        let Some(root) = self.root.clone() else { return };
        while self.cards.art.running.len() < 4 {
            let Some((kind, id)) = self.cards.art.pending.pop_front() else { break };
            let dir = root.join("app").join("assets").join(kind.dir());
            let (tx, rx) = mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = std::fs::create_dir_all(&dir);
                let (ext, format) = kind.ext();
                let path = dir.join(format!("{id}.{ext}"));
                let part = dir.join(format!("{id}.{ext}.part"));
                let mut cmd = std::process::Command::new(crate::ask::CURL);
                cmd.args(["-sSfL", "--max-time", "60", "-o"]).arg(&part).arg(kind.url(id));
                crate::ask::hidden(&mut cmd);
                let ok = cmd.status().is_ok_and(|s| s.success())
                    && std::fs::read(&part).ok().is_some_and(|b| image::load_from_memory_with_format(&b, format).is_ok())
                    && std::fs::rename(&part, &path).is_ok();
                if !ok {
                    let _ = std::fs::remove_file(&part);
                }
                let _ = tx.send((kind, id, ok));
                ctx.request_repaint();
            });
            self.cards.art.running.push(rx);
        }
    }

    fn card_detail(&mut self, ui: &mut egui::Ui, card: &SupportCard) {
        let p = pal(ui);
        let tcolor = type_color(card.card_type);
        ui.add_space(10.0);
        crate::ui_util::card(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(uri) = self.art_uri("cards", card.id) {
                    ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(124.0, 165.0)).corner_radius(CornerRadius::same(10)));
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(semibold(&card.uma, 21.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if ui.small_btn("✖").on_hover_text("Close").clicked() {
                                self.cards.selected = None;
                            }
                        });
                    });
                    ui.label(RichText::new(format!("[{}]", card.title)).italics().color(p.weak));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        badge(ui, card.rarity.label(), p.gold);
                        badge(ui, format!("{} {}", type_icon(card.card_type.label()), card.card_type.label()), tcolor);
                        badge(ui, if card.is_mlb() { "MLB".to_string() } else { format!("LB{}", card.lb) }, if card.is_mlb() { p.accent } else { pal(ui).warn });
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        lb_widget(ui, card.lb, 7.0);
                        if card.lb < 4 {
                            let next = if card.lb == 3 { "MLB".to_string() } else { format!("LB{}", card.lb + 1) };
                            if ui.small_btn(RichText::new("＋").color(p.weak)).on_hover_text(format!("Pulled another copy? Raise it to {next} and save it. Shortcut: +")).clicked() {
                                self.raise_card(card.id);
                            }
                        }
                    });
                    let cap = card.rarity.max_level(card.lb);
                    ui.label(format!("Level {} / {cap}", card.level));
                    if card.level < cap {
                        ui.label(RichText::new(format!("{} levels below this LB's cap", cap - card.level)).color(pal(ui).warn));
                    }
                    if self.roster.umas.iter().any(|u| u.name == card.uma) {
                        ui.label(RichText::new(format!("Not usable when training {}.", card.uma)).size(12.0).color(p.weak));
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("📋 Use in plan").on_hover_text("Put it in the current plan's deck").clicked() {
                            self.use_card_in_plan(card.id);
                        }
                        let comparing = self.cards.compare.contains(&card.id);
                        if ui.button(if comparing { "⇄ Comparing" } else { "⇄ Compare" }).on_hover_text("Pick it, then another card, to see their effects side by side").clicked() {
                            self.toggle_compare(card.id);
                        }
                        if !card.url.is_empty() {
                            web_link(ui, "uma.guide", &card.url);
                        }
                    });
                });
            });
            if !card.notes.is_empty() {
                ui.label(RichText::new(&card.notes).color(p.weak));
            }
        });
        ui.add_space(8.0);

        if self.info(card.id).is_none() {
            ui.label(RichText::new("No knowledge data for this card yet. Rebuild knowledge/data.").color(pal(ui).warn));
            return;
        }
        self.card_sections(ui, card.id, Some(u32::from(card.level)));

        let others: Vec<&SupportCard> = self.roster.cards.iter().filter(|c| c.uma == card.uma && c.id != card.id).collect();
        if !others.is_empty() {
            card_titled(ui, &format!("Other {} cards", card.uma), |ui| {
                for c in others {
                    if ui.link(format!("🃏 {} {} [{}] LB{}", c.rarity.label(), c.card_type.label(), c.title, c.lb)).clicked() {
                        self.jump.set(Some(Jump::Card(c.id)));
                    }
                }
            });
        }
        ui.add_space(10.0);
    }

    /// Everything known about a card, below its header: its effects at `level` (owned) beside
    /// their MLB values, its unique effect, the skills it teaches and its events. Collection's
    /// detail panel and the detail pop-up both draw it.
    pub(crate) fn card_sections(&mut self, ui: &mut egui::Ui, id: u32, level: Option<u32>) {
        let p = pal(ui);
        let Some(info) = self.info(id).cloned() else { return };
        let max_level = u32::from(crate::data::Rarity::from_id(id).unwrap_or(crate::data::Rarity::SSR).max_level(4));

        let explain = if level.is_some() { "What the card adds to training. Green numbers in the last column are what you'd gain by maxing it." } else { "What the card adds to training, fully limit broken (MLB)." };
        // Rows with something at either level: (name, yours, MLB).
        let rows: Vec<(String, Option<i32>, Option<i32>)> = info
            .effects
            .iter()
            .map(|e| (e.name.clone(), level.and_then(|l| effect_at(&e.points, l)), effect_at(&e.points, max_level)))
            .filter(|(_, now, max)| now.unwrap_or(0) != 0 || max.unwrap_or(0) != 0)
            .collect();
        card_explained(ui, "Support effects", explain, |ui| {
            // The table fills the card: two halves side by side when there's room, else one.
            let w = ui.available_width();
            let gap = 48.0;
            // Halves only when each half still fits the longest names (about 165 px, "Initial Friendship Gauge") whole.
            let need = EFFECT_NAME_W + effect_values_w(level);
            let halves = w >= 2.0 * need + gap && rows.len() > 4;
            if halves {
                let mid = rows.len().div_ceil(2);
                let col = (w - gap) / 2.0;
                let r = ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (k, part) in [&rows[..mid], &rows[mid..]].into_iter().enumerate() {
                        ui.allocate_ui_with_layout(vec2(col, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(col);
                            effects_grid(ui, (id, k), part, level, max_level, col);
                        });
                    }
                });
                // A faint rule between the halves, so a row doesn't read across both.
                let x = r.response.rect.left() + col + gap / 2.0;
                ui.painter().vline(x, r.response.rect.y_range(), egui::Stroke::new(1.0, p.border));
            } else {
                effects_grid(ui, (id, 0), &rows, level, max_level, w);
            }
        });

        if let Some(u) = &info.unique {
            card_explained(ui, "Unique effect", "An extra effect only this card has, once it reaches the level shown.", |ui| {
                match level {
                    Some(l) if l >= u.level => badge(ui, format!("Active · from Lv {}", u.level), p.accent),
                    Some(_) => badge(ui, format!("Locked until Lv {}", u.level), pal(ui).warn),
                    None => badge(ui, format!("From Lv {}", u.level), p.weak),
                };
                ui.add_space(4.0);
                if !u.condition.is_empty() {
                    ui.label(RichText::new(format!("{}:", u.condition)).color(p.weak));
                }
                for e in &u.effects {
                    ui.label(format!("• {} {}", e.name, e.value));
                }
            });
        }

        if !info.hints.is_empty() {
            card_explained(ui, &format!("Skill hints ({})", info.hints.len()), "Skills this card can offer at a discount during a career.", |ui| {
                for &s in &info.hints {
                    ui.horizontal(|ui| {
                        self.skill_link(ui, s, false);
                        if let Some(sk) = self.kb.skills.get(&s) {
                            let tags = self.kb.tag_line(&sk.tags());
                            ui.label(RichText::new(format!("{} SP{}", sk.sp, if tags.is_empty() { String::new() } else { format!(" · {tags}") })).size(12.0).color(p.weak));
                        }
                    });
                }
            });
        }
        if !info.event_skills.is_empty() {
            card_titled(ui, "Skills from events", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for &s in &info.event_skills {
                        self.skill_link(ui, s, false);
                    }
                });
            });
        }

        if !info.events.is_empty() {
            card_explained(ui, &format!("Events ({})", info.events.len()), "The card's story events and what each choice gives. Click one to open it.", |ui| {
                for (i, e) in info.events.iter().enumerate() {
                    egui::CollapsingHeader::new(&e.title).id_salt(("card_ev", id, i)).show(ui, |ui| {
                        ui.label(RichText::new(&e.kind).size(11.5).color(p.weak));
                        if e.options.is_empty() {
                            ui.label(RichText::new("No outcome data.").color(p.weak));
                        }
                        for (n, alts) in e.options.iter().enumerate() {
                            if e.options.len() > 1 {
                                ui.label(RichText::new(format!("Option {}", n + 1)).strong());
                            }
                            if alts.len() > 1 {
                                ui.label(RichText::new("One of these, at random (odds not in the data):").size(12.0).color(p.weak));
                            }
                            for lines in alts {
                                ui.label(format!("  • {}", lines.join(", ")));
                            }
                        }
                    });
                }
            });
        }
    }
}


const EFFECT_NAME_W: f32 = 175.0;
const EFFECT_VAL_W: f32 = 92.0;

/// The width a support-effects table's value columns take (with their spacing).
fn effect_values_w(level: Option<u32>) -> f32 {
    (if level.is_some() { 2.0 } else { 1.0 }) * (EFFECT_VAL_W + 12.0) + 8.0
}

/// One support-effects table `width` wide: the effect's name takes what the value columns leave.
fn effects_grid(ui: &mut egui::Ui, salt: (u32, usize), rows: &[(String, Option<i32>, Option<i32>)], level: Option<u32>, max_level: u32, width: f32) {
    let p = pal(ui);
    let (val_w, spacing) = (EFFECT_VAL_W, 12.0);
    let cols = if level.is_some() { 2.0 } else { 1.0 };
    let name_w = (width - effect_values_w(level)).max(120.0);
    let left = |ui: &mut egui::Ui, h: f32, text: RichText, hover: Option<&str>| {
        ui.allocate_ui_with_layout(vec2(name_w, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(name_w);
            let r = ui.add(egui::Label::new(text).truncate());
            if let Some(h) = hover {
                r.on_hover_text(h);
            }
        });
    };
    let dash = |v: Option<i32>| v.map_or("—".to_string(), |v| v.to_string());
    // Numbers line up on their last digit.
    let right = |ui: &mut egui::Ui, h: f32, text: RichText| {
        ui.allocate_ui_with_layout(vec2(val_w, h), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.set_width(val_w);
            ui.label(text);
        });
    };
    egui::Grid::new(("card_effects", salt)).num_columns(1 + cols as usize).striped(true).spacing([spacing, 5.0]).show(ui, |ui| {
        left(ui, 18.0, RichText::new("Effect").color(p.weak), None);
        if let Some(level) = level {
            right(ui, 18.0, RichText::new(format!("Yours · Lv {level}")).color(p.weak));
        }
        right(ui, 18.0, RichText::new(format!("MLB · Lv {max_level}")).color(p.weak));
        ui.end_row();
        for (name, now, max) in rows {
            left(ui, 20.0, RichText::new(name.as_str()), Some(name.as_str()));
            if level.is_some() {
                right(ui, 20.0, semibold(dash(*now), 14.0));
                let max_text = if max != now { semibold(dash(*max), 14.0).color(pal(ui).good) } else { RichText::new(dash(*max)).color(p.weak) };
                right(ui, 20.0, max_text);
            } else {
                right(ui, 20.0, semibold(dash(*max), 14.0));
            }
            ui.end_row();
        }
    });
}

/// A quick action on a card tile.
#[derive(Clone, Copy)]
enum CardAction {
    UseInPlan,
    Compare,
    Details,
}

const ADD_W: f32 = 78.0;
const ADD_H: f32 = ADD_W * 346.0 / 260.0;
const ADD_TILE: egui::Vec2 = egui::vec2(ADD_W, ADD_H + 32.0);

fn short_effect(name: &str) -> &str {
    match name {
        "Friendship Bonus" => "Friendship",
        "Training Effectiveness" => "Training",
        "Specialty Priority" => "Specialty",
        "Initial Friendship Gauge" | "Initial Friendship Gauge (uma.guide: Starting Bond)" => "Start bond",
        "Energy Cost Reduction" => "Energy cut",
        "Wit Friendship Recovery" => "Wit recov.",
        n => n,
    }
}

#[cfg(test)]
mod tests {

    /// Every uma and card in the knowledge data ships with its art, so nothing shows as a
    /// placeholder. `tools/update_knowledge.py` downloads the art of new ones; run it again if
    /// this fails.
    #[test]
    fn every_uma_and_card_has_art() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let kb = crate::knowledge::Knowledge::load(Some(&root));
        let missing: Vec<String> = kb
            .umas
            .keys()
            .map(|id| ("umas", *id))
            .chain(kb.cards.keys().map(|id| ("cards", *id)))
            .filter(|(dir, id)| !root.join("app/assets").join(dir).join(format!("{id}.webp")).is_file())
            .map(|(dir, id)| format!("{dir}/{id}"))
            .collect();
        assert!(missing.is_empty(), "no art for {} (run tools/update_knowledge.py): {:?}", missing.len(), &missing[..missing.len().min(10)]);
    }

    #[test]
    fn art_urls_follow_uma_guide() {
        // Checked against uma.guide on 2026-10-28: Special Week's icon is byte for byte the bundled one.
        assert_eq!(Art::Uma.url(100101), "https://uma.guide/icon/uma/chr_icon_1001_100101_01.webp");
        assert_eq!(Art::Card.url(10001), "https://uma.guide/img/card/composite/tex_support_card_10001_thumb.webp");
        assert_eq!((Art::Uma.dir(), Art::Card.dir()), ("umas", "cards"));
    }

    use super::*;

    /// The Cards table sorts by an effect at the owned level and by type; the LB chips, effect
    /// minimums and "Teaches…" filter what they say.
    #[test]
    fn cards_sort_and_filter() {
        let mut app = App::for_tests();
        let v = |app: &App, i: usize| app.effect_value(&app.roster.cards[i], effect::FRIENDSHIP);
        app.cards.sort = TableSort { key: "friendship", desc: true };
        let list = app.filtered_cards();
        assert!(list.windows(2).all(|w| v(&app, w[0]) >= v(&app, w[1])));

        app.cards.sort = TableSort { key: "type", desc: false };
        let types: Vec<u8> = app.filtered_cards().iter().map(|&i| app.roster.cards[i].card_type as u8).collect();
        assert!(types.windows(2).all(|w| w[0] <= w[1]));

        app.cards.filter.lbs = [4].into();
        let mlb = app.filtered_cards();
        assert!(!mlb.is_empty() && mlb.iter().all(|&i| app.roster.cards[i].lb >= 4));

        app.cards.filter = CardFilter { minimums: vec![(effect::FRIENDSHIP, 25)], ..Default::default() };
        assert!(app.filtered_cards().iter().all(|&i| v(&app, i) >= 25));

        let first = app.roster.cards[0].id;
        let skill = app.card_skills(first)[0];
        app.cards.filter = CardFilter { teaches: vec![skill], ..Default::default() };
        let teaching = app.filtered_cards();
        assert!(teaching.iter().any(|&i| app.roster.cards[i].id == first));
        assert!(teaching.iter().all(|&i| app.card_skills(app.roster.cards[i].id).contains(&skill)));

        let back: CardFilter = serde_json::from_value(serde_json::to_value(&app.cards.filter).unwrap()).unwrap();
        assert_eq!(back, app.cards.filter);
    }

    /// "Use in current plan" fills the first empty slot once; Compare opens at two cards.
    #[test]
    fn use_in_plan_and_compare() {
        let mut app = App::for_tests();
        app.planner.plan.deck = [None; 5];
        app.planner.plan.trainee = None;
        let id = app.roster.cards[0].id;
        app.use_card_in_plan(id);
        assert_eq!(app.planner.plan.deck[0], Some(id));
        app.use_card_in_plan(id);
        assert_eq!(app.planner.plan.deck.iter().filter(|d| **d == Some(id)).count(), 1);
        let first_uma = app.roster.cards[0].uma.clone();
        let other = app.roster.cards.iter().find(|c| c.uma != first_uma).unwrap().id;
        app.toggle_compare(id);
        assert!(!app.cards.compare_open);
        app.toggle_compare(other);
        assert!(app.cards.compare_open);
        assert_eq!(app.cards.compare, vec![id, other]);
        app.toggle_compare(id);
        assert_eq!(app.cards.compare, vec![other]);
    }
}
