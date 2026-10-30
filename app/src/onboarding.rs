//! Setup: the first-run wizard, and Settings → Profile's actions. Load a saved profile
//! (`profile.rs`), or click the umas and support cards you own (`onboarding_grid.rs`, with a
//! brush, multi-select, keys and Paste a list in `paste.rs`). It opens by itself when the roster is
//! empty, and replaces the page until it's finished or skipped. Every change is saved to
//! `account/` straight away and can be undone.

use std::collections::BTreeSet;
use std::path::PathBuf;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke, vec2};

use crate::App;
use crate::data::{CardType, Rarity};
use crate::profile::{self, Profile, stamp};
use crate::theme::{pal, size};
use crate::ui_util::{ToastKind, card, chibi, semibold, stepper, today, type_color, type_icon};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Step {
    Welcome,
    Import,
    /// "Make it yours": the trainer name, mode, theme and mascot.
    You,
    Umas,
    Cards,
    Done,
}

impl Step {
    /// The steps of one path through setup, with their labels.
    pub fn path(via_import: bool) -> Vec<(Step, &'static str)> {
        if via_import {
            vec![(Step::Welcome, "Welcome"), (Step::Import, "Load a profile"), (Step::You, "Make it yours"), (Step::Done, "All set")]
        } else {
            vec![(Step::Welcome, "Welcome"), (Step::You, "Make it yours"), (Step::Umas, "Your umas"), (Step::Cards, "Your cards"), (Step::Done, "All set")]
        }
    }

    pub fn next(self, via_import: bool) -> Option<Step> {
        let path = Step::path(via_import);
        let i = path.iter().position(|(s, _)| *s == self)?;
        path.get(i + 1).map(|(s, _)| *s)
    }

    pub fn back(self, via_import: bool) -> Option<Step> {
        let path = Step::path(via_import);
        let i = path.iter().position(|(s, _)| *s == self)?;
        i.checked_sub(1).map(|i| path[i].0)
    }
}

/// A change to the roster, as setup makes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Edit {
    Uma { id: u32, stars: u8, potential: u8 },
    /// `level: None` sets the most the LB allows.
    Card { id: u32, lb: u8, level: Option<u8> },
    RemoveUma(u32),
    RemoveCard(u32),
}

/// What a row was before a change, to undo it (`None`: it wasn't there).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Change {
    Uma { id: u32, before: Option<(u8, u8)> },
    Card { id: u32, before: Option<(u8, u8)> },
}

pub(crate) struct OnboardingState {
    pub open: bool,
    pub step: Option<Step>,
    /// The trainer name on "Make it yours".
    pub name: String,
    /// `name` was filled from the account for this visit to the step.
    pub name_loaded: bool,
    pub search: String,
    /// Card type filter on the cards step.
    pub kind: Option<String>,
    /// Show R cards too.
    pub show_r: bool,
    pub only_owned: bool,
    /// Newest releases first (new players own recent banners).
    pub newest_first: bool,
    /// The tile whose details show in the side panel.
    pub selected: Option<u32>,
    /// Tiles picked for a bulk change (Ctrl-click, Shift-click).
    pub multi: BTreeSet<u32>,
    /// Where a Shift-click range starts.
    pub anchor: Option<u32>,
    /// The keyboard's tile.
    pub focus: Option<u32>,
    /// Scroll the focused tile into view (it moved by keyboard).
    pub scroll_to_focus: bool,
    /// The search box takes focus (typing started on the grid).
    pub focus_search: bool,
    /// "New picks start as": stars and potential for umas, LB for cards.
    pub brush_stars: u8,
    pub brush_pot: u8,
    pub brush_lb: u8,
    /// Changes to undo, newest last; each entry is one action (a click, a bulk change, a pasted list).
    pub undo: Vec<Vec<Change>>,
    pub paste: crate::paste::PasteState,
    /// The roster changed since the digest was last written.
    pub dirty: bool,
    /// Profile files in profiles/ and Downloads, scanned when the Import step opens.
    pub found: Option<Vec<(PathBuf, Result<Profile, String>)>>,
    pub path: String,
    /// A profile picked to import, waiting for the user to confirm.
    pub pending: Option<(PathBuf, Profile)>,
    /// Settings → Profile's "Start over" confirmation is showing.
    pub confirm_start_over: bool,
    /// Setup went the "Load a profile" way.
    pub via_import: bool,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            open: false,
            step: None,
            name: String::new(),
            name_loaded: false,
            search: String::new(),
            kind: None,
            show_r: false,
            only_owned: false,
            newest_first: false,
            selected: None,
            multi: BTreeSet::new(),
            anchor: None,
            focus: None,
            scroll_to_focus: false,
            focus_search: false,
            brush_stars: 3,
            brush_pot: 1,
            brush_lb: 0,
            undo: Vec::new(),
            paste: Default::default(),
            dirty: false,
            found: None,
            path: String::new(),
            pending: None,
            confirm_start_over: false,
            via_import: false,
        }
    }
}

/// Setup's pages are a centred column this wide at most.
const COLUMN: f32 = 1100.0;

/// Lay `add` out in a centred column.
pub(crate) fn centred<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let w = ui.available_width().min(COLUMN);
    let pad = ((ui.available_width() - w) / 2.0).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(pad);
        ui.vertical(|ui| {
            ui.set_width(w);
            add(ui)
        })
        .inner
    })
    .inner
}

