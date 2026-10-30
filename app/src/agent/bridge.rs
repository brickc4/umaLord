//! Files the MCP server (`umalord.exe --mcp`, started by OpenCode) and the umaLord window share,
//! in one folder (`%APPDATA%\umaLord\agent`, passed to the server as `UMALORD_AGENT_DIR`):
//!
//! - `approval-<n>.json`: a Write tool's question. The window shows it in the chat and writes
//!   `approval-<n>.answer` ("yes" or "no"); the server waits for that file (up to 50 s, under
//!   OpenCode's MCP request timeout).
//! - `ui-<n>.json`: a page `open_page` or `select` asked for; the window opens it when the answer
//!   finishes.
//! - `draft.json`: the server's working plan and Race sim setup after every change, so the window
//!   can open the assistant's draft in the Planner or rerun its Compare (same seed, same result),
//!   and so the next question's server (one per `opencode run`) carries on from it.
//!
//! Every value read back is checked: a plan must parse as a `Plan`, builds must have five stats.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{OpenAt, SimSetup};
use crate::App;
use crate::sim::run::{SimOptions, UmaBuild};

pub(crate) const ENV: &str = "UMALORD_AGENT_DIR";
const ANSWER_WAIT: Duration = Duration::from_secs(50);

/// The window's folder for the bridge.
pub(crate) fn dir() -> Option<PathBuf> {
    crate::settings::app_dir().map(|d| d.join("agent"))
}

fn stamp() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn build_json(u: &UmaBuild) -> Value {
    let d = &u.desc;
    json!({
        "outfit": u.outfit, "stars": u.stars, "unique_level": u.unique_level,
        "stats": [d.speed, d.stamina, d.power, d.guts, d.wisdom],
        "strategy": d.strategy, "apt": [d.distance_apt, d.surface_apt, d.strategy_apt], "mood": d.mood,
        "skills": u.skills,
    })
}

fn build_from(v: &Value) -> Option<UmaBuild> {
    let mut b = crate::tab_sim::default_uma();
    b.outfit = v["outfit"].as_u64().map(|o| o as u32);
    b.stars = v["stars"].as_u64()?.clamp(1, 5) as u8;
    b.unique_level = v["unique_level"].as_u64().unwrap_or(1) as u32;
    let s: Vec<f64> = v["stats"].as_array()?.iter().filter_map(Value::as_f64).collect();
    if s.len() != 5 {
        return None;
    }
    let d = &mut b.desc;
    (d.speed, d.stamina, d.power, d.guts, d.wisdom) = (s[0], s[1], s[2], s[3], s[4]);
    d.strategy = v["strategy"].as_i64()? as i32;
    let apt: Vec<usize> = v["apt"].as_array()?.iter().filter_map(|x| x.as_u64().map(|a| (a as usize).min(7))).collect();
    if apt.len() != 3 {
        return None;
    }
    (d.distance_apt, d.surface_apt, d.strategy_apt) = (apt[0], apt[1], apt[2]);
    d.mood = v["mood"].as_i64().unwrap_or(0).clamp(-2, 2) as i32;
    b.skills = v["skills"].as_array()?.iter().filter_map(|x| x.as_u64().map(|s| s as u32)).collect();
    Some(b)
}

fn setup_json(s: &SimSetup) -> Value {
    json!({
        "course": s.course, "ground": s.ground, "weather": s.weather, "season": s.season, "time": s.time,
        "umas": [build_json(&s.umas[0]), build_json(&s.umas[1])],
        "samples": s.opts.samples, "seed": s.opts.seed, "from_plan": s.from_plan,
    })
}

fn setup_from(v: &Value) -> Option<SimSetup> {
    let umas = v["umas"].as_array()?;
    let code = |k: &str| v[k].as_i64().map(|c| c.clamp(1, 4) as i32);
    Some(SimSetup {
        course: v["course"].as_u64()? as u32,
        ground: code("ground")?,
        weather: code("weather")?,
        season: code("season")?,
        time: v["time"].as_i64().map_or(2, |t| t.clamp(2, 4) as i32),
        umas: [build_from(umas.first()?)?, build_from(umas.get(1)?)?],
        opts: SimOptions { samples: v["samples"].as_u64().unwrap_or(1000).clamp(10, 5000) as usize, seed: v["seed"].as_f64().unwrap_or(12345.0), ..Default::default() },
        from_plan: v["from_plan"].as_bool().unwrap_or(false),
    })
}

