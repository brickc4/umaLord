//! Layer two: the parent careers that close what the best lineage lacks.
//!
//! A step is one parent career to repeat: a trainee, the two finished umas she runs with, and
//! what the uma she leaves must have. Her product then sits in the runner's lineage with its own
//! parents (the run's legacies, whose own pinks count at the runner's start). The planner tries
//! every owned trainee with every pair of legacies, for one step paired with a finished uma the
//! account has, or two steps paired with each other, and keeps the route with the fewest expected
//! careers. No step may expect more than `CAP` careers under the cautious pink rule; when nothing
//! fits, it says so and returns the cheapest route anyway, marked over the cap.

use super::outcome::{PINK_STARS, at_least, careers};
use super::score::{Lineage, pairable, rank, score};
use super::{APTITUDES, Goal, Inventory, Parent, STATS, after_start, green_spark, pink_spark, ranks};
use crate::affinity::chara;
use crate::knowledge::{Distance, Knowledge};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The most careers a step may expect (with the 30 TP reroll), the player's limit for grinding.
pub const CAP: f64 = 20.0;

/// Something the finished uma a step leaves should have.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Want {
    Pink { aptitude: usize, stars: u8 },
    Blue { stat: usize, stars: u8 },
    Green { uma: u32, stars: u8 },
}

impl Want {
    pub fn label(&self, kb: &Knowledge) -> String {
        match self {
            Want::Pink { aptitude, stars } => format!("{} pink {stars}★+", APTITUDES[*aptitude]),
            Want::Blue { stat, stars } => format!("{} blue {stars}★+", STATS[*stat]),
            Want::Green { uma, stars } => format!("{} green {stars}★+", kb.sparks.get(&green_spark(*uma, 1)).map_or_else(|| super::uma_name(kb, *uma), |s| s.name.clone())),
        }
    }

    /// As a spark chip shows it: its colour (0 blue, 1 pink, 2 green), its name, and the stars wanted.
    pub fn chip(&self, kb: &Knowledge) -> (u8, String, u8) {
        match self {
            Want::Pink { aptitude, stars } => (1, APTITUDES[*aptitude].to_string(), *stars),
            Want::Blue { stat, stars } => (0, STATS[*stat].to_string(), *stars),
            Want::Green { uma, stars } => (2, kb.sparks.get(&green_spark(*uma, 1)).map_or_else(|| super::uma_name(kb, *uma), |s| s.name.clone()), *stars),
        }
    }

    /// One career's chance of it, where it doesn't hang on which aptitude a pink lands on: a blue
    /// by how this account's careers finish the stat, a green by its usual rank.
    pub fn odds(&self, h: &super::outcome::History) -> Option<f64> {
        match self {
            Want::Pink { .. } => None,
            Want::Blue { stat, stars } => Some(h.blue(*stat, *stars)),
            Want::Green { stars, .. } => Some(h.green(*stars)),
        }
    }

    /// Whether a finished uma's own sparks have it.
    fn key(&self) -> (u32, u8) {
        match self {
            Want::Pink { aptitude, stars } => (pink_spark(*aptitude, 0) / 100, *stars),
            Want::Blue { stat, stars } => (*stat as u32 + 1, *stars),
            Want::Green { uma, stars } => (*uma, *stars),
        }
    }

    pub fn met(&self, own: &[u32]) -> bool {
        self.short(own) == 0
    }

    /// Stars missing: 0 when met, the full count when the spark isn't there at all.
    pub fn short(&self, own: &[u32]) -> u8 {
        let (key, stars) = self.key();
        let best = own.iter().filter(|id| *id / 100 == key).map(|id| (id % 100) as u8).max().unwrap_or(0);
        stars.saturating_sub(best)
    }
}

/// One step: a parent career to repeat until the uma it leaves has the must-haves.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Run {
    pub trainee: u32,
    /// The finished umas she runs with (trained_chara_ids), and their outfits (what the game
    /// records as her parents, still known if one is later released).
    pub legacies: [u64; 2],
    pub legacy_umas: [u32; 2],
    pub must: Vec<Want>,
    pub nice: Vec<Want>,
    /// G1s worth winning in this career: each one her parents also won adds 3 affinity to that
    /// link (and a G1 won may leave a race spark, 1 in 5). Name, and how many of her parents won it.
    pub g1s: Vec<(String, u8)>,
    /// Stats worth finishing at 1100 or more: a stat there makes its blue spark 2★ or better 80%
    /// of the time (55% at 600 to 1099). By index (Speed, Stamina, Power, Guts, Wit).
    pub stat_aims: Vec<usize>,
    /// One career's chance of the must-haves under the lower of the two pink rules (the
    /// cautious one the cap uses), and under the higher (the best case).
    pub p: f64,
    pub p_best: f64,
}

impl Run {
    /// Careers to expect, rerolling the sparks each time.
    pub fn careers(&self) -> f64 {
        careers(self.p)
    }
}

/// A choice the player made that the plan must keep: the first step's trainee, one of her two
/// parents, or the finished uma the runner pairs with. The rest is planned around it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Pin {
    pub trainee: Option<u32>,
    pub legacy: Option<u64>,
    pub partner: Option<u64>,
    /// Steps the player gave up on: routes steer around them.
    pub avoid: Vec<Avoid>,
}

impl Pin {
    /// Whether nothing is pinned (steps to avoid don't count: they only narrow the search).
    pub fn is_empty(&self) -> bool {
        self.trainee.is_none() && self.legacy.is_none() && self.partner.is_none()
    }

    fn avoids(&self, inv: &Inventory, c: &Cand) -> bool {
        // A starter step's legacies are any two (no index): its ids are 0, as its Run records.
        let ids = c.legacies.map(|i| inv.parents.get(i).map_or(0, |p| p.id));
        self.avoid.iter().any(|a| a.matches(c.trainee, ids))
    }
}

