//! A saved team plan laid out for someone new to the game: what it's for, its careers in order
//! (one row each, the open one's card beside the list or under its row), and the team before
//! and after. Terms a beginner meets carry a one-line hover.

use crate::App;
use crate::lineage_plans::when;
use crate::parentage::saved::{Built, Career, TeamPlan, Which};
use crate::theme::pal;
use crate::ui_util::semibold;
use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};

/// What a term means, for its hover.
fn meaning(term: &str) -> &'static str {
    match term {
        "own" => "Her own career: a career run to make her a strong racer.",
        "parent" => "Parent career: a career run mainly to make a good parent; what she inherits passes on to the uma she parents.",
        "manual" => "Manual: you play this career yourself; manual careers come out much stronger.",
        "it" => "Independent Training: the game's 50-minute background career; good for parents.",
        "ace" => "The Ace is the race's top runner; she scores 10% more.",
        "finished" => "A finished uma is one whose career you completed; she can race, and be a parent.",
        "round" => "Points one Team Trials round scores.",
        "today" => "Your best finished umas, as they race now: no new careers.",
        "inherit" => "A finished uma passes some of what she has on to the umas she parents; these are her sparks.",
        "front" => "Front Runner: leads from the start and tries to stay in front.",
        "pace" => "Pace Chaser: runs just behind the leaders, then takes over late.",
        "late" => "Late Surger: waits mid-pack and moves up late in the race.",
        "end" => "End Closer: stays at the back and sprints past everyone at the end.",
        "mlb" => "MLB: max limit break (LB4), a support card at full strength. Merging copies of a card raises it; a friend's card you borrow is often MLB.",
        "pts" => "Points one Team Trials round scores.",
        "lengths" => "How much farther ahead she finishes, on average, in the simulated races.",
        "careers" => "Careers fewer to reach the goal, usually.",
        "stats" => "Her five stats added up at the end of a simulated career.",
        "vet" => "The number is the game's id for one finished uma (its Veteran List entry), so you can tell two of the same uma apart.",
        _ => "",
    }
}

/// The running styles a text names, as the hover that explains them (one line each).
pub(crate) fn style_hint(text: &str) -> Option<String> {
    let lines: Vec<&str> = [("Front Runner", "front"), ("Pace Chaser", "pace"), ("Late Surger", "late"), ("End Closer", "end")].iter().filter(|(name, _)| text.contains(name)).map(|(_, k)| meaning(k)).collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// What a term means, for a hover outside this file ("mlb", "vet").
pub(crate) fn explain(term: &str) -> &'static str {
    meaning(term)
}

/// A piece of rich text: its words, weight, colour, and the term it explains, if any.
pub(crate) struct Span {
    text: String,
    bold: bool,
    color: Option<Color32>,
    term: Option<&'static str>,
}

impl Span {
    pub(crate) fn t(s: &str) -> Span {
        Span { text: s.into(), bold: false, color: None, term: None }
    }
    pub(crate) fn b(s: &str) -> Span {
        Span { text: s.into(), bold: true, color: None, term: None }
    }
    /// A term: it never breaks across lines, so its underline stays under it.
    pub(crate) fn term(s: &str, term: &'static str) -> Span {
        Span { text: s.replace(' ', "\u{a0}"), bold: false, color: None, term: Some(term) }
    }
    fn colored(mut self, c: Color32) -> Span {
        self.color = Some(c);
        self
    }
}

/// A wrapped paragraph whose terms are underlined and explain themselves on hover: egui can't
/// hover one span of a wrapped label, so the paragraph is one galley and the pointer is mapped to
/// the character under it.
pub(crate) fn rich(ui: &mut egui::Ui, spans: &[Span], size: f32, max_w: f32) {
    let p = pal(ui);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width().min(max_w);
    let mut terms = Vec::new();
    let mut at = 0usize;
    for s in spans {
        let font = if s.bold { FontId::new(size, egui::FontFamily::Name(crate::theme::SEMIBOLD.into())) } else { FontId::proportional(size) };
        // Bold words also take the strong text colour, so they stand out whatever the font's weights.
        let ink = s.color.unwrap_or(if s.bold { ui.visuals().strong_text_color() } else { p.text });
        let fmt = egui::TextFormat { font_id: font, color: ink, line_height: Some(size * 1.5), ..Default::default() };
        job.append(&s.text, 0.0, fmt);
        let n = s.text.chars().count();
        if let Some(t) = s.term {
            terms.push((at, at + n, t));
        }
        at += n;
    }
    let galley = ui.painter().layout_job(job);
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(rect.min, galley.clone(), p.text);
    // Each term gets a dotted underline under its own characters, row by row (egui's underline
    // lands on the wrong row once the line height is raised).
    for (a, b, _) in &terms {
        let mut runs: Vec<(f32, f32, f32)> = Vec::new();
        for i in *a..*b {
            let r = galley.pos_from_cursor(egui::text::CCursor { index: egui::text::CharIndex(i), prefer_next_row: true });
            let next = galley.pos_from_cursor(egui::text::CCursor::new(i + 1));
            let right = if (next.bottom() - r.bottom()).abs() < 1.0 { next.left() } else { r.left() + size * 0.5 };
            match runs.last_mut() {
                Some(run) if (run.2 - r.bottom()).abs() < 1.0 => run.1 = right,
                _ => runs.push((r.left(), right, r.bottom())),
            }
        }
        for (x0, x1, y) in runs {
            let y = rect.top() + y - size * 0.18;
            let mut x = rect.left() + x0;
            while x < rect.left() + x1 {
                ui.painter().line_segment([pos2(x, y), pos2((x + 2.0).min(rect.left() + x1), y)], Stroke::new(1.0, p.weak));
                x += 4.0;
            }
        }
    }
    if let Some(pos) = resp.hover_pos() {
        let i = galley.cursor_from_pos(pos - rect.min).index.0;
        if let Some((_, _, t)) = terms.iter().find(|(a, b, _)| i >= *a && i < *b) {
            resp.on_hover_text(meaning(t));
        }
    }
}

