//! Ask: an assistant that answers questions about this account and works the app with the tools
//! in `agent/`: it drafts plans, picks decks and skills, runs the race simulator, and saves plans
//! and results once the user approves.
//!
//! Two backends. The Claude API: raw HTTP through Windows' curl.exe (Rust has no official
//! Anthropic SDK, and TLS crates need a C compiler this toolchain lacks), streamed over SSE; the UI
//! thread runs the tools (they read and change App state) and a worker thread only does HTTP.
//! OpenCode: `opencode run` with the same tools over MCP (`umalord.exe --mcp`), which writes its
//! approvals, page requests and draft to the bridge folder this tab watches (`agent/bridge.rs`).

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke, vec2};
use serde_json::{Value, json};

use crate::App;
use crate::ui_util::UiExt;
use crate::agent::{Approval, Card, Exec, OpenAt, ToolOut, Visual, Wait};
use crate::tab_umas::filter_heading;
use crate::theme::{pal, size};
use crate::ui_util::{badge, semibold, truncate};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
/// Tool rounds per question: enough for plan → deck → skills → simulations → save.
const MAX_TOOL_ROUNDS: u32 = 30;
/// Wall-clock limit per question.
const QUESTION_LIMIT: Duration = Duration::from_secs(15 * 60);

/// (model id, label, supports server-side refusal fallbacks, supports `thinking.display: "updates"`)
pub const MODELS: [(&str, &str, bool, bool); 3] = [
    ("claude-opus-5-5", "Claude Opus 5.5", true, true),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5", true, true),
    ("claude-haiku-5-5", "Claude Haiku 5.5", false, false),
];
pub const EFFORTS: [&str; 4] = ["low", "medium", "high", "xhigh"];

/// A setup token works only through Claude Code: the API refuses it from other apps.
const SETUP_TOKEN_NEEDS_CLAUDE_CODE: &str = "That's a Claude setup token (sk-ant-oat…), which only works through Claude Code. Install Claude Code (claude.com/claude-code) so umaLord can use it, or paste an API key from console.anthropic.com.";

enum Auth {
    ApiKey(String),
    /// OAuth access token from `ant auth print-credentials --access-token`.
    Bearer(String),
}

/// One line of the chat.
pub(crate) enum Turn {
    User(String),
    Assistant(String),
    /// A short progress note the model wrote between tool calls.
    Note(String),
    Tool(ToolTurn),
}

pub(crate) struct ToolTurn {
    pub icon: &'static str,
    pub label: String,
    pub state: ToolState,
}

pub(crate) enum ToolState {
    Running,
    /// A Write tool waiting for Approve or Deny; with a bridge id when OpenCode's server asked.
    Asking(String, Option<u64>),
    Done(Card),
    Failed(Card),
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Backend {
    /// OpenCode with a free model: no key, no cost.
    OpenCode,
    /// Claude: through Claude Code when it's installed (its login, a setup token or an API key,
    /// `ask_claude_code`), else the Claude API with the user's key.
    ClaudeApi,
}

pub(crate) struct OcState {
    pub exe: Option<std::path::PathBuf>,
    /// Claude Code, which runs the Claude backend when it's installed.
    pub claude_exe: Option<std::path::PathBuf>,
    pub models: Vec<String>,
    pub models_rx: Option<Receiver<Result<Vec<String>, String>>>,
    pub model: String,
    /// OpenCode's session id, so follow-up questions keep the conversation.
    pub session: Option<String>,
    pub rx: Option<Receiver<crate::ask_opencode::OcEvent>>,
    pub child: Arc<Mutex<Option<Child>>>,
    /// The current answer already has a turn that new text parts append to.
    pub answer_open: bool,
    pub install_rx: Option<Receiver<crate::ask_opencode::InstallEvent>>,
    pub install_progress: u64,
    pub install_error: Option<String>,
    /// The bridge folder: when it was last scanned, the approvals already shown, the draft's
    /// time, and the plans/ files' times when the question started.
    pub scanned: Option<Instant>,
    pub plans_scanned: Option<Instant>,
    pub shown: HashSet<u64>,
    pub draft_seen: Option<SystemTime>,
    pub plans_before: HashMap<PathBuf, SystemTime>,
}

/// One tool call of the current round.
struct Call {
    id: String,
    name: String,
    input: Value,
    /// Index of its card in `turns`.
    turn: usize,
    state: CallState,
}

enum CallState {
    Queued,
    Waiting(Wait),
    /// The card shows the question; Approve or Deny answers it.
    Asking,
    /// The `tool_result` block.
    Ready(Value),
}

/// The response being streamed.
struct Stream {
    rx: Receiver<StreamEvent>,
    child: Arc<Mutex<Option<Child>>>,
    /// Content block index → its turn (text or note).
    turns: HashMap<usize, usize>,
}

pub(crate) struct AskState {
    pub backend: Backend,
    pub oc: OcState,
    /// Full API transcript, append-only (thinking blocks stay valid).
    messages: Vec<Value>,
    pub(crate) turns: Vec<Turn>,
    input: String,
    stream: Option<Stream>,
    calls: Vec<Call>,
    rounds: u32,
    started: Option<Instant>,
    /// The system prompt, fixed for the conversation: thinking blocks are tied to it.
    system: Option<String>,
    pub(crate) model: usize,
    pub(crate) effort: usize,
    key_input: String,
    remember: bool,
    saved_key: Option<String>,
    status: Option<(String, bool)>,
    tokens_in: u64,
    tokens_out: u64,
    cache_read: u64,
    activity: Vec<String>,
    /// Approve (true) or Deny clicked on the card at this turn.
    answer: Option<(usize, bool)>,
    /// Settings → AI assistant: Write tools that may run without asking.
    pub auto_save: bool,
    pub auto_log: bool,
    /// `UMALORD_APPROVE=ask`: take the screenshot when an approval card shows.
    pub shot_on_approval: bool,
}

/// The OpenCode assistant's draft, as its card in the chat.
const DRAFT_CARD: &str = "The assistant's working plan";

/// Example questions shown on an empty chat.
const SUGGESTIONS: [&str; 4] = [
    "Plan the next Champions Meeting for my best Pace uma and simulate it.",
    "Build me a Speed-heavy deck from my MLB SSRs for a Medium Turf race.",
    "Which of my support cards give a hint for a corner recovery skill?",
    "Which of my umas should I raise to Potential 5 first, and why?",
];

impl AskState {
    pub fn new(root: Option<&Path>) -> Self {
        let saved_key = crate::settings::get("api_key").and_then(|v| v.as_str().map(str::to_string)).filter(|k| !k.is_empty());
        let exe = crate::ask_opencode::find_exe(root);
        let saved = |k: &str| crate::settings::get(k).and_then(|v| v.as_str().map(str::to_string));
        let flag = |k: &str| !cfg!(test) && crate::settings::get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        let backend = match saved("ask_backend").as_deref() {
            Some("claude_api") => Backend::ClaudeApi,
            // Default to the free backend; if OpenCode isn't installed the tab offers the download.
            _ => Backend::OpenCode,
        };
        Self {
            backend,
            oc: OcState {
                exe,
                claude_exe: crate::ask_claude_code::find_exe(),
                models: Vec::new(),
                models_rx: None,
                model: saved("opencode_model").unwrap_or_else(|| crate::ask_opencode::DEFAULT_MODEL.to_string()),
                session: None,
                rx: None,
                child: Default::default(),
                answer_open: false,
                install_rx: None,
                install_progress: 0,
                install_error: None,
                scanned: None,
                plans_scanned: None,
                shown: HashSet::new(),
                draft_seen: None,
                plans_before: HashMap::new(),
            },
            messages: Vec::new(),
            turns: Vec::new(),
            input: String::new(),
            stream: None,
            calls: Vec::new(),
            rounds: 0,
            started: None,
            system: None,
            model: 0,
            effort: 1,
            key_input: String::new(),
            remember: false,
            saved_key,
            status: None,
            tokens_in: 0,
            tokens_out: 0,
            cache_read: 0,
            activity: Vec::new(),
            answer: None,
            // UMALORD_APPROVE=1 allows plan saves and result logs without asking (unattended runs).
            auto_save: flag("agent_auto_save_plan") || std::env::var("UMALORD_APPROVE").is_ok_and(|v| v == "1"),
            auto_log: flag("agent_auto_log_result") || std::env::var("UMALORD_APPROVE").is_ok_and(|v| v == "1"),
            shot_on_approval: std::env::var("UMALORD_APPROVE").is_ok_and(|v| v == "ask"),
        }
    }

    fn auth(&self) -> Option<Auth> {
        if let Ok(k) = std::env::var("ANTHROPIC_API_KEY")
            && !k.trim().is_empty()
        {
            return Some(Auth::ApiKey(k.trim().to_string()));
        }
        if !self.key_input.trim().is_empty() {
            return Some(Auth::ApiKey(self.key_input.trim().to_string()));
        }
        if let Some(k) = &self.saved_key {
            return Some(Auth::ApiKey(k.clone()));
        }
        ant_token().map(Auth::Bearer)
    }

    /// The key or token the user gave, in `auth`'s order; none means Claude Code's own login.
    fn claude_key(&self) -> Option<String> {
        std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty())
            .or_else(|| Some(self.key_input.trim().to_string()).filter(|k| !k.is_empty()))
            .or_else(|| self.saved_key.clone())
    }

    /// The Claude backend runs through Claude Code when it's installed.
    pub(crate) fn via_claude_code(&self) -> bool {
        self.backend == Backend::ClaudeApi && self.oc.claude_exe.is_some()
    }

    fn auth_label(&self) -> &'static str {
        if std::env::var("ANTHROPIC_API_KEY").is_ok_and(|k| !k.trim().is_empty()) {
            "ANTHROPIC_API_KEY"
        } else if !self.key_input.trim().is_empty() {
            "key entered below"
        } else if self.saved_key.is_some() {
            "saved key"
        } else {
            "no key set yet"
        }
    }

    /// Whether a Write tool may run without the approval card.
    fn auto_approved(&self, tool: &str, a: &Approval) -> bool {
        !a.always_ask && ((tool == "save_plan" && self.auto_save) || (tool == "log_result" && self.auto_log))
    }

    fn clear(&mut self) {
        self.messages.clear();
        self.turns.clear();
        self.calls.clear();
        self.activity.clear();
        self.status = None;
        self.system = None;
        self.tokens_in = 0;
        self.tokens_out = 0;
        self.cache_read = 0;
        self.oc.session = None;
        self.oc.shown.clear();
        self.oc.draft_seen = None;
        self.end_process();
    }

    /// End the conversation's assistant process (Claude Code keeps one warm between questions)
    /// and forget its session, which belongs to that backend: after a backend switch or a new
    /// conversation the next question starts fresh.
    pub(crate) fn end_process(&mut self) {
        self.oc.session = None;
        if let Some(c) = self.oc.child.lock().unwrap_or_else(|p| p.into_inner()).take() {
            crate::ask_claude_code::kill(c);
        }
    }
}

#[cfg(windows)]
pub(crate) fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW: no console flash from a GUI app
}
#[cfg(not(windows))]
pub(crate) fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd
}

/// Windows ships curl.exe since Windows 10 1803; elsewhere it's `curl` on PATH.
pub(crate) const CURL: &str = if cfg!(windows) { "curl.exe" } else { "curl" };