/// The assistant's draft as the window reads it.
pub(crate) struct Draft {
    pub plan: crate::tab_planner::Plan,
    pub open_file: Option<PathBuf>,
    pub sim: Option<SimSetup>,
    /// A Compare ran on this setup (so "Open in Race sim" reruns it).
    pub compared: bool,
}

pub(crate) fn read_draft(dir: &Path) -> Option<Draft> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("draft.json")).ok()?).ok()?;
    let plan = serde_json::from_value(v["plan"].clone()).ok()?;
    // Only a plan file inside plans/ counts.
    let open_file = v["open_file"].as_str().map(PathBuf::from).filter(|p| p.parent().and_then(|d| d.file_name()).is_some_and(|d| d == "plans"));
    Some(Draft { plan, open_file, sim: setup_from(&v["sim"]), compared: v["compared"].as_bool().unwrap_or(false) })
}

/// When the draft last changed.
pub(crate) fn draft_time(dir: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(dir.join("draft.json")).and_then(|m| m.modified()).ok()
}

/// Forget the last conversation's draft and anything left unanswered.
pub(crate) fn clear(dir: &Path) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name == "draft.json" || name.starts_with("approval-") || name.starts_with("ui-") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// A question waiting in the folder for the window.
#[derive(Clone, Debug)]
pub(crate) struct Pending {
    pub id: u64,
    pub tool: String,
    pub question: String,
}

/// Questions the server asked that have no answer yet, oldest first.
pub(crate) fn pending_approvals(dir: &Path) -> Vec<Pending> {
    let mut out: Vec<Pending> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let id: u64 = name.strip_prefix("approval-")?.strip_suffix(".json")?.parse().ok()?;
            if dir.join(format!("approval-{id}.answer")).exists() {
                return None;
            }
            let v: Value = serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            Some(Pending { id, tool: v["tool"].as_str()?.to_string(), question: v["question"].as_str()?.to_string() })
        })
        .collect();
    out.sort_by_key(|p| p.id);
    out
}

pub(crate) fn answer(dir: &Path, id: u64, yes: bool) {
    let _ = std::fs::write(dir.join(format!("approval-{id}.answer")), if yes { "yes" } else { "no" });
}

/// Server side: ask the window and wait for the answer. Err when nobody answered in time.
pub(crate) fn ask_window(dir: &Path, tool: &str, question: &str) -> Result<bool, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let id = stamp();
    let req = dir.join(format!("approval-{id}.json"));
    let ans = dir.join(format!("approval-{id}.answer"));
    std::fs::write(&req, json!({"tool": tool, "question": question}).to_string()).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let result = loop {
        if let Ok(a) = std::fs::read_to_string(&ans) {
            break Ok(a.trim() == "yes");
        }
        if start.elapsed() > ANSWER_WAIT {
            break Err("The user didn't answer the approval card in time. Ask them in chat whether to go ahead, then call the tool again.".to_string());
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let _ = std::fs::remove_file(&req);
    let _ = std::fs::remove_file(&ans);
    result
}

fn open_key(at: &OpenAt) -> Value {
    match at {
        OpenAt::Home => json!({"page": "home"}),
        OpenAt::Plan | OpenAt::DraftPlan => json!({"page": "plan"}),
        OpenAt::Race | OpenAt::DraftRace => json!({"page": "race"}),
        OpenAt::Results(i) => json!({"page": "results", "log": i}),
        OpenAt::Umas => json!({"page": "umas"}),
        OpenAt::Cards => json!({"page": "cards"}),
        OpenAt::Skills => json!({"page": "skills"}),
        OpenAt::Uma(id) => json!({"page": "umas", "id": id}),
        OpenAt::Card(id) => json!({"page": "cards", "id": id}),
        OpenAt::Skill(id) => json!({"page": "skills", "id": id}),
    }
}

fn open_from(v: &Value) -> Option<OpenAt> {
    let id = v["id"].as_u64().map(|i| i as u32);
    Some(match (v["page"].as_str()?, id) {
        ("home", _) => OpenAt::Home,
        // The server's plan is its own: the window opens the draft.
        ("plan", _) => OpenAt::DraftPlan,
        ("race", _) => OpenAt::DraftRace,
        ("results", _) => OpenAt::Results(v["log"].as_u64().unwrap_or(0).min(4) as usize),
        ("umas", Some(i)) => OpenAt::Uma(i),
        ("cards", Some(i)) => OpenAt::Card(i),
        ("skills", Some(i)) => OpenAt::Skill(i),
        ("umas", None) => OpenAt::Umas,
        ("cards", None) => OpenAt::Cards,
        ("skills", None) => OpenAt::Skills,
        _ => return None,
    })
}

/// Window side: take the pages the server asked for.
pub(crate) fn take_ui_requests(dir: &Path) -> Vec<OpenAt> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("ui-")))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|p| {
            let v = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
            let _ = std::fs::remove_file(&p);
            open_from(&v?)
        })
        .collect()
}