/// "Her own career" or "Parent career", as a chip that explains itself.
pub(crate) fn kind_chip(ui: &mut egui::Ui, own: bool) -> egui::Response {
    let p = pal(ui);
    let (text, fill, ink, stroke) = if own { ("Her own career", p.raised, p.text, p.weak) } else { ("Parent career", p.accent.gamma_multiply(0.2), p.accent, p.accent) };
    crate::ui_util::pill_padded(ui, RichText::new(text).size(11.5).color(ink), fill, Stroke::new(1.0, stroke.gamma_multiply(0.7)), vec2(8.0, 3.0)).on_hover_text(meaning(if own { "own" } else { "parent" }))
}

fn ace_chip(ui: &mut egui::Ui) -> egui::Response {
    let p = pal(ui);
    crate::ui_util::pill_padded(ui, RichText::new("★ Ace").size(11.5).color(p.gold), p.gold.gamma_multiply(0.15), Stroke::NONE, vec2(8.0, 3.0)).on_hover_text(meaning("ace"))
}

/// The header of an open step's card.
pub(crate) struct StepHead {
    pub line: String,
    pub context: String,
    pub own: bool,
    pub next: bool,
}

/// A number tile: the big line, its size and colour, and the line under it (with a hover).
struct Tile {
    big: String,
    size: f32,
    good: bool,
    small: String,
    hover: Option<&'static str>,
}

impl Tile {
    fn new(big: String, small: &str) -> Tile {
        Tile { big, size: 22.0, good: false, small: small.into(), hover: None }
    }
    fn size(mut self, s: f32) -> Tile {
        self.size = s;
        self
    }
    fn good(mut self) -> Tile {
        self.good = true;
        self
    }
    fn hover(mut self, h: &'static str) -> Tile {
        self.hover = Some(h);
        self
    }
}

/// Tiles side by side, sharing `width`.
fn tile_row(ui: &mut egui::Ui, tiles: &[Tile], width: f32) {
    let p = pal(ui);
    let n = tiles.len().max(1) as f32;
    let each = (width - 8.0 * (n - 1.0)) / n;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for t in tiles {
            ui.allocate_ui_with_layout(vec2(each, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(each);
                egui::Frame::new().fill(p.raised).corner_radius(10).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(semibold(&t.big, t.size).color(if t.good { p.good } else { ui.visuals().strong_text_color() })).wrap());
                    let r = ui.add(egui::Label::new(RichText::new(&t.small).size(12.5).color(p.weak)).wrap());
                    if let Some(h) = t.hover {
                        r.on_hover_text(h);
                    }
                });
            });
        }
    });
}

/// "+5,765" with a real minus.
fn signed(v: f64) -> String {
    format!("{}{}", if v < 0.0 { "−" } else { "+" }, crate::lineage_kit::thousands(v.abs()))
}

impl App {
    /// The open step: the one the player chose, else the next.
    fn open_step(&self, which: Which, plan: &TeamPlan) -> Option<usize> {
        let chosen = self.lineage.p.open_step.as_ref().filter(|(w, _)| *w == which).and_then(|(_, k)| plan.careers.iter().position(|c| &c.key == k));
        chosen.or_else(|| plan.careers.iter().position(|c| c.done.is_none()))
    }

    fn step_head(&self, plan: &TeamPlan, i: usize) -> StepHead {
        let c = &plan.careers[i];
        let name = crate::parentage::short_name(&self.kb, c.brief.trainee);
        let kind = if c.own { format!("{name}'s own career") } else { format!("{name}'s parent career") };
        let place = match (&c.race, c.brief.for_runner, c.own) {
            (Some(r), _, _) => r.clone(),
            (None, Some(r), false) => format!("for {}", crate::parentage::short_name(&self.kb, r)),
            _ => "for the team".into(),
        };
        let context = match &plan.built {
            Built::Cm { team, .. } => format!("CM {} {} · {}", team.event.id, team.event.name, team.event.course_name),
            Built::Tt { .. } => match c.brief.replaces {
                Some(old) => format!("{place} · takes the place the plan gave {}", crate::parentage::short_name(&self.kb, old)),
                None => format!("Team Trials · {place}"),
            },
        };
        StepHead { line: format!("Step {} of {} · {kind} · {place}", i + 1, plan.careers.len()), context, own: c.own, next: plan.next().is_some_and(|n| n.key == c.key) }
    }

    /// The open step's card, for the action pane.
    pub(crate) fn open_step_card(&mut self, ui: &mut egui::Ui, which: Which, plan: &TeamPlan) {
        match self.open_step(which, plan) {
            Some(i) => {
                let head = self.step_head(plan, i);
                let b = plan.careers[i].brief.clone();
                self.career_card(ui, &b, Some(&head));
            }
            None => {
                crate::ui_util::card(ui, |ui| {
                    ui.label(semibold("Every career in this plan is done", crate::theme::size::MD));
                    crate::lineage_page::prose(ui, "Re-plan to see whether another career would still raise the team, or plan for the next event.", 13.0, pal(ui).weak);
                });
            }
        }
    }

