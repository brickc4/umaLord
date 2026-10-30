//! Layer one, exact: how good a pair of legacies is for the goal. Start aptitudes and stats are
//! fixed by the six umas' sparks, the inherited unique by the legacies' greens, and each wanted
//! white's chance by the slots' affinity. Every pair of finished umas is scored; nothing is
//! sampled.

use super::{Goal, Parent, after_start, ranks};
use crate::knowledge::Knowledge;
use crate::lineage::over_both;

/// One lineage scored for the goal.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Lineage {
    /// Indices into the parents scored.
    pub legacies: [usize; 2],
    /// Per target: aptitude, base rank, rank after the start, pink stars in the lineage.
    pub targets: Vec<(usize, u8, u8, u32)>,
    /// Pink stars short of A, summed over the targets (0 when every target starts at A).
    pub short: u32,
    /// Whether the wanted inherited unique is a legacy's green (true when none is wanted).
    pub green: bool,
    /// Stats added at the start.
    pub blues: [u32; 5],
    /// The lineage's affinity, and each slot's: legacy 1, its parents, legacy 2, its parents.
    pub affinity: u32,
    pub slots: [u32; 6],
    /// Each wanted white's chance to fire at least once over the two inspirations.
    pub whites: Vec<(u32, f64)>,
    /// 0 unless the goal's must-haves are met, then up to 1 for affinity, blues and whites.
    pub fit: f64,
}

impl Lineage {
    pub fn meets(&self) -> bool {
        self.short == 0 && self.green
    }
}

/// Whether two legacies may be paired for the goal: neither is the runner's character, they
/// are two different characters, and at most one is borrowed.
pub fn pairable(goal: &Goal, a: &Parent, b: &Parent) -> bool {
    goal.allows(a) && goal.allows(b) && a.chara() != b.chara() && !(super::is_borrowed(a.id) && super::is_borrowed(b.id))
}

/// Score one pair.
pub fn score(kb: &Knowledge, goal: &Goal, parents: &[Parent], legacies: [usize; 2]) -> Lineage {
    let [a, b] = legacies.map(|i| &parents[i]);
    let base = kb.umas.get(&goal.runner).map(|u| ranks(&u.aptitude)).unwrap_or([0; 10]);
    let mut pinks = [0u8; 10];
    (0..10).for_each(|i| pinks[i] = a.pink_line[i] + b.pink_line[i]);
    let mut short = 0;
    let targets = goal
        .targets
        .iter()
        .map(|&i| {
            let stars = u32::from(pinks[i]);
            let after = after_start(base[i], stars);
            short += super::stars_for_a(base[i]).map_or(99, |n| n.saturating_sub(stars));
            (i, base[i], after, stars)
        })
        .collect();
    let green = goal.green.is_none_or(|g| [a, b].iter().any(|p| p.green.is_some_and(|(u, _)| u == g)));
    let mut blues = [0; 5];
    (0..5).for_each(|i| blues[i] = a.blue_line[i] + b.blue_line[i]);
    let lin = kb.affinity.lineage(goal.runner, &[Some(a.legacy.clone()), Some(b.legacy.clone())]);
    let mut slots = [0; 6];
    for (j, l) in lin.legacies.iter().enumerate() {
        if let Some(l) = l {
            slots[3 * j] = l.total();
            slots[3 * j + 1] = l.grandparent(0);
            slots[3 * j + 2] = l.grandparent(1);
        }
    }
    let sparks: [&[u32]; 6] = [&a.own, line(a, 0), line(a, 1), &b.own, line(b, 0), line(b, 1)];
    let whites: Vec<(u32, f64)> = goal
        .whites
        .iter()
        .map(|&key| {
            let miss: f64 = sparks
                .iter()
                .zip(slots)
                .flat_map(|(ids, aff)| ids.iter().filter(move |id| *id / 100 == key).map(move |id| (*id, aff)))
                .filter_map(|(id, aff)| kb.sparks.get(&id).map(|s| 1.0 - over_both(&s.kind, s.stars, aff)))
                .product();
            (key, 1.0 - miss)
        })
        .collect();
    let mut lineage = Lineage { legacies, targets, short, green, blues, affinity: lin.total(), slots, whites, fit: 0.0 };
    if lineage.meets() {
        let wanted: Vec<(u32, u32)> = goal.stats.iter().zip(blues).filter(|(w, _)| **w > 0).map(|(w, b)| (*w, b)).collect();
        let blue = if wanted.is_empty() { 1.0 } else { wanted.iter().map(|(w, b)| (f64::from(*b) / f64::from(*w)).min(1.0)).sum::<f64>() / wanted.len() as f64 };
        let white = if lineage.whites.is_empty() { 1.0 } else { lineage.whites.iter().map(|w| w.1).sum::<f64>() / lineage.whites.len() as f64 };
        lineage.fit = 0.5 * (f64::from(lineage.affinity) / 200.0).min(1.0) + 0.3 * blue + 0.2 * white;
    }
    lineage
}

