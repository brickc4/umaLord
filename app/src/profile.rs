//! Profiles: one account's data in one file (`<name>.umalord.json`), to back it up, hand it to
//! someone, or switch accounts. A profile holds the roster (`account/`), saved plans, results and
//! `insights.md`, as text. Importing one replaces the current account's files; the current ones
//! are saved to `profiles/backups/` first, so nothing is lost.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::data::Roster;

pub const EXT: &str = ".umalord.json";
const FORMAT: &str = "umalord-profile";
pub const DIR: &str = "profiles";

/// The files of an empty account.
const BLANK: [(&str, &str); 9] = [
    ("account/umas.md", include_str!("../templates/account/umas.md")),
    ("account/support_cards.md", include_str!("../templates/account/support_cards.md")),
    ("account/parents.md", include_str!("../templates/account/parents.md")),
    ("insights.md", include_str!("../templates/insights.md")),
    ("results/careers.md", include_str!("../templates/results/careers.md")),
    ("results/champions_meeting.md", include_str!("../templates/results/champions_meeting.md")),
    ("results/league_of_heroes.md", include_str!("../templates/results/league_of_heroes.md")),
    ("results/sparks.md", include_str!("../templates/results/sparks.md")),
    ("results/team_trials.md", include_str!("../templates/results/team_trials.md")),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub exported: String,
    /// Counts, for showing the file before importing it.
    #[serde(default)]
    pub umas: usize,
    #[serde(default)]
    pub cards: usize,
    #[serde(default)]
    pub plans: usize,
    /// Relative path → file text.
    pub files: BTreeMap<String, String>,
}

impl Profile {
    /// "57 umas · 100 cards · 1 plan · saved 2026-10-18".
    pub fn summary(&self) -> String {
        let plans = if self.plans == 1 { "1 plan".to_string() } else { format!("{} plans", self.plans) };
        format!("{} umas · {} cards · {plans} · saved {}", self.umas, self.cards, self.exported)
    }
}

/// "2026-10-18-1760781234": today and the Unix time, unique enough for backup names.
pub fn stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("{}-{secs}", crate::ui_util::local_today())
}

/// Whether a profile may write this path: only the account's own files, never outside them.
fn allowed(rel: &str) -> bool {
    let name_ok = |n: &str| !n.is_empty() && n.ends_with(".md") && !n.contains(['/', '\\', ':']) && !n.starts_with('.');
    match rel.split_once('/') {
        Some(("account", f)) => matches!(f, "umas.md" | "support_cards.md" | "parents.md" | "veterans.json"),
        Some(("plans", f)) => (name_ok(f) && f != "_template.md") || rel == crate::parentage::track::FILE || rel == crate::parentage::modes::FILE || rel == crate::parentage::saved::FILE,
        Some(("results", f)) => name_ok(f),
        None => rel == "insights.md",
        _ => false,
    }
}

/// The account's files under `root`, as relative paths.
fn account_files(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = ["account/umas.md", "account/support_cards.md", "account/parents.md", crate::veterans::FILE, "insights.md"].map(String::from).to_vec();
    for dir in ["plans", "results"] {
        let mut names: Vec<String> = std::fs::read_dir(root.join(dir))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .map(|n| format!("{dir}/{n}"))
            .filter(|rel| allowed(rel))
            .collect();
        names.sort();
        out.extend(names);
    }
    out.retain(|rel| root.join(rel).is_file());
    out
}

/// The current account as a profile.
pub fn collect(root: &Path, name: &str, today: &str) -> Result<Profile, String> {
    let mut files = BTreeMap::new();
    for rel in account_files(root) {
        let text = std::fs::read_to_string(root.join(&rel)).map_err(|e| format!("Couldn't read {rel}: {e}"))?;
        files.insert(rel, text);
    }
    let umas = files.get("account/umas.md").map(|t| crate::data::set_meta(t, "Profile", name));
    if let Some(u) = umas {
        files.insert("account/umas.md".into(), u);
    }
    Ok(with_counts(Profile { format: FORMAT.into(), version: 1, name: name.trim().to_string(), exported: today.into(), umas: 0, cards: 0, plans: 0, files }))
}

fn with_counts(mut p: Profile) -> Profile {
    let get = |k: &str| p.files.get(k).map_or("", String::as_str);
    let r = Roster::parse(get("account/support_cards.md"), get("account/umas.md"));
    p.umas = r.umas.len();
    p.cards = r.cards.len();
    p.plans = p.files.keys().filter(|k| k.starts_with("plans/") && k.ends_with(".md")).count();
    p
}

/// A file name from a profile name: "Sam's account" → "sams-account".
fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().filter(|c| !matches!(c, '\'' | '’')).map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "profile".into() } else { s }
}

