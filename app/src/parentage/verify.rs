//! How a plan's careers spread: each step is repeated until a career (with its reroll) meets the
//! must-haves, so its careers follow a geometric distribution, and the plan's are their sum.
//! Sampled under both pink rules (the cautious one the cap uses, and the best case), so the
//! answer shows how much the disputed rule matters.

use super::jobs::Run;
use super::outcome::with_reroll;

/// The careers a plan takes, under one rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spread {
    /// 10th, 50th and 90th percentiles.
    pub pctl: [u32; 3],
    /// The chance the plan is done within the budget, if one was given.
    pub within: Option<f64>,
}

/// The careers a plan takes under the cautious rule and the best case.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Verify {
    pub cautious: Spread,
    pub best: Spread,
    pub samples: usize,
}

/// Sample a plan `samples` times. `budget` is the parent careers the player has before the
/// runner must start.
pub fn verify(steps: &[Run], budget: Option<u32>, samples: usize, seed: f64) -> Verify {
    let mut rng = crate::sim::rng::Rng::new(seed);
    let mut spread = |p: &dyn Fn(&Run) -> f64| {
        let mut totals: Vec<u32> = (0..samples)
            .map(|_| {
                steps
                    .iter()
                    .map(|s| {
                        let q = with_reroll(p(s));
                        if q >= 1.0 {
                            1
                        } else if q <= 0.0 {
                            10_000
                        } else {
                            // Inverse transform: the careers until the first success.
                            let u = rng.random().clamp(1e-12, 1.0 - 1e-12);
                            (u.ln() / (1.0 - q).ln()).floor() as u32 + 1
                        }
                    })
                    .sum::<u32>()
            })
            .collect();
        totals.sort_unstable();
        let at = |f: f64| totals.get(((totals.len() as f64 - 1.0) * f).round() as usize).copied().unwrap_or(0);
        Spread { pctl: [at(0.1), at(0.5), at(0.9)], within: budget.map(|b| totals.iter().filter(|t| **t <= b).count() as f64 / totals.len().max(1) as f64) }
    };
    let cautious = spread(&|s: &Run| s.p);
    let best = spread(&|s: &Run| s.p_best);
    Verify { cautious, best, samples }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(p: f64) -> Run {
        Run { p, p_best: p, ..Default::default() }
    }

    #[test]
    fn geometric_spread() {
        // One step at 50% a career with the reroll: 75% done in one career, 93.75% in two.
        let v = verify(&[run(0.5)], Some(1), 20_000, 7.0);
        assert!((v.cautious.within.unwrap() - 0.75).abs() < 0.02, "{v:?}");
        assert_eq!(v.cautious.pctl[1], 1);
        // A certain step always takes one career; two steps at least two.
        let v = verify(&[run(1.0), run(0.2)], None, 5_000, 7.0);
        assert!(v.cautious.pctl[0] >= 2 && v.cautious.within.is_none());
        // The median of a 10% step (19% with the reroll) is about 3–4 careers.
        let v = verify(&[run(0.1)], Some(20), 20_000, 3.0);
        assert!((3..=4).contains(&v.cautious.pctl[1]), "{v:?}");
        assert!((v.cautious.within.unwrap() - (1.0 - 0.81f64.powi(20))).abs() < 0.02);
        // A better best case finishes sooner.
        let v = verify(&[Run { p: 0.1, p_best: 0.3, ..Default::default() }], None, 5_000, 1.0);
        assert!(v.best.pctl[2] < v.cautious.pctl[2]);
    }
}

/// One step in plain numbers: the careers it usually takes (the median) and the unlucky case (1 in
/// 10 players need this many or more), each career rerolled.
pub fn step_spread(p: f64) -> (u32, u32) {
    let q = with_reroll(p);
    if q >= 1.0 {
        return (1, 1);
    }
    if q <= 0.0 {
        return (u32::MAX, u32::MAX);
    }
    let at = |miss: f64| (miss.ln() / (1.0 - q).ln()).ceil().max(1.0) as u32;
    (at(0.5), at(0.1))
}

/// The share of players still going after `n` careers of a step: luck doesn't build up, so the
/// next career has the same chance whatever came before.
pub fn still_going(p: f64, n: u32) -> f64 {
    (1.0 - with_reroll(p)).powi(n as i32)
}

/// "about 1 in 7" for a chance, rounded to a whole number.
pub fn one_in(p: f64) -> String {
    if p >= 0.995 {
        "every time".into()
    } else if p <= 0.0 {
        "never".into()
    } else {
        format!("about 1 in {}", (1.0 / p).round().max(1.0) as u32)
    }
}

/// A fixed word for a chance: likely (over 2 in 3), even (about half), unlikely (under 1 in 3),
/// long shot (under 1 in 10).
pub fn word(p: f64) -> &'static str {
    if p > 2.0 / 3.0 {
        "likely"
    } else if p >= 1.0 / 3.0 {
        "about even"
    } else if p >= 0.1 {
        "unlikely"
    } else {
        "a long shot"
    }
}

#[cfg(test)]
mod spread_tests {
    use super::*;

    #[test]
    fn plain_numbers() {
        // 14.7% a career, 27.2% with the reroll: half are done by 3, 1 in 10 need 8 or more.
        assert_eq!(step_spread(0.147), (3, 8));
        assert_eq!(step_spread(1.0), (1, 1));
        assert!((still_going(0.5, 2) - 0.0625).abs() < 1e-12);
        assert_eq!(one_in(0.147), "about 1 in 7");
        assert_eq!(word(0.147), "unlikely");
        assert_eq!(word(0.05), "a long shot");
        assert_eq!(word(0.8), "likely");
    }
}
