//! Finished umas (the game's veterans: the umas a career leaves behind, which become parents).
//!
//! The game has no export, so they come from a memory reader: UmaExtractor's `data.json` on
//! Windows (github.com/xancia/UmaExtractor), or `tools/read_veterans.py` on Linux, which writes the
//! same format. That's the server's own `trained_chara_array`: per uma its card, stats, aptitudes,
//! skills, sparks (`factor_info_array`), win records (`win_saddle_id_array`) and its parents and
//! grandparents (`succession_chara_array`, positions 10/20 and 11, 12 / 21, 22).
//!
//! Importing keeps the records, without anyone's account ids, in `account/veterans.json`, and
//! rewrites the imported rows of `account/parents.md` (marked by `#<id>` after the name; rows typed
//! by hand stay), which the planner's parents and Step 5 already read.

use crate::App;
use crate::knowledge::Knowledge;
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

pub const FILE: &str = "account/veterans.json";
const PARENTS_FILE: &str = "account/parents.md";

/// The game's rank ids, 1 = G … 19 = UG.
const RANKS: [&str; 19] = ["G", "G+", "F", "F+", "E", "E+", "D", "D+", "C", "C+", "B", "B+", "A", "A+", "S", "S+", "SS", "SS+", "UG"];

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Veteran {
    pub trained_chara_id: u64,
    pub card_id: u32,
    pub rank: u32,
    pub rank_score: u32,
    /// Stars (1–5) when it was trained.
    pub rarity: u8,
    /// Potential (1–5) when it was trained.
    pub talent_level: u8,
    pub scenario_id: u32,
    pub speed: u32,
    pub stamina: u32,
    pub power: u32,
    pub guts: u32,
    pub wiz: u32,
    pub create_time: Value,
    pub factor_info_array: Vec<FactorRef>,
    /// Older dumps list the ids alone.
    pub factor_id_array: Vec<u32>,
    pub win_saddle_id_array: Vec<u32>,
    pub succession_chara_array: Vec<Ancestor>,
    /// Skills learned, the unique with its level.
    pub skill_array: Vec<SkillRef>,
    /// Running style it raced with: 1 Front, 2 Pace, 3 Late, 4 End.
    pub running_style: u8,
    /// Aptitudes after inheritance, 1 = G … 8 = S.
    pub proper_ground_turf: u8,
    pub proper_ground_dirt: u8,
    pub proper_distance_short: u8,
    pub proper_distance_mile: u8,
    pub proper_distance_middle: u8,
    pub proper_distance_long: u8,
    pub proper_running_style_nige: u8,
    pub proper_running_style_senko: u8,
    pub proper_running_style_sashi: u8,
    pub proper_running_style_oikomi: u8,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SkillRef {
    pub skill_id: u32,
    pub level: u32,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct FactorRef {
    pub factor_id: u32,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Ancestor {
    pub position_id: u32,
    pub card_id: u32,
    pub rank: u32,
    pub talent_level: u8,
    pub factor_info_array: Vec<FactorRef>,
    pub factor_id_array: Vec<u32>,
    pub win_saddle_id_array: Vec<u32>,
}

fn factor_ids(info: &[FactorRef], ids: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = info.iter().map(|f| f.factor_id).chain(ids.iter().copied()).collect();
    out.dedup();
    out
}

impl Veteran {
    pub fn factors(&self) -> Vec<u32> {
        factor_ids(&self.factor_info_array, &self.factor_id_array)
    }

    /// Its parent at `position` (10 or 20), or that parent's parents (11, 12 / 21, 22).
    pub fn ancestor(&self, position: u32) -> Option<&Ancestor> {
        self.succession_chara_array.iter().find(|a| a.position_id == position)
    }

    pub fn rank_label(&self) -> String {
        rank_label(self.rank)
    }

    /// The day it finished, from `create_time` ("2025-09-16 02:50:09").
    pub fn date(&self) -> String {
        self.create_time.as_str().map(|s| s.chars().take(10).collect()).unwrap_or_default()
    }

    /// As a legacy for affinity: its G1 wins, and its own parents with theirs.
    pub fn legacy(&self, kb: &Knowledge) -> crate::affinity::Legacy {
        let gp = |pos| self.ancestor(pos).map(|a| crate::affinity::Ancestor { uma: a.card_id, wins: kb.g1_wins(&a.win_saddle_id_array) });
        crate::affinity::Legacy { uma: self.card_id, wins: kb.g1_wins(&self.win_saddle_id_array), parents: [gp(10), gp(20)] }
    }
}

impl Ancestor {
    pub fn factors(&self) -> Vec<u32> {
        factor_ids(&self.factor_info_array, &self.factor_id_array)
    }
}

pub fn rank_label(rank: u32) -> String {
    match rank {
        1..=19 => RANKS[rank as usize - 1].to_string(),
        0 => "?".into(),
        _ => "UG+".into(),
    }
}

/// What the finished umas prove about the roster: every trainee they used is owned, at no fewer
/// stars and no lower potential than she was trained at (neither ever goes down). Edits that add
/// a missing uma or raise one, never lower anything.
pub fn roster_edits(kb: &Knowledge, roster: &crate::data::Roster, vets: &[Veteran]) -> Vec<crate::onboarding::Edit> {
    let mut need: std::collections::BTreeMap<u32, (u8, u8)> = Default::default();
    for v in vets.iter().filter(|v| kb.umas.contains_key(&v.card_id) && v.rarity > 0) {
        let e = need.entry(v.card_id).or_default();
        *e = (e.0.max(v.rarity.min(5)), e.1.max(v.talent_level.clamp(1, 5)));
    }
    need.into_iter()
        .filter_map(|(id, (stars, potential))| {
            let have = roster.umas.iter().find(|u| u.id == id).map(|u| (u.stars, u.potential));
            let (s, p) = have.map_or((stars, potential), |(s, p)| (s.max(stars), p.max(potential)));
            (have != Some((s, p))).then_some(crate::onboarding::Edit::Uma { id, stars: s, potential: p })
        })
        .collect()
}

/// Read a data.json (an array of finished umas, or an object holding one as
/// `trained_chara_array`), drop every account id in it, and keep one record per uma.
pub fn parse(text: &str) -> Result<Vec<Value>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("Not JSON: {e}"))?;
    let list = match v {
        Value::Array(a) => a,
        Value::Object(mut o) => match o.remove("trained_chara_array") {
            Some(Value::Array(a)) => a,
            _ => return Err("No finished umas in it (expected UmaExtractor's data.json).".into()),
        },
        _ => return Err("Expected a list of finished umas (UmaExtractor's data.json).".into()),
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for mut item in list {
        let Some(obj) = item.as_object_mut() else { continue };
        // A finished uma: its id, its outfit, and its sparks or lineage (not just any list with ids).
        let (Some(id), Some(_)) = (obj.get("trained_chara_id").and_then(Value::as_u64), obj.get("card_id").and_then(Value::as_u64)) else { continue };
        if !["factor_info_array", "factor_id_array", "succession_chara_array"].iter().any(|k| obj.get(*k).is_some_and(Value::is_array)) {
            continue;
        }
        if !seen.insert(id) {
            continue;
        }
        scrub(&mut item);
        out.push(item);
    }
    if out.is_empty() {
        return Err("No finished umas in it (expected UmaExtractor's data.json).".into());
    }
    Ok(out)
}

/// Remove account ids (`viewer_id`, `owner_viewer_id`) at any depth: borrowed parents carry
/// other players' ids.
fn scrub(v: &mut Value) {
    match v {
        Value::Object(o) => {
            o.remove("viewer_id");
            o.remove("owner_viewer_id");
            o.values_mut().for_each(scrub);
        }
        Value::Array(a) => a.iter_mut().for_each(scrub),
        _ => {}
    }
}

/// The imported finished umas; a record that doesn't read is skipped, not the whole file.
pub fn load(root: &Path) -> Vec<Veteran> {
    let records: Vec<Value> = std::fs::read_to_string(root.join(FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    records.into_iter().filter_map(|r| serde_json::from_value(r).ok()).collect()
}

/// "Speed 3★", from a spark id.
fn spark(kb: &Knowledge, id: u32) -> Option<(&str, String)> {
    let s = kb.sparks.get(&id)?;
    Some((s.kind.as_str(), format!("{} {}★", s.name, s.stars)))
}

/// A list of sparks split by colour: (blue, pink, green, white incl. race and scenario).
fn split(kb: &Knowledge, ids: &[u32]) -> [Vec<String>; 4] {
    let mut out: [Vec<String>; 4] = Default::default();
    for id in ids {
        let Some((kind, text)) = spark(kb, *id) else { continue };
        let i = match kind {
            "blue" => 0,
            "pink" => 1,
            "green" => 2,
            _ => 3,
        };
        out[i].push(text);
    }
    out
}

fn uma_name(kb: &Knowledge, card: u32) -> String {
    kb.umas.get(&card).map_or_else(|| format!("uma {card}"), |u| format!("{} [{}]", u.name, u.outfit))
}

fn scenario_name(kb: &Knowledge, id: u32) -> String {
    scenario_of(id).and_then(|n| kb.scenarios.as_ref()?.scenarios.iter().find(|s| s.name == n).map(|s| s.name.clone())).unwrap_or_else(|| format!("scenario {id}"))
}

/// The game's scenario id, as a scenarios.json name. Not Global's release order: the ids follow
/// JP's, so 3 is Grand Concert (JP Grand Live) and 4 Trackblazer (JP Climax). Checked
/// 2026-10-29 against the scenario sparks real careers carry: every id-3 career holds "Our Grand
/// Concert", every id-4 one "TS Climax Scenario".
pub(crate) fn scenario_of(id: u32) -> Option<&'static str> {
    match id {
        1 => Some("URA Finale"),
        2 => Some("Unity Cup"),
        3 => Some("Grand Concert"),
        4 => Some("Trackblazer"),
        _ => None,
    }
}

/// One ancestor in a cell: "Gold Ship [Red Strife]: Stamina 2★ · Long 1★ · Unique 1★, 4 white".
fn ancestor_line(kb: &Knowledge, a: &Ancestor) -> String {
    let [blue, pink, green, white] = split(kb, &a.factors());
    let mut parts: Vec<String> = blue.into_iter().chain(pink).chain(green).collect();
    if !white.is_empty() {
        parts.push(format!("{} white", white.len()));
    }
    format!("{}: {}", uma_name(kb, a.card_id), if parts.is_empty() { "no sparks".into() } else { parts.join(" · ") })
}

fn cell(s: &str) -> String {
    s.replace('|', "/").replace('\n', " ")
}

/// The parents.md row for a finished uma.
pub fn row(kb: &Knowledge, v: &Veteran) -> String {
    let [blue, pink, green, white] = split(kb, &v.factors());
    let lineage: Vec<String> = [(10, 11, 12), (20, 21, 22)]
        .into_iter()
        .filter_map(|(p, g1, g2)| {
            let parent = v.ancestor(p)?;
            let gps: Vec<String> = [g1, g2].into_iter().filter_map(|g| v.ancestor(g)).map(|a| uma_name(kb, a.card_id)).collect();
            Some(format!("{}{}", ancestor_line(kb, parent), if gps.is_empty() { String::new() } else { format!(" (its parents: {})", gps.join(", ")) }))
        })
        .collect();
    let g1 = kb.g1_wins(&v.win_saddle_id_array).len();
    let built = format!("{} rank ({}) · {} · {g1} G1 win{}", v.rank_label(), v.rank_score, scenario_name(kb, v.scenario_id), if g1 == 1 { "" } else { "s" });
    format!(
        "| {} #{} | {} | {} | {} | {} | {} | {} | {} |",
        cell(&uma_name(kb, v.card_id)),
        v.trained_chara_id,
        cell(&blue.join(", ")),
        cell(&pink.join(", ")),
        cell(&green.join(", ")),
        cell(&white.join(", ")),
        cell(&lineage.join("; ")),
        cell(&built),
        v.date()
    )
}

/// The finished uma an imported row is for: its name ends with `#<trained_chara_id>`.
pub fn row_id(name: &str) -> Option<u64> {
    name.trim().rsplit_once(" #").and_then(|(_, id)| id.parse().ok())
}

/// An imported row: its `#id` is a finished uma, imported now or before (a hand-typed
/// "Gold Ship #2" isn't one, so it stays).
fn imported(line: &str, ids: &std::collections::BTreeSet<u64>) -> bool {
    line.split('|').nth(1).and_then(row_id).is_some_and(|id| ids.contains(&id))
}

/// parents.md with the imported rows (those of `vets` and of `before`, the previous import)
/// replaced by `vets` (best rank first), and rows typed by hand kept, after them.
pub fn parents_md(kb: &Knowledge, text: &str, vets: &[Veteran], before: &[Veteran], today: &str) -> Result<String, String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text.lines().collect();
    let broken = || "account/parents.md has no parents table (| Uma | … with its separator line); restore it from app/templates/account/parents.md.".to_string();
    let start = lines.iter().position(|l| l.trim_start().starts_with("| Uma")).ok_or_else(broken)?;
    let end = lines[start..].iter().position(|l| !l.trim_start().starts_with('|')).map_or(lines.len(), |i| start + i);
    if start + 2 > end {
        return Err(broken());
    }
    let ids: std::collections::BTreeSet<u64> = vets.iter().chain(before).map(|v| v.trained_chara_id).collect();
    let manual: Vec<&str> = lines[start + 2..end].iter().copied().filter(|l| !imported(l, &ids)).collect();
    let mut sorted: Vec<&Veteran> = vets.iter().collect();
    sorted.sort_by(|a, b| b.rank_score.cmp(&a.rank_score).then(a.trained_chara_id.cmp(&b.trained_chara_id)));
    let mut out: Vec<String> = lines[..start + 2].iter().map(|l| l.to_string()).collect();
    out.extend(sorted.iter().map(|v| row(kb, v)));
    out.extend(manual.iter().map(|l| l.to_string()));
    out.extend(lines[end..].iter().map(|l| l.to_string()));
    let joined = out.join(newline) + newline;
    Ok(crate::data::set_meta(&joined, "Last updated", today))
}

/// The spark table out of uma.guide's data chunk (its `JSON.parse('…')` block of factors), as
/// `tools/update_knowledge.py` writes sparks.json.
pub fn sparks_from_uma_guide(chunk: &str) -> Result<Vec<crate::knowledge::SparkInfo>, String> {
    const KINDS: [&str; 7] = ["blue", "pink", "green", "white", "race", "scenario", "bonus"];
    for block in crate::affinity::js_json_blocks(chunk) {
        let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(&block) else { continue };
        let num = |r: &Value, k: &str| r.get(k).and_then(Value::as_u64);
        if rows.first().is_none_or(|r| ["id", "rarity", "grade", "type"].iter().any(|k| num(r, k).is_none()) || r.get("name").is_none()) {
            continue;
        }
        let mut out: Vec<crate::knowledge::SparkInfo> = rows
            .iter()
            .filter_map(|r| {
                let t = num(r, "type")? as usize;
                Some(crate::knowledge::SparkInfo { id: num(r, "id")? as u32, name: r.get("name")?.as_str()?.to_string(), kind: KINDS.get(t.wrapping_sub(1)).map_or_else(|| t.to_string(), |k| k.to_string()), stars: num(r, "rarity")? as u8 })
            })
            .collect();
        out.sort_by_key(|s| s.id);
        return Ok(out);
    }
    Err("no spark table in uma.guide's data; its format may have changed".into())
}

/// The win records (saddles) out of uma.guide's data chunk, where they're a plain object literal:
/// `={1:{races:[{id:100501,name:"Satsuki Sho",…},…],name:"Classic Triple Crown"},…}`. Race ids
/// there are race-instance ids; races.json's race id is that / 100.
pub fn saddles_from_uma_guide(chunk: &str) -> Result<std::collections::HashMap<u32, crate::knowledge::Saddle>, String> {
    let bad = || "no win table in uma.guide's data; its format may have changed".to_string();
    let start = chunk.find("={1:{races:[").ok_or_else(bad)? + 1;
    let mut depth = 0i32;
    let end = chunk[start..]
        .char_indices()
        .find_map(|(i, c)| {
            depth += i32::from(c == '{') - i32::from(c == '}');
            (depth == 0).then_some(start + i + 1)
        })
        .ok_or_else(bad)?;
    let mut rest = &chunk[start + 1..end - 1];
    let mut out = std::collections::HashMap::new();
    while let Some(head) = rest.find(":{races:[") {
        let id: u32 = rest[..head].trim_start_matches(',').trim().parse().map_err(|_| bad())?;
        rest = &rest[head + ":{races:[".len()..];
        let close = rest.find("],name:\"").ok_or_else(bad)?;
        let races: std::collections::BTreeSet<u32> = rest[..close].split("id:").skip(1).filter_map(|s| s.split(|c: char| !c.is_ascii_digit()).next()?.parse::<u32>().ok()).map(|r| r / 100).collect();
        rest = &rest[close + "],name:\"".len()..];
        let mut name = String::new();
        let mut chars = rest.char_indices();
        let mut after = rest.len();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some((_, 'u')) => {
                        let hex: String = chars.by_ref().take(4).map(|(_, h)| h).collect();
                        name.extend(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32));
                    }
                    Some((_, c)) => name.push(c),
                    None => {}
                },
                '"' => {
                    after = i + 1;
                    break;
                }
                c => name.push(c),
            }
        }
        rest = rest[after..].trim_start_matches('}');
        out.insert(id, crate::knowledge::Saddle { name, races: races.into_iter().collect() });
    }
    if out.is_empty() { Err(bad()) } else { Ok(out) }
}

