//! The Ask tab's Claude backend when Claude Code (claude.com/claude-code) is installed: questions
//! run through the `claude` CLI, on the user's Claude Code login, a setup token or an API key.
//!
//! Anthropic serves Claude subscription tokens (`sk-ant-oat…`, from `claude setup-token`) only
//! to Claude's own apps: the Messages API answers them with a bare 429. Claude Code takes them
//! (`CLAUDE_CODE_OAUTH_TOKEN`), so the app hands its credential to the CLI instead of calling the
//! API. The design follows OpenClaw's `claude-cli` backend (github.com/openclaw/openclaw,
//! extensions/anthropic, read 2026-10-27):
//!
//! - **Managed isolation:** no user or project settings, hooks, plugins, auto-memory, CLAUDE.md,
//!   slash commands or Chrome (`--setting-sources ""` plus `RESTRICTED_SETTINGS`), only Read,
//!   Glob and Grep among Claude Code's own tools, and only umaLord's MCP server
//!   (`--strict-mcp-config`). Inherited `ANTHROPIC_*` and `CLAUDE_CODE_*` overrides are cleared
//!   (`CLEAR_ENV`) so nothing steers the CLI to another endpoint or credential.
//! - **One warm process per conversation:** `--input-format stream-json` takes each question on
//!   stdin, so follow-ups skip the startup. AGENTS.md goes in once, as the `initialize` control
//!   request's `appendSystemPrompt`. A fresh conversation gets its own `--session-id`; after the
//!   process stops (Stop, a changed model), the next question resumes it with `--resume`.
//! - **Permissions over stdio:** `--permission-prompt-tool stdio` turns each permission question
//!   into a `can_use_tool` control request. Reading inside the umaLord folder needs none; reads
//!   elsewhere and every other tool are denied (`decide`). umaLord's own tools are pre-approved and
//!   ask the user through the window's approval cards (agent/bridge.rs).
//! - **Streaming:** `--include-partial-messages` streams the answer as it's written.
//!
//! Events come out as `ask_opencode::OcEvent`, so the chat treats this and OpenCode alike.

use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Value, json};

use crate::ask_opencode::{OcEvent, ToolEvent};

/// Settings for every run, as OpenClaw's restricted runs pass them: no hooks, plugins,
/// auto-memory or instruction files.
const RESTRICTED_SETTINGS: &str = r#"{"disableAllHooks":true,"enabledPlugins":{},"autoMemoryEnabled":false,"claudeMdExcludes":["**/CLAUDE.md","**/CLAUDE.local.md","**/.claude/rules/**"]}"#;

/// Claude Code's own tools the assistant gets: reading and searching only.
const NATIVE_TOOLS: [&str; 3] = ["Read", "Glob", "Grep"];

/// Environment Claude Code reads before its own login, which could send it to another provider,
/// endpoint or credential (OpenClaw's `CLAUDE_CLI_CLEAR_ENV`), plus a parent Claude Code's markers.
const CLEAR_ENV: [&str; 46] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_API_KEY_OLD",
    "ANTHROPIC_API_TOKEN",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_CUSTOM_HEADERS",
    "ANTHROPIC_OAUTH_TOKEN",
    "ANTHROPIC_UNIX_SOCKET",
    "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
    "CLAUDE_CODE_DISABLE_1M_CONTEXT",
    "CLAUDE_CODE_DISABLE_ADAPTIVE_THINKING",
    "MAX_THINKING_TOKENS",
    "CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_OAUTH_REFRESH_TOKEN",
    "CLAUDE_CODE_OAUTH_SCOPES",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
    "CLAUDE_CODE_PLUGIN_CACHE_DIR",
    "CLAUDE_CODE_PLUGIN_SEED_DIR",
    "CLAUDE_CODE_REMOTE",
    "CLAUDE_CODE_USE_COWORK_PLUGINS",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_USE_VERTEX",
    "OTEL_EXPORTER_OTLP_ENDPOINT",
    "OTEL_EXPORTER_OTLP_HEADERS",
    "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
    "OTEL_EXPORTER_OTLP_LOGS_HEADERS",
    "OTEL_EXPORTER_OTLP_LOGS_PROTOCOL",
    "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
    "OTEL_EXPORTER_OTLP_METRICS_HEADERS",
    "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL",
    "OTEL_EXPORTER_OTLP_PROTOCOL",
    "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
    "OTEL_EXPORTER_OTLP_TRACES_HEADERS",
    "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL",
    "OTEL_LOGS_EXPORTER",
    "OTEL_METRICS_EXPORTER",
    "OTEL_SDK_DISABLED",
    "OTEL_TRACES_EXPORTER",
    "CLAUDECODE",
    "NODE_OPTIONS",
    "DEBUG",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_CODE_SIMPLE",
];

