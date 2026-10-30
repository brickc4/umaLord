//! The parent planner: which two finished umas make the best lineage for a runner's goal, what
//! that lineage still lacks, and a staged queue of parent careers that closes the gaps. Each step
//! has must-haves (the player keeps running it until a finished uma has them) and nice-to-haves.
//!
//! `score` is exact: it scores every pair of finished umas with the game's start rules and
//! affinity (`lineage.rs`, `affinity.rs`). `jobs` plans parent careers from closed-form spark odds
//! (`outcome`), and never asks for a step expected to take more than `jobs::CAP` careers. `track`
//! keeps the plan in plans/parents.json and checks each import against its current step. The
//! design and its open questions are in `knowledge/parenting.md` (The parent planner).

// Nothing outside the tests calls it until the Lineage page does: drop this then.
#![allow(dead_code)]

pub mod baseline;
pub mod brief;
pub mod ceiling;
pub mod jobs;
pub mod modes;
pub mod outcome;
pub mod progress;
pub mod score;
pub mod saved;
pub mod text;
pub mod track;
pub mod tt;
pub mod team;
pub mod verify;

use crate::affinity::chara;
use crate::knowledge::{Aptitude, Knowledge, grade_rank};
use crate::veterans::Veteran;
use serde::{Deserialize, Serialize};

/// The ten aptitudes, in the order the planner indexes them.
pub const APTITUDES: [&str; 10] = ["Turf", "Dirt", "Sprint", "Mile", "Medium", "Long", "Front Runner", "Pace Chaser", "Late Surger", "End Closer"];
pub const STATS: [&str; 5] = ["Speed", "Stamina", "Power", "Guts", "Wit"];

/// An uma's ten aptitude grades as ranks (G 1 … S 8), in `APTITUDES` order.
pub fn ranks(a: &Aptitude) -> [u8; 10] {
    [&a.turf, &a.dirt, &a.sprint, &a.mile, &a.medium, &a.long, &a.front, &a.pace, &a.late, &a.end].map(|g| grade_rank(g))
}

/// Pink spark keys (spark id / 100) in `APTITUDES` order.
const PINK_KEYS: [u32; 10] = [11, 12, 31, 32, 33, 34, 21, 22, 23, 24];

/// The aptitude a pink spark raises, from its key.
fn pink_index(key: u32) -> Option<usize> {
    PINK_KEYS.iter().position(|k| *k == key)
}

/// The spark id of a pink on aptitude `i` at `stars`.
pub fn pink_spark(i: usize, stars: u8) -> u32 {
    PINK_KEYS[i] * 100 + u32::from(stars)
}

/// The spark id of an uma's green (her unique) at `stars`.
pub fn green_spark(uma: u32, stars: u8) -> u32 {
    uma * 100 + u32::from(stars)
}

/// The grade rank after the start's pink stars (`lineage::after_start`).
pub fn after_start(rank: u8, stars: u32) -> u8 {
    if rank >= 7 || stars == 0 {
        return rank.max(1);
    }
    (rank.max(1) + stars.div_ceil(3).min(4) as u8).min(7)
}

/// Pink stars needed in the lineage for an aptitude to start at A, or None when it's more than
/// four grades below.
pub fn stars_for_a(rank: u8) -> Option<u32> {
    let grades = 7u8.saturating_sub(rank.max(1));
    (grades <= 4).then(|| if grades == 0 { 0 } else { 3 * u32::from(grades) - 2 })
}

/// What a set of sparks adds up to: pink stars per aptitude, start stats per blue, its green.
fn tally(kb: &Knowledge, ids: &[u32]) -> ([u8; 10], [u32; 5], Option<(u32, u8)>) {
    let (mut pink, mut blue, mut green) = ([0u8; 10], [0u32; 5], None);
    for s in ids.iter().filter_map(|id| kb.sparks.get(id)) {
        let key = s.id / 100;
        match s.kind.as_str() {
            "pink" => {
                if let Some(i) = pink_index(key) {
                    pink[i] += s.stars;
                }
            }
            "blue" if (1..=5).contains(&key) => blue[key as usize - 1] += crate::lineage::blue_start(s.stars),
            "green" => green = Some((key, s.stars)),
            _ => {}
        }
    }
    (pink, blue, green)
}

/// One finished uma as the planner sees it: its own sparks, its two parents' (which become the
/// runner's grandparents), and what they add up to.
#[derive(Clone, Debug, Default)]
pub struct Parent {
    /// The game's trained_chara_id; 0 for a parent career not run yet.
    pub id: u64,
    pub uma: u32,
    pub own: Vec<u32>,
    /// Its parents: uma and sparks.
    pub parents: [Option<(u32, Vec<u32>)>; 2],
    pub legacy: crate::affinity::Legacy,
    /// Its own pink stars per aptitude.
    pub pink_own: [u8; 10],
    /// Pink stars per aptitude over it and its parents: what it brings to a lineage.
    pub pink_line: [u8; 10],
    /// Stats its line's blue sparks add at the start.
    pub blue_line: [u32; 5],
    /// Its own green: the uma whose unique it is, and stars.
    pub green: Option<(u32, u8)>,
}

