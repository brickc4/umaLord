//! What a lineage gives a trainee, so the assistant never does spark arithmetic by hand: the
//! fixed start (aptitudes raised by the six umas' pink stars, stats added by their blue sparks,
//! the legacies' inherited uniques) and each spark's odds at the two April inspirations. The
//! rules are in `knowledge/mechanics.md` (Sparks and inheritance) and `knowledge/parenting.md`.

use crate::knowledge::{Aptitude, Knowledge, SparkInfo, grade_rank};

const GRADES: [&str; 8] = ["G", "F", "E", "D", "C", "B", "A", "S"];

/// Pink spark name → the aptitude it raises.
fn aptitude<'a>(a: &'a Aptitude, pink: &str) -> Option<&'a str> {
    Some(match pink {
        "Turf" => &a.turf,
        "Dirt" => &a.dirt,
        "Sprint" => &a.sprint,
        "Mile" => &a.mile,
        "Medium" => &a.medium,
        "Long" => &a.long,
        "Front Runner" => &a.front,
        "Pace Chaser" => &a.pace,
        "Late Surger" => &a.late,
        "End Closer" => &a.end,
        _ => return None,
    })
}

/// The grade after the start: 1–3 pink stars raise it one grade, 4–6 two, 7–9 three, 10+ four,
/// never past A (an S stays S).
pub fn after_start(grade: &str, stars: u32) -> &'static str {
    let rank = grade_rank(grade).max(1);
    if rank >= 7 || stars == 0 {
        return GRADES[rank as usize - 1];
    }
    let steps = stars.div_ceil(3).min(4) as u8;
    GRADES[(rank + steps).min(7) as usize - 1]
}

/// Stats a blue spark adds at the start.
pub fn blue_start(stars: u8) -> u32 {
    match stars {
        1 => 5,
        2 => 12,
        3 => 21,
        _ => 0,
    }
}

/// A spark's chance to fire at one inspiration with 0 affinity, by kind and stars.
pub fn base_chance(kind: &str, stars: u8) -> f64 {
    let i = usize::from(stars.clamp(1, 3)) - 1;
    match kind {
        "blue" => [0.70, 0.80, 0.90][i],
        // 1★ is 1% on uma.guide's Sparks page and IceDynamix's sheet, 2% in its Parenting guide.
        "pink" => [0.01, 0.03, 0.05][i],
        "green" => [0.05, 0.10, 0.15][i],
        "race" => [0.01, 0.02, 0.03][i],
        "white" | "scenario" => [0.03, 0.06, 0.09][i],
        _ => 0.0,
    }
}

/// One of the six umas: who, which of the six, its sparks and the affinity its sparks roll with.
pub struct Slot<'a> {
    pub label: String,
    pub legacy: bool,
    pub sparks: Vec<&'a SparkInfo>,
    pub affinity: u32,
}

/// Chance a spark fires at least once over the two inspirations.
pub fn over_both(kind: &str, stars: u8, affinity: u32) -> f64 {
    let p = (base_chance(kind, stars) * (1.0 + f64::from(affinity) / 100.0)).min(1.0);
    1.0 - (1.0 - p) * (1.0 - p)
}

fn pct(p: f64) -> String {
    if p >= 0.995 { "~100%".into() } else { format!("{:.1}%", p * 100.0) }
}