/// A step to steer around: this trainee with these two legacies (in either order).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Avoid {
    pub trainee: u32,
    pub legacies: [u64; 2],
}

impl Avoid {
    pub fn of(run: &Run) -> Self {
        Avoid { trainee: run.trainee, legacies: run.legacies }
    }

    pub fn matches(&self, trainee: u32, ids: [u64; 2]) -> bool {
        self.trainee == trainee && (self.legacies == ids || self.legacies == [ids[1], ids[0]])
    }
}

/// The planner's answer for a goal.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct Proposal {
    /// Pink stars each target needs.
    pub need: Vec<(usize, u32)>,
    /// Targets more than four grades below A: no lineage starts them at A.
    pub impossible: Vec<usize>,
    /// The best lineage from the finished umas the account has.
    pub now: Option<Lineage>,
    pub steps: Vec<Run>,
    /// The finished uma the last step's product pairs with, for a one-step route.
    pub partner: Option<u64>,
    /// The lineage the route ends in, assuming each step's must-haves exactly.
    pub after: Option<Lineage>,
    /// True when a step expects more than `CAP` careers: nothing fits under it.
    pub over_cap: bool,
    /// Other one-step routes, cheapest first, one per trainee.
    pub alternatives: Vec<Run>,
}

impl Proposal {
    pub fn careers(&self) -> f64 {
        self.steps.iter().map(Run::careers).sum()
    }
}

/// Two-step candidates by the pink they're for, the stars they bring per target and the green.
type ByJob = BTreeMap<(Option<(usize, u8)>, Vec<u32>, bool), Vec<Cand>>;
/// Partners by the stars they bring per target and the green, with up to three characters.
type Group = (Vec<u32>, bool, Vec<(u32, usize)>);

/// One parent career considered: trainee, legacies, the pink it's for (aptitude and stars, or
/// none when only finishing it matters), and what it brings.
#[derive(Clone, Debug)]
struct Cand {
    trainee: u32,
    legacies: [usize; 2],
    pink: Option<(usize, u8)>,
    /// Pink stars its product brings to the runner per target (its own pink plus its parents').
    brings: Vec<u32>,
    green: bool,
    p: f64,
    p_best: f64,
    /// Affinity its product would have with the runner, apart from the other legacy.
    tie: u32,
}

impl Cand {
    fn careers(&self) -> f64 {
        careers(self.p)
    }

    fn better(&self, o: &Cand) -> bool {
        (self.careers(), std::cmp::Reverse(self.tie)) < (o.careers(), std::cmp::Reverse(o.tie))
    }
}

/// Plan the steps for a goal.
pub fn propose(kb: &Knowledge, goal: &Goal, inv: &Inventory) -> Proposal {
    propose_pinned(kb, goal, inv, &Pin::default())
}

/// Plan the steps for a goal, keeping the player's pinned choice.
pub fn propose_pinned(kb: &Knowledge, goal: &Goal, inv: &Inventory, pin: &Pin) -> Proposal {
    let need = match goal.need(kb) {
        Ok(n) => n,
        Err(impossible) => return Proposal { impossible, ..Default::default() },
    };
    let now = rank(kb, goal, &inv.parents, 1).into_iter().next();
    let mut out = Proposal { need: need.clone(), now: now.clone(), ..Default::default() };
    if now.as_ref().is_some_and(Lineage::meets) {
        return out;
    }
    let partners = Partners::new(goal, &need, &inv.parents, pin.partner);
    let pinned = |c: &Cand| pin.trainee.is_none_or(|t| c.trainee == t) && pin.legacy.is_none_or(|id| c.legacies.iter().any(|i| inv.parents[*i].id == id));
    // Two-step candidates that keep the pin (one of the two steps must).
    let mut two_pinned: ByJob = BTreeMap::new();
    // The best one-step route (with its partner's chara constraint checked), per trainee.
    let mut one: BTreeMap<u32, Cand> = BTreeMap::new();
    // The best careers per (pink, stars brought, green): candidates for two-step routes.
    let mut two: ByJob = BTreeMap::new();
    for_each_cand(kb, goal, inv, &need, None, |c| {
        if pin.avoids(inv, c) {
            return;
        }
        let green_needed = goal.green.is_some() && !c.green;
        let req: Vec<u32> = need.iter().zip(&c.brings).map(|((_, n), b)| n.saturating_sub(*b)).collect();
        let ok = pinned(c);
        if ok && partners.exists(&req, green_needed, chara(c.trainee)) && one.get(&c.trainee).is_none_or(|o| c.better(o)) {
            one.insert(c.trainee, c.clone());
        }
        keep_best(&mut two, c);
        if ok {
            keep_best(&mut two_pinned, c);
        }
    });
    let mut singles: Vec<Cand> = one.into_values().collect();
    singles.sort_by(|a, b| a.careers().total_cmp(&b.careers()).then(b.tie.cmp(&a.tie)));
    // A pinned partner is a finished uma, so it rules out two new parents.
    let pair = if pin.partner.is_some() { None } else if pin.is_empty() { best_pair(goal, &need, &two, &two) } else { best_pair(goal, &need, &two_pinned, &two) };
    let fits = |c: &f64| *c <= CAP;
    let single = singles.first().filter(|c| fits(&c.careers()));
    let double = pair.as_ref().filter(|(a, b)| fits(&a.careers()) && fits(&b.careers()));
    let pick_single = match (single, double) {
        (Some(s), Some((a, b))) => s.careers() <= a.careers() + b.careers(),
        (Some(_), None) => true,
        (None, Some(_)) => false,
        // Nothing under the cap: the cheapest route anyway, marked.
        (None, None) => {
            out.over_cap = true;
            match (singles.first(), &pair) {
                (Some(s), Some((a, b))) => s.careers() <= a.careers() + b.careers(),
                (s, _) => s.is_some(),
            }
        }
    };
    if pick_single {
        // Routes about as cheap as the cheapest and under the cap: the one whose final lineage is
        // best. Past the cap, only the cheapest.
        let cheapest = singles.first().map_or(0.0, Cand::careers);
        let slack = if cheapest <= CAP { (cheapest * 1.15 + 0.3).min(CAP) } else { cheapest };
        let mut best: Option<(f64, u32, Run, u64, Lineage)> = None;
        for c in singles.iter().filter(|c| c.careers() <= slack).take(20) {
            let (run, product) = to_run(kb, goal, inv, c);
            let req: Vec<u32> = need.iter().zip(&c.brings).map(|((_, n), b)| n.saturating_sub(*b)).collect();
            let Some(w) = partners.best(kb, goal, inv, &product, &req, goal.green.is_some() && !c.green) else { continue };
            let l = score(kb, goal, &[product, inv.parents[w].clone()], [0, 1]);
            if best.as_ref().is_none_or(|b| (l.fit, l.affinity) > (b.0, b.1)) {
                best = Some((l.fit, l.affinity, run, inv.parents[w].id, l));
            }
        }
        if let Some((_, _, mut run, w, l)) = best {
            // The uma her product pairs with shares a link with her too: its G1s count.
            let ps: Vec<&Parent> = run.legacies.iter().chain([w].iter()).filter_map(|id| inv.by_id(*id)).collect();
            if ps.len() == 3 {
                run.g1s = g1_targets(kb, run.trainee, &ps);
            }
            out.partner = Some(w);
            out.after = Some(l);
            out.steps.push(run);
        }
    } else if let Some((a, b)) = &pair {
        let (ra, pa) = to_run(kb, goal, inv, a);
        let (rb, pb) = to_run(kb, goal, inv, b);
        out.after = Some(score(kb, goal, &[pa, pb], [0, 1]));
        // The harder step first: its result decides what the easier one must make.
        let mut steps = vec![ra, rb];
        steps.sort_by(|x, y| y.careers().total_cmp(&x.careers()));
        out.steps = steps;
    }
    // Two generations (step 1's product as one of step 2's parents) compete on cost with the
    // routes above, and stand in when nothing fits the cap. A plan of two new parents can cost
    // far more than one where the second career only has to finish (play-throughs, 2026-10-28).
    let current = if out.over_cap || out.steps.is_empty() { f64::INFINITY } else { out.steps.iter().map(Run::careers).sum() };
    if pin.is_empty()
        && let Some((r1, r2, w, l)) = two_generations(kb, goal, inv, &need, &two, pin, current)
    {
        out.over_cap = false;
        out.steps = vec![r1, r2];
        out.partner = w;
        out.after = Some(l);
    }
    let chosen: Vec<u32> = out.steps.iter().map(|s| s.trainee).collect();
    out.alternatives = singles.iter().filter(|c| !chosen.contains(&c.trainee) && fits(&c.careers())).take(3).map(|c| to_run(kb, goal, inv, c).0).collect();
    out
}