/// Claude Code 2.1.98 added `excludeDynamicSections`, which keeps the cached prompt prefix the
/// same across turns.
const DYNAMIC_SECTIONS_MINIMUM: [u32; 3] = [2, 1, 98];

/// What the CLI signs in with, chosen by what the user gave: nothing (Claude Code's own login),
/// a setup token from `claude setup-token`, or an API key.
#[derive(Clone, PartialEq)]
pub enum Credential {
    Login,
    SetupToken(String),
    ApiKey(String),
}

/// Never prints the key or token.
impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Login => "Login",
            Self::SetupToken(_) => "SetupToken(…)",
            Self::ApiKey(_) => "ApiKey(…)",
        })
    }
}

impl Credential {
    pub fn from_key(key: Option<&str>) -> Self {
        match key.map(str::trim).filter(|k| !k.is_empty()) {
            None => Self::Login,
            Some(k) if is_setup_token(k) => Self::SetupToken(k.to_string()),
            Some(k) => Self::ApiKey(k.to_string()),
        }
    }

    /// How the chat's side panel names it.
    pub fn describe(&self) -> &'static str {
        match self {
            Self::Login => "your Claude Code login",
            Self::SetupToken(_) => "your setup token (Claude account)",
            Self::ApiKey(_) => "your API key",
        }
    }

    fn secret(&self) -> Option<&str> {
        match self {
            Self::Login => None,
            Self::SetupToken(s) | Self::ApiKey(s) => Some(s),
        }
    }
}

/// A Claude subscription token (`claude setup-token` prints `sk-ant-oat01-…`): only Claude's own
/// apps may use it, so it has to go through Claude Code.
pub fn is_setup_token(key: &str) -> bool {
    key.trim().starts_with("sk-ant-oat")
}

/// `claude` on PATH, or where its installers put it.
pub fn find_exe() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    let path = std::env::var_os("PATH").unwrap_or_default();
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    // Apps opened from Finder don't get the shell's PATH, so look where Homebrew and npm put it.
    let mac: &[&str] = if cfg!(target_os = "macos") { &["/opt/homebrew/bin", "/usr/local/bin"] } else { &[] };
    let extra = home.iter().flat_map(|h| [h.join(".local").join("bin"), h.join(".claude").join("local"), h.join(".npm-global").join("bin")]).chain(mac.iter().map(PathBuf::from));
    std::env::split_paths(&path).chain(extra).flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file())
}

/// Whether this Claude Code takes `excludeDynamicSections`, from `claude --version` (asked once).
fn supports_dynamic_sections(exe: &Path) -> bool {
    static ANSWER: OnceLock<bool> = OnceLock::new();
    *ANSWER.get_or_init(|| {
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--version").stdin(Stdio::null()).stderr(Stdio::null());
        crate::ask::hidden(&mut cmd);
        cmd.output().ok().and_then(|o| parse_version(&String::from_utf8_lossy(&o.stdout))).is_some_and(|v| v >= DYNAMIC_SECTIONS_MINIMUM)
    })
}

/// "2.1.304 (Claude Code)" → [2, 1, 304]. Prereleases don't count.
fn parse_version(text: &str) -> Option<[u32; 3]> {
    let word = text.split_whitespace().find(|w| w.split('.').count() == 3)?;
    let mut parts = word.split('.').map(|p| p.parse::<u32>().ok());
    Some([parts.next()??, parts.next()??, parts.next()??])
}

