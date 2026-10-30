//! Affinity: how well a trainee matches her legacies, which multiplies every spark's chance to
//! pass on at the two inspirations (`× (1 + affinity / 100)`, `knowledge/mechanics.md`).
//!
//! The game files each character into relation groups (style, distance, surface, dorm, classmates,
//! the real horses' history…), each worth some points: `knowledge/data/affinity.json`, the game's
//! `succession_relation` and `succession_relation_member` tables as uma.guide ships them. Two
//! characters' base affinity is the points of the groups they share; three characters' is the
//! points of the groups all three share. On Global, every G1 both members of a link won adds 3
//! (G2/G3 stopped counting with the July 2026 patch). The sums follow uma.guide's Roster Viewer
//! and umalab's `affinity.py` (github.com/iurker1758/umalab, whose rules were checked against the
//! Global client), both read 2026-10-27; the pair values match GachaData's published table.
//!
//! Each parent's and grandparent's sparks roll with that slot's own affinity: a parent's covers its
//! link with the trainee, the trios through its parents, its link with the other parent and the
//! G1s it shares with each; a grandparent's is its trio plus the G1s it shares with its parent. The
//! parents' link counts in both, so the slots don't add up to the lineage total.
//!
//! Characters are chara ids: an uma's id / 100 (all outfits of one character share it).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Points per G1 that both members of a link won (Global, since 2026-07-01).
pub const RACE_POINTS: u32 = 3;

/// The game's affinity tables.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AffinityTable {
    #[serde(default)]
    pub source: String,
    /// When the tables were read (UTC, ISO 8601).
    #[serde(default)]
    pub fetched: String,
    /// Points per relation group.
    pub points: BTreeMap<u32, u32>,
    /// Each character's relation groups.
    pub members: BTreeMap<u32, Vec<u32>>,
}

/// The character an uma (outfit) id belongs to.
pub fn chara(uma_id: u32) -> u32 {
    if uma_id >= 100_000 { uma_id / 100 } else { uma_id }
}

/// ◎ above 150, ○ from 51, △ below: the symbols the game shows. Affinity counts linearly, so the
/// symbol is only a summary.
pub fn symbol(points: u32) -> &'static str {
    if points > 150 {
        "◎"
    } else if points >= 51 {
        "○"
    } else {
        "△"
    }
}

/// One legacy (a parent) as far as affinity cares: who it is, the G1s it won, and its own parents.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Legacy {
    /// Uma or chara id.
    pub uma: u32,
    /// Names of the G1 races it won.
    pub wins: BTreeSet<String>,
    pub parents: [Option<Ancestor>; 2],
}

/// A legacy's own parent (the trainee's grandparent).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ancestor {
    pub uma: u32,
    pub wins: BTreeSet<String>,
}

/// One legacy's affinity, in parts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegacyAffinity {
    /// Trainee and legacy.
    pub direct: u32,
    /// Trainee, legacy and each of its parents.
    pub with_grandparent: [u32; 2],
    /// G1s the legacy shares with each of its parents, × 3.
    pub grandparent_races: [u32; 2],
    /// The two legacies with each other.
    pub with_other: u32,
    /// G1s the two legacies share, × 3.
    pub other_races: u32,
}

impl LegacyAffinity {
    /// The legacy's affinity: what its own sparks roll with, and the symbol shown beside it.
    pub fn total(&self) -> u32 {
        self.direct + self.with_grandparent.iter().sum::<u32>() + self.grandparent_races.iter().sum::<u32>() + self.with_other + self.other_races
    }

    /// Each grandparent's affinity, which its sparks roll with.
    pub fn grandparent(&self, i: usize) -> u32 {
        self.with_grandparent[i] + self.grandparent_races[i]
    }
}

/// Affinity for a trainee and up to two legacies.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineageAffinity {
    pub legacies: [Option<LegacyAffinity>; 2],
}

impl LineageAffinity {
    /// The whole lineage: both legacies and their parents, with the two legacies' link counted once.
    pub fn total(&self) -> u32 {
        let mut t = 0;
        let mut pair = 0;
        for l in self.legacies.iter().flatten() {
            t += l.total() - l.with_other - l.other_races;
            pair = l.with_other + l.other_races;
        }
        t + pair
    }
}