/// The plain-text report: start aptitudes (with the target's three marked), start stats, inherited
/// uniques, then each slot's pink, green and white odds.
pub fn report(kb: &Knowledge, trainee: u32, slots: &[Slot], target: &[&str]) -> String {
    let Some(info) = kb.umas.get(&trainee) else { return "Unknown trainee.".into() };
    let mut out = format!("Trainee: {} [{}].\n\nAt the start (fixed, nothing rolled):\n", info.name, info.outfit);
    // Pink stars by aptitude across all six.
    let mut pinks: std::collections::BTreeMap<&str, u32> = Default::default();
    let mut blues: std::collections::BTreeMap<&str, u32> = Default::default();
    for s in slots.iter().flat_map(|s| s.sparks.iter()) {
        match s.kind.as_str() {
            "pink" => *pinks.entry(s.name.as_str()).or_default() += u32::from(s.stars),
            "blue" => *blues.entry(s.name.as_str()).or_default() += blue_start(s.stars),
            _ => {}
        }
    }
    let names = ["Turf", "Dirt", "Sprint", "Mile", "Medium", "Long", "Front Runner", "Pace Chaser", "Late Surger", "End Closer"];
    let mut lines = Vec::new();
    for n in names {
        let Some(base) = aptitude(&info.aptitude, n) else { continue };
        let stars = pinks.get(n).copied().unwrap_or(0);
        let is_target = target.contains(&n);
        if stars == 0 && !is_target {
            continue;
        }
        let after = after_start(base, stars);
        let flag = if is_target { if grade_rank(after) >= 7 { " ✔ target" } else { " ⚠ target below A" } } else { "" };
        let needed = if is_target && grade_rank(after) < 7 {
            let grades = 7 - grade_rank(base).max(1);
            if grades > 4 {
                " (more than four grades below A: the start can't reach A)".to_string()
            } else {
                format!(" (A needs {}+ {n} pink stars in the lineage)", 3 * u32::from(grades) - 2)
            }
        } else {
            String::new()
        };
        lines.push(format!("- {n}: {base} → {after} ({stars} pink star{} in the lineage){flag}{needed}", if stars == 1 { "" } else { "s" }));
    }
    out += &if lines.is_empty() { "- No pink sparks in the lineage.\n".to_string() } else { lines.join("\n") + "\n" };
    if !blues.is_empty() {
        out += &format!("- Stats added: {}.\n", blues.iter().map(|(k, v)| format!("{k} +{v}")).collect::<Vec<_>>().join(", "));
    }
    let greens: Vec<String> = slots.iter().filter(|s| s.legacy).flat_map(|s| s.sparks.iter().filter(|x| x.kind == "green").map(move |x| format!("{} (from {})", x.name, s.label))).collect();
    if !greens.is_empty() {
        out += &format!("- Inherited uniques: {}.\n", greens.join(", "));
    }
    out += "\nAt the two April inspirations (each spark rolls base × (1 + slot affinity/100); shown as the chance it fires at least once over both):\n";
    for s in slots {
        let pink: Vec<String> = s.sparks.iter().filter(|x| x.kind == "pink").map(|x| format!("{} {}★ {}", x.name, x.stars, pct(over_both("pink", x.stars, s.affinity)))).collect();
        let green: Vec<String> = s.sparks.iter().filter(|x| x.kind == "green").map(|x| format!("{} {}★ {}", x.name, x.stars, pct(over_both("green", x.stars, s.affinity)))).collect();
        let whites: Vec<&&SparkInfo> = s.sparks.iter().filter(|x| matches!(x.kind.as_str(), "white" | "race" | "scenario")).collect();
        let expected: f64 = whites.iter().map(|x| 2.0 * (base_chance(&x.kind, x.stars) * (1.0 + f64::from(s.affinity) / 100.0)).min(1.0)).sum();
        out += &format!(
            "- {} (affinity {}): pink {}; green {}; {} white sparks, about {:.2} hints expected.\n",
            s.label,
            s.affinity,
            if pink.is_empty() { "none".into() } else { pink.join(", ") },
            if green.is_empty() { "none".into() } else { green.join(", ") },
            whites.len(),
            expected
        );
    }
    out += "\nAn A aptitude only reaches S through a pink proc at an inspiration.";
    out
}

/// Star odds (1★, 2★, 3★) for a blue spark of a stat that finished at `value`.
pub fn blue_star_odds(value: u32) -> [f64; 3] {
    if value >= 1100 {
        [0.20, 0.70, 0.10]
    } else if value >= 600 {
        [0.45, 0.50, 0.05]
    } else {
        [0.90, 0.10, 0.0]
    }
}

/// Star odds for white and green sparks by rank: SS or better, or below.
pub fn rank_star_odds(ss: bool) -> [f64; 3] {
    if ss { [0.20, 0.70, 0.10] } else { [0.45, 0.50, 0.05] }
}

fn careers(p: f64) -> String {
    if p <= 0.0 {
        "never".into()
    } else {
        let with_reroll = 1.0 - (1.0 - p) * (1.0 - p);
        format!("about {:.1} careers (with the 30 TP spark reroll, {:.1})", 1.0 / p, 1.0 / with_reroll)
    }
}