/// What a friend's legacy would need to bring for the runner to start at A today: which of the
/// player's finished umas she'd pair with, the pink stars she (with her two parents) must carry
/// per target, whether she must carry the wanted unique, and the characters with the most
/// affinity to the runner and that partner (best first). The player looks for one among their
/// friends and adds her as a borrow.
#[derive(Clone, Debug, PartialEq)]
pub struct BorrowWish {
    pub partner: u64,
    pub stars: Vec<(usize, u32)>,
    pub green: Option<u32>,
    pub charas: Vec<(u32, u32)>,
    /// Whether one legacy line can carry it: she and her two parents have one pink each, at
    /// most 3★, so the stars need at most three pinks.
    pub possible: bool,
}

pub fn borrow_wish(kb: &Knowledge, goal: &Goal, inv: &Inventory) -> Option<BorrowWish> {
    let need = goal.need(kb).ok()?;
    // The partner that leaves the least for the friend to bring, then the best affinity to the runner.
    let w = inv
        .parents
        .iter()
        .filter(|p| goal.allows(p) && !super::is_borrowed(p.id))
        .min_by_key(|p| {
            let short: u32 = need.iter().map(|(i, n)| n.saturating_sub(u32::from(p.pink_line[*i]))).sum();
            let green = goal.green.is_some_and(|g| p.green.is_some_and(|(u, _)| u == g));
            (short, !green, std::cmp::Reverse(kb.affinity.pair(goal.runner, p.uma)))
        })?;
    let stars: Vec<(usize, u32)> = need.iter().map(|(i, n)| (*i, n.saturating_sub(u32::from(w.pink_line[*i])))).filter(|(_, n)| *n > 0).collect();
    let green = goal.green.filter(|g| !w.green.is_some_and(|(u, _)| u == *g));
    let mut charas: Vec<(u32, u32)> = kb
        .affinity
        .members
        .keys()
        .filter(|c| **c != chara(goal.runner) && **c != w.chara() && kb.umas.keys().any(|u| chara(*u) == **c))
        .map(|c| (*c, kb.affinity.pair(goal.runner, *c) + kb.affinity.pair(w.uma, *c)))
        .collect();
    charas.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    charas.truncate(5);
    let pinks: u32 = stars.iter().map(|(_, n)| n.div_ceil(3)).sum();
    Some(BorrowWish { partner: w.id, stars, green, charas, possible: pinks <= 3 })
}

/// A starter step's legacies: any two (the player's own or a friend's), when the account has
/// fewer than two finished umas to plan with.
const ANY: usize = usize::MAX;

/// The id step 1's product stands under while step 2 is planned with her as a parent.
pub const FROM_STEP: u64 = u64::MAX - 3000;

