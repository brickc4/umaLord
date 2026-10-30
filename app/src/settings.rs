//! Per-user settings in %APPDATA%\umaLord\settings.json (theme, saved API key), in
//! `~/Library/Application Support/umaLord` on macOS, and `$XDG_CONFIG_HOME/umaLord`
//! (`~/.config/umaLord`) elsewhere.

use std::path::PathBuf;

use serde_json::Value;

/// The per-user folder for settings, the live-data cache and the assistant's bridge.
pub fn app_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.map(|d| d.join("umaLord"))
}

/// A folder in the user's home, like Downloads or Pictures (`%USERPROFILE%\<name>` on Windows).
/// On Linux the desktop's own choice wins: `XDG_DOWNLOAD_DIR` / `XDG_PICTURES_DIR` in
/// `~/.config/user-dirs.dirs` (often lowercase or translated).
pub fn user_dir(name: &str) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?);
    if cfg!(all(unix, not(target_os = "macos"))) {
        let config = std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| home.join(".config"), PathBuf::from);
        let text = std::fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default();
        if let Some(dir) = xdg_user_dir(&text, name, &home) {
            return Some(dir);
        }
    }
    Some(home.join(name))
}

/// `XDG_<NAME>_DIR="$HOME/…"` from a user-dirs.dirs file ("Downloads" asks for XDG_DOWNLOAD_DIR).
fn xdg_user_dir(text: &str, name: &str, home: &std::path::Path) -> Option<PathBuf> {
    // Only Downloads drops its s: XDG_DOWNLOAD_DIR, but XDG_PICTURES_DIR.
    let key = format!("XDG_{}_DIR=", if name == "Downloads" { "DOWNLOAD".to_string() } else { name.to_uppercase() });
    let line = text.lines().map(str::trim).find(|l| l.starts_with(&key))?;
    let value = line[key.len()..].trim_matches('"');
    let path = match value.strip_prefix("$HOME") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(value),
    };
    (path != home).then_some(path)
}

pub fn path() -> Option<PathBuf> {
    app_dir().map(|d| d.join("settings.json"))
}

fn read_all() -> Value {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Default::default()))
}

pub fn get(key: &str) -> Option<Value> {
    read_all().get(key).cloned().filter(|v| !v.is_null())
}

/// Set one key, keeping the others. `Value::Null` removes it.
pub fn set(key: &str, value: Value) -> bool {
    let Some(p) = path() else { return false };
    let mut all = read_all();
    if let Some(map) = all.as_object_mut() {
        if value.is_null() {
            map.remove(key);
        } else {
            map.insert(key.to_string(), value);
        }
    }
    p.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && std::fs::write(&p, all.to_string()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_user_dirs() {
        let home = std::path::Path::new("/home/u");
        let text = "# comment\nXDG_DOWNLOAD_DIR=\"$HOME/downloads\"\nXDG_PICTURES_DIR=\"/data/pics\"\nXDG_MUSIC_DIR=\"$HOME\"\n";
        assert_eq!(xdg_user_dir(text, "Downloads", home), Some(PathBuf::from("/home/u/downloads")));
        assert_eq!(xdg_user_dir(text, "Pictures", home), Some(PathBuf::from("/data/pics")));
        assert_eq!(xdg_user_dir(text, "Music", home), None, "a folder set to home itself means unset");
        assert_eq!(xdg_user_dir("", "Downloads", home), None);
    }
}
