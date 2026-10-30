//! Planner: pick a purpose, trainee and deck; see deck totals, hint coverage and skills to buy;
//! copy the build to Umalator; save the plan as markdown in plans/ for Claude to simulate.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, Stroke, vec2};
use serde::{Deserialize, Serialize};

use crate::App;
use crate::ui_util::UiExt;
use crate::career::CareerResult;
use crate::recommend::RankCache;
use crate::sim::run::{ChartRow, Progress};
use crate::data::{Rarity, Roster};
use crate::knowledge::{Distance, EventPreset, Knowledge, RaceInfo, Style, Surface, UmalatorCourse, effect, grade_rank};
use crate::theme::pal;
use crate::ui_util::{
    badge, card, card_titled, card_titled_to, draw_lb, grade_badge, open_url, search_combo, semibold, stars_widget, tile_ring, today,
    truncate, type_color_str, type_icon,
};

/// Moomoolator, the umalator fork the project simulates with.
const UMALATOR_URL: &str = "https://umalator.app/";
pub(crate) const PLAN_MARKER: &str = "<!-- umalord-plan";

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
pub(crate) enum Purpose {
    #[default]
    ChampionsMeeting,
    TeamTrials,
    Parent,
    LeagueOfHeroes,
    Other,
}

impl Purpose {
    pub(crate) const ALL: [Purpose; 5] = [Purpose::ChampionsMeeting, Purpose::TeamTrials, Purpose::Parent, Purpose::LeagueOfHeroes, Purpose::Other];
    pub fn label(self) -> &'static str {
        match self {
            Self::ChampionsMeeting => "Champions Meeting",
            Self::TeamTrials => "Team Trials",
            Self::Parent => "Parent run",
            Self::LeagueOfHeroes => "League of Heroes",
            Self::Other => "Other",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::ChampionsMeeting => "🏆",
            Self::TeamTrials => "👥",
            Self::Parent => "🌱",
            Self::LeagueOfHeroes => "⭐",
            Self::Other => "✏",
        }
    }
    fn slug(self) -> &'static str {
        match self {
            Self::ChampionsMeeting => "cm",
            Self::TeamTrials => "team-trials",
            Self::Parent => "parent",
            Self::LeagueOfHeroes => "loh",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Plan {
    pub title: String,
    pub created: String,
    pub purpose: Purpose,
    pub distance: Distance,
    pub surface: Surface,
    pub style: Style,
    /// Race id of the target course, if any.
    pub course: Option<u32>,
    /// Date of the Champions Meeting the goal came from, as it was when the plan was saved.
    #[serde(default)]
    pub cm_date: Option<String>,
    /// That Champions Meeting's cup name, e.g. "Capricorn Cup" (`Knowledge::find_cm` matches it).
    #[serde(default)]
    pub cm_name: Option<String>,
    /// That preset's Umalator course id, which pins its inner or outer layout.
    #[serde(default)]
    pub umalator_course: Option<u32>,
    pub conditions: String,
    pub spark_goal: String,
    pub trainee: Option<u32>,
    /// Five owned cards.
    pub deck: [Option<u32>; 5],
    /// Borrowed friend card and its LB.
    pub friend: Option<u32>,
    pub friend_lb: u8,
    pub parents: [String; 2],
    pub target: [u32; 5],
    pub skills: Vec<u32>,
    pub notes: String,
    /// Training scenario, e.g. "Grand Concert"; empty means the current one.
    #[serde(default)]
    pub scenario: String,
    /// Skill points to spend, set by hand; `None` uses the career simulation's estimate.
    #[serde(default)]
    pub sp_budget: Option<u32>,
    /// Skills auto-pick must buy, and skills it must never buy.
    #[serde(default)]
    pub pinned_skills: Vec<u32>,
    #[serde(default)]
    pub banned_skills: Vec<u32>,
    /// The Fast Learner condition: skills cost 10% of their base price less (uma.guide glossary).
    #[serde(default)]
    pub fast_learner: bool,
    /// How much the meta sheet's skill advice counts in auto-pick.
    #[serde(default)]
    pub meta_influence: crate::skill_opt::MetaInfluence,
    /// What auto-pick aims for.
    #[serde(default)]
    pub pick_objective: crate::skill_opt::PickObjective,
    /// At the skill shop at the end of a real career.
    #[serde(default)]
    pub shop: SkillShop,
}

/// "I'm at the skill shop now": the career's real final stats and SP, and the skills on offer with
/// their hint levels. Auto-pick then works on exactly these.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub(crate) struct SkillShop {
    pub on: bool,
    /// Speed, Stamina, Power, Guts, Wit.
    pub stats: [u32; 5],
    pub sp: u32,
    /// Skill → hint level (0–5).
    pub offered: BTreeMap<u32, u8>,
}

impl Plan {
    /// Fill the goal from a Champions Meeting preset: ground, weather and season, distance and
    /// surface from its Umalator course, that exact course for Open Umalator, and the race that runs
    /// on it. Returns a note when races.json has no such race.
    pub(crate) fn apply_cm(&mut self, kb: &Knowledge, preset: &EventPreset) -> Option<String> {
        for (group, code) in [(&GROUNDS, preset.ground), (&WEATHERS, preset.weather), (&SEASONS, preset.season)] {
            if let Some(value) = code.checked_sub(1).and_then(|i| group.get(usize::from(i)))
                && !has_condition(&self.conditions, value)
            {
                self.conditions = set_condition(&self.conditions, group, value);
            }
        }
        self.purpose = Purpose::ChampionsMeeting;
        self.cm_date = Some(preset.date.clone());
        self.cm_name = Some(preset.name.clone());
        self.umalator_course = Some(preset.course);
        if let Some(s) = kb.cm_global_scenario(preset) {
            self.scenario = s.to_string();
        }
        let Some(course) = kb.umalator_courses.iter().find(|c| c.id == preset.course) else {
            return Some(format!("{} isn't in Umalator's course list, so set distance and surface by hand", preset.course_name));
        };
        self.distance = Distance::of_metres(course.distance);
        self.surface = if course.surface == "Dirt" { Surface::Dirt } else { Surface::Turf };
        if let Some(race) = preset_race(kb, preset, course) {
            self.course = Some(race.id);
            return None;
        }
        // Keep a race the user chose only if it runs on this course.
        if self.course.and_then(|id| kb.races.iter().find(|r| r.id == id)).is_none_or(|r| !runs_on(r, course)) {
            self.course = None;
        }
        Some(format!("no race in races.json runs on {}, so pick the course by hand if you want one; Open Umalator still uses the preset's course", preset.course_name))
    }
}

/// Whether a race runs on an Umalator course (same track, distance and surface).
pub(crate) fn runs_on(race: &RaceInfo, course: &UmalatorCourse) -> bool {
    race.track_id == Some(course.track_id) && race.distance == course.distance && race.surface == course.surface
}

/// The race a CM preset runs as: races.json's own entry for that cup on its course if there is one
/// (not the Finals), else the course's best-graded race, oldest id first (the career G1).
fn preset_race<'a>(kb: &'a Knowledge, preset: &EventPreset, course: &UmalatorCourse) -> Option<&'a RaceInfo> {
    let grade = |g: &str| ["G1", "G2", "G3", "OP/L (Open/Listed)"].iter().position(|x| *x == g);
    let on_course: Vec<&RaceInfo> = kb.races.iter().filter(|r| runs_on(r, course) && grade(&r.grade).is_some()).collect();
    on_course
        .iter()
        .find(|r| r.name.contains(&preset.name) && !r.name.contains("Finals"))
        .or_else(|| on_course.iter().min_by_key(|r| (grade(&r.grade), r.id)))
        .copied()
}

/// A short name for a deck-total tile; the full name shows on hover.
fn short_effect(kind: u32, name: &str) -> &str {
    match kind {
        effect::FRIENDSHIP => "Friendship",
        effect::TRAINING => "Training",
        effect::MOOD => "Mood",
        effect::SPECIALTY => "Specialty",
        effect::RACE => "Race bonus",
        effect::HINT_FREQ => "Hint freq.",
        effect::HINT_LEVELS => "Hint levels",
        effect::INIT_BOND => "Initial bond",
        effect::FAIL_PROTECT => "Fail protect",
        effect::ENERGY_COST => "Energy cost",
        effect::SKILL_POINTS => "Skill points",
        effect::WIT_RECOVERY => "Wit recovery",
        _ => name,
    }
}

/// Width of the label column in the Goal card.
const LABEL_W: f32 = 112.0;

/// One labelled row: the label in a fixed-width column, then `add`'s widgets, which wrap inside
/// their own column so wrapped lines stay aligned.
fn field_row<R>(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal_top(|ui| {
        let h = ui.spacing().interact_size.y;
        ui.allocate_ui_with_layout(vec2(LABEL_W, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_size(vec2(LABEL_W, h));
            ui.label(label);
        });
        ui.horizontal_wrapped(add).inner
    })
    .inner
}

/// "2026-10-25" → "Oct 25".
fn short_date(date: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let mut parts = date.split('-').skip(1).map(|s| s.parse::<usize>().unwrap_or(0));
    match (parts.next(), parts.next()) {
        (Some(m @ 1..=12), Some(d)) => format!("{} {d}", MONTHS[m - 1]),
        _ => date.to_string(),
    }
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            title: String::new(),
            created: crate::ui_util::local_today(),
            purpose: Purpose::ChampionsMeeting,
            distance: Distance::Medium,
            surface: Surface::Turf,
            style: Style::Pace,
            course: None,
            cm_date: None,
            cm_name: None,
            umalator_course: None,
            conditions: String::new(),
            spark_goal: String::new(),
            trainee: None,
            deck: [None; 5],
            friend: None,
            friend_lb: 4,
            parents: Default::default(),
            target: [0; 5],
            skills: Vec::new(),
            notes: String::new(),
            scenario: String::new(),
            sp_budget: None,
            pinned_skills: Vec::new(),
            banned_skills: Vec::new(),
            fast_learner: false,
            meta_influence: Default::default(),
            pick_objective: Default::default(),
            shop: SkillShop::default(),
        }
    }
}

/// Simulated skill values for one goal (trainee, stats, style, course, conditions).
pub(crate) struct SkillValues {
    pub key: u64,
    pub rows: HashMap<u32, ChartRow>,
    /// Stat points one length is worth on this course: +50 Speed's simulated gain, inverted.
    pub stats_per_length: Option<f64>,
    /// Stat points a skill point is worth: lengths per SP of the skills worth buying.
    pub sp_value: Option<f64>,
    pub label: String,
}

pub(crate) struct SkillJob {
    pub(crate) key: u64,
    pub(crate) progress: Arc<Progress>,
    pub(crate) rx: mpsc::Receiver<(Vec<ChartRow>, Option<f64>)>,
    pub(crate) count: usize,
    pub(crate) label: String,
    /// Pick skills when the values arrive.
    pub(crate) then_pick: bool,
}

type CareerCache = Option<((u64, u64, [Option<u32>; 5], Option<u32>), CareerResult)>;

pub(crate) struct PlannerState {
    pub plan: Plan,
    pub(crate) files: Vec<(PathBuf, String)>,
    pub(crate) open_file: Option<PathBuf>,
    /// What Auto-pick or the skill simulation last did, shown under Step 5 (true = a warning).
    pub(crate) skill_status: Option<(String, bool)>,
    course_search: String,
    pub(crate) only_fitting: bool,
    /// Deck slot whose card picker is open: 0..=4 own cards, 5 the friend card.
    pub(crate) picker: Option<usize>,
    picker_search: String,
    picker_type: Option<String>,
    pub skill_values: Option<SkillValues>,
    pub skill_job: Option<SkillJob>,
    pub rank_cache: RefCell<RankCache>,
    pub career_cache: RefCell<CareerCache>,
    /// Per plan key: whether each skill can trigger on the plan's course.
    pub activation_cache: RefCell<(u64, HashMap<u32, Option<bool>>)>,
    /// `plan_key` for the last plan fields it saw.
    pub(crate) key_memo: std::cell::Cell<Option<(u64, u64)>>,
    /// The auto-pick optimiser running in the background, and its last result (for a `pick_key`).
    pub pick_job: Option<crate::skill_pick::PickJob>,
    pub pick: Option<(u64, crate::skill_opt::Outcome)>,
    /// Step 5's table order.
    pub pick_sort: crate::ui_util::TableSort,
    /// "Paste the list" in skill shop mode: the text being typed, while open.
    pub shop_paste: Option<String>,
    /// The plan's parents' white-spark skills, for the parents they were read for (Reload clears it).
    pub parent_sparks: RefCell<Option<([String; 2], BTreeMap<u32, String>)>>,
}

