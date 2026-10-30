//! What one parent career makes, in closed form: the odds of each spark it leaves and how many
//! careers a step takes. The tables are `knowledge/mechanics.md`'s (Sparks and inheritance); the
//! two rules for which aptitude a pink lands on disagree, so both are kept and the planner holds
//! steps to the lower of the two.

use crate::lineage::{blue_star_odds, rank_star_odds};
use crate::veterans::Veteran;

/// A pink spark's stars: 1★ / 2★ / 3★.
pub const PINK_STARS: [f64; 3] = [0.20, 0.70, 0.10];

/// The chance of `k` stars or more from star odds (k of 0 is certain).
pub fn at_least(odds: [f64; 3], k: u8) -> f64 {
    if k == 0 { 1.0 } else { odds[usize::from(k.min(3)) - 1..].iter().sum() }
}

/// The Umamusume Wiki's weight for each grade in the pink draw (the middle of its ranges).
fn wiki_weight(rank: u8) -> f64 {
    match rank {
        8 => 1.075,
        7 => 1.0,
        6 => 0.875,
        5 => 0.775,
        4 => 0.65,
        3 => 0.45,
        2 => 0.25,
        1 => 0.10,
        _ => 0.0,
    }
}

/// The chance her pink lands on aptitude `i`, given her final grade ranks: uma.guide's rule (even
/// among her A and S aptitudes, none below A) and the Wiki's (every grade weighted).
pub fn pink_odds(ranks: &[u8; 10], i: usize) -> (f64, f64) {
    let top = ranks.iter().filter(|r| **r >= 7).count();
    let even = if ranks[i] >= 7 { 1.0 / top as f64 } else { 0.0 };
    let total: f64 = ranks.iter().map(|r| wiki_weight(*r)).sum();
    let weighted = if total > 0.0 { wiki_weight(ranks[i]) / total } else { 0.0 };
    (even, weighted)
}

/// One career's chance with the 30 TP spark reroll: two draws, keep the better.
pub fn with_reroll(p: f64) -> f64 {
    1.0 - (1.0 - p.clamp(0.0, 1.0)).powi(2)
}

/// Careers to expect until a career (with its reroll) succeeds.
pub fn careers(p: f64) -> f64 {
    let q = with_reroll(p);
    if q <= 0.0 { f64::INFINITY } else { 1.0 / q }
}

/// The chance of success within `n` careers, rerolling each.
pub fn within(p: f64, n: u32) -> f64 {
    1.0 - (1.0 - with_reroll(p)).powi(n as i32)
}

/// How this account's parents have finished: per stat, the share that ended below 600, at
/// 600–1099 and at 1100 or more (a blue spark's stars depend on it), and the share at SS or better
/// (a green's or white's stars depend on it).
#[derive(Clone, Debug, PartialEq)]
pub struct History {
    pub bands: [[f64; 3]; 5],
    pub ss: f64,
    pub careers: usize,
}

impl History {
    /// With no finished umas yet: every stat at 600–1099, below SS.
    pub fn from(vets: &[Veteran]) -> Self {
        if vets.is_empty() {
            return History { bands: [[0.0, 1.0, 0.0]; 5], ss: 0.0, careers: 0 };
        }
        let n = vets.len() as f64;
        let mut bands = [[0.0; 3]; 5];
        for v in vets {
            for (i, s) in [v.speed, v.stamina, v.power, v.guts, v.wiz].into_iter().enumerate() {
                bands[i][if s >= 1100 { 2 } else if s >= 600 { 1 } else { 0 }] += 1.0 / n;
            }
        }
        let ss = vets.iter().filter(|v| v.rank >= 17).count() as f64 / n;
        History { bands, ss, careers: vets.len() }
    }

    /// The chance a career's blue spark is `stat` at `k` stars or more: the stat is one of five,
    /// its stars by the band it finishes in.
    pub fn blue(&self, stat: usize, k: u8) -> f64 {
        let reps = [300, 800, 1100];
        0.2 * (0..3).map(|b| self.bands[stat][b] * at_least(blue_star_odds(reps[b]), k)).sum::<f64>()
    }

    /// The chance a career's green (her unique) has `k` stars or more, at this account's usual rank.
    pub fn green(&self, k: u8) -> f64 {
        self.ss * at_least(rank_star_odds(true), k) + (1.0 - self.ss) * at_least(rank_star_odds(false), k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haru_urara_dirt() {
        // Haru Urara [Bestest Prize ♪]: Dirt, Sprint and Late at A, Mile and End at B, the rest G.
        let ranks = [1, 7, 7, 6, 1, 1, 1, 1, 7, 6];
        let (even, weighted) = pink_odds(&ranks, 1);
        assert!((even - 1.0 / 3.0).abs() < 1e-9);
        assert!(weighted < even && weighted > 0.15, "{weighted}");
        // Dirt 2★+ a career under the even rule: 26.7% (`lineage::parent_run_report`).
        assert!((even * at_least(PINK_STARS, 2) - 0.2667).abs() < 1e-3);
        assert_eq!(pink_odds(&ranks, 0).0, 0.0, "a G aptitude is never in the even draw");
    }

    #[test]
    fn careers_with_reroll() {
        assert!((with_reroll(0.5) - 0.75).abs() < 1e-12);
        assert!((careers(0.5) - 4.0 / 3.0).abs() < 1e-12);
        assert!(careers(0.0).is_infinite());
        assert!((within(0.1, 1) - 0.19).abs() < 1e-12);
        assert_eq!(at_least(PINK_STARS, 0), 1.0);
        assert!((at_least(PINK_STARS, 2) - 0.8).abs() < 1e-12);
    }

    #[test]
    fn history_bands() {
        let v = |stamina, rank| Veteran { speed: 1150, stamina, power: 700, guts: 400, wiz: 700, rank, ..Default::default() };
        let h = History::from(&[v(1100, 17), v(800, 14)]);
        assert_eq!(h.bands[0], [0.0, 0.0, 1.0]);
        assert_eq!(h.bands[1], [0.0, 0.5, 0.5]);
        assert_eq!(h.ss, 0.5);
        // Speed always finished at 1100+: 3★ is 1/5 × 10%.
        assert!((h.blue(0, 3) - 0.02).abs() < 1e-12);
        assert!((h.green(2) - (0.5 * 0.8 + 0.5 * 0.55)).abs() < 1e-12);
        assert_eq!(History::from(&[]).blue(1, 3), 0.2 * 0.05);
    }
}