/// Write a file only this user can read: the temp folder is per-user on Windows, but /tmp is
/// shared, and the header file holds the API key.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

fn ant_token() -> Option<String> {
    let out = hidden(std::process::Command::new("ant").args(["auth", "print-credentials", "--access-token"])).output().ok()?;
    let token = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !token.is_empty()).then_some(token)
}

// ---------------------------------------------------------------------------
// Streaming

pub(crate) enum StreamEvent {
    /// Text for the content block at this index.
    Text(usize, String),
    /// A progress note's text for the block at this index.
    Note(usize, String),
    /// A tool call started streaming its input.
    Tool(String),
    /// The whole message: content, stop_reason, usage, model, and `invalid_inputs` (tool use id →
    /// the raw input that wasn't valid JSON).
    Done(Result<Value, String>),
}

/// The message as the SSE events build it.
#[derive(Default)]
struct Acc {
    blocks: Vec<Value>,
    partial: HashMap<usize, String>,
    stop_reason: Value,
    usage: serde_json::Map<String, Value>,
    model: String,
    invalid: serde_json::Map<String, Value>,
    error: Option<String>,
}

impl Acc {
    /// Apply one SSE `data:` payload.
    fn apply(&mut self, data: &Value) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        let idx = data["index"].as_u64().unwrap_or(0) as usize;
        match data["type"].as_str().unwrap_or_default() {
            "message_start" => {
                self.model = data["message"]["model"].as_str().unwrap_or_default().to_string();
                if let Some(u) = data["message"]["usage"].as_object() {
                    self.usage.extend(u.clone());
                }
            }
            "content_block_start" => {
                let block = data["content_block"].clone();
                if self.blocks.len() <= idx {
                    self.blocks.resize(idx + 1, Value::Null);
                }
                match block["type"].as_str().unwrap_or_default() {
                    "tool_use" => {
                        self.partial.insert(idx, String::new());
                        out.push(StreamEvent::Tool(block["name"].as_str().unwrap_or_default().to_string()));
                    }
                    "text" => {
                        if let Some(t) = block["text"].as_str().filter(|t| !t.is_empty()) {
                            out.push(StreamEvent::Text(idx, t.to_string()));
                        }
                    }
                    _ => {}
                }
                self.blocks[idx] = block;
            }
            "content_block_delta" => {
                let d = &data["delta"];
                let Some(block) = self.blocks.get_mut(idx) else { return out };
                let append = |b: &mut Value, key: &str, s: &str| {
                    let now = b[key].as_str().unwrap_or_default().to_string();
                    b[key] = Value::String(now + s);
                };
                match d["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        let t = d["text"].as_str().unwrap_or_default();
                        append(block, "text", t);
                        out.push(StreamEvent::Text(idx, t.to_string()));
                    }
                    "input_json_delta" => self.partial.entry(idx).or_default().push_str(d["partial_json"].as_str().unwrap_or_default()),
                    "thinking_delta" => {
                        let t = d["thinking"].as_str().unwrap_or_default();
                        append(block, "thinking", t);
                        if !t.is_empty() {
                            out.push(StreamEvent::Note(idx, t.to_string()));
                        }
                    }
                    "signature_delta" => append(block, "signature", d["signature"].as_str().unwrap_or_default()),
                    "citations_delta" => {
                        if !block["citations"].is_array() {
                            block["citations"] = json!([]);
                        }
                        if let Some(c) = block["citations"].as_array_mut() {
                            c.push(d["citation"].clone());
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some(raw) = self.partial.remove(&idx)
                    && let Some(block) = self.blocks.get_mut(idx)
                {
                    // Eager input streaming: the API doesn't validate the input, so parse it strictly.
                    match serde_json::from_str::<Value>(if raw.trim().is_empty() { "{}" } else { &raw }) {
                        Ok(v) if v.is_object() => block["input"] = v,
                        _ => {
                            block["input"] = json!({});
                            if let Some(id) = block["id"].as_str() {
                                self.invalid.insert(id.to_string(), Value::String(raw));
                            }
                        }
                    }
                }
            }
            "message_delta" => {
                if !data["delta"]["stop_reason"].is_null() {
                    self.stop_reason = data["delta"]["stop_reason"].clone();
                }
                if !data["delta"]["stop_details"].is_null() {
                    self.usage.insert("stop_details".into(), data["delta"]["stop_details"].clone());
                }
                if let Some(u) = data["usage"].as_object() {
                    self.usage.extend(u.clone());
                }
            }
            "error" => self.error = Some(data["error"]["message"].as_str().unwrap_or("The API reported an error.").to_string()),
            _ => {}
        }
        out
    }

    fn finish(mut self) -> Result<Value, String> {
        if let Some(e) = self.error {
            return Err(format!("API error: {e}"));
        }
        let details = self.usage.remove("stop_details").unwrap_or(Value::Null);
        Ok(json!({
            "content": self.blocks.into_iter().filter(|b| !b.is_null()).collect::<Vec<_>>(),
            "stop_reason": self.stop_reason,
            "stop_details": details,
            "usage": self.usage,
            "model": self.model,
            "invalid_inputs": self.invalid,
        }))
    }
}

/// POST the body with curl and stream the answer back. Headers go through a temp file so the
/// key isn't on a command line.
fn stream_request(body: Value, auth: Auth, betas: Vec<&'static str>, child: Arc<Mutex<Option<Child>>>, tx: Sender<StreamEvent>, wake: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let send = |e: StreamEvent| {
            let _ = tx.send(e);
            wake();
        };
        let tmp = std::env::temp_dir();
        let stamp = format!("umalord-{}-{}", std::process::id(), SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        let body_path = tmp.join(format!("{stamp}.json"));
        let head_path = tmp.join(format!("{stamp}.hdr"));
        let mut headers = vec!["content-type: application/json".to_string(), "anthropic-version: 2023-06-01".to_string()];
        let mut betas = betas;
        match &auth {
            Auth::ApiKey(k) => headers.push(format!("x-api-key: {k}")),
            Auth::Bearer(t) => {
                headers.push(format!("authorization: Bearer {t}"));
                betas.push("oauth-2025-04-20");
            }
        }
        if !betas.is_empty() {
            headers.push(format!("anthropic-beta: {}", betas.join(",")));
        }
        let result = (|| -> Result<Value, String> {
            std::fs::write(&body_path, body.to_string()).map_err(|e| e.to_string())?;
            write_private(&head_path, headers.join("\n").as_bytes()).map_err(|e| e.to_string())?;
            let mut cmd = std::process::Command::new(CURL);
            cmd.args(["-sS", "-N", "--max-time", "900", "-X", "POST", API_URL, "-H", &format!("@{}", head_path.display()), "--data-binary", &format!("@{}", body_path.display()), "-w", "\n%{http_code}"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            hidden(&mut cmd);
            let mut proc = cmd.spawn().map_err(|e| format!("couldn't run {CURL}: {e}"))?;
            let stdout = proc.stdout.take();
            let mut stderr = proc.stderr.take();
            *child.lock().unwrap_or_else(|p| p.into_inner()) = Some(proc);
            let mut acc = Acc::default();
            let mut other = String::new();
            for line in BufReader::new(stdout.ok_or("no output from curl")?).lines().map_while(Result::ok) {
                if let Some(d) = line.strip_prefix("data:") {
                    if let Ok(v) = serde_json::from_str::<Value>(d.trim()) {
                        for e in acc.apply(&v) {
                            send(e);
                        }
                    }
                } else if !line.starts_with("event:") && !line.trim().is_empty() {
                    other.push_str(&line);
                    other.push('\n');
                }
            }
            let mut err = String::new();
            if let Some(e) = stderr.as_mut() {
                let _ = e.read_to_string(&mut err);
            }
            let status = child.lock().unwrap_or_else(|p| p.into_inner()).take().and_then(|mut p| p.wait().ok());
            let Some(status) = status else { return Err("Stopped.".into()) };
            if !status.success() {
                return Err(format!("curl failed: {}", err.trim()));
            }
            let (json_text, code) = other.trim_end().rsplit_once('\n').map_or(("", other.trim()), |(a, b)| (a, b.trim()));
            if code != "200" {
                let msg = serde_json::from_str::<Value>(json_text).ok().and_then(|v| v["error"]["message"].as_str().map(String::from)).unwrap_or_else(|| "unknown error".into());
                return Err(format!("API error {code}: {msg}"));
            }
            acc.finish()
        })();
        let _ = std::fs::remove_file(&body_path);
        let _ = std::fs::remove_file(&head_path);
        send(StreamEvent::Done(result));
    });
}

// ---------------------------------------------------------------------------
// The conversation

impl App {
    fn system_prompt(&self) -> String {
        let mut roster = String::new();
        for u in &self.roster.umas {
            roster.push_str(&format!("- uma {} [{}] {}★ P{}\n", u.name, u.outfit, u.stars, u.potential));
        }
        for c in &self.roster.cards {
            roster.push_str(&format!("- card {} {} {} [{}] LB{} Lv{}\n", c.rarity.label(), c.card_type.label(), c.uma, c.title, c.lb, c.level));
        }
        format!(
            "You are the umaLord assistant inside the user's desktop app for Uma Musume: Pretty Derby (Global/English server). Today is {today}.\n\
             You answer questions about this account (its umas and support cards, what to train and why, decks, skills, parents and sparks, and goals such as \
             Team Trials, Champions Meeting, parent runs and League of Heroes), and you can work the app: draft career plans, choose trainees, decks and skills, \
             run the race simulator, and save plans and results once the user approves.\n\n\
             How to work:\n\
             - Use the tools for every game fact (aptitudes, growth, skill effects, card effects, events). Don't state game numbers from memory; if a tool can't confirm something, say it's unverified.\n\
             - Recommend owned umas and cards first. Read support effects at the owned level, not max level. A support card can't go in the deck of its own character, and a deck holds one card per character plus one borrowed friend card.\n\
             - Label every claim as data (game data from the tools), simulation (simulator output, or a plan's Simulation section), community meta (the meta sheet, get_cm_meta) or opinion. Check insights.md and results/ for the account's real results before giving an opinion, and say how confident you are.\n\
             - For goals by mode (Champions Meeting, Team Trials, League of Heroes, Legend Races) and which training scenario suits what, read knowledge/modes.md; a broad \"get competitive\" ask covers the next CM, Team Trials and the parenting that feeds both.\n\
             - For parents and parenting, read knowledge/parenting.md first and follow its method (calendar → runner → the lineage she needs → what the account has → a queue of parent runs with odds and careers → budget). get_affinity, rank_legacies, plan_lineage and parent_run_odds work out affinity, start aptitudes and spark odds from the game's own rules; never estimate them, and label their results as calculated (exact, from the game's rules), not simulation. Work out the goal from the account and the calendar instead of asking.\n\
             - For how the game works (race mechanics, Wit, HP, Team Trials scoring, the Champions Meeting format, sparks, Grand Concert), courses and skill advice, read knowledge/mechanics.md, knowledge/racecourses.md and knowledge/skills.md with read_note: sourced notes, each section naming its source. get_skill includes the community skill tier list; quote it as community opinion.\n\n\
             Building and testing a career, when the user asks for a plan, a deck, skills or a simulation:\n\
             1. Understand the goal: the event, the trainee or style, what matters to them. Ask only if it's genuinely unclear; otherwise say what you assumed.\n\
             2. For a Champions Meeting: list_upcoming_events, then get_cm_meta for its style ranking, decks and skills. Say whether the CM is announced or estimated, and name the skills it turns off.\n\
             3. Draft: new_plan (with the CM), recommend_trainees or the user's pick, set_plan (trainee, style, target stats: the meta sheet's recommended stats are a fair start), then recommend_deck or set_deck.\n\
             4. Skills: auto_pick_skills, which simulates and optimises within the SP budget.\n\
             5. Verify: run_compare with builds \"plan\" races the trainee without and with her skills to buy; run_stamina when stamina is in doubt.\n\
             6. Present: the result first, then the plan as a short list, each claim labelled, with the course, conditions, races and seed the simulations used. Offer to save it.\n\
             - Draft tools change the Planner and the Race sim at once, and the user can undo each from the chat. new_plan replaces the plan open in the Planner: if one is open (get_plan), say so.\n\
             - save_plan, log_result and update_owned show the user an approval card. Call them only when the user asked for that or agreed to it.\n\
             - Run at most 4 simulations (auto_pick_skills, simulate_skills, run_compare, run_skill_chart, run_stamina, stat_sensitivity) per question unless the user asks for more.\n\
             - Be concise: lead with the answer, then the reasons. Use short markdown lists or tables.\n\n\
             The account's roster when this conversation started ({} umas, {} support cards; get_account_summary has the current one):\n{roster}",
            self.roster.umas.len(),
            self.roster.cards.len(),
            today = crate::ui_util::today(),
        )
    }