/// What one parent career of `uma` makes: her pink (even among the aptitudes she finishes at A or
/// S, `a_aptitudes`), her blue (stat even among five, stars by its final value), her green (3★ or
/// more only; stars by rank), with the odds of each at 2★ or better and the careers to expect.
pub fn parent_run_report(name: &str, stars: u8, a_aptitudes: &[&str], stats: [u32; 5], ss: bool) -> String {
    let mut out = format!("A parent career of {name} (stars {stars}, finishing {} rank):\n", if ss { "SS or better" } else { "below SS" });
    if a_aptitudes.is_empty() {
        out += "- Pink: she has no A or S aptitude, so her pink comes from her B-or-lower grades (the Umamusume Wiki weights them; uma.guide says only A/S count): not a way to make a chosen pink.\n";
    } else {
        let n = a_aptitudes.len() as f64;
        out += &format!(
            "- Pink: one of the {} aptitudes she finishes at A or S ({}), each 1 in {} at best (one source also lets her B and C aptitudes in, and inspirations or events can raise more to A, so this is an upper bound); at 1★/2★/3★ 20/70/10%. A given one at 2★+: {:.1}% a career, {}. At 3★: {:.1}%.\n",
            a_aptitudes.len(),
            a_aptitudes.join(", "),
            a_aptitudes.len(),
            80.0 / n,
            careers(0.8 / n),
            10.0 / n
        );
    }
    let names = ["Speed", "Stamina", "Power", "Guts", "Wit"];
    let blues: Vec<String> = names
        .iter()
        .zip(stats)
        .map(|(n, v)| {
            let o = blue_star_odds(v);
            format!("{n} (finishing {v}) 2★+ {:.1}%, 3★ {:.1}%", 20.0 * (o[1] + o[2]), 20.0 * o[2])
        })
        .collect();
    let chosen_3 = if stats.iter().any(|v| *v >= 1100) { format!(" A 3★ of one chosen stat that finishes at 1100+: 2% a career, {}.", careers(0.02)) } else { String::new() };
    out += &format!("- Blue: one stat, each 1 in 5, stars by that stat's final value: {}.{chosen_3}\n", blues.join("; "));
    if stars >= 3 {
        let o = rank_star_odds(ss);
        out += &format!("- Green (her unique): always made; 2★+ {:.0}%, 3★ {:.0}%. 2★+: {}.\n", 100.0 * (o[1] + o[2]), 100.0 * o[2], careers(o[1] + o[2]));
    } else {
        out += "- Green: none (only umas at 3★ or more make a green spark).\n";
    }
    let o = rank_star_odds(ss);
    out += &format!("- White: each learned skill makes its spark 20% (white), 25% (◎) or 40% (gold), more with each copy already in her lineage (about 35 / 40 / 70% at six; sources differ on the steps); stars {:.0}/{:.0}/{:.0}% for 1★/2★/3★ at this rank.", 100.0 * o[0], 100.0 * o[1], 100.0 * o[2]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_thresholds() {
        assert_eq!(after_start("B", 1), "A", "one star raises a grade");
        assert_eq!(after_start("C", 3), "B");
        assert_eq!(after_start("C", 4), "A");
        assert_eq!(after_start("G", 12), "C", "four grades at most (the wiki's example)");
        assert_eq!(after_start("E", 30), "A", "never past A");
        assert_eq!(after_start("A", 9), "A");
        assert_eq!(after_start("S", 1), "S");
        assert_eq!(after_start("D", 0), "D");
        assert_eq!(blue_start(3), 21);
    }

    #[test]
    fn inspiration_odds() {
        // A 2★ pink at 100 affinity: 6% a roll, 11.64% over both.
        assert!((over_both("pink", 2, 100) - (1.0 - 0.94f64 * 0.94)).abs() < 1e-9);
        assert!(over_both("blue", 3, 200) > 0.99, "capped at certain");
        assert_eq!(base_chance("bonus", 1), 0.0);
    }

    #[test]
    fn parent_run_odds() {
        let r = parent_run_report("Haru Urara", 5, &["Dirt", "Sprint", "Late Surger"], [1200, 300, 800, 450, 400], true);
        assert!(r.contains("each 1 in 3") && r.contains("26.7% a career"), "{r}");
        assert!(r.contains("Speed (finishing 1200) 2★+ 16.0%, 3★ 2.0%"), "{r}");
        assert!(r.contains("2★+ 80%"), "{r}");
        assert!(parent_run_report("x", 2, &["Turf"], [0; 5], false).contains("Green: none"));
        let none = parent_run_report("x", 3, &[], [900; 5], true);
        assert!(none.contains("no A or S aptitude") && !none.contains("1 in 0") && !none.contains("chosen stat"), "{none}");
    }

    #[test]
    fn report_marks_targets() {
        let kb = Knowledge::load(crate::data::find_root(None).as_deref());
        let dirt = kb.sparks.values().find(|s| s.kind == "pink" && s.name == "Dirt" && s.stars == 1).unwrap();
        let speed = kb.sparks.values().find(|s| s.kind == "blue" && s.name == "Speed" && s.stars == 3).unwrap();
        // Oguri Cap [Starlight Beat]: dirt B, so one Dirt star makes it A.
        let slots = [Slot { label: "Legacy 1".into(), legacy: true, sparks: vec![dirt, speed], affinity: 100 }];
        let r = report(&kb, 100601, &slots, &["Dirt", "Mile", "End Closer"]);
        assert!(r.contains("Dirt: B → A (1 pink star in the lineage) ✔ target"), "{r}");
        assert!(r.contains("Speed +21"), "{r}");
        assert!(r.contains("End Closer: ") && r.contains("⚠ target below A"), "{r}");
    }
}
