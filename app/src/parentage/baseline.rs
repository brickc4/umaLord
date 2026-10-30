//! What the player can field today: the best team from their finished umas as they are (stats,
//! skills and aptitudes after inheritance, from the import), raced against the planned team on
//! the same course, so every plan shows its gain over what's already there.

use crate::knowledge::{Distance, Style, Surface};
use crate::sim::course::Course;
use crate::sim::data::SimData;
use crate::sim::params::{HorseDesc, RaceParams};
use crate::sim::run::{SimOptions, UmaBuild};
use crate::veterans::Veteran;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A finished uma in today's team.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Today {
    /// The game's id for this finished uma, shown as "#id".
    pub vet: u64,
    pub outfit: u32,
    pub style: Style,
    /// Her average win share against the others raced with her (none when she's the only one).
    pub share: Option<f64>,
}

/// The game's aptitude, 1 = G … 8 = S, is `grade_rank`'s scale.
fn rank(v: u8) -> u8 {
    v.min(8)
}

pub fn surface_grade(v: &Veteran, s: Surface) -> u8 {
    rank(match s {
        Surface::Turf => v.proper_ground_turf,
        Surface::Dirt => v.proper_ground_dirt,
    })
}

pub fn distance_grade(v: &Veteran, d: Distance) -> u8 {
    rank(match d {
        Distance::Sprint => v.proper_distance_short,
        Distance::Mile => v.proper_distance_mile,
        Distance::Medium => v.proper_distance_middle,
        Distance::Long => v.proper_distance_long,
    })
}

pub fn style_grade(v: &Veteran, s: Style) -> u8 {
    rank(match s {
        Style::Front => v.proper_running_style_nige,
        Style::Pace => v.proper_running_style_senko,
        Style::Late => v.proper_running_style_sashi,
        Style::End => v.proper_running_style_oikomi,
    })
}

/// Her best style; the one she raced with breaks a tie.
pub fn best_style(v: &Veteran) -> Style {
    let raced = match v.running_style {
        1 => Style::Front,
        2 => Style::Pace,
        4 => Style::End,
        _ => Style::Late,
    };
    [raced, Style::Front, Style::Pace, Style::Late, Style::End].into_iter().max_by_key(|s| (style_grade(v, *s), *s == raced)).unwrap_or(raced)
}

/// Whether she can run this race: B or better in its surface and distance.
pub fn fits(v: &Veteran, surface: Surface, distance: Distance) -> bool {
    surface_grade(v, surface) >= 6 && distance_grade(v, distance) >= 6
}

/// Her simulator build for a race, in her best style: her real stats and skills.
pub fn build(data: &SimData, v: &Veteran, surface: Surface, distance: Distance) -> Option<(UmaBuild, Style)> {
    let style = best_style(v);
    // The simulator's index: S = 0 … G = 7.
    let grade = |r: u8| 8 - usize::from(r.clamp(1, 8));
    let stars = v.rarity.clamp(1, 5);
    let unique = data.unique_skill(v.card_id, stars);
    let unique_level = v.skill_array.iter().find(|s| Some(s.skill_id) == unique).map_or(1, |s| s.level.max(1));
    let mut skills: Vec<u32> = unique.into_iter().collect();
    skills.extend(v.skill_array.iter().map(|s| s.skill_id).filter(|id| Some(*id) != unique && data.skills.contains_key(id)));
    let mut b = UmaBuild {
        outfit: Some(v.card_id),
        stars,
        unique_level,
        desc: HorseDesc {
            speed: f64::from(v.speed),
            stamina: f64::from(v.stamina),
            power: f64::from(v.power),
            guts: f64::from(v.guts),
            wisdom: f64::from(v.wiz),
            strategy: crate::tab_sim::strategy_of(style),
            distance_apt: grade(distance_grade(v, distance)),
            surface_apt: grade(surface_grade(v, surface)),
            strategy_apt: grade(style_grade(v, style)),
            mood: 2,
        },
        skills,
        forced: Default::default(),
    };
    b.normalise(data);
    Some((b, style))
}

/// Her finished umas that can run a race, strongest first by the game's rank score, one per
/// character (her best), at most `n`, skipping characters in `taken`.
pub fn candidates<'a>(vets: &'a [Veteran], surface: Surface, distance: Distance, taken: &[u32], n: usize) -> Vec<&'a Veteran> {
    let mut list: Vec<&Veteran> = vets.iter().filter(|v| v.card_id > 0 && fits(v, surface, distance) && !taken.contains(&crate::affinity::chara(v.card_id))).collect();
    list.sort_by(|a, b| b.rank_score.cmp(&a.rank_score));
    let mut seen = Vec::new();
    list.retain(|v| {
        let c = crate::affinity::chara(v.card_id);
        let fresh = !seen.contains(&c);
        seen.push(c);
        fresh
    });
    list.truncate(n);
    list
}

/// Race every pair of builds head to head: each one's average win share against the others,
/// and each skill's average fire rate.
pub fn race_pool(data: &SimData, course: &Arc<Course>, race: &RaceParams, builds: &[Option<UmaBuild>], opts: &SimOptions) -> Vec<(Option<f64>, BTreeMap<u32, f64>)> {
    let n = builds.len();
    let mut share = vec![Vec::new(); n];
    let mut rates: Vec<BTreeMap<u32, Vec<f64>>> = vec![Default::default(); n];
    for i in 0..n {
        for j in i + 1..n {
            let (Some(a), Some(b)) = (&builds[i], &builds[j]) else { continue };
            let res = crate::sim::run::run_comparison(data, course, race, a, b, opts, true, false, None);
            let runs = res.gaps.len().max(1) as f64;
            let b_share = res.gaps.iter().filter(|g| **g > 0.0).count() as f64 / runs;
            share[i].push(1.0 - b_share);
            share[j].push(b_share);
            for (k, u) in [(i, &res.umas[0]), (j, &res.umas[1])] {
                // The simulator reports rates as percentages.
                for s in &u.skills {
                    rates[k].entry(s.id).or_default().push(s.rate / 100.0);
                }
            }
        }
    }
    share
        .into_iter()
        .zip(rates)
        .map(|(s, r)| ((!s.is_empty()).then(|| s.iter().sum::<f64>() / s.len() as f64), r.into_iter().map(|(id, v)| (id, v.iter().sum::<f64>() / v.len() as f64)).collect()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vet() -> Veteran {
        Veteran {
            card_id: 100401,
            rarity: 3,
            running_style: 1,
            proper_ground_turf: 7,
            proper_ground_dirt: 4,
            proper_distance_mile: 7,
            proper_distance_middle: 6,
            proper_distance_short: 6,
            proper_distance_long: 3,
            proper_running_style_nige: 7,
            proper_running_style_senko: 4,
            ..Default::default()
        }
    }

    #[test]
    fn reads_the_games_aptitudes() {
        let v = vet();
        assert_eq!(surface_grade(&v, Surface::Turf), 7, "7 is A");
        assert_eq!(best_style(&v), Style::Front);
        assert!(fits(&v, Surface::Turf, Distance::Medium), "B in Medium runs it");
        assert!(!fits(&v, Surface::Turf, Distance::Long));
        assert!(!fits(&v, Surface::Dirt, Distance::Mile), "D on Dirt");
    }
}