/// A random version-4 UUID for a new session, without a uuid crate.
fn new_uuid() -> String {
    let mut bytes = [0u8; 16];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        h.write_usize(i);
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// Claude Code's `--mcp-config` for umaLord's tools: this exe as a stdio MCP server on the folder,
/// with the bridge folder the window watches for approvals.
pub fn mcp_config(exe: &Path, root: &Path, bridge: Option<&Path>) -> String {
    let mut env = serde_json::Map::new();
    if let Some(b) = bridge {
        env.insert(crate::agent::bridge::ENV.into(), Value::String(b.display().to_string()));
    }
    json!({
        "mcpServers": {"umalord": {
            "type": "stdio",
            "command": exe.display().to_string(),
            "args": ["--mcp", root.display().to_string()],
            "env": env,
        }}
    })
    .to_string()
}

/// One question for Claude Code.
pub struct Request {
    pub exe: PathBuf,
    pub root: PathBuf,
    /// A model id from `ask::MODELS` and an effort from `ask::EFFORTS`.
    pub model: String,
    pub effort: String,
    pub credential: Credential,
    /// The conversation's Claude session, once it has one.
    pub session: Option<String>,
    pub question: String,
    /// `mcp_config`'s JSON.
    pub mcp: Option<String>,
}

impl Request {
    /// What the warm process was started with; a question with anything else needs a new one.
    fn fingerprint(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let credential = format!("{:?}{}", self.credential, self.credential.secret().unwrap_or_default());
        for part in [self.exe.display().to_string(), self.root.display().to_string(), self.model.clone(), self.effort.clone(), credential, self.mcp.clone().unwrap_or_default()] {
            h.write(part.as_bytes());
            h.write_u8(0);
        }
        h.finish()
    }

    fn args(&self, session: &Session) -> Vec<String> {
        let mut args: Vec<String> = [
            "--print",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--permission-prompt-tool",
            "stdio",
            "--permission-mode",
            "default",
            "--setting-sources",
            "",
            "--settings",
            RESTRICTED_SETTINGS,
            "--disable-slash-commands",
            "--no-chrome",
            "--strict-mcp-config",
            "--disallowedTools",
            "ScheduleWakeup,CronCreate,Monitor",
        ]
        .map(String::from)
        .to_vec();
        args.extend(["--tools".into(), NATIVE_TOOLS.join(",")]);
        args.extend(["--model".into(), self.model.clone(), "--effort".into(), self.effort.clone()]);
        if let Some(m) = &self.mcp {
            args.extend(["--mcp-config".into(), m.clone(), "--allowedTools".into(), "mcp__umalord".into()]);
        }
        match session {
            Session::New(id) => args.extend(["--session-id".into(), id.clone()]),
            Session::Resume(id) => args.extend(["--resume".into(), id.clone()]),
        }
        args
    }
}

enum Session {
    New(String),
    Resume(String),
}

/// The conversation's Claude Code process between questions: its stdin, the lines it prints,
/// the last of what it wrote to stderr, and what it was started with. The process itself sits in
/// the Ask tab's `child`, so Stop, New conversation and closing the window kill it.
struct Warm {
    stdin: ChildStdin,
    lines: Receiver<Value>,
    stderr: Arc<Mutex<String>>,
    fingerprint: u64,
    session: String,
}

static WARM: Mutex<Option<Warm>> = Mutex::new(None);

/// Run one question. Events arrive on the returned channel, ending with `Done`. `child` holds
/// the warm process.
pub fn ask(req: Request, child: Arc<Mutex<Option<Child>>>, wake: impl Fn() + Send + 'static) -> Receiver<OcEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let emit = |e: OcEvent| {
            let e = match e {
                OcEvent::Error(m) => OcEvent::Error(redact(&m, &req.credential)),
                e => e,
            };
            let _ = tx.send(e);
            wake();
        };
        let result = turn(&req, &child, &emit);
        emit(OcEvent::Done(result.map_err(|e| redact(&e, &req.credential))));
    });
    rx
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Stop a process and everything it started, and reap it. On Windows an npm install runs
/// `claude.cmd`, whose `node` child would outlive a plain kill of `cmd.exe` and hold its pipes.
pub fn kill(mut proc: Child) {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/T", "/F", "/PID", &proc.id().to_string()]).stdout(Stdio::null()).stderr(Stdio::null());
        crate::ask::hidden(&mut cmd);
        let _ = cmd.status();
    }
    let _ = proc.kill();
    let _ = proc.wait();
}

/// How long to wait for a record before checking whether the user stopped the turn.
const POLL: std::time::Duration = std::time::Duration::from_millis(250);
/// How long Claude Code may take to answer `initialize`.
const STARTUP: std::time::Duration = std::time::Duration::from_secs(60);