impl Parent {
    pub fn from_veteran(kb: &Knowledge, v: &Veteran) -> Self {
        let parents = [10, 20].map(|pos| v.ancestor(pos).map(|a| (a.card_id, a.factors())));
        Self::build(kb, v.trained_chara_id, v.card_id, v.factors(), parents, v.legacy(kb))
    }

    /// The finished uma a parent career of `trainee` with legacies `a` and `b` would leave, with
    /// the sparks `own` (what the step asks for).
    pub fn planned(kb: &Knowledge, trainee: u32, own: Vec<u32>, a: &Parent, b: &Parent) -> Self {
        let gp = |p: &Parent| crate::affinity::Ancestor { uma: p.uma, wins: p.legacy.wins.clone() };
        let legacy = crate::affinity::Legacy { uma: trainee, wins: Default::default(), parents: [Some(gp(a)), Some(gp(b))] };
        Self::build(kb, 0, trainee, own, [Some((a.uma, a.own.clone())), Some((b.uma, b.own.clone()))], legacy)
    }

    fn build(kb: &Knowledge, id: u64, uma: u32, own: Vec<u32>, parents: [Option<(u32, Vec<u32>)>; 2], legacy: crate::affinity::Legacy) -> Self {
        let (pink_own, mut blue_line, green) = tally(kb, &own);
        let mut pink_line = pink_own;
        for (_, ids) in parents.iter().flatten() {
            let (p, b, _) = tally(kb, ids);
            (0..10).for_each(|i| pink_line[i] += p[i]);
            (0..5).for_each(|i| blue_line[i] += b[i]);
        }
        Parent { id, uma, own, parents, legacy, pink_own, pink_line, blue_line, green }
    }

    pub fn chara(&self) -> u32 {
        chara(self.uma)
    }
}

/// What the runner needs from her lineage.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Goal {
    /// The uma (outfit) to train for the goal.
    pub runner: u32,
    /// Aptitudes (`APTITUDES` indices) that must start at A: the course's surface and distance
    /// and her style.
    pub targets: Vec<usize>,
    /// An inherited unique she needs: the uma (outfit) whose green spark it is.
    pub green: Option<u32>,
    /// Stats the lineage's blue sparks should add at the start. Wanted, never required.
    pub stats: [u32; 5],
    /// White sparks (keys: spark id / 100) worth inheriting. Wanted, never required.
    pub whites: Vec<u32>,
    /// What it's for, e.g. "CM 22 (Aquarius Cup), Late Surger".
    pub label: String,
}

impl Goal {
    /// Pink stars each target needs in the lineage, or the targets more than four grades below A.
    pub fn need(&self, kb: &Knowledge) -> Result<Vec<(usize, u32)>, Vec<usize>> {
        let r = kb.umas.get(&self.runner).map(|u| ranks(&u.aptitude)).unwrap_or([0; 10]);
        let mut out = Vec::new();
        let mut bad = Vec::new();
        for &i in &self.targets {
            match stars_for_a(r[i]) {
                Some(n) => out.push((i, n)),
                None => bad.push(i),
            }
        }
        if bad.is_empty() { Ok(out) } else { Err(bad) }
    }

    /// Whether a legacy may sit in the runner's lineage: never her own character.
    pub fn allows(&self, p: &Parent) -> bool {
        p.chara() != chara(self.runner)
    }
}

/// The account as the planner needs it: its finished umas, the umas it can train (id, stars),
/// and what its past careers say about how parents finish.
#[derive(Clone)]
pub struct Inventory {
    pub parents: Vec<Parent>,
    pub owned: Vec<(u32, u8)>,
    pub history: outcome::History,
}

impl Inventory {
    pub fn new(kb: &Knowledge, vets: &[Veteran], owned: Vec<(u32, u8)>) -> Self {
        Inventory { parents: vets.iter().map(|v| Parent::from_veteran(kb, v)).collect(), owned, history: outcome::History::from(vets) }
    }

    pub fn by_id(&self, id: u64) -> Option<&Parent> {
        self.parents.iter().find(|p| p.id == id)
    }

    /// Add a friend's legacy the player can borrow today: her outfit, sparks and her two
    /// parents' (outfit and sparks). It joins the parents with a borrowed id; a lineage takes at
    /// most one borrowed legacy.
    pub fn borrow(&mut self, kb: &Knowledge, b: &Borrow) -> u64 {
        let id = BORROWED + self.parents.iter().filter(|p| is_borrowed(p.id)).count() as u64;
        let gp = |(u, _): &(u32, Vec<u32>)| crate::affinity::Ancestor { uma: *u, wins: Default::default() };
        let legacy = crate::affinity::Legacy { uma: b.uma, wins: Default::default(), parents: [Some(gp(&b.parents[0])), Some(gp(&b.parents[1]))] };
        let [x, y] = b.parents.clone();
        self.parents.push(Parent::build(kb, id, b.uma, b.own.clone(), [Some(x), Some(y)], legacy));
        id
    }
}