impl App {
    /// Open setup at a step.
    pub(crate) fn open_setup(&mut self, step: Step) {
        self.finish_setup_edits();
        let o = &mut self.onboarding;
        o.open = true;
        o.step = Some(step);
        o.search.clear();
        o.selected = None;
        o.multi.clear();
        o.anchor = None;
        o.focus = None;
        o.name_loaded = false;
        match step {
            Step::Import => {
                o.found = None;
                o.via_import = true;
            }
            Step::Umas | Step::Cards => o.via_import = false,
            _ => {}
        }
    }

    /// Go to the step after this one.
    fn setup_next(&mut self) {
        let step = self.onboarding.step.unwrap_or(Step::Welcome);
        if step == Step::You {
            self.save_setup_name();
        }
        if let Some(next) = step.next(self.onboarding.via_import) {
            let via = self.onboarding.via_import;
            self.open_setup(next);
            self.onboarding.via_import = via;
        }
    }

    fn setup_back(&mut self) {
        let step = self.onboarding.step.unwrap_or(Step::Welcome);
        if let Some(back) = step.back(self.onboarding.via_import) {
            let via = self.onboarding.via_import;
            self.open_setup(back);
            self.onboarding.via_import = via;
        }
    }

    /// Close setup (Skip, Close, or a tab clicked).
    pub(crate) fn close_setup(&mut self) {
        self.finish_setup_edits();
        self.onboarding.open = false;
    }

    /// After a run of edits: the digest for the Ask tab, once rather than on every click.
    pub(crate) fn finish_setup_edits(&mut self) {
        if std::mem::take(&mut self.onboarding.dirty) && std::env::var_os("UMALORD_PROFILE").is_none() {
            self.write_digest();
        }
    }

    /// After the account's files changed underneath the app: re-read everything that depends on them.
    fn account_changed(&mut self) {
        self.reload_roster();
        self.planner.plan = Default::default();
        self.planner.open_file = None;
        self.planner.skill_status = None;
        self.planner.refresh_files(self.root.as_deref());
        self.planner.rank_cache.borrow_mut().clear();
        *self.planner.career_cache.borrow_mut() = None;
        self.notes.loaded = None;
        self.umas.selected = None;
        self.cards.selected = None;
        self.onboarding.undo.clear();
        self.write_digest();
    }

    /// The profile's name: what umas.md says, else "My account".
    pub(crate) fn profile_name(&self) -> String {
        self.roster.profile.clone().unwrap_or_else(|| "My account".into())
    }

    /// Settings → Profile → Save my data: write the account to profiles/.
    pub(crate) fn export_profile(&self) -> Result<PathBuf, String> {
        let root = self.root.as_ref().ok_or("umaLord folder not found.")?;
        profile::export(root, &self.profile_name(), &crate::ui_util::local_today())
    }

    fn import_profile(&mut self, p: &Profile) -> Result<String, String> {
        let root = self.root.clone().ok_or("umaLord folder not found.")?;
        let saved = profile::import(&root, p, &stamp())?;
        self.account_changed();
        let mut msg = format!("Loaded {}: {} umas and {} support cards.", p.name, self.roster.umas.len(), self.roster.cards.len());
        if let Some(s) = saved {
            msg += &format!(" What was here before is saved as profiles/backups/{}.", profile::file_name(&s));
        }
        Ok(msg)
    }

    /// Back up and empty the account, named for whoever it's for now.
    pub(crate) fn start_over(&mut self, name: &str) -> Result<Option<PathBuf>, String> {
        let root = self.root.clone().ok_or("umaLord folder not found.")?;
        let saved = profile::start_over(&root, name, &stamp())?;
        self.account_changed();
        Ok(saved)
    }

    /// Write `- Profile: <name>` in account/umas.md.
    pub(crate) fn set_profile_name(&mut self, name: &str) -> Result<(), String> {
        let root = self.root.clone().ok_or("umaLord folder not found.")?;
        let path = root.join(crate::data::UMAS_FILE);
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        std::fs::write(&path, crate::data::set_meta(&text, "Profile", name)).map_err(|e| e.to_string())?;
        self.reload_roster();
        self.onboarding.dirty = true;
        Ok(())
    }

    /// The name from "Make it yours", or "Trainer" if it was left empty.
    fn save_setup_name(&mut self) {
        let name = self.onboarding.name.trim();
        let name = if name.is_empty() { "Trainer".to_string() } else { name.to_string() };
        if self.roster.profile.as_deref() != Some(name.as_str())
            && let Err(e) = self.set_profile_name(&name)
        {
            self.toast(e, ToastKind::Error);
        }
    }