/// G1s in both lists, as points.
fn shared_races(a: &BTreeSet<String>, b: &BTreeSet<String>) -> u32 {
    a.intersection(b).count() as u32 * RACE_POINTS
}

impl AffinityTable {
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    fn groups(&self, uma: u32) -> &[u32] {
        self.members.get(&chara(uma)).map_or(&[], Vec::as_slice)
    }

    /// Points of the groups every one of `umas` belongs to; 0 if two of them are the same character.
    pub fn shared(&self, umas: &[u32]) -> u32 {
        let charas: Vec<u32> = umas.iter().map(|u| chara(*u)).collect();
        if charas.iter().enumerate().any(|(i, c)| charas[..i].contains(c)) {
            return 0;
        }
        let Some((first, rest)) = umas.split_first() else { return 0 };
        self.groups(*first).iter().filter(|g| rest.iter().all(|u| self.groups(*u).contains(g))).map(|g| self.points.get(g).copied().unwrap_or(0)).sum()
    }

    /// Base affinity between two umas.
    pub fn pair(&self, a: u32, b: u32) -> u32 {
        self.shared(&[a, b])
    }

    /// Whether the table knows this uma (a character released after it was read won't be there).
    pub fn knows(&self, uma: u32) -> bool {
        self.members.contains_key(&chara(uma))
    }

    /// One legacy's affinity with the trainee, given the other legacy (if any).
    /// A slot holding the trainee's own character (or its parent's) adds no relation points, but
    /// its shared G1s still count (umalab, checked in the Global client 2026-07-30).
    pub fn legacy(&self, trainee: u32, legacy: &Legacy, other: Option<&Legacy>) -> LegacyAffinity {
        let mut a = LegacyAffinity { direct: self.pair(trainee, legacy.uma), ..Default::default() };
        for (i, gp) in legacy.parents.iter().enumerate() {
            if let Some(gp) = gp {
                a.with_grandparent[i] = self.shared(&[trainee, legacy.uma, gp.uma]);
                a.grandparent_races[i] = shared_races(&legacy.wins, &gp.wins);
            }
        }
        if let Some(o) = other {
            a.with_other = self.pair(legacy.uma, o.uma);
            a.other_races = shared_races(&legacy.wins, &o.wins);
        }
        a
    }

    /// Affinity for a trainee and her legacies.
    pub fn lineage(&self, trainee: u32, legacies: &[Option<Legacy>; 2]) -> LineageAffinity {
        let [a, b] = legacies;
        LineageAffinity {
            legacies: [
                a.as_ref().map(|l| self.legacy(trainee, l, b.as_ref())),
                b.as_ref().map(|l| self.legacy(trainee, l, a.as_ref())),
            ],
        }
    }

    /// The characters with the highest base affinity to `trainee`, best first.
    pub fn best_partners(&self, trainee: u32) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = self.members.keys().filter(|c| **c != chara(trainee)).map(|c| (*c, self.pair(trainee, *c))).collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out
    }

    /// Read the tables out of uma.guide's data chunk (its `JSON.parse('…')` blocks), as
    /// `tools/update_knowledge.py` does, without running any of the site's code.
    pub fn from_uma_guide(chunk: &str, fetched: &str) -> Result<AffinityTable, String> {
        let mut t = AffinityTable { source: "uma.guide's data (the game's succession_relation and succession_relation_member tables)".into(), fetched: fetched.into(), ..Default::default() };
        for block in js_json_blocks(chunk) {
            let Ok(serde_json::Value::Array(rows)) = serde_json::from_str::<serde_json::Value>(&block) else { continue };
            let Some(first) = rows.first() else { continue };
            let num = |r: &serde_json::Value, k: &str| r.get(k).and_then(serde_json::Value::as_u64).map(|n| n as u32);
            // The first block of each kind, as the updater's `pick` takes.
            if t.points.is_empty() && num(first, "relationPoint").is_some() && num(first, "relationType").is_some() {
                t.points = rows.iter().filter_map(|r| Some((num(r, "relationType")?, num(r, "relationPoint")?))).collect();
            } else if t.members.is_empty() && num(first, "charaId").is_some() && num(first, "relationType").is_some() {
                let mut m: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
                for r in &rows {
                    if let (Some(c), Some(g)) = (num(r, "charaId"), num(r, "relationType")) {
                        m.entry(c).or_default().push(g);
                    }
                }
                for groups in m.values_mut() {
                    groups.sort_unstable();
                    groups.dedup();
                }
                t.members = m;
            }
        }
        if t.points.is_empty() || t.members.is_empty() {
            return Err("no affinity tables in uma.guide's data; its format may have changed".into());
        }
        Ok(t)
    }
}

