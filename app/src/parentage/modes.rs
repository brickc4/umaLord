//! How each finished career was run: Independent Training (Global's 50-minute background
//! career) or manual. The finished-uma import (a memory reader's snapshot; the game has no
//! export) holds the uma, not how her career was played, so the player marks it, one tap at
//! check-off; unmarked careers take the default for what they were for (Independent Training
//! for a parent step, manual for a runner). Kept in plans/career-modes.json by finished uma id.

use crate::veterans::Veteran;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const FILE: &str = "plans/career-modes.json";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Independent,
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ModeLog {
    pub marks: BTreeMap<u64, Mode>,
}

impl ModeLog {
    pub fn mark(&mut self, id: u64, mode: Mode) {
        self.marks.insert(id, mode);
    }

    /// The mode a finished uma ran in, or `default` when the player hasn't said.
    pub fn of(&self, id: u64, default: Mode) -> Mode {
        self.marks.get(&id).copied().unwrap_or(default)
    }

    /// Of a runner outfit's careers marked Independent Training, how many reached the stamina
    /// floor: (reached, marked). None until one is marked, so the card shows nothing invented.
    pub fn floor_rate(&self, vets: &[Veteran], outfit: u32, stamina: u32) -> Option<(u32, u32)> {
        let runs: Vec<&Veteran> = vets.iter().filter(|v| v.card_id == outfit && self.marks.get(&v.trained_chara_id) == Some(&Mode::Independent)).collect();
        (!runs.is_empty()).then(|| (runs.iter().filter(|v| v.stamina >= stamina).count() as u32, runs.len() as u32))
    }
}

pub fn load(root: &Path) -> ModeLog {
    std::fs::read_to_string(root.join(FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(root: &Path, log: &ModeLog) -> Result<(), String> {
    let path = root.join(FILE);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(log).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vet(id: u64, card: u32, stamina: u32) -> Veteran {
        Veteran { trained_chara_id: id, card_id: card, stamina, ..Default::default() }
    }

    #[test]
    fn floor_rate_counts_only_marked_independent_runs() {
        let vets = vec![vet(1, 100601, 820), vet(2, 100601, 700), vet(3, 100601, 900), vet(4, 100101, 950)];
        let mut log = ModeLog::default();
        assert_eq!(log.floor_rate(&vets, 100601, 800), None, "nothing marked: nothing to show");
        log.mark(1, Mode::Independent);
        log.mark(2, Mode::Independent);
        log.mark(3, Mode::Manual);
        log.mark(4, Mode::Independent);
        assert_eq!(log.floor_rate(&vets, 100601, 800), Some((1, 2)));
        assert_eq!(log.of(3, Mode::Independent), Mode::Manual);
        assert_eq!(log.of(9, Mode::Independent), Mode::Independent, "unmarked takes the default");
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("umalord-modes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load(&dir), ModeLog::default());
        let mut log = ModeLog::default();
        log.mark(812, Mode::Manual);
        save(&dir, &log).unwrap();
        assert_eq!(load(&dir), log);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