    /// Apply roster changes: each account file is read and written once. With `record`, the
    /// changes can be undone as one action. Returns how many rows changed.
    pub(crate) fn apply_edits(&mut self, edits: &[Edit], record: bool) -> Result<usize, String> {
        // UMALORD_PROFILE shows a profile in memory; writing would land in the real account files.
        if std::env::var_os("UMALORD_PROFILE").is_some() {
            return Err("This roster is a profile shown without loading it (UMALORD_PROFILE), so it can't be edited.".into());
        }
        let root = self.root.clone().ok_or("umaLord folder not found.")?;
        let today = crate::ui_util::local_today();
        let (up, cp) = (root.join(crate::data::UMAS_FILE), root.join(crate::data::CARDS_FILE));
        let mut umas = std::fs::read_to_string(&up).map_err(|e| e.to_string())?;
        let mut cards = std::fs::read_to_string(&cp).map_err(|e| e.to_string())?;
        let (mut umas_changed, mut cards_changed) = (false, false);
        let mut undo = Vec::new();
        for e in edits {
            match *e {
                Edit::Uma { id, stars, potential } => {
                    let info = self.kb.umas.get(&id).ok_or("No data for that uma.")?;
                    let before = self.owned_uma(id).map(|u| (u.stars, u.potential));
                    if before == Some((stars, potential)) {
                        continue;
                    }
                    let row = crate::data::UmaRow { id, name: info.name.clone(), outfit: info.outfit.clone(), stars, potential };
                    umas = crate::data::upsert_uma(&umas, &row, &today)?.0;
                    umas_changed = true;
                    undo.push(Change::Uma { id, before });
                }
                Edit::Card { id, lb, level } => {
                    let info = self.kb.cards.get(&id).ok_or("No data for that card.")?;
                    let card_type = CardType::parse(&info.kind).ok_or("Unknown card type.")?;
                    let level = level.unwrap_or_else(|| Rarity::from_id(id).map_or(1, |r| r.max_level(lb)));
                    let before = self.owned_card(id).map(|c| (c.lb, c.level));
                    if before == Some((lb, level)) {
                        continue;
                    }
                    // Empty notes keep the row's own.
                    let row = crate::data::CardRow { id, title: info.title.clone(), uma: info.uma.clone(), card_type, lb, level, notes: String::new() };
                    cards = crate::data::upsert_card(&cards, &row, &today)?.0;
                    cards_changed = true;
                    undo.push(Change::Card { id, before });
                }
                Edit::RemoveUma(id) => {
                    let Some(u) = self.owned_uma(id) else { continue };
                    let before = Some((u.stars, u.potential));
                    if let Some(t) = crate::data::remove_row(&umas, id, &today) {
                        umas = t;
                        umas_changed = true;
                        undo.push(Change::Uma { id, before });
                    }
                }
                Edit::RemoveCard(id) => {
                    let Some(c) = self.owned_card(id) else { continue };
                    let before = Some((c.lb, c.level));
                    if let Some(t) = crate::data::remove_row(&cards, id, &today) {
                        cards = t;
                        cards_changed = true;
                        undo.push(Change::Card { id, before });
                    }
                }
            }
        }
        if umas_changed {
            std::fs::write(&up, umas).map_err(|e| e.to_string())?;
        }
        if cards_changed {
            std::fs::write(&cp, cards).map_err(|e| e.to_string())?;
        }
        let n = undo.len();
        if n > 0 {
            if record {
                self.onboarding.undo.push(undo);
                let excess = self.onboarding.undo.len().saturating_sub(50);
                self.onboarding.undo.drain(..excess);
            }
            self.roster_edited();
            self.onboarding.dirty = true;
        }
        Ok(n)
    }

    /// Ctrl+Z: put back what the last action changed.
    pub(crate) fn undo_setup(&mut self) {
        let Some(group) = self.onboarding.undo.pop() else {
            self.toast("Nothing to undo.", ToastKind::Info);
            return;
        };
        let edits: Vec<Edit> = group
            .iter()
            .rev()
            .map(|c| match *c {
                Change::Uma { id, before: Some((stars, potential)) } => Edit::Uma { id, stars, potential },
                Change::Uma { id, before: None } => Edit::RemoveUma(id),
                Change::Card { id, before: Some((lb, level)) } => Edit::Card { id, lb, level: Some(level) },
                Change::Card { id, before: None } => Edit::RemoveCard(id),
            })
            .collect();
        match self.apply_edits(&edits, false) {
            Ok(n) => self.toast(format!("Undid {n} change{}.", if n == 1 { "" } else { "s" }), ToastKind::Info),
            Err(e) => self.toast(e, ToastKind::Error),
        }
    }

    /// Apply edits from a click, with a toast on failure.
    pub(crate) fn edit_roster(&mut self, edits: &[Edit]) {
        if let Err(e) = self.apply_edits(edits, true) {
            self.toast(e, ToastKind::Error);
        }
    }

    /// Pulled another copy of an owned card: one more LB, saved at once, with Undo. The level
    /// stays: a copy raises the cap, not the level.
    pub(crate) fn raise_card(&mut self, id: u32) {
        let Some(c) = self.owned_card(id).cloned() else { return };
        if c.lb >= 4 {
            self.toast(format!("{} [{}] is already maxed (MLB).", c.uma, c.title), ToastKind::Info);
            return;
        }
        let lb = c.lb + 1;
        let now = if lb >= 4 { "maxed (MLB)".to_string() } else { format!("LB{lb}") };
        self.raise(&[Edit::Card { id, lb, level: Some(c.level) }], format!("{} [{}] is now {now}.", c.uma, c.title));
    }

    /// Pulled another copy of an owned uma: one more star, saved at once, with Undo.
    pub(crate) fn raise_uma(&mut self, id: u32) {
        let Some(u) = self.owned_uma(id).cloned() else { return };
        if u.stars >= 5 {
            self.toast(format!("{} [{}] already has 5 stars.", u.name, u.outfit), ToastKind::Info);
            return;
        }
        let stars = u.stars + 1;
        self.raise(&[Edit::Uma { id, stars, potential: u.potential }], format!("{} [{}] is now {stars}★.", u.name, u.outfit));
    }

    fn raise(&mut self, edits: &[Edit], done: String) {
        match self.apply_edits(edits, true) {
            Ok(_) => {
                self.finish_setup_edits();
                self.toast_action(done, ToastKind::Success, "Undo", crate::ui_util::ToastAction::Undo);
            }
            Err(e) => self.toast(e, ToastKind::Error),
        }
    }

    pub(crate) fn roster_edited(&mut self) {
        self.reload_roster();
        self.planner.rank_cache.borrow_mut().clear();
        *self.planner.career_cache.borrow_mut() = None;
    }