/// Two generations: for the cheapest first steps (one per kind of product), add her product as
/// a parent and plan a second step that runs with her, paired with a finished uma or with the
/// first product itself. The cheapest under the cap, or None.
/// The cheapest two-generation route under `bound` expected careers (a second step costs at
/// least one career, so a first step past `bound - 1` can't win).
fn two_generations(kb: &Knowledge, goal: &Goal, inv: &Inventory, need: &[(usize, u32)], two: &ByJob, pin: &Pin, bound: f64) -> Option<(Run, Run, Option<u64>, Lineage)> {
    let mut firsts: Vec<&Cand> = two.values().filter_map(|l| l.first()).filter(|c| c.careers() <= CAP && c.careers() + 1.0 < bound).collect();
    firsts.sort_by(|a, b| a.careers().total_cmp(&b.careers()));
    let mut best: Option<(f64, Run, Run, Option<u64>, Lineage)> = None;
    for c1 in firsts.into_iter().take(40) {
        let (r1, mut p1) = to_run(kb, goal, inv, c1);
        p1.id = FROM_STEP;
        let mut inv2 = inv.clone();
        inv2.parents.push(p1);
        let at = inv2.parents.len() - 1;
        let partners = Partners::new(goal, need, &inv2.parents, None);
        let mut second: Option<Cand> = None;
        for_each_cand(kb, goal, &inv2, need, Some(at), |c| {
            let total = c1.careers() + c.careers();
            if c.careers() > CAP || total >= bound || chara(c.trainee) == chara(c1.trainee) || best.as_ref().is_some_and(|b| total >= b.0) || pin.avoids(&inv2, c) {
                return;
            }
            // Her green only counts as the runner's own legacy, which the partner check covers.
            let green_needed = goal.green.is_some() && !c.green;
            let req: Vec<u32> = need.iter().zip(&c.brings).map(|((_, n), b)| n.saturating_sub(*b)).collect();
            if partners.exists(&req, green_needed, chara(c.trainee)) && second.as_ref().is_none_or(|o| c.better(o)) {
                second = Some(c.clone());
            }
        });
        let Some(c2) = second else { continue };
        let (r2, product) = to_run(kb, goal, &inv2, &c2);
        let req: Vec<u32> = need.iter().zip(&c2.brings).map(|((_, n), b)| n.saturating_sub(*b)).collect();
        let Some(w) = partners.best(kb, goal, &inv2, &product, &req, goal.green.is_some() && !c2.green) else { continue };
        let l = score(kb, goal, &[product, inv2.parents[w].clone()], [0, 1]);
        if l.meets() {
            best = Some((c1.careers() + c2.careers(), r1, r2, Some(inv2.parents[w].id), l));
        }
    }
    best.map(|(_, r1, r2, w, l)| (r1, r2, w, l))
}

/// Keep the three best candidates per key, with different trainees, so two steps can always differ.
fn keep_best(map: &mut ByJob, c: &Cand) {
    let list = map.entry((c.pink, c.brings.clone(), c.green)).or_default();
    if let Some(i) = list.iter().position(|o| chara(o.trainee) == chara(c.trainee)) {
        if c.better(&list[i]) {
            list[i] = c.clone();
        }
    } else {
        list.push(c.clone());
    }
    list.sort_by(|a, b| a.careers().total_cmp(&b.careers()).then(b.tie.cmp(&a.tie)));
    list.truncate(3);
}

/// Every useful parent career: each owned trainee (not the runner's character) with each pair
/// of finished umas she may run with, for each pink on a target she can make at each star count,
/// and for no pink at all (when what her parents bring is enough).
fn for_each_cand(kb: &Knowledge, goal: &Goal, inv: &Inventory, need: &[(usize, u32)], with: Option<usize>, mut f: impl FnMut(&Cand)) {
    let ps = &inv.parents;
    let aff = &kb.affinity;
    for &(t, stars) in &inv.owned {
        let Some(info) = kb.umas.get(&t) else { continue };
        if chara(t) == chara(goal.runner) {
            continue;
        }
        let base = ranks(&info.aptitude);
        let green = goal.green == Some(t) && stars >= 3;
        let direct = aff.pair(goal.runner, t);
        let trio: Vec<u32> = ps.iter().map(|p| aff.shared(&[goal.runner, t, p.uma])).collect();
        // Fewer than two finished umas: a starter career with any two legacies, her own grades deciding the pink.
        if ps.len() < 2 && with.is_none() {
            for (j, (i, n)) in need.iter().enumerate() {
                let (even, weighted) = super::outcome::pink_odds(&base, *i);
                let (low, best) = (even.min(weighted), even.max(weighted));
                if low <= 0.0 || *n == 0 {
                    continue;
                }
                for k in 1..=3u8 {
                    let mut brings = vec![0; need.len()];
                    brings[j] = u32::from(k).min(*n);
                    f(&Cand { trainee: t, legacies: [ANY, ANY], pink: Some((*i, k)), brings, green, p: low * at_least(PINK_STARS, k), p_best: best * at_least(PINK_STARS, k), tie: direct });
                }
            }
        }
        for a in 0..ps.len() {
            if ps[a].chara() == chara(t) {
                continue;
            }
            for b in a + 1..ps.len() {
                if ps[b].chara() == chara(t) || ps[b].chara() == ps[a].chara() || (super::is_borrowed(ps[a].id) && super::is_borrowed(ps[b].id)) {
                    continue;
                }
                if with.is_some_and(|w| a != w && b != w) {
                    continue;
                }
                let (pa, pb) = (&ps[a], &ps[b]);
                let fin: [u8; 10] = std::array::from_fn(|i| after_start(base[i], u32::from(pa.pink_line[i] + pb.pink_line[i])));
                // Stars past what a target needs change nothing: clamp them, so fewer routes differ.
                let gp: Vec<u32> = need.iter().map(|(i, n)| u32::from(pa.pink_own[*i] + pb.pink_own[*i]).min(*n)).collect();
                let tie = direct + trio[a] + trio[b];
                let mut c = Cand { trainee: t, legacies: [a, b], pink: None, brings: gp.clone(), green, p: 1.0, p_best: 1.0, tie };
                f(&c);
                for (j, (i, n)) in need.iter().enumerate() {
                    // The cautious chance is the lower of the two rules: uma.guide's even rule
                    // never lands a pink below A, and the Wiki's weights dilute it over every grade.
                    let (even, weighted) = super::outcome::pink_odds(&fin, *i);
                    let (low, best) = (even.min(weighted), even.max(weighted));
                    if low <= 0.0 || gp[j] >= *n {
                        continue;
                    }
                    for k in 1..=3u8 {
                        c.pink = Some((*i, k));
                        c.brings = gp.clone();
                        c.brings[j] = (c.brings[j] + u32::from(k)).min(*n);
                        c.p = low * at_least(PINK_STARS, k);
                        c.p_best = best * at_least(PINK_STARS, k);
                        f(&c);
                    }
                }
            }
        }
    }
}