impl App {
    /// Import finished umas from a data.json: keep them in account/veterans.json and rewrite the
    /// imported rows of account/parents.md. Returns what changed.
    pub(crate) fn import_veterans(&mut self, path: &Path) -> Result<String, String> {
        if std::env::var_os("UMALORD_PROFILE").is_some() {
            return Err("This roster is a profile shown without loading it (UMALORD_PROFILE), so it can't be edited.".into());
        }
        let root = self.root.clone().ok_or("umaLord folder not found.")?;
        let text = std::fs::read_to_string(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
        let records = parse(&text)?;
        let vets: Vec<Veteran> = records.iter().filter_map(|r| serde_json::from_value(r.clone()).ok()).collect();
        let old = load(&root);
        crate::profile::ensure_account(&root)?;
        let parents_path = root.join(PARENTS_FILE);
        let parents = std::fs::read_to_string(&parents_path).map_err(|e| e.to_string())?;
        let new_parents = parents_md(&self.kb, &parents, &vets, &old, &crate::ui_util::local_today())?;
        // The game's list is the truth (a released uma leaves it), but keep the last import, in
        // case this file was older or partial.
        if root.join(FILE).is_file() {
            let dir = root.join(crate::profile::DIR).join("backups");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            std::fs::copy(root.join(FILE), dir.join(format!("veterans-{}.json", crate::profile::stamp().replace([' ', ':'], "-")))).map_err(|e| e.to_string())?;
        }
        let pretty = serde_json::to_string_pretty(&Value::Array(records)).map_err(|e| e.to_string())?;
        std::fs::write(root.join(FILE), pretty + "\n").map_err(|e| e.to_string())?;
        std::fs::write(&parents_path, new_parents).map_err(|e| e.to_string())?;
        let before = old.len();
        *self.planner.parent_sparks.borrow_mut() = None;
        // The runs prove what's owned: add missing umas and raise stars or potential (undoable).
        let edits = roster_edits(&self.kb, &self.roster, &vets);
        let raised = self.apply_edits(&edits, true)?;
        self.finish_setup_edits();
        self.write_digest();
        let unknown = vets.iter().filter(|v| !self.kb.umas.contains_key(&v.card_id)).count();
        let mut msg = format!("Imported {} finished umas (there were {before}); account/parents.md lists them with their sparks.", vets.len());
        if raised > 0 {
            msg += &format!(" Your roster had {raised} uma{} missing or below the stars or potential they were trained at; they're updated (Ctrl+Z undoes it).", if raised == 1 { "" } else { "s" });
        }
        if unknown > 0 {
            msg += &format!(" {unknown} are outfits the game data doesn't know yet; update the game data to name them.");
        }
        if vets.len() < before {
            msg += &format!(" That's fewer than before: the last import is kept in {}/backups/.", crate::profile::DIR);
        }
        // The parent plan: confirm steps checked off by hand, then say if a new uma meets the step.
        if let Some(mut plan) = crate::parentage::track::load(&root).filter(|p| !p.paused) {
            let notes = plan.confirm(&self.kb, &vets, &crate::ui_util::local_today());
            if !notes.is_empty() {
                crate::parentage::track::save(&root, &plan)?;
                msg += &format!(" {}", notes.join(" "));
            }
            if let Some(note) = crate::parentage::text::import_note(&self.kb, &plan, &vets) {
                msg += &format!(" {note}");
            }
        }
        Ok(msg)
    }

    /// The finished umas imported so far (none while UMALORD_PROFILE shows another account).
    pub(crate) fn veterans(&self) -> Vec<Veteran> {
        if std::env::var_os("UMALORD_PROFILE").is_some() {
            return Vec::new();
        }
        self.root.as_deref().map(load).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb() -> Knowledge {
        Knowledge::load(crate::data::find_root(None).as_deref())
    }

    /// A finished uma as the game sends it (ids from real data; the account ids are made up).
    fn sample() -> &'static str {
        r#"[{"trained_chara_id": 7, "viewer_id": 111, "owner_viewer_id": 111, "card_id": 100601, "rank": 14, "rank_score": 12500,
            "scenario_id": 3, "create_time": "2025-09-16 02:50:09", "win_saddle_id_array": [1, 155],
            "factor_info_array": [{"factor_id": 103, "level": 0}, {"factor_id": 1102, "level": 0}, {"factor_id": 2000101, "level": 0}],
            "succession_chara_array": [
                {"position_id": 10, "card_id": 100701, "owner_viewer_id": 222, "factor_info_array": [{"factor_id": 203}], "win_saddle_id_array": [155]},
                {"position_id": 11, "card_id": 101301, "owner_viewer_id": 222, "factor_info_array": [], "win_saddle_id_array": []}]},
           {"trained_chara_id": 7, "card_id": 100601}]"#
    }

    #[test]
    fn parse_scrubs_ids_and_dedupes() {
        let recs = parse(sample()).unwrap();
        assert_eq!(recs.len(), 1, "the same uma twice is kept once");
        let text = serde_json::to_string(&recs).unwrap();
        assert!(!text.contains("viewer_id"), "no account ids, including other players' in the lineage");
        assert!(parse("[]").is_err() && parse("{}").is_err() && parse("nope").is_err());
        assert!(parse(r#"[{"card_id": 1001, "trained_chara_id": 1}]"#).is_err(), "ids alone aren't a finished uma");
        let wrapped = format!(r#"{{"trained_chara_array": {}}}"#, sample());
        assert_eq!(parse(&wrapped).unwrap().len(), 1);
    }

    #[test]
    fn decodes_sparks_wins_and_lineage() {
        let kb = kb();
        let v: Veteran = serde_json::from_value(parse(sample()).unwrap().remove(0)).unwrap();
        assert_eq!(v.rank_label(), "A+");
        assert_eq!(v.date(), "2025-09-16");
        // Saddle 1 is the Classic Triple Crown, 155 Satsuki Sho at another venue: one name.
        let wins = kb.g1_wins(&v.win_saddle_id_array);
        assert!(wins.contains("Satsuki Sho") && wins.contains("Kikuka Sho"), "{wins:?}");
        assert_eq!(wins.len(), 3);
        let r = row(&kb, &v);
        assert!(r.starts_with("| Oguri Cap [Starlight Beat] #7 | Speed 3★ | Turf 2★ |"), "{r}");
        assert!(r.contains("Right-Handed ○ 1★") && r.contains("Gold Ship [") && r.contains("its parents: Mejiro McQueen ["), "{r}");
        assert!(r.contains("A+ rank (12500) · Grand Concert · 3 G1 wins"), "{r}");
        let l = v.legacy(&kb);
        assert_eq!(l.parents[0].as_ref().map(|a| a.wins.len()), Some(1));
        assert!(l.parents[1].is_none(), "position 20 is empty");
    }

    /// The chunk parsers read uma.guide's spark block and its win table literal.
    #[test]
    fn reads_uma_guide_tables() {
        let chunk = r#"a=JSON.parse('[{"id":101,"name":"Speed","description":"x","rarity":1,"grade":1,"type":1},{"id":1102,"name":"Turf","description":"y","rarity":2,"grade":1,"type":2}]'),ze={1:{races:[{id:100501,name:"Satsuki Sho",distance:2e3,ground:"TURF"},{id:101001,name:"Japanese Derby",distance:2400,ground:"TURF"}],name:"Classic Triple Crown"},155:{races:[{id:102801,name:"Satsuki Sho",distance:2e3,ground:"TURF"}],name:"Satsuki \"Sho\""}},q=1"#;
        let s = sparks_from_uma_guide(chunk).unwrap();
        assert_eq!((s[1].name.as_str(), s[1].kind.as_str(), s[1].stars), ("Turf", "pink", 2));
        let w = saddles_from_uma_guide(chunk).unwrap();
        assert_eq!(w[&1].races, vec![1005, 1010]);
        assert_eq!(w[&155].name, "Satsuki \"Sho\"");
        assert_eq!(w[&155].races, vec![1028]);
        assert!(sparks_from_uma_guide("x").is_err() && saddles_from_uma_guide("x").is_err());
    }

    /// Finished runs add missing umas and raise stars and potential, never lower them.
    #[test]
    fn runs_raise_the_roster() {
        let kb = kb();
        let roster = crate::data::Roster::sample();
        let owned = roster.umas.iter().find(|u| u.potential < 5).unwrap().clone();
        let run = |id, rarity, talent_level| Veteran { card_id: id, rarity, talent_level, ..Default::default() };
        let missing = kb.umas.keys().copied().find(|id| !roster.umas.iter().any(|u| u.id == *id)).unwrap();
        let vets = [run(owned.id, owned.stars, 5), run(owned.id, 1, 1), run(missing, 3, 2)];
        let edits = roster_edits(&kb, &roster, &vets);
        use crate::onboarding::Edit;
        assert!(edits.contains(&Edit::Uma { id: owned.id, stars: owned.stars, potential: 5 }), "{edits:?}");
        assert!(edits.contains(&Edit::Uma { id: missing, stars: 3, potential: 2 }));
        assert_eq!(edits.len(), 2);
        assert!(roster_edits(&kb, &roster, &[run(owned.id, 1, 1)]).is_empty(), "never lowers");
    }

    #[test]
    fn rewrites_imported_rows_and_keeps_hand_typed_ones() {
        let kb = kb();
        let template = include_str!("../templates/account/parents.md");
        let manual = "| My old parent | Speed 3★ | | | | | CM 20 | 2026-01-01 |";
        let text = format!("{}{manual}\n", template);
        let v: Veteran = serde_json::from_value(parse(sample()).unwrap().remove(0)).unwrap();
        let hand_hash = "| Gold Ship #2 | Speed 3★ | | | | | | 2026-01-01 |";
        let text = format!("{text}{hand_hash}\n");
        let once = parents_md(&kb, &text, std::slice::from_ref(&v), &[], "2026-10-27").unwrap();
        let twice = parents_md(&kb, &once, std::slice::from_ref(&v), std::slice::from_ref(&v), "2026-10-27").unwrap();
        assert_eq!(once, twice, "importing again replaces, never duplicates");
        assert!(once.contains(manual) && once.contains(hand_hash), "hand rows stay, even one ending in #2");
        assert!(once.contains("#7 |") && once.contains("- Last updated: 2026-10-27"));
        // A uma released from the game drops out on the next import.
        let gone = parents_md(&kb, &once, &[], std::slice::from_ref(&v), "2026-10-27").unwrap();
        assert!(!gone.contains("#7 |") && gone.contains(manual));
        let crlf = parents_md(&kb, &text.replace('\n', "\r\n"), std::slice::from_ref(&v), &[], "2026-10-27").unwrap();
        assert!(crlf.contains("#7 |") && !crlf.replace("\r\n", "").contains('\n'), "CRLF stays CRLF");
        assert!(parents_md(&kb, "# no table\n| Uma | Blue |\n", &[], &[], "x").is_err(), "a broken table is an error, not a panic");
        let table = crate::data::tables(&once).into_iter().next().unwrap();
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[0].len(), table.headers.len());
    }
}