impl App {
    /// Server side: write the working plan and the sim setup for the window.
    pub(crate) fn write_bridge_draft(&self) {
        let Some(dir) = &self.agent.bridge else { return };
        let s = &self.sim;
        let setup = SimSetup { course: s.course, ground: s.ground, weather: s.weather, season: s.season, time: s.time, umas: s.umas.clone(), opts: s.opts.clone(), from_plan: s.from_plan };
        let v = json!({
            "plan": self.planner.plan,
            "open_file": self.planner.open_file,
            "sim": setup_json(&setup),
            "compared": self.sim.compare.is_some(),
        });
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join("draft.json"), v.to_string());
    }

    /// Server side: carry on from the last question's draft.
    pub(crate) fn load_bridge_draft(&mut self) {
        let Some(d) = self.agent.bridge.as_deref().and_then(read_draft) else { return };
        self.planner.plan = d.plan;
        self.planner.open_file = d.open_file;
        if let Some(s) = d.sim {
            self.restore_sim(s);
        }
    }

    /// Server side: ask the window to open a page.
    pub(crate) fn bridge_ui_request(&self, at: &OpenAt) {
        let Some(dir) = &self.agent.bridge else { return };
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join(format!("ui-{}.json", stamp())), open_key(at).to_string());
    }

    /// Window side: open the assistant's draft plan in the Planner (undoable from the chat).
    pub(crate) fn open_bridge_draft(&mut self, ctx: &eframe::egui::Context, race: bool) -> Result<String, String> {
        let dir = dir().ok_or("No settings folder.")?;
        let d = read_draft(&dir).ok_or("The assistant hasn't drafted anything yet.")?;
        self.agent_snapshot("opened the assistant's draft");
        self.planner.plan = d.plan;
        self.planner.open_file = d.open_file;
        self.planner.skill_status = None;
        if !race {
            self.go_career(crate::CareerTab::Plan);
            return Ok("Opened the assistant's plan in the Planner.".into());
        }
        let s = d.sim.ok_or("The draft has no Race sim setup.")?;
        self.restore_sim(s);
        self.sim.mode = crate::tab_sim::Mode::Compare;
        self.sim.auto_seed = false;
        self.go_career(crate::CareerTab::Race);
        if d.compared {
            self.sim_start(ctx);
            return Ok("Opened the assistant's Race sim setup and raced it again with the same seed.".into());
        }
        Ok("Opened the assistant's Race sim setup.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_setups_round_trip() {
        let mut u = crate::tab_sim::default_uma();
        u.outfit = Some(100601);
        u.skills = vec![100611, 200012];
        let s = SimSetup { course: 10506, ground: 2, weather: 3, season: 4, time: 3, umas: [u.clone(), crate::tab_sim::default_uma()], opts: SimOptions { samples: 700, seed: 99.0, ..Default::default() }, from_plan: true };
        let back = setup_from(&setup_json(&s)).expect("parses");
        assert_eq!(back.umas[0], u);
        assert_eq!((back.course, back.ground, back.weather, back.season, back.time, back.opts.samples, back.opts.seed, back.from_plan), (10506, 2, 3, 4, 3, 700, 99.0, true));
        assert!(setup_from(&json!({"course": 1})).is_none());
    }

    #[test]
    fn approvals_and_page_requests_go_through_the_folder() {
        let dir = std::env::temp_dir().join(format!("umalord-bridge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d2 = dir.clone();
        // The "window" answers yes as soon as the question shows up.
        let window = std::thread::spawn(move || {
            for _ in 0..200 {
                if let Some(p) = pending_approvals(&d2).first() {
                    assert_eq!(p.tool, "save_plan");
                    answer(&d2, p.id, true);
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            panic!("no question appeared");
        });
        assert_eq!(ask_window(&dir, "save_plan", "Save plan 'X'?"), Ok(true));
        window.join().unwrap();
        assert!(pending_approvals(&dir).is_empty(), "answered questions are cleaned up");
        std::fs::write(dir.join("ui-1.json"), open_key(&OpenAt::Card(30052)).to_string()).unwrap();
        std::fs::write(dir.join("ui-2.json"), open_key(&OpenAt::Plan).to_string()).unwrap();
        assert_eq!(take_ui_requests(&dir), vec![OpenAt::Card(30052), OpenAt::DraftPlan]);
        assert!(take_ui_requests(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