    /// "What this plan is for": the event and the plan in a sentence on the left, its numbers
    /// as three tiles on the right (under the sentence when narrow).
    pub(crate) fn plan_summary(&mut self, ui: &mut egui::Ui, plan: &TeamPlan) {
        let p = pal(ui);
        let n = plan.careers.len();
        let careers = format!("{n} career{}", if n == 1 { "" } else { "s" });
        let round = |x: f64| crate::lineage_kit::thousands((x / 1000.0).round() * 1000.0);
        let (mut spans, tiles): (Vec<Span>, Vec<Tile>) = match &plan.built {
            Built::Tt { team, .. } => {
                let spans = vec![
                    Span::b("Team Trials"),
                    Span::t(" is a weekly event: you enter 15 runners, three in each of five races (Sprint, Mile, Medium, Long, Dirt), and score points by how they place and the skills they show. This plan is "),
                    Span::b(&careers),
                    Span::t("; run them in order to raise what your team scores."),
                ];
                let tiles = if team.today_total > 0.0 {
                    vec![
                        Tile::new(round(team.today_total), "pts a round · today").hover(meaning("today")),
                        Tile::new(round(team.today_total + team.gain), "after the plan"),
                        Tile::new(signed((team.gain / 1000.0).round() * 1000.0), "pts a round").good(),
                    ]
                } else {
                    vec![Tile::new(careers.clone(), "to run"), Tile::new("—".into(), "no team today to compare with").hover(meaning("finished"))]
                };
                (spans, tiles)
            }
            Built::Cm { team, .. } => {
                let e = &team.event;
                let parents = plan.careers.iter().filter(|c| !c.own).count();
                let mut spans = vec![
                    Span::b(&format!("Champions Meeting {}, the {}", e.id, e.name)),
                    Span::t(&format!(", is raced on {}. You enter three runners. This plan is ", e.course_name)),
                    Span::b(&careers),
                    Span::t(": "),
                ];
                if parents > 0 {
                    let mut fors: Vec<u32> = plan.careers.iter().filter(|c| !c.own).filter_map(|c| c.brief.for_runner).collect();
                    fors.dedup();
                    let names = fors.iter().map(|r| crate::parentage::short_name(&self.kb, *r)).collect::<Vec<_>>().join(" and ");
                    spans.extend([
                        Span::t(&format!("{} ", if parents == 1 { "one".to_string() } else { parents.to_string() })),
                        Span::term(if parents == 1 { "parent career" } else { "parent careers" }, "parent"),
                        Span::t(&format!(" to give {names} the right start, then each runner's ")),
                    ]);
                } else {
                    spans.push(Span::t("each runner's "));
                }
                spans.extend([Span::term("own career", "own"), Span::t(".")]);
                let shares: Vec<f64> = team.runners.iter().filter_map(|r| r.vs_today).collect();
                let days = crate::tab_planner::days_apart(&e.date, &crate::ui_util::today()).max(0);
                let mut tiles = Vec::new();
                if let (Some(t), Some(lo), Some(hi)) = (team.today.first(), shares.iter().copied().reduce(f64::min), shares.iter().copied().reduce(f64::max)) {
                    tiles.push(Tile::new(format!("#{} {}", t.vet, crate::parentage::short_name(&self.kb, t.outfit)), "your best today").size(18.0).hover("Your best finished uma for this course, as she races now."));
                    let range = if (hi - lo) < 0.005 { format!("{:.0}%", hi * 100.0) } else { format!("{:.0}–{:.0}%", lo * 100.0, hi * 100.0) };
                    tiles.push(Tile::new(range, "each planned runner beats her"));
                }
                tiles.push(Tile::new(when(&e.date), &format!("Round 1 · in {days} days")));
                (spans, tiles)
            }
        };
        if plan.basis.vets.is_empty() {
            spans.push(Span::t(" "));
            spans.push(Span::t("It's planned for a fresh account: no finished umas were imported.").colored(p.warn));
        }
        self.summary_card(ui, &spans, &tiles);
    }

    /// One runner's summary: her goal and steps in a sentence, the step, careers to go and her
    /// start as tiles.
    pub(crate) fn runner_summary(&mut self, ui: &mut egui::Ui, plan: &crate::parentage::track::ParentPlan) {
        let name = crate::parentage::short_name(&self.kb, plan.goal.runner);
        let targets: Vec<&str> = plan.goal.targets.iter().map(|t| crate::parentage::APTITUDES[*t]).collect();
        let aims = match targets.len() {
            0 => String::new(),
            1 => targets[0].to_string(),
            _ => format!("{} and {}", targets[..targets.len() - 1].join(", "), targets[targets.len() - 1]),
        };
        let total = plan.steps.len() + 1;
        let mut spans = vec![Span::b("One runner: "), Span::t(&format!("{name}, aimed at {aims}. This plan is ")), Span::b(&format!("{total} step{}", if total == 1 { "" } else { "s" })), Span::t(": ")];
        for (i, s) in plan.steps.iter().enumerate() {
            spans.extend([Span::t(if i == 0 { "a " } else { ", then a " }), Span::term("parent career", "parent"), Span::t(&format!(" of {}", crate::parentage::short_name(&self.kb, s.run.trainee)))]);
        }
        spans.extend([Span::t(if plan.steps.is_empty() { "her " } else { ", then her " }), Span::term("own career", "own"), Span::t(".")]);
        let current = plan.current();
        let left: u32 = plan.steps.iter().skip(current.unwrap_or(plan.steps.len())).map(|s| crate::parentage::verify::step_spread(s.run.p).0.min(50)).sum::<u32>() + 1;
        let tiles = vec![
            Tile::new(format!("step {} of {total}", current.map_or(total, |i| i + 1)), &current.map_or(format!("next: {name}'s own career"), |i| format!("next: {}'s parent career", crate::parentage::short_name(&self.kb, plan.steps[i].run.trainee)))).size(20.0),
            Tile::new(format!("usually {left} career{}", if left == 1 { "" } else { "s" }), "to go").size(20.0),
            Tile::new("A".into(), &format!("she starts at A in {aims}")),
        ];
        self.summary_card(ui, &spans, &tiles);
    }

