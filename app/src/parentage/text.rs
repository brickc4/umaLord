//! The planner's answers as text for the assistant (and, until its page is designed, for the
//! player through it). Odds are labelled calculated: they come from the game's rules and the
//! published spark tables, not from a simulation.

use super::jobs::{CAP, Proposal, Run};
use super::score::Lineage;
use super::track::ParentPlan;
use super::{APTITUDES, Goal, Inventory, Parent, STATS, green_spark, parent_name, uma_name};
use crate::affinity::symbol;
use crate::knowledge::Knowledge;
use crate::veterans::Veteran;

const GRADES: [&str; 9] = ["?", "G", "F", "E", "D", "C", "B", "A", "S"];

fn pct(p: f64) -> String {
    if p >= 0.995 { "~100%".into() } else { format!("{:.1}%", p * 100.0) }
}

pub fn goal(kb: &Knowledge, g: &Goal) -> String {
    let mut out = format!("{}: {} at A", uma_name(kb, g.runner), g.targets.iter().map(|i| APTITUDES[*i]).collect::<Vec<_>>().join(", "));
    if let Some(u) = g.green {
        out += &format!(", inheriting {}", kb.sparks.get(&green_spark(u, 1)).map_or_else(|| uma_name(kb, u), |s| s.name.clone()));
    }
    let stats: Vec<String> = g.stats.iter().enumerate().filter(|(_, v)| **v > 0).map(|(i, v)| format!("{} +{v}", STATS[i])).collect();
    if !stats.is_empty() {
        out += &format!("; wanted at the start: {}", stats.join(", "));
    }
    if !g.whites.is_empty() {
        out += &format!("; whites worth inheriting: {}", g.whites.iter().map(|k| white_name(kb, *k)).collect::<Vec<_>>().join(", "));
    }
    if !g.label.is_empty() {
        out += &format!(" ({})", g.label);
    }
    out
}

fn white_name(kb: &Knowledge, key: u32) -> String {
    (1..=3).find_map(|s| kb.sparks.get(&(key * 100 + s))).map_or_else(|| format!("spark {key}"), |s| s.name.clone())
}