    /// Where the user is in the app, sent with each question (appended, so the cache and the
    /// thinking blocks stay valid).
    fn app_state_note(&self) -> String {
        let page = match self.tab {
            crate::Tab::Career => format!("Career → {}", self.career_tab.info().1),
            crate::Tab::Collection => "Collection".to_string(),
            t => t.info().1.to_string(),
        };
        let p = &self.planner.plan;
        let plan = match p.trainee.and_then(|t| self.kb.umas.get(&t)) {
            Some(u) => format!("'{}' ({} [{}], {})", if p.title.is_empty() { "untitled" } else { &p.title }, u.name, u.outfit, p.purpose.label()),
            None if p.title.is_empty() => "empty".to_string(),
            None => format!("'{}' (no trainee yet)", p.title),
        };
        format!("<app_state>Page before the question: {page}. Working plan in the Planner: {plan}. Today: {}.</app_state>", crate::ui_util::today())
    }

    fn send_request(&mut self, ctx: &egui::Context) {
        // Tests drive the loop by hand; they never call the API (a key in the environment would bill it).
        if cfg!(test) {
            self.ask.status = Some(("Tests don't call the API.".into(), false));
            return;
        }
        let Some(auth) = self.ask.auth() else {
            self.ask.status = Some(("No API key. Set ANTHROPIC_API_KEY, enter one in ⚙ Settings → AI assistant, or log in with the ant CLI.".into(), true));
            return;
        };
        if let Auth::ApiKey(k) = &auth
            && crate::ask_claude_code::is_setup_token(k)
        {
            self.ask.status = Some((SETUP_TOKEN_NEEDS_CLAUDE_CODE.into(), true));
            return;
        }
        if self.ask.system.is_none() {
            self.ask.system = Some(self.system_prompt());
        }
        let (model, _, fallbacks, updates) = MODELS[self.ask.model];
        let mut body = json!({
            "model": model,
            "max_tokens": 64000,
            "stream": true,
            // The tools render first, so this breakpoint caches tools and system together.
            "system": [{"type": "text", "text": self.ask.system.clone().unwrap_or_default(), "cache_control": {"type": "ephemeral"}}],
            "tools": crate::agent::api_tools(true),
            "messages": self.ask.messages,
            "output_config": {"effort": EFFORTS[self.ask.effort]},
            // And the growing conversation.
            "cache_control": {"type": "ephemeral"},
        });
        let mut betas = Vec::new();
        if updates {
            // The model's notes between tool calls come back as thinking blocks: ask for their text.
            body["thinking"] = json!({"type": "adaptive", "display": "updates"});
            betas.push("thinking-display-updates-2026-08-18");
        }
        if fallbacks {
            // Server-side refusal fallback: a declined request is re-run on Anthropic's recommended model.
            body["fallbacks"] = json!("default");
            betas.push("server-side-fallback-2026-07-01");
        }
        let (tx, rx) = channel();
        let child: Arc<Mutex<Option<Child>>> = Default::default();
        let wake = ctx.clone();
        stream_request(body, auth, betas, child.clone(), tx, move || wake.request_repaint());
        self.ask.stream = Some(Stream { rx, child, turns: HashMap::new() });
        self.ask.status = None;
    }

    /// An approval card is waiting for the user.
    pub(crate) fn approval_showing(&self) -> bool {
        self.ask.turns.iter().any(|t| matches!(t, Turn::Tool(x) if matches!(x.state, ToolState::Asking(..))))
    }

    pub(crate) fn ask_busy(&self) -> bool {
        self.ask.stream.is_some() || !self.ask.calls.is_empty() || self.ask.oc.rx.is_some()
    }

    /// What's running now, for the line under the chat.
    fn busy_line(&self) -> (String, Option<f32>) {
        for c in &self.ask.calls {
            let label = crate::agent::find(&c.name).map_or(c.name.clone(), |s| s.label.to_string());
            match &c.state {
                CallState::Waiting(w) => {
                    let frac = match w {
                        Wait::Pick { .. } => self.planner.pick_job.as_ref().map(|j| j.fraction()),
                        Wait::SkillValues { .. } => self.planner.skill_job.as_ref().map(|j| j.fraction()),
                        _ => self.sim_job_fraction(),
                    };
                    return (format!("{label}…"), frac);
                }
                CallState::Asking => return ("Waiting for your answer on the card above.".into(), None),
                _ => {}
            }
        }
        if self.approval_showing() {
            return ("Waiting for your answer on the card above.".into(), None);
        }
        if self.ask.stream.is_some() {
            return (self.ask.activity.last().map_or("Thinking…".to_string(), |a| truncate(a, 80)), None);
        }
        let line = match self.ask.activity.last() {
            // OpenCode reports umaLord's tools by name: say what they do.
            Some(a) if a.starts_with("umalord_") => crate::agent::find(a.split(' ').next().unwrap_or_default()).map_or(a.clone(), |s| format!("Done: {}", s.label)),
            Some(a) => format!("Looking up: {}", truncate(a, 80)),
            None => "Working…".to_string(),
        };
        (line, None)
    }

    pub(crate) fn ask_poll(&mut self, ctx: &egui::Context) {
        self.opencode_poll(ctx);
        self.stream_poll(ctx);
        self.advance_calls(ctx);
    }

    fn stream_poll(&mut self, ctx: &egui::Context) {
        let Some(s) = &self.ask.stream else { return };
        let events: Vec<StreamEvent> = s.rx.try_iter().collect();
        for e in events {
            match e {
                StreamEvent::Text(idx, t) => self.stream_text(idx, &t, false),
                StreamEvent::Note(idx, t) => self.stream_text(idx, &t, true),
                StreamEvent::Tool(name) => {
                    let label = crate::agent::find(&name).map_or(name, |s| s.label.to_string());
                    self.ask.activity.push(format!("Preparing: {label}"));
                }
                StreamEvent::Done(result) => {
                    self.ask.stream = None;
                    match result {
                        Ok(msg) => self.handle_message(ctx, msg),
                        Err(e) => {
                            self.ask.status = Some((e, true));
                            self.finish_question(ctx);
                        }
                    }
                    return;
                }
            }
        }
    }

    /// Streamed text goes into its block's turn; a note's into a note turn.
    fn stream_text(&mut self, idx: usize, t: &str, note: bool) {
        let Some(s) = &mut self.ask.stream else { return };
        match s.turns.get(&idx).copied() {
            Some(i) => match &mut self.ask.turns[i] {
                Turn::Assistant(text) | Turn::Note(text) => text.push_str(t),
                _ => {}
            },
            None => {
                s.turns.insert(idx, self.ask.turns.len());
                self.ask.turns.push(if note { Turn::Note(t.to_string()) } else { Turn::Assistant(t.to_string()) });
            }
        }
    }