    /// The summary card: the label and sentence, with the tiles beside it from 900 px of card
    /// width, else under it.
    fn summary_card(&mut self, ui: &mut egui::Ui, spans: &[Span], tiles: &[Tile]) {
        let p = pal(ui);
        crate::ui_util::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let w = ui.available_width();
            let block = (157.0 * tiles.len() as f32).min(470.0).min(w);
            let text = |ui: &mut egui::Ui, width: f32| {
                ui.label(RichText::new("WHAT THIS PLAN IS FOR").size(12.0).strong().color(p.weak));
                ui.add_space(6.0);
                rich(ui, spans, 15.0, width.min(crate::lineage_page::PROSE_W + 80.0));
            };
            if w >= 900.0 {
                ui.horizontal_top(|ui| {
                    let tw = w - block - 24.0;
                    ui.allocate_ui_with_layout(vec2(tw, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(tw);
                        text(ui, tw);
                    });
                    ui.add_space(8.0);
                    tile_row(ui, tiles, block);
                });
            } else {
                text(ui, w);
                ui.add_space(10.0);
                tile_row(ui, tiles, w);
            }
        });
    }

    /// The careers in order, one row each; "Show how" opens a row's card (in the action pane
    /// when `wide`, else under the row).
    pub(crate) fn plan_steps(&mut self, ui: &mut egui::Ui, which: Which, plan: &TeamPlan, wide: bool) {
        let p = pal(ui);
        let open = self.open_step(which, plan);
        let next = plan.careers.iter().position(|c| c.done.is_none());
        let (done, total) = plan.progress();
        // Careers to go: a parent step's typical count, one for a runner's own.
        let to_go: u32 = plan.careers.iter().filter(|c| c.done.is_none()).map(|c| if c.own { 1 } else { c.brief.p.map_or(1, |p| crate::parentage::verify::step_spread(p).0.min(50)) }).sum();
        let mut pick: Option<Option<String>> = None;
        let mut head_rect = Rect::NOTHING;
        crate::ui_util::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let r = ui.horizontal_wrapped(|ui| {
                ui.label(semibold("The steps, in order", 17.0));
                ui.add_space(8.0);
                ui.label(RichText::new(format!("{done} of {total} done")).size(13.5).strong());
                ui.label(RichText::new(format!("· about {to_go} career{} to go", if to_go == 1 { "" } else { "s" })).size(13.5).color(p.weak));
                ui.label(RichText::new(if wide { "· Show how opens a step's details on the left" } else { "· Show how opens a step's details under it" }).size(12.5).color(p.weak));
            });
            head_rect = r.response.rect;
            ui.add_space(6.0);
            // One segment per step.
            let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
            let gap = 3.0;
            let seg = ((bar.width() - gap * (total.max(1) - 1) as f32) / total.max(1) as f32).max(1.0);
            for (i, c) in plan.careers.iter().enumerate() {
                let r = Rect::from_min_size(pos2(bar.left() + i as f32 * (seg + gap), bar.top()), vec2(seg, 6.0));
                let fill = if c.done.is_some() { p.accent } else { p.raised };
                ui.painter().rect_filled(r, 3.0, fill);
                if Some(i) == next {
                    ui.painter().rect_stroke(r, 3.0, Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
                }
            }
            ui.add_space(8.0);
            let full = ui.available_width() >= 1000.0;
            let cols: [f32; 4] = if full { [28.0, 230.0, 150.0, 112.0] } else { [28.0, 190.0, 124.0, 84.0] };
            let mut centers: Vec<(egui::Pos2, bool)> = Vec::new();
            for (i, c) in plan.careers.iter().enumerate() {
                let is_next = Some(i) == next;
                let is_open = Some(i) == open;
                let fill = if is_next || (is_open && !wide) { p.accent.gamma_multiply(0.12) } else { Color32::TRANSPARENT };
                let stroke = if is_open && wide { Stroke::new(1.5, p.accent) } else { Stroke::NONE };
                ui.add_space(2.0);
                egui::Frame::new().fill(fill).stroke(stroke).corner_radius(8).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        // Col 1: the marker.
                        let (m, _) = ui.allocate_exact_size(vec2(cols[0], 28.0), Sense::hover());
                        let center = m.center();
                        centers.push((center, c.done.is_some()));
                        let painter = ui.painter();
                        if c.done.is_some() {
                            painter.circle_filled(center, 13.0, p.accent);
                            painter.text(center, Align2::CENTER_CENTER, "✔", FontId::proportional(13.0), p.on_accent);
                        } else if is_next {
                            painter.circle(center, 12.0, p.surface, Stroke::new(2.0, p.accent));
                            painter.text(center, Align2::CENTER_CENTER, (i + 1).to_string(), FontId::new(13.0, egui::FontFamily::Name(crate::theme::SEMIBOLD.into())), p.accent);
                        } else {
                            painter.circle(center, 12.0, p.surface, Stroke::new(1.5, p.weak.gamma_multiply(0.6)));
                            painter.text(center, Align2::CENTER_CENTER, (i + 1).to_string(), FontId::proportional(13.0), p.weak);
                        }
                        // In a narrow pane the race moves under the reason; the reason stops at about
                        // 75 characters, with the race and gain right after it, not at the far edge.
                        let compact = ui.available_width() < 840.0;
                        let others = if compact { cols[1] + cols[3] + 2.0 * ui.spacing().item_spacing.x } else { cols[1] + cols[2] + cols[3] + 3.0 * ui.spacing().item_spacing.x };
                        let flex = (ui.available_width() - others).clamp(160.0, 600.0);
                        // Col 2: who, her status and the kind of career.
                        ui.allocate_ui_with_layout(vec2(cols[1], 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(cols[1]);
                            ui.horizontal_top(|ui| {
                                self.avatar(ui, c.brief.trainee, 32.0);
                                ui.vertical(|ui| {
                                    ui.add(egui::Label::new(semibold(crate::parentage::short_name(&self.kb, c.brief.trainee), 14.5)).wrap());
                                    let (text, color) = if c.done.is_some() {
                                        ("✔ Done", p.accent)
                                    } else if is_next {
                                        ("Next", p.accent)
                                    } else {
                                        ("To come", p.weak)
                                    };
                                    if is_next {
                                        crate::ui_util::pill_padded(ui, RichText::new(text).size(11.5).color(p.on_accent), p.accent, Stroke::NONE, vec2(8.0, 2.0));
                                    } else {
                                        ui.label(RichText::new(text).size(12.0).strong().color(color));
                                    }
                                });
                            });
                            ui.horizontal_wrapped(|ui| {
                                kind_chip(ui, c.own);
                                if self.career_is_ace(plan, c) {
                                    ace_chip(ui);
                                }
                            });
                        });
                        // Col 3: what it's for, and how long or when it was done.
                        ui.allocate_ui_with_layout(vec2(flex, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(flex);
                            let why = if c.what_for.is_empty() { c.label.clone() } else { c.what_for.clone() };
                            let r = ui.add(egui::Label::new(RichText::new(&why).size(13.5)).wrap());
                            if let Some(h) = style_hint(&why) {
                                r.on_hover_text(h);
                            }
                            match &c.done {
                                Some(d) => {
                                    let mut spans = vec![Span::t(&format!("Marked done {} ", when(&d.at)))];
                                    if let Some(v) = d.vet {
                                        spans.extend([Span::t("with "), Span::term("finished uma", "finished"), Span::t(" "), Span::b(&format!("#{v} {}", crate::parentage::short_name(&self.kb, c.brief.trainee)))]);
                                    }
                                    rich(ui, &spans, 12.5, flex);
                                }
                                None => {
                                    let usual = if c.usually.is_empty() { crate::parentage::saved::usually(if c.own { None } else { c.brief.p }) } else { c.usually.clone() };
                                    let how = if c.brief.mode == "Manual career" { Span::term("manual", "manual") } else { Span::term("Independent Training", "it") };
                                    rich(ui, &[Span::t(&format!("{} · ", crate::lineage_page::capitalize(&usual))).colored(p.weak), how.colored(p.weak)], 12.5, flex);
                                }
                            }
                            if compact {
                                self.step_helps(ui, plan, c);
                            }
                        });
                        // Col 4: what it helps.
                        if !compact {
                            ui.allocate_ui_with_layout(vec2(cols[2], 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.set_width(cols[2]);
                                self.step_helps(ui, plan, c);
                            });
                        }
                        // Col 5: Show how.
                        ui.allocate_ui_with_layout(vec2(cols[3], 0.0), egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            ui.set_width(cols[3]);
                            if is_open && wide {
                                ui.label(RichText::new("← Shown at left").size(13.0).color(p.weak));
                            } else if is_open {
                                if ui.link(RichText::new("Hide ▲").size(13.0)).clicked() {
                                    pick = Some(None);
                                }
                            } else if ui.link(semibold("Show how ›", 13.0)).clicked() {
                                pick = Some(Some(c.key.clone()));
                            }
                        });
                    });
                    // In one column the open step's card sits under its row.
                    if is_open && !wide && self.lineage.p.steps_folded != Some(which) {
                        ui.add_space(8.0);
                        if ui.link(RichText::new("↑ Back to the steps").size(13.5)).clicked() {
                            ui.scroll_to_rect(head_rect, Some(egui::Align::TOP));
                        }
                        ui.add_space(6.0);
                        let head = self.step_head(plan, i);
                        let b = c.brief.clone();
                        // Inside a row the card must wrap to the row, not widen it.
                        ui.scope(|ui| {
                            ui.set_max_width(ui.available_width());
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                            self.career_card(ui, &b, Some(&head));
                        });
                    }
                });
            }
            // The line joining the markers: accent beside done steps.
            for w in centers.windows(2) {
                let (a, b) = (w[0].0 + vec2(0.0, 14.0), w[1].0 - vec2(0.0, 14.0));
                if b.y > a.y {
                    ui.painter().line_segment([a, b], Stroke::new(2.0, if w[0].1 { p.accent } else { p.weak.gamma_multiply(0.4) }));
                }
            }
        });
        match pick {
            Some(Some(k)) => {
                self.lineage.p.open_step = Some((which, k));
                self.lineage.p.steps_folded = None;
            }
            Some(None) => self.lineage.p.steps_folded = Some(which),
            None => {}
        }
    }

    /// Whether a career's runner is her race's Ace (Team Trials) or the team's ace (CM).
    fn career_is_ace(&self, plan: &TeamPlan, c: &Career) -> bool {
        if !c.own {
            return false;
        }
        match &plan.built {
            Built::Tt { team, .. } => team.rows.iter().any(|r| Some(r.name) == c.race.as_deref() && r.slots.first().is_some_and(|s| s.trainee == c.brief.trainee)),
            Built::Cm { team, .. } => team.runners.iter().max_by(|a, b| a.sim.map_or(0.0, |s| s.0).total_cmp(&b.sim.map_or(0.0, |s| s.0))).is_some_and(|r| r.trainee == c.brief.trainee),
        }
    }

    /// A step's fourth column: the race and its gain, or the runner it's for.
    fn step_helps(&self, ui: &mut egui::Ui, plan: &TeamPlan, c: &Career) {
        let p = pal(ui);
        match &plan.built {
            Built::Tt { team, .. } => {
                if let Some(race) = &c.race {
                    ui.label(semibold(race, 13.0));
                    if let Some(g) = team.rows.iter().find(|r| r.name == race.as_str()).and_then(|r| r.gain) {
                        ui.label(RichText::new(format!("part of {}", signed(g))).size(12.5).color(p.weak));
                    }
                }
            }
            Built::Cm { team, .. } => {
                if c.own {
                    ui.label(semibold(crate::parentage::short_name(&self.kb, c.brief.trainee), 13.0));
                    if let (Some(v), Some(t)) = (team.runners.iter().find(|r| r.trainee == c.brief.trainee).and_then(|r| r.vs_today), team.today.first()) {
                        ui.label(RichText::new(format!("beats #{} in {:.0}%", t.vet, v * 100.0)).size(12.5).color(p.weak)).on_hover_text(format!("#{} {} is your best finished uma for this course as she races now. {}", t.vet, crate::parentage::short_name(&self.kb, t.outfit), meaning("vet")));
                    }
                } else if let Some(r) = c.brief.for_runner {
                    ui.horizontal(|ui| {
                        self.avatar(ui, r, 18.0);
                        ui.label(RichText::new(format!("for {}", crate::parentage::short_name(&self.kb, r))).size(12.5));
                    });
                    if !c.brief.must.is_empty() {
                        let must = c.brief.must.iter().map(|w| w.label(&self.kb)).collect::<Vec<_>>().join(", ");
                        ui.add(egui::Label::new(RichText::new(format!("her parent: {must}")).size(12.0).color(p.weak)).wrap());
                    }
                }
            }
        }
    }

    /// "Your team, before and after": per race (Team Trials) or for the CM, the runners racing
    /// now and the runners after the plan.
    pub(crate) fn before_after(&mut self, ui: &mut egui::Ui, plan: &TeamPlan) {
        let p = pal(ui);
        crate::ui_util::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semibold("Your team, before and after", 17.0));
            let legend = match &plan.built {
                Built::Tt { .. } => "Points are per Team Trials round, estimated by racing your umas against each other in the simulator; the real races have twelve runners.".to_string(),
                Built::Cm { team, .. } => match team.today.first() {
                    Some(t) => format!("Win rates come from simulated races on the CM {} course, each runner against your best runner today, #{} {}.", team.event.id, t.vet, crate::parentage::short_name(&self.kb, t.outfit)),
                    None => "None of your finished umas can run this course yet, so there's no team today to compare with.".into(),
                },
            };
            crate::lineage_page::prose(ui, &legend, 12.5, p.weak);
            ui.add_space(6.0);
            let wide = ui.available_width() >= 760.0;
            match &plan.built {
                Built::Tt { team, lengths, .. } => {
                    for (k, row) in team.rows.iter().enumerate() {
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(semibold(row.name, 15.0));
                            ui.label(RichText::new(lengths.get(k).cloned().unwrap_or_default()).size(12.5).color(p.weak));
                            if let Some(g) = row.gain {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                    ui.label(semibold(format!("{} pts a round", signed(g)), 15.0).color(if g >= 0.0 { p.good } else { p.weak }));
                                });
                            }
                        });
                        let before: Vec<(u32, String, String)> =
                            row.today.iter().map(|(t, pts)| (t.outfit, format!("{} pts", crate::lineage_kit::thousands(*pts)), format!("#{} · {} · {} pts a round", t.vet, crate::parentage::uma_name(&self.kb, t.outfit), crate::lineage_kit::thousands(*pts)))).collect();
                        // After: the plan's runners; a replacement takes her slot's place.
                        let after: Vec<(u32, bool, bool, Option<String>)> = row
                            .slots
                            .iter()
                            .map(|s| {
                                let swap = plan.careers.iter().find(|c| c.race.as_deref() == Some(row.name) && c.brief.replaces == Some(s.trainee));
                                let same = row.today.iter().any(|t| t.0.outfit == s.trainee);
                                let chara = |u: u32| crate::affinity::chara(u);
                                let who = swap.map_or(s.trainee, |c| c.brief.trainee);
                                let here = row.today.iter().any(|t| chara(t.0.outfit) == chara(who));
                                let elsewhere = team.rows.iter().find(|r| r.name != row.name && r.today.iter().any(|t| chara(t.0.outfit) == chara(who))).map(|r| r.name);
                                let note = match (swap, same, here, elsewhere) {
                                    (Some(_), _, true, _) => Some(format!("A new career of her, in the place the plan first gave {}, whose unique skill fires unreliably.", crate::parentage::short_name(&self.kb, s.trainee))),
                                    (Some(_), _, _, _) => Some(format!("Takes the place the plan first gave {}, whose unique skill fires unreliably.", crate::parentage::short_name(&self.kb, s.trainee))),
                                    (None, false, true, _) => Some("Racing now too: a new career, built for this race, scores more.".to_string()),
                                    (None, false, _, Some(r)) => Some(format!("Moves over from {r}, with a new career built for {}.", row.name)),
                                    _ => None,
                                };
                                (who, s.ace, same && swap.is_none(), note)
                            })
                            .collect();
                        self.before_after_row(ui, wide, &before, &after, &[]);
                    }
                }
                Built::Cm { team, .. } => {
                    ui.separator();
                    let before: Vec<(u32, String, String)> = team.today.iter().map(|t| (t.outfit, String::new(), format!("#{} · {}", t.vet, crate::parentage::uma_name(&self.kb, t.outfit)))).collect();
                    let ace = team.runners.iter().max_by(|a, b| a.sim.map_or(0.0, |s| s.0).total_cmp(&b.sim.map_or(0.0, |s| s.0))).map(|r| r.trainee);
                    let after: Vec<(u32, bool, bool, Option<String>)> = team
                        .runners
                        .iter()
                        .map(|r| {
                            let again = team.today.iter().any(|t| crate::affinity::chara(t.outfit) == crate::affinity::chara(r.trainee));
                            (r.trainee, Some(r.trainee) == ace, false, again.then(|| "Racing now too: a new career, built for this course, beats her.".to_string()))
                        })
                        .collect();
                    let best = team.today.first().map(|t| t.vet);
                    let results: Vec<String> = team
                        .runners
                        .iter()
                        .map(|r| match (r.vs_today, best) {
                            (Some(v), Some(id)) => format!("beats #{id} in {:.0}%", v * 100.0),
                            _ => String::new(),
                        })
                        .collect();
                    self.before_after_row(ui, wide, &before, &after, &results);
                }
            }
        });
    }

    /// One race's before (avatar, name, points; the id on hover) and after (avatar, name, Ace,
    /// New career or Same as now, a replacement's reason, a CM result).
    fn before_after_row(&self, ui: &mut egui::Ui, wide: bool, before: &[(u32, String, String)], after: &[(u32, bool, bool, Option<String>)], results: &[String]) {
        let p = pal(ui);
        let before_box = |this: &Self, ui: &mut egui::Ui| {
            egui::Frame::new().fill(p.raised).corner_radius(9).inner_margin(egui::Margin::symmetric(11, 9)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                rich(ui, &[Span::t("Racing now: your best ").colored(p.weak), Span::term("finished umas", "finished").colored(p.weak)], 12.5, 600.0);
                if before.is_empty() {
                    ui.label(RichText::new("none of yours can run it yet").size(13.0).color(p.weak));
                }
                for (uma, pts, hover) in before {
                    ui.horizontal(|ui| {
                        this.avatar(ui, *uma, 24.0);
                        ui.label(RichText::new(crate::parentage::short_name(&this.kb, *uma)).size(13.5)).on_hover_text(hover);
                        if !pts.is_empty() {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                ui.label(RichText::new(pts).size(13.0).color(p.weak)).on_hover_text(hover);
                            });
                        }
                    });
                }
            });
        };
        let after_box = |this: &Self, ui: &mut egui::Ui| {
            egui::Frame::new().stroke(Stroke::new(1.0, p.weak.gamma_multiply(0.5))).corner_radius(9).inner_margin(egui::Margin::symmetric(11, 9)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("After the plan").size(12.5).color(p.weak));
                for (k, (uma, ace, same, why)) in after.iter().enumerate() {
                    ui.horizontal(|ui| {
                        this.avatar(ui, *uma, 24.0);
                        ui.label(semibold(crate::parentage::short_name(&this.kb, *uma), 13.5));
                        if *ace {
                            ace_chip(ui);
                        }
                        let (tag, fill, ink) = if *same { ("Same as now", p.raised, p.weak) } else { ("New career", p.accent.gamma_multiply(0.2), p.accent) };
                        crate::ui_util::pill_padded(ui, RichText::new(tag).size(11.5).color(ink), fill, Stroke::NONE, vec2(8.0, 2.0));
                        if let Some(r) = results.get(k).filter(|r| !r.is_empty()) {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                ui.label(RichText::new(r).size(12.5).color(p.weak)).on_hover_text(meaning("vet"));
                            });
                        }
                    });
                    if let Some(w) = why {
                        ui.horizontal(|ui| {
                            ui.add_space(30.0);
                            ui.add(egui::Label::new(RichText::new(w).size(12.5).color(p.weak)).wrap());
                        });
                    }
                }
            });
        };
        if wide {
            let w = ui.available_width();
            let inner = w - 16.0 - 2.0 * ui.spacing().item_spacing.x - 2.0;
            let (bw, aw) = (inner * 0.44, inner * 0.56);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(bw, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(bw);
                    before_box(self, ui);
                });
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 40.0), Sense::hover());
                ui.painter().text(r.center(), Align2::CENTER_CENTER, "→", FontId::proportional(18.0), p.weak);
                ui.allocate_ui_with_layout(vec2(aw, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(aw);
                    after_box(self, ui);
                });
            });
        } else {
            before_box(self, ui);
            ui.add_space(6.0);
            after_box(self, ui);
        }
        ui.add_space(6.0);
    }
}