/// The finished umas that could be the other legacy, grouped by the pink stars they bring per
/// target and whether they carry the wanted green, with up to three characters each (enough to
/// avoid the step's trainee).
struct Partners {
    groups: Vec<Group>,
    /// The pinned partner, the only one allowed.
    only: Option<u64>,
}

impl Partners {
    fn new(goal: &Goal, need: &[(usize, u32)], parents: &[Parent], only: Option<u64>) -> Self {
        let mut groups: Vec<Group> = Vec::new();
        for (i, p) in parents.iter().enumerate().filter(|(_, p)| goal.allows(p) && only.is_none_or(|id| p.id == id)) {
            let v: Vec<u32> = need.iter().map(|(j, _)| u32::from(p.pink_line[*j])).collect();
            let g = goal.green.is_some_and(|w| p.green.is_some_and(|(u, _)| u == w));
            let at = match groups.iter().position(|x| x.0 == v && x.1 == g) {
                Some(at) => at,
                None => {
                    groups.push((v, g, Vec::new()));
                    groups.len() - 1
                }
            };
            let charas = &mut groups[at].2;
            if charas.len() < 3 && !charas.iter().any(|(c, _)| *c == p.chara()) {
                charas.push((p.chara(), i));
            }
        }
        Partners { groups, only }
    }

    fn exists(&self, req: &[u32], green: bool, avoid: u32) -> bool {
        self.groups.iter().any(|(v, g, cs)| (!green || *g) && v.iter().zip(req).all(|(have, want)| have >= want) && cs.iter().any(|(c, _)| *c != avoid))
    }

    /// The partner meeting the requirement with the best lineage for the runner.
    fn best(&self, kb: &Knowledge, goal: &Goal, inv: &Inventory, product: &Parent, req: &[u32], green: bool) -> Option<usize> {
        let need = goal.need(kb).unwrap_or_default();
        let ok = |p: &Parent| {
            pairable(goal, product, p) && (!green || goal.green.is_some_and(|w| p.green.is_some_and(|(u, _)| u == w))) && need.iter().zip(req).all(|((j, _), r)| u32::from(p.pink_line[*j]) >= *r)
        };
        let mut best: Option<(f64, u32, usize)> = None;
        for (i, p) in inv.parents.iter().enumerate().filter(|(_, p)| ok(p) && self.only.is_none_or(|id| p.id == id)) {
            let l = score(kb, goal, &[product.clone(), p.clone()], [0, 1]);
            if best.is_none_or(|(f, a, _)| (l.fit, l.affinity) > (f, a)) {
                best = Some((l.fit, l.affinity, i));
            }
        }
        best.map(|b| b.2)
    }
}

/// The cheapest two steps that together meet every target (and the wanted green), with
/// different trainees.
fn best_pair(goal: &Goal, need: &[(usize, u32)], left: &ByJob, right: &ByJob) -> Option<(Cand, Cand)> {
    let mut best: Option<(f64, Cand, Cand)> = None;
    for (kx, cx) in left.iter() {
        for (ky, cy) in right.iter() {
            if goal.green.is_some() && !kx.2 && !ky.2 {
                continue;
            }
            if !need.iter().enumerate().all(|(j, (_, n))| kx.1[j] + ky.1[j] >= *n) {
                continue;
            }
            for a in cx.iter() {
                for b in cy.iter().filter(|b| chara(b.trainee) != chara(a.trainee)) {
                    let total = a.careers() + b.careers();
                    if best.as_ref().is_none_or(|(t, _, _)| total < *t) {
                        best = Some((total, a.clone(), b.clone()));
                    }
                }
            }
        }
    }
    best.map(|(_, a, b)| (a, b))
}

/// A considered career as a step, and the finished uma it would leave at its must-haves.
fn to_run(kb: &Knowledge, goal: &Goal, inv: &Inventory, c: &Cand) -> (Run, Parent) {
    if c.legacies[0] == ANY {
        return starter_run(kb, goal, c);
    }
    let [a, b] = c.legacies.map(|i| &inv.parents[i]);
    let mut must = Vec::new();
    let mut nice = Vec::new();
    let mut own = Vec::new();
    if let Some((i, k)) = c.pink {
        must.push(Want::Pink { aptitude: i, stars: k });
        own.push(pink_spark(i, k));
        if k < 3 {
            nice.push(Want::Pink { aptitude: i, stars: k + 1 });
        }
    } else if let Some(&(i, _)) = goal.need(kb).unwrap_or_default().iter().find(|(i, n)| *n > 0 && u32::from(a.pink_own[*i] + b.pink_own[*i]) < *n) {
        nice.push(Want::Pink { aptitude: i, stars: 1 });
    }
    if c.green {
        must.push(Want::Green { uma: c.trainee, stars: 1 });
        nice.push(Want::Green { uma: c.trainee, stars: 2 });
        own.push(green_spark(c.trainee, 1));
    }
    for (stat, w) in goal.stats.iter().enumerate() {
        if *w > 0 {
            nice.push(Want::Blue { stat, stars: 2 });
        }
    }
    let g1s = g1_targets(kb, c.trainee, &[a, b]);
    let stat_aims = goal.stats.iter().enumerate().filter(|(_, w)| **w > 0).map(|(i, _)| i).collect();
    let run = Run { trainee: c.trainee, legacies: [a.id, b.id], legacy_umas: [a.uma, b.uma], must, nice, g1s, stat_aims, p: c.p, p_best: c.p_best };
    (run, Parent::planned(kb, c.trainee, own, a, b))
}