/// How a skill's restrictions relate to the plan's target.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Fit {
    Target,
    General,
    Other,
}

/// The plan's race as Umalator codes: course id, and ground, weather, season and time (1-based).
#[derive(Clone, Debug, Default)]
pub(crate) struct RaceSetup {
    pub course: Option<u32>,
    pub ground: Option<u8>,
    pub weather: Option<u8>,
    pub season: Option<u8>,
    pub time: Option<u8>,
    /// Anything guessed or left out.
    pub notes: Vec<String>,
}

struct DeckCard {
    id: u32,
    level: u32,
    lb: u8,
    borrowed: bool,
}

impl PlannerState {
    pub fn new(root: Option<&Path>) -> Self {
        let mut s = Self {
            plan: Plan::default(),
            files: Vec::new(),
            open_file: None,
            skill_status: None,
            course_search: String::new(),
            only_fitting: true,
            picker: None,
            picker_search: String::new(),
            picker_type: None,
            skill_values: None,
            skill_job: None,
            rank_cache: RefCell::new(RankCache::default()),
            career_cache: RefCell::new(None),
            activation_cache: RefCell::new((0, HashMap::new())),
            key_memo: std::cell::Cell::new(None),
            pick_job: None,
            pick: None,
            pick_sort: crate::ui_util::TableSort { key: "pick", desc: true },
            shop_paste: None,
            parent_sparks: RefCell::new(None),
        };
        s.refresh_files(root);
        s
    }

    /// Skill values simulated for this goal, if any.
    pub fn current_skill_values(&self, key: u64) -> Option<&SkillValues> {
        self.skill_values.as_ref().filter(|v| v.key == key)
    }

    pub fn refresh_files(&mut self, root: Option<&Path>) {
        self.files.clear();
        let Some(root) = root else { return };
        let Ok(dir) = std::fs::read_dir(root.join("plans")) else { return };
        for entry in dir.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if !name.ends_with(".md") || name.starts_with('_') {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let title = text.lines().find_map(|l| l.strip_prefix("# ")).unwrap_or(&name).to_string();
            self.files.push((path, title));
        }
        self.files.sort_by(|a, b| b.0.cmp(&a.0));
    }

    /// Open the card picker for a deck slot (0..=4 own cards, 5 friend).
    pub fn open_picker(&mut self, slot: usize) {
        self.picker = Some(slot.min(5));
        self.picker_search.clear();
    }

    /// Fill a sample plan for screenshots: the given trainee and her five best owned MLB SSRs.
    pub fn demo(&mut self, roster: &Roster, kb: &Knowledge, trainee: u32) {
        let Some(info) = kb.umas.get(&trainee) else { return };
        let a = &info.aptitude;
        self.plan.trainee = Some(trainee);
        self.plan.style = a.best_style();
        self.plan.distance = Distance::ALL.into_iter().max_by_key(|d| grade_rank(a.distance(*d))).unwrap_or_default();
        self.plan.surface = if grade_rank(&a.dirt) > grade_rank(&a.turf) { Surface::Dirt } else { Surface::Turf };
        self.plan.target = [1200, 1000, 1000, 600, 1000];
        let mut cards: Vec<_> = roster.cards.iter().filter(|c| c.rarity == Rarity::SSR && c.is_mlb() && c.uma != info.name).collect();
        cards.sort_by_key(|c| -kb.cards.get(&c.id).and_then(|i| i.effect(effect::TRAINING, u32::from(c.level))).unwrap_or(0));
        let mut used = Vec::new();
        let mut slot = 0;
        for c in cards {
            if slot == 5 {
                break;
            }
            if used.contains(&c.uma) {
                continue;
            }
            used.push(c.uma.clone());
            self.plan.deck[slot] = Some(c.id);
            slot += 1;
        }
    }
}

pub(crate) const GROUNDS: [&str; 4] = ["Firm", "Good", "Soft", "Heavy"];
pub(crate) const WEATHERS: [&str; 4] = ["Sunny", "Cloudy", "Rainy", "Snowy"];
pub(crate) const SEASONS: [&str; 4] = ["Spring", "Summer", "Autumn", "Winter"];