/// The text of every `JSON.parse('…')` or `` JSON.parse(`…`) `` in a script, unescaped.
pub(crate) fn js_json_blocks(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = src;
    // Single-quoted strings (the affinity tables) and template literals (the spark table) escape alike.
    while let Some(i) = rest.find("JSON.parse(") {
        rest = &rest[i + "JSON.parse(".len()..];
        let Some(quote) = rest.chars().next().filter(|c| matches!(c, '\'' | '`')) else { continue };
        rest = &rest[1..];
        let mut text = String::new();
        let mut chars = rest.char_indices();
        let mut end = None;
        while let Some((j, c)) = chars.next() {
            match c {
                c if c == quote => {
                    end = Some(j);
                    break;
                }
                // JavaScript's escapes, as the updater's `unescape_template` reads them.
                '\\' => match chars.next() {
                    Some((_, 'n')) => text.push('\n'),
                    Some((_, 't')) => text.push('\t'),
                    Some((_, 'r')) => text.push('\r'),
                    Some((_, 'b')) => text.push('\u{8}'),
                    Some((_, 'f')) => text.push('\u{c}'),
                    Some((_, 'v')) => text.push('\u{b}'),
                    Some((_, '0')) => text.push('\0'),
                    Some((_, '\n')) => {}
                    Some((_, c @ ('u' | 'x'))) => {
                        let hex: String = if c == 'x' {
                            (0..2).filter_map(|_| chars.next().map(|(_, h)| h)).collect()
                        } else if chars.clone().next().is_some_and(|(_, h)| h == '{') {
                            chars.next();
                            chars.by_ref().map(|(_, h)| h).take_while(|h| *h != '}').collect()
                        } else {
                            (0..4).filter_map(|_| chars.next().map(|(_, h)| h)).collect()
                        };
                        if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            text.push(ch);
                        }
                    }
                    Some((_, other)) => text.push(other),
                    None => break,
                },
                c => text.push(c),
            }
        }
        let Some(end) = end else { break };
        out.push(text);
        rest = &rest[end..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> AffinityTable {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../knowledge/data/affinity.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    // Chara ids: Special Week 1001, Silence Suzuka 1002, Tokai Teio 1003, Gold Ship 1007,
    // Mejiro McQueen 1013, Admire Vega 1033, Narita Taishin 1050, Haru Urara 1052.

    /// Pair values from the game's tables, checked against GachaData's published table
    /// (gacha-data.com/umamusume/affinity: Admire Vega with Narita Taishin 32, with Gold Ship 29).
    #[test]
    fn pairs_match_published_values() {
        let t = table();
        assert_eq!(t.pair(1033, 1050), 32);
        assert_eq!(t.pair(1033, 1007), 29);
        assert_eq!(t.pair(1001, 1002), t.pair(1002, 1001), "symmetric");
        assert_eq!(t.pair(100101, 100102), 0, "two outfits of one character");
        assert_eq!(t.pair(100101, 1002), t.pair(1001, 1002), "outfit ids map to the character");
        assert!(t.pair(1052, 1001) < t.pair(1002, 1001), "Haru Urara, a dirt sprinter, matches Special Week less than Suzuka does");
    }

    /// A trio scores at most what its weakest pair does, and a repeated character scores nothing.
    #[test]
    fn trios_are_bounded_by_pairs() {
        let t = table();
        let umas: Vec<u32> = t.members.keys().copied().take(12).collect();
        for &a in &umas {
            for &b in &umas {
                for &c in &umas {
                    let trio = t.shared(&[a, b, c]);
                    if a == b || b == c || a == c {
                        assert_eq!(trio, 0);
                    } else {
                        assert!(trio <= t.pair(a, b).min(t.pair(a, c)).min(t.pair(b, c)));
                    }
                }
            }
        }
    }

    #[test]
    fn lineage_sums_and_race_bonus() {
        let t = table();
        let wins = |r: &[&str]| r.iter().map(|s| s.to_string()).collect::<BTreeSet<String>>();
        let gp = |uma, r: &[&str]| Some(Ancestor { uma, wins: wins(r) });
        let a = Legacy { uma: 1002, wins: wins(&["Arima Kinen", "Japan Cup"]), parents: [gp(1013, &["Arima Kinen"]), gp(1003, &[])] };
        let b = Legacy { uma: 1007, wins: wins(&["Japan Cup", "Arima Kinen", "Satsuki Sho"]), parents: [None, None] };
        let l = t.lineage(1001, &[Some(a.clone()), Some(b.clone())]);
        let la = l.legacies[0].clone().unwrap();
        assert_eq!(la.direct, t.pair(1001, 1002));
        assert_eq!(la.with_grandparent, [t.shared(&[1001, 1002, 1013]), t.shared(&[1001, 1002, 1003])]);
        assert_eq!(la.grandparent_races, [3, 0], "one shared G1 with McQueen");
        assert_eq!(la.other_races, 6, "two G1s shared with the other legacy");
        assert_eq!(la.grandparent(0), la.with_grandparent[0] + 3);
        let lb = l.legacies[1].clone().unwrap();
        assert_eq!(lb.with_other, la.with_other);
        // The lineage counts the legacies' link once.
        assert_eq!(l.total(), la.total() + lb.total() - lb.with_other - lb.other_races);
        // A legacy of the trainee's own character adds no relation points, but its shared G1s count.
        let same = Legacy { uma: 100102, wins: wins(&["Arima Kinen"]), parents: [gp(1013, &["Arima Kinen"]), None] };
        let s = t.legacy(100101, &same, None);
        assert_eq!((s.direct, s.with_grandparent), (0, [0, 0]));
        assert_eq!(s.grandparent_races, [3, 0]);
        assert_eq!(symbol(151), "◎");
        assert_eq!(symbol(150), "○");
        assert_eq!(symbol(51), "○");
        assert_eq!(symbol(50), "△");
    }

    #[test]
    fn best_partners_rank_by_pair() {
        let t = table();
        let best = t.best_partners(100101);
        assert!(best.windows(2).all(|w| w[0].1 >= w[1].1));
        assert!(!best.iter().any(|(c, _)| *c == 1001));
        assert_eq!(best[0].1, t.pair(1001, best[0].0));
    }

    /// The chunk parser reads single-quoted JSON.parse blocks, escapes included, like the updater.
    #[test]
    fn reads_uma_guide_chunk() {
        let chunk = r#"x=1,Qe=JSON.parse('[{"relationType":101,"relationPoint":2},{"relationType":508,"relationPoint":7}]'),Ze=JSON.parse('[{"id":1,"relationType":101,"charaId":1001},{"id":2,"relationType":508,"charaId":1001},{"id":3,"relationType":508,"charaId":1002}]'),y=JSON.parse('{"a":"it\'s \u00e9 \x41\u{1F40E}"}'),Q2=JSON.parse('[{"relationType":101,"relationPoint":99}]');"#;
        let t = AffinityTable::from_uma_guide(chunk, "2026-10-27T00:00:00Z").unwrap();
        assert_eq!(t.points.get(&508), Some(&7));
        assert_eq!(t.members.get(&1001), Some(&vec![101, 508]));
        assert_eq!(t.pair(1001, 1002), 7);
        assert_eq!(js_json_blocks(chunk)[2], "{\"a\":\"it's \u{e9} A\u{1F40E}\"}");
        assert_eq!(t.points.get(&101), Some(&2), "the first block wins, as in the updater");
        assert!(AffinityTable::from_uma_guide("nothing here", "").is_err());
    }
}
