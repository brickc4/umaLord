//! Race sim: Moomoolator's simulator running inside the app (see `sim/`). Compare two builds over
//! many runs, chart which skills gain the most, check stamina, or estimate HP, on any course.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};

use crate::App;
use crate::ui_util::UiExt;
use crate::knowledge::{Distance, Style};
use crate::sim::builder::build_skill_data;
use crate::sim::course::Course;
use crate::sim::data::SimData;
use crate::sim::hpcalc;
use crate::sim::params::{APTITUDES, HorseDesc, NIGE, OIKOMI, OONIGE, SASI, SENKOU, base_stats, parse_aptitude, strategy_name};
use crate::sim::run::{ChartRow, CompareResult, HpCalcResult, Progress, SimOptions, Trace, UmaBuild, race_params, run_chart, run_comparison, run_hpcalc};
use crate::theme::size;
use crate::sim::solver::Perspective;
use crate::theme::pal;
use crate::ui_util::{card, card_titled, page_header, search_combo, semibold, stat_row, tier_color};

pub const GROUNDS: [&str; 4] = ["Firm", "Good", "Soft", "Heavy"];
pub const WEATHERS: [&str; 4] = ["Sunny", "Cloudy", "Rainy", "Snowy"];
pub const SEASONS: [&str; 4] = ["Spring", "Summer", "Autumn", "Winter"];
pub const TIMES: [(i32, &str); 3] = [(2, "Midday"), (3, "Evening"), (4, "Night")];
const MOODS: [&str; 5] = ["Awful", "Bad", "Normal", "Good", "Great"];
const STRATEGIES: [i32; 5] = [NIGE, SENKOU, SASI, OIKOMI, OONIGE];
/// Moomoolator shows finish times as simulated seconds × 1.18 (the game's clock).
const GAME_CLOCK: f64 = 1.18;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Compare,
    Chart,
    Stamina,
    HpCalc,
}

/// The results dashboard's tabs.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ResultTab {
    Overview,
    Race,
    Skills,
    Stamina,
    Runs,
}

/// Leave-one-out ("what each skill is worth") or +50 per stat ("where training points matter").
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ExtraKind {
    Contribution,
    Sensitivity,
}

pub(crate) struct ExtraJob {
    kind: ExtraKind,
    progress: Arc<Progress>,
    rx: mpsc::Receiver<Vec<(String, f64)>>,
}

/// A finished Compare, kept for the Runs tab.
#[derive(Clone)]
pub(crate) struct HistoryEntry {
    pub label: String,
    pub verdict: String,
    pub when: String,
    pub gaps: Vec<f64>,
}

/// The replay of the median race on the course strip.
pub(crate) struct Replay {
    /// Seconds into the race (simulated).
    pub t: f32,
    pub playing: bool,
    pub speed: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ChartPool {
    /// Skills this account can get, plus Uma 1's own.
    Account,
    AllGlobal,
}

enum JobResult {
    Compare(CompareResult),
    Chart(Vec<ChartRow>),
    Stamina(HpCalcResult),
}

pub(crate) struct Job {
    progress: Arc<Progress>,
    rx: mpsc::Receiver<JobResult>,
    label: String,
}

pub(crate) struct SimState {
    pub mode: Mode,
    pub course: u32,
    pub ground: i32,
    pub weather: i32,
    pub season: i32,
    pub time: i32,
    pub opts: SimOptions,
    pub auto_seed: bool,
    pub umas: [UmaBuild; 2],
    pub heal_pct: f64,
    pub force_full: bool,
    pub chart_pool: ChartPool,
    pub job: Option<Job>,
    pub compare: Option<CompareResult>,
    pub chart: Option<Vec<ChartRow>>,
    pub chart_per_sp: bool,
    pub stamina: Option<HpCalcResult>,
    /// The setup came from the planner, so results can be saved back to the plan.
    pub from_plan: bool,
    /// The run's inputs, frozen when it started, for saving and labels.
    pub ran: Option<RunInfo>,
    pub result_tab: ResultTab,
    /// Overview's chart: the cumulative share instead of the histogram.
    pub cdf: bool,
    /// The last 10 Compares, oldest first, and the one pinned as a ghost.
    pub history: Vec<HistoryEntry>,
    pub pinned: Option<usize>,
    pub extra: Option<ExtraJob>,
    /// Leave-one-out and +50 stat results for the last run: (label, lengths).
    pub contribution: Option<Vec<(String, f64)>>,
    pub sensitivity: Option<Vec<(String, f64)>>,
    pub replay: Replay,
    /// Where the results were drawn last frame, and a pending "Save chart as PNG".
    pub results_rect: Option<Rect>,
    pub save_png: bool,
}

#[derive(Clone)]
pub(crate) struct RunInfo {
    pub course: u32,
    pub conditions: String,
    pub umas: [UmaBuild; 2],
    pub opts: SimOptions,
    /// Ground, weather, season, time of day.
    pub race: (i32, i32, i32, i32),
}

/// The settings a run starts from, copied so the state can change while it runs.
struct SimSnapshot {
    mode: Mode,
    ground: i32,
    weather: i32,
    season: i32,
    time: i32,
    force_full: bool,
}

pub fn default_uma() -> UmaBuild {
    // Moomoolator's Global defaults.
    UmaBuild {
        outfit: None,
        stars: 3,
        unique_level: 1,
        desc: HorseDesc {
            speed: 1200.0,
            stamina: 1200.0,
            power: 800.0,
            guts: 400.0,
            wisdom: 400.0,
            strategy: SENKOU,
            distance_apt: 0,
            surface_apt: 1,
            strategy_apt: 1,
            mood: 2,
        },
        skills: Vec::new(),
        forced: HashMap::new(),
    }
}

impl Default for SimState {
    fn default() -> Self {
        SimState {
            mode: Mode::Compare,
            course: 10506,
            ground: 1,
            weather: 1,
            season: 4,
            time: 2,
            opts: SimOptions::default(),
            auto_seed: true,
            umas: [default_uma(), default_uma()],
            heal_pct: 0.0,
            force_full: false,
            chart_pool: ChartPool::Account,
            job: None,
            compare: None,
            chart: None,
            chart_per_sp: false,
            stamina: None,
            from_plan: false,
            ran: None,
            result_tab: ResultTab::Overview,
            cdf: false,
            history: Vec::new(),
            pinned: None,
            extra: None,
            contribution: None,
            sensitivity: None,
            replay: Replay { t: 0.0, playing: false, speed: 1.0 },
            results_rect: None,
            save_png: false,
        }
    }
}

pub fn style_of(strategy: i32) -> Style {
    match strategy {
        SENKOU => Style::Pace,
        SASI => Style::Late,
        OIKOMI => Style::End,
        _ => Style::Front,
    }
}

pub fn strategy_of(style: Style) -> i32 {
    match style {
        Style::Front => NIGE,
        Style::Pace => SENKOU,
        Style::Late => SASI,
        Style::End => OIKOMI,
    }
}

fn distance_of(course: &Course) -> Distance {
    match course.distance_type {
        1 => Distance::Sprint,
        2 => Distance::Mile,
        3 => Distance::Medium,
        _ => Distance::Long,
    }
}

/// "2:25.9" from simulated seconds, on the game's clock.
pub fn game_time(sim_seconds: f64) -> String {
    let t = sim_seconds * GAME_CLOCK;
    format!("{}:{:04.1}", (t / 60.0).floor(), t % 60.0)
}

pub fn conditions_text(ground: i32, weather: i32, season: i32, time: i32) -> String {
    fn pick(list: &[&'static str; 4], code: i32) -> &'static str {
        list.get((code - 1).clamp(0, 3) as usize).copied().unwrap_or("?")
    }
    let t = TIMES.iter().find(|(c, _)| *c == time).map_or("Midday", |(_, n)| n);
    format!("{} · {} · {} · {t}", pick(&GROUNDS, ground), pick(&WEATHERS, weather), pick(&SEASONS, season))
}

fn new_seed() -> f64 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() ^ (d.as_secs() as u32)).unwrap_or(1);
    f64::from(nanos % 0x7fff_ffff)
}

impl App {
    /// Fill an uma's aptitudes from her data for the current course and style.
    pub(crate) fn fill_aptitudes(&mut self, i: usize) {
        let Some(course) = self.sim_data.as_ref().and_then(|d| d.courses.get(&self.sim.course)).cloned() else { return };
        let u = &mut self.sim.umas[i];
        let Some(info) = u.outfit.and_then(|o| self.kb.umas.get(&o)) else { return };
        let a = &info.aptitude;
        u.desc.distance_apt = parse_aptitude(a.distance(distance_of(&course)));
        u.desc.surface_apt = parse_aptitude(if course.surface == 2 { &a.dirt } else { &a.turf });
        u.desc.strategy_apt = parse_aptitude(a.style(style_of(u.desc.strategy)));
    }

    /// Pick an outfit: stars from the roster, its unique skill, aptitudes and best style.
    pub(crate) fn set_outfit(&mut self, i: usize, outfit: u32) {
        let stars = self.owned_uma(outfit).map_or(3, |u| u.stars);
        let Some(data) = self.sim_data.clone() else { return };
        let u = &mut self.sim.umas[i];
        let old_unique = u.outfit.and_then(|o| data.unique_skill(o, u.stars));
        u.skills.retain(|s| Some(*s) != old_unique);
        u.outfit = Some(outfit);
        u.stars = stars;
        u.unique_level = u32::from(stars % 3 + stars / 3).max(1);
        if let Some(s) = data.unique_skill(outfit, stars) {
            u.skills.insert(0, s);
        }
        if let Some(info) = self.kb.umas.get(&outfit) {
            u.desc.strategy = strategy_of(info.aptitude.best_style());
        }
        self.fill_aptitudes(i);
    }

    fn skill_label(&self, id: u32) -> String {
        match self.kb.skills.get(&id) {
            Some(s) => s.name.clone(),
            None => self.sim_data.as_ref().map_or(format!("Skill {id}"), |d| d.skill_name(id)),
        }
    }

