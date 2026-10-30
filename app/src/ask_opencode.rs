//! The Ask tab's free backend: OpenCode (opencode.ai) run in the umaLord folder, read-only.
//!
//! Each question runs `opencode run --format json`, which prints one JSON event per line:
//! `tool_use` (a file read or search, or one of umaLord's tools), `text` (part of the answer),
//! `step_finish` (token counts) and `error`. Follow-up questions pass `--session <id>` so OpenCode
//! keeps the conversation. OpenCode reads AGENTS.md for its instructions and
//! knowledge/digest/owned.md for the roster, and gets umaLord's agent tools from this exe run as
//! an MCP server (`mcp_config`, passed in `OPENCODE_CONFIG_CONTENT`).

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use serde_json::Value;

/// Permissions for every run: it may read and search files, nothing else.
pub const READ_ONLY: &str = r#"{"edit":"deny","bash":"deny","webfetch":"deny","websearch":"deny","external_directory":"deny"}"#;

pub const DEFAULT_MODEL: &str = "opencode/deepseek-v4.1-flash-free";

/// OpenCode keeps sessions in a local SQLite database; two processes at once can fail with
/// "database is locked". Every OpenCode process the app starts holds this lock while it runs.
static RUN_LOCK: Mutex<()> = Mutex::new(());

fn run_lock() -> std::sync::MutexGuard<'static, ()> {
    RUN_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// Strip terminal colour codes (ESC [ ... letter) from OpenCode's error output.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// One readable line from OpenCode's stderr.
fn error_summary(stderr: &str) -> String {
    let text = strip_ansi(stderr);
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() { "OpenCode stopped with an error".to_string() } else { lines.join(" ") }
}

/// Data-use notes for OpenCode's free models, from opencode.ai/docs/zen (checked 2026-10-16).
pub fn model_note(model: &str) -> &'static str {
    let m = model.strip_prefix("opencode/").unwrap_or(model);
    match m {
        "deepseek-v4.1-flash-free" | "glm-5.3-flash-free" | "space-bunny-free" | "longcat-2.5-preview-free" => "keeps nothing, not used for training",
        "beam-free" => "not used for training; kept 30 days",
        "ll-alpha-free" => "may be used to improve the model; kept 30 days",
        "big-pickle" | "exo-free" | "mimo-v2.6-flash-free" | "mimo-v2.5-free" => "may be used to improve the model",
        m if m.starts_with("nemotron") => "NVIDIA trial: use is logged",
        "muse-spark-1.3-contributor-free" => "used to train Meta models",
        _ if model.starts_with("opencode/") => "no data policy listed",
        _ => "your own provider",
    }
}

pub fn is_free(model: &str) -> bool {
    model.starts_with("opencode/") && (model.ends_with("-free") || model == "opencode/big-pickle")
}

/// One finished tool call: the tool ("read", or "umalord_run_compare" for umaLord's own), what it
/// looked at, and what it returned.
#[derive(Debug, PartialEq, Default)]
pub struct ToolEvent {
    pub tool: String,
    pub target: String,
    pub output: String,
    pub error: bool,
}

#[derive(Debug, PartialEq)]
pub enum OcEvent {
    Session(String),
    Text(String),
    /// More of the text being streamed (Claude Code's partial messages): no paragraph break.
    TextDelta(String),
    Tool(ToolEvent),
    Tokens(u64, u64),
    Error(String),
    Done(Result<(), String>),
}

/// The bundled tools/opencode/opencode.exe, else `opencode` on PATH.
pub fn find_exe(root: Option<&Path>) -> Option<PathBuf> {
    let name = if cfg!(windows) { "opencode.exe" } else { "opencode" };
    if let Some(p) = root.map(|r| r.join("tools").join("opencode").join(name))
        && p.is_file()
    {
        return Some(p);
    }
    std::env::var_os("PATH")?.to_str()?.split(if cfg!(windows) { ';' } else { ':' }).map(|d| Path::new(d).join(name)).find(|p| p.is_file())
}

pub(crate) fn command(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::ask::hidden(&mut cmd);
    cmd
}

