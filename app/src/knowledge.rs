//! Game knowledge from `knowledge/data/*.json`, built by `tools/update_knowledge.py` from
//! uma.guide's Global data. Read-only here; rebuild the JSON to refresh it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const KNOWLEDGE_DIR: &str = "knowledge/data";

// ---------------------------------------------------------------------------
// Aptitudes

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Default)]
pub enum Surface {
    #[default]
    Turf,
    Dirt,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Default)]
pub enum Distance {
    Sprint,
    Mile,
    #[default]
    Medium,
    Long,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Default)]
pub enum Style {
    Front,
    #[default]
    Pace,
    Late,
    End,
}

impl Style {
    /// With its article, for a sentence: "a Pace Chaser", "an End Closer".
    pub fn article(self) -> String {
        let l = self.label();
        format!("{} {l}", if l.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" })
    }
}

impl Surface {
    pub const ALL: [Surface; 2] = [Surface::Turf, Surface::Dirt];
    pub fn label(self) -> &'static str {
        match self {
            Self::Turf => "Turf",
            Self::Dirt => "Dirt",
        }
    }
    /// `ground_type` value in skill conditions.
    fn code(self) -> u8 {
        self as u8 + 1
    }
}

impl Distance {
    pub const ALL: [Distance; 4] = [Distance::Sprint, Distance::Mile, Distance::Medium, Distance::Long];
    pub fn label(self) -> &'static str {
        match self {
            Self::Sprint => "Sprint",
            Self::Mile => "Mile",
            Self::Medium => "Medium",
            Self::Long => "Long",
        }
    }
    /// `distance_type` value in skill conditions.
    fn code(self) -> u8 {
        self as u8 + 1
    }
    /// Category of a race distance in metres.
    pub fn of_metres(m: u32) -> Self {
        match m {
            0..=1400 => Self::Sprint,
            1401..=1800 => Self::Mile,
            1801..=2400 => Self::Medium,
            _ => Self::Long,
        }
    }
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Front, Style::Pace, Style::Late, Style::End];
    pub fn label(self) -> &'static str {
        match self {
            Self::Front => "Front Runner",
            Self::Pace => "Pace Chaser",
            Self::Late => "Late Surger",
            Self::End => "End Closer",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Front => "Front",
            Self::Pace => "Pace",
            Self::Late => "Late",
            Self::End => "End",
        }
    }
    /// `running_style` value in skill conditions.
    fn code(self) -> u8 {
        self as u8 + 1
    }
    /// Umalator's strategy names.
    pub fn umalator(self) -> &'static str {
        match self {
            Self::Front => "Nige",
            Self::Pace => "Senkou",
            Self::Late => "Sasi",
            Self::End => "Oikomi",
        }
    }
}

/// S > A > ... > G; unknown grades rank lowest.
pub fn grade_rank(g: &str) -> u8 {
    match g.trim() {
        "S" => 8,
        "A" => 7,
        "B" => 6,
        "C" => 5,
        "D" => 4,
        "E" => 3,
        "F" => 2,
        "G" => 1,
        _ => 0,
    }
}


#[derive(Deserialize, Clone, Debug, Default)]
pub struct Aptitude {
    pub turf: String,
    pub dirt: String,
    pub sprint: String,
    pub mile: String,
    pub medium: String,
    pub long: String,
    pub front: String,
    pub pace: String,
    pub late: String,
    pub end: String,
}

impl Aptitude {
    pub fn surface(&self, s: Surface) -> &str {
        match s {
            Surface::Turf => &self.turf,
            Surface::Dirt => &self.dirt,
        }
    }
    pub fn distance(&self, d: Distance) -> &str {
        match d {
            Distance::Sprint => &self.sprint,
            Distance::Mile => &self.mile,
            Distance::Medium => &self.medium,
            Distance::Long => &self.long,
        }
    }
    pub fn style(&self, s: Style) -> &str {
        match s {
            Style::Front => &self.front,
            Style::Pace => &self.pace,
            Style::Late => &self.late,
            Style::End => &self.end,
        }
    }
    /// The style with the best grade (first wins ties).
    pub fn best_style(&self) -> Style {
        Style::ALL.into_iter().max_by_key(|s| (grade_rank(self.style(*s)), 4 - *s as u8)).unwrap_or(Style::Pace)
    }
}