impl App {
    /// Open Collection's Add cards with this card picked (for after the player pulls it).
    fn add_card_dialog(&mut self, id: u32) {
        self.tab = crate::Tab::Collection;
        self.collection_tab = crate::CollectionTab::Cards;
        let mut a = std::mem::take(&mut self.cards.add);
        a.open = true;
        self.pick_add_card(&mut a, id);
        self.cards.add = a;
    }

    /// "How to raise the ceiling": the plan's measured changes, strongest first, five at a time.
    /// `working`: the measurements are still running (One runner's worker); `None`: a plan made
    /// before they existed, with a button to measure (returns whether it was pressed). A fresh
    /// account's card starts with importing finished umas.
    pub(crate) fn ceiling_card(&mut self, ui: &mut egui::Ui, key: &str, levers: Option<&[crate::parentage::ceiling::Lever]>, tried: &crate::parentage::ceiling::Tried, fresh: bool, working: bool) -> bool {
        let p = pal(ui);
        let all = self.lineage.p.ceiling_all.as_deref() == Some(key);
        let mut toggle = false;
        let mut measure = false;
        let mut import = false;
        crate::ui_util::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semibold("How to raise the ceiling", 17.0));
            crate::lineage_page::prose(ui, "What would raise this plan most, measured while it was made. Each one is a separate change; their gains don't simply add up.", 12.5, p.weak);
            ui.add_space(6.0);
            if working {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Trying the cards and parents you don't have yet…").size(13.0).color(p.weak));
                });
                return;
            }
            let wide = ui.available_width() >= 900.0;
            let cols: [f32; 3] = if wide { [112.0, 52.0, 196.0] } else { [92.0, 48.0, 0.0] };
            if fresh {
                ui.separator();
                import = self.ceiling_row(ui, cols, None, wide);
            }
            let Some(levers) = levers else {
                ui.separator();
                crate::lineage_page::prose(ui, "This plan was made before umaLord measured what would raise it.", 13.5, p.weak);
                let label = if key == "runner" { "Measure it" } else { "Re-plan to measure it" };
                measure = ui.add(crate::ui_util::primary_button(ui, label, 13.0)).on_hover_text("Runs the simulations that find the umas, cards and parents that would raise the plan most.").clicked();
                return;
            };
            if levers.is_empty() {
                ui.separator();
                ui.label(RichText::new("Nothing we tried raised this plan by enough to list; your account already covers it.").size(14.5));
                crate::lineage_page::prose(ui, &format!("Tried: {}. Re-plan after a new banner to try again.", tried.text()), 12.5, p.weak);
            }
            let shown = if all { levers.len() } else { levers.len().min(5) };
            for l in levers.iter().take(shown) {
                ui.separator();
                self.ceiling_row(ui, cols, Some(l), wide);
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if levers.len() > 5 && ui.link(semibold(if all { "Show fewer ▲".to_string() } else { format!("Show all ({}) ▾", levers.len()) }, 13.5)).clicked() {
                    toggle = true;
                }
                ui.label(RichText::new("Only umas and cards already out on Global are suggested.").size(12.5).color(p.weak));
            });
        });
        if toggle {
            self.lineage.p.ceiling_all = if all { None } else { Some(key.to_string()) };
        }
        if import {
            self.import_finished_from_downloads();
        }
        measure
    }

    /// One row: the gain, the art, what to do and why with how it was measured, and an action.
    /// `None` is a fresh account's import row; returns whether its import was pressed.
    fn ceiling_row(&mut self, ui: &mut egui::Ui, cols: [f32; 3], l: Option<&crate::parentage::ceiling::Lever>, wide: bool) -> bool {
        use crate::parentage::ceiling::{LeverKind, Unit};
        let p = pal(ui);
        let mut import = false;
        let mut add: Option<(LeverKind, u32)> = None;
        ui.horizontal_top(|ui| {
            // The gain: the number large, its unit under it.
            ui.allocate_ui_with_layout(vec2(cols[0], 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(cols[0]);
                let size = if wide { 26.0 } else { 22.0 };
                let (num, unit, term) = match l {
                    None => ("fewer".to_string(), "careers".to_string(), "careers"),
                    Some(l) => {
                        let text = l.gain_text();
                        let (n, u) = text.split_once(' ').map_or((text.clone(), String::new()), |(n, u)| (n.to_string(), u.to_string()));
                        let term = match l.unit {
                            Unit::Points => "pts",
                            Unit::Lengths => "lengths",
                            Unit::Careers => "careers",
                            Unit::Stats => "stats",
                        };
                        (n, u, term)
                    }
                };
                ui.label(semibold(num, size).color(p.good));
                rich(ui, &[Span::term(&unit, term).colored(p.weak)], 12.5, cols[0]);
            });
            // The art: the uma, the card (with its LB diamonds for a limit break), or the runner.
            ui.allocate_ui_with_layout(vec2(cols[1], 0.0), egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.set_width(cols[1]);
                let w = cols[1] - 8.0;
                match l {
                    None => {
                        let (r, _) = ui.allocate_exact_size(vec2(w, w), Sense::hover());
                        ui.painter().rect(r, 10.0, p.raised, Stroke::new(1.0, p.weak.gamma_multiply(0.5)), egui::StrokeKind::Inside);
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, "↓", FontId::proportional(20.0), p.text);
                    }
                    Some(l) => match (l.kind, l.card) {
                        (LeverKind::GetCard, Some(c)) => {
                            self.card_art(ui, c, w, false);
                        }
                        (LeverKind::LimitBreak, Some(c)) => {
                            self.card_art(ui, c, w, false);
                            let (r, resp) = ui.allocate_exact_size(vec2(w, 10.0), Sense::hover());
                            crate::ui_util::draw_lb(ui.painter(), pos2(r.left() + 2.0, r.center().y), l.lb.unwrap_or(0), 4.5);
                            resp.on_hover_text(format!("LB{} now; MLB is all four", l.lb.unwrap_or(0)));
                        }
                        _ => {
                            if let Some(u) = l.uma {
                                self.avatar(ui, u, w);
                            }
                        }
                    },
                }
            });
            let action_w = if wide { cols[2] } else { 0.0 };
            // About 75 characters, with the action right after the text.
            let text_w = (ui.available_width() - action_w - ui.spacing().item_spacing.x).clamp(200.0, 640.0);
            ui.allocate_ui_with_layout(vec2(text_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(text_w);
                let (kicker, title, why) = match l {
                    None => (
                        "Import your finished umas".to_string(),
                        "Import your finished umas; plans with them usually need fewer careers.".to_string(),
                        "This plan can't see any finished umas yet, so every parent in it is a friend's or a new career.".to_string(),
                    ),
                    Some(l) => {
                        let kicker = match l.kind {
                            LeverKind::GetUma => "Get an uma",
                            LeverKind::GetCard => "Get a card",
                            LeverKind::LimitBreak => "Limit-break a card",
                            LeverKind::Parent => "Borrow or breed a better parent",
                        };
                        let mut why = l.why.clone();
                        if !l.helps.is_empty() {
                            why.push_str(&format!(" It also helps {}.", l.helps.iter().map(|u| crate::parentage::short_name(&self.kb, *u)).collect::<Vec<_>>().join(", ")));
                        }
                        (kicker.to_string(), l.title.clone(), why)
                    }
                };
                let looking = l.is_some_and(|l| l.kind == LeverKind::Parent && self.lineage.look_for.as_deref() == Some(format!("ceiling-parent:{}", l.uma.unwrap_or(0)).as_str()));
                ui.label(RichText::new(kicker).size(12.5).strong().color(p.weak));
                ui.add(egui::Label::new(semibold(title, 14.5)).wrap());
                if let Some(spark) = l.and_then(|l| l.spark.clone()) {
                    ui.horizontal_wrapped(|ui| {
                        for part in spark.split(" and ") {
                            // "End Closer pink 1★": the aptitude and its stars, as the game draws them.
                            let (name, stars) = part.rsplit_once(" pink ").map_or((part, 1), |(n, s)| (n, s.trim_end_matches('★').parse::<u8>().unwrap_or(1)));
                            crate::lineage_kit::spark_chip(ui, crate::lineage_kit::Spark::Pink, name, 0, Some(stars), false, true);
                        }
                    });
                }
                ui.add(egui::Label::new(RichText::new(why).size(13.5)).wrap());
                ui.horizontal_wrapped(|ui| {
                    if let Some(l) = l {
                        let tag = crate::ui_util::pill_padded(ui, RichText::new(&l.measured).size(12.0).color(p.accent), p.accent.gamma_multiply(0.15), Stroke::new(1.0, p.accent.gamma_multiply(0.55)), vec2(7.0, 3.0));
                        let compared = l.compared.clone();
                        tag.on_hover_ui(|ui| {
                            ui.set_max_width(320.0);
                            ui.label(semibold("What was compared", 13.0));
                            ui.add(egui::Label::new(RichText::new(compared).size(12.5)).wrap());
                        });
                    }
                    if !wide {
                        if let Some(a) = ceiling_action(ui, l) {
                            match a {
                                Some(x) => add = Some(x),
                                None => import = true,
                            }
                        }
                    }
                });
                if let (true, Some(l)) = (looking, l) {
                    ui.add_space(6.0);
                    egui::Frame::new().fill(p.raised).corner_radius(8).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(semibold("What to look for in a friend's legacy", 13.5));
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("One of:").size(12.5).color(p.weak));
                            for u in &l.look {
                                self.avatar(ui, *u, 22.0);
                                ui.label(RichText::new(crate::parentage::short_name(&self.kb, *u)).size(12.5));
                            }
                        });
                        let spark = l.spark.clone().unwrap_or_default();
                        let partner = l.partner.map_or(String::new(), |id| format!(", to pair with your #{id}"));
                        crate::lineage_page::prose(ui, &format!("She must carry {spark} or more, counting her and her two parents{partner}. The characters are ranked by affinity with your runner and her partner. A lineage takes one friend's legacy, and you borrow her again for each career that uses her."), 12.5, p.weak);
                    });
                }
            });
            if wide {
                ui.allocate_ui_with_layout(vec2(cols[2], 0.0), egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.set_width(cols[2]);
                    if let Some(a) = ceiling_action(ui, l) {
                        match a {
                            Some(x) => add = Some(x),
                            None => import = true,
                        }
                    }
                });
            }
        });
        match add {
            Some((LeverKind::GetUma, _)) => self.open_setup(crate::onboarding::Step::Umas),
            Some((LeverKind::Parent, u)) => {
                let key = format!("ceiling-parent:{u}");
                self.lineage.look_for = if self.lineage.look_for.as_deref() == Some(key.as_str()) { None } else { Some(key) };
            }
            Some((_, card)) => self.add_card_dialog(card),
            None => {}
        }
        import
    }
}