/// Models OpenCode can use right now (`opencode models`), free ones first.
pub fn list_models(exe: PathBuf) -> Receiver<Result<Vec<String>, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let list = || {
            let _lock = run_lock();
            command(&exe).arg("models").output().map_err(|e| e.to_string()).and_then(|out| {
                let mut models: Vec<String> = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|l| l.contains('/') && !l.contains(' '))
                    .map(String::from)
                    .collect();
                models.sort_by_key(|m| (!is_free(m), m.clone()));
                if models.is_empty() { Err(error_summary(&String::from_utf8_lossy(&out.stderr))) } else { Ok(models) }
            })
        };
        let mut result = list();
        // Another OpenCode (say, one the user runs) may hold the database; try again shortly.
        for _ in 0..3 {
            if !result.as_ref().is_err_and(|e| e.contains("locked")) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1500));
            result = list();
        }
        let _ = tx.send(result);
    });
    rx
}

/// Turn one line of `--format json` output into events.
pub fn parse_line(line: &str) -> Vec<OcEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(s) = v["sessionID"].as_str() {
        out.push(OcEvent::Session(s.to_string()));
    }
    let part = &v["part"];
    match v["type"].as_str().unwrap_or_default() {
        "text" => {
            if let Some(t) = part["text"].as_str().filter(|t| !t.trim().is_empty()) {
                out.push(OcEvent::Text(t.to_string()));
            }
        }
        "tool_use" => {
            let state = &part["state"];
            let input = &state["input"];
            let target = ["filePath", "pattern", "path", "description", "query"].iter().find_map(|k| input[k].as_str()).unwrap_or_default();
            let output = state["output"].as_str().or_else(|| state["error"].as_str()).unwrap_or_default();
            out.push(OcEvent::Tool(ToolEvent {
                tool: part["tool"].as_str().unwrap_or("tool").to_string(),
                target: target.to_string(),
                output: output.to_string(),
                error: state["status"] == "error",
            }));
        }
        "step_finish" => {
            let t = &part["tokens"];
            let input = t["input"].as_u64().unwrap_or(0) + t["cache"]["read"].as_u64().unwrap_or(0) + t["cache"]["write"].as_u64().unwrap_or(0);
            out.push(OcEvent::Tokens(input, t["output"].as_u64().unwrap_or(0) + t["reasoning"].as_u64().unwrap_or(0)));
        }
        "error" => {
            let e = &v["error"];
            let msg = e["data"]["message"].as_str().or_else(|| e["message"].as_str()).or_else(|| e["name"].as_str()).unwrap_or("OpenCode reported an error");
            out.push(OcEvent::Error(msg.to_string()));
        }
        _ => {}
    }
    out
}

/// OpenCode's config for umaLord's tools: this exe as a local MCP server on the folder, with the
/// bridge folder the window watches for approvals (opencode.ai/docs/mcp-servers, checked
/// 2026-10-18: `mcp.<name>` = `{type: "local", command, environment, timeout}`; inline config
/// from `OPENCODE_CONFIG_CONTENT` overrides opencode.json).
pub fn mcp_config(exe: &Path, root: &Path, bridge: Option<&Path>) -> String {
    let mut env = serde_json::Map::new();
    if let Some(b) = bridge {
        env.insert(crate::agent::bridge::ENV.into(), Value::String(b.display().to_string()));
    }
    serde_json::json!({
        "mcp": {"umalord": {
            "type": "local",
            "command": [exe.display().to_string(), "--mcp", root.display().to_string()],
            "environment": env,
            "enabled": true,
            // Loading the game data takes a moment; the tool list itself doesn't wait for it.
            "timeout": 20000,
        }}
    })
    .to_string()
}