fn write_profile(p: &Profile, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

/// Save the current account to `profiles/<name>-<date>.umalord.json`; returns the path.
pub fn export(root: &Path, name: &str, today: &str) -> Result<PathBuf, String> {
    let p = collect(root, name, today)?;
    let path = root.join(DIR).join(format!("{}-{today}{EXT}", slug(name)));
    write_profile(&p, &path)?;
    Ok(path)
}

/// Read and check a profile file.
pub fn read(path: &Path) -> Result<Profile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let p: Profile = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| format!("{} isn't a umaLord profile.", file_name(path)))?;
    if p.format != FORMAT {
        return Err(format!("{} isn't a umaLord profile.", file_name(path)));
    }
    if let Some(bad) = p.files.keys().find(|k| !allowed(k)) {
        return Err(format!("{} has a file it may not write ({bad}); not importing it.", file_name(path)));
    }
    if !p.files.contains_key("account/umas.md") || !p.files.contains_key("account/support_cards.md") {
        return Err(format!("{} has no roster in it.", file_name(path)));
    }
    Ok(with_counts(p))
}

pub fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().to_string())
}

/// Save the current account to `profiles/backups/` if it has anything in it; returns the path.
pub fn backup(root: &Path, now: &str) -> Result<Option<PathBuf>, String> {
    let roster = Roster::load(Some(root)).0;
    let plans = account_files(root).iter().any(|f| f.starts_with("plans/"));
    if roster.is_empty() && !plans {
        return Ok(None);
    }
    let name = roster.profile.clone().unwrap_or_else(|| "Backup".into());
    let p = collect(root, &name, &now[..10.min(now.len())])?;
    let path = root.join(DIR).join("backups").join(format!("{}-{}{EXT}", slug(&name), now.replace([':', ' '], "-")));
    write_profile(&p, &path)?;
    Ok(Some(path))
}

/// Empty the account: blank roster, parents, results and insights; saved plans removed.
pub fn clear(root: &Path) -> Result<(), String> {
    for rel in account_files(root).into_iter().filter(|f| f.starts_with("plans/")) {
        let path = root.join(&rel);
        std::fs::remove_file(&path).map_err(|e| format!("Couldn't remove {rel}: {e}"))?;
        let _ = std::fs::remove_file(path.with_extension("simulate.log"));
    }
    // The imported finished umas have no blank template: an empty account has none.
    let _ = std::fs::remove_file(root.join(crate::veterans::FILE));
    for (rel, text) in BLANK {
        let path = root.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, text).map_err(|e| format!("Couldn't write {rel}: {e}"))?;
    }
    Ok(())
}