/// The G1s her parents won that she can run (her surface and distance grades B or better),
/// most shared first: each adds 3 affinity to every link with a parent that won it.
fn g1_targets(kb: &Knowledge, trainee: u32, parents: &[&Parent]) -> Vec<(String, u8)> {
    let Some(info) = kb.umas.get(&trainee) else { return Vec::new() };
    let r = ranks(&info.aptitude);
    let mut count: BTreeMap<&str, u8> = BTreeMap::new();
    for p in parents {
        for w in &p.legacy.wins {
            *count.entry(w.as_str()).or_default() += 1;
        }
    }
    let runnable = |name: &str| {
        kb.races.iter().find(|x| x.name == name && x.grade == "G1").is_some_and(|x| {
            let s = if x.surface == "Dirt" { 1 } else { 0 };
            let d = 2 + Distance::ALL.iter().position(|d| *d == Distance::of_metres(x.distance)).unwrap_or(1);
            r[s] >= 6 && r[d] >= 6
        })
    };
    let mut out: Vec<(String, u8)> = count.into_iter().filter(|(n, _)| runnable(n)).map(|(n, c)| (n.to_string(), c)).collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out.truncate(5);
    out
}

/// A starter step: her career with any two legacies, for the pink her own grades make likely.
fn starter_run(kb: &Knowledge, goal: &Goal, c: &Cand) -> (Run, Parent) {
    let mut must = Vec::new();
    let mut nice = Vec::new();
    let mut own = Vec::new();
    if let Some((i, k)) = c.pink {
        must.push(Want::Pink { aptitude: i, stars: k });
        own.push(pink_spark(i, k));
        if k < 3 {
            nice.push(Want::Pink { aptitude: i, stars: k + 1 });
        }
    }
    if c.green {
        must.push(Want::Green { uma: c.trainee, stars: 1 });
        own.push(green_spark(c.trainee, 1));
    }
    for (stat, w) in goal.stats.iter().enumerate() {
        if *w > 0 {
            nice.push(Want::Blue { stat, stars: 2 });
        }
    }
    let stat_aims = goal.stats.iter().enumerate().filter(|(_, w)| **w > 0).map(|(i, _)| i).collect();
    let run = Run { trainee: c.trainee, legacies: [0, 0], legacy_umas: [0, 0], must, nice, g1s: Vec::new(), stat_aims, p: c.p, p_best: c.p_best };
    let legacy = crate::affinity::Legacy { uma: c.trainee, wins: Default::default(), parents: [None, None] };
    (run, Parent::build(kb, 0, c.trainee, own, [None, None], legacy))
}

#[cfg(test)]
mod tests {
    use super::super::outcome::History;
    use super::super::tests::{kb, parent, spark};
    use super::*;

    fn inv(parents: Vec<Parent>, owned: Vec<(u32, u8)>) -> Inventory {
        Inventory { parents, owned, history: History::from(&[]) }
    }

    /// Oguri Cap needs one Dirt star and no finished uma has one: one step of Haru Urara, the
    /// trainee with the fewest high aptitudes, for a Dirt pink of any stars.
    #[test]
    fn one_step_for_a_missing_pink() {
        let kb = kb();
        let turf = spark(&kb, "pink", "Turf", 2);
        let goal = Goal { runner: 100601, targets: vec![1, 3, 8], ..Default::default() };
        let parents = vec![parent(&kb, 1, 106801, vec![turf], [(100101, vec![]), (100201, vec![])]), parent(&kb, 2, 100201, vec![turf], [(100101, vec![]), (106801, vec![])])];
        let p = propose(&kb, &goal, &inv(parents, vec![(105201, 3), (101901, 3), (100602, 5)]));
        assert!(p.now.as_ref().is_some_and(|l| !l.meets()));
        assert_eq!(p.steps.len(), 1, "{p:?}");
        let s = &p.steps[0];
        assert_eq!(s.trainee, 105201, "Haru Urara over Agnes Digital, never another Oguri Cap");
        assert_eq!(s.must, vec![Want::Pink { aptitude: 1, stars: 1 }]);
        assert!(s.careers() < 5.0 && !p.over_cap);
        assert!(p.partner.is_some() && p.after.as_ref().is_some_and(Lineage::meets));
        assert_eq!(p.alternatives.first().map(|r| r.trainee), Some(101901));
    }