fn condition_parts(conditions: &str) -> Vec<String> {
    conditions.split(['·', ',', '/']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

pub(crate) fn has_condition(conditions: &str, value: &str) -> bool {
    condition_parts(conditions).iter().any(|p| p.eq_ignore_ascii_case(value))
}

/// Set one ground, weather or season in a "Firm · Sunny · Winter" string, replacing the other
/// value from the same group, or removing it if it is already set. Free text is kept.
pub(crate) fn set_condition(conditions: &str, group: &[&str], value: &str) -> String {
    let in_group = |p: &str, g: &[&str]| g.iter().any(|x| p.eq_ignore_ascii_case(x));
    let mut parts = condition_parts(conditions);
    match parts.iter().position(|p| in_group(p, group)) {
        Some(i) if parts[i].eq_ignore_ascii_case(value) => {
            parts.remove(i);
        }
        Some(i) => parts[i] = value.to_string(),
        None => parts.push(value.to_string()),
    }
    let rank = |p: &str| [&GROUNDS[..], &WEATHERS[..], &SEASONS[..]].iter().position(|g| in_group(p, g)).unwrap_or(3);
    parts.sort_by_key(|p| rank(p));
    parts.join(" · ")
}

/// Days since 1970-01-01 for a "YYYY-MM-DD" date (Howard Hinnant's days_from_civil).
/// Days since 1970-01-01 for a "YYYY-MM-DD" date.
pub(crate) fn day_number(date: &str) -> i64 {
    let mut parts = date.split('-').map(|s| s.parse::<i64>().unwrap_or(0));
    let (y, m, d) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

pub(crate) fn days_apart(a: &str, b: &str) -> i64 {
    (day_number(a) - day_number(b)).abs()
}

/// Umalator's share-link fragment: the state as gzipped JSON in base64, URI-encoded.
pub(crate) fn umalator_hash(state: &serde_json::Value) -> String {
    use std::io::Write;
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(state.to_string().as_bytes()).expect("writing to memory can't fail");
    let bytes = gz.finish().expect("writing to memory can't fail");
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i > chunk.len() {
                out.push_str("%3D");
                continue;
            }
            match B64[((n >> (18 - 6 * i)) & 63) as usize] {
                b'+' => out.push_str("%2B"),
                b'/' => out.push_str("%2F"),
                c => out.push(c as char),
            }
        }
    }
    out
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

pub(crate) fn race_distance(category: &str, metres: u32) -> Distance {
    match category {
        "Short" | "Sprint" => Distance::Sprint,
        "Mile" => Distance::Mile,
        "Medium" | "Middle" => Distance::Medium,
        "Long" => Distance::Long,
        _ => Distance::of_metres(metres),
    }
}

impl App {
    fn deck_cards(&self) -> Vec<DeckCard> {
        let mut out = Vec::new();
        for id in self.planner.plan.deck.iter().flatten() {
            if let Some(c) = self.owned_card(*id) {
                out.push(DeckCard { id: c.id, level: u32::from(c.level), lb: c.lb, borrowed: false });
            }
        }
        if let Some(id) = self.planner.plan.friend {
            let lb = self.planner.plan.friend_lb.min(4);
            let level = Rarity::from_id(id).map_or(50, |r| u32::from(r.max_level(lb)));
            out.push(DeckCard { id, level, lb, borrowed: true });
        }
        out
    }

    pub(crate) fn skill_fit(&self, id: u32) -> Fit {
        let Some(s) = self.kb.skills.get(&id) else { return Fit::Other };
        let p = &self.planner.plan;
        let tags = s.tags();
        if !tags.fits(Some(p.style), Some(p.distance), Some(p.surface)) {
            return Fit::Other;
        }
        if !tags.tracks.is_empty() {
            let course_track = p.course.and_then(|c| self.kb.races.iter().find(|r| r.id == c)).and_then(|r| r.track_id);
            return if course_track.is_some_and(|t| tags.tracks.contains(&t)) { Fit::Target } else { Fit::Other };
        }
        if tags.is_general() { Fit::General } else { Fit::Target }
    }

    pub(crate) fn planner_tab(&mut self, ui: &mut egui::Ui) {
        // A narrow window keeps the plan's width: the saved plans fold into a rail with a menu.
        if ui.available_width() < 1100.0 {
            egui::Panel::left("plan_list_rail").resizable(false).exact_size(crate::collection_kit::RAIL_W).frame(crate::collection_kit::rail_frame()).show(ui, |ui| self.plan_rail(ui));
        } else {
            egui::Panel::left("plan_list").resizable(false).exact_size(240.0).frame(crate::side_frame()).show(ui, |ui| self.plan_list(ui));
        }
        egui::CentralPanel::default().frame(crate::page_frame()).show(ui, |ui| {
            let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if let Some(y) = self.debug_scroll {
                area = area.vertical_scroll_offset(y);
            }
            area.show(ui, |ui| {
                // set_max_width can widen past the viewport, so cap it at what's visible.
                ui.set_max_width(ui.available_width().min(1120.0));
                self.plan_editor(ui);
            });
        });
        if self.planner.picker.is_some() {
            self.deck_picker(ui);
        }
    }

    fn new_plan(&mut self) {
        self.planner.plan = Plan::default();
        self.planner.open_file = None;
        self.planner.skill_status = None;
    }

    /// The saved plans on a narrow window: ＋ for a new plan, 📂 for a menu of saved ones.
    fn plan_rail(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            let new = crate::ui_util::primary_button(ui, "＋", 15.0).min_size(vec2(32.0, 32.0));
            if ui.add(new).on_hover_text("New plan: start an empty one. Save the current one first if you want to keep it.").clicked() {
                self.new_plan();
            }
            ui.add_space(6.0);
            let mut load = None;
            ui.menu_button(RichText::new("📂").size(16.0), |ui| {
                ui.set_min_width(240.0);
                ui.label(RichText::new("Saved plans").strong());
                if self.planner.files.is_empty() {
                    ui.label(RichText::new("No saved plans yet. Plans you save land in the plans folder.").size(12.5).color(pal(ui).weak));
                }
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for (path, title) in &self.planner.files {
                        let open = self.planner.open_file.as_ref() == Some(path);
                        if ui.add(egui::Button::new(format!("📋 {title}")).selected(open)).clicked() {
                            load = Some(path.clone());
                            ui.close();
                        }
                    }
                });
            })
            .response
            .on_hover_text(format!("Saved plans ({}): click to pick one", self.planner.files.len()));
            if let Some(path) = load {
                self.open_plan_file(&path);
            }
        });
    }

    fn plan_list(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let new = crate::ui_util::primary_button(ui, "＋ New plan", 14.5).min_size(vec2(ui.available_width(), 32.0));
        if ui.add(new).on_hover_text("Start an empty plan. Save the current one first if you want to keep it.").clicked() {
            self.new_plan();
        }
        crate::ui_util::section_label(ui, "Saved plans", "Plans you saved, newest first. Click one to open it. They live in the plans folder as markdown files.");
        let mut load = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.planner.files.is_empty() {
                ui.label(RichText::new("No saved plans yet. Plans you save land in the plans folder.").size(12.5).color(p.weak));
            }
            for (path, title) in &self.planner.files {
                let open = self.planner.open_file.as_ref() == Some(path);
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let b = egui::Button::new(RichText::new(format!("📋 {title}")).size(13.5)).truncate().selected(open).min_size(vec2(ui.available_width(), 28.0));
                if ui.add(b).on_hover_text(format!("{title}\n{name}")).clicked() {
                    load = Some(path.clone());
                }
            }
        });
        if let Some(path) = load {
            self.open_plan_file(&path);
        }
    }

    /// Open a saved plan in the planner. Returns false (with a toast) if it has no data block.
    pub(crate) fn open_plan_file(&mut self, path: &std::path::Path) -> bool {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let json = text.split(PLAN_MARKER).nth(1).and_then(|t| t.split("-->").next());
        match json.map(serde_json::from_str::<Plan>) {
            Some(Ok(plan)) => {
                self.planner.plan = plan;
                self.planner.open_file = Some(path.to_path_buf());
                self.planner.skill_status = None;
                true
            }
            _ => {
                self.toast("That plan has no app data block (written by hand?). Read it in ⚙ Settings → Data and updates → Browse notes and data files.", crate::ui_util::ToastKind::Warn);
                false
            }
        }
    }

    fn plan_editor(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);

        // --- Title and actions
        ui.horizontal(|ui| {
            ui.set_min_height(30.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.planner.plan.title)
                    .hint_text(semibold("Untitled plan (click to name it)", 22.0))
                    .font(egui::FontId::new(22.0, egui::FontFamily::Name(crate::theme::SEMIBOLD.into())))
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 0)))
                    .desired_width(ui.available_width() - 540.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let save = crate::ui_util::primary_button(ui, "💾 Save plan", 14.0).min_size(vec2(0.0, 30.0));
                let hover = "Saves it as plans/<date>-<uma>-<purpose>.md, so you can reopen it from the list on the left. \
                             Claude can then simulate it and write a recommendation into the file.";
                if ui.add(save).on_hover_text(hover).clicked() {
                    self.save_plan();
                }
                let sim = ui.add(egui::Button::new(semibold("▶ Simulate", 14.0)).min_size(vec2(0.0, 30.0))).on_hover_text(
                    "Race the plan in the Race sim tab: the trainee with her skills to buy against the same build without them, \
                     on the plan's course and conditions.",
                );
                if sim.clicked() {
                    let ctx = ui.ctx().clone();
                    if let Err(e) = self.simulate_plan(&ctx) {
                        self.toast(e, crate::ui_util::ToastKind::Warn);
                    }
                }
                let open = ui
                    .add(egui::Button::new("↗ Open Umalator").min_size(vec2(0.0, 30.0)))
                    .on_hover_text("Opens the umalator.app website with this build as Uma 1 and Uma 2, on the plan's course and conditions");
                if open.clicked() {
                    match self.umalator_link() {
                        Ok((link, notes)) => {
                            open_url(&link);
                            ui.ctx().copy_text(link);
                            let mut msg = "Opened Umalator with this build in both slots; change Uma 2 to compare. The link is also on the clipboard.".to_string();
                            if !notes.is_empty() {
                                msg += &format!(" Note: {}.", notes.join("; "));
                            }
                            self.toast(msg, crate::ui_util::ToastKind::Success);
                        }
                        Err(_) => open_url(UMALATOR_URL),
                    }
                }
                if ui
                    .add(egui::Button::new("📋 Copy for Umalator").min_size(vec2(0.0, 30.0)))
                    .on_hover_text("Copies this build to paste into umalator.app: ▶ UMA → Uma 1 or 2 → import (↑) → Paste from clipboard")
                    .clicked()
                {
                    match self.umalator_build() {
                        Ok((json, dropped)) => {
                            ui.ctx().copy_text(json);
                            let mut msg = "Build copied. In Umalator open ▶ UMA, pick Uma 1 or 2, then import (↑) → Paste from clipboard.".to_string();
                            if !dropped.is_empty() {
                                msg += &format!(" Left out (Umalator doesn't know them): {}.", dropped.join(", "));
                            }
                            self.toast(msg, crate::ui_util::ToastKind::Success);
                        }
                        Err(e) => self.toast(e, crate::ui_util::ToastKind::Warn),
                    }
                }
            });
        });
        ui.label(
            RichText::new("Work down the page in five steps: the goal, a trainee, her target stats, a deck of cards, and the skills to buy. Then save the plan.")
                .size(13.0)
                .color(p.weak),
        );
        ui.add_space(10.0);

        // --- Goal: the purpose on the title line, then one labelled row per field.
        card(ui, |ui| {
            field_row(ui, semibold("Step 1 · Goal", 15.5), |ui| {
                for purpose in Purpose::ALL {
                    ui.selectable_value(&mut self.planner.plan.purpose, purpose, format!("{} {}", purpose.icon(), purpose.label()));
                }
            });
            ui.add_space(2.0);
            if self.planner.plan.purpose == Purpose::ChampionsMeeting {
                self.cm_picker(ui);
            }
            self.scenario_row(ui);
            self.target_row(ui);
            field_row(ui, RichText::new("Conditions").color(p.weak), |ui| {
                for group in [&GROUNDS[..], &WEATHERS[..], &SEASONS[..]] {
                    for value in group {
                        let on = has_condition(&self.planner.plan.conditions, value);
                        if ui.add(egui::Button::new(RichText::new(*value).size(12.5)).selected(on).corner_radius(CornerRadius::same(10)).small()).clicked() {
                            self.planner.plan.conditions = set_condition(&self.planner.plan.conditions, group, value);
                        }
                    }
                    ui.add_space(6.0);
                }
                // Fill what's left of the chip row, or wrap to a full-width line.
                let left = ui.available_size_before_wrap().x - 4.0;
                let w = if left >= 200.0 { left } else { ui.available_width() };
                ui.add(egui::TextEdit::singleline(&mut self.planner.plan.conditions).hint_text("or type, e.g. night race").desired_width(w))
                    .on_hover_text("The chips edit this text; anything else you type is kept.");
            });
            if self.planner.plan.purpose == Purpose::Parent {
                field_row(ui, RichText::new("Sparks wanted").color(p.weak), |ui| {
                    let w = ui.available_width();
                    ui.add(egui::TextEdit::singleline(&mut self.planner.plan.spark_goal).hint_text("e.g. Speed 3★, Turf, unique").desired_width(w));
                });
            }
        });
        ui.add_space(8.0);
        self.meta_panel(ui);

        // Trainee on the left; her target stats, parents and the notes on the right, when there's room.
        let side_by_side = ui.available_width() >= 900.0;
        if side_by_side {
            // The two columns end level: the shorter one's last card stretches.
            crate::ui_util::equal_columns(ui, "plan_columns", 2, |ui, i, bottom| {
                if i == 0 {
                    card_titled_to(ui, "Step 2 · Trainee", bottom, |ui| self.trainee_picker(ui))
                } else {
                    card_titled(ui, "Step 3 · Target stats and parents", |ui| self.stats_and_parents(ui));
                    card_titled_to(ui, "📝 Notes", bottom, |ui| self.notes_box(ui))
                }
            });
        } else {
            card_titled(ui, "Step 2 · Trainee", |ui| self.trainee_picker(ui));
            card_titled(ui, "Step 3 · Target stats and parents", |ui| self.stats_and_parents(ui));
        }
        card(ui, |ui| {
            self.deck_editor(ui);
            self.deck_hints(ui);
        });
        ui.add_space(10.0);
        card(ui, |ui| self.skill_picker(ui));
        ui.add_space(10.0);
        if !side_by_side {
            card_titled(ui, "📝 Notes", |ui| self.notes_box(ui));
        }
        ui.add_space(12.0);
    }

    /// What the community meta sheet says for the plan's Champions Meeting and style: the styles
    /// that do best, target stats, trainees, its decks by tier and its skill advice.
    fn meta_panel(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let Some(sheet) = self.kb.pvp_meta.clone() else { return };
        let Some((ev, st)) = self.plan_meta().map(|(e, s)| (e.clone(), s.clone())) else {
            if self.planner.plan.purpose == Purpose::ChampionsMeeting {
                card(ui, |ui| {
                    ui.label(
                        RichText::new("📈 Pick an upcoming CM above to see what the meta sheet recommends for it. The deck fill and Auto-pick then put its picks first.")
                            .size(12.5)
                            .color(p.weak),
                    );
                });
                ui.add_space(8.0);
            }
            return;
        };
        let source = format!(
            "{}\n{}\nRead {} by tools/update_knowledge.py --only meta.\n{}",
            sheet.source, sheet.authors, sheet.fetched, sheet.caveat
        );
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(semibold(format!("📈 Meta · CM {} {} · {}", ev.number, ev.name, self.planner.plan.style.label()), 15.5));
                ui.label(RichText::new("ⓘ community PvP sheet").size(12.0).color(p.weak)).on_hover_text(source);
            });
            ui.label(
                RichText::new(format!(
                    "What the community's PvP sheet recommends for this CM. ✨ Recommend whole deck (Step 4) puts its picks (★) first; in Step 5, \"Meta sheet\" sets how much they count for skills. Opinion, not game data{}.",
                    if ev.number > 21 { "; the sheet says its advice past CM 21 still needs checking" } else { "" }
                ))
                .size(12.0)
                .color(p.weak),
            );
            ui.add_space(4.0);
            if !ev.style_ranking.is_empty() {
                field_row(ui, RichText::new("Best styles").color(p.weak), |ui| {
                    ui.label(RichText::new(ev.style_ranking.replace('>', " › ")).strong());
                    if !ev.analysis.is_empty() {
                        ui.label(RichText::new("ⓘ why").size(12.0).color(p.weak)).on_hover_text(&ev.analysis);
                    }
                });
            }
            if ev.stats.len() == 5 {
                field_row(ui, RichText::new("Target stats").color(p.weak), |ui| {
                    let names = ["Spd", "Sta", "Pow", "Gut", "Wit"];
                    let text: Vec<String> = ev.stats.iter().zip(names).map(|(v, n)| format!("{n} {v}")).collect();
                    ui.label(text.join(" · ")).on_hover_text(if ev.description.is_empty() { "\"++\" means more is better." } else { &ev.description });
                    let parsed: Vec<Option<u32>> = ev.stats.iter().map(|v| v.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()).collect();
                    if parsed.iter().all(Option::is_some) && ui.small_btn("Use these").on_hover_text("Set Step 3's target stats to these").clicked() {
                        for (i, v) in parsed.into_iter().enumerate() {
                            self.planner.plan.target[i] = v.unwrap_or(self.planner.plan.target[i]);
                        }
                    }
                });
            }
            if !st.umas.is_empty() {
                field_row(ui, RichText::new("Trainees").color(p.weak), |ui| {
                    for u in &st.umas {
                        match u.id.filter(|id| self.owned_uma(*id).is_some()) {
                            Some(id) => {
                                let on = self.planner.plan.trainee == Some(id);
                                let b = egui::Button::new(RichText::new(format!("✔ {}", u.name)).size(12.5)).selected(on).corner_radius(CornerRadius::same(10)).small();
                                if ui.add(b).on_hover_text(format!("You own her. Rated {} by the sheet. Click to make her the trainee.", u.rating)).clicked() {
                                    self.planner.plan.trainee = Some(id);
                                }
                            }
                            None => {
                                ui.label(RichText::new(&u.name).size(12.5).color(p.weak)).on_hover_text(format!("Not in your roster. Rated {} by the sheet.", u.rating));
                            }
                        }
                    }
                });
            }
            egui::CollapsingHeader::new(RichText::new("The sheet's decks and skill advice").size(13.0)).id_salt("meta_more").show(ui, |ui| {
                for tier in crate::meta::TIERS {
                    let Some(deck) = st.decks.get(tier) else { continue };
                    field_row(ui, RichText::new(format!("{tier} deck")).color(p.weak), |ui| {
                        let chip = |ui: &mut egui::Ui, c: &crate::meta::MetaCard, borrow: bool| {
                            let owned = c.id.and_then(|id| self.owned_card(id));
                            let name = if c.any.is_empty() { c.name.clone() } else { format!("any {}", c.any) };
                            let text = if borrow { format!("↔ {name}") } else { name };
                            let color = if owned.is_some() { pal(ui).good } else { p.weak };
                            let mut hover = vec![if borrow { "Borrowed from a friend".to_string() } else { "One of your five".to_string() }];
                            if !c.note.is_empty() {
                                hover.push(format!("Brought for: {}", c.note));
                            }
                            if let Some(lb) = c.min_lb {
                                hover.push(format!("Needs LB {lb} or higher{}", if c.alt.is_empty() { String::new() } else { format!(", {}", c.alt) }));
                            }
                            match owned {
                                Some(o) => hover.push(format!("You own it at LB {}", o.lb)),
                                None if c.id.is_none() && c.any.is_empty() => hover.push("Not on Global yet".into()),
                                None if c.any.is_empty() => hover.push("Not in your roster".into()),
                                None => {}
                            }
                            ui.label(RichText::new(text).size(12.5).color(color)).on_hover_text(hover.join("\n"));
                        };
                        for c in &deck.cards {
                            chip(ui, c, false);
                        }
                        if let Some(b) = &deck.borrow {
                            chip(ui, b, true);
                        }
                    });
                }
                if !st.honorable.is_empty() {
                    field_row(ui, RichText::new("Also good").color(p.weak), |ui| {
                        for c in &st.honorable {
                            let owned = c.id.is_some_and(|id| self.owned_card(id).is_some());
                            ui.label(RichText::new(&c.name).size(12.5).color(if owned { pal(ui).good } else { p.weak })).on_hover_text(if c.note.is_empty() { "Honorable mention".to_string() } else { format!("Honorable mention: {}", c.note) });
                        }
                    });
                }
                ui.label(RichText::new("Green: you own it. ↔: the borrowed friend card.").size(11.5).color(p.weak));
                ui.add_space(4.0);
                for (label, text) in [("Acceleration", &st.accel), ("Skills", &st.advice)] {
                    if !text.is_empty() {
                        field_row(ui, RichText::new(label).color(p.weak), |ui| {
                            ui.add(egui::Label::new(RichText::new(text).size(12.5)).wrap());
                        });
                    }
                }
                if !st.parents.is_empty() {
                    field_row(ui, RichText::new("Parents").color(p.weak), |ui| {
                        let list: Vec<String> = st.parents.iter().map(|x| format!("{} ({})", x.uma, self.kb.skill_name(x.skill))).collect();
                        ui.add(egui::Label::new(RichText::new(format!("The advice names these umas' uniques, which a trainee inherits from parents: {}.", list.join(", "))).size(12.5)).wrap());
                    });
                }
            });
        });
        ui.add_space(8.0);
    }

    fn notes_box(&mut self, ui: &mut egui::Ui) {
        ui.add(egui::TextEdit::multiline(&mut self.planner.plan.notes).hint_text("Anything Claude should know when simulating this plan").desired_rows(2).desired_width(f32::INFINITY));
    }

    fn stats_and_parents(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        // Five equal stat boxes across the card.
        let w = ((ui.available_width() - 4.0 * 8.0) / 5.0).clamp(64.0, 96.0);
        ui.horizontal(|ui| {
            for (i, name) in ["Speed", "Stamina", "Power", "Guts", "Wit"].iter().enumerate() {
                ui.vertical(|ui| {
                    ui.label(RichText::new(*name).size(12.0).color(p.weak));
                    ui.add_sized([w, 26.0], egui::DragValue::new(&mut self.planner.plan.target[i]).range(0..=2000).speed(10));
                });
            }
        });
        ui.add_space(6.0);
        let saved = self.saved_parents();
        for i in 0..2 {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.planner.plan.parents[i]).hint_text(format!("Parent {} and its key sparks", i + 1)).desired_width(ui.available_width() - 108.0));
                if saved.is_empty() {
                    ui.add_enabled(false, egui::Button::new("📂 Saved").min_size(vec2(100.0, 0.0)))
                        .on_disabled_hover_text(RichText::new("None saved yet. Add parents to account/parents.md.").color(p.weak));
                } else {
                    let items = || saved.iter().map(|s| (Some(s.clone()), s.clone(), String::new())).collect();
                    let current = Some(self.planner.plan.parents[i].clone());
                    if let Some(s) = search_combo(ui, ("saved_parent", i), "📂 Saved", 100.0, items, current.as_ref()) {
                        self.planner.plan.parents[i] = s;
                    }
                }
            });
        }
        let pink = self.pink_sparks_wanted();
        if !pink.is_empty() {
            ui.label(RichText::new(format!("✨ Pink sparks that would help: {}", pink.join(", "))).size(12.5).color(p.gold));
        }
    }

    /// Whether the plan's Champions Meeting turns this skill off (official announcement).
    pub(crate) fn plan_disabled(&self, skill: u32) -> bool {
        self.plan_cm().is_some_and(|e| e.disabled_skills.contains(&skill))
    }

    /// The Champions Meeting the plan was made for (`Knowledge::find_cm`).
    pub(crate) fn plan_cm(&self) -> Option<&EventPreset> {
        let p = &self.planner.plan;
        self.kb.find_cm(p.cm_date.as_deref()?, p.umalator_course, p.cm_name.as_deref())
    }

    /// One click on an upcoming Champions Meeting fills the course, distance, surface and conditions.
    fn cm_picker(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let mut shown: Vec<EventPreset> = self.kb.upcoming_cms(&today()).into_iter().take(4).cloned().collect();
        // A plan linked to a CM that has since passed still shows it, so it can be unlinked.
        let linked = self.plan_cm().map(|e| (e.name.clone(), e.date.clone()));
        if let Some(e) = self.plan_cm()
            && !shown.iter().any(|u| u.date == e.date)
        {
            shown.insert(0, e.clone());
        }
        let mut pick = None;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(LABEL_W);
                ui.label(RichText::new("Upcoming CM").color(p.weak));
                if shown.iter().any(|e| !e.announced) {
                    // One mark per line: the label column is too narrow for both on one.
                    let legend = if shown.iter().any(|e| e.announced) { "✔ announced\n? estimated" } else { "? estimated" };
                    ui.label(RichText::new(legend).size(11.0).color(p.weak))
                        .on_hover_text("✔: announced in the game's news. ?: not announced yet, so Umalator's guess at the date, course and conditions.");
                }
            });
            ui.horizontal_wrapped(|ui| {
                if shown.is_empty() {
                    ui.label(RichText::new("none in knowledge/data/umalator.json; refresh it with tools/update_knowledge.py").size(12.5).color(p.weak));
                }
                for e in &shown {
                    let on = linked.as_ref().is_some_and(|(n, d)| *n == e.name && *d == e.date);
                    let small = egui::FontId::proportional(12.0);
                    let mut job = egui::text::LayoutJob::default();
                    let (mark, mark_color) = if e.announced { ("✔ ", pal(ui).good) } else { ("? ", pal(ui).warn) };
                    job.append(mark, 0.0, egui::TextFormat::simple(egui::FontId::proportional(13.0), mark_color));
                    job.append(&e.name, 0.0, egui::TextFormat::simple(egui::FontId::new(13.5, egui::FontFamily::Name(crate::theme::SEMIBOLD.into())), p.text));
                    job.append(&format!(" · {}\n", short_date(&e.date)), 0.0, egui::TextFormat::simple(small.clone(), p.weak));
                    job.append(&e.course_name, 0.0, egui::TextFormat::simple(small, p.weak));
                    let action = if on { "Click again to unlink it (the course and conditions stay)." } else { "Click to fill the goal from it." };
                    let round1 = e.round1.map(|t| format!("Round 1: {} ({})\n", crate::live::fmt_local(t), crate::live::fmt_utc(t))).unwrap_or_default();
                    let disabled = if e.disabled_skills.is_empty() { String::new() } else { format!("{} skills won't activate in it.\n", e.disabled_skills.len()) };
                    let hover = format!("{} {}\n{} · {}\n{round1}{disabled}{}\n{action}", e.name, e.date, e.course_name, e.conditions, e.status_explained());
                    if ui.add(egui::Button::new(job).selected(on).corner_radius(CornerRadius::same(10))).on_hover_text(hover).clicked() {
                        pick = Some((e.clone(), on));
                    }
                }
            });
        });
        ui.add_space(2.0);
        match pick {
            Some((e, true)) => {
                self.planner.plan.cm_date = None;
                self.planner.plan.cm_name = None;
                self.planner.plan.umalator_course = None;
                self.toast(format!("Unlinked the {}; the course and conditions stay.", e.name), crate::ui_util::ToastKind::Success);
            }
            Some((e, false)) => {
                let msg = match self.planner.plan.apply_cm(&self.kb, &e) {
                    Some(note) => (format!("Filled the goal from the {}; {note}.", e.name), true),
                    None => (format!("Filled course, distance, surface and conditions from the {} ({}).", e.name, e.date), false),
                };
                self.toast_flag(msg);
            }
            None => {}
        }
    }

    /// The training scenario the career runs in: it sets training values, skill-point sources and
    /// which cards are scenario-linked, for the career simulation and the card picks.
    fn scenario_row(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let Some(all) = self.kb.scenarios.as_ref().map(|s| s.scenarios.clone()) else { return };
        let current = self.plan_scenario_name();
        let auto = self.planner.plan.scenario.is_empty();
        let mut pick = None;
        field_row(ui, RichText::new("Scenario").color(p.weak), |ui| {
            for sc in all.iter().filter(|s| s.global) {
                let on = sc.matches(&current);
                let mut hover = sc.card_note.clone();
                if !sc.link.is_empty() {
                    hover += &format!("\nLinked: {}. {}", sc.link.join(", "), sc.link_note);
                }
                if on && auto {
                    hover += "\nPicked automatically: the scenario of the next Champions Meeting (uma.guide).";
                }
                if ui.add(egui::Button::new(RichText::new(&sc.name).size(12.5)).selected(on).corner_radius(CornerRadius::same(10)).small()).on_hover_text(hover).clicked() {
                    pick = Some(sc.name.clone());
                }
            }
            if let Some(sc) = all.iter().find(|s| s.matches(&current))
                && !sc.link.is_empty()
            {
                let linked: Vec<&str> = sc.link.iter().map(String::as_str).filter(|l| self.roster.cards.iter().any(|c| c.uma == *l)).collect();
                let text = if linked.is_empty() { "you own no linked cards".to_string() } else { format!("you own linked: {}", linked.join(", ")) };
                ui.label(RichText::new(text).size(12.0).color(p.weak));
            }
        });
        if let Some(name) = pick {
            self.planner.plan.scenario = name;
        }
    }

    /// Distance, surface and style, then the course: a race found by search, or the CM's.
    fn target_row(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let current = self.planner.plan.course.and_then(|id| self.kb.races.iter().find(|r| r.id == id)).cloned();
        let cm_course = self.plan_cm().map(|e| e.course_name.clone());
        field_row(ui, RichText::new("Target").color(p.weak), |ui| {
            let plan = &mut self.planner.plan;
            egui::ComboBox::from_id_salt("pl_dist").width(84.0).selected_text(plan.distance.label()).show_ui(ui, |ui| {
                for d in Distance::ALL {
                    ui.selectable_value(&mut plan.distance, d, d.label());
                }
            });
            egui::ComboBox::from_id_salt("pl_surf").width(70.0).selected_text(plan.surface.label()).show_ui(ui, |ui| {
                for d in Surface::ALL {
                    ui.selectable_value(&mut plan.surface, d, d.label());
                }
            });
            egui::ComboBox::from_id_salt("pl_style").width(118.0).selected_text(plan.style.label()).show_ui(ui, |ui| {
                for d in Style::ALL {
                    ui.selectable_value(&mut plan.style, d, d.label());
                }
            });
            ui.add_space(10.0);
            ui.label(RichText::new("Course").color(p.weak));
            match (&current, &cm_course) {
                (Some(r), _) => {
                    badge(ui, format!("{} {} {}m · {}", r.track, r.surface, r.distance, r.name), p.link).on_hover_text(format!("{} ({}), turns {}", r.name, r.grade, r.direction));
                    if ui.small_btn("✖").on_hover_text("Clear the course (unlinks the CM too)").clicked() {
                        let plan = &mut self.planner.plan;
                        plan.course = None;
                        plan.cm_date = None;
                        plan.umalator_course = None;
                    }
                }
                (None, Some(c)) => {
                    ui.label(RichText::new(format!("{c} (no race matches)")).color(p.weak));
                }
                (None, None) => {
                    ui.label(RichText::new("any").color(p.weak));
                }
            }
            ui.add(egui::TextEdit::singleline(&mut self.planner.course_search).hint_text("🔍 search race or track").desired_width(170.0));
        });
        let weak = p.weak;
        let q = self.planner.course_search.trim().to_lowercase();
        if q.len() < 2 {
            return;
        }
        let mut seen = std::collections::HashSet::new();
        let matches: Vec<_> = self
            .kb
            .races
            .iter()
            .filter(|r| r.grade != "Unknown (0)" && (r.name.to_lowercase().contains(&q) || r.track.to_lowercase().contains(&q)))
            .filter(|r| seen.insert((r.name.clone(), r.distance, r.surface.clone())))
            .take(12)
            .cloned()
            .collect();
        let mut pick = None;
        ui.horizontal(|ui| {
            ui.add_space(LABEL_W + ui.spacing().item_spacing.x);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                if matches.is_empty() {
                    ui.label(RichText::new("No match.").color(weak));
                }
                for r in &matches {
                    let label = format!("{} ({}) · {} {} {}m {}", r.name, r.grade, r.track, r.surface, r.distance, r.direction);
                    if ui.selectable_label(false, label).clicked() {
                        pick = Some(r.clone());
                    }
                }
            });
        });
        if let Some(r) = pick {
            let plan = &mut self.planner.plan;
            plan.course = Some(r.id);
            plan.cm_date = None;
            plan.umalator_course = None;
            plan.distance = race_distance(&r.category, r.distance);
            plan.surface = if r.surface == "Dirt" { Surface::Dirt } else { Surface::Turf };
            self.planner.course_search.clear();
        }
    }

    fn trainee_picker(&mut self, ui: &mut egui::Ui) {
        let pl = &self.planner.plan;
        let weak = pal(ui).weak;
        let score = |id: u32| {
            self.kb.umas.get(&id).map_or(0, |i| {
                let a = &i.aptitude;
                u32::from(grade_rank(a.distance(pl.distance))) * 3 + u32::from(grade_rank(a.surface(pl.surface))) * 3 + u32::from(grade_rank(a.style(pl.style))) * 2
            })
        };
        let mut options: Vec<(u32, String)> = self
            .roster
            .umas
            .iter()
            .map(|u| {
                let apt = self.kb.umas.get(&u.id).map(|i| {
                    let a = &i.aptitude;
                    format!("{} {} · {} {} · {} {}", pl.distance.label(), a.distance(pl.distance), pl.surface.label(), a.surface(pl.surface), pl.style.short(), a.style(pl.style))
                });
                (u.id, format!("{} [{}] {}★ P{}  —  {}", u.name, u.outfit, u.stars, u.potential, apt.unwrap_or_default()))
            })
            .collect();
        options.sort_by_key(|(id, _)| std::cmp::Reverse((score(*id), self.owned_uma(*id).map_or(0, |u| u.stars))));
        let selected_text = pl.trainee.and_then(|id| options.iter().find(|o| o.0 == id)).map_or("Choose a trainee…".to_string(), |o| o.1.clone());
        let mut trainee = pl.trainee;
        let items = || {
            let mut items: Vec<(Option<u32>, String, String)> = vec![(None, "SORTED BY APTITUDE FIT FOR THE GOAL ABOVE".into(), String::new())];
            items.extend(options.iter().map(|(id, label)| (Some(*id), label.clone(), String::new())));
            items
        };
        if let Some(id) = search_combo(ui, "pl_trainee", selected_text, ui.available_width().min(600.0), items, trainee.as_ref()) {
            trainee = Some(id);
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("✨ Best fits for this goal:").size(12.5).color(pal(ui).accent));
            let meta = self.plan_meta_trainees();
            for id in self.trainee_candidates().into_iter().take(4) {
                let (Some(u), Some(info)) = (self.owned_uma(id), self.kb.umas.get(&id)) else { continue };
                let a = &info.aptitude;
                let star = if meta.contains(&id) { "★ " } else { "" };
                let label = format!("{star}{} {}★ · {} {} {}", u.name, u.stars, a.distance(pl.distance), a.surface(pl.surface), a.style(pl.style));
                let b = egui::Button::new(RichText::new(label).size(12.5)).selected(pl.trainee == Some(id)).corner_radius(CornerRadius::same(12));
                let sheet = if meta.contains(&id) { "\n★ The meta sheet recommends her for this CM and style." } else { "" };
                if ui.add(b).on_hover_text(format!("{} [{}]\n{} · {} · {} aptitude{sheet}", u.name, u.outfit, pl.distance.label(), pl.surface.label(), pl.style.label())).clicked() {
                    trainee = Some(id);
                }
            }
        });
        if trainee != self.planner.plan.trainee {
            self.planner.plan.trainee = trainee;
            if let Some(info) = trainee.and_then(|t| self.kb.umas.get(&t)) {
                self.planner.plan.style = info.aptitude.best_style();
            }
        }

        let Some(id) = self.planner.plan.trainee else { return };
        let (Some(uma), Some(info)) = (self.owned_uma(id).cloned(), self.kb.umas.get(&id)) else { return };
        let pl = &self.planner.plan;
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if let Some(uri) = self.art_uri("umas", id) {
                let art = egui::Image::new(uri).fit_to_exact_size(vec2(84.0, 84.0)).corner_radius(CornerRadius::same(12)).sense(egui::Sense::click());
                if ui.add(art).on_hover_text("Click for her details").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.open_detail(crate::detail_modal::Detail::Uma(id), None);
                }
            }
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(semibold(&uma.name, 17.0));
                    stars_widget(ui, uma.stars, 6.0);
                    badge(ui, format!("P{}", uma.potential), if uma.potential >= 5 { pal(ui).accent } else { pal(ui).warn });
                });
                ui.horizontal(|ui| {
                    grade_badge(ui, pl.distance.label(), info.aptitude.distance(pl.distance));
                    grade_badge(ui, pl.surface.label(), info.aptitude.surface(pl.surface));
                    grade_badge(ui, pl.style.short(), info.aptitude.style(pl.style));
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Growth").size(11.5).color(weak));
                        ui.horizontal(|ui| {
                            for (n, v) in ["Spd", "Sta", "Pow", "Gut", "Wit"].iter().zip(info.growth) {
                                if v > 0 {
                                    badge(ui, format!("{n} +{v}%"), pal(ui).gold);
                                }
                            }
                        });
                    });
                });
            });
        });
        let weak_apts: Vec<String> = [
            (pl.distance.label(), info.aptitude.distance(pl.distance)),
            (pl.surface.label(), info.aptitude.surface(pl.surface)),
            (pl.style.label(), info.aptitude.style(pl.style)),
        ]
        .iter()
        .filter(|(_, g)| grade_rank(g) < 7)
        .map(|(l, g)| format!("{l} {g}"))
        .collect();
        if !weak_apts.is_empty() {
            ui.label(RichText::new(format!("Below A: {}. Pink sparks from parents raise aptitudes.", weak_apts.join(", "))).size(12.5).color(weak));
        }
        self.aptitude_warning(ui);
        if let Some(u) = info.unique_skill {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Unique").color(weak));
                self.skill_link(ui, u, false);
                if self.skill_fit(u) == Fit::Other {
                    ui.label(RichText::new("doesn't fit this target").color(pal(ui).warn).size(12.0));
                }
            });
        }
    }

    /// Card art with LB diamonds and a caption; used for deck slots and the picker.
    fn art_tile(&self, ui: &mut egui::Ui, id: Option<u32>, width: f32, lb: Option<u8>, caption: &str, enabled: bool) -> egui::Response {
        let p = pal(ui);
        let h = width * 346.0 / 260.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(width, h + 34.0), if enabled { Sense::click() } else { Sense::hover() });
        if ui.is_rect_visible(rect) {
            let img = egui::Rect::from_min_size(rect.min, vec2(width, h));
            match id.and_then(|id| self.art_uri("cards", id)) {
                Some(uri) => {
                    let mut image = egui::Image::new(uri).corner_radius(CornerRadius::same(8));
                    if !enabled {
                        image = image.tint(Color32::from_white_alpha(70));
                    }
                    crate::ui_util::paint_cover(ui, image, img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)));
                }
                None => {
                    ui.painter().rect_filled(img, CornerRadius::same(8), p.raised);
                    ui.painter().rect_stroke(img, CornerRadius::same(8), Stroke::new(1.5, p.border), egui::StrokeKind::Inside);
                    let label = if id.is_some() { "?" } else { "＋" };
                    ui.painter().text(img.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(28.0), p.weak);
                }
            }
            if let Some(lb) = lb {
                draw_lb(ui.painter(), egui::pos2(rect.left() + 2.0, img.bottom() + 9.0), lb, 5.0);
            }
            ui.painter().text(
                egui::pos2(rect.center().x, img.bottom() + 25.0),
                egui::Align2::CENTER_CENTER,
                truncate(caption, (width / 7.0) as usize),
                egui::FontId::proportional(12.0),
                if enabled { p.text } else { p.weak },
            );
            tile_ring(ui, img, false, enabled && resp.hovered(), 10);
        }
        // A click picks; right-click opens the card's details.
        if let Some(id) = id.filter(|_| resp.secondary_clicked()) {
            self.open_detail(crate::detail_modal::Detail::Card(id), None);
        }
        resp
    }

    fn deck_editor(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        ui.horizontal(|ui| {
            ui.label(semibold("Step 4 · Deck", 15.5));
            if ui.available_width() > 700.0 {
                ui.label(RichText::new("five of your cards plus one borrowed from a friend; click a slot to change it").size(12.0).color(p.weak));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let recommend = crate::ui_util::primary_button(ui, "✨ Recommend whole deck", 13.5);
                let how = "The meta sheet's cards for this CM and style come first (★), strongest backing first; simulated careers rank the rest.";
                if ui.add(recommend).on_hover_text(format!("Replace all six slots with the best fits for this goal and trainee. {how}")).clicked() {
                    let picks = self.recommend_deck(true);
                    self.toast(self.deck_fill_message(&picks, "Recommended"), crate::ui_util::ToastKind::Success);
                }
                if ui.button("✨ Fill empty slots").on_hover_text(format!("Keep the cards you chose and fill the rest. {how}")).clicked() {
                    let picks = self.recommend_deck(false);
                    self.toast(self.deck_fill_message(&picks, "Filled"), crate::ui_util::ToastKind::Success);
                }
            });
        });
        ui.add_space(4.0);
        let trainee_name = self.planner.plan.trainee.and_then(|t| self.owned_uma(t)).map(|u| u.name.clone());
        let mut open = None;
        let mut clear = None;
        // The totals sit beside the six slots when there's room, else below them.
        let beside = ui.available_width() >= 1000.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for slot in 0..6 {
                let is_friend = slot == 5;
                let id = if is_friend { self.planner.plan.friend } else { self.planner.plan.deck[slot] };
                let (lb, caption) = match id {
                    Some(id) if is_friend => (Some(self.planner.plan.friend_lb), self.kb.cards.get(&id).map_or(String::new(), |c| c.uma.clone())),
                    Some(id) => (self.owned_card(id).map(|c| c.lb), self.owned_card(id).map_or(String::new(), |c| c.uma.clone())),
                    None => (None, if is_friend { "Friend".to_string() } else { format!("Slot {}", slot + 1) }),
                };
                ui.vertical(|ui| {
                    let tile = self.art_tile(ui, id, 92.0, lb, &caption, true);
                    // Built only while hovered: the reason comes from the slot ranking.
                    let tile = tile.on_hover_ui(|ui| {
                        let Some(c) = id.and_then(|id| self.kb.cards.get(&id)) else {
                            ui.label(if is_friend { "Empty: click to borrow a friend's SSR card" } else { "Empty: click to choose one of your cards" });
                            return;
                        };
                        ui.label(semibold(format!("{} [{}]", c.uma, c.title), 13.5));
                        match self.card_value(c.id, ui.ctx()) {
                            Ok(Some((score, reason))) => {
                                ui.label(format!("Score {score:.0} · {reason}"));
                            }
                            Ok(None) => {}
                            Err(()) => {
                                ui.label(RichText::new("Working out why this card fits…").color(p.weak));
                            }
                        }
                        ui.label(RichText::new("Click to change").color(p.weak));
                    });
                    if tile.clicked() {
                        open = Some(slot);
                    }
                    ui.horizontal(|ui| {
                        if is_friend {
                            ui.label(RichText::new("LB").size(12.0).color(p.weak));
                            ui.add(egui::DragValue::new(&mut self.planner.plan.friend_lb).range(0..=4));
                        }
                        if id.is_some() && ui.small_btn("✖").on_hover_text("Remove").clicked() {
                            clear = Some(slot);
                        }
                    });
                });
            }
            if beside {
                ui.add_space(8.0);
                ui.vertical(|ui| self.deck_totals(ui));
            }
        });
        if !beside {
            ui.add_space(6.0);
            self.deck_totals(ui);
        }
        if let Some(slot) = open {
            self.planner.picker = Some(slot);
            self.planner.picker_search.clear();
        }
        match clear {
            Some(5) => self.planner.plan.friend = None,
            Some(s) => self.planner.plan.deck[s] = None,
            None => {}
        }

        // Warnings: the trainee's own cards, and two cards of one character.
        let mut charas: HashMap<String, usize> = HashMap::new();
        let mut warnings = Vec::new();
        for d in self.deck_cards() {
            if let Some(c) = self.kb.cards.get(&d.id) {
                *charas.entry(c.uma.clone()).or_default() += 1;
                if trainee_name.as_deref() == Some(c.uma.as_str()) {
                    warnings.push(format!("⚠ {} is the trainee; her card can't be in the deck.", c.uma));
                }
            }
        }
        for (name, n) in charas {
            if n > 1 {
                warnings.push(format!("⚠ Two {name} cards; a deck holds only one card per character."));
            }
        }
        for w in warnings {
            ui.label(RichText::new(w).color(pal(ui).warn));
        }

        let deck = self.deck_cards();
        if !deck.is_empty() {
            ui.add_space(4.0);
            egui::CollapsingHeader::new(RichText::new("✨ Why these cards").size(13.5)).id_salt("deck_why").show(ui, |ui| {
                let skills_how = if self.planner.current_skill_values(self.plan_key()).is_some() {
                    "simulated on the target course"
                } else {
                    "estimated from how their conditions fit (🏁 Simulate skills below to measure them)"
                };
                ui.label(
                    RichText::new(format!(
                        "Each card is scored by what it adds to a simulated {} career with the rest of the deck: stats toward the target stats \
                         (less once a stat passes its target), skill points, and the scenario link. Then the skills it can teach, {skills_how}, \
                         for this trainee's style and the course's distance, direction and slopes. Scores are stat-point equivalents. \
                         Suggestions from a model (knowledge/scenarios.json lists its assumptions), not a solved optimum.",
                        self.plan_scenario_name()
                    ))
                    .size(12.0)
                    .color(p.weak),
                );
                if self.ranking_busy() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Scoring the cards…").size(12.5).color(p.weak));
                    });
                }
                let ctx = ui.ctx().clone();
                egui::Grid::new("deck_why_grid").num_columns(3).spacing([12.0, 4.0]).striped(true).show(ui, |ui| {
                    for d in &deck {
                        let Some(c) = self.kb.cards.get(&d.id) else { continue };
                        let Ok(Some((score, reason))) = self.card_value(d.id, &ctx) else { continue };
                        ui.label(semibold(format!("{} {}", c.uma, if d.borrowed { "(friend)" } else { "" }), 13.5));
                        ui.label(RichText::new(format!("{score:.0}")).color(p.accent)).on_hover_text("Recommendation score before the repeat-type penalty");
                        ui.label(RichText::new(reason).size(12.5).color(p.weak));
                        ui.end_row();
                    }
                });
            });
        }
    }

    /// Modal grid of card art for the slot being edited.
    fn deck_picker(&mut self, ui: &mut egui::Ui) {
        let Some(slot) = self.planner.picker else { return };
        let is_friend = slot == 5;
        let ctx = ui.ctx().clone();
        let p = pal(ui);
        let trainee_name = self.planner.plan.trainee.and_then(|t| self.owned_uma(t)).map(|u| u.name.clone());
        let in_deck: Vec<String> = (0..5)
            .filter(|s| *s != slot)
            .filter_map(|s| self.planner.plan.deck[s])
            .chain(if is_friend { None } else { self.planner.plan.friend })
            .filter_map(|id| self.kb.cards.get(&id).map(|c| c.uma.clone()))
            .collect();
        // (id, uma, title, type, lb)
        let mut items: Vec<(u32, String, String, String, Option<u8>)> = if is_friend {
            let today = today();
            self.kb.cards.values().filter(|c| c.rarity == "SSR" && c.released(&today)).map(|c| (c.id, c.uma.clone(), c.title.clone(), c.kind.clone(), None)).collect()
        } else {
            self.roster.cards.iter().map(|c| (c.id, c.uma.clone(), c.title.clone(), c.card_type.label().to_string(), Some(c.lb))).collect()
        };
        let order = |t: &str| ["Speed", "Stamina", "Power", "Guts", "Wit", "Pal", "Group"].iter().position(|x| *x == t).unwrap_or(9);
        items.sort_by(|a, b| order(&a.3).cmp(&order(&b.3)).then(b.4.cmp(&a.4)).then(b.0.cmp(&a.0)));
        let ranked_ready = self.slot_candidates_ready(slot, &ctx);
        let ranking = ranked_ready.is_none();
        let ranked = ranked_ready.unwrap_or_default();
        let rank_of: HashMap<u32, (usize, f32, String)> = ranked.iter().enumerate().map(|(i, c)| (c.id, (i + 1, c.score, c.reason.clone()))).collect();

        let mut pick: Option<Option<u32>> = None;
        let modal = egui::Modal::new(egui::Id::unique("deck_picker")).show(&ctx, |ui| {
            ui.set_width(820.0);
            ui.horizontal(|ui| {
                ui.label(semibold(if is_friend { "Choose a friend card" } else { "Choose a card" }, 19.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✖ Close").clicked() {
                        pick = Some(self.planner.plan.deck.get(slot).copied().flatten().or(if is_friend { self.planner.plan.friend } else { None }));
                    }
                    if ui.button("Leave empty").clicked() {
                        pick = Some(None);
                    }
                });
            });
            if is_friend {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Any Global SSR, at the LB set under the slot. Find someone lending it:").size(12.5).color(p.weak));
                    // Friend databases from the community resource list (rentry.co/uma-resources).
                    if ui.link(RichText::new("uma.moe ↗").size(12.5)).on_hover_text("uma.moe: a friend database to search for a support card to borrow").clicked() {
                        open_url("https://uma.moe/");
                    }
                    if ui.link(RichText::new("ChronoGenesis ↗").size(12.5)).on_hover_text("ChronoGenesis: search trainers' showcased umas and MLB support cards").clicked() {
                        open_url("https://chronogenesis.net/friend_search");
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.planner.picker_search).hint_text("🔍 Search uma or title").desired_width(260.0));
                ui.selectable_value(&mut self.planner.picker_type, None, "All");
                for t in ["Speed", "Stamina", "Power", "Guts", "Wit", "Pal", "Group"] {
                    let text = RichText::new(format!("{} {t}", type_icon(t))).color(crate::ui_util::type_text(ui, t));
                    ui.selectable_value(&mut self.planner.picker_type, Some(t.to_string()), text);
                }
            });
            ui.add_space(6.0);
            let q = self.planner.picker_search.trim().to_lowercase();
            let height = (ctx.content_rect().height() - 260.0).clamp(300.0, 620.0);
            // A fixed height: a pop-up that shrinks while filtering may not grow back.
            egui::ScrollArea::vertical().min_scrolled_height(height).max_height(height).auto_shrink([false, false]).show(ui, |ui| {
                if q.is_empty() && self.planner.picker_type.is_none() && ranking {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Scoring every card for this slot (simulated careers)…").size(12.5).color(p.weak));
                    });
                    ui.separator();
                }
                if q.is_empty() && self.planner.picker_type.is_none() && !ranked.is_empty() {
                    ui.label(RichText::new("✨ RECOMMENDED FOR THIS SLOT").size(11.5).color(p.accent));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        for (i, c) in ranked.iter().take(7).enumerate() {
                            let Some(info) = self.kb.cards.get(&c.id) else { continue };
                            let lb = if is_friend { None } else { self.owned_card(c.id).map(|o| o.lb) };
                            ui.vertical(|ui| {
                                let resp = self.art_tile(ui, Some(c.id), 84.0, lb, &info.uma, true);
                                let resp = resp.on_hover_text(format!("#{} · score {:.0}\n{} [{}]\n{}", i + 1, c.score, info.uma, info.title, c.reason));
                                if resp.clicked() {
                                    pick = Some(Some(c.id));
                                }
                                let star = crate::meta::stars(c.meta);
                                ui.label(RichText::new(format!("#{} · {:.0} {star}", i + 1, c.score)).size(11.5).color(p.accent));
                            });
                        }
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    ui.label(RichText::new("ALL CARDS").size(11.5).color(p.weak));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 12.0);
                    for (id, uma, title, kind, lb) in &items {
                        if self.planner.picker_type.as_ref().is_some_and(|t| t != kind) {
                            continue;
                        }
                        if !q.is_empty() && !format!("{uma} {title}").to_lowercase().contains(&q) {
                            continue;
                        }
                        let blocked = if trainee_name.as_deref() == Some(uma.as_str()) {
                            Some("This is the trainee's own card")
                        } else if in_deck.contains(uma) {
                            Some("Another card of this character is already in the deck")
                        } else {
                            None
                        };
                        let resp = self.art_tile(ui, Some(*id), 84.0, *lb, uma, blocked.is_none());
                        let rank = rank_of.get(id).map(|(r, s, why)| format!("\n#{r} for this slot · score {s:.0}\n{why}")).unwrap_or_default();
                        let resp = resp.on_hover_text(format!("{uma} [{title}]\n{kind}{rank}{}", blocked.map(|b| format!("\n{b}")).unwrap_or_default()));
                        if resp.clicked() {
                            pick = Some(Some(*id));
                        }
                    }
                });
            });
        });
        if modal.should_close() && pick.is_none() {
            self.planner.picker = None;
        }
        if let Some(choice) = pick {
            if is_friend {
                self.planner.plan.friend = choice;
            } else {
                self.planner.plan.deck[slot] = choice;
            }
            self.planner.picker = None;
        }
    }

    /// Type counts, then each key support effect summed over the deck, in a compact grid.
    fn deck_totals(&self, ui: &mut egui::Ui) {
        let deck = self.deck_cards();
        let p = pal(ui);
        if deck.is_empty() {
            ui.label(RichText::new("Deck totals show here once you pick cards.").size(12.5).color(p.weak));
            return;
        }
        let mut types: BTreeMap<String, usize> = BTreeMap::new();
        for d in &deck {
            if let Some(c) = self.kb.cards.get(&d.id) {
                *types.entry(c.kind.clone()).or_default() += 1;
            }
        }
        ui.horizontal_wrapped(|ui| {
            for (t, n) in &types {
                badge(ui, format!("{} {t} ×{n}", type_icon(t)), type_color_str(t));
            }
            let borrowed = deck.iter().find(|d| d.borrowed).map(|d| format!(" · friend LB{} Lv{}", d.lb, d.level)).unwrap_or_default();
            ui.label(RichText::new(format!("{} cards{borrowed}", deck.len())).size(12.5).color(p.weak));
        });

        let sum = |kind: u32| -> i32 { deck.iter().filter_map(|d| self.kb.cards.get(&d.id).and_then(|c| c.effect(kind, d.level))).sum() };
        ui.add_space(4.0);
        ui.label(RichText::new("DECK TOTALS AT THESE LEVELS  ⓘ").size(11.0).color(p.weak))
            .on_hover_text("Sums of each card's effect at its level. Data, not a simulation.");
        let totals: Vec<(u32, &str, i32)> = effect::KEY.iter().map(|(k, n)| (*k, *n, sum(*k))).filter(|t| t.2 != 0).collect();
        let gap = 6.0;
        let cols = ((ui.available_width() + gap) / 150.0).floor().clamp(1.0, 4.0) as usize;
        let cell = (ui.available_width() - gap * (cols - 1) as f32) / cols as f32;
        egui::Grid::new("deck_totals").num_columns(cols).spacing([gap, gap]).show(ui, |ui| {
            for (i, (kind, name, v)) in totals.iter().enumerate() {
                egui::Frame::new().fill(p.raised).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                    ui.set_width(cell - 16.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(short_effect(*kind, name)).size(12.0).color(p.weak));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(semibold(v.to_string(), 14.0)));
                    });
                })
                .response
                .on_hover_text(*name);
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        let stats = ["Speed", "Stamina", "Power", "Guts", "Wit"];
        let parts: Vec<String> = stats
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let bonus = sum(effect::SPEED + i as u32);
                let init = sum(effect::INIT_SPEED + i as u32);
                (bonus != 0 || init != 0).then(|| format!("{s} +{bonus}/+{init}"))
            })
            .collect();
        if !parts.is_empty() {
            ui.add_space(2.0);
            ui.label(RichText::new(format!("Stat bonus / initial:  {}", parts.join("  ·  "))).size(12.0).color(p.weak));
        }
    }

    /// Every skill the deck hints, grouped by how it fits the target; collapsed, since the skill
    /// list below shows the same skills with their sources.
    fn deck_hints(&self, ui: &mut egui::Ui) {
        let deck = self.deck_cards();
        if deck.is_empty() {
            return;
        }
        let p = pal(ui);
        // Hint coverage
        let mut hints: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        for d in &deck {
            if let Some(c) = self.kb.cards.get(&d.id) {
                for s in c.hints.iter().chain(&c.event_skills) {
                    let list = hints.entry(*s).or_default();
                    if !list.contains(&c.uma) {
                        list.push(c.uma.clone());
                    }
                }
            }
        }
        let mut by_fit: BTreeMap<Fit, Vec<(u32, Vec<String>)>> = BTreeMap::new();
        for (s, cards) in hints {
            by_fit.entry(self.skill_fit(s)).or_default().push((s, cards));
        }
        let count = |f: Fit| by_fit.get(&f).map_or(0, Vec::len);
        let header = format!(
            "💡 Skill hints from the deck: {} fit · {} anywhere · {} wrong",
            count(Fit::Target),
            count(Fit::General),
            count(Fit::Other)
        );
        egui::CollapsingHeader::new(RichText::new(header).size(13.5)).id_salt("deck_hints").show(ui, |ui| {
            for (fit, title) in [(Fit::Target, "✔ Fits this target"), (Fit::General, "◯ Works anywhere"), (Fit::Other, "✖ Wrong style, distance or surface")] {
                let Some(list) = by_fit.get(&fit) else { continue };
                let color = match fit {
                    Fit::Target => pal(ui).good,
                    Fit::General => p.text,
                    Fit::Other => pal(ui).warn,
                };
                ui.label(RichText::new(format!("{title} ({})", list.len())).strong().color(color));
                ui.horizontal_wrapped(|ui| {
                    for (s, cards) in list {
                        self.skill_link(ui, *s, fit == Fit::Other);
                        ui.label(RichText::new(cards.join(", ")).size(11.0).color(p.weak));
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    /// Skills the plan can buy, with where each comes from: the trainee's own (unlocked), her
    /// events, the deck's hints and events, and the scenario's link golds for linked cards.
    pub(crate) fn plan_skill_candidates(&self) -> BTreeMap<u32, Vec<String>> {
        let mut cands: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        if let Some(id) = self.planner.plan.trainee
            && let (Some(info), Some(owned)) = (self.kb.umas.get(&id), self.owned_uma(id))
        {
            for s in &info.skills {
                if s.potential <= owned.potential {
                    cands.entry(s.id).or_default().push(if s.potential <= 1 { "innate".into() } else { format!("P{}", s.potential) });
                }
            }
            for &s in &info.event_skills {
                cands.entry(s).or_default().push("her events".into());
            }
        }
        let scenario = self.plan_scenario().cloned();
        for d in self.deck_cards() {
            if let Some(c) = self.kb.cards.get(&d.id) {
                for s in c.hints.iter().chain(&c.event_skills) {
                    cands.entry(*s).or_default().push(c.uma.clone());
                }
                if let Some(gold) = scenario.as_ref().and_then(|sc| sc.link_golds.get(&c.uma))
                    && let Some(id) = self.kb.skill_order.iter().find(|id| self.kb.skills[*id].name == *gold && self.kb.skills[*id].rarity >= 2)
                {
                    cands.entry(*id).or_default().push(format!("{} link (lyrics event)", c.uma));
                }
            }
        }
        cands
    }

    /// What the career simulation did and assumed, for hover text.
    pub(crate) fn career_explainer(&self, c: &CareerResult) -> String {
        let mut lines = vec![format!(
            "Per career on average: {:.0} trainings ({:.0} with a friendship/rainbow card), {:.0} races.",
            c.trainings_mean, c.rainbows_mean, c.races_mean
        )];
        if let Some(sc) = self.plan_scenario() {
            for e in &sc.extra_sp {
                lines.push(format!("+{:.0} SP: {}{}", e.sp, e.what, if e.sourced { "" } else { " (assumed)" }));
            }
            if !sc.link_bonus.assumed.is_empty() {
                lines.push(format!("Linked card: +{:.0} SP, +{:.0} stats ({}, assumed)", sc.link_bonus.sp, sc.link_bonus.stats, sc.link_bonus.assumed));
            }
            lines.push(format!("Training values: {}", sc.source));
        }
        if let Some(s) = &self.kb.scenarios {
            let a = &s.common.assumed;
            lines.push(format!(
                "Assumed: great mood, rest below {:.0} energy, {} optional races, objective races won {:.0}% of the time, chain events {:.0}%, random events {:.0}%.",
                a.rest_below_energy,
                a.optional_races,
                a.objective_win_rate * 100.0,
                a.chain_event_chance * 100.0,
                a.random_event_chance * 100.0
            ));
            lines.push(format!("Rules from knowledge/scenarios.json (updated {}). Log real careers with their final SP in Results to check the model.", s.updated));
        }
        lines.join("\n")
    }

    /// "Recommended 6 cards: 4 from the meta sheet's …, 2 from simulated careers."
    pub(crate) fn deck_fill_message(&self, picks: &[crate::recommend::CardPick], verb: &str) -> String {
        let n = picks.len();
        let from_meta = picks.iter().filter(|c| c.meta > 0.0).count();
        let cards = format!("{n} card{}", if n == 1 { "" } else { "s" });
        match self.plan_meta_label() {
            Some(label) => format!(
                "{verb} {cards}: {from_meta} ★ from the meta sheet's picks for {label}, {} from simulated {} careers. Hover a card to see why.",
                n - from_meta,
                self.plan_scenario_name()
            ),
            None => format!("{verb} {cards} from simulated {} careers. Hover a card to see why; they're suggestions from a model.", self.plan_scenario_name()),
        }
    }

    /// The quick pick: choose the skills to buy from what's known now (`recommend_skills`: chart
    /// gains or estimates, no re-simulation) and say what it chose. The optimiser refines it.
    pub(crate) fn auto_pick_skills(&mut self) -> String {
        let ids: Vec<u32> = self.pick_rows().into_keys().collect();
        self.planner.plan.skills = self.recommend_skills(&ids);
        let meta = self.plan_meta_skills();
        let n = self.planner.plan.skills.len();
        let from_meta = self.planner.plan.skills.iter().filter(|s| meta.get(s).is_some_and(|(w, _)| *w >= 1.0)).count();
        let budget = self.sp_budget().map_or(String::new(), |b| format!(" within the {b:.0} SP budget"));
        let meta_note = match self.plan_meta_label() {
            Some(label) if from_meta > 0 => format!(", {from_meta} of them ★ recommended by the meta sheet for {label}"),
            Some(label) => format!("; nothing this deck teaches is on the meta sheet's list for {label}"),
            None => String::new(),
        };
        format!("Picked {n} skill{}{budget}{meta_note}.", if n == 1 { "" } else { "s" })
    }

    /// SP the chosen skills cost after expected hints, counting a ○/white once when its ◎/gold is
    /// also chosen.
    pub(crate) fn chosen_cost(&self, chosen: &[u32], career: Option<&CareerResult>) -> f64 {
        chosen
            .iter()
            .map(|id| {
                let lower_chosen = self.lower_version(*id).is_some_and(|l| chosen.contains(&l));
                let full = self.skill_cost(*id, career);
                if lower_chosen { full - self.skill_cost(self.lower_version(*id).unwrap_or(*id), career) } else { full }
            })
            // An empty sum is -0.0, which prints as "-0".
            .sum::<f64>()
            + 0.0
    }

    /// The plan's scenario name: its own, else the one current for the next Champions Meeting.
    pub(crate) fn plan_scenario_name(&self) -> String {
        let s = &self.planner.plan.scenario;
        if !s.is_empty() {
            return s.clone();
        }
        self.kb.current_scenario(&today()).unwrap_or_else(|| "URA Finale".into())
    }

    /// Umalator's paste format for one uma; returns the JSON and the skills it had to leave out.
    pub(crate) fn umalator_build(&self) -> Result<(String, Vec<String>), String> {
        self.umalator_uma().map(|(uma, dropped)| (uma.to_string(), dropped))
    }

    /// A umalator.app link that opens with this build as Uma 1 and Uma 2, on the plan's course and
    /// conditions. Returns the link and notes on anything it had to guess or leave out.
    pub(crate) fn umalator_link(&self) -> Result<(String, Vec<String>), String> {
        let (uma, dropped) = self.umalator_uma()?;
        let mut notes = Vec::new();
        if !dropped.is_empty() {
            notes.push(format!("left out (Umalator doesn't know them): {}", dropped.join(", ")));
        }
        let mut state = serde_json::json!({ "uma1": uma, "uma2": uma });
        let setup = self.plan_race_setup();
        notes.extend(setup.notes);
        if let Some(c) = setup.course {
            state["courseId"] = c.into();
        }
        for (key, v) in [("ground", setup.ground), ("weather", setup.weather), ("season", setup.season), ("time", setup.time)] {
            if let Some(v) = v {
                state[key] = v.into();
            }
        }
        Ok((format!("{UMALATOR_URL}#{}", umalator_hash(&state)), notes))
    }

    /// Hash of the goal skill values depend on: trainee, stats, style, course and conditions.
    pub(crate) fn plan_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let p = &self.planner.plan;
        // Many callers per frame: hash the plan fields first and reuse the key while they match.
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (p.trainee, p.target, p.style.label(), p.course, p.umalator_course, &p.cm_date, &p.conditions, p.distance.label(), p.surface.label()).hash(&mut h);
        let inputs = h.finish();
        if let Some((i, key)) = self.planner.key_memo.get()
            && i == inputs
        {
            return key;
        }
        let s = self.plan_race_setup();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (p.trainee, p.target, p.style.label(), s.course, s.ground, s.weather, s.season, s.time).hash(&mut h);
        let key = h.finish();
        self.planner.key_memo.set(Some((inputs, key)));
        key
    }

    /// The plan's Umalator course and condition codes: the CM preset's exact course when the goal
    /// still runs on it, else the race's course (preferring a layout an event uses); conditions from
    /// the plan's chips, then its CM or the nearest event on that course.
    pub(crate) fn plan_race_setup(&self) -> RaceSetup {
        let p = &self.planner.plan;
        let kb = &self.kb;
        let mut notes = Vec::new();
        let race = p.course.and_then(|id| kb.races.iter().find(|r| r.id == id));
        // A CM preset's course is exact, layout included, as long as the goal still runs on it.
        let exact = p.umalator_course.and_then(|id| kb.umalator_courses.iter().find(|c| c.id == id)).filter(|c| match race {
            Some(r) => runs_on(r, c),
            None => Distance::of_metres(c.distance) == p.distance && c.surface == p.surface.label(),
        });
        // Otherwise: Umalator has inner and outer layouts where the race data doesn't; prefer one an event uses.
        let course = exact.or_else(|| race.and_then(|race| {
            let fits: Vec<_> = kb.umalator_courses.iter().filter(|c| runs_on(race, c)).collect();
            let pick = fits.iter().find(|c| kb.umalator_presets.iter().any(|e| e.course == c.id)).or(fits.first()).copied();
            match pick {
                Some(c) if fits.len() > 1 => notes.push(format!("picked the {} course; check the layout", c.layout)),
                None => notes.push(format!("{} isn't in Umalator; pick the course there", race.name)),
                _ => {}
            }
            pick
        }));
        // Conditions from the plan, else from the plan's CM or the nearest event on this course,
        // else Umalator's defaults.
        let today = today();
        let linked = self.plan_cm();
        let preset = course.and_then(|c| {
            let on_course = || kb.umalator_presets.iter().filter(move |e| e.course == c.id);
            linked.filter(|e| e.course == c.id).or_else(|| on_course().min_by_key(|e| days_apart(&e.date, &today)))
        });
        if let Some(e) = preset.filter(|e| exact.is_some() && !e.announced) {
            notes.push(format!("the {}'s course and conditions are estimated, not announced yet", e.name));
        }
        let mut from_event = false;
        let mut codes = [None; 3];
        for (k, (group, event_code)) in [(&GROUNDS, preset.map(|e| e.ground)), (&WEATHERS, preset.map(|e| e.weather)), (&SEASONS, preset.map(|e| e.season))]
            .into_iter()
            .enumerate()
        {
            let planned = group.iter().position(|v| has_condition(&p.conditions, v)).map(|i| i as u8 + 1);
            from_event |= planned.is_none() && event_code.is_some();
            codes[k] = planned.or(event_code);
        }
        if let Some(e) = preset
            && from_event
        {
            notes.push(format!("conditions the plan leaves open come from {} ({})", e.name, e.date));
        }
        RaceSetup { course: course.map(|c| c.id), ground: codes[0], weather: codes[1], season: codes[2], time: preset.map(|e| e.time), notes }
    }

    fn umalator_uma(&self) -> Result<(serde_json::Value, Vec<String>), String> {
        let p = &self.planner.plan;
        let id = p.trainee.ok_or("Pick a trainee first.")?;
        let owned = self.owned_uma(id).ok_or("The trainee isn't in your roster.")?;
        let info = self.kb.umas.get(&id).ok_or("No knowledge data for this trainee.")?;
        if p.target.iter().all(|v| *v == 0) {
            return Err("Enter target stats first.".into());
        }
        let known = |s: &u32| self.kb.umalator_skills.is_empty() || self.kb.umalator_skills.contains(s);
        let mut skills: Vec<u32> = info.unique_skill.into_iter().collect();
        let mut dropped = Vec::new();
        for s in &p.skills {
            if known(s) {
                skills.push(*s);
            } else {
                dropped.push(self.kb.skill_name(*s));
            }
        }
        let a = &info.aptitude;
        let stars = owned.stars.clamp(1, 5);
        let json = serde_json::json!({
            "outfitId": id.to_string(),
            "starCount": stars,
            "speed": p.target[0],
            "stamina": p.target[1],
            "power": p.target[2],
            "guts": p.target[3],
            "wisdom": p.target[4],
            "strategy": p.style.umalator(),
            "distanceAptitude": a.distance(p.distance),
            "surfaceAptitude": a.surface(p.surface),
            "strategyAptitude": a.style(p.style),
            // Umalator order: four distances, four styles, then turf and dirt.
            "aptitudes": [&a.sprint, &a.mile, &a.medium, &a.long, &a.front, &a.pace, &a.late, &a.end, &a.turf, &a.dirt],
            "skills": skills.iter().map(u32::to_string).collect::<Vec<_>>(),
            "uniqueLv": stars % 3 + stars / 3,
            "mood": 2,
            "popularity": 1,
        });
        Ok((json, dropped))
    }

    pub(crate) fn save_plan(&mut self) {
        let Some(root) = self.root.clone() else {
            self.toast("umaLord folder not found.", crate::ui_util::ToastKind::Error);
            return;
        };
        let Some(trainee) = self.planner.plan.trainee.and_then(|t| self.owned_uma(t)).cloned() else {
            self.toast("Pick a trainee first.", crate::ui_util::ToastKind::Warn);
            return;
        };
        if self.planner.plan.title.trim().is_empty() {
            self.planner.plan.title = format!("{} [{}] — {}", trainee.name, trainee.outfit, self.planner.plan.purpose.label());
        }
        let dir = root.join("plans");
        let path = self.planner.open_file.clone().unwrap_or_else(|| {
            let base = format!("{}-{}-{}", self.planner.plan.created, slugify(&trainee.name), self.planner.plan.purpose.slug());
            let mut path = dir.join(format!("{base}.md"));
            let mut n = 2;
            while path.exists() {
                path = dir.join(format!("{base}-{n}.md"));
                n += 1;
            }
            path
        });
        // Keep what Claude wrote below "## Simulation" in an existing file.
        let kept = std::fs::read_to_string(&path).ok().and_then(|old| {
            let start = old.find("## Simulation")?;
            let end = old.find(PLAN_MARKER).unwrap_or(old.len());
            Some(old[start..end].trim_end().to_string())
        });
        let text = self.plan_markdown(kept);
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, text)) {
            Ok(()) => {
                self.toast_action(format!("Saved {}", path.display()), crate::ui_util::ToastKind::Success, "Show", crate::ui_util::ToastAction::Open(path.clone()));
                self.planner.open_file = Some(path);
                self.planner.refresh_files(Some(&root));
                self.home_dirty();
            }
            Err(e) => self.toast(format!("Couldn't save: {e}"), crate::ui_util::ToastKind::Error),
        }
    }

    pub(crate) fn plan_markdown(&self, kept: Option<String>) -> String {
        use std::fmt::Write;
        let p = &self.planner.plan;
        let mut md = String::new();
        let _ = writeln!(md, "# {}\n", p.title);
        let course = p
            .course
            .and_then(|c| self.kb.races.iter().find(|r| r.id == c))
            .map(|r| format!("{} {} {}m ({}), {}", r.track, r.surface, r.distance, r.direction, r.name))
            .unwrap_or_else(|| "not set".into());
        let _ = writeln!(md, "## Goal\n");
        let _ = writeln!(md, "- Purpose: {}", p.purpose.label());
        let _ = writeln!(md, "- Scenario: {}", self.plan_scenario_name());
        let _ = writeln!(md, "- Target: {} {}, {}", p.distance.label(), p.surface.label(), p.style.label());
        let _ = writeln!(md, "- Course: {course}");
        if let Some(e) = self.plan_cm() {
            let status = if e.announced { "announced".to_string() } else { "estimated, not announced yet".to_string() };
            let _ = writeln!(md, "- Champions Meeting: {} ({}), {}, {}; {status}", e.name, e.date, e.course_name, e.conditions);
            if !e.disabled_skills.is_empty() {
                let names: Vec<String> = e.disabled_skills.iter().map(|s| self.kb.skill_name(*s)).collect();
                let _ = writeln!(md, "- Won't activate in this CM: {}", names.join(", "));
            }
        }
        if let Some(c) = p.umalator_course {
            let _ = writeln!(md, "- Umalator course id: {c}");
        }
        if !p.conditions.is_empty() {
            let _ = writeln!(md, "- Conditions: {}", p.conditions);
        }
        if !p.spark_goal.is_empty() {
            let _ = writeln!(md, "- Sparks wanted: {}", p.spark_goal);
        }
        let _ = writeln!(md, "- Created: {} with the umaLord app\n", p.created);

        let _ = writeln!(md, "## Trainee\n");
        if let Some((u, info)) = p.trainee.and_then(|t| Some((self.owned_uma(t)?, self.kb.umas.get(&t)?))) {
            let a = &info.aptitude;
            let _ = writeln!(md, "- {} [{}], {}★, Potential {}", u.name, u.outfit, u.stars, u.potential);
            let _ = writeln!(
                md,
                "- Aptitude for the target: {} {}, {} {}, {} {}",
                p.distance.label(),
                a.distance(p.distance),
                p.surface.label(),
                a.surface(p.surface),
                p.style.label(),
                a.style(p.style)
            );
            if let Some(us) = info.unique_skill {
                let _ = writeln!(md, "- Unique: {}", self.kb.skill_name(us));
            }
        }
        let _ = writeln!(md, "\n## Deck\n\n| Slot | Card | Type | LB | Level |\n|---|---|---|---|---|");
        for (i, d) in self.deck_cards().iter().enumerate() {
            if let Some(c) = self.kb.cards.get(&d.id) {
                let slot = if d.borrowed { "Friend (borrowed)".to_string() } else { (i + 1).to_string() };
                let _ = writeln!(md, "| {slot} | {} [{}] ({}) | {} | {} | {} |", c.uma, c.title, c.rarity, c.kind, d.lb, d.level);
            }
        }
        let deck = self.deck_cards();
        let sum = |kind: u32| -> i32 { deck.iter().filter_map(|d| self.kb.cards.get(&d.id).and_then(|c| c.effect(kind, d.level))).sum() };
        let _ = writeln!(md, "\nDeck totals at these levels (sums of card effects; data, not simulation):\n");
        let totals: Vec<String> = effect::KEY.iter().filter(|(k, _)| sum(*k) != 0).map(|(k, n)| format!("{n} {}", sum(*k))).collect();
        let _ = writeln!(md, "{}\n", totals.join(" · "));

        let career = self.plan_career();
        let simulated = self.planner.current_skill_values(self.plan_key()).map(|v| v.label.clone());
        let pick = self.current_pick();
        let _ = writeln!(md, "## Skills to buy\n\n| Skill | SP | After hints | Gain alone | In this set | Effect | Fits target |\n|---|---|---|---|---|---|---|");
        for s in &p.skills {
            if let Some(k) = self.kb.skills.get(s) {
                let fit = match self.skill_fit(*s) {
                    Fit::Target => "yes",
                    Fit::General => "anywhere",
                    Fit::Other => "no",
                };
                let (gain, sim) = match pick.and_then(|o| o.singles.get(s)) {
                    Some(g) => (*g, true),
                    None => self.skill_lengths(*s),
                };
                let gain = if sim { format!("{gain:+.2} L") } else { format!("~{gain:.2} L (est.)") };
                let in_set = pick.and_then(|o| o.marginal.get(s)).map_or("—".to_string(), |g| format!("{g:+.2} L"));
                let pin = if p.pinned_skills.contains(s) { " (pinned)" } else { "" };
                let _ = writeln!(md, "| {}{pin} | {} | {:.0} | {gain} | {in_set} | {} | {fit} |", k.name, k.sp, self.skill_cost(*s, career.as_ref()), k.effect_text());
            }
        }
        let _ = writeln!(md);
        if let Some(v) = pick.and_then(|o| o.verify.as_ref()) {
            let _ = writeln!(
                md,
                "Auto-pick (simulator output, realistic races with wit checks and rushing on): the set gains {:+.2} L in {} races against the trainee with only her unique (95%: {:+.2} to {:+.2}); its skills' single gains add up to {:.2} L. Aim: {}; meta sheet: {}{}.\n",
                v.mean,
                v.runs,
                v.lo,
                v.hi,
                v.singles_sum,
                p.pick_objective.label(),
                p.meta_influence.label(),
                if p.fast_learner { "; Fast Learner" } else { "" }
            );
        }
        if let Some(label) = &simulated {
            let _ = writeln!(md, "Gain alone: simulator output (each skill alone on {label}, in realistic races, against the trainee with only her unique; gains don't add up, see In this set).");
        } else {
            let _ = writeln!(md, "Gains: estimates from how each skill's conditions fit the target (opinion, not simulated).");
        }
        let spent = self.chosen_cost(&p.skills, career.as_ref());
        match (p.sp_budget, &career) {
            (Some(b), _) => {
                let _ = writeln!(md, "SP: {spent:.0} of a {b} budget set by hand.");
            }
            (None, Some(c)) => {
                let _ = writeln!(
                    md,
                    "SP: {spent:.0} of ≈{:.0} expected at the end of the career (career simulation of this deck in {}, 10–90%: {:.0}–{:.0}; a model whose assumptions are in knowledge/scenarios.json).",
                    c.sp_mean,
                    self.plan_scenario_name(),
                    c.sp_p10,
                    c.sp_p90
                );
            }
            _ => {}
        }
        let _ = writeln!(
            md,
            "\n## Target stats\n\n| Speed | Stamina | Power | Guts | Wit |\n|---|---|---|---|---|\n| {} | {} | {} | {} | {} |\n",
            p.target[0], p.target[1], p.target[2], p.target[3], p.target[4]
        );
        let parents: Vec<&str> = p.parents.iter().map(String::as_str).filter(|s| !s.is_empty()).collect();
        let _ = writeln!(md, "## Parents\n\n{}\n", if parents.is_empty() { "Not set.".to_string() } else { parents.iter().map(|s| format!("- {s}")).collect::<Vec<_>>().join("\n") });
        if !p.notes.is_empty() {
            let _ = writeln!(md, "## Notes\n\n{}\n", p.notes);
        }
        match kept {
            Some(k) => {
                let _ = writeln!(md, "{k}\n");
            }
            None => {
                let _ = writeln!(
                    md,
                    "## Simulation\n\nNot run yet. Press ▶ Simulate in the planner, then 💾 Save to plan in the Race sim tab, or ask Claude to simulate it.\n\n## Recommendation\n\nPending.\n\n## Outcome\n\nLink the matching entry in `results/` when it's in.\n"
                );
            }
        }
        let _ = writeln!(md, "{PLAN_MARKER}\n{}\n-->", serde_json::to_string_pretty(p).unwrap_or_default());
        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("T.M. Opera O"), "t-m-opera-o");
        assert_eq!(slugify("Special Week"), "special-week");
    }

    #[test]
    fn condition_chips_edit_the_string() {
        let s = set_condition("", &GROUNDS, "Firm");
        let s = set_condition(&s, &SEASONS, "Winter");
        let s = set_condition(&s, &WEATHERS, "Sunny");
        assert_eq!(s, "Firm · Sunny · Winter");
        assert_eq!(set_condition(&s, &GROUNDS, "Heavy"), "Heavy · Sunny · Winter");
        assert_eq!(set_condition(&s, &WEATHERS, "Sunny"), "Firm · Winter");
        assert_eq!(set_condition("night race, Firm", &SEASONS, "Spring"), "Firm · Spring · night race");
        assert!(has_condition("Firm · Sunny", "sunny"));
    }

    #[test]
    fn plan_round_trips_through_json() {
        let mut p = Plan::default();
        p.trainee = Some(100601);
        p.deck[0] = Some(30076);
        p.skills = vec![200512];
        let back: Plan = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.trainee, Some(100601));
        assert_eq!(back.deck[0], Some(30076));
        assert_eq!(back.skills, vec![200512]);
    }
}