/// Write any missing account file from its blank template (a new install or a fresh clone has
/// none: `account/`, `results/` and `insights.md` are the user's and stay out of git and zips).
pub fn ensure_account(root: &Path) -> Result<(), String> {
    for (rel, text) in BLANK {
        let path = root.join(rel);
        if path.exists() {
            continue;
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, text).map_err(|e| format!("Couldn't write {rel}: {e}"))?;
    }
    let plans = root.join("plans");
    std::fs::create_dir_all(&plans).map_err(|e| e.to_string())?;
    let template = plans.join("_template.md");
    if !template.exists() {
        std::fs::write(&template, include_str!("../templates/plans/_template.md")).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Back up the current account, empty it, and name the new one. Returns the backup's path.
pub fn start_over(root: &Path, name: &str, now: &str) -> Result<Option<PathBuf>, String> {
    let saved = backup(root, now)?;
    clear(root)?;
    let path = root.join("account/umas.md");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    std::fs::write(&path, crate::data::set_meta(&text, "Profile", name)).map_err(|e| e.to_string())?;
    Ok(saved)
}

/// Replace the current account with a profile (backing the current one up first).
pub fn import(root: &Path, p: &Profile, now: &str) -> Result<Option<PathBuf>, String> {
    let saved = backup(root, now)?;
    clear(root)?;
    for (rel, text) in &p.files {
        if !allowed(rel) {
            continue;
        }
        let path = root.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, text).map_err(|e| format!("Couldn't write {rel}: {e}"))?;
    }
    Ok(saved)
}

/// Profile files to offer: in `profiles/`, its backups and the Downloads folder, newest first.
pub fn find(root: &Path) -> Vec<(PathBuf, Result<Profile, String>)> {
    let mut dirs = vec![root.join(DIR), root.join(DIR).join("backups")];
    if let Some(downloads) = crate::settings::user_dir("Downloads") {
        dirs.push(downloads);
    }
    let mut out: Vec<(PathBuf, std::time::SystemTime)> = dirs
        .iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .filter(|e| e.file_name().to_string_lossy().to_lowercase().ends_with(EXT))
        .map(|e| (e.path(), e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH)))
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out.into_iter().map(|(path, _)| {
        let p = read(&path);
        (path, p)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("umalord-profile-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (rel, text) in BLANK {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        std::fs::create_dir_all(root.join("plans")).unwrap();
        std::fs::write(root.join("plans/_template.md"), "# template").unwrap();
        root
    }

    /// Export from one folder, import into another: the same files come back, and the folder
    /// that imported it had its own data backed up first.
    #[test]
    fn round_trip_with_backup() {
        let a = temp_root("a");
        std::fs::write(a.join("account/umas.md"), crate::data::SAMPLE_UMAS).unwrap();
        std::fs::write(a.join("account/support_cards.md"), crate::data::SAMPLE_CARDS).unwrap();
        std::fs::write(a.join("plans/2026-10-17-oguri-cap-cm.md"), "# a plan").unwrap();
        let path = export(&a, "Sam's account", "2026-10-18").unwrap();
        assert_eq!(file_name(&path), "sams-account-2026-10-18.umalord.json");
        let p = read(&path).unwrap();
        assert_eq!((p.umas, p.cards, p.plans), (57, 100, 1), "{}", p.summary());
        assert!(!p.files.contains_key("plans/_template.md"));

        let b = temp_root("b");
        std::fs::write(b.join("account/umas.md"), crate::data::set_meta(crate::data::SAMPLE_UMAS, "Profile", "Other")).unwrap();
        let saved = import(&b, &p, "2026-10-18 12:00").unwrap().expect("b had umas, so it was backed up");
        assert!(saved.starts_with(b.join("profiles/backups")));
        assert_eq!(read(&saved).unwrap().name, "Other");
        let r = Roster::load(Some(&b)).0;
        assert_eq!((r.umas.len(), r.cards.len(), r.profile.as_deref()), (57, 100, Some("Sam's account")));
        assert_eq!(std::fs::read_to_string(b.join("plans/2026-10-17-oguri-cap-cm.md")).unwrap(), "# a plan");
        assert!(b.join("plans/_template.md").is_file(), "the plan template stays");

        // Start over: backed up, then empty and named.
        let saved = start_over(&b, "Fresh", "2026-10-18 13:00").unwrap();
        assert!(saved.is_some());
        let r = Roster::load(Some(&b)).0;
        assert!(r.is_empty());
        assert_eq!(r.profile.as_deref(), Some("Fresh"));
        assert!(!b.join("plans/2026-10-17-oguri-cap-cm.md").exists());
        // An empty account isn't backed up again.
        assert!(backup(&b, "2026-10-18 14:00").unwrap().is_none());
        for root in [a, b] {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    /// A fresh clone has no account files; the app writes blank ones and leaves existing ones alone.
    #[test]
    fn fresh_folder_gets_blank_account() {
        let root = std::env::temp_dir().join(format!("umalord-profile-fresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("knowledge/data")).unwrap();
        std::fs::write(root.join("knowledge/data/skills.json"), "[]").unwrap();
        assert_eq!(crate::data::find_root(Some(root.clone())).as_deref(), Some(root.as_path()));
        ensure_account(&root).unwrap();
        assert!(Roster::load(Some(&root)).0.is_empty());
        for (rel, _) in BLANK {
            assert!(root.join(rel).is_file(), "{rel}");
        }
        assert!(root.join("plans/_template.md").is_file());
        std::fs::write(root.join("insights.md"), "mine").unwrap();
        ensure_account(&root).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("insights.md")).unwrap(), "mine");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_paths_outside_the_account() {
        assert!(allowed("account/veterans.json"));
        assert!(allowed(crate::parentage::track::FILE) && allowed(crate::parentage::modes::FILE) && !allowed("plans/other.json"));
        for bad in ["../evil.md", "account/../../x.md", "plans/../x.md", "app/src/main.rs", "account/other.md", "results/a\\b.md", "plans/_template.md", "C:/x.md"] {
            assert!(!allowed(bad), "{bad}");
        }
        for ok in ["account/umas.md", "insights.md", "plans/2026-10-17-oguri-cap-cm.md", "results/careers.md"] {
            assert!(allowed(ok), "{ok}");
        }
        let root = temp_root("bad");
        let path = root.join("bad.umalord.json");
        std::fs::write(&path, r#"{"format":"umalord-profile","version":1,"name":"x","exported":"x","files":{"account/umas.md":"","account/support_cards.md":"","../evil.md":"x"}}"#).unwrap();
        assert!(read(&path).unwrap_err().contains("may not write"));
        let _ = std::fs::remove_dir_all(root);
    }
}