/// Run one question. Events arrive on the returned channel, ending with `Done`.
/// `child` holds the running process so the UI can stop it. `mcp` is `mcp_config`'s JSON.
#[allow(clippy::too_many_arguments)]
pub fn ask(
    exe: PathBuf,
    root: PathBuf,
    model: String,
    session: Option<String>,
    question: String,
    mcp: Option<String>,
    child: Arc<Mutex<Option<Child>>>,
    wake: impl Fn() + Send + 'static,
) -> Receiver<OcEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let send = |tx: &Sender<OcEvent>, e: OcEvent| {
            let _ = tx.send(e);
            wake();
        };
        let mut cmd = command(&exe);
        cmd.current_dir(&root).env("OPENCODE_PERMISSION", READ_ONLY).args(["run", "--format", "json", "--model", &model, "--dir"]).arg(&root);
        if let Some(m) = &mcp {
            cmd.env("OPENCODE_CONFIG_CONTENT", m);
        }
        if let Some(s) = &session {
            cmd.args(["--session", s]);
        }
        cmd.arg("--").arg(&question);
        let _lock = run_lock();
        let mut proc = match cmd.spawn() {
            Ok(p) => p,
            Err(e) => return send(&tx, OcEvent::Done(Err(format!("couldn't start OpenCode: {e}")))),
        };
        let stdout = proc.stdout.take();
        let mut stderr = proc.stderr.take();
        let err_text = std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(e) = stderr.as_mut() {
                let _ = e.read_to_string(&mut s);
            }
            s
        });
        *child.lock().unwrap_or_else(|p| p.into_inner()) = Some(proc);
        let mut saw_error = false;
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                for e in parse_line(&line) {
                    saw_error |= matches!(e, OcEvent::Error(_));
                    send(&tx, e);
                }
            }
        }
        let status = child.lock().unwrap_or_else(|p| p.into_inner()).take().and_then(|mut p| p.wait().ok());
        let err = err_text.join().unwrap_or_default();
        let result = match status {
            Some(s) if s.success() || saw_error => Ok(()),
            Some(_) => Err(error_summary(&err)),
            None => Err("Stopped.".to_string()),
        };
        send(&tx, OcEvent::Done(result));
    });
    rx
}

// ---------------------------------------------------------------------------
// Install

/// The OpenCode release the app installs, pinned with the SHA-256 GitHub publishes for it.
/// To update: take the new release's `opencode-windows-x64.zip` URL, size and `digest` from
/// https://api.github.com/repos/anomalyco/opencode/releases/latest.
pub const RELEASE: &str = "v1.19.3";
const ZIP_URL: &str = "https://github.com/anomalyco/opencode/releases/download/v1.19.3/opencode-windows-x64.zip";
const ZIP_SHA256: &str = "e4ce9117f55fc068414513789f8c7d1049f855d7518bb4e350b9170a3a20d8c3";
pub const ZIP_SIZE: u64 = 62_204_138;

pub enum InstallEvent {
    /// Bytes downloaded so far.
    Progress(u64),
    Done(Result<PathBuf, String>),
}

pub fn install_dir(root: &Path) -> PathBuf {
    root.join("tools").join("opencode")
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Download the pinned Windows release with curl.exe, check its size and SHA-256, and extract
/// opencode.exe into `dir`. Nothing is extracted unless the checksum matches.
pub fn install_blocking(dir: &Path, progress: impl Fn(u64)) -> Result<PathBuf, String> {
    if !cfg!(windows) {
        return Err("The pinned OpenCode download is the Windows build. Install opencode with your package manager instead.".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    let part = dir.join("opencode-download.zip.part");
    let _ = std::fs::remove_file(&part);
    let mut cmd = Command::new("curl.exe");
    cmd.args(["-L", "--fail", "--silent", "--show-error", "-o"]).arg(&part).arg(ZIP_URL);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    crate::ask::hidden(&mut cmd);
    let mut curl = cmd.spawn().map_err(|e| format!("couldn't start curl.exe: {e}"))?;
    let status = loop {
        if let Some(s) = curl.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        progress(std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0));
        std::thread::sleep(std::time::Duration::from_millis(250));
    };
    if !status.success() {
        let mut err = String::new();
        if let Some(mut e) = curl.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        let _ = std::fs::remove_file(&part);
        return Err(format!("Download failed: {}", err.trim()));
    }
    let size = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    progress(size);
    let hash = sha256_file(&part).map_err(|e| e.to_string())?;
    if size != ZIP_SIZE || hash != ZIP_SHA256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("The download didn't match the published checksum ({size} bytes, sha256 {hash}), so nothing was installed."));
    }
    let extracted = (|| -> Result<PathBuf, String> {
        let file = std::fs::File::open(&part).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        let mut entry = zip.by_name("opencode.exe").map_err(|e| format!("opencode.exe not found in the download: {e}"))?;
        let tmp = dir.join("opencode.exe.part");
        let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        drop(out);
        let exe = dir.join("opencode.exe");
        let _ = std::fs::remove_file(&exe);
        std::fs::rename(&tmp, &exe).map_err(|e| e.to_string())?;
        Ok(exe)
    })();
    let _ = std::fs::remove_file(&part);
    extracted
}