    /// A `.umalord.json` dropped on the window opens it in setup's Import step; any other `.json`
    /// is taken for finished umas (UmaExtractor's data.json) and imported.
    pub(crate) fn take_dropped_profile(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(path) = dropped.iter().find(|p| {
            let n = p.to_string_lossy().to_lowercase();
            n.ends_with(".json") && !n.ends_with(profile::EXT)
        }) {
            let r = self.import_veterans(path);
            self.toast_flag(r.map_or_else(|e| (e, true), |m| (m, false)));
            return;
        }
        let Some(path) = dropped.into_iter().find(|p| p.to_string_lossy().to_lowercase().ends_with(profile::EXT)) else { return };
        self.open_setup(Step::Import);
        match profile::read(&path) {
            Ok(p) => self.onboarding.pending = Some((path, p)),
            Err(e) => self.toast(e, ToastKind::Warn),
        }
    }

    /// The setup page, in place of the tab's page: a sticky header (steps, and the grid's tools),
    /// a sticky footer (Back, progress, Next), the selected tile's editor docked right, and the step.
    pub(crate) fn onboarding_page(&mut self, ui: &mut egui::Ui) {
        let step = self.onboarding.step.unwrap_or(Step::Welcome);
        let grid = match step {
            Step::Umas => Some(crate::onboarding_grid::GridKind::Umas),
            Step::Cards => Some(crate::onboarding_grid::GridKind::Cards),
            _ => None,
        };
        let ctx = ui.ctx().clone();
        egui::Panel::top("setup_head").frame(egui::Frame::new().inner_margin(Margin { left: 18, right: 18, top: 10, bottom: 8 })).show(ui, |ui| {
            centred(ui, |ui| {
                self.setup_header(ui, step);
                if let Some(kind) = grid {
                    self.setup_toolbar(ui, kind);
                }
            });
        });
        if step != Step::Welcome && step != Step::Done {
            egui::Panel::bottom("setup_foot").frame(egui::Frame::new().inner_margin(Margin::symmetric(18, 10))).show(ui, |ui| centred(ui, |ui| self.setup_footer(ui, step)));
        }
        if let Some(kind) = grid
            && self.setup_selection_owned(kind)
        {
            egui::Panel::right("setup_edit").resizable(false).exact_size(270.0).frame(crate::side_frame()).show(ui, |ui| self.setup_side_editor(ui, kind));
        }
        egui::CentralPanel::default().frame(crate::page_frame()).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                centred(ui, |ui| match step {
                    Step::Welcome => self.setup_welcome(ui),
                    Step::Import => self.setup_import(ui),
                    Step::You => self.setup_you(ui),
                    Step::Umas | Step::Cards => self.setup_grid(ui, grid.expect("a grid step")),
                    Step::Done => self.setup_done(ui),
                });
                ui.add_space(12.0);
            });
        });
        if let Some(kind) = grid {
            self.setup_keys(&ctx, kind);
        }
        self.paste_dialog(&ctx);
    }

    /// The stepper, and Skip or Close.
    fn setup_header(&mut self, ui: &mut egui::Ui, step: Step) {
        let path = Step::path(self.onboarding.via_import);
        let at = path.iter().position(|(s, _)| *s == step).unwrap_or(0);
        let labels: Vec<&str> = path.iter().map(|(_, l)| *l).collect();
        ui.horizontal(|ui| {
            ui.label(semibold("✨ Set up", size::XL));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (label, hover) = if self.roster.is_empty() {
                    ("Skip for now", "Look around first. ⚙ Settings → Profile → Set up my collection opens this again.")
                } else {
                    ("✖ Close", "Back to the app")
                };
                if ui.button(label).on_hover_text(hover).clicked() {
                    self.close_setup();
                }
                // The stepper takes the rest of the row.
                let w = ui.available_width();
                ui.allocate_ui_with_layout(vec2(w, 50.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // Finished steps can be revisited, except Welcome's choice and an import.
                    if let Some(i) = stepper(ui, &labels, at)
                        && path[i].0 != Step::Welcome
                        && path[i].0 != Step::Import
                    {
                        let via = self.onboarding.via_import;
                        self.open_setup(path[i].0);
                        self.onboarding.via_import = via;
                    }
                });
            });
        });
    }

    /// Back, how much is added, and Next.
    fn setup_footer(&mut self, ui: &mut egui::Ui, step: Step) {
        let p = pal(ui);
        ui.horizontal(|ui| {
            if step.back(self.onboarding.via_import).is_some() && ui.add(egui::Button::new("← Back").min_size(vec2(90.0, 34.0))).clicked() {
                self.setup_back();
            }
            ui.add_space(12.0);
            ui.label(RichText::new(format!("{} umas · {} support cards added", self.roster.umas.len(), self.roster.cards.len())).color(p.weak));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let next = match step {
                    Step::You if self.onboarding.via_import => "Finish →",
                    Step::You => "Next: your umas →",
                    Step::Umas => "Next: your support cards →",
                    Step::Cards => "Finish →",
                    _ => "",
                };
                if !next.is_empty() && ui.add(crate::ui_util::primary_button(ui, next, size::BODY).min_size(vec2(0.0, 34.0))).clicked() {
                    self.setup_next();
                }
                if !self.onboarding.undo.is_empty() && ui.button("↶ Undo").on_hover_text("Undo the last change (Ctrl+Z)").clicked() {
                    self.undo_setup();
                }
            });
        });
    }

    fn setup_welcome(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        // A hero in the theme's colours, with Seiun Sky.
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(w, 190.0), Sense::hover());
        crate::ui_util::gradient_rect(ui.painter(), rect, 16.0, p.surface, p.sky);
        ui.painter().rect_stroke(rect, CornerRadius::same(16), Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(28.0, 24.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.vertical(|ui| {
            ui.set_max_width(rect.width() - 230.0);
            ui.label(semibold("Welcome to umaLord", size::XXL + 4.0));
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Your helper for Uma Musume: Pretty Derby (Global): it knows the umas and support cards you own, plans careers for \
                         Champions Meetings and more, picks decks and skills, and races builds in a built-in simulator.",
                    )
                    .size(size::MD)
                    .color(p.text),
                )
                .wrap(),
            );
            ui.add_space(4.0);
            ui.label(RichText::new("First, tell it what you own. Everything stays on this computer.").color(p.weak));
        });
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::theme::chibi() != crate::theme::Chibi::Off {
                chibi(ui, 0, 160.0);
            }
        });
        ui.add_space(16.0);
        let half = (ui.available_width() - 16.0) / 2.0;
        let mut go = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            if self.choice_card(ui, half, Collage::Umas, "📥 Load a profile", "Someone saved your account as a .umalord.json file, or you have a backup. Load it and you're done.").clicked() {
                go = Some(Step::Import);
            }
            if self.choice_card(ui, half, Collage::Cards, "✨ Set up from scratch", "Click the umas and support cards you own: one click each. A few minutes, and you can add more any time.").clicked() {
                go = Some(Step::You);
            }
        });
        match go {
            Some(Step::Import) => self.open_setup(Step::Import),
            Some(_) => {
                self.open_setup(Step::You);
                self.onboarding.via_import = false;
            }
            None => {}
        }
        if !self.roster.is_empty() {
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!(
                    "This app already has {} umas and {} cards ({}). Setting up from scratch adds to them; ⚙ Settings → Profile → Start over empties them first.",
                    self.roster.umas.len(),
                    self.roster.cards.len(),
                    self.profile_name()
                ))
                .size(size::SM)
                .color(p.warn),
            );
        }
    }

    /// A big choice with a little collage of art, a title and a line on it.
    fn choice_card(&self, ui: &mut egui::Ui, width: f32, collage: Collage, title: &str, body: &str) -> egui::Response {
        let p = pal(ui);
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 210.0), Sense::click());
        let hovered = resp.hovered();
        let painter = ui.painter();
        if hovered {
            painter.add(egui::Shadow { offset: [0, 4], blur: 18, spread: 0, color: Color32::from_black_alpha(70) }.as_shape(rect, CornerRadius::same(14)));
        }
        painter.rect_filled(rect, CornerRadius::same(14), if hovered { p.raised } else { p.surface });
        painter.rect_stroke(rect, CornerRadius::same(14), Stroke::new(if hovered { 1.5 } else { 1.0 }, if hovered { p.accent } else { p.border }), egui::StrokeKind::Inside);
        // The collage: three overlapping pictures.
        let ids = self.collage_ids(collage);
        let (w, h) = match collage {
            Collage::Umas => (72.0, 72.0),
            Collage::Cards => (60.0, 80.0),
        };
        for (k, id) in ids.iter().enumerate() {
            let at = rect.left_top() + vec2(22.0 + k as f32 * (w * 0.62), 20.0 + if k == 1 { -6.0 } else { 0.0 });
            let r = Rect::from_min_size(at, vec2(w, h));
            painter.rect_filled(r.expand(2.0), CornerRadius::same(10), p.surface);
            let sub = if collage == Collage::Umas { "umas" } else { "cards" };
            if let Some(uri) = self.art_uri(sub, *id) {
                crate::ui_util::paint_cover(ui, egui::Image::new(uri).corner_radius(CornerRadius::same(8)), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)));
            }
        }
        let text_x = rect.left() + 22.0;
        let painter = ui.painter();
        painter.text(egui::pos2(text_x, rect.top() + 116.0), Align2::LEFT_TOP, title, FontId::new(size::LG + 2.0, egui::FontFamily::Name(crate::theme::SEMIBOLD.into())), p.text);
        let galley = painter.layout(body.to_string(), FontId::proportional(13.5), p.weak, width - 44.0);
        painter.galley(egui::pos2(text_x, rect.top() + 146.0), galley, p.weak);
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    /// Three pictures for a choice card: well-known umas, or SSR cards, that have art here.
    fn collage_ids(&self, collage: Collage) -> Vec<u32> {
        let wanted: &[u32] = match collage {
            Collage::Umas => &[100101, 102001, 100601],
            Collage::Cards => &[30028, 30076, 30044],
        };
        let have = |id: &u32| if collage == Collage::Umas { self.uma_art.contains(id) } else { self.card_art.contains(id) };
        let mut ids: Vec<u32> = wanted.iter().copied().filter(have).collect();
        let pool: Vec<u32> = if collage == Collage::Umas { self.uma_art.iter().copied().collect() } else { self.card_art.iter().copied().filter(|c| *c >= 30_000).collect() };
        let mut pool = pool;
        pool.sort_unstable();
        for id in pool {
            if ids.len() >= 3 {
                break;
            }
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }

    /// "Make it yours": the name, then how the app looks.
    fn setup_you(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        if !self.onboarding.name_loaded {
            self.onboarding.name = self.roster.profile.clone().unwrap_or_default();
            self.onboarding.name_loaded = true;
        }
        card(ui, |ui| {
            ui.label(semibold("What should we call you?", size::XL));
            ui.label(RichText::new("Your trainer name: Home greets you by it, and profiles you save carry it.").color(p.weak));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.onboarding.name)
                        .hint_text("Your trainer name")
                        .font(FontId::proportional(20.0))
                        .desired_width(340.0)
                        .margin(Margin::symmetric(10, 6)),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.setup_next();
                }
                if self.onboarding.name.trim().is_empty() {
                    ui.label(RichText::new("Leave it empty to be called Trainer.").size(size::SM).color(p.weak));
                }
            });
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(semibold("How should it look?", size::LG));
            ui.label(RichText::new("Try them: the app changes as you click. All of this is in ⚙ Settings later.").color(p.weak));
            ui.add_space(8.0);
            ui.label(semibold("Mode", size::BODY));
            self.mode_picker(ui);
            ui.add_space(10.0);
            ui.label(semibold("Theme", size::BODY));
            self.theme_picker(ui);
            ui.add_space(4.0);
            ui.label(semibold("Background and Seiun Sky", size::BODY));
            self.background_toggle(ui);
            ui.add_space(4.0);
            self.chibi_picker(ui);
        });
    }

    fn setup_import(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        if self.onboarding.found.is_none() {
            self.onboarding.found = Some(self.root.as_deref().map(profile::find).unwrap_or_default());
        }
        // A file picked: confirm before replacing anything.
        if let Some((path, prof)) = self.onboarding.pending.clone() {
            let mut go = false;
            let mut cancel = false;
            card(ui, |ui| {
                ui.label(semibold(format!("📄 {}", prof.name), 20.0));
                ui.label(RichText::new(prof.summary()).color(p.weak));
                ui.label(RichText::new(path.display().to_string()).size(12.0).color(p.weak));
                ui.add_space(6.0);
                if !self.roster.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "This replaces what's in the app now ({}: {} umas, {} cards). That's saved to profiles/backups first, so you can load it back.",
                            self.profile_name(),
                            self.roster.umas.len(),
                            self.roster.cards.len()
                        ))
                        .color(p.warn),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.add(crate::ui_util::primary_button(ui, "📥 Load this profile", 15.0).min_size(vec2(0.0, 32.0))).clicked() {
                        go = true;
                    }
                    if ui.button("Pick another").clicked() {
                        cancel = true;
                    }
                });
            });
            if go {
                match self.import_profile(&prof) {
                    Ok(msg) => {
                        self.toast(msg, ToastKind::Success);
                        self.onboarding.pending = None;
                        self.open_setup(Step::You);
                        self.onboarding.via_import = true;
                    }
                    Err(e) => self.toast(e, ToastKind::Warn),
                }
            }
            if cancel {
                self.onboarding.pending = None;
            }
            return;
        }
        card(ui, |ui| {
            ui.label(semibold("Load a profile", 20.0));
            ui.label(RichText::new("A profile is one .umalord.json file with someone's umas, cards, parents, plans and results. Pick one below, drag the file onto this window, or paste where it is.").color(p.weak));
            ui.add_space(8.0);
            let found = self.onboarding.found.clone().unwrap_or_default();
            if found.is_empty() {
                ui.label(RichText::new("No profiles in the profiles folder or in Downloads yet.").color(p.weak));
            }
            for (path, prof) in &found {
                ui.horizontal(|ui| {
                    let folder = path.parent().and_then(|d| d.file_name()).map_or(String::new(), |n| n.to_string_lossy().to_string());
                    match prof {
                        Ok(prof) => {
                            if ui.add(crate::ui_util::primary_button(ui, "Choose", 13.5)).clicked() {
                                self.onboarding.pending = Some((path.clone(), prof.clone()));
                            }
                            ui.label(semibold(format!("📄 {}", prof.name), 15.0));
                            ui.label(RichText::new(prof.summary()).color(p.weak));
                            ui.label(RichText::new(format!("{} in {folder}", profile::file_name(path))).size(12.0).color(p.weak));
                        }
                        Err(e) => {
                            ui.label(RichText::new(format!("⚠ {e}")).size(12.5).color(p.warn));
                        }
                    }
                });
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.onboarding.path).hint_text("…or paste the file's full path").desired_width(420.0));
                if ui.button("Open").clicked() {
                    let path = PathBuf::from(self.onboarding.path.trim().trim_matches('"'));
                    match profile::read(&path) {
                        Ok(prof) => self.onboarding.pending = Some((path, prof)),
                        Err(e) => self.toast(e, ToastKind::Warn),
                    }
                }
                if ui.button("📂 Open the profiles folder").on_hover_text("Put a .umalord.json file here and it shows up above").clicked()
                    && let Some(root) = &self.root
                {
                    let dir = root.join(profile::DIR);
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = crate::ui_util::show_in_folder(&dir);
                }
                if ui.button("🔄 Look again").clicked() {
                    self.onboarding.found = None;
                }
            });
        });
    }

    /// "All set": what's in the app now, counted up, the next Champions Meeting, and where to go.
    fn setup_done(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let umas = &self.roster.umas;
        let cards = &self.roster.cards;
        // Count up once, over 0.8 s, the first time the page shows.
        let start_id = egui::Id::unique("setup_done_start");
        let now = ui.input(|i| i.time);
        let start = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(start_id, || now));
        let t = (((now - start) / 0.8) as f32).clamp(0.0, 1.0);
        let ease = 1.0 - (1.0 - t).powi(3);
        if t < 1.0 {
            ui.ctx().request_repaint();
        }
        let count = |n: usize| ((n as f32) * ease).round() as usize;
        ui.horizontal(|ui| {
            if crate::theme::chibi() != crate::theme::Chibi::Off {
                chibi(ui, 3, 110.0);
            }
            ui.vertical(|ui| {
                ui.add_space(18.0);
                ui.label(semibold(format!("You're all set, {}!", self.profile_name()), size::XXL));
                ui.label(RichText::new(self.friendly_line()).size(size::MD).color(p.weak));
            });
        });
        ui.add_space(10.0);
        let stars = |n: u8| umas.iter().filter(|u| u.stars == n).count();
        let ssr = cards.iter().filter(|c| c.rarity == Rarity::SSR).count();
        let mlb_ssr = cards.iter().filter(|c| c.rarity == Rarity::SSR && c.is_mlb()).count();
        crate::ui_util::stat_row(
            ui,
            &[
                ("🐴", "Umas", count(umas.len()).to_string(), format!("{} at 5★ · {} at 4★ · {} at 3★", stars(5), stars(4), stars(3)), p.pink),
                ("🃏", "Support cards", count(cards.len()).to_string(), format!("{ssr} SSR · {} SR", cards.iter().filter(|c| c.rarity == Rarity::SR).count()), p.link),
                ("💎", "Maxed SSRs", count(mlb_ssr).to_string(), "fully limit broken (MLB)".into(), p.accent),
            ],
        );
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(semibold("Maxed SSRs by type", size::MD));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 20.0;
                for t in CardType::ALL {
                    let n = cards.iter().filter(|c| c.card_type == t && c.rarity == Rarity::SSR && c.is_mlb()).count();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(3), type_color(t));
                        ui.label(RichText::new(format!("{} {}", type_icon(t.label()), t.label())).color(crate::ui_util::type_text(ui, t.label())));
                        ui.label(semibold(count(n).to_string(), size::BODY));
                    });
                }
            });
        });
        if let Some(cm) = self.kb.upcoming_cms(&today()).first().copied().cloned() {
            let mut plan = false;
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(semibold(format!("🏆 Next Champions Meeting: {}", cm.short_label()), size::MD));
                        ui.label(RichText::new(format!("{} · {} · {}", cm.course_name, cm.conditions, cm.status())).color(p.weak));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        plan = ui.add(crate::ui_util::primary_button(ui, "📋 Plan for it", size::BODY).min_size(vec2(0.0, 32.0))).clicked();
                    });
                });
            });
            if plan {
                self.close_setup();
                self.plan_for_cm(&cm);
            }
        }
        ui.add_space(12.0);
        let third = (ui.available_width() - 24.0) / 3.0;
        let mut go = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for (k, (icon, title, body)) in [
                ("🏇", "Plan a career", "Pick a goal and a trainee; get a deck and skills to buy."),
                ("📚", "Explore your collection", "Everything you own, with effects at your level."),
                ("🏠", "Open Home", "Your stable at a glance and what's coming up."),
            ]
            .into_iter()
            .enumerate()
            {
                if big_tile(ui, third, icon, title, body).clicked() {
                    go = Some(k);
                }
            }
        });
        match go {
            Some(0) => {
                self.close_setup();
                self.go_career(crate::CareerTab::Plan);
            }
            Some(1) => {
                self.close_setup();
                self.tab = crate::Tab::Collection;
            }
            Some(_) => {
                self.close_setup();
                self.tab = crate::Tab::Home;
            }
            None => {}
        }
        ui.add_space(8.0);
        if ui.link("📝 Change what I own").clicked() {
            self.open_setup(Step::Umas);
        }
    }

    /// One line about the roster, e.g. "Your strongest type is Speed: 9 maxed SSRs."
    pub(crate) fn friendly_line(&self) -> String {
        let cards = &self.roster.cards;
        let counts: Vec<(CardType, usize)> =
            CardType::ALL.into_iter().map(|t| (t, cards.iter().filter(|c| c.card_type == t && c.rarity == Rarity::SSR && c.is_mlb()).count())).collect();
        let most = counts.iter().map(|(_, n)| *n).max().unwrap_or(0);
        let top: Vec<&str> = counts.iter().filter(|(_, n)| *n == most).map(|(t, _)| t.label()).collect();
        match top.as_slice() {
            _ if most == 0 && !cards.is_empty() => format!("{} support cards and {} umas are ready to plan with.", cards.len(), self.roster.umas.len()),
            _ if most == 0 => "Add your support cards any time from Collection → ＋ Add cards.".into(),
            [one] => format!("Your strongest type is {one}: {most} maxed SSR{}.", if most == 1 { "" } else { "s" }),
            [first @ .., last] => format!("Your strongest types are {} and {last}: {most} maxed SSRs each.", first.join(", ")),
            [] => String::new(),
        }
    }

    /// Settings → Profile → Start over: confirm, back up, empty, and open setup.
    pub(crate) fn start_over_dialog(&mut self, ctx: &egui::Context) {
        if !self.onboarding.confirm_start_over {
            return;
        }
        let mut go = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::unique("start_over")).show(ctx, |ui| {
            ui.set_width(460.0);
            let p = pal(ui);
            ui.label(semibold("Start over?", 19.0));
            ui.label(format!(
                "This empties the app: {}'s {} umas, {} cards, parents, saved plans, results and notes.",
                self.profile_name(),
                self.roster.umas.len(),
                self.roster.cards.len()
            ));
            ui.label(RichText::new("They're saved to profiles/backups first, so ⚙ Settings → Profile → Load a profile can bring them back.").color(p.weak));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("⚠ Back up and start over").color(Color32::WHITE)).fill(p.bad)).clicked() {
                    go = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if go {
            match self.start_over("") {
                Ok(saved) => {
                    self.onboarding.name.clear();
                    self.open_setup(Step::Welcome);
                    if let Some(s) = saved {
                        self.toast(format!("Saved the old account as profiles/backups/{}.", profile::file_name(&s)), ToastKind::Success);
                    }
                }
                Err(e) => self.toast(e, ToastKind::Warn),
            }
        }
        if go || cancel || modal.should_close() {
            self.onboarding.confirm_start_over = false;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Collage {
    Umas,
    Cards,
}

/// A large clickable tile: icon, title and a line on what it does.
fn big_tile(ui: &mut egui::Ui, width: f32, icon: &str, title: &str, body: &str) -> egui::Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 104.0), Sense::click());
    let hovered = resp.hovered();
    let painter = ui.painter();
    if hovered {
        painter.add(egui::Shadow { offset: [0, 3], blur: 14, spread: 0, color: Color32::from_black_alpha(60) }.as_shape(rect, CornerRadius::same(12)));
    }
    painter.rect_filled(rect, CornerRadius::same(12), if hovered { p.raised } else { p.surface });
    painter.rect_stroke(rect, CornerRadius::same(12), Stroke::new(if hovered { 1.5 } else { 1.0 }, if hovered { p.accent } else { p.border }), egui::StrokeKind::Inside);
    painter.text(rect.left_top() + vec2(16.0, 16.0), Align2::LEFT_TOP, icon, FontId::proportional(24.0), p.accent);
    painter.text(rect.left_top() + vec2(52.0, 18.0), Align2::LEFT_TOP, title, FontId::new(size::MD + 1.0, egui::FontFamily::Name(crate::theme::SEMIBOLD.into())), p.text);
    let galley = painter.layout(body.to_string(), FontId::proportional(13.0), p.weak, width - 68.0);
    painter.galley(Pos2::new(rect.left() + 52.0, rect.top() + 46.0), galley, p.weak);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The app on a scratch copy of the account (blank templates), so tests never touch `account/`.
    pub(crate) fn scratch_app(tag: &str) -> (App, PathBuf) {
        let dir = std::env::temp_dir().join(format!("umalord-setup-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        profile::ensure_account(&dir).unwrap();
        let mut a = App::for_tests();
        a.root = Some(dir.clone());
        a.reload_roster();
        assert!(a.roster.is_empty(), "a scratch account starts empty");
        (a, dir)
    }

    #[test]
    fn steps_follow_each_path() {
        let walk = |via: bool| {
            let mut s = Step::Welcome;
            let mut seen = vec![s];
            while let Some(n) = s.next(via) {
                seen.push(n);
                s = n;
            }
            seen
        };
        assert_eq!(walk(false), [Step::Welcome, Step::You, Step::Umas, Step::Cards, Step::Done]);
        assert_eq!(walk(true), [Step::Welcome, Step::Import, Step::You, Step::Done]);
        assert_eq!(Step::Umas.back(false), Some(Step::You));
        assert_eq!(Step::You.back(true), Some(Step::Import));
        assert_eq!(Step::Welcome.back(false), None);
    }

    /// Next on "Make it yours" writes the name (or Trainer) as `- Profile:`.
    #[test]
    fn the_setup_name_is_saved() {
        let (mut a, dir) = scratch_app("name");
        a.open_setup(Step::You);
        a.onboarding.name = "Sam".into();
        a.setup_next();
        assert_eq!(a.onboarding.step, Some(Step::Umas));
        let text = std::fs::read_to_string(dir.join(crate::data::UMAS_FILE)).unwrap();
        assert!(text.contains("- Profile: Sam"), "{text}");
        a.open_setup(Step::You);
        a.onboarding.name.clear();
        a.setup_next();
        assert_eq!(a.roster.profile.as_deref(), Some("Trainer"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Batched edits write once, and Ctrl+Z puts every row back the way it was.
    #[test]
    fn edits_and_undo() {
        let (mut a, dir) = scratch_app("undo");
        a.apply_edits(&[Edit::Card { id: 30076, lb: 4, level: None }, Edit::Uma { id: 100601, stars: 5, potential: 5 }], true).unwrap();
        assert_eq!(a.owned_card(30076).map(|c| (c.lb, c.level)), Some((4, 50)), "the level is the LB's cap");
        assert_eq!(a.owned_uma(100601).map(|u| (u.stars, u.potential)), Some((5, 5)));
        a.apply_edits(&[Edit::Card { id: 30076, lb: 2, level: None }], true).unwrap();
        assert_eq!(a.owned_card(30076).map(|c| c.lb), Some(2));
        a.undo_setup();
        assert_eq!(a.owned_card(30076).map(|c| (c.lb, c.level)), Some((4, 50)), "back to MLB");
        a.undo_setup();
        assert!(a.owned_card(30076).is_none() && a.owned_uma(100601).is_none(), "the first action added both");
        assert!(a.onboarding.undo.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Pulling a copy adds one LB (level kept) or one star, saved and undoable, and stops at the top.
    #[test]
    fn raise_on_pull() {
        let (mut a, dir) = scratch_app("raise");
        a.apply_edits(&[Edit::Card { id: 30076, lb: 2, level: Some(40) }, Edit::Uma { id: 100601, stars: 4, potential: 2 }], false).unwrap();
        a.raise_card(30076);
        assert_eq!(a.owned_card(30076).map(|c| (c.lb, c.level)), Some((3, 40)), "the level stays");
        a.raise_card(30076);
        a.raise_card(30076);
        assert_eq!(a.owned_card(30076).map(|c| c.lb), Some(4), "MLB is the top");
        a.raise_uma(100601);
        a.raise_uma(100601);
        assert_eq!(a.owned_uma(100601).map(|u| (u.stars, u.potential)), Some((5, 2)), "5 stars is the top; potential stays");
        a.reload_roster();
        assert_eq!(a.owned_card(30076).map(|c| c.lb), Some(4), "saved to the account file");
        a.undo_setup();
        assert_eq!(a.owned_uma(100601).map(|u| u.stars), Some(4));
        a.undo_setup();
        assert_eq!(a.owned_card(30076).map(|c| (c.lb, c.level)), Some((3, 40)));
        let _ = std::fs::remove_dir_all(dir);
    }
}