/// A row's action button: Add or Update in my collection (`Some(Some(kind, id))`), or the fresh
/// account's import (`Some(None)`); nothing for a parent.
fn ceiling_action(ui: &mut egui::Ui, l: Option<&crate::parentage::ceiling::Lever>) -> Option<Option<(crate::parentage::ceiling::LeverKind, u32)>> {
    use crate::parentage::ceiling::LeverKind;
    match l {
        None => ui.add(crate::ui_util::primary_button(ui, "Import from the game", 13.0)).clicked().then_some(None),
        Some(l) => match (l.kind, l.card, l.uma) {
            (LeverKind::GetUma, _, Some(u)) => ui.button("Add to my collection").on_hover_text("Once you've pulled her: opens the umas you own, to add her.").clicked().then_some(Some((LeverKind::GetUma, u))),
            (LeverKind::GetCard, Some(c), _) => ui.button("Add to my collection").on_hover_text("Once you've pulled it: opens Add cards with it picked.").clicked().then_some(Some((LeverKind::GetCard, c))),
            (LeverKind::LimitBreak, Some(c), _) => ui.button("Update in my collection").on_hover_text("Once you've merged copies: opens it in Add cards to set its LB.").clicked().then_some(Some((LeverKind::LimitBreak, c))),
            (LeverKind::Parent, _, Some(u)) => ui.button("What to look for ▸").on_hover_text("Which friend's legacy to look for, and what she must carry.").clicked().then_some(Some((LeverKind::Parent, u))),
            _ => None,
        },
    }
}