// ---------------------------------------------------------------------------
// Records

#[derive(Deserialize, Clone, Debug)]
pub struct PotentialSkill {
    pub id: u32,
    /// Potential level that unlocks it; 1 = innate.
    pub potential: u8,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Evolution {
    pub base: u32,
    pub evolved: u32,
    /// Alternatives: any one inner list, all of whose conditions hold.
    pub conditions: Vec<Vec<String>>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Objective {
    pub when: String,
    pub race: String,
    pub goal: String,
    #[serde(default)]
    pub race_id: Option<u32>,
    /// "Specific_Race", "Fan_Count", …
    #[serde(default, rename = "type")]
    pub kind: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct EventOption {
    pub label: String,
    pub outcomes: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct UmaEvent {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub options: Vec<EventOption>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct UmaInfo {
    pub id: u32,
    pub name: String,
    pub outfit: String,
    pub release: String,
    /// Speed, Stamina, Power, Guts, Wit.
    pub base: [i32; 5],
    /// Growth rate bonus in percent, same order.
    pub growth: [i32; 5],
    pub aptitude: Aptitude,
    pub unique_skill: Option<u32>,
    pub skills: Vec<PotentialSkill>,
    pub event_skills: Vec<u32>,
    pub evolved: Vec<Evolution>,
    pub objectives: Vec<Objective>,
    pub events: Vec<UmaEvent>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct EffectTable {
    #[serde(rename = "type")]
    pub kind: u32,
    pub name: String,
    /// (level, value) pairs; values between levels are interpolated.
    pub points: Vec<(u32, i32)>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct NamedValue {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct UniqueEffect {
    pub level: u32,
    pub condition: String,
    pub effects: Vec<NamedValue>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CardEvent {
    pub title: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// Options; each option lists alternative outcomes (one happens at random), each a list of lines.
    pub options: Vec<Vec<Vec<String>>>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CardInfo {
    pub id: u32,
    pub uma: String,
    pub title: String,
    pub rarity: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub release: String,
    pub effects: Vec<EffectTable>,
    pub unique: Option<UniqueEffect>,
    pub hints: Vec<u32>,
    pub event_skills: Vec<u32>,
    pub events: Vec<CardEvent>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SkillEffect {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: f64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AltActivation {
    pub conditions: String,
    pub effects: Vec<SkillEffect>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SkillInfo {
    pub id: u32,
    pub name: String,
    pub desc: String,
    pub category: String,
    pub rarity: u8,
    pub sp: u32,
    pub wit_check: bool,
    pub conditions: String,
    pub pre: String,
    pub effects: Vec<SkillEffect>,
    pub duration: f64,
    pub cooldown: f64,
    #[serde(default)]
    pub alt: Option<AltActivation>,
    #[serde(default)]
    pub summary: String,
    /// Evaluation points: what the skill adds to an uma's rating (uma.guide's data; purple skills
    /// take points off).
    #[serde(default)]
    pub grade: i32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct RaceInfo {
    pub id: u32,
    pub name: String,
    pub grade: String,
    pub distance: u32,
    pub category: String,
    pub surface: String,
    pub direction: String,
    pub track: String,
    pub track_id: Option<u32>,
}

/// A spark (the game's factor) as a finished uma's data names it (`sparks.json`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SparkInfo {
    pub id: u32,
    pub name: String,
    /// blue, pink, green, white, race, scenario or bonus.
    pub kind: String,
    pub stars: u8,
}

/// A win record (the game's saddle): one race, or a title such as the Triple Crown that stands
/// for several (`saddles.json`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Saddle {
    pub name: String,
    /// Race ids, as in races.json.
    pub races: Vec<u32>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GlossaryEntry {
    pub term: String,
    pub aliases: Vec<String>,
    pub description: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
struct Meta {
    built: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
struct UmalatorMeta {
    #[serde(default)]
    built: String,
    skills: Vec<u32>,
    #[serde(default)]
    courses: Vec<UmalatorCourse>,
    #[serde(default)]
    presets: Vec<EventPreset>,
}

/// One of Umalator's courses; `layout` is "inner", "outer", "outer→inner" or empty.
#[derive(Deserialize, Clone, Debug)]
pub struct UmalatorCourse {
    pub id: u32,
    pub track_id: u32,
    pub distance: u32,
    pub surface: String,
    #[serde(default)]
    pub layout: String,
}

/// Where an event's dates and conditions come from, best first.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EventSource {
    /// The official Global news (umamusume.com).
    Official,
    /// Moomoolator's presets, fetched live from umalator.app.
    Umalator,
    /// `knowledge/data/umalator.json`, shipped with the app.
    #[default]
    Bundled,
}

impl EventSource {
    pub fn label(self) -> &'static str {
        match self {
            EventSource::Official => "umamusume.com",
            EventSource::Umalator => "umalator.app",
            EventSource::Bundled => "the bundled data",
        }
    }
}

/// A Champions Meeting or League of Heroes event: its course and conditions, from Umalator's
/// presets, with the official announcement's dates, conditions and disabled skills on top
/// (`live::merge`). Condition codes are 1-based in GROUNDS / WEATHERS / SEASONS order; time 2..4
/// is Midday..Night.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EventPreset {
    /// The event's number, e.g. 20 for CM 20.
    #[serde(default)]
    pub id: u32,
    /// "Champions Meeting" or "League of Heroes".
    #[serde(default)]
    pub event: String,
    pub name: String,
    pub date: String,
    pub course: u32,
    /// e.g. "Nakayama Turf 2500m (inner)".
    #[serde(default)]
    pub course_name: String,
    pub ground: u8,
    pub weather: u8,
    pub season: u8,
    pub time: u8,
    /// e.g. "Good · Cloudy · Winter · Midday".
    #[serde(default)]
    pub conditions: String,
    /// Umalator's own flag: false until it considers the course and conditions final.
    #[serde(default)]
    pub confirmed: bool,
    /// When league selection opens, Round 1 starts and the event ends (UTC seconds), from the
    /// official announcement.
    #[serde(default)]
    pub league_selection: Option<i64>,
    #[serde(default)]
    pub round1: Option<i64>,
    #[serde(default)]
    pub ends: Option<i64>,
    /// Skills that won't activate in this event (official announcement).
    #[serde(default)]
    pub disabled_skills: Vec<u32>,
    #[serde(default)]
    pub source: EventSource,
    /// The official site has announced it: dates and conditions are final.
    #[serde(default)]
    pub announced: bool,
    /// The official article's id (`live::article_url`).
    #[serde(default)]
    pub article: Option<u32>,
    /// The conditions as the announcement wrote them.
    #[serde(default)]
    pub official_conditions: Option<String>,
}

impl EventPreset {
    pub fn is_cm(&self) -> bool {
        self.event == "Champions Meeting"
    }

    /// "Capricorn Cup · Oct 31".
    pub fn short_label(&self) -> String {
        format!("{} · {}", self.name, crate::live::short_day(&self.date))
    }

    /// "Announced on umamusume.com" or "Estimated by umalator.app".
    pub fn status(&self) -> String {
        if self.announced {
            format!("Announced on {}", self.source.label())
        } else {
            format!("Estimated by {}", self.source.label())
        }
    }

    /// A line for hovers: whether the date, course and conditions are final.
    pub fn status_explained(&self) -> String {
        if self.announced {
            format!("Announced on {}: the dates, course and conditions are official.", self.source.label())
        } else {
            format!(
                "Estimated: {} guesses the date, course and conditions until the official announcement, which can change them.",
                if self.source == EventSource::Bundled { "the bundled Umalator data" } else { "umalator.app" }
            )
        }
    }
}

/// One entry of uma.guide's Champions Meeting list (`cm_schedule.json`), with the training
/// scenario that was current for it.
#[derive(Deserialize, Clone, Debug)]
pub struct CmEvent {
    #[serde(default)]
    pub series: String,
    pub number: u32,
    #[serde(default)]
    pub scenario: String,
}

// ---------------------------------------------------------------------------
// Effects and skills

/// A support effect's value at `level`: linear between listed levels, rounded down, as uma.guide
/// shows it. `None` before the first listed level (the card doesn't have the effect yet).
pub fn effect_at(points: &[(u32, i32)], level: u32) -> Option<i32> {
    let first = points.first()?;
    if level < first.0 {
        return None;
    }
    let i = points.iter().rposition(|p| p.0 <= level)?;
    let (x0, y0) = points[i];
    match points.get(i + 1) {
        Some(&(x1, y1)) if x1 > x0 => {
            let t = (level - x0) as f64 / (x1 - x0) as f64;
            Some((y0 as f64 + (y1 - y0) as f64 * t).floor() as i32)
        }
        _ => Some(y0),
    }
}

impl CardInfo {
    pub fn effect(&self, kind: u32, level: u32) -> Option<i32> {
        self.effects.iter().find(|e| e.kind == kind).and_then(|e| effect_at(&e.points, level))
    }

    /// Out on Global by `today` (YYYY-MM-DD). uma.guide dates cards it lists ahead of their
    /// Global release 2038-01-19.
    pub fn released(&self, today: &str) -> bool {
        self.release.is_empty() || self.release.as_str() <= today
    }
}

/// Support effect type ids, as in the data.
pub mod effect {
    pub const FRIENDSHIP: u32 = 1;
    pub const MOOD: u32 = 2;
    /// Speed Bonus; Stamina..Wit bonuses follow as 4..7.
    pub const SPEED: u32 = 3;
    pub const TRAINING: u32 = 8;
    /// Initial Speed; initial Stamina..Wit follow as 10..13.
    pub const INIT_SPEED: u32 = 9;
    pub const INIT_BOND: u32 = 14;
    pub const RACE: u32 = 15;
    pub const HINT_LEVELS: u32 = 17;
    pub const HINT_FREQ: u32 = 18;
    pub const SPECIALTY: u32 = 19;
    pub const FAIL_PROTECT: u32 = 27;
    pub const ENERGY_COST: u32 = 28;
    pub const SKILL_POINTS: u32 = 30;
    pub const WIT_RECOVERY: u32 = 31;

    /// Effects worth comparing when ranking or summing a deck.
    pub const KEY: [(u32, &str); 12] = [
        (FRIENDSHIP, "Friendship Bonus"),
        (TRAINING, "Training Effectiveness"),
        (MOOD, "Mood Effect"),
        (SPECIALTY, "Specialty Priority"),
        (RACE, "Race Bonus"),
        (HINT_FREQ, "Hint Frequency"),
        (HINT_LEVELS, "Hint Levels"),
        (INIT_BOND, "Initial Friendship Gauge"),
        (FAIL_PROTECT, "Failure Protection"),
        (ENERGY_COST, "Energy Cost Reduction"),
        (SKILL_POINTS, "Skill Point Bonus"),
        (WIT_RECOVERY, "Wit Friendship Recovery"),
    ];
}

/// What a skill's activation conditions restrict it to. Empty means "any".
#[derive(Default, Clone, Debug, PartialEq)]
pub struct SkillTags {
    pub styles: Vec<Style>,
    pub distances: Vec<Distance>,
    pub surfaces: Vec<Surface>,
    pub tracks: Vec<u32>,
}

impl SkillTags {
    pub fn fits(&self, style: Option<Style>, distance: Option<Distance>, surface: Option<Surface>) -> bool {
        style.is_none_or(|s| self.styles.is_empty() || self.styles.contains(&s))
            && distance.is_none_or(|d| self.distances.is_empty() || self.distances.contains(&d))
            && surface.is_none_or(|s| self.surfaces.is_empty() || self.surfaces.contains(&s))
    }

    pub fn is_general(&self) -> bool {
        self.styles.is_empty() && self.distances.is_empty() && self.surfaces.is_empty() && self.tracks.is_empty()
    }
}

fn condition_values(cond: &str, key: &str) -> Vec<u32> {
    cond.split(['&', '@', '\n'])
        .filter_map(|c| c.trim().strip_prefix(key)?.strip_prefix("==")?.trim().parse().ok())
        .collect()
}

impl SkillInfo {
    fn all_conditions(&self) -> String {
        let mut s = format!("{}&{}", self.conditions, self.pre);
        if let Some(alt) = &self.alt {
            s.push('&');
            s.push_str(&alt.conditions);
        }
        s
    }

    pub fn tags(&self) -> SkillTags {
        let c = self.all_conditions();
        let mut t = SkillTags {
            styles: Style::ALL.into_iter().filter(|s| condition_values(&c, "running_style").contains(&(s.code() as u32))).collect(),
            distances: Distance::ALL
                .into_iter()
                .filter(|d| condition_values(&c, "distance_type").contains(&(d.code() as u32)))
                .collect(),
            surfaces: Surface::ALL.into_iter().filter(|s| condition_values(&c, "ground_type").contains(&(s.code() as u32))).collect(),
            tracks: condition_values(&c, "track_id"),
        };
        t.tracks.sort_unstable();
        t.tracks.dedup();
        t
    }

    pub fn tier(&self) -> &'static str {
        match self.category.as_str() {
            "Unique" if self.rarity >= 3 => "Unique",
            "Unique" => "Inherited unique",
            "Evolved" => "Evolved",
            _ if self.rarity >= 2 => "Gold",
            _ => "White",
        }
    }

    /// e.g. "Target Speed +0.35, HP Recovery +5.5%".
    pub fn effect_text(&self) -> String {
        effects_text(&self.effects)
    }
}

pub fn effects_text(effects: &[SkillEffect]) -> String {
    effects
        .iter()
        .map(|e| match e.kind.as_str() {
            "Stamina Recovery" => format!("HP Recovery {:+}%", (e.value * 1000.0).round() / 10.0),
            k if k.ends_with(" Stat") => format!("{k} {:+}", e.value),
            "Change Lane" => format!("Target Lane +{}", e.value),
            k => format!("{k} {:+}", e.value),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Where a skill can come from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SkillSource {
    Unique(u32),
    /// Innate (potential 1) or unlocked at that potential level.
    Potential(u32, u8),
    UmaEvent(u32),
    Evolved { uma: u32, base: u32 },
    Hint(u32),
    CardEvent(u32),
}

impl SkillSource {
    pub fn uma(self) -> Option<u32> {
        match self {
            Self::Unique(u) | Self::Potential(u, _) | Self::UmaEvent(u) | Self::Evolved { uma: u, .. } => Some(u),
            _ => None,
        }
    }
    pub fn card(self) -> Option<u32> {
        match self {
            Self::Hint(c) | Self::CardEvent(c) => Some(c),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Store

#[derive(Default)]
pub struct Knowledge {
    pub umas: HashMap<u32, UmaInfo>,
    pub cards: HashMap<u32, CardInfo>,
    pub skills: HashMap<u32, SkillInfo>,
    /// Skill ids sorted by name.
    pub skill_order: Vec<u32>,
    pub races: Vec<RaceInfo>,
    pub tracks: BTreeMap<u32, String>,
    pub glossary: Vec<GlossaryEntry>,
    pub sources: HashMap<u32, Vec<SkillSource>>,
    /// Skill ids Umalator (umalator.app) can load; empty if unknown.
    pub umalator_skills: HashSet<u32>,
    pub umalator_courses: Vec<UmalatorCourse>,
    pub umalator_presets: Vec<EventPreset>,
    pub cm_schedule: Vec<CmEvent>,
    /// Scenario rules for the career simulation (`knowledge/scenarios.json`).
    pub scenarios: Option<crate::career::Scenarios>,
    /// The community PvP meta sheet (`pvp_meta.json`).
    pub pvp_meta: Option<crate::meta::PvpMeta>,
    /// The community skill tier list (`skill_tiers.json`).
    pub skill_tiers: Option<crate::notes::SkillTiers>,
    /// The game's affinity tables (`affinity.json`, or a newer copy the live refresh fetched).
    pub affinity: crate::affinity::AffinityTable,
    /// Spark (factor) ids and win record (saddle) ids, for decoding finished umas.
    pub sparks: HashMap<u32, SparkInfo>,
    pub saddles: HashMap<u32, Saddle>,
    /// The sourced glossary and tips from `knowledge/mechanics.md`.
    pub terms: Vec<crate::notes::Term>,
    pub tips: Vec<crate::notes::Tip>,
    /// When `umalator.json` (the simulator's skills, courses and CM presets) was built.
    pub umalator_built: Option<String>,
    pub built: Option<String>,
    pub error: Option<String>,
}

fn read<T: for<'de> Deserialize<'de>>(dir: &Path, file: &str) -> Result<T, String> {
    let text = std::fs::read_to_string(dir.join(file)).map_err(|e| format!("{file}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{file}: {e}"))
}

impl Knowledge {
    /// The G1 races a finished uma's win records stand for, by races.json name (a race won at two
    /// venues, or in two years, is one name). Affinity counts these.
    pub fn g1_wins(&self, saddles: &[u32]) -> std::collections::BTreeSet<String> {
        saddles
            .iter()
            .filter_map(|s| self.saddles.get(s))
            .flat_map(|s| s.races.iter())
            .filter_map(|id| self.races.iter().find(|r| r.id == *id && r.grade == "G1"))
            .map(|r| r.name.clone())
            .collect()
    }
}

/// The day the Global server opened.
pub const GLOBAL_LAUNCH: &str = "2025-06-26";

impl Knowledge {
    pub fn load(root: Option<&Path>) -> Knowledge {
        let Some(root) = root else {
            return Knowledge { error: Some("umaLord folder not found".into()), ..Default::default() };
        };
        let dir = root.join(KNOWLEDGE_DIR);
        let mut k = match Self::load_dir(&dir) {
            Ok(k) => k,
            Err(e) => Knowledge {
                error: Some(format!("{e} (run: python -I -X utf8 tools/update_knowledge.py)")),
                ..Default::default()
            },
        };
        k.scenarios = crate::career::Scenarios::load(Some(root));
        let mechanics = std::fs::read_to_string(root.join("knowledge").join("mechanics.md")).unwrap_or_default();
        k.terms = crate::notes::parse_terms(&mechanics);
        k.tips = crate::notes::parse_tips(&mechanics);
        k
    }

    fn load_dir(dir: &Path) -> Result<Knowledge, String> {
        let umas: Vec<UmaInfo> = read(dir, "umas.json")?;
        let cards: Vec<CardInfo> = read(dir, "cards.json")?;
        let skills: Vec<SkillInfo> = read(dir, "skills.json")?;
        let races: Vec<RaceInfo> = read(dir, "races.json")?;
        let glossary: Vec<GlossaryEntry> = read(dir, "glossary.json").unwrap_or_default();
        let meta: Meta = read(dir, "meta.json").unwrap_or_default();
        let umalator: UmalatorMeta = read(dir, "umalator.json").unwrap_or_default();
        let mut k = Self::from_parts(umas, cards, skills, races, glossary, Some(meta.built).filter(|b| !b.is_empty()));
        k.umalator_skills = umalator.skills.into_iter().collect();
        k.umalator_courses = umalator.courses;
        k.umalator_presets = umalator.presets;
        k.umalator_built = Some(umalator.built).filter(|b| !b.is_empty());
        k.cm_schedule = read(dir, "cm_schedule.json").unwrap_or_default();
        k.pvp_meta = read(dir, "pvp_meta.json").ok();
        k.skill_tiers = crate::notes::SkillTiers::load(&dir.join("skill_tiers.json"));
        k.affinity = read(dir, "affinity.json").unwrap_or_default();
        k.sparks = read::<Vec<SparkInfo>>(dir, "sparks.json").unwrap_or_default().into_iter().map(|s| (s.id, s)).collect();
        k.saddles = read(dir, "saddles.json").unwrap_or_default();
        Ok(k)
    }

    pub fn from_parts(
        mut umas: Vec<UmaInfo>,
        mut cards: Vec<CardInfo>,
        skills: Vec<SkillInfo>,
        races: Vec<RaceInfo>,
        glossary: Vec<GlossaryEntry>,
        built: Option<String>,
    ) -> Knowledge {
        // uma.guide dates what Global launched with by JP's calendar (2017-01-01 for umas,
        // 2021-02-24 for cards); on Global they all arrived on launch day.
        for r in umas.iter_mut().map(|u| &mut u.release).chain(cards.iter_mut().map(|c| &mut c.release)) {
            if !r.is_empty() && r.as_str() < GLOBAL_LAUNCH {
                *r = GLOBAL_LAUNCH.to_string();
            }
        }
        let mut sources: HashMap<u32, Vec<SkillSource>> = HashMap::new();
        let mut add = |skill: u32, src: SkillSource| {
            let list = sources.entry(skill).or_default();
            if !list.contains(&src) {
                list.push(src);
            }
        };
        for u in &umas {
            if let Some(s) = u.unique_skill {
                add(s, SkillSource::Unique(u.id));
            }
            for s in &u.skills {
                add(s.id, SkillSource::Potential(u.id, s.potential));
            }
            for &s in &u.event_skills {
                add(s, SkillSource::UmaEvent(u.id));
            }
            for e in &u.evolved {
                add(e.evolved, SkillSource::Evolved { uma: u.id, base: e.base });
            }
        }
        for c in &cards {
            for &s in &c.hints {
                add(s, SkillSource::Hint(c.id));
            }
            for &s in &c.event_skills {
                add(s, SkillSource::CardEvent(c.id));
            }
        }
        let mut skill_order: Vec<u32> = skills.iter().map(|s| s.id).collect();
        let skills: HashMap<u32, SkillInfo> = skills.into_iter().map(|s| (s.id, s)).collect();
        skill_order.sort_by(|a, b| skills[a].name.to_lowercase().cmp(&skills[b].name.to_lowercase()).then(a.cmp(b)));
        let tracks = races.iter().filter_map(|r| Some((r.track_id?, r.track.clone()))).collect();
        Knowledge {
            umas: umas.into_iter().map(|u| (u.id, u)).collect(),
            cards: cards.into_iter().map(|c| (c.id, c)).collect(),
            skills,
            skill_order,
            races,
            tracks,
            glossary,
            sources,
            umalator_skills: HashSet::new(),
            umalator_courses: Vec::new(),
            umalator_presets: Vec::new(),
            cm_schedule: Vec::new(),
            scenarios: None,
            pvp_meta: None,
            skill_tiers: None,
            affinity: Default::default(),
            sparks: HashMap::new(),
            saddles: HashMap::new(),
            terms: Vec::new(),
            tips: Vec::new(),
            umalator_built: None,
            built,
            error: None,
        }
    }

    /// The training scenario that was (or will be) current for a Champions Meeting, per uma.guide.
    /// The CM's scenario when it's one Global has: a later CM's listed scenario can be JP's, a
    /// projection, which a plan for Global must not train in.
    pub fn cm_global_scenario(&self, preset: &EventPreset) -> Option<&str> {
        self.cm_scenario(preset).filter(|s| self.scenarios.as_ref().is_some_and(|g| g.scenarios.iter().any(|x| x.global && x.matches(s))))
    }

    pub fn cm_scenario(&self, preset: &EventPreset) -> Option<&str> {
        self.cm_schedule
            .iter()
            .find(|c| c.series == "CM" && c.number == preset.id && preset.is_cm())
            .map(|c| c.scenario.as_str())
            .filter(|s| !s.is_empty())
    }

    /// The scenario of the next Champions Meeting from `today`, else the latest one.
    pub fn current_scenario(&self, today: &str) -> Option<String> {
        let next = self.upcoming_cms(today).into_iter().find_map(|e| self.cm_global_scenario(e).map(String::from));
        next.or_else(|| {
            let mut past: Vec<&EventPreset> = self.umalator_presets.iter().filter(|e| e.is_cm()).collect();
            past.sort_by(|a, b| b.date.cmp(&a.date));
            past.into_iter().find_map(|e| self.cm_global_scenario(e).map(String::from))
        })
    }

    pub fn is_loaded(&self) -> bool {
        !self.skills.is_empty()
    }

    pub fn skill_name(&self, id: u32) -> String {
        self.skills.get(&id).map_or_else(|| format!("Skill {id}"), |s| s.name.clone())
    }

    pub fn track_name(&self, id: u32) -> String {
        self.tracks.get(&id).cloned().unwrap_or_else(|| format!("Track {id}"))
    }

    /// Champions Meeting events from `today` (YYYY-MM-DD) on, soonest first: those starting
    /// today or later, and one still running (its official end hasn't passed).
    pub fn upcoming_cms(&self, today: &str) -> Vec<&EventPreset> {
        let mut out: Vec<_> = self
            .umalator_presets
            .iter()
            .filter(|e| e.is_cm() && (e.date.as_str() >= today || e.ends.is_some_and(|t| crate::live::utc_date(t).as_str() >= today)))
            .collect();
        out.sort_by(|a, b| a.date.cmp(&b.date));
        out
    }

    /// The Champions Meeting a plan was made for. Live data moves estimated dates (and can change
    /// the course), so it matches the cup's name within 21 days of the saved date first, then the
    /// exact date, then the same course within 21 days.
    pub fn find_cm(&self, date: &str, course: Option<u32>, name: Option<&str>) -> Option<&EventPreset> {
        let cms = || self.umalator_presets.iter().filter(|e| e.is_cm());
        let saved = crate::live::date_epoch(date);
        let near = move |e: &&EventPreset| saved.zip(crate::live::date_epoch(&e.date)).is_some_and(|(a, b)| (a - b).abs() <= 21 * 86_400);
        cms()
            .filter(near)
            .find(|e| name.is_some_and(|n| e.name.eq_ignore_ascii_case(n)))
            .or_else(|| cms().find(|e| e.date == date))
            .or_else(|| cms().filter(near).find(|e| course == Some(e.course)))
    }

    /// Human tag line for a skill, e.g. "Pace Chaser · Medium · Tokyo".
    pub fn tag_line(&self, tags: &SkillTags) -> String {
        let mut parts: Vec<String> = Vec::new();
        parts.extend(tags.styles.iter().map(|s| s.label().to_string()));
        parts.extend(tags.distances.iter().map(|d| d.label().to_string()));
        parts.extend(tags.surfaces.iter().map(|s| s.label().to_string()));
        parts.extend(tags.tracks.iter().map(|t| self.track_name(*t)));
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_interpolation_matches_uma_guide() {
        // Yamanin Zephyr [To Winds Untraveled]: uma.guide shows Lv30..50 as below.
        let friendship = [(1, 10), (25, 15), (30, 15), (45, 20)];
        let training = [(1, 1), (30, 10), (50, 15)];
        let init_speed = [(45, 15), (50, 30)];
        let at = |p: &[(u32, i32)]| [30, 35, 40, 45, 50].map(|l| effect_at(p, l));
        assert_eq!(at(&friendship), [Some(15), Some(16), Some(18), Some(20), Some(20)]);
        assert_eq!(at(&training), [Some(10), Some(11), Some(12), Some(13), Some(15)]);
        assert_eq!(at(&init_speed), [None, None, None, Some(15), Some(30)]);
    }

    #[test]
    fn skill_tags_from_conditions() {
        let s = SkillInfo {
            id: 1,
            name: "x".into(),
            desc: String::new(),
            category: "Speed Boost".into(),
            rarity: 1,
            sp: 100,
            wit_check: true,
            conditions: "running_style==2&distance_type==3&phase==1@running_style==3&track_id==10006".into(),
            pre: String::new(),
            effects: vec![],
            duration: 0.0,
            cooldown: 0.0,
            alt: None,
            summary: String::new(),
            grade: 0,
        };
        let t = s.tags();
        assert_eq!(t.styles, vec![Style::Pace, Style::Late]);
        assert_eq!(t.distances, vec![Distance::Medium]);
        assert_eq!(t.tracks, vec![10006]);
        assert!(t.fits(Some(Style::Pace), Some(Distance::Medium), Some(Surface::Turf)));
        assert!(!t.fits(Some(Style::Front), None, None));
    }

    #[test]
    fn grades_order() {
        assert!(grade_rank("S") > grade_rank("A") && grade_rank("A") > grade_rank("G"));
        let apt = Aptitude { front: "G".into(), pace: "A".into(), late: "A".into(), end: "C".into(), ..Default::default() };
        assert_eq!(apt.best_style(), Style::Pace);
    }

    /// The built knowledge files load and agree with what uma.guide shows.
    #[test]
    fn knowledge_files_load() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let k = Knowledge::load(Some(root));
        assert!(k.error.is_none(), "{:?}", k.error);
        let spe = &k.umas[&100101];
        assert_eq!(k.skill_name(spe.unique_skill.unwrap()), "Shooting Star");
        assert_eq!(spe.aptitude.medium, "A");
        let zephyr = &k.cards[&30126];
        assert_eq!(zephyr.effect(effect::FRIENDSHIP, 35), Some(16));
        assert!(k.sources[&spe.unique_skill.unwrap()].contains(&SkillSource::Unique(100101)));
    }
}