fn turn(req: &Request, child: &Arc<Mutex<Option<Child>>>, emit: &dyn Fn(OcEvent)) -> Result<(), String> {
    let mut warm = lock(&WARM);
    let fingerprint = req.fingerprint();
    let alive = lock(child).as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
    let reusable = alive && warm.as_ref().is_some_and(|w| w.fingerprint == fingerprint && req.session.as_deref() == Some(w.session.as_str()));
    if !reusable {
        if let Some(old) = lock(child).take() {
            kill(old);
        }
        *warm = None;
        *warm = Some(match req.session.clone() {
            None => start(req, Session::New(new_uuid()), child)?,
            // A session that was never saved (stopped before its first answer) can't be resumed;
            // start a fresh one rather than failing every later question.
            Some(id) => match start(req, Session::Resume(id), child) {
                Err(e) if e != "Stopped." => start(req, Session::New(new_uuid()), child)?,
                r => r?,
            },
        });
    }
    let w = warm.as_mut().expect("started above");
    emit(OcEvent::Session(w.session.clone()));
    let message = json!({"type": "user", "message": {"role": "user", "content": req.question}, "parent_tool_use_id": null, "uuid": new_uuid(), "session_id": w.session});
    if send(&mut w.stdin, &message).is_err() {
        return Err(lost(&mut warm, child));
    }
    let mut parser = Parser::default();
    loop {
        let v = match w.lines.recv_timeout(POLL) {
            Ok(v) => v,
            // Stop takes the process out of `child`; on Windows its pipes may stay open a moment.
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if lock(child).is_some() => continue,
            Err(_) => return Err(lost(&mut warm, child)),
        };
        // Claude Code's output owns the session id: a resume can fork it.
        if let Some(id) = v["session_id"].as_str().filter(|id| !id.is_empty() && *id != w.session) {
            w.session = id.to_string();
            emit(OcEvent::Session(id.to_string()));
        }
        match v["type"].as_str().unwrap_or_default() {
            "control_request" => {
                let reply = control_reply(&v, &req.root);
                if send(&mut w.stdin, &reply).is_err() {
                    return Err(lost(&mut warm, child));
                }
            }
            "control_response" | "keep_alive" => {}
            kind => {
                for e in parser.line_value(&v) {
                    emit(e);
                }
                if kind == "result" {
                    return Ok(());
                }
            }
        }
    }
}

/// Start the process and send `initialize`; the system prompt (AGENTS.md) goes in here.
fn start(req: &Request, session: Session, child: &Arc<Mutex<Option<Child>>>) -> Result<Warm, String> {
    let mut cmd = crate::ask_opencode::command(&req.exe);
    cmd.stdin(Stdio::piped()).current_dir(&req.root);
    for var in CLEAR_ENV {
        cmd.env_remove(var);
    }
    // The workspace's Git state would rewrite the prompt prefix on every resume.
    cmd.env("CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS", "1");
    match &req.credential {
        Credential::Login => {}
        Credential::SetupToken(t) => _ = cmd.env("CLAUDE_CODE_OAUTH_TOKEN", t),
        Credential::ApiKey(k) => _ = cmd.env("ANTHROPIC_API_KEY", k),
    }
    let id = match &session {
        Session::New(id) | Session::Resume(id) => id.clone(),
    };
    cmd.args(req.args(&session));
    // Hold the slot while spawning, so a Stop now waits and then finds the process to kill.
    let mut slot = lock(child);
    let mut proc = cmd.spawn().map_err(|e| format!("couldn't start Claude Code: {e}"))?;
    let (Some(stdin), Some(stdout), Some(stderr_pipe)) = (proc.stdin.take(), proc.stdout.take(), proc.stderr.take()) else {
        let _ = proc.kill();
        return Err("couldn't connect to Claude Code".into());
    };
    let (tx, lines) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            // Launchers can print plain diagnostics; only JSON records are protocol.
            if let Ok(v) = serde_json::from_str::<Value>(&line)
                && tx.send(v).is_err()
            {
                break;
            }
        }
    });
    let stderr = Arc::new(Mutex::new(String::new()));
    let tail = stderr.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut pipe = stderr_pipe;
        while let Ok(n) = pipe.read(&mut buf) {
            if n == 0 {
                break;
            }
            let mut t = lock(&tail);
            t.push_str(&String::from_utf8_lossy(&buf[..n]));
            if t.len() > 8192 {
                // Keep whole lines only: a clipped line's missing start could hide a secret from
                // `redact`.
                let from = t.len() - 4096;
                let cut = t[..].char_indices().map(|(i, _)| i).find(|&i| i >= from).unwrap_or(t.len());
                let cut = t[cut..].find('\n').map_or(t.len(), |nl| cut + nl + 1);
                t.drain(..cut);
            }
        }
    });
    *slot = Some(proc);
    drop(slot);
    let mut warm = Warm { stdin, lines, stderr, fingerprint: req.fingerprint(), session: id };
    let agents = std::fs::read_to_string(req.root.join("AGENTS.md")).unwrap_or_default();
    let init_id = new_uuid();
    let init = json!({"type": "control_request", "request_id": init_id, "request": {
        "subtype": "initialize",
        "appendSystemPrompt": agents,
        // The app's prompt, not a snapshot saved with the session, applies after a resume.
        "systemPromptSnapshot": false,
        "excludeDynamicSections": supports_dynamic_sections(&req.exe),
    }});
    if send(&mut warm.stdin, &init).is_err() {
        return Err(exit_error(&warm, child));
    }
    let deadline = std::time::Instant::now() + STARTUP;
    loop {
        let v = match warm.lines.recv_timeout(POLL) {
            Ok(v) => v,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if lock(child).is_none() => return Err("Stopped.".into()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if std::time::Instant::now() < deadline => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if let Some(p) = lock(child).take() {
                    kill(p);
                }
                return Err("Claude Code didn't start within a minute.".into());
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Err(exit_error(&warm, child)),
        };
        if v["type"] == "control_response" && v["response"]["request_id"] == init_id.as_str() {
            if v["response"]["subtype"] == "success" {
                return Ok(warm);
            }
            if let Some(p) = lock(child).take() {
                kill(p);
            }
            return Err(format!("Claude Code didn't start: {}", v["response"]["error"].as_str().unwrap_or("initialize failed")));
        }
    }
}