    pub(crate) fn sim_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.sim_poll(&ctx);
        egui::Panel::left("sim_race").resizable(false).exact_size(300.0).frame(crate::side_frame()).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.sim_race_panel(ui));
        });
        egui::CentralPanel::default().frame(crate::page_frame()).show(ui, |ui| {
            let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if let Some(y) = self.debug_scroll {
                area = area.vertical_scroll_offset(y);
            }
            area.show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(1180.0));
                self.sim_main(ui);
            });
        });
    }

    /// Collect a finished run, and keep repainting while one is going.
    pub(crate) fn sim_poll(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.sim.extra {
            match job.rx.try_recv() {
                Ok(items) => {
                    let kind = job.kind;
                    self.sim.extra = None;
                    match kind {
                        ExtraKind::Contribution => self.sim.contribution = Some(items),
                        ExtraKind::Sensitivity => self.sim.sensitivity = Some(items),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(mpsc::TryRecvError::Disconnected) => self.sim.extra = None,
            }
        }
        let Some(job) = &self.sim.job else { return };
        match job.rx.try_recv() {
            Ok(result) => {
                match result {
                    JobResult::Compare(r) => {
                        if !r.warnings.is_empty() {
                            self.toast(format!("Left out: {}", r.warnings.join("; ")), crate::ui_util::ToastKind::Warn);
                        }
                        self.remember_run(&r);
                        self.sim.contribution = None;
                        self.sim.sensitivity = None;
                        self.sim.replay = Replay { t: 0.0, playing: false, speed: self.sim.replay.speed };
                        self.sim.compare = Some(r);
                    }
                    JobResult::Chart(rows) => self.sim.chart = Some(rows),
                    JobResult::Stamina(r) => self.sim.stamina = Some(r),
                }
                self.sim.job = None;
            }
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.toast("The simulation stopped unexpectedly.", crate::ui_util::ToastKind::Error);
                self.sim.job = None;
            }
        }
    }

    /// The left panel, "Race setup": the CM, the course, conditions, what to test, and the
    /// advanced options folded away. Fits a 960×600 window without scrolling.
    fn sim_race_panel(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        ui.horizontal(|ui| {
            ui.label(semibold("Race setup", size::LG));
            ui.label(RichText::new("ⓘ").color(p.weak)).on_hover_text(
                "A Rust port of Moomoolator's engine (umalator.app, engine v2). It simulates the umas against a virtual pacemaker; \
                 order conditions count as met. The same seed gives the same results as the site.",
            );
        });
        let Some(data) = self.sim_data.clone() else {
            ui.label(RichText::new(self.sim_error.clone().unwrap_or_default()).color(p.warn));
            ui.label(RichText::new("Run: python -I -X utf8 tools/update_knowledge.py --only umalator").size(size::SM).color(p.weak));
            return;
        };
        ui.add_space(2.0);

        // The next Champions Meetings, one per row, and earlier ones in a dropdown.
        let today = crate::ui_util::today();
        let upcoming: Vec<_> = self.kb.upcoming_cms(&today).into_iter().take(3).cloned().collect();
        let mut past: Vec<_> = self.kb.umalator_presets.iter().filter(|e| e.is_cm() && e.date < today && !upcoming.iter().any(|u| u.name == e.name && u.date == e.date)).cloned().collect();
        past.sort_by(|a, b| b.date.cmp(&a.date));
        let mut chosen = None;
        for e in &upcoming {
            let on = self.sim.course == e.course && self.sim.ground == i32::from(e.ground) && self.sim.season == i32::from(e.season);
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
            let fill = if on { p.accent.gamma_multiply(if ui.visuals().dark_mode { 0.28 } else { 0.16 }) } else if resp.hovered() { p.hover } else { egui::Color32::TRANSPARENT };
            ui.painter().rect(rect, CornerRadius::same(7), fill, Stroke::new(1.0, if on { p.accent_strong } else { p.border }), egui::StrokeKind::Inside);
            let (mark, status, color) = if e.announced { ("✔", "announced", p.good) } else { ("?", "estimated", p.warn) };
            ui.painter().text(rect.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, format!("{mark} {}", e.short_label()), FontId::proportional(size::SM), p.text);
            ui.painter().text(rect.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, status, FontId::proportional(size::XS), color);
            let scenario = self.kb.cm_scenario(e).map(|s| format!("\nScenario: {s}")).unwrap_or_default();
            let off = if e.disabled_skills.is_empty() { String::new() } else { format!("\n{} skills won't activate; picking it drops them from both builds.", e.disabled_skills.len()) };
            if resp.on_hover_text(format!("{} ({})\n{} · {}{scenario}{off}\n{}", e.name, e.date, e.course_name, e.conditions, e.status_explained())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                chosen = Some(e.clone());
            }
            ui.add_space(2.0);
        }
        if !past.is_empty() {
            egui::ComboBox::from_id_salt("sim_past_cm").width(ui.available_width() - 8.0).selected_text(RichText::new("Earlier Champions Meetings…").size(size::SM)).show_ui(ui, |ui| {
                for e in past.iter().take(12) {
                    if ui.selectable_label(false, format!("{} · {}", e.short_label(), e.course_name)).clicked() {
                        chosen = Some(e.clone());
                    }
                }
            });
        }
        if let Some(e) = chosen {
            self.sim.course = e.course;
            self.sim.ground = i32::from(e.ground);
            self.sim.weather = i32::from(e.weather);
            self.sim.season = i32::from(e.season);
            self.sim.time = i32::from(e.time);
            self.fill_aptitudes(0);
            self.fill_aptitudes(1);
            let dropped = self.drop_disabled_skills(&e.disabled_skills);
            if dropped > 0 {
                self.toast(format!("Took out {dropped} skills that won't activate in the {}.", e.name), crate::ui_util::ToastKind::Info);
            }
        }
        ui.add_space(4.0);

        // The course, and a strip of it.
        let items = || {
            let mut items: Vec<(Option<u32>, String, String)> = Vec::new();
            let mut by_track: Vec<(&u32, &Course)> = data.courses.iter().collect();
            by_track.sort_by_key(|(id, c)| (c.race_track_id, c.surface, c.distance as u32, **id));
            let mut last_track = None;
            for (id, c) in by_track {
                if last_track != Some(c.race_track_id) {
                    items.push((None, data.tracks.get(&c.race_track_id).cloned().unwrap_or_default(), String::new()));
                    last_track = Some(c.race_track_id);
                }
                let turn = match c.turn {
                    1 => "right",
                    2 => "left",
                    _ => "straight",
                };
                items.push((Some(*id), format!("{} · {turn}", data.course_name(*id)), String::new()));
            }
            items
        };
        let current = data.course_name(self.sim.course);
        if let Some(id) = search_combo(ui, "sim_course", current, ui.available_width() - 8.0, items, Some(&self.sim.course)) {
            self.sim.course = id;
            self.fill_aptitudes(0);
            self.fill_aptitudes(1);
        }
        if let Some(c) = data.courses.get(&self.sim.course) {
            crate::charts::course_strip(ui, c, 50.0, &[]);
        }
        ui.add_space(4.0);

        // Conditions: ground and weather, season and time of day.
        egui::Grid::new("sim_conditions").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
            let pick = |ui: &mut egui::Ui, id: &str, value: &mut i32, names: &[(i32, &str)]| {
                let current = names.iter().find(|(c, _)| c == value).map_or("?", |(_, n)| n);
                egui::ComboBox::from_id_salt(id).width(74.0).selected_text(RichText::new(current).size(size::SM)).show_ui(ui, |ui| {
                    for (c, n) in names {
                        ui.selectable_value(value, *c, *n);
                    }
                });
            };
            let list = |names: &[&'static str; 4]| -> Vec<(i32, &'static str)> { names.iter().enumerate().map(|(i, n)| (i as i32 + 1, *n)).collect() };
            ui.label(RichText::new("Ground").size(size::SM).color(p.weak));
            pick(ui, "sim_ground", &mut self.sim.ground, &list(&GROUNDS));
            ui.label(RichText::new("Weather").size(size::SM).color(p.weak));
            pick(ui, "sim_weather", &mut self.sim.weather, &list(&WEATHERS));
            ui.end_row();
            ui.label(RichText::new("Season").size(size::SM).color(p.weak));
            pick(ui, "sim_season", &mut self.sim.season, &list(&SEASONS));
            ui.label(RichText::new("Time").size(size::SM).color(p.weak));
            pick(ui, "sim_time", &mut self.sim.time, &TIMES);
            ui.end_row();
        });
        ui.add_space(6.0);

        // What to test.
        ui.label(RichText::new("WHAT TO TEST").size(size::XS).color(p.weak));
        let modes = [
            (Mode::Compare, "⚖ Compare".to_string(), "Race Uma 1 against Uma 2 many times: who wins, by how much, and where."),
            (Mode::Chart, "📊 Skills".to_string(), "Add each skill to Uma 1 in turn and measure how many lengths it gains on this course."),
            (Mode::Stamina, "♥ Stamina".to_string(), "Run Uma 1 alone and see the HP she has left at the finish."),
            (Mode::HpCalc, "⚡ Quick".to_string(), "A formula instead of races: an instant, rougher stamina estimate."),
        ];
        ui.scope(|ui| {
            ui.spacing_mut().button_padding.x = 5.0;
            crate::ui_util::segmented(ui, &mut self.sim.mode, &modes);
        });
        if self.sim.mode == Mode::Stamina {
            ui.tick(&mut self.sim.force_full, "Force a full spurt").on_hover_text("Shows how far short of a full spurt the build is");
        }
        if self.sim.mode == Mode::HpCalc {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Recovery skills (% of max HP)").size(size::SM));
                ui.add(egui::DragValue::new(&mut self.sim.heal_pct).range(0.0..=50.0).speed(0.5));
            });
        }
        ui.add_space(4.0);

        egui::CollapsingHeader::new(RichText::new("Advanced options").size(size::SM)).id_salt("sim_advanced").show(ui, |ui| {
            ui.label(RichText::new("The defaults match umalator.app. Leave them unless you know you want something else.").size(size::XS).color(p.weak));
            let o = &mut self.sim.opts;
            ui.horizontal(|ui| {
                ui.label("Runs").on_hover_text("How many races to run. More is steadier but slower; 1000 is plenty.");
                ui.add(egui::DragValue::new(&mut o.samples).range(20..=10_000).speed(10));
                ui.label("Seed").on_hover_text("The starting point for the random numbers. The same seed gives the same results.");
                ui.add_enabled(!self.sim.auto_seed, egui::DragValue::new(&mut o.seed).range(0.0..=4_294_967_295.0).speed(1.0).fixed_decimals(0));
            });
            ui.tick(&mut self.sim.auto_seed, "New seed each run").on_hover_text("Off: rerun with the same seed for repeatable results");
            ui.tick(&mut o.sync_rng, "Sync RNG").on_hover_text("Both umas see the same random events, so the gap shows the build difference");
            ui.tick(&mut o.wit_check, "Wit check").on_hover_text("Skills can fail to activate on a Wit roll");
            ui.tick(&mut o.rushed, "Rushed (kakari)").on_hover_text("Umas can get over-eager mid-race and burn HP, as in the game");
            ui.tick(&mut o.lane_movement, "Lane movement").on_hover_text("Umas move between lanes on corners and straights");
            ui.tick(&mut o.lead_competition, "Spot struggle").on_hover_text("Front runners fighting for the lead early on");
            let mut dueling = o.compete_fight;
            if ui.tick(&mut dueling, "Dueling").on_hover_text("Late-race duels on the final straight").changed() {
                o.compete_fight = dueling;
                o.dueling = dueling.then_some([30.0, 30.0, 25.0, 20.0, 15.0]);
            }
            if let (true, Some(r)) = (o.compete_fight, o.dueling.as_mut()) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Duel %").size(size::SM));
                    for (i, n) in ["Run", "Front", "Pace", "Late", "End"].iter().enumerate() {
                        ui.label(RichText::new(*n).size(size::XS).color(p.weak));
                        ui.add(egui::DragValue::new(&mut r[i]).range(0.0..=100.0).speed(1.0));
                    }
                });
            }
        });
    }

    /// Keep a finished Compare in the history (the last 10).
    fn remember_run(&mut self, r: &CompareResult) {
        let Some(ran) = &self.sim.ran else { return };
        let course = self.sim_data.as_ref().map_or(String::new(), |d| d.course_name(ran.course));
        let name = |u: &UmaBuild| u.outfit.and_then(|o| self.kb.umas.get(&o)).map_or("no outfit".to_string(), |i| i.name.clone());
        let label = format!("{} vs {} · {course} · {} · {} races", name(&ran.umas[0]), name(&ran.umas[1]), ran.conditions, r.gaps.len());
        let wins = r.gaps.iter().filter(|g| **g > 0.0).count() as f64 / r.gaps.len().max(1) as f64 * 100.0;
        let verdict = if r.mean >= 0.0 { format!("Uma 2 by {:.2} L ({wins:.0}%)", r.mean) } else { format!("Uma 1 by {:.2} L ({:.0}%)", -r.mean, 100.0 - wins) };
        let when = crate::live::fmt_local(crate::live::now());
        self.sim.history.push(HistoryEntry { label, verdict, when, gaps: r.gaps.clone() });
        if self.sim.history.len() > 10 {
            self.sim.history.remove(0);
            self.sim.pinned = self.sim.pinned.and_then(|i| i.checked_sub(1));
        }
    }

    /// Start the selected mode in a background thread.
    pub(crate) fn sim_start(&mut self, ctx: &egui::Context) {
        let Some(data) = self.sim_data.clone() else { return };
        let Some(course) = data.courses.get(&self.sim.course).cloned() else {
            self.toast("Pick a course first.", crate::ui_util::ToastKind::Warn);
            return;
        };
        if self.sim.auto_seed {
            self.sim.opts.seed = new_seed();
        }
        for u in &mut self.sim.umas {
            u.normalise(&data);
        }
        let course = Arc::new(course);
        let s = SimSnapshot { mode: self.sim.mode, ground: self.sim.ground, weather: self.sim.weather, season: self.sim.season, time: self.sim.time, force_full: self.sim.force_full };
        let progress = Arc::new(Progress::default());
        let (tx, rx) = mpsc::channel();
        let (opts, u1, u2) = (self.sim.opts.clone(), self.sim.umas[0].clone(), self.sim.umas[1].clone());
        self.sim.ran = Some(RunInfo {
            course: self.sim.course,
            conditions: conditions_text(s.ground, s.weather, s.season, s.time),
            umas: [u1.clone(), u2.clone()],
            opts: opts.clone(),
            race: (s.ground, s.weather, s.season, s.time),
        });
        let repaint = ctx.clone();
        let p2 = progress.clone();
        let label;
        match s.mode {
            Mode::Compare => {
                label = format!("{} runs", opts.samples);
                progress.total.store(opts.samples, Ordering::Relaxed);
                let race = race_params(s.ground, s.weather, s.season, s.time, None);
                std::thread::spawn(move || {
                    let r = run_comparison(&data, &course, &race, &u1, &u2, &opts, true, true, Some(&p2));
                    let _ = tx.send(JobResult::Compare(r));
                    repaint.request_repaint();
                });
                self.sim.compare = None;
            }
            Mode::Stamina => {
                label = format!("{} runs", opts.samples);
                progress.total.store(opts.samples, Ordering::Relaxed);
                let race = race_params(s.ground, s.weather, s.season, s.time, None);
                let force = s.force_full;
                std::thread::spawn(move || {
                    let r = run_hpcalc(&data, &course, &race, &u1, &opts, force, Some(&p2));
                    let _ = tx.send(JobResult::Stamina(r));
                    repaint.request_repaint();
                });
                self.sim.stamina = None;
            }
            Mode::Chart => {
                let race = race_params(s.ground, s.weather, s.season, s.time, Some(u1.desc.strategy));
                let skills = self.chart_candidates(&data, &course, &u1, &race);
                if skills.is_empty() {
                    self.toast("No skills to test for this uma on this course.", crate::ui_util::ToastKind::Warn);
                    return;
                }
                label = format!("{} skills", skills.len());
                let seed = opts.seed;
                std::thread::spawn(move || {
                    let rows = run_chart(&data, &course, &race, &u1, &skills, seed, false, &p2);
                    let _ = tx.send(JobResult::Chart(rows));
                    repaint.request_repaint();
                });
                self.sim.chart = None;
            }
            Mode::HpCalc => return,
        }
        self.sim.job = Some(Job { progress, rx, label });
    }

    /// Skills worth testing on Uma 1: buyable (white or gold, not unique or evolved), not already
    /// held, and able to activate on this course.
    fn chart_candidates(&self, data: &SimData, course: &Course, uma: &UmaBuild, race: &crate::sim::params::RaceParams) -> Vec<u32> {
        let horse = base_stats(&uma.desc);
        let own_outfit: Vec<u32> = uma.outfit.and_then(|o| self.kb.umas.get(&o)).map(|i| i.skills.iter().map(|s| s.id).chain(i.event_skills.iter().copied()).collect()).unwrap_or_default();
        let pool: Vec<u32> = match self.sim.chart_pool {
            ChartPool::Account => {
                let mut v: Vec<u32> = self.kb.sources.keys().copied().filter(|id| self.skill_available(*id)).chain(own_outfit).collect();
                v.sort_unstable();
                v.dedup();
                v
            }
            ChartPool::AllGlobal => data.skills.keys().copied().filter(|id| !data.not_in_game.contains(id)).collect(),
        };
        pool.into_iter()
            .filter(|id| {
                let Some(def) = data.skills.get(id) else { return false };
                (def.rarity == 1 || def.rarity == 2) && !uma.skills.contains(id) && self.kb.skills.get(id).is_none_or(|s| s.sp > 0)
            })
            .filter(|id| {
                build_skill_data(data, &horse, race, course, *id, Perspective::Mine, 1)
                    .is_ok_and(|v| v.iter().any(|s| s.regions.first().is_some_and(|r| r.start < 9999.0)))
            })
            .collect()
    }

    fn sim_main(&mut self, ui: &mut egui::Ui) {
        let course = self.sim_data.as_ref().map(|d| format!("{} · {}", d.course_name(self.sim.course), conditions_text(self.sim.ground, self.sim.weather, self.sim.season, self.sim.time)));
        let what = match self.sim.mode {
            Mode::Compare => "Two umas race each other many times on",
            Mode::Chart => "Each skill is added to Uma 1 in turn to see how much it helps on",
            Mode::Stamina => "Uma 1 races alone to check her HP lasts on",
            Mode::HpCalc => "An instant estimate of Uma 1's HP needs on",
        };
        let subtitle = format!("{what} {}.", course.unwrap_or_default());
        let mut run = false;
        page_header(ui, "🏁 Race sim", &subtitle, |ui| {
            if self.sim.mode != Mode::HpCalc {
                match &self.sim.job {
                    Some(job) => {
                        if ui.small_btn("✖").on_hover_text("Stop the run").clicked() {
                            job.progress.cancel.store(true, Ordering::Relaxed);
                        }
                        let done = job.progress.done.load(Ordering::Relaxed) as f32;
                        let total = job.progress.total.load(Ordering::Relaxed).max(1) as f32;
                        ui.add(egui::ProgressBar::new((done / total).min(1.0)).desired_width(160.0).text(job.label.clone()));
                    }
                    None => {
                        let b = crate::ui_util::primary_button(ui, "▶ Run", 14.5).min_size(vec2(84.0, 30.0));
                        run = ui.add(b).on_hover_text("Start the simulation. It runs in the background; you can keep using the app.").clicked();
                    }
                }
            }
            if ui.add(egui::Button::new("📋 Load from Planner").min_size(vec2(0.0, 30.0))).on_hover_text("Fill in the Planner's trainee, target stats, skills, course and conditions").clicked() {
                match self.load_plan_into_sim() {
                    Ok(note) => self.toast(note, crate::ui_util::ToastKind::Success),
                    Err(e) => self.toast(e, crate::ui_util::ToastKind::Warn),
                }
            }
        });
        if run {
            let ctx = ui.ctx().clone();
            self.sim_start(&ctx);
        }
        let two = self.sim.mode == Mode::Compare;
        if two && ui.available_width() >= 900.0 {
            ui.columns(2, |cols| {
                self.uma_panel(&mut cols[0], 0);
                self.uma_panel(&mut cols[1], 1);
            });
        } else {
            self.uma_panel(ui, 0);
            if two {
                self.uma_panel(ui, 1);
            }
        }
        ui.add_space(6.0);
        let top = ui.cursor().top();
        match self.sim.mode {
            Mode::Compare => self.compare_dashboard(ui),
            Mode::Chart => self.chart_results(ui),
            Mode::Stamina => self.stamina_results(ui),
            Mode::HpCalc => self.hp_calc(ui),
        }
        // Where the results were drawn, for "Save chart as PNG".
        self.sim.results_rect = Some(Rect::from_min_max(pos2(ui.min_rect().left(), top), pos2(ui.min_rect().right(), ui.cursor().top())));
        if self.sim.save_png {
            self.sim.save_png = false;
            self.save_results_png(ui.ctx());
        }
    }

    /// Stats from the community meta sheet for the Champions Meeting the sim is set to, if any.
    fn sim_meta_stats(&self) -> Option<(String, [f64; 5])> {
        let e = self.kb.umalator_presets.iter().find(|e| e.is_cm() && e.course == self.sim.course && i32::from(e.ground) == self.sim.ground && i32::from(e.season) == self.sim.season)?;
        let ev = self.kb.pvp_meta.as_ref()?.events.iter().find(|m| m.series == "CM" && m.number == e.id)?;
        let mut out = [0.0; 5];
        for (i, s) in ev.stats.iter().take(5).enumerate() {
            let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
            out[i] = digits.parse().ok()?;
        }
        Some((format!("CM {} {}", e.id, e.name), out))
    }

    fn uma_panel(&mut self, ui: &mut egui::Ui, i: usize) {
        let Some(data) = self.sim_data.clone() else { return };
        let p = pal(ui);
        let dark = ui.visuals().dark_mode;
        let title = if self.sim.mode == Mode::Compare { format!("Uma {}", i + 1) } else { "Uma".to_string() };
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 5.0, crate::charts::uma_color(i, dark));
                ui.label(semibold(&title, 15.5));
                if let Some(uri) = self.sim.umas[i].outfit.and_then(|o| self.art_uri("umas", o)) {
                    ui.add(egui::Image::new(uri).fit_to_exact_size(vec2(26.0, 26.0)).corner_radius(CornerRadius::same(13)));
                }
                // Outfit: yours first, then every Global trainee.
                let items = || {
                    let mut items: Vec<(Option<u32>, String, String)> = vec![(None, "YOUR UMAS".into(), String::new())];
                    let mut owned: Vec<_> = self.roster.umas.iter().collect();
                    owned.sort_by(|a, b| b.stars.cmp(&a.stars).then(a.name.cmp(&b.name)));
                    for u in &owned {
                        items.push((Some(u.id), format!("{} [{}] {}★", u.name, u.outfit, u.stars), String::new()));
                    }
                    items.push((None, "ALL GLOBAL TRAINEES".into(), String::new()));
                    let mut all: Vec<_> = self.kb.umas.values().filter(|u| self.owned_uma(u.id).is_none()).collect();
                    all.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
                    for u in all {
                        items.push((Some(u.id), format!("{} [{}]", u.name, u.outfit), String::new()));
                    }
                    items
                };
                let current = self.sim.umas[i].outfit;
                let text = current.and_then(|o| self.kb.umas.get(&o)).map_or("No outfit (no unique)".to_string(), |u| format!("{} [{}]", u.name, u.outfit));
                if let Some(o) = search_combo(ui, ("sim_outfit", i), text, (ui.available_width() - 130.0).clamp(140.0, 250.0), items, current.as_ref()) {
                    self.set_outfit(i, o);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.sim.mode == Mode::Compare {
                        let other = 1 - i;
                        if ui.small_btn(if i == 0 { "Copy →" } else { "← Copy" }).on_hover_text(format!("Copy this build to Uma {}", other + 1)).clicked() {
                            self.sim.umas[other] = self.sim.umas[i].clone();
                        }
                        if i == 0 && ui.small_btn("⇄ Swap").on_hover_text("Swap Uma 1 and Uma 2").clicked() {
                            self.sim.umas.swap(0, 1);
                        }
                    }
                });
            });
            let meta = self.sim_meta_stats();
            let plan_target = self.planner.plan.target;
            let other_desc = self.sim.umas[1 - i].desc.clone();
            let u = &mut self.sim.umas[i];
            ui.horizontal(|ui| {
                ui.label(RichText::new("Stars").color(p.weak));
                ui.add(egui::DragValue::new(&mut u.stars).range(1..=5));
                ui.label(RichText::new("Unique Lv").color(p.weak)).on_hover_text("Level of her unique skill, 1–6");
                ui.add(egui::DragValue::new(&mut u.unique_level).range(1..=6));
                ui.label(RichText::new("Mood").color(p.weak));
                egui::ComboBox::from_id_salt(("sim_mood", i)).width(76.0).selected_text(MOODS[(u.desc.mood + 2).clamp(0, 4) as usize]).show_ui(ui, |ui| {
                    for (k, m) in MOODS.iter().enumerate() {
                        ui.selectable_value(&mut u.desc.mood, k as i32 - 2, *m);
                    }
                });
            });
            // Five equal stat boxes, each label centred over its box.
            ui.horizontal(|ui| {
                let w = ((ui.available_width() - 4.0 * 8.0) / 5.0).clamp(56.0, 84.0);
                for (name, v) in [
                    ("Speed", &mut u.desc.speed),
                    ("Stamina", &mut u.desc.stamina),
                    ("Power", &mut u.desc.power),
                    ("Guts", &mut u.desc.guts),
                    ("Wit", &mut u.desc.wisdom),
                ] {
                    ui.allocate_ui_with_layout(vec2(w, 46.0), egui::Layout::top_down(egui::Align::Center), |ui| {
                        ui.label(RichText::new(name).size(11.5).color(p.weak));
                        ui.add_sized([w, 24.0], egui::DragValue::new(v).range(1.0..=2000.0).speed(5.0).fixed_decimals(0));
                    });
                }
            });
            // Stat templates, and the meta sheet's stats for this CM.
            let mut set_stats: Option<[f64; 5]> = None;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                match &meta {
                    Some((label, stats)) => {
                        if ui.small_btn("📈 Use meta stats").on_hover_text(format!("The community meta sheet's recommended stats for {label}: {}. Community opinion, not game data.", stats.map(|v| format!("{v:.0}")).join(" / "))).clicked() {
                            set_stats = Some(*stats);
                        }
                    }
                    None => {
                        ui.add_enabled(false, egui::Button::new("📈 Use meta stats").small()).on_disabled_hover_text("Pick a Champions Meeting on the left: the meta sheet recommends stats per CM.");
                    }
                }
                egui::ComboBox::from_id_salt(("stat_template", i)).selected_text(RichText::new("Stat templates").size(size::SM)).show_ui(ui, |ui| {
                    if ui.selectable_label(false, "Umalator's default (1200 / 1200 / 800 / 400 / 400)").clicked() {
                        set_stats = Some([1200.0, 1200.0, 800.0, 400.0, 400.0]);
                    }
                    if plan_target.iter().any(|v| *v > 0) && ui.selectable_label(false, format!("Your plan's targets ({})", plan_target.map(|v| v.to_string()).join(" / "))).clicked() {
                        set_stats = Some(plan_target.map(f64::from));
                    }
                    if ui.selectable_label(false, format!("Uma {}'s stats", 2 - i)).clicked() {
                        set_stats = Some([other_desc.speed, other_desc.stamina, other_desc.power, other_desc.guts, other_desc.wisdom]);
                    }
                });
            });
            if let Some(s) = set_stats {
                [u.desc.speed, u.desc.stamina, u.desc.power, u.desc.guts, u.desc.wisdom] = s;
            }
            let mut refill = false;
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Style").color(p.weak));
                egui::ComboBox::from_id_salt(("sim_style", i)).width(110.0).selected_text(strategy_name(u.desc.strategy)).show_ui(ui, |ui| {
                    for s in STRATEGIES {
                        if ui.selectable_value(&mut u.desc.strategy, s, strategy_name(s)).clicked() {
                            refill = true;
                        }
                    }
                });
                for (label, v) in [("Distance", &mut u.desc.distance_apt), ("Surface", &mut u.desc.surface_apt), ("Style apt.", &mut u.desc.strategy_apt)] {
                    ui.label(RichText::new(label).color(p.weak)).on_hover_text(format!("Her {} aptitude for this race, S (best) to G", label.trim_end_matches(" apt.").to_lowercase()));
                    egui::ComboBox::from_id_salt((label, i)).width(34.0).selected_text(APTITUDES[(*v).min(7)]).show_ui(ui, |ui| {
                        for (k, g) in APTITUDES.iter().enumerate() {
                            ui.selectable_value(v, k, *g);
                        }
                    });
                }
            });
            if refill {
                self.fill_aptitudes(i);
            }
            ui.add_space(4.0);
            // Skills
            let unique = self.sim.umas[i].outfit.and_then(|o| data.unique_skill(o, self.sim.umas[i].stars));
            let mut remove = None;
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("Skills ({})", self.sim.umas[i].skills.len())).color(p.weak));
                for (k, id) in self.sim.umas[i].skills.iter().enumerate() {
                    let tier = self.kb.skills.get(id).map_or("White", |s| s.tier());
                    let color = tier_color(ui, tier);
                    let name = self.skill_label(*id);
                    let jp = data.not_in_game.contains(id);
                    let text = if Some(*id) == unique { format!("★ {name}") } else { name };
                    let b = egui::Button::new(RichText::new(format!("{text}  ✖")).size(12.5).color(color))
                        .fill(color.gamma_multiply(0.10))
                        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
                        .corner_radius(CornerRadius::same(12))
                        .small();
                    let hover = self.kb.skills.get(id).map_or(String::new(), |s| format!("{} · {} SP\n{}", s.tier(), s.sp, if s.summary.is_empty() { &s.desc } else { &s.summary }));
                    let hover = if jp { format!("{hover}\nNot on Global yet") } else { hover };
                    if ui.add(b).on_hover_text(format!("{hover}\nClick to remove")).clicked() {
                        remove = Some(k);
                    }
                }
            });
            if let Some(k) = remove {
                self.sim.umas[i].skills.remove(k);
            }
            let held: Vec<u32> = self.sim.umas[i].skills.clone();
            let own: Vec<u32> = self.sim.umas[i].outfit.and_then(|o| self.kb.umas.get(&o)).map(|u| u.skills.iter().map(|s| s.id).chain(u.event_skills.iter().copied()).collect()).unwrap_or_default();
            // Built only while the dropdown is open: about 900 skills.
            let items = || {
                let mut items: Vec<(Option<u32>, String, String)> = Vec::new();
                let label = |id: u32| {
                    let s = self.kb.skills.get(&id);
                    let tier = s.map_or("", |s| s.tier());
                    let sp = s.map_or(String::new(), |s| format!(" · {} SP", s.sp));
                    (format!("{} · {tier}{sp}", self.skill_label(id)), s.map(|s| if s.summary.is_empty() { s.desc.clone() } else { s.summary.clone() }).unwrap_or_default())
                };
                let push_group = |items: &mut Vec<(Option<u32>, String, String)>, title: &str, ids: Vec<u32>| {
                    let mut ids: Vec<(String, u32)> = ids.into_iter().filter(|id| data.skills.contains_key(id) && !held.contains(id)).map(|id| (self.skill_label(id), id)).collect();
                    ids.sort();
                    ids.dedup();
                    if ids.is_empty() {
                        return;
                    }
                    items.push((None, title.to_string(), String::new()));
                    for (_, id) in ids {
                        let (l, h) = label(id);
                        items.push((Some(id), l, h));
                    }
                };
                push_group(&mut items, "THIS UMA'S SKILLS", own.clone());
                let account: HashSet<u32> = self.kb.sources.keys().copied().filter(|id| self.skill_available(*id) && !own.contains(id)).collect();
                push_group(&mut items, "ON YOUR ACCOUNT (cards and umas you own)", account.iter().copied().collect());
                let rest: Vec<u32> = data.skills.keys().copied().filter(|id| !data.not_in_game.contains(id) && !own.contains(id) && !account.contains(id)).collect();
                push_group(&mut items, "ALL GLOBAL SKILLS", rest);
                items
            };
            let (add, clear) = ui
                .horizontal(|ui| {
                    let add = search_combo(ui, ("sim_add_skill", i), "＋ Add skill…", 260.0, items, None);
                    (add, ui.small_btn("Clear").on_hover_text("Remove every skill except the unique").clicked())
                })
                .inner;
            if let Some(id) = add {
                self.sim.umas[i].skills.push(id);
                self.sim.umas[i].normalise(&data);
            }
            if clear {
                self.sim.umas[i].skills.retain(|s| Some(*s) == unique);
            }
        });
        ui.add_space(6.0);
    }

    /// Compare's results: the verdict, then tabs.
    fn compare_dashboard(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let Some(r) = self.sim.compare.clone() else {
            if self.sim.job.is_none() {
                ui.label(RichText::new("Set up Uma 1 and Uma 2 and press ▶ Run. The gap is Uma 2's lead over Uma 1 in lengths (1 L = 2.5 m).").color(p.weak));
            }
            return;
        };
        self.verdict_card(ui, &r);
        ui.add_space(6.0);
        let tabs = [
            (ResultTab::Overview, "Overview".to_string(), "Who wins, by how much, and how the gaps spread"),
            (ResultTab::Race, "Race".to_string(), "The course, where the lengths are won, and speed and HP along the race"),
            (ResultTab::Skills, "Skills".to_string(), "Where each skill fired, what each is worth, and where training points matter"),
            (ResultTab::Stamina, "Stamina".to_string(), "HP at the finish and what a full spurt needs"),
            (ResultTab::Runs, "Runs".to_string(), "Earlier runs to compare against, each uma's numbers, and export"),
        ];
        crate::ui_util::segmented(ui, &mut self.sim.result_tab, &tabs);
        ui.add_space(6.0);
        match self.sim.result_tab {
            ResultTab::Overview => self.results_overview(ui, &r),
            ResultTab::Race => self.results_race(ui, &r),
            ResultTab::Skills => self.results_skills(ui, &r),
            ResultTab::Stamina => self.results_stamina(ui, &r),
            ResultTab::Runs => self.results_runs(ui, &r),
        }
    }

    /// "Uma 2 wins 83% of 1000 races, by 3.52 L (≈ 0.40 s) on average", with the 95% interval and a
    /// plain note when the gap is within noise.
    fn verdict_card(&self, ui: &mut egui::Ui, r: &CompareResult) {
        let p = pal(ui);
        let dark = ui.visuals().dark_mode;
        let runs = r.gaps.len().max(1);
        let ahead = r.gaps.iter().filter(|g| **g > 0.0).count() as f64 / runs as f64 * 100.0;
        let (winner, k, lead, share) = if r.mean >= 0.0 { ("Uma 2", 1, r.mean, ahead) } else { ("Uma 1", 0, -r.mean, 100.0 - ahead) };
        let n = runs as f64;
        let sd = (r.gaps.iter().map(|g| (g - r.mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
        let half = 1.96 * sd / n.sqrt();
        let d = self.sim.ran.as_ref().and_then(|x| self.sim_data.as_ref()?.courses.get(&x.course)).map_or(2000.0, |c| c.distance);
        let speed = d / r.umas[0].mean_time.max(1.0);
        let secs = lead * 2.5 / speed.max(1.0);
        card(ui, |ui| {
            let mut job = egui::text::LayoutJob::default();
            let font = egui::FontId::new(size::XL, egui::FontFamily::Name(crate::theme::SEMIBOLD.into()));
            job.append(&format!("{winner} wins {share:.0}% of {runs} races"), 0.0, egui::TextFormat::simple(font.clone(), crate::charts::uma_color(k, dark)));
            job.append(&format!(", by {lead:.2} L (≈ {secs:.2} s) on average."), 0.0, egui::TextFormat::simple(font, p.text));
            ui.add(egui::Label::new(job).wrap());
            ui.label(RichText::new(format!("95% interval for the average gap: {:+.2} to {:+.2} L (1 L = 2.5 m). Simulator output.", r.mean - half, r.mean + half)).size(size::SM).color(p.weak));
            if lead < half || lead < 0.1 {
                ui.label(RichText::new("The gap is within noise: treat the two builds as even, or run more races.").size(size::SM).color(p.warn));
            }
        });
    }

    fn results_overview(&mut self, ui: &mut egui::Ui, r: &CompareResult) {
        let p = pal(ui);
        let dark = ui.visuals().dark_mode;
        let colors = [crate::charts::uma_color(0, dark), crate::charts::uma_color(1, dark)];
        let runs = r.gaps.len();
        let sign = |v: f64| format!("{v:+.2} L");
        stat_row(
            ui,
            &[
                ("⚖", "Average gap", sign(r.mean), "Uma 2's lead over Uma 1".into(), if r.mean >= 0.0 { colors[1] } else { colors[0] }),
                ("◎", "Typical gap", sign(r.median), format!("median of {runs} runs"), p.accent),
                ("↧", "Worst for Uma 2", sign(r.min), "smallest gap".into(), p.weak),
                ("↥", "Best for Uma 2", sign(r.max), "largest gap".into(), p.weak),
            ],
        );
        ui.add_space(8.0);
        let ghost = self.sim.pinned.and_then(|i| self.sim.history.get(i)).map(|h| (h.gaps.clone(), h.label.clone()));
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(semibold("Gap per race", size::MD));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let options = [(false, "Histogram".to_string(), "How many races ended with each gap"), (true, "Cumulative".to_string(), "The share of races that ended below each gap")];
                    crate::ui_util::segmented(ui, &mut self.sim.cdf, &options);
                });
            });
            ui.label(RichText::new("Every race's gap at the finish. Right of zero, Uma 2 won.").size(size::SM).color(p.weak));
            let mut legend = vec![(colors[0], "Uma 1 ahead"), (colors[1], "Uma 2 ahead")];
            if ghost.is_some() {
                legend.push((p.text.gamma_multiply(0.55), "pinned run (outline)"));
            }
            crate::charts::legend(ui, &legend);
            crate::charts::distribution(ui, &r.gaps, ghost.as_ref().map(|g| g.0.as_slice()), self.sim.cdf, colors, 210.0);
            if let Some((_, label)) = &ghost {
                ui.label(RichText::new(format!("Pinned: {label}")).size(size::XS).color(p.weak));
            }
        });
        if let Some(course) = self.ran_course() {
            card(ui, |ui| {
                ui.label(semibold("Where the lengths are won", size::MD));
                ui.label(RichText::new("Uma 2's lead along the race: the line is the median race, the band holds 8 races in 10.").size(size::SM).color(p.weak));
                self.strip_with_spurts(ui, &course, r);
                crate::charts::gap_chart(ui, &r.gap_curve, course.distance as f32, 170.0, colors[1]);
            });
        }
        if self.sim.from_plan {
            ui.horizontal(|ui| {
                if ui.button("💾 Save to plan").on_hover_text("Writes these results into the plan's ## Simulation section").clicked() {
                    match self.save_sim_to_plan() {
                        Ok(msg) => self.toast(msg, crate::ui_util::ToastKind::Success),
                        Err(e) => self.toast(e, crate::ui_util::ToastKind::Warn),
                    }
                }
                ui.label(RichText::new("Uma 1 is the plan's trainee without her bought skills; Uma 2 has them.").size(size::SM).color(p.weak));
            });
        }
    }

    /// The course the last run used.
    fn ran_course(&self) -> Option<Course> {
        let id = self.sim.ran.as_ref().map_or(self.sim.course, |x| x.course);
        self.sim_data.as_ref()?.courses.get(&id).cloned()
    }

    /// The course strip, lined up with the charts, with each uma's mean spurt start.
    fn strip_with_spurts(&mut self, ui: &mut egui::Ui, course: &Course, r: &CompareResult) -> crate::charts::StripGeom {
        let dark = ui.visuals().dark_mode;
        let labels = ["Uma 1 spurts", "Uma 2 spurts"];
        let mut markers: Vec<(f64, &str, Color32)> = (0..2).filter_map(|k| r.umas[k].mean_spurt_start.map(|m| (m, labels[k], crate::charts::uma_color(k, dark)))).collect();
        // Both at about the same point: one marker.
        if markers.len() == 2 && (markers[0].0 - markers[1].0).abs() < course.distance * 0.01 {
            markers = vec![(markers[0].0, "Both spurt", pal(ui).text)];
        }
        crate::charts::course_strip_in(ui, course, 74.0, &markers, (crate::charts::AXIS_L, crate::charts::AXIS_R)).1
    }

    fn results_race(&mut self, ui: &mut egui::Ui, r: &CompareResult) {
        let p = pal(ui);
        let dark = ui.visuals().dark_mode;
        let Some(course) = self.ran_course() else { return };
        let d = course.distance as f32;
        let colors = [crate::charts::uma_color(0, dark), crate::charts::uma_color(1, dark)];
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(semibold("The race", size::MD));
                ui.add_space(8.0);
                crate::charts::legend(ui, &[(colors[0], "Uma 1"), (colors[1], "Uma 2")]);
            });
            ui.label(RichText::new("All charts share the course's distance axis. Speed and HP are the median race (the one whose gap is closest to the median).").size(size::SM).color(p.weak));
            self.replay_controls(ui, &r.median_run);
            let geom = self.strip_with_spurts(ui, &course, r);
            self.replay_dots(ui, geom, &r.median_run);
            ui.add_space(4.0);
            ui.label(RichText::new("Uma 2's lead (L)").size(size::SM).color(p.weak));
            crate::charts::gap_chart(ui, &r.gap_curve, d, 150.0, colors[1]);
            ui.add_space(4.0);
            ui.label(RichText::new("Speed (m/s): bars above are skills firing; dashes, the last spurt; ✕, HP ran out").size(size::SM).color(p.weak));
            let lines = [
                crate::charts::TraceLine { trace: &r.median_run[0], color: colors[0], label: "Uma 1" },
                crate::charts::TraceLine { trace: &r.median_run[1], color: colors[1], label: "Uma 2" },
            ];
            crate::charts::speed_chart(ui, d, &lines, 230.0, &|id| self.skill_label(id));
            ui.add_space(4.0);
            ui.label(RichText::new("HP: shaded, rushed; +, a recovery skill").size(size::SM).color(p.weak));
            let recovery = |id: u32| self.kb.skills.get(&id).is_some_and(|s| s.category == "Recovery");
            crate::charts::hp_chart(ui, d, &lines, 150.0, &recovery, &|id| self.skill_label(id));
        });
    }

    /// ▶ Replay: play, pause and scrub the median race along the course strip.
    fn replay_controls(&mut self, ui: &mut egui::Ui, traces: &[Trace; 2]) {
        let len = traces.iter().map(|t| t.pos.len()).max().unwrap_or(0);
        if len == 0 {
            return;
        }
        let total = len as f32 * crate::sim::run::DT as f32;
        let rp = &mut self.sim.replay;
        if rp.playing {
            rp.t += ui.input(|i| i.stable_dt).min(0.1) * rp.speed;
            if rp.t >= total {
                rp.t = total;
                rp.playing = false;
            }
            ui.ctx().request_repaint();
        }
        ui.horizontal(|ui| {
            let label = if rp.playing { "⏸ Pause" } else if rp.t > 0.0 && rp.t < total { "▶ Resume" } else { "▶ Replay" };
            if ui.button(label).on_hover_text("Watch the median race: two dots run the course, and skills pop up as they fire").clicked() {
                if !rp.playing && rp.t >= total {
                    rp.t = 0.0;
                }
                rp.playing = !rp.playing;
            }
            ui.add(egui::Slider::new(&mut rp.t, 0.0..=total).show_value(false)).on_hover_text("Scrub through the race");
            ui.label(RichText::new(format!("{:.1} s", rp.t * GAME_CLOCK as f32)).size(size::SM));
            for s in [1.0, 2.0, 4.0] {
                ui.selectable_value(&mut rp.speed, s, format!("{s:.0}×"));
            }
        });
    }

    /// The two dots of the replay on the strip, with skills popping up as they fire.
    fn replay_dots(&self, ui: &egui::Ui, geom: crate::charts::StripGeom, traces: &[Trace; 2]) {
        let rp = &self.sim.replay;
        if rp.t <= 0.0 && !rp.playing {
            return;
        }
        let dark = ui.visuals().dark_mode;
        let p = pal(ui);
        let i = (rp.t / crate::sim::run::DT as f32) as usize;
        let painter = ui.painter();
        for (k, t) in traces.iter().enumerate() {
            let Some(&m) = t.pos.get(i.min(t.pos.len().saturating_sub(1))) else { continue };
            let c = pos2(geom.x(m), geom.track.center().y + if k == 0 { -4.0 } else { 4.0 });
            let color = crate::charts::uma_color(k, dark);
            painter.circle(c, 6.0, color, Stroke::new(1.5, p.surface));
            for (id, s, _) in &t.skills {
                if m >= *s && m - *s < geom.distance * 0.04 {
                    let text = crate::ui_util::truncate(&self.skill_label(*id), 20);
                    let y = if k == 0 { geom.track.top() - 16.0 } else { geom.track.bottom() + 12.0 };
                    let g = painter.layout_no_wrap(text, FontId::proportional(10.5), crate::charts::ink_on(color));
                    let r = Rect::from_center_size(pos2(c.x, y), g.size() + vec2(8.0, 4.0));
                    painter.rect_filled(r, CornerRadius::same(4), color);
                    painter.galley(r.min + vec2(4.0, 2.0), g, crate::charts::ink_on(color));
                }
            }
        }
    }

    fn results_skills(&mut self, ui: &mut egui::Ui, r: &CompareResult) {
        let p = pal(ui);
        let dark = ui.visuals().dark_mode;
        let Some(course) = self.ran_course() else { return };
        let Some(ran) = self.sim.ran.clone() else { return };
        let d = course.distance as f32;
        card(ui, |ui| {
            ui.label(semibold("Where each skill fired", size::MD));
            ui.label(RichText::new("Each row is one skill: where along the course it activated across the races (darker, more often), its average start (the line) and how often it fired.").size(size::SM).color(p.weak));
            for k in [1usize, 0] {
                let mut rows: Vec<crate::charts::TimelineRow> = Vec::new();
                for id in &ran.umas[k].skills {
                    let stat = r.umas[k].skills.iter().find(|s| s.id == *id);
                    let note = if stat.is_none() { Some(self.never_fired_reason(*id, k, &course, &ran)) } else { None };
                    rows.push(crate::charts::TimelineRow {
                        name: self.skill_label(*id),
                        color: crate::charts::uma_color(k, dark),
                        rate: stat.map_or(0.0, |s| s.rate),
                        mean_start: stat.map_or(0.0, |s| s.mean_start),
                        hist: stat.map(|s| s.starts.clone()).unwrap_or_default(),
                        note,
                        always: if stat.is_none() { self.skill_not_logged(*id).map(str::to_string) } else { None },
                    });
                }
                if rows.is_empty() {
                    continue;
                }
                ui.add_space(4.0);
                ui.label(semibold(format!("Uma {}", k + 1), size::BODY).color(crate::charts::uma_color(k, dark)));
                crate::charts::skill_timeline(ui, d, &rows);
            }
        });
        // What each skill is worth, and where training points matter: runs on demand.
        let running = self.sim.extra.as_ref().map(|j| j.kind);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(semibold("What each of Uma 2's skills is worth", size::MD));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.extra_button(ui, ExtraKind::Contribution, running));
            });
            ui.label(RichText::new("Uma 2 against Uma 2 without each skill in turn, 200 races each: the lengths she'd lose without it.").size(size::SM).color(p.weak));
            match &self.sim.contribution {
                Some(items) => {
                    crate::charts::hbars(ui, items, "L", crate::charts::uma_color(1, dark), p.bad);
                }
                None => {
                    ui.label(RichText::new("Press the button to run it in the background.").size(size::SM).color(p.weak));
                }
            }
        });
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(semibold("Where Uma 2's next training points matter", size::MD));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.extra_button(ui, ExtraKind::Sensitivity, running));
            });
            ui.label(RichText::new("Uma 2 with +50 in each stat in turn, 300 races each: the lengths each +50 gains.").size(size::SM).color(p.weak));
            match &self.sim.sensitivity {
                Some(items) => {
                    crate::charts::hbars(ui, items, "L", crate::charts::uma_color(1, dark), p.bad);
                }
                None => {
                    ui.label(RichText::new("Press the button to run it in the background.").size(size::SM).color(p.weak));
                }
            }
        });
    }

    /// Skills the simulator applies without logging them as firing: passives raise a stat for the
    /// whole race, and gate skills act at the start.
    pub(crate) fn skill_not_logged(&self, id: u32) -> Option<&'static str> {
        let k = self.kb.skills.get(&id)?;
        if k.category == "Passive" {
            Some("Passive: on for the whole race while its condition holds")
        } else if k.effect_text().contains("Start Reaction") {
            Some("Acts at the gate: a quicker start")
        } else {
            None
        }
    }

    /// The likely reason a skill never activated: off in this CM, conditions that can't be met here,
    /// or (conditions met) failed wit checks and rare conditions.
    fn never_fired_reason(&self, id: u32, k: usize, course: &Course, ran: &RunInfo) -> String {
        let cm = self.kb.umalator_presets.iter().find(|e| e.is_cm() && e.course == ran.course && e.disabled_skills.contains(&id));
        if let Some(e) = cm {
            return format!("Turned off in the {}", e.name);
        }
        let Some(data) = self.sim_data.as_ref() else { return "Never activated".into() };
        let u = &ran.umas[k];
        let conditions = [ran.race.0, ran.race.1, ran.race.2, ran.race.3].map(|v| v.clamp(1, 9) as u8);
        let stats = [u.desc.speed, u.desc.stamina, u.desc.power, u.desc.guts, u.desc.wisdom];
        match crate::recommend::skill_can_activate(data, course, conditions, stats, u.desc.strategy, id) {
            Some(false) => "Its conditions can't be met on this course".into(),
            None => "The simulator can't run this skill".into(),
            Some(true) if ran.opts.wit_check => "Never fired: failed Wit checks, or a rare condition".into(),
            Some(true) => "Never fired: its condition didn't come up".into(),
        }
    }

    fn extra_button(&mut self, ui: &mut egui::Ui, kind: ExtraKind, running: Option<ExtraKind>) {
        match running {
            Some(k) if k == kind => {
                let job = self.sim.extra.as_ref().expect("job");
                if ui.small_btn("✖").on_hover_text("Stop").clicked() {
                    job.progress.cancel.store(true, Ordering::Relaxed);
                }
                let done = job.progress.done.load(Ordering::Relaxed) as f32;
                let total = job.progress.total.load(Ordering::Relaxed).max(1) as f32;
                ui.add(egui::ProgressBar::new((done / total).min(1.0)).desired_width(120.0).show_percentage());
            }
            Some(_) => {
                ui.add_enabled(false, egui::Button::new("▶ Run"));
            }
            None => {
                let label = if match kind {
                    ExtraKind::Contribution => self.sim.contribution.is_some(),
                    ExtraKind::Sensitivity => self.sim.sensitivity.is_some(),
                } {
                    "↻ Run again"
                } else {
                    "▶ Run"
                };
                if ui.button(label).on_hover_text("Runs in the background on the last run's setup").clicked() {
                    let ctx = ui.ctx().clone();
                    self.start_extra(&ctx, kind);
                }
            }
        }
    }

    /// Leave-one-out or +50 stat runs on the last run's setup, in the background.
    pub(crate) fn start_extra(&mut self, ctx: &egui::Context, kind: ExtraKind) {
        let (Some(data), Some(ran)) = (self.sim_data.clone(), self.sim.ran.clone()) else { return };
        let Some(course) = data.courses.get(&ran.course).cloned().map(Arc::new) else { return };
        let race = race_params(ran.race.0, ran.race.1, ran.race.2, ran.race.3, None);
        let u2 = ran.umas[1].clone();
        let names: Vec<(u32, String)> = u2.skills.iter().map(|id| (*id, self.skill_label(*id))).collect();
        let progress = Arc::new(Progress::default());
        let (tx, rx) = mpsc::channel();
        let (p2, repaint) = (progress.clone(), ctx.clone());
        let base_opts = ran.opts.clone();
        std::thread::spawn(move || {
            let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
            // Each item: (label, build A, build B); the value is B's lead over A.
            let items: Vec<(String, UmaBuild, UmaBuild, usize)> = match kind {
                ExtraKind::Contribution => names
                    .iter()
                    .map(|(id, name)| {
                        let mut without = u2.clone();
                        without.skills.retain(|s| s != id);
                        (name.clone(), without, u2.clone(), 200)
                    })
                    .collect(),
                ExtraKind::Sensitivity => ["Speed", "Stamina", "Power", "Guts", "Wit"]
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        let mut plus = u2.clone();
                        let s = [&mut plus.desc.speed, &mut plus.desc.stamina, &mut plus.desc.power, &mut plus.desc.guts, &mut plus.desc.wisdom];
                        *s.into_iter().nth(i).expect("five stats") += 50.0;
                        (format!("+50 {name}"), u2.clone(), plus, 300)
                    })
                    .collect(),
            };
            p2.total.store(items.len(), Ordering::Relaxed);
            let chunk = items.len().div_ceil(threads).max(1);
            let mut out: Vec<(String, f64)> = std::thread::scope(|scope| {
                let handles: Vec<_> = items
                    .chunks(chunk)
                    .map(|part| {
                        let (data, course, race, base_opts, p2) = (&data, &course, &race, &base_opts, &p2);
                        scope.spawn(move || {
                            part.iter()
                                .filter_map(|(label, a, b, runs)| {
                                    if p2.cancel.load(Ordering::Relaxed) {
                                        return None;
                                    }
                                    let opts = SimOptions { samples: *runs, ..base_opts.clone() };
                                    let r = run_comparison(data, course, race, a, b, &opts, true, false, None);
                                    p2.done.fetch_add(1, Ordering::Relaxed);
                                    Some((label.clone(), r.mean))
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
            });
            out.sort_by(|a, b| b.1.total_cmp(&a.1));
            let _ = tx.send(out);
            repaint.request_repaint();
        });
        self.sim.extra = Some(ExtraJob { kind, progress, rx });
    }

    fn results_stamina(&mut self, ui: &mut egui::Ui, r: &CompareResult) {
        let Some(course) = self.ran_course() else { return };
        let Some(ran) = self.sim.ran.clone() else { return };
        let wide = ui.available_width() >= 900.0;
        let draw = |ui: &mut egui::Ui, k: usize| self.stamina_card(ui, k, &course, &ran.umas[k], &r.umas[k].hp_left, r.umas[k].full_spurt_rate, r.umas[k].survival_rate);
        if wide {
            ui.columns(2, |cols| {
                draw(&mut cols[0], 0);
                draw(&mut cols[1], 1);
            });
        } else {
            draw(ui, 0);
            draw(ui, 1);
        }
    }

    /// One uma's stamina: HP at the finish across races, max HP against what the race needs, and the
    /// Stamina or recovery a full spurt would take (Moomoolator's HP calculator).
    #[allow(clippy::too_many_arguments)]
    fn stamina_card(&self, ui: &mut egui::Ui, k: usize, course: &Course, u: &UmaBuild, hp_left: &[f64], full_spurt: f64, survival: f64) {
        let p = pal(ui);
        let dark = ui.visuals().dark_mode;
        let color = crate::charts::uma_color(k, dark);
        let style = match u.desc.strategy {
            NIGE => 0,
            SENKOU => 1,
            SASI => 2,
            OIKOMI => 3,
            _ => 4,
        };
        let e = hpcalc::estimate(course, u.desc.stamina, u.desc.guts, u.desc.wisdom, u.desc.speed, style, 0.0);
        card(ui, |ui| {
            ui.label(semibold(format!("Uma {}", k + 1), size::MD).color(color));
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("Full spurt {full_spurt:.0}%")).size(size::SM));
                ui.label(RichText::new("·").color(p.weak));
                ui.label(RichText::new(format!("finished with HP left {survival:.0}%")).size(size::SM));
            });
            ui.add_space(4.0);
            ui.label(RichText::new("HP left at the finish, per race").size(size::SM).color(p.weak));
            crate::charts::mini_hist(ui, hp_left, color, "HP", 80.0);
            ui.add_space(6.0);
            // Max HP against what the race needs, as one bar.
            ui.label(RichText::new("Max HP and what a full-speed race needs (HP calculator)").size(size::SM).color(p.weak));
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
            let scale = e.max_hp.max(e.total_needed).max(1.0) as f32 * 1.12;
            let x = |v: f64| rect.left() + rect.width() * (v as f32 / scale);
            let painter = ui.painter();
            let bar = Rect::from_min_max(pos2(rect.left(), rect.top() + 16.0), pos2(x(e.max_hp), rect.bottom() - 4.0));
            painter.rect_filled(bar, CornerRadius::same(3), color.gamma_multiply(0.8));
            painter.text(bar.right_center() + vec2(-6.0, 0.0), Align2::RIGHT_CENTER, format!("max {:.0}", e.max_hp), FontId::proportional(10.5), crate::charts::ink_on(color));
            let need = x(e.total_needed);
            painter.line_segment([pos2(need, rect.top() + 12.0), pos2(need, rect.bottom())], Stroke::new(2.0, if e.full_spurt { p.text } else { p.bad }));
            painter.text(pos2(need, rect.top()), Align2::CENTER_TOP, format!("needs {:.0}", e.total_needed), FontId::proportional(10.5), p.text);
            resp.on_hover_text(format!("Max HP {:.0}; a full spurt needs about {:.0} ({:.0} before the final leg, {:.0} after).", e.max_hp, e.total_needed, e.phase01_hp, e.phase2_hp));
            let line = if e.full_spurt {
                format!("Enough for a full spurt, with {:.0} HP to spare. Minimum Stamina for it: {}.", e.surplus, e.min_stamina)
            } else {
                format!("Short of a full spurt: needs {} Stamina, or recovery skills worth {:.1}% of max HP.", e.min_stamina, e.heal_needed)
            };
            ui.label(RichText::new(line).size(size::SM).color(if e.full_spurt { p.text } else { p.warn }));
            ui.label(RichText::new("The HP calculator ignores mood, aptitude and ground; the races above don't.").size(size::XS).color(p.weak));
        });
    }

    fn results_runs(&mut self, ui: &mut egui::Ui, r: &CompareResult) {
        let p = pal(ui);
        let dark = ui.visuals().dark_mode;
        card(ui, |ui| {
            ui.label(semibold("Recent runs", size::MD));
            ui.label(RichText::new("The last 10 runs this session. Pin one to draw its gaps as an outline on Overview's chart.").size(size::SM).color(p.weak));
            let mut pin = None;
            for (i, h) in self.sim.history.iter().enumerate().rev() {
                ui.horizontal(|ui| {
                    let pinned = self.sim.pinned == Some(i);
                    let latest = i + 1 == self.sim.history.len();
                    if ui.selectable_label(pinned, if pinned { "📌 Pinned" } else { "📌 Pin" }).on_hover_text("Compare this run's gaps with the current one").clicked() {
                        pin = Some(if pinned { None } else { Some(i) });
                    }
                    ui.label(RichText::new(&h.when).size(size::SM).color(p.weak));
                    ui.label(RichText::new(&h.verdict).size(size::SM).strong());
                    ui.label(RichText::new(&h.label).size(size::SM).color(p.weak));
                    if latest {
                        ui.label(RichText::new("(this run)").size(size::XS).color(p.weak));
                    }
                });
            }
            if let Some(v) = pin {
                self.sim.pinned = v;
            }
        });
        card_titled(ui, "Each uma", |ui| {
            egui::Grid::new("sim_uma_stats").num_columns(3).striped(true).spacing([24.0, 4.0]).show(ui, |ui| {
                ui.label("");
                ui.label(semibold("Uma 1", 14.0).color(crate::charts::uma_color(0, dark)));
                ui.label(semibold("Uma 2", 14.0).color(crate::charts::uma_color(1, dark)));
                ui.end_row();
                let rows: [(&str, Box<dyn Fn(&crate::sim::run::UmaStats) -> String>); 11] = [
                    ("Finish (game clock)", Box::new(|u| format!("{}  ({:.2} s)", game_time(u.mean_time), u.mean_time))),
                    ("Full spurt", Box::new(|u| format!("{:.1}%", u.full_spurt_rate))),
                    ("HP survival", Box::new(|u| format!("{:.1}%", u.survival_rate))),
                    ("Last spurt starts", Box::new(|u| u.mean_spurt_start.map_or("—".into(), |m| format!("{m:.0} m")))),
                    ("HP runs out (when it does)", Box::new(|u| u.mean_hp_out.map_or("—".into(), |m| format!("{m:.0} m")))),
                    ("Spurt speed lost when short", Box::new(|u| if u.full_spurt_rate >= 99.9 { "—".into() } else { format!("{:.2} m/s", -u.non_full_speed_loss) })),
                    ("Rushed", Box::new(|u| if u.rushed_rate > 0.0 { format!("{:.1}% ({:.0} m)", u.rushed_rate, u.rushed_mean_length) } else { "0%".into() })),
                    ("Start delay", Box::new(|u| format!("{:.3} s", u.mean_start_delay))),
                    ("First into the late race", Box::new(|u| format!("{:.0}%", u.first_late_rate))),
                    ("Spot struggle", Box::new(|u| format!("{:.0}%", u.lead_competition_rate))),
                    ("Dueling", Box::new(|u| format!("{:.0}%", u.dueling_rate))),
                ];
                for (label, f) in rows.iter() {
                    ui.label(RichText::new(*label).color(p.weak));
                    ui.label(f(&r.umas[0]));
                    ui.label(f(&r.umas[1]));
                    ui.end_row();
                }
            });
        });
        card_titled(ui, "Export", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("📋 Copy summary").on_hover_text("Copy the setup and results as markdown, to paste into notes or a plan").clicked() {
                    match self.sim_markdown() {
                        Ok(md) => {
                            ui.ctx().copy_text(md);
                            self.toast("Copied the summary as markdown.", crate::ui_util::ToastKind::Success);
                        }
                        Err(e) => self.toast(e, crate::ui_util::ToastKind::Warn),
                    }
                }
                if ui.button("🖼 Save chart as PNG").on_hover_text("Save the results shown on the tab you pick next (Overview, Race…) as a picture in your Pictures folder, under umaLord").clicked() {
                    self.sim.save_png = true;
                }
                if self.sim.from_plan && ui.button("💾 Save to plan").on_hover_text("Writes these results into the plan's ## Simulation section").clicked() {
                    match self.save_sim_to_plan() {
                        Ok(msg) => self.toast(msg, crate::ui_util::ToastKind::Success),
                        Err(e) => self.toast(e, crate::ui_util::ToastKind::Warn),
                    }
                }
            });
        });
    }

    /// Save the results area as a PNG in Pictures\umaLord, by cropping a screenshot.
    fn save_results_png(&self, ctx: &egui::Context) {
        let Some(rect) = self.sim.results_rect else { return };
        let Some(dir) = crate::settings::user_dir("Pictures").map(|d| d.join("umaLord")) else { return };
        let stamp = crate::live::fmt_local(crate::live::now()).replace([':', ' ', ','], "-");
        let path = dir.join(format!("race-sim-{stamp}.png"));
        let ppp = ctx.pixels_per_point();
        let ctx2 = ctx.clone();
        ctx.request_screenshot(move |img| {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let Some(full) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) else { return };
            let x0 = ((rect.left() * ppp).max(0.0) as u32).min(w as u32 - 1);
            let y0 = ((rect.top() * ppp).max(0.0) as u32).min(h as u32 - 1);
            let cw = ((rect.width() * ppp) as u32).min(w as u32 - x0).max(1);
            let ch = ((rect.height() * ppp) as u32).min(h as u32 - y0).max(1);
            let crop = image::imageops::crop_imm(&full, x0, y0, cw, ch).to_image();
            let saved = std::fs::create_dir_all(&dir).is_ok() && crop.save(&path).is_ok();
            if saved {
                crate::ui_util::toast_action(&ctx2, format!("Saved {}", path.display()), crate::ui_util::ToastKind::Success, "Show", crate::ui_util::ToastAction::Open(path.clone()));
            } else {
                crate::ui_util::toast(&ctx2, format!("Couldn't save {}", path.display()), crate::ui_util::ToastKind::Error);
            }
            ctx2.request_repaint();
        });
    }

    fn chart_results(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Test").color(p.weak));
            ui.selectable_value(&mut self.sim.chart_pool, ChartPool::Account, "Skills on your account").on_hover_text("Hints and events from cards you own, skills of umas you own, and Uma 1's own skills");
            ui.selectable_value(&mut self.sim.chart_pool, ChartPool::AllGlobal, "All Global skills");
            ui.separator();
            ui.tick(&mut self.sim.chart_per_sp, "Sort by lengths per 100 SP");
        });
        let Some(rows) = self.sim.chart.clone() else {
            if self.sim.job.is_none() {
                ui.label(RichText::new("▶ Run tests every skill on Uma 1, two rounds like Moomoolator (25 runs, then 175 more for skills that gain). Wit checks, rushing and lane movement are off so each skill's own effect shows.").color(p.weak));
            }
            return;
        };
        let cost = |id: u32| self.kb.skills.get(&id).map_or(0, |s| s.sp);
        let mut rows = rows;
        if self.sim.chart_per_sp {
            rows.sort_by(|a, b| {
                let pa = a.mean / cost(a.id).max(1) as f64;
                let pb = b.mean / cost(b.id).max(1) as f64;
                pb.total_cmp(&pa)
            });
        }
        let mut add = None;
        card(ui, |ui| {
            ui.label(RichText::new("Upper bound: the chart assumes the skill's conditions are met as modelled (order conditions always). Uniques and evolved skills are left out.").size(12.0).color(p.weak));
            egui::Grid::new("sim_chart").num_columns(8).striped(true).spacing([14.0, 3.0]).show(ui, |ui| {
                for h in ["", "Skill", "Mean", "Median", "Min–max", "SP", "L / 100 SP", "Runs"] {
                    ui.label(RichText::new(h).size(12.0).color(p.weak));
                }
                ui.end_row();
                for r in &rows {
                    if ui.small_btn("＋").on_hover_text("Add to Uma 1").clicked() {
                        add = Some(r.id);
                    }
                    self.skill_link(ui, r.id, !self.skill_available(r.id));
                    ui.label(semibold(format!("{:+.2} L", r.mean), 13.5).color(if r.mean > 0.05 { pal(ui).good } else { p.weak }));
                    ui.label(format!("{:+.2}", r.median));
                    ui.label(RichText::new(format!("{:+.2} … {:+.2}", r.min, r.max)).size(12.0).color(p.weak));
                    let sp = cost(r.id);
                    ui.label(sp.to_string());
                    ui.label(if sp > 0 { format!("{:.2}", r.mean / sp as f64 * 100.0) } else { "—".into() });
                    ui.label(RichText::new(r.samples.to_string()).size(12.0).color(p.weak));
                    ui.end_row();
                }
            });
        });
        if let Some(id) = add {
            self.sim.umas[0].skills.push(id);
            if let Some(d) = self.sim_data.clone() {
                self.sim.umas[0].normalise(&d);
            }
        }
    }

    fn stamina_results(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let dark = ui.visuals().dark_mode;
        let Some(r) = self.sim.stamina.clone() else {
            if self.sim.job.is_none() {
                ui.label(RichText::new("▶ Run simulates Uma 1 alone and reports the HP she has left at the finish.").color(p.weak));
            }
            return;
        };
        let med = |v: &[f64]| crate::sim::run::median_of(v);
        let survived = r.remaining.iter().filter(|h| **h >= 0.0).count() as f64 / r.remaining.len().max(1) as f64 * 100.0;
        stat_row(
            ui,
            &[
                ("♥", "Max HP", format!("{:.0}", r.max_hp), "from Stamina".into(), p.accent),
                ("◎", "HP left", format!("{:.0}", med(&r.remaining)), "median; below 0 means she ran dry".into(), if med(&r.remaining) >= 0.0 { p.good } else { p.warn }),
                ("⚡", "Full spurt", format!("{:.0}%", r.full_spurt_runs as f64 / r.runs.max(1) as f64 * 100.0), "of runs at top speed to the end".into(), p.gold),
                ("🏁", "HP survival", format!("{survived:.0}%"), "of runs finished with HP left".into(), p.link),
                ("↘", "Downhill saving", format!("{:.0}", med(&r.downhill_save)), "median HP saved on slopes".into(), p.weak),
            ],
        );
        ui.add_space(6.0);
        let Some(course) = self.sim_data.as_ref().and_then(|d| d.courses.get(&self.sim.course)).cloned() else { return };
        let u = self.sim.umas[0].clone();
        let full = r.full_spurt_runs as f64 / r.runs.max(1) as f64 * 100.0;
        self.stamina_card(ui, 0, &course, &u, &r.remaining, full, survived);
        card(ui, |ui| {
            ui.label(semibold("Speed and HP, median run", size::MD));
            let lines = [crate::charts::TraceLine { trace: &r.median_run, color: crate::charts::uma_color(0, dark), label: "Uma 1" }];
            crate::charts::speed_chart(ui, course.distance as f32, &lines, 220.0, &|id| self.skill_label(id));
            let recovery = |id: u32| self.kb.skills.get(&id).is_some_and(|s| s.category == "Recovery");
            crate::charts::hp_chart(ui, course.distance as f32, &lines, 150.0, &recovery, &|id| self.skill_label(id));
        });
    }

    fn hp_calc(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let Some(course) = self.sim_data.as_ref().and_then(|d| d.courses.get(&self.sim.course)).cloned() else { return };
        let u = &self.sim.umas[0];
        card_titled(ui, "HP calculator", |ui| {
            ui.label(RichText::new("Moomoolator's closed-form estimate from Uma 1's Stamina, Guts, Wit and Speed. It ignores mood, aptitude and ground; confirm with ▶ Stamina.").size(12.0).color(p.weak));
            egui::Grid::new("hp_calc").num_columns(8).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                for h in ["Style", "Max HP", "Needed", "Surplus", "Full spurt", "Min Stamina", "With downhills", "Min Stamina (downhills)"] {
                    ui.label(RichText::new(h).size(12.0).color(p.weak));
                }
                ui.end_row();
                for (k, name) in ["Front Runner", "Pace Chaser", "Late Surger", "End Closer", "Runaway"].iter().enumerate() {
                    let e = hpcalc::estimate(&course, u.desc.stamina, u.desc.guts, u.desc.wisdom, u.desc.speed, k, self.sim.heal_pct);
                    let mine = matches!((k, u.desc.strategy), (0, NIGE) | (1, SENKOU) | (2, SASI) | (3, OIKOMI) | (4, OONIGE));
                    ui.label(if mine { semibold(*name, 13.5) } else { RichText::new(*name) });
                    ui.label(format!("{:.0}", e.max_hp));
                    ui.label(format!("{:.0}", e.total_needed)).on_hover_text(format!("Opening and middle legs {:.0} + final leg and spurt {:.0}", e.phase01_hp, e.phase2_hp));
                    ui.label(RichText::new(format!("{:+.0}", e.surplus)).color(if e.surplus >= 0.0 { pal(ui).good } else { pal(ui).warn }));
                    ui.label(if e.full_spurt { "yes".to_string() } else { format!("no (+{:.1}% heal)", e.heal_needed) });
                    ui.label(e.min_stamina.to_string());
                    ui.label(RichText::new(format!("{:+.0}", e.adjusted_surplus)).color(if e.adjusted_full_spurt { pal(ui).good } else { pal(ui).warn }))
                        .on_hover_text(format!("Downhill mode saves about {:.0} HP, so {:.0} is needed", e.downhill_savings, e.adjusted_needed));
                    ui.label(e.adjusted_min_stamina.to_string());
                    ui.end_row();
                }
            });
            let e = hpcalc::estimate(&course, u.desc.stamina, u.desc.guts, u.desc.wisdom, u.desc.speed, 0, 0.0);
            ui.label(
                RichText::new(format!(
                    "Downhills: {:.0} m; at this Wit, downhill mode is on {:.0}% of the time on them.",
                    e.downhill_distance,
                    e.downhill_mode_rate * 100.0
                ))
                .size(12.0)
                .color(p.weak),
            );
        });
    }

    /// The planner's trainee as a simulator build; `with_skills` adds the skills chosen to buy.
    pub(crate) fn plan_sim_build(&self, with_skills: bool) -> Result<UmaBuild, String> {
        self.plan_sim_build_for(with_skills, self.planner.plan.target)
    }

    /// The plan's trainee with these stats (the targets, or the skill shop's real ones).
    pub(crate) fn plan_sim_build_for(&self, with_skills: bool, target: [u32; 5]) -> Result<UmaBuild, String> {
        let data = self.sim_data.as_ref().ok_or("No simulator data; run tools/update_knowledge.py --only umalator.")?;
        let p = &self.planner.plan;
        let id = p.trainee.ok_or("Pick a trainee first.")?;
        let owned = self.owned_uma(id).ok_or("The trainee isn't in your roster.")?;
        let info = self.kb.umas.get(&id).ok_or("No knowledge data for this trainee.")?;
        if target.iter().all(|v| *v == 0) {
            return Err("Enter target stats first.".into());
        }
        let a = &info.aptitude;
        let stars = owned.stars.clamp(1, 5);
        let mut skills: Vec<u32> = data.unique_skill(id, stars).into_iter().collect();
        if with_skills {
            skills.extend(p.skills.iter().copied().filter(|s| data.skills.contains_key(s)));
        }
        let mut b = UmaBuild {
            outfit: Some(id),
            stars,
            unique_level: u32::from(stars % 3 + stars / 3).max(1),
            desc: HorseDesc {
                speed: f64::from(target[0]),
                stamina: f64::from(target[1]),
                power: f64::from(target[2]),
                guts: f64::from(target[3]),
                wisdom: f64::from(target[4]),
                strategy: strategy_of(p.style),
                distance_apt: parse_aptitude(a.distance(p.distance)),
                surface_apt: parse_aptitude(a.surface(p.surface)),
                strategy_apt: parse_aptitude(a.style(p.style)),
                mood: 2,
            },
            skills,
            forced: HashMap::new(),
        };
        b.normalise(data);
        Ok(b)
    }

    /// Put the plan into the simulator: course and conditions, and the trainee without (Uma 1)
    /// and with (Uma 2) her skills to buy.
    pub(crate) fn load_plan_into_sim(&mut self) -> Result<String, String> {
        let without = self.plan_sim_build(false)?;
        let with = self.plan_sim_build(true)?;
        let setup = self.plan_race_setup();
        let mut notes = setup.notes.clone();
        match setup.course {
            Some(c) => self.sim.course = c,
            None => notes.push("the plan has no course, so the simulator keeps its own".into()),
        }
        if let Some(g) = setup.ground {
            self.sim.ground = i32::from(g);
        }
        if let Some(w) = setup.weather {
            self.sim.weather = i32::from(w);
        }
        if let Some(s) = setup.season {
            self.sim.season = i32::from(s);
        }
        if let Some(t) = setup.time {
            self.sim.time = i32::from(t);
        }
        self.sim.umas = [without, with];
        self.sim.from_plan = true;
        self.sim.compare = None;
        let disabled = self.plan_cm().map(|e| e.disabled_skills.clone()).unwrap_or_default();
        let dropped = self.drop_disabled_skills(&disabled);
        if dropped > 0 {
            notes.push(format!("left out {dropped} skills that won't activate in this CM"));
        }
        let mut msg = "Loaded the plan: Uma 1 without the skills to buy, Uma 2 with them.".to_string();
        if !notes.is_empty() {
            msg += &format!(" Note: {}.", notes.join("; "));
        }
        Ok(msg)
    }

    /// Take skills a Champions Meeting turns off out of both builds; returns how many went.
    pub(crate) fn drop_disabled_skills(&mut self, disabled: &[u32]) -> usize {
        let mut n = 0;
        for u in &mut self.sim.umas {
            let before = u.skills.len();
            u.skills.retain(|s| !disabled.contains(s));
            n += before - u.skills.len();
        }
        n
    }

    /// How far the running Race sim job is, 0–1.
    pub(crate) fn sim_job_fraction(&self) -> Option<f32> {
        let p = self.sim.job.as_ref().map(|j| &j.progress).or(self.sim.extra.as_ref().map(|j| &j.progress))?;
        let done = p.done.load(Ordering::Relaxed) as f32;
        Some((done / p.total.load(Ordering::Relaxed).max(1) as f32).min(1.0))
    }

    /// Stop the Race sim's runs (the assistant's Stop button).
    pub(crate) fn cancel_sim_jobs(&self) {
        for p in self.sim.job.as_ref().map(|j| &j.progress).into_iter().chain(self.sim.extra.as_ref().map(|j| &j.progress)) {
            p.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Planner's "Simulate": load the plan and run Compare.
    pub(crate) fn simulate_plan(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let msg = self.load_plan_into_sim()?;
        self.sim.mode = Mode::Compare;
        self.go_career(crate::CareerTab::Race);
        self.sim_start(ctx);
        if self.sim.job.is_some() {
            self.toast(msg, crate::ui_util::ToastKind::Success);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real Compare, then every dashboard tab draws in both modes; the extras finish in the
    /// background; the history keeps the run.
    #[test]
    fn dashboard_draws_and_extras_run() {
        let mut a = App::for_tests();
        a.tab = crate::Tab::Career;
        a.career_tab = crate::CareerTab::Race;
        a.sim.opts.samples = 60;
        a.sim.auto_seed = false;
        let ctx = egui::Context::default();
        a.sim_start(&ctx);
        let start = std::time::Instant::now();
        while a.sim.job.is_some() {
            assert!(start.elapsed().as_secs() < 60, "the run never finished");
            a.sim_poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let r = a.sim.compare.clone().expect("a result");
        assert_eq!(r.gap_curve.len(), 101);
        assert!(r.gap_curve.iter().all(|g| g.p10 <= g.p50 && g.p50 <= g.p90));
        assert_eq!(a.sim.history.len(), 1);
        for kind in [ExtraKind::Contribution, ExtraKind::Sensitivity] {
            a.start_extra(&ctx, kind);
            while a.sim.extra.is_some() {
                assert!(start.elapsed().as_secs() < 120, "the extra run never finished");
                a.sim_poll(&ctx);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        assert_eq!(a.sim.sensitivity.as_ref().map(Vec::len), Some(5));
        assert!(a.sim.contribution.is_some());
        for mode in [crate::theme::Mode::Dark, crate::theme::Mode::Light] {
            crate::theme::install(&ctx, mode);
            for tab in [ResultTab::Overview, ResultTab::Race, ResultTab::Skills, ResultTab::Stamina, ResultTab::Runs] {
                a.sim.result_tab = tab;
                a.sim.cdf = tab == ResultTab::Runs;
                a.sim.pinned = Some(0);
                a.sim.replay.t = 20.0;
                let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0))), ..Default::default() };
                let _ = crate::ui_util::test_frame(&ctx, input, |ui| a.frame(ui));
            }
        }
    }
}