    fn handle_message(&mut self, ctx: &egui::Context, resp: Value) {
        let u = &resp["usage"];
        self.ask.tokens_in += u["input_tokens"].as_u64().unwrap_or(0) + u["cache_read_input_tokens"].as_u64().unwrap_or(0) + u["cache_creation_input_tokens"].as_u64().unwrap_or(0);
        self.ask.cache_read += u["cache_read_input_tokens"].as_u64().unwrap_or(0);
        self.ask.tokens_out += u["output_tokens"].as_u64().unwrap_or(0);
        let stop = resp["stop_reason"].as_str().unwrap_or_default().to_string();
        if stop == "refusal" {
            // The whole chain declined: nothing of this turn runs, and it isn't echoed (the
            // transcript still ends with the user's message, which the next question joins).
            let why = resp["stop_details"]["explanation"].as_str().unwrap_or("The model declined this request.");
            self.ask.status = Some((format!("Declined: {why}"), true));
            self.finish_question(ctx);
            return;
        }
        let content = resp["content"].as_array().cloned().unwrap_or_default();
        // Echo the assistant turn back unchanged, except blocks before a fallback boundary that
        // the API says to omit, and the fallback marker itself.
        let boundary = content.iter().rposition(|b| b["type"] == "fallback");
        let echoed: Vec<Value> = content
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                let t = b["type"].as_str().unwrap_or_default();
                t != "fallback" && !(boundary.is_some_and(|f| *i < f) && matches!(t, "thinking" | "redacted_thinking" | "tool_use"))
            })
            .map(|(_, b)| b.clone())
            .collect();
        if !echoed.is_empty() {
            self.ask.messages.push(json!({"role": "assistant", "content": echoed}));
        }
        let calls: Vec<Value> = echoed.iter().filter(|b| b["type"] == "tool_use").cloned().collect();
        if calls.is_empty() {
            if stop == "max_tokens" {
                self.ask.status = Some(("The answer hit the length limit and is cut off.".into(), true));
            } else {
                let served = resp["model"].as_str().unwrap_or_default();
                if served != MODELS[self.ask.model].0 && !served.is_empty() {
                    self.ask.status = Some((format!("Answered by {served} (fallback)."), false));
                }
            }
            self.finish_question(ctx);
            return;
        }
        for c in calls {
            let id = c["id"].as_str().unwrap_or_default().to_string();
            let name = c["name"].as_str().unwrap_or_default().to_string();
            let spec = crate::agent::find(&name);
            let turn = self.ask.turns.len();
            self.ask.turns.push(Turn::Tool(ToolTurn { icon: spec.map_or("🔧", |s| s.icon), label: spec.map_or(name.clone(), |s| s.label.to_string()), state: ToolState::Running }));
            let state = if stop == "max_tokens" {
                // A cut-off input often still parses: don't run it.
                CallState::Ready(json!({"type": "tool_result", "tool_use_id": id, "is_error": true, "content": "Your output hit the length limit before this tool's input was complete, so it didn't run. Call it again with a shorter input."}))
            } else if let Some(raw) = resp["invalid_inputs"].get(&id) {
                CallState::Ready(json!({"type": "tool_result", "tool_use_id": id, "is_error": true, "content": json!({"INVALID_JSON": raw}).to_string()}))
            } else {
                CallState::Queued
            };
            if let CallState::Ready(_) = state
                && let Turn::Tool(t) = &mut self.ask.turns[turn]
            {
                t.state = ToolState::Failed(Card { summary: "Its input was cut off or not valid JSON; the model was told to try again.".into(), ..Default::default() });
            }
            self.ask.activity.push(format!("{name} {}", c["input"]));
            self.ask.calls.push(Call { id, name, input: c["input"].clone(), turn, state });
        }
        self.ask.rounds += 1;
        self.advance_calls(ctx);
    }

    fn set_tool_turn(&mut self, turn: usize, out: &ToolOut) {
        if let Some(Turn::Tool(t)) = self.ask.turns.get_mut(turn) {
            t.state = if out.is_error { ToolState::Failed(out.card.clone()) } else { ToolState::Done(out.card.clone()) };
        }
    }

    /// Run the round's tool calls in order (a later call may need an earlier one's result), then
    /// send all their results together.
    fn advance_calls(&mut self, ctx: &egui::Context) {
        if self.ask.calls.is_empty() {
            return;
        }
        let mut calls = std::mem::take(&mut self.ask.calls);
        let answer = self.ask.answer.take();
        for i in 0..calls.len() {
            let result = |c: &Call, out: &ToolOut| json!({"type": "tool_result", "tool_use_id": c.id, "is_error": out.is_error, "content": out.text});
            loop {
                let next = match &calls[i].state {
                    CallState::Ready(_) => break,
                    CallState::Queued => match self.agent_call(ctx, &calls[i].name, &calls[i].input) {
                        Exec::Done(out) => {
                            self.set_tool_turn(calls[i].turn, &out);
                            CallState::Ready(result(&calls[i], &out))
                        }
                        Exec::Wait(w) => CallState::Waiting(w),
                        Exec::Ask(a) if self.ask.auto_approved(&calls[i].name, &a) => {
                            let out = self.agent_write(&calls[i].name, &calls[i].input);
                            self.set_tool_turn(calls[i].turn, &out);
                            CallState::Ready(result(&calls[i], &out))
                        }
                        Exec::Ask(a) => {
                            if let Some(Turn::Tool(t)) = self.ask.turns.get_mut(calls[i].turn) {
                                t.state = ToolState::Asking(a.question, None);
                            }
                            CallState::Asking
                        }
                    },
                    CallState::Waiting(w) => match self.agent_wait(*w) {
                        Some(out) => {
                            self.set_tool_turn(calls[i].turn, &out);
                            CallState::Ready(result(&calls[i], &out))
                        }
                        None => {
                            ctx.request_repaint_after(Duration::from_millis(150));
                            self.ask.calls = calls;
                            return;
                        }
                    },
                    CallState::Asking => match answer {
                        Some((turn, yes)) if turn == calls[i].turn => {
                            let out = if yes { self.agent_write(&calls[i].name, &calls[i].input) } else { ToolOut::err("The user declined.") };
                            self.set_tool_turn(calls[i].turn, &out);
                            CallState::Ready(result(&calls[i], &out))
                        }
                        _ => {
                            self.ask.calls = calls;
                            return;
                        }
                    },
                };
                calls[i].state = next;
            }
        }
        let results: Vec<Value> = calls.into_iter().filter_map(|c| if let CallState::Ready(v) = c.state { Some(v) } else { None }).collect();
        self.ask.messages.push(json!({"role": "user", "content": results}));
        let over_time = self.ask.started.is_some_and(|s| s.elapsed() > QUESTION_LIMIT);
        if self.ask.rounds >= MAX_TOOL_ROUNDS || over_time {
            // The transcript stays valid: the next question joins these tool results.
            self.ask.status = Some(("Stopped after many steps. Ask it to carry on, or ask something narrower.".into(), true));
            self.finish_question(ctx);
            return;
        }
        self.send_request(ctx);
    }

    /// The answer is done: open the page the assistant asked for.
    fn finish_question(&mut self, ctx: &egui::Context) {
        self.ask.started = None;
        if let Some(at) = self.agent.open_after.take()
            // A screenshot of a question (UMALORD_ASK) shows the chat.
            && !self.debug_asked
        {
            self.open_at(ctx, &at);
        }
    }

    /// Stop the answer: the stream, the running jobs, and the tool calls left (they get "Stopped"
    /// results, so the transcript stays valid).
    fn stop_answer(&mut self, ctx: &egui::Context) {
        if let Some(s) = self.ask.stream.take() {
            if let Some(mut c) = s.child.lock().unwrap_or_else(|p| p.into_inner()).take() {
                let _ = c.kill();
            }
            // Drop what streamed of the unfinished turn: it never reached the transcript.
            let mut gone: Vec<usize> = s.turns.values().copied().collect();
            gone.sort_unstable();
            for i in gone.into_iter().rev() {
                if i < self.ask.turns.len() {
                    self.ask.turns.remove(i);
                }
            }
        }
        let calls = std::mem::take(&mut self.ask.calls);
        if !calls.is_empty() {
            let mut results = Vec::new();
            for c in calls {
                if let CallState::Waiting(w) = c.state {
                    self.agent_cancel(w);
                }
                let v = match c.state {
                    CallState::Ready(v) => v,
                    _ => {
                        if let Some(Turn::Tool(t)) = self.ask.turns.get_mut(c.turn) {
                            t.state = ToolState::Failed(Card { summary: "Stopped.".into(), ..Default::default() });
                        }
                        json!({"type": "tool_result", "tool_use_id": c.id, "is_error": true, "content": "Stopped by the user."})
                    }
                };
                results.push(v);
            }
            self.ask.messages.push(json!({"role": "user", "content": results}));
        }
        if let Some(child) = self.ask.oc.child.lock().unwrap_or_else(|p| p.into_inner()).take() {
            crate::ask_claude_code::kill(child);
        }
        self.ask.status = Some(("Stopped.".into(), true));
        self.finish_question(ctx);
    }

    /// Kill the request and OpenCode, if running (the window is closing).
    pub(crate) fn stop_ask_processes(&mut self) {
        let children = [self.ask.stream.as_ref().map(|s| s.child.clone()), Some(self.ask.oc.child.clone())];
        for child in children.into_iter().flatten() {
            if let Some(mut c) = child.lock().unwrap_or_else(|p| p.into_inner()).take() {
                let _ = c.kill();
            }
        }
    }

    pub(crate) fn submit_question(&mut self, ctx: &egui::Context, q: String) {
        self.ask.turns.push(Turn::User(q.clone()));
        self.ask.activity.clear();
        self.ask.status = None;
        self.agent.sims = 0;
        self.agent.open_after = None;
        if self.ask.backend == Backend::OpenCode || self.ask.via_claude_code() {
            self.send_opencode(ctx, q);
            return;
        }
        let blocks = [json!({"type": "text", "text": q}), json!({"type": "text", "text": self.app_state_note()})];
        // After a refusal or a stopped tool loop the transcript already ends with a user message;
        // add the question to it rather than starting a second one.
        match self.ask.messages.last_mut() {
            Some(last) if last["role"] == "user" => {
                if let Some(content) = last["content"].as_array_mut() {
                    content.extend(blocks);
                }
            }
            _ => self.ask.messages.push(json!({"role": "user", "content": blocks})),
        }
        self.ask.rounds = 0;
        self.ask.started = Some(Instant::now());
        self.send_request(ctx);
    }

    /// Open a page from a chat card or `open_page`.
    pub(crate) fn open_at(&mut self, ctx: &egui::Context, at: &OpenAt) {
        use crate::{CareerTab, CollectionTab, Jump, Tab};
        match at {
            OpenAt::Home => self.tab = Tab::Home,
            OpenAt::Plan => self.go_career(CareerTab::Plan),
            OpenAt::Race => self.go_career(CareerTab::Race),
            OpenAt::Results(i) => {
                self.show_results_log(*i);
                self.go_career(CareerTab::Results);
            }
            OpenAt::Uma(id) => self.jump.set(Some(Jump::Uma(*id))),
            OpenAt::Card(id) => self.jump.set(Some(Jump::Card(*id))),
            OpenAt::Skill(id) => self.jump.set(Some(Jump::Skill(*id))),
            OpenAt::Umas | OpenAt::Cards | OpenAt::Skills => {
                self.tab = Tab::Collection;
                self.collection_tab = match at {
                    OpenAt::Umas => CollectionTab::Umas,
                    OpenAt::Cards => CollectionTab::Cards,
                    _ => CollectionTab::Skills,
                };
            }
            OpenAt::DraftPlan | OpenAt::DraftRace => {
                let r = self.open_bridge_draft(ctx, *at == OpenAt::DraftRace);
                self.toast_result(r);
            }
        }
    }

    // -----------------------------------------------------------------------
    // OpenCode

    /// Ask OpenCode, or Claude through Claude Code: both run as a process in the umaLord folder
    /// with umaLord's tools over MCP, and report back as `OcEvent`s.
    fn send_opencode(&mut self, ctx: &egui::Context, q: String) {
        let claude = self.ask.via_claude_code();
        let exe = if claude { self.ask.oc.claude_exe.clone() } else { self.ask.oc.exe.clone() };
        let (Some(exe), Some(root)) = (exe, self.root.clone()) else {
            let msg = match (self.root.is_some(), claude) {
                (false, _) => "The umaLord folder wasn't found, so the assistant has nothing to read.",
                (true, true) => "Claude Code isn't installed: claude isn't on PATH.",
                (true, false) => "OpenCode isn't installed: it isn't in tools/opencode or on PATH.",
            };
            self.ask.status = Some((msg.into(), true));
            return;
        };
        self.write_digest();
        let bridge = crate::agent::bridge::dir();
        if let Some(dir) = &bridge {
            if self.ask.oc.session.is_none() {
                // A new conversation starts without the last one's draft.
                crate::agent::bridge::clear(dir);
            }
            let _ = std::fs::create_dir_all(dir);
        }
        self.ask.oc.draft_seen = bridge.as_deref().and_then(crate::agent::bridge::draft_time);
        self.ask.oc.plans_before = self.plan_times();
        let ctx = ctx.clone();
        self.ask.oc.answer_open = false;
        if claude {
            let mcp = std::env::current_exe().ok().map(|me| crate::ask_claude_code::mcp_config(&me, &root, bridge.as_deref()));
            let request = crate::ask_claude_code::Request {
                exe,
                root,
                model: MODELS[self.ask.model].0.to_string(),
                effort: EFFORTS[self.ask.effort].to_string(),
                credential: crate::ask_claude_code::Credential::from_key(self.ask.claude_key().as_deref()),
                session: self.ask.oc.session.clone(),
                question: q,
                mcp,
            };
            self.ask.oc.rx = Some(crate::ask_claude_code::ask(request, self.ask.oc.child.clone(), move || ctx.request_repaint()));
            self.ask.status = None;
            return;
        }
        let mcp = std::env::current_exe().ok().map(|exe| crate::ask_opencode::mcp_config(&exe, &root, bridge.as_deref()));
        self.ask.oc.rx = Some(crate::ask_opencode::ask(
            exe,
            root,
            self.ask.oc.model.clone(),
            self.ask.oc.session.clone(),
            q,
            mcp,
            self.ask.oc.child.clone(),
            move || ctx.request_repaint(),
        ));
        self.ask.status = None;
    }

    /// When each plan in plans/ was last written.
    fn plan_times(&self) -> HashMap<PathBuf, SystemTime> {
        let Some(root) = &self.root else { return HashMap::new() };
        std::fs::read_dir(root.join("plans"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
            .filter(|(p, _)| p.extension().is_some_and(|x| x == "md") && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('_')))
            .collect()
    }

    /// Plans the assistant saved since the question started: a toast with Open for each.
    fn check_saved_plans(&mut self) {
        let now = self.plan_times();
        let mut changed: Vec<PathBuf> = now.iter().filter(|(p, t)| self.ask.oc.plans_before.get(*p) != Some(*t)).map(|(p, _)| p.clone()).collect();
        if changed.is_empty() {
            return;
        }
        changed.sort();
        for path in changed {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let title = text.lines().find_map(|l| l.strip_prefix("# ")).unwrap_or_default().to_string();
            self.toast_action(format!("The assistant saved '{title}'"), crate::ui_util::ToastKind::Success, "Open", crate::ui_util::ToastAction::OpenPlan(path));
        }
        self.ask.oc.plans_before = now;
        self.planner.refresh_files(self.root.as_deref());
        self.home_dirty();
    }

    /// While OpenCode answers: show the server's approval questions, collect its page requests,
    /// note a new draft, and toast plans it saved.
    fn bridge_poll(&mut self, ctx: &egui::Context, done: bool) {
        let Some(dir) = crate::agent::bridge::dir() else { return };
        if !done && self.ask.oc.scanned.is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
            return;
        }
        self.ask.oc.scanned = Some(Instant::now());
        for p in crate::agent::bridge::pending_approvals(&dir) {
            if !self.ask.oc.shown.insert(p.id) {
                continue;
            }
            let spec = crate::agent::find(&p.tool);
            let auto = (p.tool == "save_plan" && self.ask.auto_save) || (p.tool == "log_result" && self.ask.auto_log);
            let state = if auto {
                crate::agent::bridge::answer(&dir, p.id, true);
                ToolState::Done(Card { summary: format!("{} (allowed in Settings)", p.question.trim_end_matches('?')), ..Default::default() })
            } else {
                ToolState::Asking(p.question.clone(), Some(p.id))
            };
            self.ask.turns.push(Turn::Tool(ToolTurn { icon: spec.map_or("✏", |s| s.icon), label: spec.map_or(p.tool.clone(), |s| s.label.to_string()), state }));
        }
        for at in crate::agent::bridge::take_ui_requests(&dir) {
            self.agent.open_after = Some(at);
        }
        let draft = crate::agent::bridge::draft_time(&dir);
        if draft.is_some() && draft != self.ask.oc.draft_seen {
            self.ask.oc.draft_seen = draft;
            if let Some(d) = crate::agent::bridge::read_draft(&dir) {
                let trainee = d.plan.trainee.and_then(|t| self.kb.umas.get(&t)).map_or("no trainee yet".to_string(), |u| format!("{} [{}]", u.name, u.outfit));
                let title = if d.plan.title.is_empty() { d.plan.purpose.label().to_string() } else { d.plan.title.clone() };
                let mut card = Card { summary: format!("{title} · {trainee} · {} cards · {} skills", d.plan.deck.iter().flatten().count() + usize::from(d.plan.friend.is_some()), d.plan.skills.len()), ..Default::default() };
                card.visual = Visual::Deck { cards: d.plan.deck.iter().flatten().copied().collect(), friend: d.plan.friend };
                card.open.push(OpenAt::DraftPlan);
                if d.compared {
                    card.open.push(OpenAt::DraftRace);
                }
                // One card for the draft in this answer, kept last: the newest replaces it.
                let since = self.ask.turns.iter().rposition(|t| matches!(t, Turn::User(_))).map_or(0, |i| i + 1);
                if let Some(old) = self.ask.turns[since..].iter().position(|t| matches!(t, Turn::Tool(x) if x.label == DRAFT_CARD)) {
                    self.ask.turns.remove(since + old);
                }
                self.ask.turns.push(Turn::Tool(ToolTurn { icon: "📋", label: DRAFT_CARD.into(), state: ToolState::Done(card) }));
            }
        }
        if done || self.ask.oc.plans_scanned.is_none_or(|t| t.elapsed() > Duration::from_secs(2)) {
            self.ask.oc.plans_scanned = Some(Instant::now());
            self.check_saved_plans();
        }
        ctx.request_repaint_after(Duration::from_millis(300));
    }

    pub(crate) fn start_opencode_install(&mut self, ctx: &egui::Context) {
        let Some(root) = self.root.clone() else { return };
        let ctx = ctx.clone();
        self.ask.oc.install_error = None;
        self.ask.oc.install_progress = 0;
        self.ask.oc.install_rx = Some(crate::ask_opencode::install(crate::ask_opencode::install_dir(&root), move || ctx.request_repaint()));
    }

    /// Download card shown while OpenCode isn't installed.
    pub(crate) fn opencode_install_ui(&mut self, ui: &mut egui::Ui) {
        use crate::ask_opencode::{RELEASE, ZIP_SIZE};
        let p = pal(ui);
        let mb = |b: u64| b as f32 / 1_048_576.0;
        ui.label(semibold("OpenCode isn't installed yet", 16.0));
        ui.label(RichText::new("It answers your questions for free, with no key and no account.").color(p.weak));
        ui.add_space(6.0);
        // The pinned download is the Windows build; elsewhere OpenCode comes from the package
        // manager (it's in the Nix dev shell) and `find_exe` picks it up from PATH.
        if !cfg!(windows) {
            ui.label("Install `opencode` with your package manager so it's on PATH, then restart umaLord.");
            ui.label(RichText::new("With Nix: it's in this repo's dev shell (nix develop), or run nix shell nixpkgs#opencode.").size(11.5).color(p.weak));
            return;
        }
        if self.ask.oc.install_rx.is_some() {
            let done = self.ask.oc.install_progress;
            let bar = egui::ProgressBar::new((done as f32 / ZIP_SIZE as f32).min(1.0))
                .text(format!("Downloading… {:.1} / {:.1} MB", mb(done), mb(ZIP_SIZE)))
                .desired_width(ui.available_width().min(420.0));
            ui.add(bar);
        } else {
            let button = crate::ui_util::primary_button(ui, format!("⬇ Download OpenCode ({:.0} MB)", mb(ZIP_SIZE)), 14.0).min_size(vec2(240.0, 36.0));
            if ui.add_enabled(self.root.is_some(), button).clicked() {
                let ctx = ui.ctx().clone();
                self.start_opencode_install(&ctx);
            }
        }
        if let Some(err) = &self.ask.oc.install_error {
            ui.label(RichText::new(err).color(pal(ui).warn));
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("Official release {RELEASE} from github.com/anomalyco/opencode. The app checks its SHA-256 against the published one before installing it to tools\\opencode."))
                .size(11.5)
                .color(p.weak),
        );
    }

    fn opencode_poll(&mut self, ctx: &egui::Context) {
        use crate::ask_opencode::{InstallEvent, OcEvent};
        if let Some(rx) = &self.ask.oc.install_rx {
            for e in rx.try_iter().collect::<Vec<_>>() {
                match e {
                    InstallEvent::Progress(n) => self.ask.oc.install_progress = n,
                    InstallEvent::Done(Ok(exe)) => {
                        self.ask.oc.install_rx = None;
                        self.ask.oc.models_rx = Some(crate::ask_opencode::list_models(exe.clone()));
                        self.ask.oc.exe = Some(exe);
                        self.ask.status = Some(("OpenCode is installed. Ask away.".into(), false));
                    }
                    InstallEvent::Done(Err(e)) => {
                        self.ask.oc.install_rx = None;
                        self.ask.oc.install_error = Some(e);
                    }
                }
            }
        }
        if let Some(rx) = &self.ask.oc.models_rx
            && let Ok(result) = rx.try_recv()
        {
            self.ask.oc.models_rx = None;
            match result {
                Ok(models) => {
                    if !models.contains(&self.ask.oc.model) {
                        let fallback = models.iter().find(|m| m.as_str() == crate::ask_opencode::DEFAULT_MODEL).or(models.first());
                        self.ask.oc.model = fallback.cloned().unwrap_or_default();
                    }
                    self.ask.oc.models = models;
                }
                Err(e) => self.ask.status = Some((format!("Couldn't list OpenCode models: {e}"), true)),
            }
        }
        let Some(rx) = &self.ask.oc.rx else { return };
        let events: Vec<OcEvent> = rx.try_iter().collect();
        for e in events {
            match e {
                OcEvent::Session(s) => self.ask.oc.session = Some(s),
                OcEvent::Tool(t) => {
                    self.ask.activity.push(format!("{} {}", t.tool, t.target).trim().to_string());
                    // The app's own tools get a card; file reads stay in the Lookups list.
                    if let Some(spec) = crate::agent::find(&t.tool).filter(|_| t.tool.starts_with("umalord_")) {
                        let summary = t.output.lines().find(|l| !l.trim().is_empty()).map(|l| truncate(l, 110)).unwrap_or_default();
                        let card = Card { summary, ..Default::default() };
                        let state = if t.error { ToolState::Failed(card) } else { ToolState::Done(card) };
                        self.ask.turns.push(Turn::Tool(ToolTurn { icon: spec.icon, label: spec.label.to_string(), state }));
                        self.ask.oc.answer_open = false;
                    }
                }
                OcEvent::Tokens(i, o) => {
                    self.ask.tokens_in += i;
                    self.ask.tokens_out += o;
                }
                OcEvent::TextDelta(t) => match self.ask.turns.last_mut() {
                    Some(Turn::Assistant(last)) if self.ask.oc.answer_open => last.push_str(&t),
                    _ => {
                        self.ask.turns.push(Turn::Assistant(t));
                        self.ask.oc.answer_open = true;
                    }
                },
                OcEvent::Text(t) => match self.ask.turns.last_mut() {
                    Some(Turn::Assistant(last)) if self.ask.oc.answer_open => {
                        last.push_str("\n\n");
                        last.push_str(&t);
                    }
                    _ => {
                        self.ask.turns.push(Turn::Assistant(t));
                        self.ask.oc.answer_open = true;
                    }
                },
                OcEvent::Error(msg) => {
                    let who = if self.ask.via_claude_code() { "Claude Code" } else { "OpenCode" };
                    self.ask.status = Some((format!("{who}: {msg}"), true));
                }
                OcEvent::Done(result) => {
                    self.ask.oc.rx = None;
                    self.bridge_poll(ctx, true);
                    if let Err(msg) = result {
                        self.ask.status = Some((msg, true));
                    } else if !self.ask.turns.iter().rev().take_while(|t| !matches!(t, Turn::User(_))).any(|t| matches!(t, Turn::Assistant(_))) && self.ask.status.is_none() {
                        let who = if self.ask.via_claude_code() { "Claude Code" } else { "OpenCode" };
                        self.ask.status = Some((format!("{who} finished without an answer. Try again or pick another model."), true));
                    }
                    // Questions the server asked and nobody answered are moot now.
                    for t in &mut self.ask.turns {
                        if let Turn::Tool(x) = t
                            && matches!(x.state, ToolState::Asking(..))
                        {
                            x.state = ToolState::Failed(Card { summary: "Not answered in time.".into(), ..Default::default() });
                        }
                    }
                    self.finish_question(ctx);
                    ctx.request_repaint();
                    return;
                }
            }
        }
        self.bridge_poll(ctx, false);
    }

    // -----------------------------------------------------------------------
    // The tab

    pub(crate) fn ask_tab(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        egui::Panel::right("ask_settings").resizable(false).exact_size(if ui.available_width() < 1100.0 { 240.0 } else { 300.0 }).frame(crate::side_frame()).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.ask_settings(ui));
        });
        egui::Panel::bottom("ask_input").resizable(false).frame(egui::Frame::new().inner_margin(Margin::symmetric(18, 10))).show(ui, |ui| {
            let busy = self.ask_busy();
            egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(CornerRadius::same(14)).inner_margin(Margin::same(8)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let edit = ui.add_sized(
                        [ui.available_width() - 96.0, 56.0],
                        egui::TextEdit::multiline(&mut self.ask.input).hint_text("Ask about your account, or ask it to plan and simulate a career…  (Enter to send, Shift+Enter for a new line)").frame(egui::Frame::new()),
                    );
                    let enter = edit.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                    if busy {
                        let stop = egui::Button::new(semibold("■ Stop", 14.0)).min_size(vec2(84.0, 40.0));
                        if ui.add(stop).on_hover_text("Stop the answer and any simulation it started").clicked() {
                            let ctx = ui.ctx().clone();
                            self.stop_answer(&ctx);
                        }
                        return;
                    }
                    let send = crate::ui_util::primary_button(ui, "Send ➤", 14.0).min_size(vec2(84.0, 40.0));
                    if (ui.add(send).clicked() || enter) && !self.ask.input.trim().is_empty() {
                        let q = self.ask.input.trim().to_string();
                        self.ask.input.clear();
                        let ctx = ui.ctx().clone();
                        self.submit_question(&ctx, q);
                    }
                });
            });
        });
        let mut actions: Vec<ChatAction> = Vec::new();
        egui::CentralPanel::default().frame(crate::page_frame()).show(ui, |ui| {
            crate::ui_util::page_header(
                ui,
                "💬 Ask",
                "Ask in plain English, like you'd ask a friend who knows the game. It answers from your roster and the game data, and can plan and simulate careers for you.",
                |_| {},
            );
            // An empty chat starts at the top; a conversation follows its newest line.
            egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(!self.ask.turns.is_empty()).show(ui, |ui| {
                // Cap, never widen: set_max_width alone can make the column wider than the
                // panel, and everything centred or wrapped in it then runs off screen.
                ui.set_max_width(ui.available_width().min(920.0));
                let needs_install = self.ask.backend == Backend::OpenCode && self.ask.oc.exe.is_none();
                if self.ask.turns.is_empty() && needs_install {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.set_max_width(ui.available_width().min(520.0));
                        crate::ui_util::card(ui, |ui| self.opencode_install_ui(ui));
                    });
                } else if self.ask.turns.is_empty() {
                    // Fit the visible width: a long suggestion would widen the column off screen.
                    let w = (ui.clip_rect().width() - 24.0).clamp(280.0, 620.0);
                    ui.vertical_centered(|ui| {
                        ui.set_max_width(w);
                        ui.add_space(24.0);
                        if crate::theme::chibi() == crate::theme::Chibi::Off {
                            ui.label(RichText::new("💬").size(44.0).color(p.weak));
                        } else {
                            crate::ui_util::chibi(ui, 5, 110.0);
                        }
                        ui.label(semibold("What do you want to know?", 20.0));
                        let who = match self.ask.backend {
                            Backend::OpenCode => "A free model (via OpenCode)",
                            Backend::ClaudeApi if self.ask.via_claude_code() => "Claude (via Claude Code)",
                            Backend::ClaudeApi => "Claude",
                        };
                        ui.label(
                            RichText::new(format!(
                                "{who} looks things up in your roster, the uma.guide data, your plans and results, labels what is data and what is opinion, and can build a plan and race it in the simulator. You can undo every change it makes."
                            ))
                            .color(p.weak),
                        );
                        ui.add_space(16.0);
                        ui.label(RichText::new("Try one of these:").size(12.5).color(p.weak));
                        for s in SUGGESTIONS {
                            let b = egui::Button::new(RichText::new(s).size(13.5)).wrap().corner_radius(CornerRadius::same(12)).min_size(vec2(w, 34.0));
                            if ui.add(b).clicked() {
                                actions.push(ChatAction::Ask(s.to_string()));
                            }
                        }
                    });
                }
                for (i, turn) in self.ask.turns.iter().enumerate() {
                    ui.add_space(if matches!(turn, Turn::Tool(_) | Turn::Note(_)) { 4.0 } else { 10.0 });
                    match turn {
                        Turn::User(text) => {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                egui::Frame::new()
                                    .fill(p.accent.gamma_multiply(0.18))
                                    .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.45)))
                                    .corner_radius(CornerRadius { nw: 14, ne: 4, sw: 14, se: 14 })
                                    .inner_margin(Margin::symmetric(14, 10))
                                    .show(ui, |ui| {
                                        ui.set_max_width(visible(ui, 620.0));
                                        ui.label(text);
                                    });
                            });
                        }
                        Turn::Assistant(text) => {
                            ui.horizontal_top(|ui| {
                                ui.label(RichText::new("✳").size(20.0).color(Color32::from_rgb(217, 119, 87)));
                                egui::Frame::new()
                                    .fill(p.surface)
                                    .stroke(Stroke::new(1.0, p.border))
                                    .corner_radius(CornerRadius { nw: 4, ne: 14, sw: 14, se: 14 })
                                    .inner_margin(Margin::symmetric(16, 12))
                                    .show(ui, |ui| {
                                        ui.set_max_width(visible(ui, 780.0));
                                        egui_commonmark::CommonMarkViewer::new().show(ui, &mut self.md_cache, text);
                                    });
                            });
                        }
                        Turn::Note(text) => {
                            ui.horizontal_top(|ui| {
                                ui.add_space(30.0);
                                ui.add(egui::Label::new(RichText::new(format!("💭 {}", text.trim())).size(size::SM).italics().color(p.weak)).wrap());
                            });
                        }
                        Turn::Tool(t) => self.tool_card(ui, i, t, &mut actions),
                    }
                }
                if self.ask_busy() && !self.ask.calls.iter().any(|c| matches!(c.state, CallState::Asking)) {
                    ui.add_space(10.0);
                    let (line, frac) = self.busy_line();
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(line).color(p.weak));
                        if let Some(f) = frac {
                            ui.add(egui::ProgressBar::new(f).desired_width(160.0).show_percentage());
                        }
                    });
                }
                if let Some((msg, err)) = &self.ask.status {
                    ui.add_space(8.0);
                    ui.label(RichText::new(msg).color(if *err { pal(ui).warn } else { pal(ui).good }));
                }
                ui.add_space(8.0);
            });
        });
        let ctx = ui.ctx().clone();
        for a in actions {
            self.chat_action(&ctx, a);
        }
    }

    /// One tool call in the chat: what ran, a small picture of the result, and its buttons.
    fn tool_card(&self, ui: &mut egui::Ui, i: usize, t: &ToolTurn, actions: &mut Vec<ChatAction>) {
        let p = pal(ui);
        let (border, fill) = match &t.state {
            ToolState::Asking(..) => (p.accent, p.accent.gamma_multiply(0.10)),
            ToolState::Failed(_) => (p.warn.gamma_multiply(0.6), p.surface),
            _ => (p.border, p.surface),
        };
        ui.horizontal_top(|ui| {
            ui.add_space(30.0);
            egui::Frame::new().fill(fill).stroke(Stroke::new(1.0, border)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                // A set width: inside a row, a frame shrinks to its content, and the summary
                // (which truncates) and the buttons (which wrap) would squeeze to nothing.
                ui.set_width(visible(ui, 740.0));
                // The frame sits in a row: stack its contents.
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t.icon).size(15.0));
                        ui.label(semibold(&t.label, size::BODY));
                        match &t.state {
                            ToolState::Running => {
                                ui.spinner();
                            }
                            ToolState::Done(c) | ToolState::Failed(c) => {
                                let color = if matches!(t.state, ToolState::Failed(_)) { p.warn } else { p.weak };
                                ui.add(egui::Label::new(RichText::new(&c.summary).size(size::SM).color(color)).truncate());
                            }
                            ToolState::Asking(..) => {}
                        }
                    });
                    match &t.state {
                        ToolState::Asking(question, _) => {
                            ui.add(egui::Label::new(RichText::new(question).size(size::BODY)).wrap());
                            ui.horizontal(|ui| {
                                if ui.add(crate::ui_util::primary_button(ui, "✔ Approve", 13.5)).clicked() {
                                    actions.push(ChatAction::Answer(i, true));
                                }
                                if ui.button("✖ Deny").clicked() {
                                    actions.push(ChatAction::Answer(i, false));
                                }
                                ui.label(RichText::new("Settings → AI assistant can allow plan saves and result logs without asking.").size(size::XS).color(p.weak));
                            });
                        }
                        ToolState::Done(c) | ToolState::Failed(c) => {
                            self.card_visual(ui, &c.visual);
                            let undo = c.undo.filter(|u| *u < self.agent.undo.len());
                            if !c.open.is_empty() || undo.is_some() {
                                ui.horizontal_wrapped(|ui| {
                                    for at in &c.open {
                                        if ui.small_btn(at.label()).clicked() {
                                            actions.push(ChatAction::Open(at.clone()));
                                        }
                                    }
                                    if let Some(u) = undo
                                        && ui.small_btn("↶ Undo").on_hover_text("Put back the plan and Race sim as they were before this step (and every later step)").clicked()
                                    {
                                        actions.push(ChatAction::Undo(u));
                                    }
                                });
                            }
                        }
                        ToolState::Running => {}
                    }
                });
            });
        });
    }

    fn card_visual(&self, ui: &mut egui::Ui, v: &Visual) {
        let p = pal(ui);
        match v {
            Visual::None => {}
            Visual::Deck { cards, friend } => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (id, borrowed) in cards.iter().map(|c| (*c, false)).chain(friend.map(|f| (f, true))) {
                        let name = self.kb.cards.get(&id).map_or(format!("Card {id}"), |c| format!("{} [{}]", c.uma, c.title));
                        let size = vec2(40.0, 40.0 * 346.0 / 260.0);
                        let r = match self.art_uri("cards", id) {
                            Some(uri) => ui.add(egui::Image::new(uri).fit_to_exact_size(size).corner_radius(CornerRadius::same(5))),
                            None => ui.add_sized(size, egui::Label::new(RichText::new("?").color(p.weak))),
                        };
                        r.on_hover_text(if borrowed { format!("{name} (borrowed friend card)") } else { name });
                        if borrowed {
                            ui.label(RichText::new("+ friend").size(size::XS).color(p.weak));
                        }
                    }
                });
            }
            Visual::Verdict { line, gaps } => {
                ui.label(semibold(line, size::MD));
                let mut sorted = gaps.clone();
                sorted.sort_by(f64::total_cmp);
                ui.allocate_ui(vec2(360.0, 40.0), |ui| {
                    crate::charts::mini_hist(ui, &sorted, crate::charts::uma_color(1, ui.visuals().dark_mode), "L", 36.0).on_hover_text("Uma 2's lead in every race, in lengths (simulation)");
                });
            }
            Visual::Skills(ids) => {
                ui.horizontal_wrapped(|ui| {
                    for id in ids.iter().take(14) {
                        self.skill_link(ui, *id, false);
                    }
                    if ids.len() > 14 {
                        ui.label(RichText::new(format!("+{} more", ids.len() - 14)).size(size::SM).color(p.weak));
                    }
                });
            }
            Visual::Bars(items) => {
                let items: Vec<(String, f64)> = items.iter().take(8).cloned().collect();
                ui.allocate_ui(vec2(520.0, 22.0 * items.len() as f32 + 6.0), |ui| {
                    crate::charts::hbars(ui, &items, "L", p.good, p.bad);
                });
            }
        }
    }

    fn chat_action(&mut self, ctx: &egui::Context, a: ChatAction) {
        match a {
            ChatAction::Ask(q) => self.submit_question(ctx, q),
            ChatAction::Open(at) => self.open_at(ctx, &at),
            ChatAction::Undo(u) => {
                let r = self.agent_undo(u);
                self.toast_result(r);
            }
            ChatAction::Answer(turn, yes) => {
                if let Some(Turn::Tool(t)) = self.ask.turns.get(turn)
                    && let ToolState::Asking(question, bridge) = &t.state
                {
                    if let (Some(id), Some(dir)) = (*bridge, crate::agent::bridge::dir()) {
                        // OpenCode's server waits for this file.
                        crate::agent::bridge::answer(&dir, id, yes);
                        let summary = if yes { format!("Approved: {}", question.trim_end_matches('?')) } else { "You declined.".to_string() };
                        let card = Card { summary, ..Default::default() };
                        if let Some(Turn::Tool(t)) = self.ask.turns.get_mut(turn) {
                            t.state = if yes { ToolState::Done(card) } else { ToolState::Failed(card) };
                        }
                    } else {
                        self.ask.answer = Some((turn, yes));
                        self.advance_calls(ctx);
                    }
                }
            }
        }
    }

    /// The Ask tab's side panel: who answers (changed in Settings → AI assistant) and this
    /// conversation.
    fn ask_settings(&mut self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let busy = self.ask_busy();
        filter_heading(ui, "Answered by");
        let (who, detail) = match self.ask.backend {
            Backend::OpenCode => ("🆓 OpenCode · free model", self.ask.oc.model.trim_start_matches("opencode/").to_string()),
            Backend::ClaudeApi if self.ask.via_claude_code() => {
                let cred = crate::ask_claude_code::Credential::from_key(self.ask.claude_key().as_deref());
                ("✳ Claude · via Claude Code", format!("{} · {}", MODELS[self.ask.model].1, cred.describe()))
            }
            Backend::ClaudeApi => ("🔑 Claude API · your key", MODELS[self.ask.model].1.to_string()),
        };
        ui.label(semibold(who, 14.0));
        ui.label(RichText::new(detail).size(12.5).color(p.weak));
        if self.ask.backend == Backend::OpenCode && self.ask.oc.exe.is_none() {
            ui.label(RichText::new("OpenCode isn't installed yet: the download is in the chat area.").size(12.5).color(p.warn));
        }
        if ui.add(egui::Button::new("⚙ Change model, key or permissions").min_size(vec2(0.0, 28.0))).on_hover_text("Settings → AI assistant").clicked() {
            self.open_settings(crate::settings_page::SettingsPage::Ai);
        }
        filter_heading(ui, "What it can do");
        for line in [
            "📋 Draft plans, decks and skills: undo each from the chat",
            "🏁 Run the race simulator: at most 4 runs a question unless you ask",
            "💾 Save plans and log results: asks first",
            "✏ Change your roster: always asks",
        ] {
            ui.label(RichText::new(line).size(12.5).color(p.weak));
        }
        let s = &mut self.ask;
        filter_heading(ui, "This conversation");
        ui.label(RichText::new(format!("{} tokens in · {} out", s.tokens_in, s.tokens_out)).size(12.5).color(p.weak));
        if s.cache_read > 0 {
            ui.label(RichText::new(format!("{} read from the prompt cache (cheaper)", s.cache_read)).size(11.5).color(p.weak));
        }
        if s.backend == Backend::OpenCode {
            ui.label(RichText::new("Free models cost nothing; the count is for interest.").size(11.5).color(p.weak));
        } else if s.via_claude_code() && !matches!(crate::ask_claude_code::Credential::from_key(s.claude_key().as_deref()), crate::ask_claude_code::Credential::ApiKey(_)) {
            ui.label(RichText::new("On a Claude Pro or Max login or setup token, this counts against your plan's usage; a Console login is billed per use.").size(11.5).color(p.weak));
        }
        if ui.add_enabled(!busy, egui::Button::new("🗑 New conversation")).clicked() {
            s.clear();
            self.agent.undo.clear();
            if let Some(dir) = crate::agent::bridge::dir() {
                crate::agent::bridge::clear(&dir);
            }
        }
        if !self.ask.activity.is_empty() {
            ui.add_space(10.0);
            egui::CollapsingHeader::new(format!("Lookups ({})", self.ask.activity.len())).show(ui, |ui| {
                for a in &self.ask.activity {
                    ui.label(RichText::new(truncate(a, 120)).small().monospace());
                }
            });
        }
    }

    pub(crate) fn opencode_settings(ui: &mut egui::Ui, s: &mut AskState) {
        let p = pal(ui);
        let Some(exe) = s.oc.exe.clone() else { return };
        if s.oc.models.is_empty() && s.oc.models_rx.is_none() {
            s.oc.models_rx = Some(crate::ask_opencode::list_models(exe.clone()));
        }
        filter_heading(ui, "Model");
        if s.oc.models_rx.is_some() && s.oc.models.is_empty() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Loading models…").color(p.weak));
            });
        }
        let before = s.oc.model.clone();
        let models = &s.oc.models;
        let items = || {
            models
                .iter()
                .map(|m| {
                    let text = format!("{}{}", if crate::ask_opencode::is_free(m) { "🆓 " } else { "" }, m.trim_start_matches("opencode/"));
                    (Some(m.clone()), text, crate::ask_opencode::model_note(m).to_string())
                })
                .collect()
        };
        let current = s.oc.model.clone();
        if let Some(m) = crate::ui_util::search_combo(ui, "oc_model", current.trim_start_matches("opencode/"), 250.0, items, Some(&current)) {
            s.oc.model = m;
        }
        if s.oc.model != before {
            crate::settings::set("opencode_model", json!(s.oc.model));
        }
        let note = crate::ask_opencode::model_note(&s.oc.model);
        let safe = note.starts_with("keeps nothing");
        badge(ui, format!("{} {note}", if safe { "🔒" } else { "ℹ" }), if safe { p.accent } else { pal(ui).warn });
        ui.add_space(4.0);
        if ui.small_btn("🔄 Refresh models").clicked() && s.oc.models_rx.is_none() {
            s.oc.models.clear();
            s.oc.models_rx = Some(crate::ask_opencode::list_models(exe));
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "OpenCode (opencode.ai) reads your files (the roster digest, uma.guide data, plans and results) and uses umaLord's own tools to draft plans and run the simulator. \
                 It can't edit files or run commands; plans and results it saves go through the approval cards. Free models are offered for a limited time and are weaker than Claude.",
            )
            .size(11.5)
            .color(p.weak),
        );
    }

    pub(crate) fn claude_api_settings(ui: &mut egui::Ui, s: &mut AskState) {
        let p = pal(ui);
        filter_heading(ui, "Model");
        for (i, (_, label, _, _)) in MODELS.iter().enumerate() {
            ui.radio_value(&mut s.model, i, *label);
        }
        filter_heading(ui, "Effort");
        ui.horizontal(|ui| {
            for (i, e) in EFFORTS.iter().enumerate() {
                ui.selectable_value(&mut s.effort, i, *e);
            }
        });
        ui.label(RichText::new("Higher effort thinks longer and costs more.").size(12.0).color(p.weak));

        filter_heading(ui, "Sign in");
        App::claude_code_status(ui, s);
        let typed = s.key_input.trim().to_string();
        let has_key = s.auth_label() != "no key set yet";
        if s.oc.claude_exe.is_some() {
            let cred = crate::ask_claude_code::Credential::from_key(s.claude_key().as_deref());
            badge(ui, format!("✔ Using {}", cred.describe()), p.accent);
            ui.add_space(4.0);
            ui.label(
                RichText::new("Leave the box empty to use the account Claude Code is signed in with. Or paste a setup token to use your Claude account (Pro or Max) without signing Claude Code in, or an API key (sk-ant-api…) to pay per use.")
                    .size(12.0)
                    .color(p.weak),
            );
        } else {
            badge(ui, format!("{} {}", if has_key { "✔" } else { "✖" }, s.auth_label()), if has_key { p.accent } else { pal(ui).warn });
            ui.add_space(4.0);
        }
        ui.add(egui::TextEdit::singleline(&mut s.key_input).password(true).hint_text("sk-ant-api… or sk-ant-oat…").desired_width(f32::INFINITY));
        let env_key = std::env::var("ANTHROPIC_API_KEY").is_ok_and(|k| !k.trim().is_empty());
        let shown = if typed.is_empty() { s.saved_key.clone().unwrap_or_default() } else { typed.clone() };
        if env_key && !typed.is_empty() {
            ui.label(RichText::new("ANTHROPIC_API_KEY is set in the environment, so it's used instead of what you paste here.").size(12.0).color(pal(ui).warn));
        } else if crate::ask_claude_code::is_setup_token(&shown) {
            let (text, color) = if s.oc.claude_exe.is_some() {
                ("That's a setup token: umaLord hands it to Claude Code, which runs on your Claude account.", p.weak)
            } else {
                (SETUP_TOKEN_NEEDS_CLAUDE_CODE, pal(ui).warn)
            };
            ui.label(RichText::new(text).size(12.0).color(color));
        }
        ui.tick(&mut s.remember, "Remember on this PC");
        if s.remember && !typed.is_empty() && ui.button("💾 Save").clicked() {
            let ok = crate::settings::set("api_key", json!(typed));
            if ok {
                s.saved_key = Some(typed.clone());
            }
            s.status = Some((if ok { "Saved.".into() } else { "Couldn't save it.".into() }, !ok));
        }
        if s.saved_key.is_some() && ui.small_btn("Forget the saved key or token").clicked() {
            crate::settings::set("api_key", Value::Null);
            s.saved_key = None;
        }
        egui::CollapsingHeader::new("How to get a setup token or an API key").default_open(false).show(ui, |ui| {
            for line in [
                "Setup token (your Claude account): install Claude Code, run `claude setup-token` in a terminal, sign in in the browser it opens, and paste the sk-ant-oat01-… it prints. It lasts a year.",
                "Claude Code login: run `claude` in a terminal and type /login once; then leave the box empty.",
                "API key (billed per use): create one at console.anthropic.com → API Keys.",
            ] {
                ui.label(RichText::new(line).size(12.0).color(p.weak));
            }
        });
        ui.label(RichText::new("Saved keys and tokens are stored in plain text in your umaLord settings folder.").size(11.5).color(p.weak));
    }

    /// Whether Claude Code is installed, which lets Claude run on a Claude account.
    fn claude_code_status(ui: &mut egui::Ui, s: &mut AskState) {
        let p = pal(ui);
        let busy = s.oc.rx.is_some() || s.stream.is_some();
        ui.horizontal_wrapped(|ui| {
            match &s.oc.claude_exe {
                Some(exe) => _ = ui.label(RichText::new("✳ Claude Code found: questions run through it.").size(12.5)).on_hover_text(exe.display().to_string()),
                None => _ = ui.label(RichText::new("Claude Code isn't installed, so only an API key works. Install it from claude.com/claude-code to use your Claude account.").size(12.5).color(p.warn)),
            }
            let look = ui.add_enabled(!busy, egui::Button::new("🔄 Look again").small()).on_hover_text("Check PATH for claude again, after installing it").on_disabled_hover_text("Wait for the answer to finish");
            if look.clicked() {
                s.oc.claude_exe = crate::ask_claude_code::find_exe();
                s.end_process();
            }
        });
        ui.add_space(4.0);
    }

    /// Settings → AI assistant: which writes may happen without the approval card.
    pub(crate) fn agent_permissions(ui: &mut egui::Ui, s: &mut AskState) {
        let p = pal(ui);
        filter_heading(ui, "What the assistant may do without asking");
        ui.label(RichText::new("Drafts (the working plan, the Race sim's builds) always change at once and can be undone from the chat. Writing files asks first, unless you allow it here.").size(12.0).color(p.weak));
        if ui.tick(&mut s.auto_save, "Save plans to the plans folder").changed() {
            crate::settings::set("agent_auto_save_plan", json!(s.auto_save));
        }
        if ui.tick(&mut s.auto_log, "Add entries to the results logs").changed() {
            crate::settings::set("agent_auto_log_result", json!(s.auto_log));
        }
        ui.add_enabled(false, egui::Checkbox::new(&mut false, "Change your roster (limit breaks, stars, potential)"));
        ui.label(RichText::new("Roster changes always ask.").size(11.5).color(p.weak));
    }
}