fn send(stdin: &mut ChildStdin, message: &Value) -> std::io::Result<()> {
    writeln!(stdin, "{message}")?;
    stdin.flush()
}

/// The process went away mid-turn: stopped by the user, or it exited.
fn lost(warm: &mut Option<Warm>, child: &Arc<Mutex<Option<Child>>>) -> String {
    let msg = match warm.as_ref() {
        Some(w) => exit_error(w, child),
        None => "Stopped.".into(),
    };
    *warm = None;
    msg
}

fn exit_error(w: &Warm, child: &Arc<Mutex<Option<Child>>>) -> String {
    let Some(mut proc) = lock(child).take() else { return "Stopped.".into() };
    // Its output ended; if it hasn't exited yet, it won't answer any more either.
    let status = match proc.try_wait() {
        Ok(Some(status)) => Some(status),
        _ => {
            let _ = proc.kill();
            proc.wait().ok()
        }
    };
    let tail = crate::ask_opencode::strip_ansi(&lock(&w.stderr));
    let last = tail.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or_default().trim().to_string();
    match (status.and_then(|s| s.code()), last.is_empty()) {
        (_, false) => with_hint(&format!("Claude Code stopped: {last}")),
        (Some(code), true) => format!("Claude Code exited with code {code}."),
        (None, true) => "Claude Code stopped.".into(),
    }
}

/// Never show the user's key or token in an error.
fn redact(text: &str, credential: &Credential) -> String {
    match credential.secret() {
        Some(s) if s.len() > 8 => text.replace(s, "[REDACTED]"),
        _ => text.to_string(),
    }
}

/// The answer to a control request from Claude Code.
fn control_reply(v: &Value, root: &Path) -> Value {
    let request = &v["request"];
    let response = match request["subtype"].as_str().unwrap_or_default() {
        "can_use_tool" => {
            let mut answer = decide(request["tool_name"].as_str().unwrap_or_default(), &request["input"], root);
            answer["toolUseID"] = request["tool_use_id"].clone();
            answer
        }
        // No form-filling for MCP servers; umaLord's ask through the window instead.
        "elicitation" => json!({"action": "decline"}),
        _ => json!({}),
    };
    json!({"type": "control_response", "response": {"subtype": "success", "request_id": v["request_id"], "response": response}})
}