fn line(p: &Parent, j: usize) -> &[u32] {
    p.parents[j].as_ref().map_or(&[], |(_, ids)| ids.as_slice())
}

/// Every allowed pair, best first: the goal met, then fewest stars short, then fit, then affinity.
pub fn rank(kb: &Knowledge, goal: &Goal, parents: &[Parent], top: usize) -> Vec<Lineage> {
    let mut out = Vec::new();
    for i in 0..parents.len() {
        for j in i + 1..parents.len() {
            if pairable(goal, &parents[i], &parents[j]) {
                out.push(score(kb, goal, parents, [i, j]));
            }
        }
    }
    out.sort_by(|x, y| (!x.meets(), x.short, !x.green).cmp(&(!y.meets(), y.short, !y.green)).then(y.fit.total_cmp(&x.fit)).then(y.affinity.cmp(&x.affinity)));
    out.truncate(top);
    out
}

#[cfg(test)]
mod tests {
    use super::super::tests::{kb, parent, spark};
    use super::*;

    #[test]
    fn finds_the_dirt_pair() {
        let kb = kb();
        let dirt = spark(&kb, "pink", "Dirt", 1);
        let turf = spark(&kb, "pink", "Turf", 2);
        // Oguri Cap [Starlight Beat] for a dirt mile, late: Dirt B needs one Dirt star.
        let goal = Goal { runner: 100601, targets: vec![1, 3, 8], ..Default::default() };
        let parents = vec![
            parent(&kb, 1, 104601, vec![dirt], [(100101, vec![]), (100201, vec![])]),
            parent(&kb, 2, 106801, vec![turf], [(100101, vec![]), (100201, vec![])]),
            parent(&kb, 3, 100201, vec![turf], [(100101, vec![]), (106801, vec![])]),
            // Another Oguri Cap: never her own legacy.
            parent(&kb, 4, 100602, vec![dirt], [(100101, vec![]), (100201, vec![])]),
        ];
        let ranked = rank(&kb, &goal, &parents, 10);
        assert_eq!(ranked.len(), 3, "the Oguri Cap is left out");
        assert!(ranked[0].meets() && ranked[0].legacies.contains(&0));
        assert!(!ranked[2].meets() && ranked[2].short == 1);
        assert_eq!(ranked[0].targets[0], (1, 6, 7, 1), "Dirt B → A");
        assert!(ranked[0].fit > 0.0 && ranked[2].fit == 0.0);
    }

    #[test]
    fn wanted_unique_and_whites() {
        let kb = kb();
        let green = spark(&kb, "green", "Shooting Star", 1);
        let white = kb.sparks.values().find(|s| s.kind == "white" && s.stars == 3).unwrap();
        let goal = Goal { runner: 100601, targets: vec![0], green: Some(green / 100), whites: vec![white.id / 100], ..Default::default() };
        let parents = vec![parent(&kb, 1, 100101, vec![green], [(104601, vec![white.id]), (100201, vec![])]), parent(&kb, 2, 106801, vec![], [(104601, vec![]), (100201, vec![])])];
        let l = score(&kb, &goal, &parents, [0, 1]);
        assert!(l.green && l.meets());
        let p = over_both("white", 3, l.slots[1]);
        assert!((l.whites[0].1 - p).abs() < 1e-12, "{:?} vs {p}", l.whites);
        let without = Goal { green: Some(106801), ..goal };
        assert!(!score(&kb, &without, &parents, [0, 1]).green);
    }
}