/// A friend's legacy the player can borrow: her outfit, her sparks and her two parents'.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Borrow {
    pub uma: u32,
    pub own: Vec<u32>,
    pub parents: [(u32, Vec<u32>); 2],
}

/// Ids for borrowed legacies (a friend's), well clear of the game's trained_chara_ids.
const BORROWED: u64 = u64::MAX - 2000;

/// Whether a parent is a friend's, borrowed for the day.
pub fn is_borrowed(id: u64) -> bool {
    (BORROWED..BORROWED + 1000).contains(&id)
}

/// "Oguri Cap [Starlight Beat]", or the id when the game data doesn't know it.
pub fn uma_name(kb: &Knowledge, uma: u32) -> String {
    kb.umas.get(&uma).map_or_else(|| format!("uma {uma}"), |u| format!("{} [{}]", u.name, u.outfit))
}

/// Umas by character name, with the outfit only where two of the list share a character:
/// "Taiki Shuttle, Curren Chan [Fille Éclair], Curren Chan [Ma Chérie of the New Moon]".
pub fn short_names(kb: &Knowledge, umas: &[u32]) -> Vec<String> {
    umas.iter()
        .map(|u| {
            let twin = umas.iter().any(|o| o != u && crate::affinity::chara(*o) == crate::affinity::chara(*u));
            match kb.umas.get(u) {
                Some(info) if !twin => info.name.clone(),
                _ => uma_name(kb, *u),
            }
        })
        .collect()
}

/// One uma by character name, for a sentence that names her alone.
pub fn short_name(kb: &Knowledge, uma: u32) -> String {
    kb.umas.get(&uma).map_or_else(|| uma_name(kb, uma), |u| u.name.clone())
}

/// "#812 Haru Urara [Bestest Prize ♪]", or a planned parent's trainee.
pub fn parent_name(kb: &Knowledge, p: &Parent) -> String {
    if p.id == 0 {
        format!("a new {}", uma_name(kb, p.uma))
    } else if is_borrowed(p.id) {
        format!("a friend's {} (borrowed)", uma_name(kb, p.uma))
    } else {
        format!("#{} {}", p.id, uma_name(kb, p.uma))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn kb() -> Knowledge {
        Knowledge::load(crate::data::find_root(None).as_deref())
    }

    /// A spark id by kind, name and stars.
    pub fn spark(kb: &Knowledge, kind: &str, name: &str, stars: u8) -> u32 {
        kb.sparks.values().find(|s| s.kind == kind && s.name == name && s.stars == stars).unwrap_or_else(|| panic!("{kind} {name} {stars}")).id
    }

    /// A made-up finished uma: id, outfit, own sparks, and its parents' outfits and sparks.
    pub fn parent(kb: &Knowledge, id: u64, uma: u32, own: Vec<u32>, parents: [(u32, Vec<u32>); 2]) -> Parent {
        let gp = |(u, _): &(u32, Vec<u32>)| crate::affinity::Ancestor { uma: *u, wins: Default::default() };
        let legacy = crate::affinity::Legacy { uma, wins: Default::default(), parents: [Some(gp(&parents[0])), Some(gp(&parents[1]))] };
        let [a, b] = parents;
        Parent::build(kb, id, uma, own, [Some(a), Some(b)], legacy)
    }

    #[test]
    fn start_rules_match_lineage() {
        for rank in 1..=8u8 {
            for stars in 0..14 {
                let g = ["G", "F", "E", "D", "C", "B", "A", "S"][rank as usize - 1];
                assert_eq!(after_start(rank, stars), grade_rank(crate::lineage::after_start(g, stars)), "{g} {stars}");
            }
        }
        assert_eq!(stars_for_a(6), Some(1), "B needs one star");
        assert_eq!(stars_for_a(5), Some(4));
        assert_eq!(stars_for_a(3), Some(10));
        assert_eq!(stars_for_a(2), None, "F is five grades below A");
        assert_eq!(stars_for_a(7), Some(0));
    }

    /// Every outfit's unique in umas.json is the one its green spark (the game's own table) names.
    #[test]
    fn uniques_match_the_spark_table() {
        let kb = kb();
        for u in kb.umas.values() {
            let Some(green) = kb.sparks.get(&green_spark(u.id, 1)) else { continue };
            let unique = u.unique_skill.and_then(|s| kb.skills.get(&s)).map(|s| s.name.as_str()).unwrap_or_default();
            assert!(unique.eq_ignore_ascii_case(&green.name), "{} [{}]: umas.json has {unique}, its spark is {}", u.name, u.outfit, green.name);
        }
    }

    #[test]
    fn tallies_a_line() {
        let kb = kb();
        let p = parent(&kb, 1, 105201, vec![spark(&kb, "pink", "Dirt", 2), spark(&kb, "blue", "Stamina", 3)], [(100101, vec![spark(&kb, "pink", "Dirt", 1)]), (100201, vec![spark(&kb, "blue", "Stamina", 1)])]);
        assert_eq!((p.pink_own[1], p.pink_line[1]), (2, 3));
        assert_eq!(p.blue_line[1], 26);
        assert_eq!(p.chara(), 1052);
    }
}