/// Whether Claude Code may use a tool it asks about: umaLord's own tools, and reading or
/// searching inside the umaLord folder. Everything else is denied.
fn decide(tool: &str, input: &Value, root: &Path) -> Value {
    // Fold `..` by hand, then canonicalize the deepest part that exists and add the rest back:
    // a file that doesn't exist yet can't be canonicalized itself, and comparing it raw against a
    // canonical folder fails where the folder sits behind a symlink (macOS's /var →
    // /private/var) or gets Windows' \\?\ prefix.
    let resolve = |p: &Path| {
        let mut folded = PathBuf::new();
        for c in p.components() {
            match c {
                std::path::Component::ParentDir => _ = folded.pop(),
                std::path::Component::CurDir => {}
                c => folded.push(c),
            }
        }
        let mut rest = Vec::new();
        let mut base = folded.as_path();
        loop {
            if let Ok(real) = base.canonicalize() {
                return rest.iter().rev().fold(real, |acc: PathBuf, part| acc.join(part));
            }
            match (base.parent(), base.file_name()) {
                (Some(parent), Some(name)) => {
                    rest.push(name.to_owned());
                    base = parent;
                }
                _ => return folded,
            }
        }
    };
    let inside_path = |p: &str| {
        // Claude Code expands `~` itself, so a home path would look relative here.
        if p.starts_with('~') {
            return false;
        }
        let p = Path::new(p);
        resolve(&if p.is_absolute() { p.to_path_buf() } else { root.join(p) }).starts_with(resolve(root))
    };
    let inside = |key: &str| input[key].as_str().is_none_or(inside_path);
    // Glob's pattern can name another folder by itself (`/home/u/**`, `../**`).
    let pattern_ok = || {
        input["pattern"].as_str().is_none_or(|g| {
            let rooted = g.starts_with(['/', '\\', '~']) || g.contains("..") || Path::new(g).is_absolute();
            tool != "Glob" || !rooted || inside_path(g.split(['*', '?', '[', '{']).next().unwrap_or(g))
        })
    };
    let allowed = tool.starts_with("mcp__umalord__") || (NATIVE_TOOLS.contains(&tool) && inside("file_path") && inside("path") && pattern_ok());
    if allowed {
        json!({"behavior": "allow", "updatedInput": input})
    } else {
        json!({"behavior": "deny", "message": "umaLord only lets the assistant read files in its own folder and use umaLord's tools."})
    }
}

/// OpenCode's names for the same tools, which the chat's cards know: `mcp__umalord__run_compare`
/// → `umalord_run_compare`, `Read` → `read`.
fn tool_name(name: &str) -> String {
    match name.strip_prefix("mcp__umalord__") {
        Some(t) => format!("umalord_{t}"),
        None => name.to_lowercase(),
    }
}

/// Reads stream-json records. A tool's call and its result arrive in different records, so it
/// keeps each call until its result comes. Text streams as deltas (`--include-partial-messages`):
/// the first delta of a text block starts a paragraph (`Text`), the rest continue it
/// (`TextDelta`), and the finished message's copy of the text is skipped.
#[derive(Default)]
pub struct Parser {
    calls: HashMap<String, (String, String)>,
    block_started: bool,
    streamed: bool,
}

impl Parser {
    #[cfg(test)]
    pub fn line(&mut self, line: &str) -> Vec<OcEvent> {
        serde_json::from_str::<Value>(line).map(|v| self.line_value(&v)).unwrap_or_default()
    }

    pub fn line_value(&mut self, v: &Value) -> Vec<OcEvent> {
        let mut out = Vec::new();
        let content = v["message"]["content"].as_array().map(Vec::as_slice).unwrap_or_default();
        match (v["type"].as_str().unwrap_or_default(), v["subtype"].as_str().unwrap_or_default()) {
            ("system", "init") => {
                let umalord = v["mcp_servers"].as_array().and_then(|a| a.iter().find(|m| m["name"] == "umalord"));
                if let Some(status) = umalord.and_then(|m| m["status"].as_str()).filter(|s| *s != "connected" && *s != "pending") {
                    out.push(OcEvent::Error(format!("umaLord's tools didn't start in Claude Code ({status}), so it can only read files.")));
                }
            }
            ("stream_event", _) => {
                let e = &v["event"];
                match e["type"].as_str().unwrap_or_default() {
                    "message_start" => self.streamed = false,
                    "content_block_start" => self.block_started = e["content_block"]["type"] == "text",
                    "content_block_delta" if e["delta"]["type"] == "text_delta" => {
                        if let Some(t) = e["delta"]["text"].as_str().filter(|t| !t.is_empty()) {
                            self.streamed = true;
                            out.push(if std::mem::take(&mut self.block_started) { OcEvent::Text(t.to_string()) } else { OcEvent::TextDelta(t.to_string()) });
                        }
                    }
                    _ => {}
                }
            }
            ("assistant", _) => {
                for c in content {
                    match c["type"].as_str().unwrap_or_default() {
                        "text" if !self.streamed => {
                            if let Some(t) = c["text"].as_str().filter(|t| !t.trim().is_empty()) {
                                out.push(OcEvent::Text(t.to_string()));
                            }
                        }
                        "tool_use" => {
                            let input = &c["input"];
                            let target = ["file_path", "pattern", "path", "query", "name"].iter().find_map(|k| input[k].as_str()).unwrap_or_default();
                            let id = c["id"].as_str().unwrap_or_default().to_string();
                            self.calls.insert(id, (tool_name(c["name"].as_str().unwrap_or("tool")), target.to_string()));
                        }
                        _ => {}
                    }
                }
            }
            ("user", _) => {
                for c in content.iter().filter(|c| c["type"] == "tool_result") {
                    let (tool, target) = self.calls.remove(c["tool_use_id"].as_str().unwrap_or_default()).unwrap_or_else(|| ("tool".into(), String::new()));
                    let output = match &c["content"] {
                        Value::String(s) => s.clone(),
                        Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
                        _ => String::new(),
                    };
                    out.push(OcEvent::Tool(ToolEvent { tool, target, output, error: c["is_error"].as_bool().unwrap_or(false) }));
                }
            }
            ("result", _) => {
                let u = &v["usage"];
                let input = ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"].iter().filter_map(|k| u[k].as_u64()).sum();
                out.push(OcEvent::Tokens(input, u["output_tokens"].as_u64().unwrap_or(0)));
                if v["is_error"].as_bool().unwrap_or(false) {
                    let msg = v["result"].as_str().filter(|r| !r.trim().is_empty()).unwrap_or_else(|| v["subtype"].as_str().unwrap_or("Claude Code reported an error"));
                    out.push(OcEvent::Error(with_hint(msg)));
                }
            }
            _ => {}
        }
        out
    }
}