pub fn install(dir: PathBuf, wake: impl Fn() + Send + 'static) -> Receiver<InstallEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let result = install_blocking(&dir, |n| {
            let _ = tx.send(InstallEvent::Progress(n));
            wake();
        });
        let _ = tx.send(InstallEvent::Done(result));
        wake();
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Downloads ~59 MB. Run with: cargo test install_downloads -- --ignored --nocapture
    #[test]
    #[ignore]
    fn install_downloads_and_verifies() {
        let dir = std::env::temp_dir().join(format!("umalord-oc-test-{}", std::process::id()));
        let exe = install_blocking(&dir, |_| {}).expect("install");
        let out = Command::new(&exe).arg("--version").output().expect("run");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), RELEASE.trim_start_matches('v'));
        eprintln!("installed and ran {}", exe.display());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_are_readable() {
        assert_eq!(strip_ansi("\u{1b}[91m\u{1b}[1mError: \u{1b}[0mUnexpected error"), "Error: Unexpected error");
        assert_eq!(error_summary("\u{1b}[91mError:\u{1b}[0m Unexpected error\n\ndatabase is locked\n"), "Error: Unexpected error database is locked");
    }

    #[test]
    fn sha256_known_answer() {
        let path = std::env::temp_dir().join(format!("umalord-sha-{}.txt", std::process::id()));
        std::fs::write(&path, "abc").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parses_opencode_events() {
        let text = r#"{"type":"text","sessionID":"ses_1","part":{"type":"text","text":"OK umaLord"}}"#;
        assert_eq!(parse_line(text), vec![OcEvent::Session("ses_1".into()), OcEvent::Text("OK umaLord".into())]);
        let tool = r#"{"type":"tool_use","sessionID":"ses_1","part":{"type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"notes.txt"}}}}"#;
        assert_eq!(parse_line(tool)[1], OcEvent::Tool(ToolEvent { tool: "read".into(), target: "notes.txt".into(), ..Default::default() }));
        let mcp = r#"{"type":"tool_use","part":{"type":"tool","tool":"umalord_run_compare","state":{"status":"completed","input":{},"output":"Compare (simulation output)"}}}"#;
        assert_eq!(parse_line(mcp)[0], OcEvent::Tool(ToolEvent { tool: "umalord_run_compare".into(), output: "Compare (simulation output)".into(), ..Default::default() }));
        let cfg: Value = serde_json::from_str(&mcp_config(Path::new("C:/u/umalord.exe"), Path::new("C:/u"), Some(Path::new("C:/a")))).unwrap();
        assert_eq!(cfg["mcp"]["umalord"]["command"], serde_json::json!(["C:/u/umalord.exe", "--mcp", "C:/u"]));
        assert_eq!(cfg["mcp"]["umalord"]["environment"][crate::agent::bridge::ENV], "C:/a");
        let fin = r#"{"type":"step_finish","sessionID":"ses_1","part":{"tokens":{"input":972,"output":39,"reasoning":10,"cache":{"read":4096,"write":0}},"cost":0}}"#;
        assert_eq!(parse_line(fin)[1], OcEvent::Tokens(5068, 49));
        assert!(parse_line("not json").is_empty());
    }

    #[test]
    fn free_model_notes() {
        assert!(is_free("opencode/deepseek-v4.1-flash-free") && is_free("opencode/big-pickle"));
        assert!(!is_free("anthropic/claude-opus-5-5"));
        assert_eq!(model_note(DEFAULT_MODEL), "keeps nothing, not used for training");
    }
}
