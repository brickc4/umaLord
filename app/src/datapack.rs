//! The weekly game-data pack: knowledge/data and the uma and card art, refreshed from uma.guide
//! by `.github/workflows/data-pack.yml`, tested, and published as the "data-pack" release
//! (`tools/make_data_pack.py`). Once a week, when fetching is allowed (Settings → Data and
//! updates), the app reads the pack's manifest, compares each file's SHA-256 with its own copy,
//! and takes only the ones that differ, each checked against the manifest before it's written.
//! So new umas and cards (with their art) arrive without a new app release, and a package update
//! that copies older data over the data folder is put right at the next check.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

/// The pack format this app reads (`SCHEMA` in tools/make_data_pack.py).
pub const SCHEMA: u32 = 1;
/// Where the pack is published: the repository the app is built from (Cargo.toml's
/// `repository`). `data_pack_url` in settings.json overrides it, e.g. for a fork.
pub const URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases/download/data-pack");
/// Days between checks.
pub const EVERY_DAYS: i64 = 7;

#[derive(Deserialize, Debug, Clone)]
pub struct Manifest {
    pub schema: u32,
    pub built: String,
    pub zip_sha256: String,
    pub files: Vec<PackFile>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PackFile {
    pub path: String,
    pub sha256: String,
}

/// What a check did.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    UpToDate { built: String },
    Updated { built: String, files: Vec<String> },
}