/// Claude Code's sign-in errors mention "/login"; say what to do from umaLord.
fn with_hint(msg: &str) -> String {
    let lower = msg.to_lowercase();
    if msg.contains("/login") || lower.contains("not logged in") || lower.contains("invalid api key") || lower.contains("oauth") {
        format!("{msg}\n\nSign in by running `claude` in a terminal and typing /login, or paste a setup token (`claude setup-token`) or an API key in ⚙ Settings → AI assistant.")
    } else {
        msg.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records from a real warm `claude --print --input-format stream-json` run (2.1.304,
    /// 2026-10-27), trimmed.
    #[test]
    fn parses_a_streamed_turn() {
        let lines = [
            r#"{"type":"system","subtype":"init","session_id":"s1","mcp_servers":[{"name":"umalord","status":"connected"}]}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use"}}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__umalord__list_upcoming_events","input":{"count":1}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"Upcoming Champions Meetings"}]}]}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"text"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"CM 21 "}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"is next."}}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"CM 21 is next."}]}}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"CM 21 is next.","usage":{"input_tokens":4,"output_tokens":158,"cache_read_input_tokens":100,"cache_creation_input_tokens":200}}"#,
        ];
        let mut p = Parser::default();
        let events: Vec<OcEvent> = lines.iter().flat_map(|l| p.line(l)).collect();
        assert_eq!(
            events,
            vec![
                OcEvent::Tool(ToolEvent { tool: "umalord_list_upcoming_events".into(), target: String::new(), output: "Upcoming Champions Meetings".into(), error: false }),
                OcEvent::Text("CM 21 ".into()),
                OcEvent::TextDelta("is next.".into()),
                OcEvent::Tokens(304, 158),
            ]
        );
    }

    #[test]
    fn whole_messages_without_partials() {
        let mut p = Parser::default();
        let e = p.line(r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":""},{"type":"text","text":"Hello."}]}}"#);
        assert_eq!(e, vec![OcEvent::Text("Hello.".into())]);
    }

    #[test]
    fn sign_in_errors_say_how_to_sign_in() {
        let mut p = Parser::default();
        let events = p.line(r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login","usage":{}}"#);
        assert!(matches!(&events[1], OcEvent::Error(e) if e.contains("claude setup-token")));
    }

    #[test]
    fn credentials_by_prefix() {
        assert_eq!(Credential::from_key(None), Credential::Login);
        assert_eq!(Credential::from_key(Some("  ")), Credential::Login);
        assert_eq!(Credential::from_key(Some("sk-ant-oat01-abc")), Credential::SetupToken("sk-ant-oat01-abc".into()));
        assert_eq!(Credential::from_key(Some(" sk-ant-api03-xyz ")), Credential::ApiKey("sk-ant-api03-xyz".into()));
    }

    #[test]
    fn reads_only_inside_the_folder() {
        let root = std::env::temp_dir();
        let allow = |tool: &str, input: Value| decide(tool, &input, &root)["behavior"] == "allow";
        assert!(allow("Read", json!({"file_path": root.join("x.md").display().to_string()})));
        assert!(allow("Grep", json!({"pattern": "Oguri"})));
        assert!(allow("mcp__umalord__run_compare", json!({})));
        assert!(!allow("Read", json!({"file_path": "/etc/hostname"})));
        assert!(!allow("Read", json!({"file_path": "../outside.md"})));
        assert!(!allow("Bash", json!({"command": "ls"})));
        assert!(!allow("Read", json!({"file_path": "~/.ssh/id_rsa"})));
        assert!(!allow("Glob", json!({"pattern": "/etc/**"})));
        assert!(!allow("Glob", json!({"pattern": "../**/*.md"})));
        assert!(allow("Glob", json!({"pattern": "knowledge/**/*.md"})));
        assert!(!allow("Write", json!({"file_path": root.join("x.md").display().to_string()})));
    }

    /// macOS's temp folder is /var → /private/var: a folder reached through a symlink must still
    /// allow files in it, including ones that don't exist yet.
    #[cfg(unix)]
    #[test]
    fn reads_inside_a_symlinked_folder() {
        let real = std::env::temp_dir().join(format!("umalord-real-{}", std::process::id()));
        let link = std::env::temp_dir().join(format!("umalord-link-{}", std::process::id()));
        std::fs::create_dir_all(real.join("knowledge")).unwrap();
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let allow = |input: Value| decide("Read", &input, &link)["behavior"] == "allow";
        assert!(allow(json!({"file_path": link.join("knowledge").join("new.md").display().to_string()})));
        assert!(allow(json!({"file_path": real.join("x.md").display().to_string()})));
        assert!(allow(json!({"file_path": "knowledge/new.md"})));
        assert!(!allow(json!({"file_path": link.join("..").join("elsewhere.md").display().to_string()})));
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir_all(&real);
    }

    #[test]
    fn control_replies_carry_the_request_id() {
        let v = json!({"type": "control_request", "request_id": "r1", "request": {"subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": "ls"}, "tool_use_id": "t9"}});
        let reply = control_reply(&v, Path::new("/tmp"));
        assert_eq!(reply["response"]["request_id"], "r1");
        assert_eq!(reply["response"]["response"]["behavior"], "deny");
        assert_eq!(reply["response"]["response"]["toolUseID"], "t9");
    }

    #[test]
    fn versions_and_uuids() {
        assert_eq!(parse_version("2.1.304 (Claude Code)"), Some([2, 1, 304]));
        assert!(parse_version("2.1.304 (Claude Code)").unwrap() >= DYNAMIC_SECTIONS_MINIMUM);
        assert_eq!(parse_version("nightly"), None);
        let (a, b) = (new_uuid(), new_uuid());
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
    }

    /// Two questions in one warm process, against the real Claude Code on its own login (costs a
    /// little usage). Run with: cargo test --release live_warm_session -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_warm_session() {
        let exe = find_exe().expect("claude on PATH");
        let root = std::env::temp_dir().join(format!("umalord-cc-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let child: Arc<Mutex<Option<Child>>> = Default::default();
        let mut session = None;
        let mut pids = Vec::new();
        for (q, want) in [("Reply with just the word: apple", "apple"), ("Now reply with just the word: pear", "pear")] {
            let req = Request { exe: exe.clone(), root: root.clone(), model: "claude-haiku-5-5".into(), effort: "low".into(), credential: Credential::Login, session: session.clone(), question: q.into(), mcp: None };
            let (mut text, mut done) = (String::new(), None);
            for e in ask(req, child.clone(), || {}) {
                match e {
                    OcEvent::Session(s) => session = Some(s),
                    OcEvent::Text(t) | OcEvent::TextDelta(t) => text.push_str(&t),
                    OcEvent::Done(r) => done = Some(r),
                    _ => {}
                }
            }
            println!("{q} → {text:?} {done:?}");
            assert_eq!(done, Some(Ok(())));
            assert!(text.to_lowercase().contains(want));
            pids.push(lock(&child).as_ref().map(Child::id));
        }
        assert!(pids[0].is_some() && pids[0] == pids[1], "the second question reused the process: {pids:?}");
        if let Some(mut c) = lock(&child).take() {
            let _ = c.kill();
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn errors_never_show_the_secret() {
        let c = Credential::SetupToken("sk-ant-oat01-supersecret".into());
        assert!(!format!("{c:?}").contains("supersecret"));
        assert_eq!(redact("bad token sk-ant-oat01-supersecret here", &c), "bad token [REDACTED] here");
    }
}