/// The width left to the right edge of what's visible, at most `max`: a vertical scroll area lets
/// one wide line widen everything, and the rest would then run under the side panel.
fn visible(ui: &egui::Ui, max: f32) -> f32 {
    (ui.clip_rect().right() - ui.cursor().min.x - 14.0).clamp(160.0, max)
}

/// A click in the chat, carried out after drawing.
enum ChatAction {
    Ask(String),
    Open(OpenAt),
    Undo(usize),
    Answer(usize, bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A streamed answer with a progress note, text and a tool call builds the same message the
    /// non-streaming API returns; bad tool input is caught.
    #[test]
    fn sse_events_build_the_message() {
        let events = [
            json!({"type": "message_start", "message": {"model": "claude-opus-5-5", "usage": {"input_tokens": 10, "cache_read_input_tokens": 500}}}),
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": "", "signature": ""}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "Checking the schedule."}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "sig"}}),
            json!({"type": "content_block_stop", "index": 0}),
            json!({"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}),
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "Let me "}}),
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "look."}}),
            json!({"type": "content_block_stop", "index": 1}),
            json!({"type": "content_block_start", "index": 2, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "find_umas", "input": {}}}),
            json!({"type": "content_block_delta", "index": 2, "delta": {"type": "input_json_delta", "partial_json": "{\"distance\": "}}),
            json!({"type": "content_block_delta", "index": 2, "delta": {"type": "input_json_delta", "partial_json": "\"Long\"}"}}),
            json!({"type": "content_block_stop", "index": 2}),
            json!({"type": "content_block_start", "index": 3, "content_block": {"type": "tool_use", "id": "toolu_2", "name": "get_uma", "input": {}}}),
            json!({"type": "content_block_delta", "index": 3, "delta": {"type": "input_json_delta", "partial_json": "{\"query\": \"Oguri"}}),
            json!({"type": "content_block_stop", "index": 3}),
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 42}}),
            json!({"type": "message_stop"}),
        ];
        let mut acc = Acc::default();
        let mut text = String::new();
        let mut notes = String::new();
        for e in &events {
            for ev in acc.apply(e) {
                match ev {
                    StreamEvent::Text(_, t) => text += &t,
                    StreamEvent::Note(_, t) => notes += &t,
                    _ => {}
                }
            }
        }
        assert_eq!((text.as_str(), notes.as_str()), ("Let me look.", "Checking the schedule."));
        let m = acc.finish().unwrap();
        let c = m["content"].as_array().unwrap();
        assert_eq!(c.len(), 4);
        assert_eq!(c[0], json!({"type": "thinking", "thinking": "Checking the schedule.", "signature": "sig"}));
        assert_eq!(c[1]["text"], "Let me look.");
        assert_eq!(c[2]["input"], json!({"distance": "Long"}));
        assert_eq!(c[3]["input"], json!({}), "an unparseable input is echoed as an empty object");
        assert_eq!(m["invalid_inputs"]["toolu_2"], "{\"query\": \"Oguri");
        assert_eq!((m["stop_reason"].as_str(), m["usage"]["output_tokens"].as_u64(), m["usage"]["cache_read_input_tokens"].as_u64()), (Some("tool_use"), Some(42), Some(500)));
        let mut err = Acc::default();
        err.apply(&json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}));
        assert!(err.finish().unwrap_err().contains("Overloaded"));
    }

    /// Every kind of chat line draws, in both modes and at both window sizes.
    #[test]
    fn chat_draws_every_card() {
        let ctx = egui::Context::default();
        let mut a = App::for_tests();
        a.tab = crate::Tab::Ask;
        a.agent_snapshot("test");
        let card = |visual: Visual| Card { summary: "A result line".into(), visual, open: vec![OpenAt::Plan, OpenAt::Race], undo: Some(0) };
        let tool = |label: &str, state: ToolState| Turn::Tool(ToolTurn { icon: "🏁", label: label.into(), state });
        a.ask.turns = vec![
            Turn::User("Plan the next CM and simulate it.".into()),
            Turn::Note("Checking the schedule, then the meta sheet.".into()),
            tool("Recommended a deck", ToolState::Done(card(Visual::Deck { cards: vec![30028, 30052], friend: Some(30076) }))),
            tool("Raced two builds", ToolState::Done(card(Visual::Verdict { line: "Uma 2 wins 99% of 1000 races, by 3.29 L on average".into(), gaps: (0..200).map(|i| f64::from(i) / 40.0 - 1.0).collect() }))),
            tool("Auto-picked skills", ToolState::Done(card(Visual::Skills(vec![200512, 200532])))),
            tool("Tested what matters", ToolState::Done(card(Visual::Bars(vec![("+50 Power".into(), 0.22), ("+50 Wit".into(), -0.05)])))),
            tool("Loaded the plan", ToolState::Failed(Card { summary: "Enter target stats first.".into(), ..Default::default() })),
            tool("Simulated the skills on offer", ToolState::Running),
            Turn::Assistant("**Done.** A table:\n\n| a | b |\n|---|---|\n| 1 | 2 |".into()),
            tool("Saved the plan", ToolState::Asking("Save plan 'X' to a new file in plans/?".into(), None)),
        ];
        for mode in [crate::theme::Mode::Dark, crate::theme::Mode::Light] {
            crate::theme::install(&ctx, mode);
            for size in [egui::vec2(1400.0, 900.0), egui::vec2(960.0, 600.0)] {
                let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() };
                let _ = crate::ui_util::test_frame(&ctx, input, |ui| a.frame(ui));
            }
        }
        assert!(a.approval_showing());
    }

    /// A model turn with a Draft, a Long and a Write call: the Draft runs at once, the turn holds
    /// while the simulation runs, the Write waits for Approve, and then all three results go back
    /// together in order. Deny returns "The user declined".
    #[test]
    fn tool_rounds_hold_for_jobs_and_approvals() {
        let ctx = egui::Context::default();
        let (mut a, dir) = crate::onboarding::tests::scratch_app("agent-round");
        a.roster = crate::data::Roster::sample();
        a.ask.backend = Backend::ClaudeApi;
        let msg = json!({
            "content": [
                {"type": "tool_use", "id": "t1", "name": "new_plan", "input": {"goal": "Champions Meeting", "cm": "next", "trainee": "Oguri Cap [Starlight Beat]"}},
                {"type": "tool_use", "id": "t2", "name": "run_stamina", "input": {"runs": 100, "race": {"course": "Nakayama Turf 2500m"}}},
                {"type": "tool_use", "id": "t3", "name": "save_plan", "input": {"title": "Agent test"}},
            ],
            "stop_reason": "tool_use", "usage": {}, "model": "claude-opus-5-5", "invalid_inputs": {},
        });
        a.handle_message(&ctx, msg);
        assert_eq!(a.ask.calls.len(), 3);
        assert!(matches!(a.ask.calls[0].state, CallState::Ready(_)), "the Draft ran at once");
        assert!(matches!(a.ask.calls[1].state, CallState::Waiting(_)), "the Long tool waits for its job");
        let start = Instant::now();
        while matches!(a.ask.calls.get(1).map(|c| &c.state), Some(CallState::Waiting(_))) {
            assert!(start.elapsed().as_secs() < 60, "the stamina run never finished");
            a.agent_poll_jobs(&ctx);
            a.advance_calls(&ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(matches!(a.ask.calls[2].state, CallState::Asking), "the Write tool waits for the user");
        for _ in 0..5 {
            a.advance_calls(&ctx);
        }
        assert!(matches!(a.ask.calls[2].state, CallState::Asking), "still waiting: nothing is written without Approve");
        assert!(a.plan_times().is_empty(), "no plan file yet");
        let turn = a.ask.calls[2].turn;
        // Deny: the call gets an error result and nothing is written.
        a.chat_action(&ctx, ChatAction::Answer(turn, false));
        assert!(a.ask.calls.is_empty(), "all three results went back");
        let results = a.ask.messages.last().unwrap()["content"].as_array().unwrap().clone();
        let ids: Vec<&str> = results.iter().map(|r| r["tool_use_id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["t1", "t2", "t3"]);
        assert_eq!(results[2]["is_error"], true);
        assert_eq!(results[2]["content"], "The user declined.");
        assert!(results[1]["content"].as_str().unwrap().contains("full spurt"), "{}", results[1]["content"]);
        assert!(a.plan_times().is_empty());
        // Approve a save this time.
        let msg = json!({"content": [{"type": "tool_use", "id": "t4", "name": "save_plan", "input": {"title": "Agent test"}}], "stop_reason": "tool_use", "usage": {}, "model": "m", "invalid_inputs": {}});
        a.handle_message(&ctx, msg);
        let turn = a.ask.calls[0].turn;
        a.chat_action(&ctx, ChatAction::Answer(turn, true));
        let saved = a.plan_times();
        assert_eq!(saved.len(), 1, "approved: the plan is saved");
        let text = std::fs::read_to_string(saved.keys().next().unwrap()).unwrap();
        assert!(text.starts_with("# Agent test"), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