/// Only these may be written: the game data and the art, nothing else in the data folder.
pub fn allowed(path: &str) -> bool {
    let ok_name = |n: &str, ext: &str| !n.is_empty() && n.ends_with(ext) && !n.contains(['\\', ':']) && !n.starts_with('.');
    if path.contains("..") || path.starts_with('/') {
        return false;
    }
    match path.split_once('/') {
        Some(("knowledge", rest)) => match rest.strip_prefix("data/") {
            Some(f) => match f.split_once('/') {
                None => ok_name(f, ".json"),
                Some(("sim", g)) => !g.contains('/') && ok_name(g, ".json"),
                _ => false,
            },
            None => false,
        },
        Some(("app", rest)) => rest
            .strip_prefix("assets/umas/")
            .or_else(|| rest.strip_prefix("assets/cards/"))
            .is_some_and(|f| !f.contains('/') && ok_name(f, ".webp")),
        _ => false,
    }
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The manifest's files whose local copy is missing or differs.
pub fn changed(root: &Path, m: &Manifest) -> Vec<PackFile> {
    m.files
        .iter()
        .filter(|f| allowed(&f.path))
        .filter(|f| std::fs::read(root.join(&f.path)).map_or(true, |b| hex(&b) != f.sha256))
        .cloned()
        .collect()
}

/// Write the changed files from the pack's zip into `root`, each checked against the manifest.
pub fn apply(root: &Path, m: &Manifest, zip_bytes: &[u8], wanted: &[PackFile]) -> Result<Vec<String>, String> {
    if hex(zip_bytes) != m.zip_sha256 {
        return Err("the pack's zip doesn't match its manifest".into());
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(|e| e.to_string())?;
    // Read and check everything first, so a bad entry leaves the data folder untouched.
    let mut ready: Vec<(PathBuf, Vec<u8>, String)> = Vec::new();
    for f in wanted {
        if !allowed(&f.path) {
            return Err(format!("the pack lists a file it may not write: {}", f.path));
        }
        let mut entry = zip.by_name(&f.path).map_err(|e| format!("{}: {e}", f.path))?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes).map_err(|e| e.to_string())?;
        if hex(&bytes) != f.sha256 {
            return Err(format!("{} doesn't match the manifest", f.path));
        }
        ready.push((root.join(&f.path), bytes, f.path.clone()));
    }
    let mut written = Vec::new();
    for (path, bytes, rel) in ready {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let part = path.with_extension("part");
        std::fs::write(&part, &bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&part, &path).map_err(|e| e.to_string())?;
        written.push(rel);
    }
    Ok(written)
}

fn curl(url: &str) -> Result<Vec<u8>, String> {
    let mut cmd = std::process::Command::new(crate::ask::CURL);
    cmd.args(["-sSfL", "--max-time", "120", url]);
    crate::ask::hidden(&mut cmd);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(out.stdout) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// Check the pack and take what changed (on a background thread).
pub fn check(root: &Path, base: &str) -> Result<Outcome, String> {
    let m: Manifest = serde_json::from_slice(&curl(&format!("{base}/data-pack.json"))?).map_err(|e| format!("manifest: {e}"))?;
    if m.schema != SCHEMA {
        return Err(format!("the pack is format {}, this app reads {SCHEMA}: update the app", m.schema));
    }
    let wanted = changed(root, &m);
    if wanted.is_empty() {
        return Ok(Outcome::UpToDate { built: m.built });
    }
    let zip = curl(&format!("{base}/data-pack.zip"))?;
    let files = apply(root, &m, &zip, &wanted)?;
    Ok(Outcome::Updated { built: m.built, files })
}

pub fn spawn(root: PathBuf, ctx: eframe::egui::Context) -> Receiver<Result<Outcome, String>> {
    let (tx, rx) = channel();
    let base = crate::settings::get("data_pack_url").and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| URL.to_string());
    std::thread::spawn(move || {
        let _ = tx.send(check(&root, &base));
        ctx.request_repaint();
    });
    rx
}

/// Whether a week has passed since the last check (`data_pack_checked` in settings.json).
pub fn due(today: &str) -> bool {
    crate::settings::get("data_pack_checked").and_then(|v| v.as_str().map(String::from)).is_none_or(|d| crate::tab_planner::days_apart(today, &d) >= EVERY_DAYS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn pack(files: &[(&str, &[u8])]) -> (Manifest, Vec<u8>) {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (p, b) in files {
                z.start_file(*p, zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(b).unwrap();
            }
            z.finish().unwrap();
        }
        let bytes = buf.into_inner();
        let m = Manifest { schema: SCHEMA, built: "2026-10-28T06:00:00+00:00".into(), zip_sha256: hex(&bytes), files: files.iter().map(|(p, b)| PackFile { path: p.to_string(), sha256: hex(b) }).collect() };
        (m, bytes)
    }

    /// A real pack from tools/make_data_pack.py, served locally:
    /// `UMALORD_PACK_URL=http://127.0.0.1:<port> cargo test --release pack_end_to_end -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn pack_end_to_end() {
        let base = std::env::var("UMALORD_PACK_URL").expect("UMALORD_PACK_URL");
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let dir = std::env::temp_dir().join(format!("umalord-pack-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // An install a week behind: one card's art gone, an older umas.json.
        for sub in ["knowledge/data", "app/assets/cards"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        std::fs::write(dir.join("knowledge/data/umas.json"), b"[]").unwrap();
        std::fs::copy(repo.join("knowledge/data/cards.json"), dir.join("knowledge/data/cards.json")).unwrap();
        let first = check(&dir, &base).unwrap();
        let Outcome::Updated { files, .. } = &first else { panic!("{first:?}") };
        eprintln!("first check took {} files", files.len());
        assert!(files.contains(&"knowledge/data/umas.json".to_string()) && !files.contains(&"knowledge/data/cards.json".to_string()));
        assert_eq!(std::fs::read(dir.join("knowledge/data/umas.json")).unwrap(), std::fs::read(repo.join("knowledge/data/umas.json")).unwrap());
        assert!(dir.join("app/assets/cards/10001.webp").is_file());
        let again = check(&dir, &base).unwrap();
        assert!(matches!(again, Outcome::UpToDate { .. }), "{again:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_only_game_data_and_art() {
        for ok in ["knowledge/data/umas.json", "knowledge/data/sim/skill_data.json", "app/assets/umas/100101.webp", "app/assets/cards/30028.webp"] {
            assert!(allowed(ok), "{ok}");
        }
        for bad in ["account/umas.md", "plans/parents.json", "knowledge/data/../../x.json", "/etc/passwd", "app/src/main.rs", "knowledge/data/x.md", "knowledge/data/sim/a/b.json", "app/assets/umas/x.png", "knowledge/modes.md"] {
            assert!(!allowed(bad), "{bad}");
        }
    }

    #[test]
    fn takes_only_what_changed_and_checks_it() {
        let dir = std::env::temp_dir().join(format!("umalord-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("knowledge/data")).unwrap();
        std::fs::write(dir.join("knowledge/data/umas.json"), b"old").unwrap();
        std::fs::write(dir.join("knowledge/data/cards.json"), b"same").unwrap();
        let (m, zip) = pack(&[("knowledge/data/umas.json", b"new"), ("knowledge/data/cards.json", b"same"), ("app/assets/umas/110001.webp", b"art")]);
        let wanted = changed(&dir, &m);
        assert_eq!(wanted.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["knowledge/data/umas.json", "app/assets/umas/110001.webp"]);
        let written = apply(&dir, &m, &zip, &wanted).unwrap();
        assert_eq!(written.len(), 2);
        assert_eq!(std::fs::read(dir.join("knowledge/data/umas.json")).unwrap(), b"new");
        assert!(changed(&dir, &m).is_empty(), "nothing left to take");
        // A zip that doesn't match its manifest is refused, and nothing is written.
        let (m2, mut zip2) = pack(&[("knowledge/data/umas.json", b"newer")]);
        zip2.push(0);
        assert!(apply(&dir, &m2, &zip2, &changed(&dir, &m2)).is_err());
        assert_eq!(std::fs::read(dir.join("knowledge/data/umas.json")).unwrap(), b"new");
        // A manifest naming a file outside the data is refused.
        let (mut m3, zip3) = pack(&[("account/umas.md", b"x")]);
        m3.files[0].path = "account/umas.md".into();
        assert!(apply(&dir, &m3, &zip3, &m3.files.clone()).is_err());
        assert!(!dir.join("account/umas.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