/// One lineage: its two legacies, affinity, start aptitudes for the targets, stats, uniques and
/// wanted whites.
pub fn lineage(kb: &Knowledge, parents: &[&Parent], l: &Lineage) -> String {
    let names: Vec<String> = parents.iter().map(|p| parent_name(kb, p)).collect();
    let mut out = format!("{} — lineage affinity {} {} (slots {}). Start: ", names.join(" + "), l.affinity, symbol(l.affinity), l.slots.map(|s| s.to_string()).join("/"));
    out += &l
        .targets
        .iter()
        .map(|(i, before, after, stars)| {
            let mark = if *after >= 7 { "✔" } else { "⚠ below A" };
            if before == after { format!("{} {} {mark}", APTITUDES[*i], GRADES[*before as usize]) } else { format!("{} {}→{} ({stars} pink stars) {mark}", APTITUDES[*i], GRADES[*before as usize], GRADES[*after as usize]) }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let stats: Vec<String> = l.blues.iter().enumerate().filter(|(_, v)| **v > 0).map(|(i, v)| format!("{} +{v}", STATS[i])).collect();
    if !stats.is_empty() {
        out += &format!(". Stats {}", stats.join(", "));
    }
    let greens: Vec<String> = parents.iter().filter_map(|p| p.green).filter_map(|(u, s)| kb.sparks.get(&green_spark(u, s)).map(|x| format!("{} {s}★", x.name))).collect();
    if !greens.is_empty() {
        out += &format!(". Uniques {}", greens.join(", "));
    }
    if !l.whites.is_empty() {
        out += &format!(". Whites (fire at least once) {}", l.whites.iter().map(|(k, p)| format!("{} {}", white_name(kb, *k), pct(*p))).collect::<Vec<_>>().join(", "));
    }
    if !l.green {
        out += ". ⚠ The wanted unique isn't a legacy's green";
    }
    out + "."
}

/// A parent by id: a finished uma, a friend's, or the uma step 1 leaves.
fn name_id(kb: &Knowledge, inv: &Inventory, id: u64) -> String {
    if id == super::jobs::FROM_STEP {
        return "the uma step 1 leaves".into();
    }
    inv.by_id(id).map_or(format!("#{id}"), |p| parent_name(kb, p))
}

fn step(kb: &Knowledge, inv: &Inventory, n: usize, r: &Run) -> String {
    let legacies: Vec<String> = r.legacies.iter().map(|id| name_id(kb, inv, *id)).collect();
    let labels = |ws: &[super::jobs::Want]| ws.iter().map(|w| w.label(kb)).collect::<Vec<_>>().join(", ");
    let mut out = if r.legacies == [0, 0] {
        format!("Step {n}: a career of {} with any two legacies (yours or a friend's; best without pinks that raise her other aptitudes to A).\n", uma_name(kb, r.trainee))
    } else {
        format!("Step {n}: a career of {} with {} as her legacies.\n", uma_name(kb, r.trainee), legacies.join(" and "))
    };
    if r.legacies.iter().any(|id| super::is_borrowed(*id)) {
        out += "  The borrowed legacy has to be borrowed again for every career of this step, while the friend still lists her.\n";
    }
    out += &format!("  Must have: {}.", if r.must.is_empty() { "nothing beyond finishing it (her parents bring what's needed)".into() } else { labels(&r.must) });
    if !r.nice.is_empty() {
        let nice: Vec<String> = r.nice.iter().map(|w| w.odds(&inv.history).map_or_else(|| w.label(kb), |p| format!("{} ({} a career)", w.label(kb), pct(p)))).collect();
        out += &format!(" Nice to have: {}.", nice.join(", "));
    }
    out += "\n";
    if !r.g1s.is_empty() {
        out += &format!(
            "  G1s to win: {} (each adds 3 affinity to every link with a parent, or the uma she'll pair with, that won it; a G1 win may also leave a race spark, 1 in 5).\n",
            r.g1s.iter().map(|(n, c)| format!("{n} (+{})", 3 * u32::from(*c))).collect::<Vec<_>>().join(", ")
        );
    }
    if !r.stat_aims.is_empty() {
        out += &format!("  Stats to aim for: {} at 1100 or more (its blue spark comes 2★ or better 80% of the time, 55% below).\n", r.stat_aims.iter().map(|i| STATS[*i]).collect::<Vec<_>>().join(", "));
    }
    if r.p < 1.0 {
        let (typical, unlucky) = super::verify::step_spread(r.p);
        out += &format!(
            "  {} careers hit ({}, {}; best case {}). With the 30 TP spark reroll, usually {typical} career{}; 1 in 10 players need {unlucky} or more.{}\n",
            super::verify::one_in(r.p),
            pct(r.p),
            super::verify::word(r.p),
            super::verify::one_in(r.p_best),
            if typical == 1 { "" } else { "s" },
            if r.careers() > CAP {
                " ⚠ Past the cap: even the typical case is long.".to_string()
            } else if unlucky > CAP as u32 {
                format!(" ⚠ The unlucky 1 in 10 go past {CAP:.0} careers; a borrow or another trainee is worth a look first.")
            } else {
                String::new()
            }
        );
    }
    out
}

pub fn proposal(kb: &Knowledge, goal_: &Goal, inv: &Inventory, p: &Proposal, per_day: f64) -> String {
    let mut out = format!("Goal: {}.\n", goal(kb, goal_));
    if !p.impossible.is_empty() {
        return out + &format!("{} more than four grades below A for her: no lineage starts it at A (pink stars raise four grades at most). Pick a runner with a better grade there.", p.impossible.iter().map(|i| APTITUDES[*i]).collect::<Vec<_>>().join(" and "));
    }
    match &p.now {
        Some(l) => {
            let ps: Vec<&Parent> = l.legacies.iter().map(|i| &inv.parents[*i]).collect();
            out += &format!("Best lineage from your finished umas: {}\n", lineage(kb, &ps, l));
            if l.meets() {
                return out + "It already meets the goal: train the runner with these two.";
            }
        }
        None => out += "No two of your finished umas can be her legacies yet.\n",
    }
    // What the best lineage lacks, per target.
    let have = |i: usize| p.now.as_ref().and_then(|l| l.targets.iter().find(|t| t.0 == i)).map_or(0, |t| t.3);
    let short: Vec<String> = p.need.iter().filter(|(i, n)| have(*i) < *n).map(|(i, n)| format!("{} needs {n} pink stars in the lineage and the best has {}", APTITUDES[*i], have(*i))).collect();
    if !short.is_empty() {
        out += &format!("Short of A at the start: {}.\n", short.join("; "));
    }
    if p.steps.is_empty() {
        out += "No route from the umas you own reaches it: borrow a friend's legacy with the missing pinks, pick another runner, or accept a lower grade.";
        return out + &borrow_text(kb, goal_, inv);
    }
    out += &format!("\nPlan (calculated from the game's spark rules; keep running each step until a finished uma has its must-haves; odds use the Wiki's grade-weighted pink rule, uma.guide's even rule as the best case; at most {CAP:.0} careers a step):\n");
    for (i, r) in p.steps.iter().enumerate() {
        out += &step(kb, inv, i + 1, r);
    }
    if let Some(l) = &p.after {
        let names: Vec<String> = match p.partner {
            Some(w) => vec![format!("step {}'s product", p.steps.len()), name_id(kb, inv, w)],
            None => vec!["the two steps' products".into()],
        };
        out += &format!("Then the runner's lineage: {} — affinity {} {}, every target at A at the start.\n", names.join(" + "), l.affinity, symbol(l.affinity));
    }
    let total = p.careers();
    out += &format!("About {total:.1} careers in all, {:.1} days at {per_day} parent careers a day.", total / per_day.max(0.1));
    let v = super::verify::verify(&p.steps, None, 4000, 12345.0);
    let [lo, mid, hi] = v.cautious.pctl;
    out += &format!(" Half the time it takes {mid} or fewer, nine times in ten {hi} or fewer (at least {lo}); with the best-case rule, {} and {}.", v.best.pctl[1], v.best.pctl[2]);
    if p.over_cap {
        out += &format!("\n⚠ Nothing fits under {CAP:.0} careers a step: this is the cheapest route anyway. Better options: borrow a friend's legacy with the missing pinks, pick another runner, or accept a lower grade.");
        out += &borrow_text(kb, goal_, inv);
    }
    if !p.alternatives.is_empty() {
        out += "\nOther first steps: ";
        let must = |r: &Run| if r.must.is_empty() { "just finish it".to_string() } else { r.must.iter().map(|w| w.label(kb)).collect::<Vec<_>>().join(", ") };
        out += &p.alternatives.iter().map(|r| format!("{} ({}, about {:.1} careers)", uma_name(kb, r.trainee), must(r), r.careers())).collect::<Vec<_>>().join("; ");
        out += ".";
    }
    out
}

/// What to look for among friends' legacies, in plain words.
pub fn borrow_text(kb: &Knowledge, goal: &Goal, inv: &Inventory) -> String {
    let Some(w) = super::jobs::borrow_wish(kb, goal, inv) else { return String::new() };
    let mut want: Vec<String> = w.stars.iter().map(|(i, n)| format!("{n} {} pink star{}", APTITUDES[*i], if *n == 1 { "" } else { "s" })).collect();
    if let Some(u) = w.green {
        want.push(format!("{} as her own green", kb.sparks.get(&green_spark(u, 1)).map_or_else(|| uma_name(kb, u), |s| s.name.clone())));
    }
    let names: Vec<String> = w.charas.iter().map(|(c, _)| kb.umas.values().find(|u| crate::affinity::chara(u.id) == *c).map_or(format!("chara {c}"), |u| u.name.clone())).collect();
    if !w.possible {
        return format!(
            "\nNo single friend's legacy can bring it either: {} needs more than three pinks, and a legacy line has three (her own and her two parents', 3★ at most). Pick a runner with better grades here, or a lower goal.",
            want.join(" and ")
        );
    }
    format!(
        "\nTo borrow instead: pair {} with a friend's legacy carrying {} (counting her and her two parents). Characters with the most affinity here: {}. Add the one you find and the plan uses her.",
        name_id(kb, inv, w.partner),
        if want.is_empty() { "nothing more".into() } else { want.join(" and ") },
        names.join(", ")
    )
}

/// The saved plan: its steps, where it stands, and the finished umas that count for the
/// current step.
pub fn plan(kb: &Knowledge, plan: &ParentPlan, inv: &Inventory, vets: &[Veteran]) -> String {
    let mut out = format!("Parent plan (made {}): {}.\n", plan.made, goal(kb, &plan.goal));
    if let Some(why) = &plan.blocked {
        out += &format!("⚠ Waiting for your choice: {why}. Pick another route: another runner or goal, a friend's legacy, or the cheapest route past the cap if you accept it.\n");
    }
    if plan.paused {
        out += "⏸ Paused: imports aren't checked against it until you resume.\n";
    }
    for (i, s) in plan.steps.iter().enumerate() {
        let short = if s.settled { " (settled for, short of its must-haves)" } else { "" };
        match (s.done, &s.declared) {
            (Some(id), _) => out += &format!("Step {}: done with #{id}{short}.\n", i + 1),
            (None, Some(own)) => {
                let got: Vec<String> = own.iter().filter_map(|f| kb.sparks.get(f)).map(|x| format!("{} {}★", x.name, x.stars)).collect();
                out += &format!("Step {}: checked off by hand ({}){short}, waiting for the import to confirm her.\n", i + 1, got.join(", "));
            }
            (None, None) => out += &step(kb, inv, i + 1, &s.run),
        }
    }
    if let Some(w) = plan.partner {
        out += &format!("Then the last step's product pairs with {} as the runner's legacies.\n", name_id(kb, inv, w));
    }
    if plan.blocked.is_some() {
        return out;
    }
    if !plan.log.is_empty() {
        let n = plan.log.len();
        out += &format!("History{}: {}\n", if n > 5 { " (latest 5)" } else { "" }, plan.log[n.saturating_sub(5)..].join(" · "));
    }
    let Some(cur) = plan.current() else {
        return out + "Every step is done: train the runner with the lineage the plan ends in.";
    };
    let hits = plan.hits(vets);
    let run = &plan.steps[cur].run;
    if hits.is_empty() {
        out += &format!("No finished {} since step {} began. Import after each career (Settings → Profile → Import finished umas) and the step is checked.", uma_name(kb, run.trainee), cur + 1);
    } else {
        out += &format!("Step {} so far: {} career{} kept since it began (careers whose uma you released don't show).\n", cur + 1, hits.len(), if hits.len() == 1 { "" } else { "s" });
        if !hits.iter().any(|h| h.must) && run.p < 1.0 {
            let going = super::verify::still_going(run.p, hits.len() as u32);
            out += &format!(
                "Where you stand: {:.0} in 10 players are still going after {} careers here. The next career has the same {} chance; luck doesn't build up.{}\n",
                going * 10.0,
                hits.len(),
                super::verify::one_in(run.p),
                if going < 0.1 { " You're in the unlucky 1 in 10: settling for your best one, or another trainee, may cost less than carrying on." } else { "" }
            );
        }
        for h in &hits {
            let nice: Vec<String> = h.nice.iter().map(|i| run.nice[*i].label(kb)).collect();
            out += &format!(
                "- #{}: {}{}{}\n",
                h.id,
                if h.must { "has the must-haves" } else { "short of the must-haves" },
                if nice.is_empty() { String::new() } else { format!(", plus {}", nice.join(", ")) },
                if h.same_parents { "" } else { " (ran with other legacies, so its parents bring different sparks)" }
            );
        }
        out += if hits.iter().any(|h| h.must) { "Keep the one that meets them and move on." } else { "Keep running, or settle for one and the plan is worked out again around it." };
    }
    out
}

/// The line an import adds when a plan is open.
pub fn import_note(kb: &Knowledge, plan: &ParentPlan, vets: &[Veteran]) -> Option<String> {
    if plan.paused || plan.blocked.is_some() {
        return None;
    }
    let cur = plan.current()?;
    let hit = plan.hits(vets).into_iter().find(|h| h.must)?;
    let must = &plan.steps[cur].run.must;
    let what = if must.is_empty() { "finishes it".to_string() } else { format!("has its must-haves ({})", must.iter().map(|w| w.label(kb)).collect::<Vec<_>>().join(", ")) };
    Some(format!("Parent plan: #{} {what}, so step {} can end. Keep it and move on?", hit.id, cur + 1))
}

/// A Champions Meeting team: the CM and the budget, each runner, then the parent queue.
pub fn team(kb: &Knowledge, t: &super::team::CmTeam, inv: &Inventory) -> String {
    let e = &t.event;
    let mut out = format!(
        "CM {} {} — {}, {} ({}), {} days away. Parent careers there's time for: {:.0}; the queue needs about {:.1}.\n",
        e.id,
        e.name,
        e.course_name,
        e.date,
        if e.announced { "announced" } else { "estimated" },
        t.days_left,
        t.budget,
        t.needed
    );
    if !t.today.is_empty() {
        let today: Vec<String> = t.today.iter().map(|x| format!("#{} {} ({}{})", x.vet, uma_name(kb, x.outfit), x.style.label(), x.share.map_or(String::new(), |s| format!(", wins {:.0}%", s * 100.0)))).collect();
        out += &format!("What you can field today (your finished umas as they are, best first): {}.\n", today.join(", "));
    }
    for (i, r) in t.runners.iter().enumerate() {
        let lineage = if r.parents.now.as_ref().is_some_and(Lineage::meets) {
            "lineage ready ✔".to_string()
        } else if r.parents.steps.is_empty() || r.parents.over_cap {
            "⚠ no lineage under the cap".to_string()
        } else {
            format!("{} parent step{}, ~{:.1} careers", r.parents.steps.len(), if r.parents.steps.len() == 1 { "" } else { "s" }, r.parents.careers())
        };
        let deck: Vec<String> = r.plan.deck.iter().flatten().map(|c| kb.cards.get(c).map_or(c.to_string(), |c| c.uma.clone())).collect();
        let friend = r.plan.friend.and_then(|c| kb.cards.get(&c)).map_or("none".to_string(), |c| format!("{} [{}] (MLB)", c.uma, c.title));
        let skills: Vec<String> = r.plan.skills.iter().take(6).filter_map(|s| kb.skills.get(s).map(|k| k.name.clone())).collect();
        let mut sim = r.sim.map_or("not raced".to_string(), |(s, lo, hi)| format!("head to head against her two teammates she wins {:.0}% (95% {:.0}–{:.0}%, simulation)", s * 100.0, lo * 100.0, hi * 100.0));
        if let Some((surv, spurt)) = r.stamina {
            sim += &format!("; she survives {:.0}% of races and reaches full spurt in {:.0}%", surv * 100.0, spurt * 100.0);
        }
        if let (Some(v), Some(ace)) = (r.vs_today, t.today.first()) {
            sim += &format!("; against today's best (#{} {}) she wins {:.0}% ({:+.0} points over even)", ace.vet, uma_name(kb, ace.outfit), v * 100.0, (v - 0.5) * 100.0);
        }
        out += &format!(
            "Runner {}: {} ({}){}: {lineage}. Deck: {}; borrow {friend}. Skills to buy: {}{}. {sim}.\n",
            i + 1,
            uma_name(kb, r.trainee),
            r.style.label(),
            if r.meta_pick { " ★ meta pick" } else { "" },
            deck.join(", "),
            skills.join(", "),
            if r.plan.skills.len() > 6 { format!(" and {} more", r.plan.skills.len() - 6) } else { String::new() }
        );
        for n in &r.notes {
            out += &format!("  {n}\n");
        }
    }
    if t.queue.is_empty() {
        out += "No parent careers needed: train the three.\n";
    } else {
        out += "Parent queue (harder first):\n";
        for (n, (i, run)) in t.queue.iter().enumerate() {
            out += &format!("{}. For {}: {}", n + 1, uma_name(kb, t.runners[*i].trainee), step(kb, inv, n + 1, run).split_once(": ").map_or(String::new(), |x| x.1.to_string()));
        }
    }
    for n in &t.notes {
        out += &format!("⚠ {n}\n");
    }
    out
}

/// A Team Trials team: each race's runners with their estimated points, the Aces, the total,
/// and the career that raises it most.
pub fn tt(kb: &Knowledge, t: &super::tt::TtTeam) -> String {
    let mut out = format!("Team Trials team, about {:.0} points a round of the five races (estimate). Community meta read {}{}.\n", t.total, t.meta_read, if t.meta_age.1 { " (may be stale)" } else { "" });
    if t.today_total > 0.0 {
        out += &format!("What you can field today (your finished umas as they are) scores about {:.0}; raced together, the plan beats it by about {:+.0}.\n", t.today_total, t.gain);
    }
    for r in &t.rows {
        out += &format!("{} ({} style{}):\n", r.name, r.styles, if r.styles == 1 { "" } else { "s" });
        for s in &r.slots {
            let lineage = if s.parents.now.as_ref().is_some_and(Lineage::meets) {
                "lineage ready".to_string()
            } else if s.parents.steps.is_empty() || s.parents.over_cap {
                "no lineage under the cap".to_string()
            } else {
                format!("{} parent step(s), ~{:.1} careers", s.parents.steps.len(), s.parents.careers())
            };
            let friend = s.plan.friend.and_then(|c| kb.cards.get(&c)).map_or("none".to_string(), |c| format!("{} (MLB)", c.uma));
            out += &format!(
                "  {}{} ({}): ~{:.0} points (placement {:.0}, skills {:.0}){}; {lineage}; borrow {friend}.\n",
                if s.ace { "★ Ace " } else { "" },
                uma_name(kb, s.trainee),
                s.style.label(),
                s.total,
                s.placement,
                s.skills,
                match (s.natural, s.parents.now.as_ref().is_some_and(Lineage::meets)) {
                    (true, _) => "",
                    (false, true) => ", B here: her best two parents, which you have, lift her to A",
                    (false, false) => ", B here: her parents must lift her to A",
                }
            );
            for n in &s.notes {
                out += &format!("      {n}\n");
            }
        }
        if !r.today.is_empty() {
            let today: Vec<String> = r.today.iter().map(|(x, pts)| format!("#{} {} ({}) ~{:.0}", x.vet, uma_name(kb, x.outfit), x.style.label(), pts)).collect();
            out += &format!("  Today: {}{}.\n", today.join(", "), r.gain.map_or(String::new(), |g| format!("; the plan gains {g:+.0} here")));
        }
        for n in &r.notes {
            out += &format!("  ⚠ {n}\n");
        }
    }
    out += &format!("Next career: {}\n", t.next);
    for n in &t.notes {
        out += &format!("{n}\n");
    }
    out
}