    /// A finished uma with a Dirt pink as a grandparent is enough: any career run with it makes a
    /// legacy that brings the star, so the step only has to be finished.
    #[test]
    fn grandparent_route() {
        let kb = kb();
        let dirt = spark(&kb, "pink", "Dirt", 2);
        let goal = Goal { runner: 100601, targets: vec![1], ..Default::default() };
        // Another Oguri Cap holds the Dirt star: never Oguri's own legacy, fine as a grandparent.
        let parents = vec![parent(&kb, 1, 100602, vec![dirt], [(100101, vec![]), (100201, vec![])]), parent(&kb, 2, 100201, vec![], [(100101, vec![]), (106801, vec![])])];
        let p = propose(&kb, &goal, &inv(parents, vec![(106801, 3)]));
        assert_eq!(p.steps.len(), 1, "{p:?}");
        assert!(p.steps[0].must.is_empty() && p.steps[0].legacies.contains(&1));
        assert!((p.steps[0].careers() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn met_already_and_impossible() {
        let kb = kb();
        let dirt = spark(&kb, "pink", "Dirt", 1);
        let goal = Goal { runner: 100601, targets: vec![1], ..Default::default() };
        let parents = vec![parent(&kb, 1, 104601, vec![dirt], [(100101, vec![]), (100201, vec![])]), parent(&kb, 2, 100201, vec![], [(100101, vec![]), (106801, vec![])])];
        let p = propose(&kb, &goal, &inv(parents.clone(), vec![]));
        assert!(p.steps.is_empty() && p.now.as_ref().is_some_and(Lineage::meets));
        // Special Week's Dirt is G: five grades below A.
        let p = propose(&kb, &Goal { runner: 100101, targets: vec![1], ..Default::default() }, &inv(parents, vec![]));
        assert_eq!(p.impossible, vec![1]);
    }

    /// Four Late stars for a C: no single career makes them. Two generations win on cost: Haru
    /// Urara makes Late 2★, then a career that only has to finish runs with her product as a
    /// parent (the route the first plan used to skip unless nothing fitted the cap).
    #[test]
    fn two_steps_and_the_cap() {
        let kb = kb();
        // Kitasan Black [Gilded Shrine to Glory]: Late C, so Late needs 4 stars.
        let goal = Goal { runner: 106801, targets: vec![8], ..Default::default() };
        let parents = vec![parent(&kb, 1, 100201, vec![], [(100101, vec![]), (104601, vec![])]), parent(&kb, 2, 104601, vec![], [(100101, vec![]), (100201, vec![])])];
        let p = propose(&kb, &goal, &inv(parents, vec![(105201, 3), (101901, 3)]));
        assert_eq!(p.steps.len(), 2, "{p:?}");
        assert!(p.steps[1].legacies.contains(&FROM_STEP), "step 2 runs with step 1's product: {p:?}");
        assert!(p.steps[1].must.is_empty() && p.steps[1].careers() == 1.0, "and only has to finish");
        let total: f64 = p.steps.iter().map(Run::careers).sum();
        assert!(total < 5.0, "{total}");
        assert!(p.after.as_ref().is_some_and(Lineage::meets));
        assert!(p.steps.iter().all(|s| s.careers() <= CAP) && !p.over_cap);
    }

    /// A friend's Dirt legacy closes the gap today; two borrowed legacies never pair.
    #[test]
    fn borrowed_legacy() {
        let kb = kb();
        let turf = spark(&kb, "pink", "Turf", 2);
        let goal = Goal { runner: 100601, targets: vec![1], ..Default::default() };
        let mut i = inv(vec![parent(&kb, 1, 106801, vec![turf], [(100101, vec![]), (100201, vec![])])], vec![]);
        assert!(propose(&kb, &goal, &i).now.is_none(), "one finished uma can't be two legacies");
        let falcon = super::super::Borrow { uma: 104601, own: vec![spark(&kb, "pink", "Dirt", 2)], parents: [(100101, vec![]), (100201, vec![])] };
        let id = i.borrow(&kb, &falcon);
        assert!(super::super::is_borrowed(id));
        let p = propose(&kb, &goal, &i);
        assert!(p.now.as_ref().is_some_and(Lineage::meets) && p.steps.is_empty(), "{p:?}");
        let id2 = i.borrow(&kb, &super::super::Borrow { uma: 105201, ..falcon.clone() });
        let (a, b) = (i.by_id(id).unwrap(), i.by_id(id2).unwrap());
        assert!(!pairable(&goal, a, b), "two borrowed legacies never pair");
        assert!(super::super::parent_name(&kb, a).contains("borrowed"));
    }

    #[test]
    fn borrow_wish_says_what_to_find() {
        let kb = kb();
        let dirt = spark(&kb, "pink", "Dirt", 1);
        // Oguri for Dirt and End (D: 7 stars); the finished Kitasan brings one Dirt star.
        let goal = Goal { runner: 100601, targets: vec![1, 9], ..Default::default() };
        let i = inv(vec![parent(&kb, 1, 106801, vec![dirt], [(100101, vec![]), (100201, vec![])])], vec![]);
        let w = borrow_wish(&kb, &goal, &i).unwrap();
        assert_eq!(w.partner, 1);
        assert_eq!(w.stars, vec![(9, 7)], "Dirt is covered; End needs all seven");
        assert!(w.possible, "7 stars fit in three pinks (3★, 3★, 1★)");
        let far = Goal { targets: vec![1, 5, 9], ..goal.clone() };
        assert!(!borrow_wish(&kb, &far, &i).unwrap().possible, "Long B (1) and End D (7) need three pinks plus one");
        assert_eq!(w.charas.len(), 5);
        assert!(w.charas.iter().all(|(c, _)| *c != 1006 && *c != 1068));
        assert!(w.charas.windows(2).all(|x| x[0].1 >= x[1].1));
    }

    /// No finished umas: two starter careers, each for a pink her own grades make likely, paired.
    #[test]
    fn starter_route_without_finished_umas() {
        let kb = kb();
        // Kitasan Black [Gilded Shrine to Glory] for Late (C: 4 stars): two Late pinks, 2★ each.
        let goal = Goal { runner: 106801, targets: vec![8], ..Default::default() };
        let p = propose(&kb, &goal, &inv(vec![], vec![(105201, 3), (101901, 3)]));
        assert_eq!(p.steps.len(), 2, "{p:?}");
        assert!(p.steps.iter().all(|s| s.legacies == [0, 0]));
        assert!(p.after.as_ref().is_some_and(Lineage::meets));
    }

    #[test]
    fn wants_read_sparks() {
        let kb = kb();
        let own = vec![spark(&kb, "pink", "Dirt", 2), spark(&kb, "blue", "Stamina", 1), green_spark(105201, 2)];
        assert!(Want::Pink { aptitude: 1, stars: 2 }.met(&own));
        assert!(!Want::Pink { aptitude: 1, stars: 3 }.met(&own));
        assert!(!Want::Blue { stat: 1, stars: 2 }.met(&own) && Want::Blue { stat: 1, stars: 1 }.met(&own));
        assert!(Want::Green { uma: 105201, stars: 2 }.met(&own));
        assert_eq!(Want::Pink { aptitude: 1, stars: 2 }.label(&kb), "Dirt pink 2★+");
    }
}

/// The planner on the real account for many runners, courses and styles, with timings:
/// `cargo test --release parentage_real_account -- --ignored --nocapture`, with UMALORD_ROOT at
/// the account's folder (or the app's default one).
#[cfg(test)]
#[test]
#[ignore]
fn parentage_real_account() {
    let kb = super::tests::kb();
    let root = std::env::var_os("UMALORD_ROOT").map(std::path::PathBuf::from).or_else(|| crate::data::find_root(None)).unwrap();
    let vets = crate::veterans::load(&root);
    let (roster, _) = crate::data::Roster::load(Some(&root));
    let owned: Vec<(u32, u8)> = roster.umas.iter().map(|u| (u.id, u.stars)).collect();
    let inv = Inventory::new(&kb, &vets, owned.clone());
    eprintln!("{} finished umas, {} owned; history ss {:.2}", vets.len(), owned.len(), inv.history.ss);
    let filter = std::env::var("GOALS").unwrap_or_default();
    for &(runner, _) in &owned {
        let info = &kb.umas[&runner];
        for surface in [0, 1] {
            for distance in 2..6 {
                for style in 6..10 {
                    let goal = Goal { runner, targets: vec![surface, distance, style], ..Default::default() };
                    let name = format!("{} {} {} {}", super::uma_name(&kb, runner), APTITUDES[surface], APTITUDES[distance], APTITUDES[style]);
                    if !filter.is_empty() && !filter.split(';').any(|f| name.contains(f)) {
                        continue;
                    }
                    let t = std::time::Instant::now();
                    let p = propose(&kb, &goal, &inv);
                    let ms = t.elapsed().as_millis();
                    let _ = info;
                    // Invariants: a route ends in a lineage that meets the goal; no legacy is its
                    // trainee's character; the cap holds unless the answer says it doesn't.
                    if !p.steps.is_empty() {
                        assert!(p.after.as_ref().is_some_and(Lineage::meets), "{name}: route doesn't meet the goal");
                        for s in &p.steps {
                            assert!(chara(s.trainee) != chara(runner), "{name}: runner's own character as a step");
                            for id in s.legacies.iter().filter(|id| **id != FROM_STEP && **id != 0) {
                                assert!(inv.by_id(*id).is_some_and(|l| l.chara() != chara(s.trainee)), "{name}: a step's legacy is her own character");
                            }
                            assert!(p.over_cap || s.careers() <= CAP, "{name}: past the cap unmarked");
                        }
                        if let Some(w) = p.partner.and_then(|id| inv.by_id(id)) {
                            assert!(w.chara() != chara(runner) && w.chara() != chara(p.steps[0].trainee), "{name}: bad partner");
                        }
                    }
                    if filter.is_empty() {
                        let kind = if !p.impossible.is_empty() { "impossible".to_string() } else if p.now.as_ref().is_some_and(Lineage::meets) { "met".into() } else if p.steps.is_empty() { "none".into() } else { format!("{} step(s) {:.1} careers{}", p.steps.len(), p.careers(), if p.over_cap { " OVER" } else { "" }) };
                        eprintln!("{ms:>5} ms  {name}: {kind}");
                    } else {
                        eprintln!("==== {name} ({ms} ms)\n{}\n", super::text::proposal(&kb, &goal, &inv, &p, 2.0));
                    }
                }
            }
        }
    }
}

/// Goals that also want an inherited unique: each owned runner at her best course and style,
/// wanting the green of several other owned umas. Every route must end with that green.
#[cfg(test)]
#[test]
#[ignore]
fn parentage_real_account_greens() {
    let kb = super::tests::kb();
    let root = std::env::var_os("UMALORD_ROOT").map(std::path::PathBuf::from).or_else(|| crate::data::find_root(None)).unwrap();
    let vets = crate::veterans::load(&root);
    let (roster, _) = crate::data::Roster::load(Some(&root));
    let owned: Vec<(u32, u8)> = roster.umas.iter().map(|u| (u.id, u.stars)).collect();
    let inv = Inventory::new(&kb, &vets, owned.clone());
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    for (n, &(runner, _)) in owned.iter().enumerate() {
        let r = ranks(&kb.umas[&runner].aptitude);
        let best = |range: std::ops::Range<usize>| range.max_by_key(|i| r[*i]).unwrap();
        let targets = vec![best(0..2), best(2..6), best(6..10)];
        for k in 1..=4 {
            let (green, _) = owned[(n + k * 7) % owned.len()];
            if chara(green) == chara(runner) {
                continue;
            }
            let goal = Goal { runner, targets: targets.clone(), green: Some(green), stats: [0, 21, 0, 0, 0], ..Default::default() };
            let p = propose(&kb, &goal, &inv);
            let has = inv.parents.iter().any(|x| x.green.is_some_and(|(u, _)| u == green));
            let kind = if p.now.as_ref().is_some_and(Lineage::meets) {
                "met"
            } else if p.steps.is_empty() {
                "none"
            } else if p.over_cap {
                "over"
            } else {
                assert!(p.after.as_ref().is_some_and(|l| l.green && l.meets()), "{}: route without the green", super::uma_name(&kb, runner));
                if !has {
                    assert!(p.steps.iter().any(|s| s.trainee == green && s.must.contains(&Want::Green { uma: green, stars: 1 })), "{}: no step makes the green", super::uma_name(&kb, runner));
                }
                if has { "planned, green owned" } else { "planned, green made" }
            };
            *tally.entry(kind).or_default() += 1;
            if n % 9 == 0 && k == 1 {
                eprintln!("==== {} wanting {}\n{}\n", super::uma_name(&kb, runner), super::uma_name(&kb, green), super::text::proposal(&kb, &goal, &inv, &p, 2.0));
            }
        }
    }
    eprintln!("{tally:?}");
}
