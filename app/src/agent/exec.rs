//! The tools' handlers. `App::agent_call` validates the input and runs a tool: Read, Draft and UI
//! tools finish at once, Long tools start a background job the caller waits on
//! (`App::agent_wait`), and Write tools come back as a question for the user, then run with
//! `App::agent_write` once approved.

use std::path::PathBuf;

use eframe::egui;
use serde_json::{Value, json};

use super::{Approval, Exec, Kind, OpenAt, SimSetup, Snapshot, ToolOut, Visual, Wait};
use crate::App;
use crate::knowledge::{Distance, EventPreset, SparkInfo, Style, Surface, grade_rank};
use crate::onboarding::Edit;
use crate::tab_planner::{Plan, Purpose};
use crate::tab_sim::{ExtraKind, Mode};
use crate::ui_util::today;

/// Simulations one question may start; the prompt asks for at most four unless the user wants more.
pub(crate) const MAX_SIMS: u32 = 8;

fn text<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v[k].as_str().map(str::trim).filter(|s| !s.is_empty())
}

fn int(v: &Value, k: &str) -> Option<u64> {
    v[k].as_u64().or_else(|| v[k].as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
}

fn texts(v: &Value, k: &str) -> Vec<String> {
    v[k].as_array().into_iter().flatten().filter_map(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn parse_distance(v: &Value) -> Option<Distance> {
    Distance::ALL.into_iter().find(|d| v.as_str() == Some(d.label()))
}
fn parse_surface(v: &Value) -> Option<Surface> {
    Surface::ALL.into_iter().find(|d| v.as_str() == Some(d.label()))
}
fn parse_style(v: &Value) -> Option<Style> {
    Style::ALL.into_iter().find(|d| v.as_str() == Some(d.short()) || v.as_str() == Some(d.label()))
}

pub(crate) fn clip(s: String, max: usize) -> String {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}\n…(truncated)", &s[..end])
    }
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// 1-based condition codes for the names the schemas take.
fn code(list: &[&str], name: &str) -> Option<i32> {
    list.iter().position(|x| x.eq_ignore_ascii_case(name)).map(|i| i as i32 + 1)
}

const LOG_KEYS: [&str; 5] = ["careers", "team_trials", "champions_meeting", "league_of_heroes", "sparks"];

impl App {
    // -----------------------------------------------------------------------
    // Lookups shared by the tools

    pub(crate) fn find_uma_id(&self, q: &str) -> Result<u32, String> {
        let ql = q.trim().to_lowercase();
        if let Ok(id) = ql.parse::<u32>()
            && self.kb.umas.contains_key(&id)
        {
            return Ok(id);
        }
        let mut hits: Vec<u32> = self
            .kb
            .umas
            .values()
            .filter(|u| format!("{} [{}]", u.name, u.outfit).to_lowercase().contains(&ql) || u.name.to_lowercase() == ql)
            .map(|u| u.id)
            .collect();
        if hits.is_empty() {
            hits = self.kb.umas.values().filter(|u| ql.contains(&u.name.to_lowercase())).map(|u| u.id).collect();
        }
        hits.sort_by_key(|id| (self.owned_uma(*id).is_none(), *id));
        match hits.as_slice() {
            [] => Err(format!("No uma matches '{q}'.")),
            [one] => Ok(*one),
            many if self.owned_uma(many[0]).is_some() && many.iter().filter(|i| self.owned_uma(**i).is_some()).count() == 1 => Ok(many[0]),
            many => Err(format!(
                "Several match '{q}': {}. Ask again with the outfit.",
                many.iter()
                    .map(|i| {
                        let u = &self.kb.umas[i];
                        format!("{} [{}]{}", u.name, u.outfit, if self.owned_uma(*i).is_some() { " (owned)" } else { "" })
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
        }
    }

    /// An uma for affinity, where any outfit of the character will do.
    fn find_character(&self, q: &str) -> Result<u32, String> {
        let ql = q.trim().to_lowercase();
        // A bare chara id (an uma id / 100).
        if let Ok(c) = ql.parse::<u32>()
            && let Some(u) = self.kb.umas.keys().find(|id| **id / 100 == c)
        {
            return Ok(*u);
        }
        self.find_uma_id(q).or_else(|e| {
            let hits: Vec<u32> = self.kb.umas.values().filter(|u| ql.contains(&u.name.to_lowercase()) || u.name.to_lowercase().contains(&ql)).map(|u| u.id).collect();
            match hits.first() {
                Some(&first) if hits.iter().all(|h| h / 100 == first / 100) => Ok(first),
                _ => Err(e),
            }
        })
    }

    /// G1 race names as races.json writes them, from what the model wrote (case aside).
    fn g1_wins(&self, v: &Value) -> Result<std::collections::BTreeSet<String>, String> {
        let mut out = std::collections::BTreeSet::new();
        let mut unknown = Vec::new();
        for name in v.as_array().into_iter().flatten().filter_map(Value::as_str) {
            match self.kb.races.iter().find(|r| r.grade == "G1" && r.name.eq_ignore_ascii_case(name.trim())) {
                Some(r) => {
                    out.insert(r.name.clone());
                }
                None => unknown.push(name),
            }
        }
        if unknown.is_empty() { Ok(out) } else { Err(format!("Not a G1 race name: {}. find_races gives the names; only G1 wins count.", unknown.join(", "))) }
    }

    /// A character's name, from any of her outfits.
    fn character_name(&self, uma: u32) -> String {
        let c = crate::affinity::chara(uma);
        self.kb.umas.values().find(|u| u.id / 100 == c).map_or_else(|| format!("character {c}"), |u| u.name.clone())
    }

    /// The best stars this account has on any outfit of the character, if it owns one.
    fn owned_stars(&self, uma: u32) -> Option<u8> {
        let c = crate::affinity::chara(uma);
        self.roster.umas.iter().filter(|u| u.id / 100 == c).map(|u| u.stars).max()
    }

    fn affinity_text(&self, trainee: u32, legacies: &[Option<crate::affinity::Legacy>; 2]) -> String {
        use crate::affinity::symbol;
        let t = &self.kb.affinity;
        let factor = |a: u32| format!("its sparks' chances ×{:.2}", 1.0 + f64::from(a) / 100.0);
        let lineage = t.lineage(trainee, legacies);
        let mut out = format!("Trainee: {}.\n", self.character_name(trainee));
        for (i, (l, a)) in legacies.iter().zip(&lineage.legacies).enumerate() {
            let (Some(l), Some(a)) = (l, a) else { continue };
            let other = legacies[1 - i].as_ref().map(|o| self.character_name(o.uma));
            out += &format!("Legacy {}: {} — affinity {} ({}). Parts: with the trainee {}", i + 1, self.character_name(l.uma), a.total(), factor(a.total()), a.direct);
            for (j, gp) in l.parents.iter().enumerate() {
                if let Some(gp) = gp {
                    out += &format!("; trio with {} {}, shared G1s +{}", self.character_name(gp.uma), a.with_grandparent[j], a.grandparent_races[j]);
                }
            }
            if let Some(o) = &other {
                out += &format!("; with {o} (the other legacy) {}, shared G1s +{}", a.with_other, a.other_races);
            }
            out += ".\n";
            for (j, gp) in l.parents.iter().enumerate() {
                if let Some(gp) = gp {
                    let g = a.grandparent(j);
                    out += &format!("  Grandparent {}: {g} ({}).\n", self.character_name(gp.uma), factor(g));
                }
            }
        }
        let total = lineage.total();
        out += &format!("Lineage total: {total} {} (the symbol the game shows; ◎ over 150, ○ 51–150, △ 50 or less).", symbol(total));
        let unknown: Vec<String> = std::iter::once(trainee)
            .chain(legacies.iter().flatten().flat_map(|l| std::iter::once(l.uma).chain(l.parents.iter().flatten().map(|g| g.uma))))
            .filter(|u| !t.knows(*u))
            .map(|u| self.character_name(u))
            .collect();
        if !unknown.is_empty() {
            out += &format!("\nNot in the affinity tables (newer than them, counted as 0): {}.", unknown.join(", "));
        }
        out
    }

    /// The trainee and lineage a get_affinity or plan_lineage input names, with each legacy's
    /// spark ids for itself and its two parents: a finished uma brings its own; a hypothetical
    /// slot takes `sparks` like "Dirt 2★".
    fn read_lineage(&self, input: &Value) -> Result<(u32, [Option<crate::affinity::Legacy>; 2], [[Vec<u32>; 3]; 2]), String> {
        if self.kb.affinity.is_empty() {
            return Err("The affinity tables aren't loaded (knowledge/data/affinity.json).".into());
        }
        let trainee = self.find_character(input["trainee"].as_str().unwrap_or_default())?;
        let mut legacies: [Option<crate::affinity::Legacy>; 2] = [None, None];
        let mut sparks: [[Vec<u32>; 3]; 2] = Default::default();
        let finished = self.veterans();
        for (i, l) in input["legacies"].as_array().into_iter().flatten().take(2).enumerate() {
            if let Some(id) = l["finished"].as_u64() {
                let v = finished.iter().find(|v| v.trained_chara_id == id).ok_or(format!("No finished uma #{id}; list_finished_umas lists them."))?;
                legacies[i] = Some(v.legacy(&self.kb));
                sparks[i] = [v.factors(), v.ancestor(10).map(|a| a.factors()).unwrap_or_default(), v.ancestor(20).map(|a| a.factors()).unwrap_or_default()];
                continue;
            }
            if l["uma"].as_str().is_none() {
                return Err("Each legacy needs an uma, or a finished uma's #id as finished.".into());
            }
            let mut legacy = crate::affinity::Legacy { uma: self.find_character(l["uma"].as_str().unwrap_or_default())?, wins: self.g1_wins(&l["g1_wins"])?, parents: [None, None] };
            sparks[i][0] = self.spark_ids(&l["sparks"])?;
            for (j, g) in l["parents"].as_array().into_iter().flatten().take(2).enumerate() {
                legacy.parents[j] = Some(crate::affinity::Ancestor { uma: self.find_character(g["uma"].as_str().unwrap_or_default())?, wins: self.g1_wins(&g["g1_wins"])? });
                sparks[i][j + 1] = self.spark_ids(&g["sparks"])?;
            }
            legacies[i] = Some(legacy);
        }
        Ok((trainee, legacies, sparks))
    }

    /// Spark ids from "Dirt 2★" / "Speed 3*" / "Angling and Scheming 2★" (stars default to 1).
    fn spark_ids(&self, v: &Value) -> Result<Vec<u32>, String> {
        let mut out = Vec::new();
        for text in v.as_array().into_iter().flatten().filter_map(Value::as_str) {
            // "Dirt 2★", "Dirt 2 ★", "Dirt ★★", "Dirt 2*", or a name alone (1★).
            let t = text.trim();
            let stem = t.trim_end_matches(['★', '*', ' ']);
            let marks = t[stem.len()..].chars().filter(|c| matches!(c, '★' | '*')).count();
            let (name, stars) = match stem.rsplit_once(' ') {
                Some((n, s)) if s.parse::<u8>().is_ok() => (n.trim(), s.parse::<u8>().unwrap_or(1)),
                _ => (stem, if marks > 1 { marks as u8 } else { 1 }),
            };
            // The game's spelling first, then ignoring spaces and case ("Victoria por plancha☆"), then
            // a name the game prefixes ("Escapades" for "564 Escapades").
            let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
            let want = squash(name);
            let at = |f: &dyn Fn(&SparkInfo) -> bool| self.kb.sparks.values().filter(|s| s.stars == stars && f(s)).min_by_key(|s| s.id);
            let s = at(&|s| s.name.eq_ignore_ascii_case(name))
                .or_else(|| at(&|s| squash(&s.name) == want))
                .or_else(|| at(&|s| squash(&s.name).ends_with(&want) && want.len() >= 5))
                .ok_or(format!("No spark called \"{text}\" (write a name as the game does, like \"Dirt 2★\" or \"Speed 3★\")."))?;
            out.push(s.id);
        }
        Ok(out)
    }

    /// A finished uma's sparks by colour: "Blue: Speed 3★. Pink: Turf 2★. Green: … White: …".
    fn veteran_sparks(&self, v: &crate::veterans::Veteran) -> String {
        let mut by: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
        for s in v.factors().iter().filter_map(|f| self.kb.sparks.get(f)) {
            let k = match s.kind.as_str() {
                "blue" => "1 Blue",
                "pink" => "2 Pink",
                "green" => "3 Green",
                _ => "4 White",
            };
            by.entry(k).or_default().push(format!("{} {}★", s.name, s.stars));
        }
        by.iter().map(|(k, v)| format!("{}: {}.", &k[2..], v.join(", "))).collect::<Vec<_>>().join(" ")
    }

    pub(crate) fn find_card_id(&self, q: &str) -> Result<u32, String> {
        let ql = q.trim().to_lowercase();
        if let Ok(id) = ql.parse::<u32>()
            && self.kb.cards.contains_key(&id)
        {
            return Ok(id);
        }
        let mut hits: Vec<u32> = self
            .kb
            .cards
            .values()
            .filter(|c| format!("{} [{}] {} {}", c.uma, c.title, c.rarity, c.kind).to_lowercase().contains(&ql))
            .map(|c| c.id)
            .collect();
        hits.sort_by_key(|id| (self.owned_card(*id).is_none(), std::cmp::Reverse(*id)));
        let owned: Vec<u32> = hits.iter().copied().filter(|i| self.owned_card(*i).is_some()).collect();
        match (hits.as_slice(), owned.as_slice()) {
            ([], _) => Err(format!("No card matches '{q}'.")),
            ([one], _) | (_, [one]) => Ok(*one),
            (many, _) => Err(format!(
                "Several match '{q}': {}. Ask again with the title.",
                many.iter()
                    .take(15)
                    .map(|i| {
                        let c = &self.kb.cards[i];
                        format!("{} {} {} [{}]{}", c.rarity, c.kind, c.uma, c.title, self.owned_card(*i).map_or(String::new(), |o| format!(" (owned LB{})", o.lb)))
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
        }
    }

    pub(crate) fn find_skill_id(&self, q: &str) -> Result<u32, String> {
        let ql = q.trim().to_lowercase();
        if let Ok(id) = ql.parse::<u32>()
            && self.kb.skills.contains_key(&id)
        {
            return Ok(id);
        }
        if let Some(id) = self.kb.skill_order.iter().find(|i| self.kb.skills[i].name.to_lowercase() == ql) {
            return Ok(*id);
        }
        let hits: Vec<u32> = self.kb.skill_order.iter().copied().filter(|i| self.kb.skills[i].name.to_lowercase().contains(&ql)).collect();
        match hits.as_slice() {
            [] => Err(format!("No skill matches '{q}'.")),
            [one] => Ok(*one),
            many => Err(format!("Several match '{q}': {}.", many.iter().take(20).map(|i| self.kb.skills[i].name.clone()).collect::<Vec<_>>().join("; "))),
        }
    }

    fn find_skills(&self, names: &[String]) -> Result<Vec<u32>, String> {
        let mut ids = Vec::new();
        let mut errors = Vec::new();
        for n in names {
            match self.find_skill_id(n) {
                Ok(id) => ids.push(id),
                Err(e) => errors.push(e),
            }
        }
        if errors.is_empty() { Ok(ids) } else { Err(errors.join(" ")) }
    }

    /// A Champions Meeting by cup name, number ("22", "CM 22") or "next"; the nearest upcoming
    /// one when a cup name recurs.
    pub(crate) fn find_event(&self, q: &str) -> Result<EventPreset, String> {
        let now = today();
        let ql = q.trim().to_lowercase();
        if ql.is_empty() || ql == "next" {
            return self.kb.upcoming_cms(&now).first().map(|e| (*e).clone()).ok_or_else(|| "No upcoming Champions Meeting in the schedule.".to_string());
        }
        let num = ql.trim_start_matches("cm").trim().parse::<u32>().ok();
        let day = |d: &str| crate::tab_planner::day_number(d);
        let mut hits: Vec<&EventPreset> = self
            .kb
            .umalator_presets
            .iter()
            .filter(|e| e.is_cm())
            .filter(|e| num.map_or_else(|| e.name.to_lowercase().contains(&ql) || ql.contains(&e.name.to_lowercase()), |n| e.id == n))
            .collect();
        hits.sort_by_key(|e| {
            let d = day(&e.date) - day(&now);
            (d < -10, d.abs())
        });
        hits.first().map(|e| (*e).clone()).ok_or_else(|| format!("No Champions Meeting matches '{q}'. list_upcoming_events shows the schedule."))
    }

    pub(crate) fn skill_line(&self, id: u32) -> String {
        let Some(s) = self.kb.skills.get(&id) else { return format!("skill {id}") };
        let tags = self.kb.tag_line(&s.tags());
        format!(
            "{} ({}, {} SP{}): {}{}",
            s.name,
            s.tier(),
            s.sp,
            if tags.is_empty() { String::new() } else { format!(", only {tags}") },
            if s.summary.is_empty() { s.effect_text() } else { s.summary.clone() },
            if self.skill_available(id) { " [available to you]" } else { "" }
        )
    }

    fn uma_label(&self, id: u32) -> String {
        self.kb.umas.get(&id).map_or(format!("uma {id}"), |u| format!("{} [{}]", u.name, u.outfit))
    }

    fn card_label(&self, id: u32) -> String {
        let Some(c) = self.kb.cards.get(&id) else { return format!("card {id}") };
        let owned = self.owned_card(id).map_or(" (not owned)".to_string(), |o| format!(" (owned LB{} Lv{})", o.lb, o.level));
        format!("{} [{}] ({} {}){owned}", c.uma, c.title, c.rarity, c.kind)
    }

    fn skill_names(&self, ids: &[u32]) -> String {
        if ids.is_empty() { "none".into() } else { ids.iter().map(|s| self.kb.skill_name(*s)).collect::<Vec<_>>().join(", ") }
    }

    /// The working plan in a few lines, for the model.
    pub(crate) fn plan_brief(&self) -> String {
        use std::fmt::Write;
        let p = &self.planner.plan;
        let mut o = String::new();
        let saved = self.planner.open_file.as_ref().and_then(|f| f.file_name()).map_or("not saved yet".to_string(), |f| format!("saved as plans/{}", f.to_string_lossy()));
        let _ = writeln!(o, "Working plan: {} ({saved})", if p.title.is_empty() { "untitled" } else { &p.title });
        let _ = writeln!(o, "Goal: {}; {} {}, {}; scenario {}", p.purpose.label(), p.distance.label(), p.surface.label(), p.style.label(), self.plan_scenario_name());
        if let Some(e) = self.plan_cm() {
            let off = if e.disabled_skills.is_empty() { String::new() } else { format!("; turns off: {}", self.skill_names(&e.disabled_skills)) };
            let _ = writeln!(o, "Champions Meeting: CM {} {} on {} ({}), {}, {}{off}", e.id, e.name, e.date, if e.announced { "announced" } else { "estimated" }, e.course_name, e.conditions);
        }
        let race = p.course.and_then(|c| self.kb.races.iter().find(|r| r.id == c)).map_or("not set".to_string(), |r| format!("{} ({} {} {}m {})", r.name, r.track, r.surface, r.distance, r.direction));
        let setup = self.plan_race_setup();
        let sim_course = setup.course.and_then(|c| self.sim_data.as_ref().map(|d| d.course_name(c))).unwrap_or_else(|| "none".into());
        let _ = writeln!(o, "Race: {race}; simulator course {sim_course}; conditions {}", if p.conditions.is_empty() { "from the CM or nearest event" } else { &p.conditions });
        match p.trainee.and_then(|t| Some((t, self.kb.umas.get(&t)?))) {
            Some((id, info)) => {
                let a = &info.aptitude;
                let owned = self.owned_uma(id).map_or("not owned".to_string(), |u| format!("{}★ P{}", u.stars, u.potential));
                let _ = writeln!(
                    o,
                    "Trainee: {} [{}] {owned}; aptitude {} {}, {} {}, {} {}",
                    info.name,
                    info.outfit,
                    p.distance.label(),
                    a.distance(p.distance),
                    p.surface.label(),
                    a.surface(p.surface),
                    p.style.label(),
                    a.style(p.style)
                );
            }
            None => o.push_str("Trainee: not picked\n"),
        }
        let _ = writeln!(o, "Target stats: {}", if p.target.iter().all(|v| *v == 0) { "not set".to_string() } else { format!("{:?}", p.target) });
        let deck: Vec<String> = p.deck.iter().enumerate().map(|(i, d)| format!("{}. {}", i + 1, d.map_or("empty".to_string(), |id| self.card_label(id)))).collect();
        let _ = writeln!(o, "Deck: {}", deck.join("; "));
        let _ = writeln!(o, "Friend card: {}", p.friend.and_then(|f| self.kb.cards.get(&f)).map_or("none".to_string(), |c| format!("{} [{}] ({} {}) LB{}", c.uma, c.title, c.rarity, c.kind, p.friend_lb)));
        let career = self.plan_career();
        let cost = self.chosen_cost(&p.skills, career.as_ref());
        let budget = self.pick_budget().map_or("unknown".to_string(), |b| format!("{b:.0}"));
        let _ = writeln!(o, "Skills to buy ({}, about {cost:.0} SP of {budget}): {}", p.skills.len(), self.skill_names(&p.skills));
        if !p.pinned_skills.is_empty() || !p.banned_skills.is_empty() {
            let _ = writeln!(o, "Pinned: {}; banned: {}", self.skill_names(&p.pinned_skills), self.skill_names(&p.banned_skills));
        }
        let _ = writeln!(o, "SP budget: {}", p.sp_budget.map_or_else(|| career.map_or("needs a trainee and the scenario data".to_string(), |c| format!("{:.0} (career simulation estimate)", c.sp_mean)), |b| format!("{b} (set by hand)")));
        if let Some(label) = self.plan_meta_label() {
            let _ = writeln!(o, "Meta sheet entry: {label}");
        }
        if let Some(note) = self.aptitude_note() {
            let _ = writeln!(o, "Warning: {note}");
        }
        o
    }

    /// Aptitudes below A for the goal, as a sentence.
    fn aptitude_note(&self) -> Option<String> {
        let p = &self.planner.plan;
        let a = &self.kb.umas.get(&p.trainee?)?.aptitude;
        let low: Vec<String> = [(p.distance.label(), a.distance(p.distance)), (p.surface.label(), a.surface(p.surface)), (p.style.label(), a.style(p.style))]
            .into_iter()
            .filter(|(_, g)| grade_rank(g) < 7)
            .map(|(l, g)| format!("{l} {g}"))
            .collect();
        (!low.is_empty()).then(|| format!("the trainee's aptitude is below A for {} (slower in the race; pink sparks from parents raise it)", low.join(", ")))
    }

    // -----------------------------------------------------------------------
    // Read tools

    pub(crate) fn run_tool(&self, name: &str, input: &Value) -> Result<String, String> {
        match name {
            "get_account_summary" => Ok(self.account_summary()),
            "find_umas" => {
                let (d, s, st) = (parse_distance(&input["distance"]), parse_surface(&input["surface"]), parse_style(&input["style"]));
                let min = input["min_grade"].as_str().map_or(7, grade_rank);
                let mut rows: Vec<(u32, String)> = Vec::new();
                for u in &self.roster.umas {
                    let Some(info) = self.kb.umas.get(&u.id) else { continue };
                    let a = &info.aptitude;
                    let grades = [d.map(|x| a.distance(x)), s.map(|x| a.surface(x)), st.map(|x| a.style(x))];
                    if grades.iter().flatten().any(|g| grade_rank(g) < min) {
                        continue;
                    }
                    let score: u32 = grades.iter().flatten().map(|g| u32::from(grade_rank(g))).sum::<u32>() * 10 + u32::from(u.stars);
                    rows.push((
                        score,
                        format!(
                            "{} [{}] {}★ P{} | turf {} dirt {} | sprint {} mile {} medium {} long {} | front {} pace {} late {} end {}",
                            u.name, u.outfit, u.stars, u.potential, a.turf, a.dirt, a.sprint, a.mile, a.medium, a.long, a.front, a.pace, a.late, a.end
                        ),
                    ));
                }
                rows.sort_by(|a, b| b.0.cmp(&a.0));
                Ok(if rows.is_empty() { "No owned uma fits.".into() } else { rows.into_iter().map(|r| r.1).collect::<Vec<_>>().join("\n") })
            }
            "get_uma" => {
                let id = self.find_uma_id(input["query"].as_str().unwrap_or_default())?;
                let i = &self.kb.umas[&id];
                let owned = self.owned_uma(id);
                let a = &i.aptitude;
                let mut out = format!(
                    "{} [{}] (id {id}), released {}\nOwned: {}\nAptitude: turf {} dirt {} | sprint {} mile {} medium {} long {} | front {} pace {} late {} end {}\nBase stats (spd/sta/pow/guts/wit): {:?}\nGrowth %: {:?}\n",
                    i.name,
                    i.outfit,
                    i.release,
                    owned.map_or("no".to_string(), |o| format!("yes, {}★, potential {}", o.stars, o.potential)),
                    a.turf,
                    a.dirt,
                    a.sprint,
                    a.mile,
                    a.medium,
                    a.long,
                    a.front,
                    a.pace,
                    a.late,
                    a.end,
                    i.base,
                    i.growth
                );
                if let Some(u) = i.unique_skill {
                    out += &format!("Unique: {}\n", self.skill_line(u));
                }
                for s in &i.skills {
                    let locked = owned.is_some_and(|o| o.potential < s.potential);
                    out += &format!(
                        "{}{}: {}\n",
                        if s.potential <= 1 { "Innate".to_string() } else { format!("Potential {}", s.potential) },
                        if locked { " (LOCKED for this account)" } else { "" },
                        self.skill_line(s.id)
                    );
                }
                for e in &i.evolved {
                    out += &format!("Evolves {} -> {} when: {}\n", self.kb.skill_name(e.base), self.skill_line(e.evolved), e.conditions.iter().map(|c| c.join(" + ")).collect::<Vec<_>>().join(" OR "));
                }
                if !i.event_skills.is_empty() {
                    out += &format!("Event skills: {}\n", i.event_skills.iter().map(|s| self.kb.skill_name(*s)).collect::<Vec<_>>().join(", "));
                }
                out += "Career objectives:\n";
                for o in &i.objectives {
                    out += &format!("- {}: {} ({})\n", o.when, o.race, o.goal);
                }
                out += &format!("{} career events (event outcomes are in the Umas tab).\n", i.events.len());
                Ok(out)
            }
            "get_card" => {
                let id = self.find_card_id(input["query"].as_str().unwrap_or_default())?;
                let c = &self.kb.cards[&id];
                let owned = self.owned_card(id);
                let max = match c.rarity.as_str() {
                    "SSR" => 50,
                    "SR" => 45,
                    _ => 40,
                };
                let level = owned.map_or(max, |o| u32::from(o.level));
                let mut out = format!(
                    "{} {} {} [{}] (id {id}), released {}\nOwned: {}\nEffects (at owned Lv{level} / at Lv{max}):\n",
                    c.rarity,
                    c.kind,
                    c.uma,
                    c.title,
                    c.release,
                    owned.map_or("no".to_string(), |o| format!("yes, LB{} Lv{}", o.lb, o.level))
                );
                for e in &c.effects {
                    let now = crate::knowledge::effect_at(&e.points, level);
                    let top = crate::knowledge::effect_at(&e.points, max);
                    if now.unwrap_or(0) != 0 || top.unwrap_or(0) != 0 {
                        out += &format!("- {}: {} / {}\n", e.name, now.unwrap_or(0), top.unwrap_or(0));
                    }
                }
                if let Some(u) = &c.unique {
                    out += &format!(
                        "Unique effect (from Lv{}{}): {} {}\n",
                        u.level,
                        if level >= u.level { ", active" } else { ", NOT active at owned level" },
                        u.condition,
                        u.effects.iter().map(|e| format!("{} {}", e.name, e.value)).collect::<Vec<_>>().join(", ")
                    );
                }
                out += "Skill hints:\n";
                for s in &c.hints {
                    out += &format!("- {}\n", self.skill_line(*s));
                }
                if !c.event_skills.is_empty() {
                    out += &format!("Skills from events: {}\n", c.event_skills.iter().map(|s| self.kb.skill_name(*s)).collect::<Vec<_>>().join(", "));
                }
                out += "Events (options; '|' separates random alternative outcomes, odds unknown):\n";
                for e in &c.events {
                    let opts: Vec<String> = e.options.iter().map(|alts| alts.iter().map(|l| l.join(", ")).collect::<Vec<_>>().join(" | ")).collect();
                    out += &format!("- {} ({}): {}\n", e.title, e.kind, opts.join(" // "));
                }
                Ok(clip(out, 12_000))
            }
            "search_skills" => {
                let q = input["query"].as_str().unwrap_or_default().to_lowercase();
                let owned_only = input["owned_only"].as_bool().unwrap_or(true);
                let cat = input["category"].as_str();
                let (d, s, st) = (parse_distance(&input["distance"]), parse_surface(&input["surface"]), parse_style(&input["style"]));
                let hits: Vec<String> = self
                    .kb
                    .skill_order
                    .iter()
                    .filter(|id| {
                        let k = &self.kb.skills[id];
                        (q.is_empty() || k.name.to_lowercase().contains(&q) || k.summary.to_lowercase().contains(&q))
                            && cat.is_none_or(|c| k.category == c || k.tier() == c)
                            && k.tags().fits(st, d, s)
                            && (!owned_only || self.skill_available(**id))
                    })
                    .take(40)
                    .map(|id| self.skill_line(*id))
                    .collect();
                Ok(if hits.is_empty() { "No skills match.".into() } else { hits.join("\n") })
            }
            "get_skill" => {
                let id = self.find_skill_id(input["query"].as_str().unwrap_or_default())?;
                let k = &self.kb.skills[&id];
                let mut out = format!(
                    "{} ({}, {}, {} SP, wit check: {})\nEffect: {}\nTrigger (uma.guide): {}\nDescription: {}\nRaw conditions: {} | pre: {}\nBase duration {}s, cooldown {}s\nSources:\n",
                    k.name,
                    k.tier(),
                    k.category,
                    k.sp,
                    k.wit_check,
                    k.effect_text(),
                    k.summary,
                    k.desc,
                    k.conditions,
                    k.pre,
                    k.duration,
                    k.cooldown
                );
                if k.grade != 0 {
                    out = out.replacen("\nEffect:", &format!(", {} evaluation points\nEffect:", k.grade), 1);
                }
                if let Some((tiers, t)) = self.kb.skill_tiers.as_ref().and_then(|x| Some((x, x.get(id)?))) {
                    out += &format!("{}\n", tiers.explain(t));
                }
                for src in self.kb.sources.get(&id).into_iter().flatten() {
                    let who = src
                        .uma()
                        .and_then(|u| self.kb.umas.get(&u).map(|i| format!("uma {} [{}]", i.name, i.outfit)))
                        .or_else(|| src.card().and_then(|c| self.kb.cards.get(&c).map(|i| format!("card {} {} [{}]", i.rarity, i.uma, i.title))))
                        .unwrap_or_default();
                    out += &format!("- {:?} {who}{}\n", src, if self.source_available(*src) { " (available to you)" } else { "" });
                }
                Ok(out)
            }
            "find_races" => {
                let q = input["query"].as_str().unwrap_or_default().to_lowercase();
                let mut seen = std::collections::HashSet::new();
                let hits: Vec<String> = self
                    .kb
                    .races
                    .iter()
                    .filter(|r| r.name.to_lowercase().contains(&q) || r.track.to_lowercase().contains(&q))
                    .filter(|r| seen.insert((r.name.clone(), r.distance)))
                    .take(25)
                    .map(|r| format!("{} ({}): {} {} {}m {} [{}] (race id {})", r.name, r.grade, r.track, r.surface, r.distance, r.direction, r.category, r.id))
                    .collect();
                Ok(if hits.is_empty() { "No race matches.".into() } else { hits.join("\n") })
            }
            "list_upcoming_events" => Ok(self.upcoming_text(int(input, "count").unwrap_or(4) as usize)),
            "get_cm_meta" => self.cm_meta_text(text(input, "cm"), input["style"].as_str()),
            "list_plans" => {
                let files = self.plan_files();
                Ok(if files.is_empty() { "No saved plans yet.".into() } else { files.iter().map(|(p, t)| format!("plans/{} — {t}", p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())).collect::<Vec<_>>().join("\n") })
            }
            "get_plan" => match text(input, "name").filter(|n| !n.eq_ignore_ascii_case("current")) {
                None => Ok(self.plan_brief()),
                Some(name) => {
                    let path = self.find_plan_file(name)?;
                    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                    Ok(clip(text.split(crate::tab_planner::PLAN_MARKER).next().unwrap_or("").to_string(), 16_000))
                }
            },
            "get_slot_ranking" => {
                let slot = (int(input, "slot").unwrap_or(1).clamp(1, 6) - 1) as usize;
                let top = int(input, "top_n").unwrap_or(5).clamp(1, 15) as usize;
                let picks = self.rank_now(slot);
                if picks.is_empty() {
                    return Ok("No candidates for that slot.".into());
                }
                let label = if slot == 5 { "the friend slot".to_string() } else { format!("slot {}", slot + 1) };
                let mut out = format!("Cards for {label} of the working plan, best first (model output: suggestions; scores are stat-point equivalents):\n");
                for (i, c) in picks.iter().take(top).enumerate() {
                    let stars = crate::meta::stars(c.meta);
                    out += &format!("{}. {}{} — score {:.0}: {}\n", i + 1, if stars.is_empty() { String::new() } else { format!("{stars} ") }, self.card_label(c.id), c.score, c.reason);
                }
                Ok(out)
            }
            "explain_card" => {
                let id = self.find_card_id(input["query"].as_str().unwrap_or_default())?;
                Ok(self.explain_card_text(id))
            }
            "get_affinity" => {
                let (trainee, legacies, _) = self.read_lineage(input)?;
                Ok(self.affinity_text(trainee, &legacies))
            }
            "parent_run_odds" => {
                let uma = self.find_uma_id(input["uma"].as_str().unwrap_or_default())?;
                let info = self.kb.umas.get(&uma).ok_or("Unknown uma.")?;
                let stars = self.owned_uma(uma).map(|u| u.stars).ok_or(format!("{} [{}] isn't owned, so she can't be trained.", info.name, info.outfit))?;
                // Her aptitudes at A or S after the start, given the lineage she'd run with.
                let mut grades: Vec<(&str, String)> = [
                    ("Turf", &info.aptitude.turf),
                    ("Dirt", &info.aptitude.dirt),
                    ("Sprint", &info.aptitude.sprint),
                    ("Mile", &info.aptitude.mile),
                    ("Medium", &info.aptitude.medium),
                    ("Long", &info.aptitude.long),
                    ("Front Runner", &info.aptitude.front),
                    ("Pace Chaser", &info.aptitude.pace),
                    ("Late Surger", &info.aptitude.late),
                    ("End Closer", &info.aptitude.end),
                ]
                .into_iter()
                .map(|(n, g)| (n, g.clone()))
                .collect();
                let mut lineage_note = String::new();
                if input["legacies"].is_array() {
                    let mut lin = input.clone();
                    lin["trainee"] = json!(input["uma"]);
                    let (_, _, sparks) = self.read_lineage(&lin)?;
                    let mut pinks: std::collections::BTreeMap<String, u32> = Default::default();
                    for s in sparks.iter().flatten().flatten().filter_map(|id| self.kb.sparks.get(id)).filter(|s| s.kind == "pink") {
                        *pinks.entry(s.name.clone()).or_default() += u32::from(s.stars);
                    }
                    for (n, g) in grades.iter_mut() {
                        let raised = crate::lineage::after_start(g, pinks.get(*n).copied().unwrap_or(0)).to_string();
                        if raised != *g {
                            lineage_note += &format!(" Her lineage raises {n} {g}→{raised} at the start, which adds it to her pink pool.");
                            *g = raised;
                        }
                    }
                }
                let a: Vec<&str> = grades.iter().filter(|(_, g)| crate::knowledge::grade_rank(g) >= 7).map(|(n, _)| *n).collect();
                let stats: [u32; 5] = std::array::from_fn(|i| input["final_stats"].get(i).and_then(Value::as_u64).unwrap_or(800) as u32);
                let ss = input["rank_ss"].as_bool().unwrap_or(true);
                let mut out = crate::lineage::parent_run_report(&format!("{} [{}]", info.name, info.outfit), stars, &a, stats, ss);
                out += &lineage_note;
                if input["final_stats"].is_null() {
                    out += " (Final stats assumed 800 each; pass final_stats for the blue odds of a planned build.)";
                }
                Ok(out)
            }
            "plan_lineage" => {
                let (trainee, legacies, sparks) = self.read_lineage(input)?;
                let lineage = self.kb.affinity.lineage(trainee, &legacies);
                let mut slots = Vec::new();
                for (i, l) in legacies.iter().enumerate() {
                    let (Some(l), Some(a)) = (l, &lineage.legacies[i]) else { continue };
                    let spark_list = |ids: &[u32]| ids.iter().filter_map(|id| self.kb.sparks.get(id)).collect::<Vec<_>>();
                    slots.push(crate::lineage::Slot { label: format!("Legacy {}: {}", i + 1, self.character_name(l.uma)), legacy: true, sparks: spark_list(&sparks[i][0]), affinity: a.total() });
                    for (j, gp) in l.parents.iter().enumerate() {
                        if let Some(gp) = gp {
                            slots.push(crate::lineage::Slot { label: format!("Legacy {}'s parent: {}", i + 1, self.character_name(gp.uma)), legacy: false, sparks: spark_list(&sparks[i][j + 1]), affinity: a.grandparent(j) });
                        }
                    }
                }
                let t = &input["target"];
                let target: Vec<&str> = [t["surface"].as_str(), t["distance"].as_str(), t["style"].as_str().map(|s| match s {
                    "Front" => "Front Runner",
                    "Pace" => "Pace Chaser",
                    "Late" => "Late Surger",
                    "End" => "End Closer",
                    other => other,
                })]
                .into_iter()
                .flatten()
                .collect();
                Ok(format!("{}\n\nLineage total affinity: {} {}.", crate::lineage::report(&self.kb, trainee, &slots, &target), lineage.total(), crate::affinity::symbol(lineage.total())))
            }
            "rank_legacies" => {
                let t = &self.kb.affinity;
                if t.is_empty() {
                    return Err("The affinity tables aren't loaded (knowledge/data/affinity.json).".into());
                }
                let trainee = self.find_character(input["trainee"].as_str().unwrap_or_default())?;
                let with = match input["with"].as_str() {
                    Some(w) => Some(self.find_character(w)?),
                    None => None,
                };
                let owned_only = input["owned_only"].as_bool().unwrap_or(false);
                let top = int(input, "top_n").unwrap_or(10).clamp(1, 30) as usize;
                if input["finished"].as_bool() == Some(true) {
                    let finished = self.veterans();
                    if finished.is_empty() {
                        return Err("No finished umas imported yet (Settings → Profile → Import finished umas).".into());
                    }
                    let mut rows: Vec<(u32, String)> = finished
                        .iter()
                        .filter(|v| crate::affinity::chara(v.card_id) != crate::affinity::chara(trainee))
                        .map(|v| {
                            let a = t.legacy(trainee, &v.legacy(&self.kb), None);
                            (a.total(), format!("#{} {} — affinity {} as a legacy ({} with the trainee, {} through its parents, +{} shared G1s). {}", v.trained_chara_id, self.kb.umas.get(&v.card_id).map_or_else(|| format!("uma {}", v.card_id), |u| format!("{} [{}]", u.name, u.outfit)), a.total(), a.direct, a.with_grandparent.iter().sum::<u32>(), a.grandparent_races.iter().sum::<u32>(), self.veteran_sparks(v)))
                        })
                        .collect();
                    rows.sort_by(|a, b| b.0.cmp(&a.0));
                    return Ok(rows.into_iter().take(top).map(|r| r.1).collect::<Vec<_>>().join("\n") + "\n(The other legacy adds its link and shared G1s on top: get_affinity.)");
                }
                let mut rows: Vec<(u32, u32, u32)> = t
                    .best_partners(trainee)
                    .into_iter()
                    .filter(|(c, _)| with.is_none_or(|w| crate::affinity::chara(w) != *c))
                    // The tables also hold characters not on Global yet.
                    .filter(|(c, _)| self.kb.umas.keys().any(|id| id / 100 == *c))
                    .filter(|(c, _)| !owned_only || self.owned_stars(*c).is_some())
                    .map(|(c, base)| (c, base, with.map_or(0, |w| t.pair(w, c))))
                    .collect();
                rows.sort_by(|a, b| (b.1 + b.2).cmp(&(a.1 + a.2)).then(a.0.cmp(&b.0)));
                let lines: Vec<String> = rows
                    .into_iter()
                    .take(top)
                    .map(|(c, base, w)| {
                        let owned = self.owned_stars(c).map_or(" — not owned".to_string(), |s| format!(" — owned, {s}★{}", if s >= 3 { "" } else { " (below 3★: no green spark)" }));
                        let with_part = with.map_or(String::new(), |wu| format!(" + {w} with {} = {}", self.character_name(wu), base + w));
                        format!("{}: {base} with {}{with_part}{owned}", self.character_name(c), self.character_name(trainee))
                    })
                    .collect();
                Ok(if lines.is_empty() { "No candidates.".into() } else { lines.join("\n") })
            }
            "list_finished_umas" => {
                let finished = self.veterans();
                if finished.is_empty() {
                    return Err("No finished umas imported yet (Settings → Profile → Import finished umas, from UmaExtractor's data.json).".into());
                }
                let uma = input["uma"].as_str().map(str::to_lowercase);
                let spark = input["spark"].as_str().map(str::to_lowercase);
                let kind = input["kind"].as_str();
                let min = int(input, "min_stars").unwrap_or(1) as u8;
                let top = int(input, "top_n").unwrap_or(20).clamp(1, 60) as usize;
                let mut list: Vec<&crate::veterans::Veteran> = finished
                    .iter()
                    .filter(|v| uma.as_ref().is_none_or(|q| self.kb.umas.get(&v.card_id).is_some_and(|u| format!("{} [{}]", u.name, u.outfit).to_lowercase().contains(q.as_str()))))
                    .filter(|v| {
                        (spark.is_none() && kind.is_none() && min <= 1)
                            || v.factors().iter().filter_map(|f| self.kb.sparks.get(f)).any(|s| {
                                spark.as_ref().is_none_or(|q| s.name.to_lowercase().contains(q.as_str())) && kind.is_none_or(|k| s.kind == k) && s.stars >= min
                            })
                    })
                    .collect();
                list.sort_by(|a, b| b.rank_score.cmp(&a.rank_score));
                let n = list.len();
                let lines: Vec<String> = list
                    .into_iter()
                    .take(top)
                    .map(|v| {
                        let parents: Vec<String> = [10, 20].into_iter().filter_map(|p| v.ancestor(p)).map(|a| self.kb.umas.get(&a.card_id).map_or_else(|| format!("uma {}", a.card_id), |u| u.name.clone())).collect();
                        format!(
                            "#{} {} — {} rank ({}), {} G1 wins, parents: {}. {}",
                            v.trained_chara_id,
                            self.kb.umas.get(&v.card_id).map_or_else(|| format!("uma {}", v.card_id), |u| format!("{} [{}]", u.name, u.outfit)),
                            v.rank_label(),
                            v.rank_score,
                            self.kb.g1_wins(&v.win_saddle_id_array).len(),
                            if parents.is_empty() { "none recorded".into() } else { parents.join(" and ") },
                            self.veteran_sparks(v)
                        )
                    })
                    .collect();
                Ok(format!("{n} match ({} imported).\n{}", finished.len(), lines.join("\n")))
            }
            "recommend_trainees" => {
                let top = int(input, "top_n").unwrap_or(5).clamp(1, 12) as usize;
                let p = &self.planner.plan;
                let meta = self.plan_meta_trainees();
                let rows: Vec<String> = self
                    .trainee_candidates()
                    .into_iter()
                    .take(top)
                    .filter_map(|id| {
                        let (u, info) = (self.owned_uma(id)?, self.kb.umas.get(&id)?);
                        let a = &info.aptitude;
                        let low: Vec<&str> = [(p.distance.label(), a.distance(p.distance)), (p.surface.label(), a.surface(p.surface)), (p.style.label(), a.style(p.style))]
                            .into_iter()
                            .filter(|(_, g)| grade_rank(g) < 7)
                            .map(|(l, _)| l)
                            .collect();
                        let warn = if low.is_empty() { String::new() } else { format!(" ⚠ below A for {} (slower there)", low.join(", ")) };
                        Some(format!(
                            "{}{} [{}] {}★ P{} — {} {}, {} {}, {} {}{warn}",
                            if meta.contains(&id) { "★ meta sheet pick: " } else { "" },
                            u.name,
                            u.outfit,
                            u.stars,
                            u.potential,
                            p.distance.label(),
                            a.distance(p.distance),
                            p.surface.label(),
                            a.surface(p.surface),
                            p.style.label(),
                            a.style(p.style)
                        ))
                    })
                    .collect();
                let goal = format!("for {} {}, {}", p.distance.label(), p.surface.label(), p.style.label());
                Ok(if rows.is_empty() { "The roster has no umas.".into() } else { format!("Best owned trainees {goal}:\n{}", rows.join("\n")) })
            }
            "list_notes" => {
                let root = self.root.as_ref().ok_or("umaLord folder not found")?;
                let mut files = vec!["insights.md".to_string(), "sources.md".to_string(), "knowledge/mechanics.md".to_string(), "knowledge/racecourses.md".to_string(), "knowledge/skills.md".to_string()];
                for dir in ["plans", "results", "account"] {
                    for e in std::fs::read_dir(root.join(dir)).into_iter().flatten().flatten() {
                        let n = e.file_name().to_string_lossy().to_string();
                        if n.ends_with(".md") && !n.starts_with('_') {
                            files.push(format!("{dir}/{n}"));
                        }
                    }
                }
                Ok(files.join("\n"))
            }
            "read_note" => {
                let root = self.root.as_ref().ok_or("umaLord folder not found")?;
                let rel = input["path"].as_str().unwrap_or_default().replace('\\', "/");
                let allowed = rel.ends_with(".md")
                    && !rel.contains("..")
                    && (["insights.md", "sources.md", "CLAUDE.md"].contains(&rel.as_str()) || ["plans/", "results/", "account/", "knowledge/"].iter().any(|d| rel.starts_with(d)));
                if !allowed {
                    return Err(format!("Not a readable note: {rel}"));
                }
                let text = std::fs::read_to_string(root.join(&rel)).map_err(|e| format!("{rel}: {e}"))?;
                Ok(clip(text.split("<!-- umalord-plan").next().unwrap_or("").to_string(), 20_000))
            }
            "glossary" => {
                let q = input["term"].as_str().unwrap_or_default().to_lowercase();
                let hits: Vec<String> = self
                    .kb
                    .glossary
                    .iter()
                    .filter(|g| g.term.to_lowercase().contains(&q) || g.aliases.iter().any(|a| a.to_lowercase().contains(&q)))
                    .take(8)
                    .map(|g| format!("{}: {}", g.term, g.description))
                    .collect();
                Ok(if hits.is_empty() { "Not in the glossary.".into() } else { hits.join("\n") })
            }
            other => Err(format!("Unknown tool {other}")),
        }
    }

    fn account_summary(&self) -> String {
        use std::fmt::Write;
        let r = &self.roster;
        let mut o = String::new();
        let _ = writeln!(o, "Profile: {}; server: {}", r.profile.as_deref().unwrap_or("(no name)"), r.server.as_deref().unwrap_or("Global"));
        if r.umas.is_empty() && r.cards.is_empty() {
            o.push_str("The roster is empty: point the user to ⚙ Settings → Profile → Set up my collection, or Load a profile.\n");
            return o;
        }
        let _ = writeln!(o, "{} and {}", plural(r.umas.len(), "uma"), plural(r.cards.len(), "support card"));
        let stars: Vec<String> = (1..=5u8).rev().map(|s| (s, r.umas.iter().filter(|u| u.stars == s).count())).filter(|(_, n)| *n > 0).map(|(s, n)| format!("{s}★ {n}")).collect();
        let p5 = r.umas.iter().filter(|u| u.potential >= 5).count();
        let _ = writeln!(o, "Umas by stars: {}; at potential 5: {p5}", stars.join(", "));
        let mut kinds: Vec<&str> = r.cards.iter().map(|c| c.card_type.label()).collect();
        kinds.sort_unstable();
        kinds.dedup();
        for k in kinds {
            let of: Vec<&crate::data::SupportCard> = r.cards.iter().filter(|c| c.card_type.label() == k).collect();
            let mlb_ssr: Vec<String> = of.iter().filter(|c| c.rarity == crate::data::Rarity::SSR && c.is_mlb()).map(|c| c.uma.clone()).collect();
            let _ = writeln!(
                o,
                "{k}: {} cards, {} SSR, {} maxed (MLB){}",
                of.len(),
                of.iter().filter(|c| c.rarity == crate::data::Rarity::SSR).count(),
                of.iter().filter(|c| c.is_mlb()).count(),
                if mlb_ssr.is_empty() { String::new() } else { format!("; MLB SSRs: {}", mlb_ssr.join(", ")) }
            );
        }
        let _ = writeln!(o, "Saved parents: {}; saved plans: {}", self.saved_parents().len(), self.plan_files().len());
        let p = &self.planner.plan;
        let trainee = p.trainee.map_or("no trainee".to_string(), |t| self.uma_label(t));
        let _ = writeln!(o, "Working plan in the Planner: {} — {}, {}", if p.title.is_empty() { "untitled" } else { &p.title }, p.purpose.label(), trainee);
        o
    }

    fn upcoming_text(&self, n: usize) -> String {
        let now = today();
        let cms = self.kb.upcoming_cms(&now);
        if cms.is_empty() {
            return "No upcoming Champions Meeting in the schedule.".into();
        }
        let mut out = String::from("Upcoming Champions Meetings (the app's live schedule: announced = official and final, estimated = Umalator's guess, may change):\n");
        for e in cms.into_iter().take(n.max(1)) {
            let meta = self.kb.pvp_meta.as_ref().and_then(|m| m.events.iter().find(|x| x.series == "CM" && x.number == e.id));
            let styles = meta.map_or("not on the meta sheet yet".to_string(), |m| {
                let mut s: Vec<&String> = m.styles.keys().collect();
                s.sort();
                format!("meta sheet covers {}", s.iter().map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
            });
            let round1 = e.round1.map_or(String::new(), |t| format!("; Round 1 {}", crate::live::fmt_local(t)));
            out += &format!(
                "- CM {} {}: {} ({}{round1}) · {} · {} · scenario {} · turns off: {} · {styles}\n",
                e.id,
                e.name,
                e.date,
                if e.announced { "announced" } else { "estimated" },
                e.course_name,
                e.conditions,
                self.kb.cm_scenario(e).unwrap_or("unknown"),
                self.skill_names(&e.disabled_skills)
            );
        }
        out
    }

    fn cm_meta_text(&self, cm: Option<&str>, style: Option<&str>) -> Result<String, String> {
        let meta = self.kb.pvp_meta.as_ref().ok_or("No meta sheet data: run tools/update_knowledge.py --only meta.")?;
        let number = match cm {
            Some(q) => self.find_event(q)?.id,
            None => self.plan_cm_number().or_else(|| self.kb.upcoming_cms(&today()).first().map(|e| e.id)).ok_or("No CM given and none upcoming.")?,
        };
        let ev = meta.events.iter().find(|e| e.series == "CM" && e.number == number).ok_or_else(|| format!("The meta sheet has nothing for CM {number} yet."))?;
        let mut out = format!(
            "Community meta sheet ({}), fetched {}: CM {} {}. Community opinion, not game data. {}\nStyle ranking: {}\nRecommended stats (Speed/Stamina/Power/Guts/Wit): {}\n",
            meta.authors,
            meta.fetched,
            ev.number,
            ev.name,
            meta.caveat,
            ev.style_ranking,
            ev.stats.join(" / ")
        );
        if !ev.analysis.is_empty() {
            out += &format!("Analysis: {}\n", clip(ev.analysis.clone(), 700));
        }
        let mut styles: Vec<&String> = ev.styles.keys().collect();
        styles.sort_by_key(|s| ev.style_ranking.find(s.as_str()).unwrap_or(99));
        let want = style.map(|s| s.trim_end_matches(" Runner").trim_end_matches(" Chaser").trim_end_matches(" Surger").trim_end_matches(" Closer").to_string());
        for name in styles {
            if want.as_ref().is_some_and(|w| !w.eq_ignore_ascii_case(name)) {
                continue;
            }
            let st = &ev.styles[name];
            out += &format!("\n## {name}\n");
            let umas: Vec<String> = st
                .umas
                .iter()
                .map(|u| {
                    let owned = u.id.and_then(|id| self.owned_uma(id)).map_or(String::new(), |o| format!(" (owned {}★ P{})", o.stars, o.potential));
                    format!("{}{}{owned}", u.name, if u.rating.is_empty() { String::new() } else { format!(" {}", u.rating) })
                })
                .collect();
            out += &format!("Trainees: {}\n", umas.join("; "));
            let card = |c: &crate::meta::MetaCard| match c.id {
                Some(id) => {
                    let lb = c.min_lb.map_or(String::new(), |m| format!(", needs LB{m}+{}", if c.alt.is_empty() { String::new() } else { format!(" ({})", c.alt) }));
                    let note = if c.note.is_empty() { String::new() } else { format!(" — {}", c.note) };
                    format!("{}{lb}{note}", self.card_label(id))
                }
                None if !c.any.is_empty() => format!("any {} card", c.any),
                None => format!("{} (not on Global)", c.name),
            };
            for tier in crate::meta::TIERS {
                if let Some(d) = st.decks.get(tier) {
                    let cards: Vec<String> = d.cards.iter().map(card).collect();
                    let borrow = d.borrow.as_ref().map_or(String::new(), |b| format!(" | borrow: {}", card(b)));
                    out += &format!("{tier} deck: {}{borrow}\n", cards.join(" · "));
                }
            }
            if !st.honorable.is_empty() {
                out += &format!("Honorable mentions: {}\n", st.honorable.iter().map(card).collect::<Vec<_>>().join(" · "));
            }
            if !st.skills.is_empty() {
                out += &format!("Skills it recommends: {}\n", st.skills.iter().map(|s| self.kb.skill_name(s.id)).collect::<Vec<_>>().join(", "));
            }
            if !st.accel.is_empty() {
                out += &format!("Acceleration advice: {}\n", st.accel);
            }
            if !st.advice.is_empty() {
                out += &format!("Skill advice as written: {}\n", clip(st.advice.clone(), 600));
            }
            if !st.parents.is_empty() {
                out += &format!("Parents to look for: {}\n", st.parents.iter().map(|p| format!("{} ({})", p.uma, self.kb.skill_name(p.skill))).collect::<Vec<_>>().join("; "));
            }
        }
        Ok(clip(out, 10_000))
    }

    /// Saved plans, newest first (read from disk, so the MCP server sees the window's saves).
    pub(crate) fn plan_files(&self) -> Vec<(PathBuf, String)> {
        let mut files = Vec::new();
        let Some(root) = &self.root else { return files };
        for entry in std::fs::read_dir(root.join("plans")).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if !name.ends_with(".md") || name.starts_with('_') {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let title = text.lines().find_map(|l| l.strip_prefix("# ")).unwrap_or(&name).to_string();
            files.push((path, title));
        }
        files.sort_by(|a, b| b.0.cmp(&a.0));
        files
    }

    fn find_plan_file(&self, q: &str) -> Result<PathBuf, String> {
        let ql = q.to_lowercase();
        let files = self.plan_files();
        files
            .iter()
            .find(|(p, t)| p.file_name().is_some_and(|f| f.to_string_lossy().to_lowercase().contains(&ql)) || t.to_lowercase().contains(&ql))
            .map(|(p, _)| p.clone())
            .ok_or_else(|| format!("No saved plan matches '{q}'. list_plans lists them."))
    }

    /// The ranking for one slot (0..=4 own, 5 friend), worked out now (about 40 ms).
    fn rank_now(&self, slot: usize) -> Vec<crate::recommend::CardPick> {
        let p = &self.planner.plan;
        let deck = [p.deck[0], p.deck[1], p.deck[2], p.deck[3], p.deck[4], p.friend];
        crate::recommend::rank_slot(&self.rank_context(), &deck, slot)
    }

    fn explain_card_text(&self, id: u32) -> String {
        let p = &self.planner.plan;
        let owned = self.owned_card(id).is_some();
        let slot = if p.friend == Some(id) {
            5
        } else if let Some(i) = p.deck.iter().position(|d| *d == Some(id)) {
            i
        } else if owned {
            p.deck.iter().position(Option::is_none).unwrap_or(0)
        } else {
            5
        };
        let picks = self.rank_now(slot);
        let where_ = if slot == 5 { "the friend slot".to_string() } else { format!("slot {}", slot + 1) };
        let mut out = format!("{} for {where_} of the working plan (model output: a suggestion):\n", self.card_label(id));
        match picks.iter().position(|c| c.id == id) {
            Some(rank) => {
                let c = &picks[rank];
                out += &format!("Rank {} of {}, score {:.0} stat points{}.\nWhy: {}\n", rank + 1, picks.len(), c.score, if c.meta > 0.0 { format!(", meta sheet {}", crate::meta::stars(c.meta)) } else { String::new() }, c.reason);
                if rank > 0 {
                    out += &format!("Best for this slot: {} (score {:.0}): {}\n", self.card_label(picks[0].id), picks[0].score, picks[0].reason);
                }
            }
            None => out += "Not a candidate for this slot: it's the trainee's own character, another card of the same character is in the deck, or it isn't owned (own slots take owned cards only).\n",
        }
        let kind = self.kb.cards.get(&id).map_or(String::new(), |c| c.kind.clone());
        let (w, why) = self.plan_card_meta().of(id, slot == 5, &kind);
        if w > 0.0 {
            out += &format!("Meta sheet: {} ({why}). Community opinion.\n", crate::meta::stars(w));
        } else if self.plan_meta_label().is_some() {
            out += "The meta sheet doesn't list it for this CM and style.\n";
        }
        out
    }

    // -----------------------------------------------------------------------
    // Running a tool

    /// Validate and run one tool call.
    pub(crate) fn agent_call(&mut self, ctx: &egui::Context, name: &str, input: &Value) -> Exec {
        let Some(spec) = super::find(name) else { return Exec::Done(ToolOut::err(format!("Unknown tool {name}"))) };
        let input = if input.is_null() { json!({}) } else { input.clone() };
        if let Err(e) = super::validate(&spec.schema, &input, "") {
            return Exec::Done(ToolOut::err(format!("Invalid input: {e}")));
        }
        let first = |t: &str| t.lines().find(|l| !l.trim().is_empty()).map(|l| crate::ui_util::truncate(l, 110)).unwrap_or_default();
        match spec.kind {
            Kind::Read => Exec::Done(match self.run_tool(spec.name, &input) {
                Ok(t) => {
                    let s = first(&t);
                    ToolOut::ok(t, s)
                }
                Err(e) => ToolOut::err(e),
            }),
            Kind::Draft => Exec::Done(self.draft_tool(spec.name, &input)),
            Kind::Ui => Exec::Done(self.ui_tool(spec.name, &input)),
            Kind::Long => self.long_tool(ctx, spec.name, &input),
            Kind::Write => match self.approval(spec.name, &input) {
                Ok(a) => Exec::Ask(a),
                Err(e) => Exec::Done(ToolOut::err(e)),
            },
        }
    }

    /// Background jobs keep going while the caller waits: collect what finished.
    pub(crate) fn agent_poll_jobs(&mut self, ctx: &egui::Context) {
        self.sim_poll(ctx);
        self.planner_poll(ctx);
        self.pick_poll(ctx);
    }

    /// Stop a job the caller no longer waits for.
    pub(crate) fn agent_cancel(&mut self, w: Wait) {
        match w {
            Wait::Pick { .. } => {
                if let Some(j) = &self.planner.pick_job {
                    j.cancel();
                }
            }
            Wait::SkillValues { .. } => {
                if let Some(j) = &self.planner.skill_job {
                    j.cancel();
                }
            }
            Wait::Compare { .. } | Wait::Chart { .. } | Wait::Stamina { .. } | Wait::Extra(_) => self.cancel_sim_jobs(),
        }
    }

    /// Run a Long tool to the end, polling: the MCP server and tests use this.
    pub(crate) fn agent_call_blocking(&mut self, ctx: &egui::Context, name: &str, input: &Value, limit: std::time::Duration) -> Exec {
        match self.agent_call(ctx, name, input) {
            Exec::Wait(w) => {
                let start = std::time::Instant::now();
                loop {
                    self.agent_poll_jobs(ctx);
                    if let Some(out) = self.agent_wait(w) {
                        return Exec::Done(out);
                    }
                    if start.elapsed() > limit {
                        self.agent_cancel(w);
                        return Exec::Done(ToolOut::err("The simulation took too long and was stopped. Try fewer runs."));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
            }
            other => other,
        }
    }

    // -----------------------------------------------------------------------
    // Undo

    fn sim_setup(&self) -> SimSetup {
        let s = &self.sim;
        SimSetup { course: s.course, ground: s.ground, weather: s.weather, season: s.season, time: s.time, umas: s.umas.clone(), opts: s.opts.clone(), from_plan: s.from_plan }
    }

    pub(crate) fn restore_sim(&mut self, s: SimSetup) {
        let sim = &mut self.sim;
        (sim.course, sim.ground, sim.weather, sim.season, sim.time) = (s.course, s.ground, s.weather, s.season, s.time);
        sim.umas = s.umas;
        sim.opts = s.opts;
        sim.from_plan = s.from_plan;
    }

    /// Remember the working plan and the sim setup before a change; returns the snapshot's index.
    pub(crate) fn agent_snapshot(&mut self, label: impl Into<String>) -> usize {
        let snap = Snapshot { label: label.into(), plan: self.planner.plan.clone(), open_file: self.planner.open_file.clone(), sim: self.sim_setup() };
        self.agent.undo.push(snap);
        self.agent.undo.len() - 1
    }

    /// Put back what the chat changed from snapshot `idx` on (that change and every later one).
    pub(crate) fn agent_undo(&mut self, idx: usize) -> Result<String, String> {
        let snap = self.agent.undo.get(idx).cloned().ok_or("That change was already undone.")?;
        let later = self.agent.undo.len() - idx - 1;
        self.agent.undo.truncate(idx);
        self.planner.plan = snap.plan;
        self.planner.open_file = snap.open_file;
        self.planner.skill_status = None;
        self.restore_sim(snap.sim);
        Ok(if later == 0 { format!("Undid: {}.", snap.label) } else { format!("Undid: {} and {} after it.", snap.label, plural(later, "change")) })
    }

    /// Run `f` as one undoable change: a snapshot first, put back if `f` fails.
    fn drafted(&mut self, label: &str, f: impl FnOnce(&mut App) -> Result<ToolOut, String>) -> ToolOut {
        let idx = self.agent_snapshot(label);
        match f(self) {
            Ok(out) => {
                self.after_draft();
                out.undo(Some(idx))
            }
            Err(e) => {
                let _ = self.agent_undo(idx);
                ToolOut::err(e)
            }
        }
    }

    /// The MCP server keeps its draft where the window can open it.
    fn after_draft(&mut self) {
        if self.agent.bridge.is_some() {
            self.write_bridge_draft();
        }
    }

    // -----------------------------------------------------------------------
    // Draft tools

    fn draft_tool(&mut self, name: &str, input: &Value) -> ToolOut {
        match name {
            "new_plan" => self.tool_new_plan(input),
            "set_plan" => self.tool_set_plan(input),
            "recommend_deck" => {
                let all = input["replace_all"].as_bool().unwrap_or(true);
                self.drafted("deck recommendation", |a| {
                    let picks = a.recommend_deck(all);
                    let msg = a.deck_fill_message(&picks, "Picked");
                    let mut text = format!("{msg}\n");
                    for c in &picks {
                        let stars = crate::meta::stars(c.meta);
                        text += &format!("- {}{} — score {:.0}: {}\n", if stars.is_empty() { String::new() } else { format!("{stars} ") }, a.card_label(c.id), c.score, c.reason);
                    }
                    text += &format!("\n{}", a.plan_brief());
                    let p = &a.planner.plan;
                    let deck = Visual::Deck { cards: p.deck.iter().flatten().copied().collect(), friend: p.friend };
                    Ok(ToolOut::ok(text, msg).visual(deck).open(OpenAt::Plan))
                })
            }
            "set_deck" => self.tool_set_deck(input),
            "set_skills" => self.tool_set_skills(input),
            "load_plan_into_sim" => self.drafted("plan loaded into the Race sim", |a| {
                let msg = a.load_plan_into_sim()?;
                Ok(ToolOut::ok(format!("{msg}\n{}", a.sim_builds_text()), msg).open(OpenAt::Race))
            }),
            "set_sim_build" => self.tool_set_sim_build(input),
            other => ToolOut::err(format!("Unknown tool {other}")),
        }
    }

    /// An owned uma for the plan.
    fn owned_trainee(&self, q: &str) -> Result<u32, String> {
        let id = self.find_uma_id(q)?;
        if self.owned_uma(id).is_none() {
            return Err(format!("{} isn't in the account's roster; the Planner trains owned umas only.", self.uma_label(id)));
        }
        Ok(id)
    }

    fn tool_new_plan(&mut self, input: &Value) -> ToolOut {
        let purpose = match input["goal"].as_str() {
            Some(g) => Purpose::ALL.into_iter().find(|p| p.label() == g).unwrap_or_default(),
            None => Purpose::ChampionsMeeting,
        };
        let cm = if purpose == Purpose::ChampionsMeeting {
            match self.find_event(text(input, "cm").unwrap_or("next")) {
                Ok(e) => Some(e),
                Err(e) => return ToolOut::err(e),
            }
        } else {
            None
        };
        let trainee = match text(input, "trainee").map(|t| self.owned_trainee(t)) {
            Some(Err(e)) => return ToolOut::err(e),
            Some(Ok(id)) => Some(id),
            None => None,
        };
        if let Some(s) = text(input, "scenario")
            && let Err(e) = self.check_scenario(s)
        {
            return ToolOut::err(e);
        }
        self.drafted("new plan", |a| {
            let mut plan = Plan { purpose, ..Plan::default() };
            let mut notes = Vec::new();
            if let Some(e) = &cm
                && let Some(n) = plan.apply_cm(&a.kb, e)
            {
                notes.push(n);
            }
            if let Some(st) = parse_style(&input["style"]) {
                plan.style = st;
            } else if let Some(info) = trainee.and_then(|t| a.kb.umas.get(&t)) {
                plan.style = info.aptitude.best_style();
            }
            if let Some(s) = text(input, "scenario") {
                plan.scenario = s.to_string();
            }
            if let Some(t) = text(input, "title") {
                plan.title = t.to_string();
            }
            plan.trainee = trainee;
            a.planner.plan = plan;
            a.planner.open_file = None;
            a.planner.skill_status = None;
            let what = cm.as_ref().map_or(purpose.label().to_string(), |e| format!("{} · {}", e.name, crate::live::short_day(&e.date)));
            let summary = format!("New plan: {what} · {}", a.planner.plan.style.label());
            let mut text = a.plan_brief();
            if !notes.is_empty() {
                text += &format!("Notes: {}\n", notes.join("; "));
            }
            Ok(ToolOut::ok(text, summary).open(OpenAt::Plan))
        })
    }

    fn check_scenario(&self, s: &str) -> Result<(), String> {
        let names: Vec<String> = self.kb.scenarios.as_ref().map(|x| x.scenarios.iter().filter(|x| x.global).map(|x| x.name.clone()).collect()).unwrap_or_default();
        if names.is_empty() || names.iter().any(|n| n == s) { Ok(()) } else { Err(format!("Unknown scenario '{s}'. Global scenarios: {}.", names.join(", "))) }
    }

    fn tool_set_plan(&mut self, input: &Value) -> ToolOut {
        let mut p = self.planner.plan.clone();
        let mut changed: Vec<&str> = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        if let Some(q) = text(input, "trainee") {
            match self.owned_trainee(q) {
                Ok(id) => {
                    p.trainee = Some(id);
                    changed.push("trainee");
                }
                Err(e) => errors.push(e),
            }
        }
        if let Some(s) = parse_style(&input["style"]) {
            p.style = s;
            changed.push("style");
        }
        if let Some(d) = parse_distance(&input["distance"]) {
            p.distance = d;
            changed.push("distance");
        }
        if let Some(s) = parse_surface(&input["surface"]) {
            p.surface = s;
            changed.push("surface");
        }
        if let Some(q) = text(input, "course") {
            if q.eq_ignore_ascii_case("none") {
                p.course = None;
            } else {
                let ql = q.to_lowercase();
                let race = q
                    .parse::<u32>()
                    .ok()
                    .and_then(|id| self.kb.races.iter().find(|r| r.id == id))
                    .or_else(|| self.kb.races.iter().find(|r| r.name.to_lowercase() == ql))
                    .or_else(|| self.kb.races.iter().find(|r| r.name.to_lowercase().contains(&ql)));
                match race {
                    Some(r) => {
                        p.course = Some(r.id);
                        p.distance = crate::tab_planner::race_distance(&r.category, r.distance);
                        p.surface = if r.surface == "Dirt" { Surface::Dirt } else { Surface::Turf };
                    }
                    None => errors.push(format!("No race matches '{q}'; find_races lists them.")),
                }
            }
            // Picking a course by hand unlinks the CM, as in the Planner.
            (p.cm_date, p.cm_name, p.umalator_course) = (None, None, None);
            changed.push("course");
        }
        if let Some(q) = text(input, "cm") {
            if q.eq_ignore_ascii_case("none") {
                (p.cm_date, p.cm_name, p.umalator_course) = (None, None, None);
            } else {
                match self.find_event(q) {
                    Ok(e) => {
                        if let Some(n) = p.apply_cm(&self.kb, &e) {
                            notes.push(n);
                        }
                    }
                    Err(e) => errors.push(e),
                }
            }
            changed.push("Champions Meeting");
        }
        for (key, group) in [("ground", &crate::tab_planner::GROUNDS), ("weather", &crate::tab_planner::WEATHERS), ("season", &crate::tab_planner::SEASONS)] {
            if let Some(v) = text(input, key) {
                if !crate::tab_planner::has_condition(&p.conditions, v) {
                    p.conditions = crate::tab_planner::set_condition(&p.conditions, group, v);
                }
                changed.push("conditions");
            }
        }
        if let Some(stats) = input["target_stats"].as_array() {
            for (i, v) in stats.iter().take(5).enumerate() {
                p.target[i] = v.as_u64().unwrap_or(0) as u32;
            }
            changed.push("target stats");
        }
        if input["parents"].is_array() {
            let names = texts(input, "parents");
            p.parents = [names.first().cloned().unwrap_or_default(), names.get(1).cloned().unwrap_or_default()];
            changed.push("parents");
        }
        if let Some(n) = input["notes"].as_str() {
            p.notes = n.to_string();
            changed.push("notes");
        }
        if let Some(t) = text(input, "title") {
            p.title = t.to_string();
            changed.push("title");
        }
        if let Some(s) = text(input, "scenario") {
            match self.check_scenario(s) {
                Ok(()) => p.scenario = s.to_string(),
                Err(e) => errors.push(e),
            }
            changed.push("scenario");
        }
        if let Some(b) = int(input, "sp_budget") {
            p.sp_budget = (b > 0).then_some(b as u32);
            changed.push("SP budget");
        }
        if let Some(m) = input["meta_influence"].as_str() {
            use crate::skill_opt::MetaInfluence;
            p.meta_influence = match m {
                "off" => MetaInfluence::Off,
                "strong" => MetaInfluence::Strong,
                _ => MetaInfluence::TieBreak,
            };
            changed.push("meta influence");
        }
        for (key, label) in [("pin_skills", "pinned skills"), ("ban_skills", "banned skills")] {
            if input[key].is_array() {
                match self.find_skills(&texts(input, key)) {
                    Ok(ids) => {
                        if key == "pin_skills" {
                            p.pinned_skills = ids;
                        } else {
                            p.banned_skills = ids;
                        }
                    }
                    Err(e) => errors.push(e),
                }
                changed.push(label);
            }
        }
        if let Some(lb) = int(input, "friend_lb") {
            p.friend_lb = lb.min(4) as u8;
            changed.push("friend LB");
        }
        if let Some(f) = input["fast_learner"].as_bool() {
            p.fast_learner = f;
            changed.push("Fast Learner");
        }
        if !errors.is_empty() {
            return ToolOut::err(format!("Nothing changed: {}", errors.join(" ")));
        }
        if changed.is_empty() {
            return ToolOut::err("Nothing to change: give at least one field.");
        }
        changed.dedup();
        self.drafted("plan change", |a| {
            a.planner.plan = p;
            if let Some(t) = a.planner.plan.trainee
                && let Some(name) = a.kb.umas.get(&t).map(|u| u.name.clone())
                && a.planner.plan.deck.iter().flatten().any(|c| a.kb.cards.get(c).is_some_and(|ci| ci.uma == name))
            {
                notes.push(format!("the deck has a card of {name}, the trainee's own character, which can't be used: change the deck"));
            }
            let mut text = a.plan_brief();
            if !notes.is_empty() {
                text += &format!("Notes: {}\n", notes.join("; "));
            }
            Ok(ToolOut::ok(text, format!("Set {}", changed.join(", "))).open(OpenAt::Plan))
        })
    }

    fn tool_set_deck(&mut self, input: &Value) -> ToolOut {
        let mut errors = Vec::new();
        let mut ids = Vec::new();
        for q in texts(input, "cards") {
            match self.find_card_id(&q) {
                Ok(id) if self.owned_card(id).is_none() => errors.push(format!("{} isn't owned; only the friend slot takes a borrowed card.", self.card_label(id))),
                Ok(id) => ids.push(id),
                Err(e) => errors.push(e),
            }
        }
        let friend = match text(input, "friend") {
            Some(q) if q.eq_ignore_ascii_case("none") => Some(None),
            Some(q) => match self.find_card_id(q) {
                Ok(id) => Some(Some(id)),
                Err(e) => {
                    errors.push(e);
                    None
                }
            },
            None => None,
        };
        let trainee = self.planner.plan.trainee.and_then(|t| self.kb.umas.get(&t)).map(|u| u.name.clone());
        let mut seen: Vec<String> = Vec::new();
        for id in ids.iter().chain(friend.flatten().iter()) {
            let Some(c) = self.kb.cards.get(id) else { continue };
            if trainee.as_deref() == Some(c.uma.as_str()) {
                errors.push(format!("{} is the trainee's own character, which can't be in her deck.", self.card_label(*id)));
            }
            if seen.contains(&c.uma) {
                errors.push(format!("Two cards of {}: a deck holds one card per character.", c.uma));
            }
            seen.push(c.uma.clone());
        }
        if !errors.is_empty() {
            return ToolOut::err(format!("Nothing changed: {}", errors.join(" ")));
        }
        let friend_lb = int(input, "friend_lb");
        self.drafted("deck change", |a| {
            let p = &mut a.planner.plan;
            p.deck = [None; 5];
            for (i, id) in ids.iter().take(5).enumerate() {
                p.deck[i] = Some(*id);
            }
            if let Some(f) = friend {
                p.friend = f;
            }
            if let Some(lb) = friend_lb {
                p.friend_lb = lb.min(4) as u8;
            }
            let deck = Visual::Deck { cards: p.deck.iter().flatten().copied().collect(), friend: p.friend };
            Ok(ToolOut::ok(a.plan_brief(), format!("Deck set: {}{}", plural(ids.len(), "card"), if a.planner.plan.friend.is_some() { " + friend" } else { "" })).visual(deck).open(OpenAt::Plan))
        })
    }

    fn tool_set_skills(&mut self, input: &Value) -> ToolOut {
        let ids = match self.find_skills(&texts(input, "skills")) {
            Ok(ids) => ids,
            Err(e) => return ToolOut::err(format!("Nothing changed: {e}")),
        };
        let mode = input["mode"].as_str().unwrap_or("replace").to_string();
        self.drafted("skills change", |a| {
            let skills = &mut a.planner.plan.skills;
            match mode.as_str() {
                "add" => {
                    for id in &ids {
                        if !skills.contains(id) {
                            skills.push(*id);
                        }
                    }
                }
                "remove" => skills.retain(|s| !ids.contains(s)),
                _ => *skills = ids.clone(),
            }
            let chosen = a.planner.plan.skills.clone();
            let mut notes = Vec::new();
            let offered = a.pick_rows();
            let off: Vec<String> = chosen.iter().filter(|s| !offered.contains_key(s)).map(|s| a.kb.skill_name(*s)).collect();
            if !off.is_empty() {
                notes.push(format!("not offered by the trainee, deck or parents: {}", off.join(", ")));
            }
            let disabled = a.plan_cm().map(|e| e.disabled_skills.clone()).unwrap_or_default();
            let dead: Vec<String> = chosen.iter().filter(|s| disabled.contains(s)).map(|s| a.kb.skill_name(*s)).collect();
            if !dead.is_empty() {
                notes.push(format!("turned off in this CM: {}", dead.join(", ")));
            }
            let career = a.plan_career();
            let cost = a.chosen_cost(&chosen, career.as_ref());
            if let Some(b) = a.pick_budget()
                && cost > b + 1.0
            {
                notes.push(format!("they cost about {cost:.0} SP, over the {b:.0} SP budget"));
            }
            let mut text = format!("Skills to buy: {} (about {cost:.0} SP).\n", a.skill_names(&chosen));
            if !notes.is_empty() {
                text += &format!("Warnings: {}.\n", notes.join("; "));
            }
            Ok(ToolOut::ok(text, format!("{} to buy, about {cost:.0} SP", plural(chosen.len(), "skill"))).visual(Visual::Skills(chosen)).open(OpenAt::Plan))
        })
    }

    fn sim_builds_text(&self) -> String {
        let data = self.sim_data.as_ref();
        let mut out = format!(
            "Race sim: {}, {}\n",
            data.map_or(format!("course {}", self.sim.course), |d| d.course_name(self.sim.course)),
            crate::tab_sim::conditions_text(self.sim.ground, self.sim.weather, self.sim.season, self.sim.time)
        );
        for (k, u) in self.sim.umas.iter().enumerate() {
            let apt = |i: usize| crate::sim::params::APTITUDES[i.min(7)];
            out += &format!(
                "Uma {}: {}, {}/{}/{}/{}/{} {}, aptitudes {}/{}/{}, skills: {}\n",
                k + 1,
                u.outfit.map_or("no outfit".to_string(), |o| self.uma_label(o)),
                u.desc.speed,
                u.desc.stamina,
                u.desc.power,
                u.desc.guts,
                u.desc.wisdom,
                crate::sim::params::strategy_name(u.desc.strategy),
                apt(u.desc.distance_apt),
                apt(u.desc.surface_apt),
                apt(u.desc.strategy_apt),
                self.skill_names(&u.skills)
            );
        }
        out
    }

    fn tool_set_sim_build(&mut self, input: &Value) -> ToolOut {
        let i = (int(input, "uma").unwrap_or(1).clamp(1, 2) - 1) as usize;
        let outfit = match text(input, "outfit").map(|q| self.find_uma_id(q)) {
            Some(Err(e)) => return ToolOut::err(e),
            Some(Ok(id)) => Some(id),
            None => None,
        };
        let mut lists = Vec::new();
        for key in ["skills", "add_skills", "remove_skills"] {
            match input[key].is_array().then(|| self.find_skills(&texts(input, key))) {
                Some(Err(e)) => return ToolOut::err(format!("Nothing changed: {e}")),
                Some(Ok(ids)) => lists.push(Some(ids)),
                None => lists.push(None),
            }
        }
        let Some(data) = self.sim_data.clone() else { return ToolOut::err("No simulator data; run tools/update_knowledge.py --only umalator.") };
        self.drafted("Race sim build change", |a| {
            if let Some(which) = input["from_plan"].as_str() {
                a.sim.umas[i] = a.plan_sim_build(which == "with_skills")?;
            }
            if let Some(o) = outfit {
                a.set_outfit(i, o);
            }
            if let Some(stats) = input["stats"].as_array() {
                let d = &mut a.sim.umas[i].desc;
                let v: Vec<f64> = stats.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect();
                (d.speed, d.stamina, d.power, d.guts, d.wisdom) = (v[0], v[1], v[2], v[3], v[4]);
            }
            if let Some(s) = parse_style(&input["style"]) {
                a.sim.umas[i].desc.strategy = crate::tab_sim::strategy_of(s);
                a.fill_aptitudes(i);
            }
            if let Some(m) = input["mood"].as_str() {
                a.sim.umas[i].desc.mood = code(&["Awful", "Bad", "Normal", "Good", "Great"], m).unwrap_or(3) - 3;
            }
            let u = &mut a.sim.umas[i];
            let unique = u.outfit.and_then(|o| data.unique_skill(o, u.stars));
            if let Some(ids) = &lists[0] {
                u.skills = unique.into_iter().chain(ids.iter().copied().filter(|s| Some(*s) != unique)).collect();
            }
            if let Some(ids) = &lists[1] {
                for s in ids {
                    if !u.skills.contains(s) {
                        u.skills.push(*s);
                    }
                }
            }
            if let Some(ids) = &lists[2] {
                u.skills.retain(|s| !ids.contains(s));
            }
            let unknown: Vec<String> = u.skills.iter().filter(|s| !data.skills.contains_key(s)).map(|s| a.kb.skill_name(*s)).collect();
            u.skills.retain(|s| data.skills.contains_key(s));
            u.normalise(&data);
            a.sim.from_plan = false;
            let mut text = a.sim_builds_text();
            if !unknown.is_empty() {
                text += &format!("Left out (the simulator doesn't have them, mostly scenario skills): {}\n", unknown.join(", "));
            }
            Ok(ToolOut::ok(text, format!("Uma {} changed", i + 1)).open(OpenAt::Race))
        })
    }

    /// Apply a tool's `race` argument to the Race sim.
    fn apply_race_arg(&mut self, race: &Value) -> Result<Vec<String>, String> {
        let mut notes = Vec::new();
        if !race.is_object() {
            return Ok(notes);
        }
        if let Some(q) = text(race, "cm") {
            let e = self.find_event(q)?;
            let s = &mut self.sim;
            (s.course, s.ground, s.weather, s.season, s.time) = (e.course, i32::from(e.ground), i32::from(e.weather), i32::from(e.season), i32::from(e.time));
            let n = self.drop_disabled_skills(&e.disabled_skills);
            notes.push(format!("{} (CM {}, {}){}", e.name, e.id, if e.announced { "announced" } else { "estimated" }, if n > 0 { format!("; left out {n} skills it turns off") } else { String::new() }));
        }
        if let Some(q) = text(race, "course") {
            self.sim.course = self.find_sim_course(q)?;
        }
        let s = &mut self.sim;
        for (key, list, slot) in [("ground", &crate::tab_sim::GROUNDS, &mut s.ground), ("weather", &crate::tab_sim::WEATHERS, &mut s.weather), ("season", &crate::tab_sim::SEASONS, &mut s.season)] {
            if let Some(v) = text(race, key).and_then(|v| code(list.as_slice(), v)) {
                *slot = v;
            }
        }
        if let Some(t) = text(race, "time").and_then(|t| crate::tab_sim::TIMES.iter().find(|(_, n)| n.eq_ignore_ascii_case(t))) {
            s.time = t.0;
        }
        for i in 0..2 {
            self.fill_aptitudes(i);
        }
        Ok(notes)
    }

    /// A simulator course by id, course name ("Nakayama Turf 2500m") or race name.
    fn find_sim_course(&self, q: &str) -> Result<u32, String> {
        let data = self.sim_data.as_ref().ok_or("No simulator data.")?;
        if let Ok(id) = q.trim().parse::<u32>()
            && data.courses.contains_key(&id)
        {
            return Ok(id);
        }
        let ql = q.to_lowercase();
        let used = |id: &u32| self.kb.umalator_presets.iter().any(|e| e.course == *id);
        let mut by_name: Vec<u32> = data.courses.keys().copied().filter(|id| data.course_name(*id).to_lowercase().contains(&ql)).collect();
        by_name.sort_by_key(|id| (!used(id), *id));
        if let Some(id) = by_name.first() {
            return Ok(*id);
        }
        let race = self.kb.races.iter().find(|r| r.name.to_lowercase() == ql).or_else(|| self.kb.races.iter().find(|r| r.name.to_lowercase().contains(&ql)));
        if let Some(r) = race {
            let mut fits: Vec<u32> = self.kb.umalator_courses.iter().filter(|c| crate::tab_planner::runs_on(r, c)).map(|c| c.id).filter(|id| data.courses.contains_key(id)).collect();
            fits.sort_by_key(|id| (!used(id), *id));
            if let Some(id) = fits.first() {
                return Ok(*id);
            }
        }
        Err(format!("No simulator course matches '{q}'. Try 'Tokyo Turf 2400m' or a race name."))
    }

    // -----------------------------------------------------------------------
    // Long tools

    fn long_tool(&mut self, ctx: &egui::Context, name: &str, input: &Value) -> Exec {
        let sim = matches!(name, "auto_pick_skills" | "simulate_skills" | "run_compare" | "run_skill_chart" | "run_stamina" | "stat_sensitivity");
        if sim {
            if self.agent.sims >= MAX_SIMS {
                return Exec::Done(ToolOut::err(format!("That's {MAX_SIMS} simulations for this question. Show the user what you have, and ask before running more.")));
            }
            self.agent.sims += 1;
        }
        match name {
            "career_sim" => Exec::Done(match self.plan_career() {
                Some(c) => {
                    let names = ["Speed", "Stamina", "Power", "Guts", "Wit"];
                    let stats: Vec<String> = c.stats_mean.iter().zip(names).map(|(v, n)| format!("{n} {v:.0}")).collect();
                    let sources: Vec<String> = c.sp_sources.iter().map(|(n, v)| format!("{n} {v:.0}")).collect();
                    let text = format!(
                        "Career simulation of the working plan ({} careers in {}; model output built on knowledge/scenarios.json's assumptions, not game data):\nSkill points at the end: {:.0} on average (10–90%: {:.0}–{:.0}); from {}.\nFinal stats on average: {}.\nRainbow trainings {:.1}, trainings {:.1}, races {:.1} per career.",
                        c.runs,
                        self.plan_scenario_name(),
                        c.sp_mean,
                        c.sp_p10,
                        c.sp_p90,
                        sources.join(", "),
                        stats.join(", "),
                        c.rainbows_mean,
                        c.trainings_mean,
                        c.races_mean
                    );
                    ToolOut::ok(text, format!("About {:.0} SP ({:.0}–{:.0}) and {}", c.sp_mean, c.sp_p10, c.sp_p90, stats.join(" / "))).open(OpenAt::Plan)
                }
                None => ToolOut::err("No career simulation: the plan needs a trainee, and knowledge/scenarios.json needs the plan's scenario."),
            }),
            "auto_pick_skills" => {
                let undo = self.agent_snapshot("auto-picked skills");
                if let Some(o) = input["objective"].as_str() {
                    use crate::skill_opt::PickObjective;
                    self.planner.plan.pick_objective = match o {
                        "consistency" => PickObjective::Consistency,
                        "rating" => PickObjective::Rating,
                        _ => PickObjective::Lengths,
                    };
                }
                if self.planner.plan.pick_objective == crate::skill_opt::PickObjective::Rating {
                    self.after_draft();
                    let msg = self.rating_auto_pick();
                    let chosen = self.planner.plan.skills.clone();
                    let text = format!("{msg}\nSkills to buy: {}", self.skill_names(&chosen));
                    return Exec::Done(ToolOut::ok(text, msg).visual(Visual::Skills(chosen)).open(OpenAt::Plan).undo(Some(undo)));
                }
                let quick = self.auto_pick_skills();
                match self.start_pick(ctx) {
                    Ok(()) => Exec::Wait(Wait::Pick { undo: Some(undo) }),
                    Err(e) => {
                        self.after_draft();
                        let chosen = self.planner.plan.skills.clone();
                        let text = format!("{quick} The optimiser couldn't run ({e}), so these come from single gains or estimates.\nSkills to buy: {}", self.skill_names(&chosen));
                        Exec::Done(ToolOut::ok(text, quick).visual(Visual::Skills(chosen)).open(OpenAt::Plan).undo(Some(undo)))
                    }
                }
            }
            "simulate_skills" => match self.start_skill_values(ctx, false) {
                Ok(()) => Exec::Wait(Wait::SkillValues { top_n: int(input, "top_n").unwrap_or(12) as usize }),
                Err(e) => Exec::Done(ToolOut::err(e)),
            },
            "run_compare" | "run_skill_chart" | "run_stamina" => self.start_sim_tool(ctx, name, input),
            "stat_sensitivity" => {
                if self.sim.compare.is_none() || self.sim.ran.is_none() {
                    return Exec::Done(ToolOut::err("Run run_compare first: this tests Uma 2 of the last Compare."));
                }
                let kind = if input["what"].as_str() == Some("skills") { ExtraKind::Contribution } else { ExtraKind::Sensitivity };
                self.start_extra(ctx, kind);
                Exec::Wait(Wait::Extra(kind))
            }
            other => Exec::Done(ToolOut::err(format!("Unknown tool {other}"))),
        }
    }

    fn start_sim_tool(&mut self, ctx: &egui::Context, name: &str, input: &Value) -> Exec {
        if self.sim.job.is_some() {
            return Exec::Done(ToolOut::err("The Race sim is already running something. Wait for it, then try again."));
        }
        let undo = self.agent_snapshot(match name {
            "run_compare" => "Race sim setup for a Compare",
            "run_skill_chart" => "Race sim setup for a skill chart",
            _ => "Race sim setup for a stamina check",
        });
        let started = (|| {
            let mut notes = Vec::new();
            if name == "run_compare" && input["builds"].as_str().unwrap_or("plan") == "plan" {
                notes.push(self.load_plan_into_sim()?);
            }
            notes.extend(self.apply_race_arg(&input["race"])?);
            let s = &mut self.sim;
            s.auto_seed = false;
            let seed = input["seed"].as_f64().unwrap_or(12345.0);
            match name {
                "run_compare" => {
                    s.mode = Mode::Compare;
                    s.opts = crate::sim::run::SimOptions { samples: int(input, "runs").unwrap_or(1000).clamp(100, 2000) as usize, seed, ..Default::default() };
                }
                "run_skill_chart" => {
                    s.mode = Mode::Chart;
                    s.chart_pool = if input["pool"].as_str() == Some("all") { crate::tab_sim::ChartPool::AllGlobal } else { crate::tab_sim::ChartPool::Account };
                    s.opts.seed = seed;
                }
                _ => {
                    s.mode = Mode::Stamina;
                    s.opts = crate::sim::run::SimOptions { samples: int(input, "runs").unwrap_or(500).clamp(100, 2000) as usize, seed, ..Default::default() };
                }
            }
            self.sim_start(ctx);
            if self.sim.job.is_none() {
                return Err("The Race sim couldn't start: check the course, or for a skill chart that Uma 1 has skills left to test.".to_string());
            }
            Ok(notes)
        })();
        match started {
            Ok(_) => {
                self.after_draft();
                Exec::Wait(match name {
                    "run_compare" => Wait::Compare { undo: Some(undo) },
                    "run_skill_chart" => Wait::Chart { top_n: int(input, "top_n").unwrap_or(10) as usize, undo: Some(undo) },
                    _ => Wait::Stamina { undo: Some(undo) },
                })
            }
            Err(e) => {
                let _ = self.agent_undo(undo);
                Exec::Done(ToolOut::err(e))
            }
        }
    }

    /// A Long tool's result once its job has finished; None while it runs.
    pub(crate) fn agent_wait(&mut self, w: Wait) -> Option<ToolOut> {
        match w {
            Wait::Pick { undo } => {
                if self.planner.pick_job.is_some() {
                    return None;
                }
                self.after_draft();
                let status = self.planner.skill_status.clone().map(|s| s.0).unwrap_or_default();
                let Some(out) = self.current_pick().cloned() else {
                    return Some(ToolOut::err(format!("Auto-pick didn't finish for this plan: {status}")).undo(undo));
                };
                let mut text = format!("{status}\nSkills to buy (simulation output; gains are lengths in this set, from 1000 races):\n");
                let mut chosen = out.chosen.clone();
                chosen.sort_by(|a, b| out.marginal.get(b).unwrap_or(&0.0).total_cmp(out.marginal.get(a).unwrap_or(&0.0)));
                for id in &chosen {
                    let gain = out.marginal.get(id).map_or(String::new(), |g| format!(" +{g:.2} L in the set"));
                    let act = out.activation.get(id).map_or(String::new(), |r| format!(", triggers {:.0}%", r * 100.0));
                    text += &format!("- {}{gain}{act}\n", self.kb.skill_name(*id));
                }
                let budget = self.pick_budget().map_or(String::new(), |b| format!(" of {b:.0}"));
                let summary = match &out.verify {
                    Some(v) => {
                        text += &format!("The set gains {:+.2} L on average (95% interval {:+.2} to {:+.2}, {} races) over the trainee with only her unique. Spends about {:.0} SP{budget}; took {:.0} s.\n", v.mean, v.lo, v.hi, v.runs, out.spent, out.seconds);
                        format!("{} skills, {:+.2} L ({:.0} SP{budget})", chosen.len(), v.mean, out.spent)
                    }
                    None => "Nothing on offer gains enough to buy".to_string(),
                };
                Some(ToolOut::ok(text, summary).visual(Visual::Skills(out.chosen.clone())).open(OpenAt::Plan).undo(undo))
            }
            Wait::SkillValues { top_n } => {
                if self.planner.skill_job.is_some() {
                    return None;
                }
                let status = self.planner.skill_status.clone().map(|s| s.0).unwrap_or_default();
                let Some(v) = self.planner.current_skill_values(self.plan_key()) else { return Some(ToolOut::err(format!("The skill simulation didn't finish: {status}"))) };
                let mut rows: Vec<&crate::sim::run::ChartRow> = v.rows.values().collect();
                rows.sort_by(|a, b| b.mean.total_cmp(&a.mean));
                let mut text = format!("{status}\nBest skills on {} (simulation output, lengths over the trainee alone; chart mode: no wit checks or rushing):\n", v.label);
                for r in rows.iter().take(top_n.max(1)) {
                    let sp = self.kb.skills.get(&r.id).map_or(0, |s| s.sp);
                    text += &format!("- {} ({sp} SP): {:+.2} L mean, median {:+.2}, range {:+.2} to {:+.2}\n", self.kb.skill_name(r.id), r.mean, r.median, r.min, r.max);
                }
                let bars = rows.iter().take(8).map(|r| (self.kb.skill_name(r.id), r.mean)).collect();
                Some(ToolOut::ok(text, format!("Simulated {} skills", v.rows.len())).visual(Visual::Bars(bars)).open(OpenAt::Plan))
            }
            Wait::Compare { undo } => {
                if self.sim.job.is_some() {
                    return None;
                }
                let Some(r) = self.sim.compare.clone() else { return Some(ToolOut::err("The Compare stopped before it finished.").undo(undo)) };
                Some(self.compare_out(&r).undo(undo))
            }
            Wait::Chart { top_n, undo } => {
                if self.sim.job.is_some() {
                    return None;
                }
                let Some(mut rows) = self.sim.chart.clone() else { return Some(ToolOut::err("The skill chart stopped before it finished.").undo(undo)) };
                rows.sort_by(|a, b| b.mean.total_cmp(&a.mean));
                let mut text = format!("Skill chart for Uma 1 ({}). Simulation output: lengths each skill adds alone, chart mode (no wit checks or rushing).\n", self.sim_builds_text().lines().next().unwrap_or_default());
                for r in rows.iter().take(top_n.max(1)) {
                    let sp = self.kb.skills.get(&r.id).map_or(0, |s| s.sp);
                    let per = if sp > 0 { format!(", {:.2} L per 100 SP", r.mean / f64::from(sp) * 100.0) } else { String::new() };
                    text += &format!("- {} ({sp} SP): {:+.2} L mean, median {:+.2}{per}\n", self.skill_label_any(r.id), r.mean, r.median);
                }
                let bars = rows.iter().take(8).map(|r| (self.skill_label_any(r.id), r.mean)).collect();
                Some(ToolOut::ok(text, format!("Charted {} skills; best {}", rows.len(), rows.first().map_or(String::new(), |r| format!("{} {:+.2} L", self.skill_label_any(r.id), r.mean)))).visual(Visual::Bars(bars)).open(OpenAt::Race).undo(undo))
            }
            Wait::Stamina { undo } => {
                if self.sim.job.is_some() {
                    return None;
                }
                let Some(r) = self.sim.stamina.clone() else { return Some(ToolOut::err("The stamina run stopped before it finished.").undo(undo)) };
                Some(self.stamina_out(&r).undo(undo))
            }
            Wait::Extra(kind) => {
                if self.sim.extra.is_some() {
                    return None;
                }
                let items = match kind {
                    ExtraKind::Contribution => self.sim.contribution.clone(),
                    ExtraKind::Sensitivity => self.sim.sensitivity.clone(),
                }?;
                let head = match kind {
                    ExtraKind::Contribution => "What each of Uma 2's skills is worth: the gap with it minus without it (200 races each). Simulation output.",
                    ExtraKind::Sensitivity => "Where Uma 2's next training points matter: +50 in each stat (300 races each). Simulation output.",
                };
                let lines: Vec<String> = items.iter().map(|(l, v)| format!("- {l}: {v:+.2} L")).collect();
                let summary = items.first().map_or("Nothing to test".to_string(), |(l, v)| format!("Most: {l} {v:+.2} L"));
                Some(ToolOut::ok(format!("{head}\n{}", lines.join("\n")), summary).visual(Visual::Bars(items)).open(OpenAt::Race))
            }
        }
    }

    fn skill_label_any(&self, id: u32) -> String {
        match self.kb.skills.get(&id) {
            Some(s) => s.name.clone(),
            None => self.sim_data.as_ref().map_or(format!("Skill {id}"), |d| d.skill_name(id)),
        }
    }

    /// A Compare's result for the model, with its setup and a link to reproduce it.
    fn compare_out(&self, r: &crate::sim::run::CompareResult) -> ToolOut {
        let runs = r.gaps.len().max(1);
        let n = runs as f64;
        let ahead = r.gaps.iter().filter(|g| **g > 0.0).count() as f64 / n * 100.0;
        let (winner, lead, share) = if r.mean >= 0.0 { ("Uma 2", r.mean, ahead) } else { ("Uma 1", -r.mean, 100.0 - ahead) };
        let sd = (r.gaps.iter().map(|g| (g - r.mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
        let half = 1.96 * sd / n.sqrt();
        let pct = |p: f64| r.gaps.get(((runs - 1) as f64 * p).round() as usize).copied().unwrap_or(0.0);
        let line = format!("{winner} wins {share:.0}% of {runs} races, by {lead:.2} L on average");
        let mut text = format!("Compare (simulation output, Moomoolator's engine):\n{}", self.ran_setup_text());
        text += &format!(
            "Result: {line} (gap Uma 2 − Uma 1: mean {:+.2}, median {:+.2}, 95% interval {:+.2} to {:+.2}, P10 {:+.2}, P90 {:+.2}, min {:+.2}, max {:+.2}; 1 L = 2.5 m).\n",
            r.mean,
            r.median,
            r.mean - half,
            r.mean + half,
            pct(0.1),
            pct(0.9),
            r.min,
            r.max
        );
        if lead < half || lead < 0.1 {
            text += "The gap is within noise: treat them as even, or run more races.\n";
        }
        for (k, u) in r.umas.iter().enumerate() {
            text += &format!(
                "Uma {}: finish {} (game clock), full spurt {:.1}%, HP survival {:.1}%, rushed {:.1}%\n",
                k + 1,
                crate::tab_sim::game_time(u.mean_time),
                u.full_spurt_rate,
                u.survival_rate,
                u.rushed_rate
            );
        }
        let mut acts: Vec<&crate::sim::run::SkillStat> = r.umas[1].skills.iter().collect();
        acts.sort_by(|a, b| b.rate.total_cmp(&a.rate));
        if !acts.is_empty() {
            let list: Vec<String> = acts.iter().map(|s| format!("{} {:.0}% at {:.0} m", self.skill_label_any(s.id), s.rate, s.mean_start)).collect();
            text += &format!("Uma 2's skill activations: {}.\n", list.join("; "));
        }
        if let Some(ran) = &self.sim.ran {
            let fired: Vec<u32> = r.umas[1].skills.iter().filter(|s| s.rate > 0.0).map(|s| s.id).collect();
            let never: Vec<String> = ran.umas[1].skills.iter().filter(|s| !fired.contains(s) && self.skill_not_logged(**s).is_none()).map(|s| self.skill_label_any(*s)).collect();
            if !never.is_empty() {
                text += &format!("Never fired for Uma 2: {} (conditions not met on this course, or failed wit checks).\n", never.join(", "));
            }
        }
        if !r.warnings.is_empty() {
            text += &format!("Left out: {}\n", r.warnings.join("; "));
        }
        if let Ok(md) = self.sim_markdown()
            && let Some(link) = md.lines().find(|l| l.starts_with("Reproduce in Moomoolator"))
        {
            text += &format!("{link}\n");
        }
        ToolOut::ok(text, line.clone()).visual(Visual::Verdict { line, gaps: r.gaps.clone() }).open(OpenAt::Race)
    }

    fn ran_setup_text(&self) -> String {
        let Some(ran) = &self.sim.ran else { return String::new() };
        let course = self.sim_data.as_ref().map_or(format!("course {}", ran.course), |d| d.course_name(ran.course));
        let o = &ran.opts;
        let mut out = format!("Setup: {course}, {}; {} races, seed {}; sync RNG {}, wit check {}, rushed {}\n", ran.conditions, o.samples, o.seed, o.sync_rng, o.wit_check, o.rushed);
        for (k, u) in ran.umas.iter().enumerate() {
            out += &format!(
                "Uma {}: {}, {}/{}/{}/{}/{} {}, skills: {}\n",
                k + 1,
                u.outfit.map_or("no outfit".to_string(), |o| self.uma_label(o)),
                u.desc.speed,
                u.desc.stamina,
                u.desc.power,
                u.desc.guts,
                u.desc.wisdom,
                crate::sim::params::strategy_name(u.desc.strategy),
                self.skill_names(&u.skills)
            );
        }
        out
    }

    fn stamina_out(&self, r: &crate::sim::run::HpCalcResult) -> ToolOut {
        let runs = r.runs.max(1);
        let full = r.full_spurt_runs as f64 / runs as f64 * 100.0;
        let rem = &r.remaining;
        let at = |p: f64| rem.get(((rem.len().max(1) - 1) as f64 * p).round() as usize).copied().unwrap_or(0.0);
        let mut text = format!(
            "Stamina check for Uma 1 (simulation output):\n{}Max HP {:.0}. HP for a full spurt in {full:.0}% of {runs} races. HP left at the finish: median {:.0}, P10 {:.0}, P90 {:.0} (negative: ran out).\n",
            self.ran_setup_text().lines().take(2).collect::<Vec<_>>().join("\n") + "\n",
            r.max_hp,
            at(0.5),
            at(0.1),
            at(0.9)
        );
        if let (Some(data), Some(ran)) = (&self.sim_data, &self.sim.ran)
            && let Some(course) = data.courses.get(&ran.course)
        {
            let u = &ran.umas[0];
            use crate::sim::params::{NIGE, OIKOMI, SASI, SENKOU};
            let style = match u.desc.strategy {
                NIGE => 0,
                SENKOU => 1,
                SASI => 2,
                OIKOMI => 3,
                _ => 4,
            };
            let e = crate::sim::hpcalc::estimate(course, u.desc.stamina, u.desc.guts, u.desc.wisdom, u.desc.speed, style, 0.0);
            text += &format!(
                "Closed-form estimate (the HP Calculator): needs {:.0} HP, has {:.0}; {}. Stamina for a full spurt: about {} ({} with downhill savings).\n",
                e.total_needed,
                e.max_hp,
                if e.full_spurt { "enough" } else { "short" },
                e.min_stamina,
                e.adjusted_min_stamina
            );
        }
        ToolOut::ok(text, format!("Full spurt in {full:.0}% of races; median HP left {:.0}", at(0.5))).open(OpenAt::Race)
    }

    // -----------------------------------------------------------------------
    // UI tools

    fn ui_tool(&mut self, name: &str, input: &Value) -> ToolOut {
        let at = match name {
            "open_page" => match input["page"].as_str().unwrap_or_default() {
                "home" => OpenAt::Home,
                "umas" => OpenAt::Umas,
                "cards" => OpenAt::Cards,
                "skills" => OpenAt::Skills,
                "race" => OpenAt::Race,
                "results" => OpenAt::Results(0),
                _ => OpenAt::Plan,
            },
            "select" => {
                let q = input["query"].as_str().unwrap_or_default();
                let found = match input["kind"].as_str().unwrap_or_default() {
                    "uma" => self.find_uma_id(q).map(OpenAt::Uma),
                    "card" => self.find_card_id(q).map(OpenAt::Card),
                    "skill" => self.find_skill_id(q).map(OpenAt::Skill),
                    _ => {
                        let path = match self.find_plan_file(q) {
                            Ok(p) => p,
                            Err(e) => return ToolOut::err(e),
                        };
                        return self.drafted("opened a saved plan", |a| {
                            if !a.open_plan_file(&path) {
                                return Err("That plan file has no app data block.".into());
                            }
                            Ok(ToolOut::ok(a.plan_brief(), format!("Opened {}", path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())).open(OpenAt::Plan))
                        });
                    }
                };
                match found {
                    Ok(at) => at,
                    Err(e) => return ToolOut::err(e),
                }
            }
            other => return ToolOut::err(format!("Unknown tool {other}")),
        };
        let page = at.page();
        if self.agent.bridge.is_some() {
            self.bridge_ui_request(&at);
            return ToolOut::ok(format!("Asked the umaLord window to open {page} when this answer finishes."), format!("Will open {page}"));
        }
        if self.agent.mcp {
            return ToolOut::ok("No umaLord window is attached to this server, so nothing opened. Tell the user where to look in the app.", "No window to open it in");
        }
        self.agent.open_after = Some(at.clone());
        ToolOut::ok(format!("It opens {page} when this answer finishes."), format!("Will open {page}")).open(at)
    }

    // -----------------------------------------------------------------------
    // Write tools

    /// The question a Write tool asks, or why it can't run.
    fn approval(&self, name: &str, input: &Value) -> Result<Approval, String> {
        match name {
            "save_plan" => {
                let p = &self.planner.plan;
                let u = p.trainee.and_then(|t| self.owned_uma(t)).ok_or("The plan needs an owned trainee before it can be saved.")?;
                let title = text(input, "title").map(String::from).unwrap_or_else(|| if p.title.is_empty() { format!("{} [{}] — {}", u.name, u.outfit, p.purpose.label()) } else { p.title.clone() });
                let file = self.planner.open_file.as_ref().and_then(|f| f.file_name()).map_or("a new file in plans/".to_string(), |f| format!("plans/{}", f.to_string_lossy()));
                let sim = if input["with_simulation"].as_bool().unwrap_or(true) && self.sim.compare.is_some() && self.sim.from_plan { ", with the last Compare as its Simulation section" } else { "" };
                Ok(Approval { question: format!("Save plan '{title}' to {file}{sim}?"), always_ask: false })
            }
            "log_result" => {
                let (i, row) = self.result_row(input)?;
                let headers = self.result_headers(i);
                let filled: Vec<String> = headers.iter().zip(&row).filter(|(_, v)| !v.is_empty()).map(|(h, v)| format!("{h}: {v}")).collect();
                Ok(Approval { question: format!("Add to {}: {}?", crate::tab_results::LOGS[i].1, filled.join(" · ")), always_ask: false })
            }
            "update_owned" => {
                let (_, what) = self.owned_edit(input)?;
                Ok(Approval { question: format!("Change your roster: {what}?"), always_ask: true })
            }
            other => Err(format!("Unknown tool {other}")),
        }
    }

    /// Run an approved Write tool.
    pub(crate) fn agent_write(&mut self, name: &str, input: &Value) -> ToolOut {
        match name {
            "save_plan" => {
                if let Some(t) = text(input, "title") {
                    self.planner.plan.title = t.to_string();
                }
                let before = std::fs::metadata(self.planner.open_file.clone().unwrap_or_default()).and_then(|m| m.modified()).ok();
                self.save_plan();
                let Some(path) = self.planner.open_file.clone() else { return ToolOut::err("Couldn't save the plan (no trainee, or the folder isn't writable).") };
                let after = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                if after.is_none() || (before.is_some() && before == after) {
                    return ToolOut::err(format!("Couldn't write {}.", path.display()));
                }
                let mut text = format!("Saved {}.", path.file_name().map(|f| format!("plans/{}", f.to_string_lossy())).unwrap_or_default());
                if input["with_simulation"].as_bool().unwrap_or(true) && self.sim.compare.is_some() && self.sim.from_plan {
                    match self.save_sim_to_plan() {
                        Ok(_) => text += " The last Compare is in its ## Simulation section.",
                        Err(e) => text += &format!(" The Simulation section wasn't written: {e}"),
                    }
                }
                self.after_draft();
                ToolOut::ok(text.clone(), text).open(OpenAt::Plan)
            }
            "log_result" => match self.result_row(input) {
                Ok((i, row)) => match self.add_result_row(i, &row) {
                    Ok(path) => ToolOut::ok(format!("Added to {}.", path.display()), format!("Added to {}", crate::tab_results::LOGS[i].1)).open(OpenAt::Results(i)),
                    Err(e) => ToolOut::err(e),
                },
                Err(e) => ToolOut::err(e),
            },
            "update_owned" => match self.owned_edit(input) {
                Ok((edit, what)) => match self.apply_edits(&[edit], true) {
                    Ok(_) => {
                        self.write_digest();
                        ToolOut::ok(format!("Done: {what}."), what)
                    }
                    Err(e) => ToolOut::err(e),
                },
                Err(e) => ToolOut::err(e),
            },
            other => ToolOut::err(format!("Unknown tool {other}")),
        }
    }

    /// The results log and row a `log_result` call describes.
    fn result_row(&self, input: &Value) -> Result<(usize, Vec<String>), String> {
        let key = input["log"].as_str().unwrap_or_default();
        let i = LOG_KEYS.iter().position(|k| *k == key).ok_or("Unknown log.")?;
        let headers = self.result_headers(i);
        if headers.is_empty() {
            return Err(format!("{} has no table to add to.", crate::tab_results::LOGS[i].1));
        }
        let fields = input["fields"].as_object().ok_or("fields should map column names to values.")?;
        let mut row: Vec<String> = headers.iter().map(|h| if h.starts_with("Date") { crate::ui_util::local_today() } else { String::new() }).collect();
        let mut unknown = Vec::new();
        for (k, v) in fields {
            let value = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
            match headers.iter().position(|h| h.eq_ignore_ascii_case(k.trim())) {
                Some(c) => row[c] = value,
                None => unknown.push(k.clone()),
            }
        }
        if !unknown.is_empty() {
            return Err(format!("{} has no column {}. Its columns: {}.", crate::tab_results::LOGS[i].1, unknown.join(", "), headers.join(", ")));
        }
        if row.iter().filter(|c| !c.is_empty()).count() <= 1 {
            return Err("Fill in more than the date.".into());
        }
        Ok((i, row))
    }

    /// The roster edit an `update_owned` call describes, and a sentence for the question.
    fn owned_edit(&self, input: &Value) -> Result<(Edit, String), String> {
        let q = input["query"].as_str().unwrap_or_default();
        let remove = input["remove"].as_bool().unwrap_or(false);
        if input["kind"].as_str() == Some("uma") {
            let id = self.find_uma_id(q)?;
            let name = self.uma_label(id);
            let now = self.owned_uma(id).map(|u| (u.stars, u.potential));
            if remove {
                now.ok_or(format!("{name} isn't in the roster."))?;
                return Ok((Edit::RemoveUma(id), format!("remove {name}")));
            }
            let (stars, potential) = (int(input, "stars").map(|v| v as u8).or(now.map(|n| n.0)).unwrap_or(1), int(input, "potential").map(|v| v as u8).or(now.map(|n| n.1)).unwrap_or(1));
            let what = match now {
                Some((s, p)) => format!("{name}: {s}★ P{p} → {stars}★ P{potential}"),
                None => format!("add {name} at {stars}★ P{potential}"),
            };
            Ok((Edit::Uma { id, stars, potential }, what))
        } else {
            let id = self.find_card_id(q)?;
            let name = self.kb.cards.get(&id).map_or(format!("card {id}"), |c| format!("{} [{}] ({} {})", c.uma, c.title, c.rarity, c.kind));
            let now = self.owned_card(id).map(|c| (c.lb, c.level));
            if remove {
                now.ok_or(format!("{name} isn't in the roster."))?;
                return Ok((Edit::RemoveCard(id), format!("remove {name}")));
            }
            let lb = int(input, "lb").map(|v| v as u8).or(now.map(|n| n.0)).unwrap_or(0);
            let level = int(input, "level").map(|v| v as u8);
            let max = crate::data::Rarity::from_id(id).map_or(50, |r| r.max_level(lb));
            if level.is_some_and(|l| l > max) {
                return Err(format!("At LB{lb} the level can't go past {max}."));
            }
            let shown = level.unwrap_or(max);
            let what = match now {
                Some((l, lv)) => format!("{name}: LB{l} Lv{lv} → LB{lb} Lv{shown}"),
                None => format!("add {name} at LB{lb} Lv{shown}"),
            };
            Ok((Edit::Card { id, lb, level }, what))
        }
    }
}
