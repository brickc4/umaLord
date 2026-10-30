#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! umaLord: account-aware Uma Musume knowledge base, planner and results log.
//!
//! Usage: `umalord [path-to-umaLord-folder]`. Without a path it searches upward from the
//! current dir and the exe for `account/support_cards.md`.

mod affinity;
mod agent;
mod ask;
mod ask_claude_code;
mod ask_opencode;
mod career;
mod data;
mod charts;
mod collection_kit;
mod course_notes;
mod digest;
mod home;
mod knowledge;
mod lineage;
mod live;
mod meta;
mod notes;
mod onboarding;
mod onboarding_grid;
mod paste;
mod plan_sim;
mod profile;
mod recommend;
mod settings;
mod settings_page;
mod skill_opt;
mod skill_pick;
mod sim;
mod tab_career;
mod tab_cards;
mod tab_collection;
mod tab_overview;
mod tab_planner;
mod tab_results;
mod tab_sim;
mod tab_skills;
mod tab_umas;
mod theme;
mod ui_util;
mod veterans;

use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use data::{Roster, Source, SupportCard, Uma};
use eframe::egui::{self, CornerRadius, Margin, RichText, Stroke, vec2};
use knowledge::{Knowledge, SkillSource};
use ui_util::{semibold, tier_color};

/// Window icon: Special Week's portrait, embedded so the exe has it anywhere.
fn window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(include_bytes!("../assets/umas/100101.webp"), image::ImageFormat::WebP).ok()?;
    let rgba = img.into_rgba8();
    Some(egui::IconData { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
}

fn main() -> eframe::Result {
    // `umalord --simulate plans/<plan>.md`: race the plan (1000 runs, same setup as the Planner's
    // Simulate button), write its ## Simulation section, and exit without opening a window.
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--simulate") {
        let plan = args.get(1).map(PathBuf::from);
        let root = data::find_root(plan.as_ref().and_then(|p| p.parent().map(Path::to_path_buf)));
        let result = plan.ok_or_else(|| "usage: umalord --simulate <plan.md>".to_string()).and_then(|p| App::new(root).simulate_plan_file(&p));
        let (msg, code) = match result {
            Ok(m) => (m, 0),
            Err(e) => (e, 1),
        };
        // A windowed exe has no console; leave the outcome next to the plan as well.
        if let Some(p) = args.get(1) {
            let _ = std::fs::write(Path::new(p).with_extension("simulate.log"), &msg);
        }
        println!("{msg}");
        std::process::exit(code);
    }
    // `umalord --mcp [folder]`: the assistant's tools as an MCP server on stdin/stdout, for
    // OpenCode (the Ask tab starts it), Claude Code or Claude Desktop. No window.
    if args.first().is_some_and(|a| a == "--mcp") {
        let root = data::find_root(args.get(1).map(PathBuf::from));
        let stdin = std::io::stdin();
        let result = agent::mcp::serve(stdin.lock(), std::io::stdout(), move || agent::mcp::headless_app(root));
        std::process::exit(i32::from(result.is_err()));
    }
    // `umalord --import-veterans <data.json>`: import finished umas (UmaExtractor's format, or
    // tools/read_veterans.py's) into account/veterans.json and account/parents.md.
    if args.first().is_some_and(|a| a == "--import-veterans") {
        let result = args.get(1).map(PathBuf::from).ok_or_else(|| "usage: umalord --import-veterans <data.json>".to_string()).and_then(|p| {
            let root = data::find_root(None).ok_or("umaLord folder not found.")?;
            App::new(Some(root)).import_veterans(&p)
        });
        let (msg, code) = match result {
            Ok(m) => (m, 0),
            Err(e) => (e, 1),
        };
        println!("{msg}");
        std::process::exit(code);
    }
    // `umalord --export-profile [name]`: save the account to profiles/<name>-<date>.umalord.json.
    // `umalord --start-over [name]`: back the account up to profiles/backups/, then empty it.
    if let Some(cmd) = args.first().and_then(|a| a.to_str()).filter(|a| matches!(*a, "--export-profile" | "--start-over")) {
        let name = args.get(1).map(|a| a.to_string_lossy().to_string());
        let result = data::find_root(None).ok_or_else(|| "umaLord folder not found.".to_string()).and_then(|root| {
            let current = Roster::load(Some(&root)).0.profile.unwrap_or_else(|| "My account".into());
            if cmd == "--export-profile" {
                profile::export(&root, name.as_deref().unwrap_or(&current), &ui_util::local_today()).map(|p| format!("Saved {}", p.display()))
            } else {
                profile::start_over(&root, name.as_deref().unwrap_or(""), &profile::stamp())
                    .map(|b| b.map_or("Nothing to back up; the account is empty now.".into(), |p| format!("Backed up to {}; the account is empty now.", p.display())))
            }
        });
        let (msg, code) = match result {
            Ok(m) => (m, 0),
            Err(e) => (e, 1),
        };
        println!("{msg}");
        std::process::exit(code);
    }
    let root = data::find_root(args.first().map(PathBuf::from));
    // UMALORD_WINDOW=WxH overrides the start size (dev aid for full-page screenshots).
    let size = std::env::var("UMALORD_WINDOW")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(w, h)| Some([w.trim().parse().ok()?, h.trim().parse().ok()?])))
        .unwrap_or([1400.0, 900.0]);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("umaLord")
        // Wayland matches the window to umalord.desktop (and its icon) by this id; it ignores
        // with_icon.
        .with_app_id("umalord")
        .with_inner_size(size)
        .with_min_inner_size([960.0, 600.0]);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "umaLord",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            let app = App::new(root);
            // A profile shown with UMALORD_PROFILE isn't the account: keep the digest of the real one.
            if std::env::var_os("UMALORD_PROFILE").is_none() {
                app.write_digest();
            }
            theme::install(&cc.egui_ctx, app.mode);
            theme::set_compact(&cc.egui_ctx, app.compact);
            if (app.ui_scale - 1.0).abs() > 0.001 {
                cc.egui_ctx.set_zoom_factor(app.ui_scale);
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn app() -> App {
        App::for_tests()
    }

    /// Every Ask tool runs against the real files and returns something useful.
    #[test]
    fn ask_tools_answer_from_real_data() {
        let a = app();
        let ok = |name: &str, input: serde_json::Value| a.run_tool(name, &input).unwrap_or_else(|e| panic!("{name}: {e}"));
        let long = ok("find_umas", json!({"distance": "Long", "style": "Pace"}));
        assert!(long.contains("Narita Brian"), "{long}");
        let oguri = ok("get_uma", json!({"query": "Oguri Cap [Starlight Beat]"}));
        assert!(oguri.contains("Owned: yes, 5★, potential 5") && oguri.contains("Unique:"), "{oguri}");
        let kita = ok("get_card", json!({"query": "Kitasan Black"}));
        assert!(kita.contains("Owned: yes, LB4 Lv50") && kita.contains("Skill hints"), "{kita}");
        let skills = ok("search_skills", json!({"query": "corner", "owned_only": true}));
        assert!(skills.contains("[available to you]"), "{skills}");
        assert!(ok("get_skill", json!({"query": "Homestretch Haste"})).contains("Sources:"));
        assert!(ok("find_races", json!({"query": "Arima"})).contains("Nakayama"));
        assert!(ok("list_notes", json!({})).contains("insights.md"));
        assert!(ok("read_note", json!({"path": "insights.md"})).contains("Current opinions"));
        assert!(a.run_tool("read_note", &json!({"path": "../secret.md"})).is_err());
        // Two Fuji Kiseki outfits are owned, so the tool asks which one.
        let fuji = a.run_tool("get_uma", &json!({"query": "Fuji Kiseki"})).unwrap_err();
        assert!(fuji.contains("Shooting Star Revue") && fuji.contains("Succès Étoilé"), "{fuji}");
    }

    /// A saved plan's markdown carries its data block, which loads back into the same plan.
    /// Auto-pick always picks: without a course (so no skill simulation) it uses estimates and
    /// says why; with one it picks at once, then again from the simulated gains.
    #[test]
    fn auto_pick_picks_with_or_without_a_simulation() {
        let ctx = egui::Context::default();
        let mut a = app();
        a.planner.demo(&a.roster, &a.kb, 100601);
        assert!(a.plan_race_setup().course.is_none(), "the demo plan has no course");
        assert!(a.start_skill_values(&ctx, true).is_err(), "no course, so the skill simulation can't run");
        let msg = a.auto_pick_skills();
        assert!(!a.planner.plan.skills.is_empty(), "{msg}");
        let cm = a.kb.upcoming_cms("2026-10-17").first().copied().cloned().expect("an upcoming CM");
        a.planner.plan.apply_cm(&a.kb, &cm);
        a.planner.plan.skills.clear();
        a.start_skill_values(&ctx, true).unwrap();
        let start = std::time::Instant::now();
        while a.planner.skill_job.is_some() {
            assert!(start.elapsed().as_secs() < 120, "skill simulation never finished");
            a.planner_poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let (msg, warn) = a.planner.skill_status.clone().expect("a status line under Step 5");
        assert!(!warn && msg.contains("Picked"), "{msg}");
        assert!(!a.planner.plan.skills.is_empty(), "{msg}");
        assert!(a.chosen_cost(&a.planner.plan.skills, a.plan_career().as_ref()) <= a.sp_budget().unwrap() + 5.0);
        // ✨ Auto-pick: a quick pick at once, then the optimiser in the background.
        a.auto_pick(&ctx);
        assert!(a.planner.pick_job.is_some(), "{:?}", a.planner.skill_status);
        let start = std::time::Instant::now();
        while a.planner.pick_job.is_some() {
            assert!(start.elapsed().as_secs() < 120, "auto-pick never finished");
            a.pick_poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let (msg, warn) = a.planner.skill_status.clone().expect("a status line under Step 5");
        assert!(!warn && msg.contains("Picked"), "{msg}");
        let out = a.current_pick().expect("the result is for this plan").clone();
        assert_eq!(a.planner.plan.skills, out.chosen);
        assert!(out.verify.is_some() && !out.marginal.is_empty());
        assert!(a.chosen_cost(&a.planner.plan.skills, a.plan_career().as_ref()) <= a.pick_budget().unwrap() + 5.0);
    }

    /// For CM 21 the meta sheet's cards and skills come first.
    #[test]
    fn meta_sheet_picks_come_first() {
        let mut a = app();
        a.planner.demo(&a.roster, &a.kb, 100601); // Oguri Cap, Pace
        let cm = a.kb.umalator_presets.iter().find(|e| e.is_cm() && e.id == 21).cloned().expect("CM 21 preset");
        a.planner.plan.apply_cm(&a.kb, &cm);
        a.planner.plan.style = knowledge::Style::Pace;
        let label = a.plan_meta_label().expect("pvp_meta.json has CM 21");
        assert_eq!(label, "CM 21 Capricorn Cup · Pace");
        // Light Hello is in every CM 21 Pace deck.
        let meta = a.plan_card_meta();
        assert!(meta.owned.get(&30052).is_some_and(|(w, _)| meta::meta_rank(*w) == 3), "{:?}", meta.owned.get(&30052));
        // The sheet's owned Pace trainees for CM 21 lead the best fits.
        let first = a.trainee_candidates()[0];
        assert!(a.plan_meta_trainees().contains(&first), "best fit {first} isn't one of the sheet's");
        let picks = a.recommend_deck(true);
        for p in &picks {
            eprintln!("{:4.1} {:6.0} {} — {}", p.meta, p.score, a.kb.cards[&p.id].uma, p.reason);
        }
        // In every slot's ranking the sheet's picks come first, strongest backing first.
        for slot in 0..6 {
            let ranks: Vec<u8> = a.slot_candidates(slot).iter().map(|c| meta::meta_rank(c.meta)).collect();
            assert!(ranks.windows(2).all(|w| w[0] >= w[1]), "slot {slot}: {ranks:?}");
        }
        assert!(picks.iter().filter(|p| p.meta > 0.0).count() >= 4, "most of the deck should come from the sheet's CM 21 Pace decks");
        let skills = a.plan_meta_skills();
        let ids: Vec<u32> = a.plan_skill_candidates().into_keys().collect();
        let offered: Vec<u32> = ids.iter().copied().filter(|s| skills.get(s).is_some_and(|(w, _)| *w >= 1.0)).collect();
        // ★ skills come first only with the meta influence at Strong (the default is a tie-break).
        a.planner.plan.meta_influence = skill_opt::MetaInfluence::Strong;
        let chosen = a.recommend_skills(&ids);
        eprintln!("meta skills on offer: {:?}", offered.iter().map(|s| a.kb.skill_name(*s)).collect::<Vec<_>>());
        eprintln!("chosen: {:?}", chosen.iter().map(|s| a.kb.skill_name(*s)).collect::<Vec<_>>());
        for s in &offered {
            // Skills the simulator can't value (debuffs, position skills) are left to the user.
            if a.can_activate_on_plan(*s) != Some(false) && a.skill_unsimulated(*s).is_none() {
                let gold_or_white = chosen.iter().any(|c| c / 10 == s / 10);
                assert!(gold_or_white, "{} is recommended and on offer but wasn't picked", a.kb.skill_name(*s));
            }
        }
        // Off: the sheet doesn't count at all.
        a.planner.plan.meta_influence = skill_opt::MetaInfluence::Off;
        assert_eq!(a.recommend_skills(&ids), a.recommend_skills_with(&ids, 0.0));
    }

    #[test]
    fn plan_markdown_round_trips() {
        let mut a = app();
        a.planner.demo(&a.roster, &a.kb, 101601);
        a.planner.plan.title = "Test plan".into();
        let md = a.plan_markdown(None);
        for section in ["## Goal", "## Trainee", "## Deck", "## Skills to buy", "## Simulation", "## Recommendation"] {
            assert!(md.contains(section), "missing {section}");
        }
        let json = md.split("<!-- umalord-plan").nth(1).and_then(|t| t.split("-->").next()).unwrap();
        let back: tab_planner::Plan = serde_json::from_str(json).unwrap();
        assert_eq!(back.trainee, Some(101601));
        assert_eq!(back.deck, a.planner.plan.deck);
    }

    /// `umalord --simulate <plan>` races a saved plan and fills its ## Simulation section.
    #[test]
    fn simulates_a_saved_plan() {
        let mut a = app();
        a.planner.demo(&a.roster, &a.kb, 100601);
        let cm = a.kb.umalator_presets.iter().find(|e| e.course == 10506).cloned().expect("a Nakayama 2500 preset");
        a.planner.plan.apply_cm(&a.kb, &cm);
        a.planner.plan.skills = vec![200512];
        a.planner.plan.title = "Headless test".into();
        let path = std::env::temp_dir().join(format!("umalord-test-{}.md", std::process::id()));
        std::fs::write(&path, a.plan_markdown(None)).unwrap();
        let msg = app().simulate_plan_file(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(msg.contains("Gap"), "{msg}");
        assert!(text.contains("Gap, Uma 2 ahead of Uma 1 in lengths") && text.contains("https://umalator.app/#"), "{text}");
        assert!(text.contains("## Recommendation") && text.contains("<!-- umalord-plan"), "the rest of the plan stays");
        assert!(!text.contains("Not run yet"));
    }

    /// The Umalator build only carries skill ids Umalator knows.
    #[test]
    fn umalator_build_is_pasteable() {
        let mut a = app();
        a.planner.demo(&a.roster, &a.kb, 101601);
        // 300011 is a scenario skill that umalator.app doesn't have.
        a.planner.plan.skills = vec![200512, 101601111, 300011];
        let (json, dropped) = a.umalator_build().unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["outfitId"], "101601");
        assert_eq!(v["strategy"], "Senkou");
        let skills: Vec<&str> = v["skills"].as_array().unwrap().iter().filter_map(|s| s.as_str()).collect();
        assert!(skills.contains(&"200512"));
        assert!(skills.contains(&"101601111"), "umalator.app loads evolved skills");
        assert!(!skills.contains(&"300011"));
        assert_eq!(dropped.len(), 1);
    }

    /// The state inside a umalator.app share link.
    fn decode_link(link: &str) -> serde_json::Value {
        use std::io::Read;
        let hash = link.strip_prefix("https://umalator.app/#").expect("umalator.app link");
        let b64 = hash.replace("%2B", "+").replace("%2F", "/").replace("%3D", "=");
        const B64: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let sextets: Vec<u32> = b64.trim_end_matches('=').bytes().map(|c| B64.find(c as char).unwrap() as u32).collect();
        let mut gz = Vec::new();
        for q in sextets.chunks(4) {
            let n = q.iter().enumerate().fold(0, |n, (i, s)| n | s << (18 - 6 * i));
            gz.extend((0..q.len() - 1).map(|i| (n >> (16 - 8 * i)) as u8));
        }
        let mut text = String::new();
        flate2::read::GzDecoder::new(&gz[..]).read_to_string(&mut text).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// Frame times per tab, headless: `cargo test --release frame_times -- --ignored --nocapture`.
    /// The planner runs with a full recommended deck, the case that used to be slowest.
    #[test]
    #[ignore]
    fn frame_times() {
        let t0 = std::time::Instant::now();
        let mut a = app();
        eprintln!("App::new {:>7.1} ms", t0.elapsed().as_secs_f64() * 1e3);
        let ctx = egui::Context::default();
        theme::install(&ctx, theme::Mode::Dark);
        a.planner.demo(&a.roster, &a.kb, 101601);
        a.recommend_deck(true);
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, 900.0))),
            ..Default::default()
        };
        let pages = [
            (Tab::Home, None, None),
            (Tab::Collection, Some(CollectionTab::Umas), None),
            (Tab::Collection, Some(CollectionTab::Cards), None),
            (Tab::Collection, Some(CollectionTab::Skills), None),
            (Tab::Career, None, Some(CareerTab::Plan)),
            (Tab::Career, None, Some(CareerTab::Race)),
            (Tab::Career, None, Some(CareerTab::Results)),
            (Tab::Ask, None, None),
        ];
        for (tab, sub, career) in pages {
            a.tab = tab;
            if let Some(sub) = sub {
                a.collection_tab = sub;
            }
            if let Some(career) = career {
                a.career_tab = career;
            }
            let mut times = Vec::new();
            for _ in 0..6 {
                let t = std::time::Instant::now();
                let _ = crate::ui_util::test_frame(&ctx, input(), |ui| a.frame(ui));
                times.push(t.elapsed().as_secs_f64() * 1e3);
            }
            let steady = times[1..].iter().sum::<f64>() / (times.len() - 1) as f64;
            let name = sub.map(|s| format!("{s:?}")).or(career.map(|c| format!("{c:?}"))).unwrap_or(format!("{tab:?}"));
            eprintln!("{name:<10} first {:>7.1} ms · then {:>6.2} ms/frame", times[0], steady);
        }
        a.planner.plan.target[0] += 10;
        let t = std::time::Instant::now();
        let _ = a.plan_career();
        eprintln!("plan_career after a target edit {:>7.1} ms", t.elapsed().as_secs_f64() * 1e3);
        let t = std::time::Instant::now();
        let _ = a.slot_candidates(0);
        eprintln!("one slot ranking {:>7.1} ms", t.elapsed().as_secs_f64() * 1e3);
    }

    /// Searching a Collection list and then clearing the search brings the whole list back.
    #[test]
    fn clearing_a_search_restores_the_list() {
        let mut a = app();
        let ctx = egui::Context::default();
        theme::install(&ctx, theme::Mode::Dark);
        a.tab = Tab::Collection;
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, 900.0))),
            ..Default::default()
        };
        // Text drawn on screen, as a rough count of what the list shows.
        let texts = |a: &mut App| {
            let mut n = 0;
            for _ in 0..3 {
                let out = crate::ui_util::test_frame(&ctx, input(), |ui| a.frame(ui));
                n = out.shapes.iter().filter(|s| matches!(s.shape, egui::Shape::Text(_))).count();
            }
            n
        };
        for sub in [CollectionTab::Umas, CollectionTab::Cards, CollectionTab::Skills] {
            a.collection_tab = sub;
            let before = texts(&mut a);
            let set = |a: &mut App, q: &str| match sub {
                CollectionTab::Umas => a.umas.search = q.into(),
                CollectionTab::Cards => a.cards.search = q.into(),
                CollectionTab::Skills => a.skills.search = q.into(),
            };
            for q in ["o", "og", "ogu", "og", "o", ""] {
                set(&mut a, q);
                texts(&mut a);
            }
            let after = texts(&mut a);
            eprintln!("{sub:?}: {before} texts before, {after} after");
            assert_eq!(before, after, "{sub:?} list didn't come back after clearing the search");
        }
    }

    /// A plan saved against Umalator's guess (2026-10-25, course 10701, no cup name) still links
    /// to the Capricorn Cup once the official news moves it to 2026-10-31, and the skills that CM
    /// turns off can't activate and are never picked.
    #[test]
    fn old_plans_find_their_cm_after_live_data() {
        let mut a = app();
        let fixture = |n: &str| std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(n)).unwrap();
        a.live.data = Some(live::LiveData {
            announcements: live::parse_news(&fixture("news_sample.json")).unwrap(),
            umalator_presets: live::parse_umalator_presets(&fixture("umalator_presets_sample.js")),
            ..Default::default()
        });
        a.apply_live();
        a.planner.demo(&a.roster, &a.kb, 100601);
        let old: tab_planner::Plan = serde_json::from_str(r#"{"title":"old","cm_date":"2026-10-25","umalator_course":10701,"conditions":"Firm"}"#).unwrap();
        assert_eq!(old.cm_name, None, "plans saved before cm_name existed load");
        a.planner.plan.cm_date = old.cm_date;
        a.planner.plan.umalator_course = old.umalator_course;
        a.planner.plan.cm_name = None;
        let cm = a.plan_cm().cloned().expect("still linked");
        assert_eq!((cm.name.as_str(), cm.date.as_str(), cm.announced), ("Capricorn Cup", "2026-10-31", true));
        assert!(a.plan_disabled(202231), "Night Races ◎");
        assert_eq!(a.can_activate_on_plan(202231), Some(false));
        let picks = a.recommend_skills(&[202231, 202232, 202233, 200512]);
        assert!(picks.iter().all(|s| !cm.disabled_skills.contains(s)), "{picks:?}");
        a.planner.plan.apply_cm(&a.kb, &cm);
        assert_eq!(a.planner.plan.cm_name.as_deref(), Some("Capricorn Cup"));
        assert!(a.plan_markdown(None).contains("- Won't activate in this CM: Night Races ◎"));
        // Home shows it as the next CM in the week before.
        assert_eq!(a.kb.upcoming_cms("2026-10-20").first().map(|e| e.name.as_str()), Some("Capricorn Cup"));
        // And while it runs: Sagittarius started 2026-10-10 and ends 2026-10-16.
        assert_eq!(a.kb.upcoming_cms("2026-10-13").first().map(|e| e.name.as_str()), Some("Sagittarius Cup"));
    }

    /// The umalator.app link decodes back to both builds, the course and the plan's conditions.
    #[test]
    fn umalator_link_round_trips() {
        let mut a = app();
        a.planner.demo(&a.roster, &a.kb, 101601);
        // Mile Championship: Kyoto 1600m has inner and outer layouts.
        a.planner.plan.course = a.kb.races.iter().find(|r| r.name == "Mile Championship").map(|r| r.id);
        a.planner.plan.conditions = "Soft · Rainy".into();
        let (link, notes) = a.umalator_link().unwrap();
        let v = decode_link(&link);
        assert_eq!(v["uma1"]["outfitId"], "101601");
        assert_eq!(v["uma1"], v["uma2"]);
        assert!([10804, 10805].contains(&v["courseId"].as_u64().unwrap()), "a Kyoto 1600m course");
        assert!(notes.iter().any(|n| n.contains("layout")), "inner/outer is flagged: {notes:?}");
        assert_eq!((v["ground"].as_u64(), v["weather"].as_u64()), (Some(3), Some(3)));
        // An exact Umalator course id (as a CM preset sets) wins over the guess.
        a.planner.plan.umalator_course = Some(10805);
        let (link, notes) = a.umalator_link().unwrap();
        assert_eq!(decode_link(&link)["courseId"], 10805);
        assert!(!notes.iter().any(|n| n.contains("layout")), "{notes:?}");
    }

    /// Picking a Champions Meeting preset fills the goal, and Open Umalator uses its exact course.
    #[test]
    fn cm_preset_fills_the_plan() {
        let mut a = app();
        a.planner.demo(&a.roster, &a.kb, 101601);
        // Sagittarius Cup 2026-10-04: Nakayama Turf 2500m (inner), Good · Cloudy · Winter.
        let upcoming = a.kb.upcoming_cms("2026-10-01");
        let e = upcoming.first().copied().cloned().expect("CM presets in umalator.json");
        assert_eq!((e.name.as_str(), e.date.as_str(), e.course), ("Sagittarius Cup", "2026-10-04", 10506));
        assert!(upcoming.windows(2).all(|w| w[0].date <= w[1].date), "soonest first");
        assert!(upcoming.iter().all(|u| u.is_cm() && !u.course_name.is_empty() && !u.conditions.is_empty()));

        a.planner.plan.conditions = "Heavy · night race".into();
        a.planner.plan.distance = knowledge::Distance::Sprint;
        assert_eq!(a.planner.plan.apply_cm(&a.kb, &e), None);
        let p = &a.planner.plan;
        assert_eq!(p.conditions, "Good · Cloudy · Winter · night race", "chip format, free text kept");
        assert_eq!((p.distance, p.surface), (knowledge::Distance::Long, knowledge::Surface::Turf));
        assert_eq!((p.cm_date.as_deref(), p.umalator_course), (Some("2026-10-04"), Some(10506)));
        let race = a.kb.races.iter().find(|r| Some(r.id) == p.course).expect("a race on the CM course");
        assert_eq!((race.track.as_str(), race.distance, race.surface.as_str()), ("Nakayama", 2500, "Turf"));
        assert!(race.name.contains("Sagittarius Cup") && !race.name.contains("Finals"), "{}", race.name);
        // Picking it again doesn't toggle the conditions off.
        a.planner.plan.apply_cm(&a.kb, &e);
        assert_eq!(a.planner.plan.conditions, "Good · Cloudy · Winter · night race");

        let (link, notes) = a.umalator_link().unwrap();
        let v = decode_link(&link);
        assert_eq!(v["courseId"], 10506);
        assert_eq!((v["ground"].as_u64(), v["weather"].as_u64(), v["season"].as_u64(), v["time"].as_u64()), (Some(2), Some(2), Some(4), Some(2)));
        assert!(!notes.iter().any(|n| n.contains("layout")), "{notes:?}");
        assert!(a.plan_markdown(None).contains("- Champions Meeting: Sagittarius Cup (2026-10-04)"));

        // Plans saved before these fields existed still load.
        let old: tab_planner::Plan = serde_json::from_str(r#"{"title":"old","course":1023,"conditions":"Firm"}"#).unwrap();
        assert_eq!((old.cm_date, old.umalator_course, old.course), (None, None, Some(1023)));
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Tab {
    Home,
    /// Umas, support cards and skills, as sub-tabs.
    Collection,
    /// Planner, race sim and results, as sub-tabs.
    Career,
    Ask,
    /// The notes and data files: not in the top bar, opened from Settings (`App::open_notes`).
    Notes,
}

/// The Career tab's sub-tabs, in the order a career goes: plan it, race it, log it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum CareerTab {
    Plan,
    Race,
    Results,
}

impl CareerTab {
    /// Icon, label and what the page is for.
    pub(crate) fn info(self) -> (&'static str, &'static str, &'static str) {
        match self {
            CareerTab::Plan => ("📋", "Plan", "Plan a career step by step: goal, trainee, deck of support cards, and skills to buy."),
            CareerTab::Race => ("🏁", "Race sim", "Race two builds against each other many times to see which one wins, and by how much."),
            CareerTab::Results => ("🏆", "Results", "Write down what really happened in careers and events, so plans can be checked against it."),
        }
    }
}
impl Tab {
    /// Icon, label and what the page is for, as the top bar and Help show them.
    pub(crate) fn info(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Tab::Home => ("🏠", "Home", "Your stable at a glance, the next Champions Meeting, and shortcuts to common jobs."),
            Tab::Collection => ("📚", "Collection", "Everything you own: umas and support cards, plus every skill in the game and where you can get it."),
            Tab::Career => (
                "🏇",
                "Career",
                "Plan a career, race the build against others in the simulator, and log what really happened.",
            ),
            Tab::Ask => ("💬", "Ask", "Ask questions about your account in plain English and get answers from your own data."),
            Tab::Notes => ("📝", "Notes", "Read the notes, plans and insights saved in this folder."),
        }
    }
}

/// Game words the app uses, for the Help window. General terms only; numbers come from the data.
pub(crate) const GLOSSARY: &[(&str, &str)] = &[
    ("Uma, trainee", "A horse girl. You train one in each career; that uma is the trainee."),
    ("Outfit", "A version of an uma, like [Reeling in the Big One]. Each outfit has its own unique skill and aptitudes."),
    ("Stars ★", "An uma's star rank, 1 to 5. You raise it with shards, and it makes her stronger, including her unique skill."),
    ("Potential (P1–P5)", "Unlocks an uma's own skills in steps. At P5 all of them are available to buy."),
    ("Aptitude", "A letter grade, S down to G, for how well an uma runs on a surface (turf, dirt), a distance and a running style. A or better is what you want; lower grades slow her down."),
    ("Running style", "Front Runner leads from the start. Pace Chaser runs just behind the leaders. Late Surger waits mid-pack and moves up late. End Closer stays at the back and sprints at the end."),
    ("Stats", "Speed, Stamina, Power, Guts and Wit (wisdom). Training raises them during a career."),
    ("Growth", "A trainee's bonus to training gains in some stats, like Wit +20%."),
    ("Support card", "Up to six cards go with you into a career: five of yours and one borrowed. They boost training and teach skills."),
    ("Card types", "Speed, Stamina, Power, Guts and Wit cards help train that stat. Pal and Group cards work differently."),
    ("SSR, SR, R", "Card rarity, best first."),
    ("LB, limit break", "Merging copies of a card raises its level cap. LB0 is one copy; LB4, also called MLB, is fully limit broken."),
    ("MLB", "Max limit break (LB4): the card at full strength."),
    ("Card level", "Higher levels make a card's effects stronger, up to the cap its LB allows."),
    ("Friend card", "A card borrowed from another player for one career. It can be any card, at their LB."),
    ("Hint", "A card or event offering a skill at a discount. More hints for the same skill mean a bigger discount."),
    ("SP, skill points", "Earned during a career and spent on skills at the end."),
    ("White, gold skills", "Ordinary and stronger skills. A gold skill usually needs its white version first."),
    ("Unique skill", "Each outfit's own skill. As a parent, an uma can pass a weaker copy on."),
    ("Evolved skill", "A stronger version of a skill some umas can unlock during a career by meeting its conditions."),
    ("Sparks", "What a parent passes to the next trainee: blue for stats, pink for aptitudes, green for her unique skill, white for skills and races."),
    ("Parent", "A saved, finished uma used as a legacy for a new career. Two parents give their sparks."),
    ("Scenario", "The career's training mode, like URA Finale or Unity Cup. Each one trains differently."),
    ("Champions Meeting (CM)", "A monthly player-versus-player event on one course with set conditions."),
    ("Team Trials", "Races of your team of umas against other players' teams."),
    ("League of Heroes", "A ranked player-versus-player event."),
    ("Meta", "What most strong players currently run. The app's ★ marks come from a community sheet of PvP decks, umas and skills per Champions Meeting."),
    ("Lengths (L)", "How far ahead one uma finishes. One length is 2.5 m."),
    ("Full spurt", "Running the last stretch at top speed. It needs enough HP, which comes from Stamina."),
    ("Rushed", "When an uma gets over-eager mid-race and burns HP. Wit lowers the chance."),
    ("Conditions", "The ground (Firm to Heavy), weather and season of a race. Some skills only work in some conditions."),
];

/// Keyboard shortcuts, as Help lists them (`App::shortcuts` handles them).
pub(crate) const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+1 … Ctrl+4", "Switch tabs: Home, Collection, Career, Ask"),
    ("Ctrl+,", "Open Settings"),
    ("Ctrl+F", "Jump to the page's search box"),
    ("Ctrl+S", "Save the plan (on Career → Plan)"),
    ("F5", "Reload your roster, game data and plans from disk"),
    ("Esc", "Close Help, a dialog, or the details panel"),
    ("+", "Pulled another copy: one more LB or star for the open card or uma"),
    ("?", "Open this Help"),
];

/// The top bar's tabs, left to right. Notes isn't one: Settings → Data and updates opens it.
const TABS: [Tab; 4] = [Tab::Home, Tab::Collection, Tab::Career, Tab::Ask];

/// The Collection tab's sub-tabs.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum CollectionTab {
    Umas,
    Cards,
    Skills,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum View {
    Grid,
    Table,
}

/// Navigation requested from inside a borrowed UI closure; applied at the end of the frame.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Jump {
    Uma(u32),
    Card(u32),
    Skill(u32),
}

pub(crate) struct App {
    pub root: Option<PathBuf>,
    pub roster: Roster,
    pub source: Source,
    pub kb: Knowledge,
    pub card_art: HashSet<u32>,
    pub uma_art: HashSet<u32>,
    pub tab: Tab,
    pub collection_tab: CollectionTab,
    pub career_tab: CareerTab,
    pub view: View,
    pub jump: Cell<Option<Jump>>,
    /// Skills the account can get now; rebuilt after the roster changes.
    pub available_skills: std::cell::RefCell<Option<HashSet<u32>>>,
    /// The Help window is open.
    pub help_open: bool,
    /// The sky gradient and the theme's decorations behind the pages; `mascot` in settings.json
    /// (the name it had when it also drew Seiun Sky). Where she shows is `theme::chibi()`.
    pub mascot: bool,
    /// Dark, light or System; saved as `theme` in settings.json. `dark` is what it resolves to.
    pub mode: theme::Mode,
    /// Zoom for the whole window (0.9–1.25); `ui_scale` in settings.json.
    pub ui_scale: f32,
    /// Compact spacing; `density` in settings.json.
    pub compact: bool,
    /// Toasts asked for this frame, shown by `App::frame` (code without a `Context` can ask too).
    pub pending_toasts: std::cell::RefCell<Vec<(String, ui_util::ToastKind, Option<(String, ui_util::ToastAction)>)>>,
    /// Ctrl+F was pressed: the page's search box takes focus when it's drawn.
    pub focus_search: Cell<bool>,
    pub umas: tab_umas::UmasState,
    pub cards: tab_cards::CardsState,
    pub skills: tab_skills::SkillsState,
    pub planner: tab_planner::PlannerState,
    pub results: tab_results::ResultsState,
    pub notes: tab_results::NotesState,
    /// The tab to go back to from the notes page.
    pub notes_back: Tab,
    pub sim: tab_sim::SimState,
    /// Moomoolator's data for the native simulator (`knowledge/data/sim/`).
    pub sim_data: Option<std::sync::Arc<sim::data::SimData>>,
    pub sim_error: Option<String>,
    pub ask: ask::AskState,
    /// The assistant's undo snapshots and pending page (`agent/`).
    pub agent: agent::AgentState,
    pub md_cache: egui_commonmark::CommonMarkCache,
    /// Dark mode (default) or light; saved in settings.json.
    pub dark: bool,
    /// The colour theme (accent and background); saved in settings.json.
    pub look: theme::Look,
    /// The Settings window.
    pub settings: settings_page::SettingsState,
    /// The live Champions Meeting schedule (official news and Umalator), refreshed in the background.
    pub live: live::LiveState,
    /// Home's derived data and daily picks.
    pub home: home::HomeState,
    /// UMALORD_RAIL: start with the Collection filters folded, for screenshots.
    pub debug_rail: bool,
    /// UMALORD_EXTRA: after a Compare, also run "what each skill is worth" and "where training points matter".
    pub debug_extra: bool,
    /// UMALORD_REPLAY: the replay's position once the run finishes.
    pub debug_replay: Option<f32>,
    /// First-run setup, and Settings → Profile's actions.
    pub onboarding: onboarding::OnboardingState,
    screenshot_to: Option<PathBuf>,
    screenshot_requested: bool,
    debug_question: Option<String>,
    pub debug_asked: bool,
    /// When an approval card first showed (UMALORD_APPROVE=ask).
    debug_approval_at: Option<f64>,
    debug_install: bool,
    /// `UMALORD_SCROLL=<px>`: hold the planner scrolled this far down, for screenshots.
    pub debug_scroll: Option<f32>,
    /// `UMALORD_RUN=1`: start the Race sim run (or the planner's skill simulation) on launch.
    debug_run: bool,
    /// `UMALORD_HOVER=<id>`: draw that setup tile as if the mouse were on it, for screenshots.
    pub debug_hover: Option<u32>,
}

/// The frame every page's central panel uses: see-through, with the same margins everywhere.
pub(crate) fn page_frame() -> egui::Frame {
    egui::Frame::new().inner_margin(Margin { left: 18, right: 18, top: 14, bottom: 8 })
}

/// The frame of a page's side panel (filters, lists): see-through, a hairline on its inner edge
/// comes from the panel itself.
pub(crate) fn side_frame() -> egui::Frame {
    egui::Frame::new().inner_margin(Margin { left: 14, right: 12, top: 14, bottom: 8 })
}

fn load_sim(root: Option<&Path>) -> (Option<std::sync::Arc<sim::data::SimData>>, Option<String>) {
    let Some(root) = root else { return (None, Some("umaLord folder not found".into())) };
    match sim::data::SimData::load(root) {
        Ok(d) => (Some(std::sync::Arc::new(d)), None),
        Err(e) => (None, Some(format!("Simulator data missing: {e}"))),
    }
}

/// IDs of the `.webp` files in `<root>/app/assets/<sub>/`.
fn scan_art(root: Option<&Path>, sub: &str) -> HashSet<u32> {
    let Some(root) = root else { return HashSet::new() };
    std::fs::read_dir(root.join("app").join("assets").join(sub))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.path().file_stem()?.to_str()?.parse().ok())
        .collect()
}

impl App {
    fn new(root: Option<PathBuf>) -> Self {
        // A new install or a fresh clone has no account files yet: start them from the templates.
        if let Some(Err(e)) = root.as_deref().map(profile::ensure_account) {
            eprintln!("{e}");
        }
        let (roster, source) = Roster::load(root.as_deref());
        let env = |key: &str| std::env::var(key).ok();
        let (sim_data, sim_error) = load_sim(root.as_deref());
        let saved = |key: &str| settings::get(key).and_then(|v| v.as_str().map(String::from));
        let mascot = env("UMALORD_MASCOT").map(|v| v == "1").or_else(|| settings::get("mascot").and_then(|v| v.as_bool())).unwrap_or(true);
        let mode = theme::Mode::from_key(&env("UMALORD_THEME").or_else(|| saved("theme")).unwrap_or_default());
        // Before `chibi` existed, `mascot: false` hid her too; keep her hidden for those users.
        let chibi_default = if settings::get("mascot").and_then(|v| v.as_bool()) == Some(false) { theme::Chibi::Off } else { theme::Chibi::Home };
        theme::use_chibi(env("UMALORD_CHIBI").or_else(|| saved("chibi")).and_then(|k| theme::Chibi::from_key(&k)).unwrap_or(chibi_default));
        let ui_scale = env("UMALORD_SCALE")
            .and_then(|s| s.parse::<f32>().ok())
            .or_else(|| settings::get("ui_scale").and_then(|v| v.as_f64()).map(|v| v as f32))
            .unwrap_or(1.0)
            .clamp(*theme::UI_SCALE.start(), *theme::UI_SCALE.end());
        let compact = env("UMALORD_DENSITY").or_else(|| saved("density")).as_deref() == Some("compact");
        let mut app = Self {
            sim: Default::default(),
            sim_data,
            sim_error,
            card_art: scan_art(root.as_deref(), "cards"),
            uma_art: scan_art(root.as_deref(), "umas"),
            kb: Knowledge::load(root.as_deref()),
            ask: ask::AskState::new(root.as_deref()),
            agent: Default::default(),
            planner: tab_planner::PlannerState::new(root.as_deref()),
            notes: tab_results::NotesState::default(),
            notes_back: Tab::Home,
            root,
            roster,
            source,
            tab: Tab::Home,
            collection_tab: CollectionTab::Umas,
            career_tab: CareerTab::Plan,
            view: View::Grid,
            jump: Cell::new(None),
            available_skills: std::cell::RefCell::new(None),
            help_open: false,
            mascot,
            mode,
            ui_scale,
            compact,
            pending_toasts: Default::default(),
            focus_search: Cell::new(false),
            umas: Default::default(),
            cards: Default::default(),
            skills: Default::default(),
            results: Default::default(),
            md_cache: Default::default(),
            // System resolves once the window knows Windows' theme (`App::frame`).
            dark: mode != theme::Mode::Light,
            look: theme::Look::from_key(&env("UMALORD_LOOK").or_else(|| settings::get("look").and_then(|v| v.as_str().map(String::from))).unwrap_or_default()),
            onboarding: Default::default(),
            settings: Default::default(),
            live: Default::default(),
            home: Default::default(),
            debug_rail: false,
            debug_extra: false,
            debug_replay: None,
            screenshot_to: env("UMALORD_SCREENSHOT").map(PathBuf::from),
            screenshot_requested: false,
            debug_question: env("UMALORD_ASK").filter(|q| !q.trim().is_empty()),
            debug_asked: false,
            debug_approval_at: None,
            debug_install: env("UMALORD_INSTALL").is_some_and(|v| v == "1"),
            debug_scroll: env("UMALORD_SCROLL").and_then(|s| s.parse().ok()),
            debug_run: env("UMALORD_RUN").is_some_and(|v| v == "1"),
            debug_hover: env("UMALORD_HOVER").and_then(|s| s.parse().ok()),
        };
        // The live CM schedule: the cache now, a refresh on the first frame if it's stale. Tests
        // stay on the bundled data, so they don't depend on the network or this PC's cache.
        let live_mode = if cfg!(test) { Some("off".to_string()) } else { env("UMALORD_LIVE") };
        app.live_start(live_mode.as_deref());
        app.tab = match env("UMALORD_TAB").as_deref() {
            Some("umas" | "cards" | "skills" | "collection") => Tab::Collection,
            Some("planner" | "sim" | "results" | "career") => Tab::Career,
            Some("notes") => Tab::Notes,
            Some("ask") => Tab::Ask,
            _ => Tab::Home,
        };
        app.collection_tab = match env("UMALORD_TAB").as_deref() {
            Some("cards") => CollectionTab::Cards,
            Some("skills") => CollectionTab::Skills,
            _ => CollectionTab::Umas,
        };
        app.career_tab = match env("UMALORD_TAB").as_deref() {
            Some("sim") => CareerTab::Race,
            Some("results") => CareerTab::Results,
            _ => CareerTab::Plan,
        };
        app.help_open = env("UMALORD_HELP").is_some_and(|v| v == "1");
        // UMALORD_SETTINGS=1 opens Settings; UMALORD_SETTINGS_PAGE=<section> picks the section.
        if env("UMALORD_SETTINGS").is_some_and(|v| v == "1") {
            let page = env("UMALORD_SETTINGS_PAGE").and_then(|k| settings_page::SettingsPage::from_key(&k)).unwrap_or_default();
            app.open_settings(page);
        }
        theme::use_look(app.look);
        // A new install (or after Start over): setup opens by itself. UMALORD_SETUP=<step> forces a step.
        if app.roster.is_empty() && matches!(app.source, Source::Folder(_)) {
            app.open_setup(onboarding::Step::Welcome);
        }
        let step = match env("UMALORD_SETUP").as_deref() {
            Some("welcome") => Some(onboarding::Step::Welcome),
            Some("import") => Some(onboarding::Step::Import),
            Some("you") => Some(onboarding::Step::You),
            Some("umas") => Some(onboarding::Step::Umas),
            Some("cards") => Some(onboarding::Step::Cards),
            Some("done") => Some(onboarding::Step::Done),
            Some(_) => {
                app.onboarding.open = false;
                None
            }
            None => None,
        };
        if let Some(step) = step {
            app.open_setup(step);
        }
        // UMALORD_PROFILE=<file.umalord.json>: show that profile's roster without importing it
        // (for checking a profile, and for screenshots after Start over).
        if let Some(path) = env("UMALORD_PROFILE") {
            match profile::read(Path::new(&path)) {
                Ok(p) => {
                    let get = |k: &str| p.files.get(k).cloned().unwrap_or_default();
                    app.roster = Roster::parse(&get("account/support_cards.md"), &get("account/umas.md"));
                    app.onboarding.open = step.is_some();
                }
                Err(e) => eprintln!("{e}"),
            }
        }
        // UMALORD_ADD=<card id>: open the Cards tab's Add cards dialog with that card picked.
        if let Some(id) = env("UMALORD_ADD").and_then(|s| s.parse::<u32>().ok()) {
            app.tab = Tab::Collection;
            app.collection_tab = CollectionTab::Cards;
            let mut a = std::mem::take(&mut app.cards.add);
            a.open = true;
            app.pick_add_card(&mut a, id);
            app.cards.add = a;
        }
        if env("UMALORD_VIEW").as_deref() == Some("table") {
            app.view = View::Table;
        }
        // UMALORD_COMPARE=<card id>,<card id>: open the Compare cards window.
        if let Some(ids) = env("UMALORD_COMPARE") {
            for id in ids.split(',').filter_map(|s| s.trim().parse::<u32>().ok()) {
                app.toggle_compare(id);
            }
        }
        // UMALORD_COURSE=cm: the Skills course filter on the next Champions Meeting.
        if env("UMALORD_COURSE").as_deref() == Some("cm")
            && let Some(e) = app.kb.upcoming_cms(&ui_util::today()).first().copied().cloned()
        {
            app.skills.course = Some(tab_skills::CourseCheck {
                course: e.course,
                conditions: [e.ground, e.weather, e.season, e.time],
                cm: Some(e.name.clone()),
                disabled: e.disabled_skills.clone(),
                label: format!("the {}", e.name),
            });
        }
        // UMALORD_RESULTS=overview|race|skills|stamina|runs: the Race sim dashboard's tab.
        if let Some(t) = env("UMALORD_RESULTS") {
            app.sim.result_tab = match t.as_str() {
                "race" => tab_sim::ResultTab::Race,
                "skills" => tab_sim::ResultTab::Skills,
                "stamina" => tab_sim::ResultTab::Stamina,
                "runs" => tab_sim::ResultTab::Runs,
                _ => tab_sim::ResultTab::Overview,
            };
        }
        // UMALORD_EXTRA=1: also run the Skills tab's two background runs after the Compare.
        app.debug_extra = env("UMALORD_EXTRA").as_deref() == Some("1");
        // UMALORD_REPLAY=<seconds>: hold the replay at that point of the median race.
        app.debug_replay = env("UMALORD_REPLAY").and_then(|s| s.parse::<f32>().ok());
        // UMALORD_RAIL=1: fold the Collection filters into the rail (in memory only).
        if env("UMALORD_RAIL").as_deref() == Some("1") {
            app.debug_rail = true;
        }
        if let Some(id) = env("UMALORD_SELECT").and_then(|s| s.parse::<u32>().ok()) {
            match (app.tab, app.career_tab) {
                (Tab::Collection, _) if app.collection_tab == CollectionTab::Skills => app.skills.selected = Some(id),
                (Tab::Career, CareerTab::Plan) => {
                    app.planner.demo(&app.roster, &app.kb, id);
                    if app.debug_run
                        && let Some(cm) = app.kb.upcoming_cms(&ui_util::today()).first().copied().cloned()
                    {
                        app.planner.plan.apply_cm(&app.kb, &cm);
                    }
                    app.recommend_deck(true);
                    if let Some(slot) = env("UMALORD_PICKER").and_then(|s| s.parse().ok()) {
                        app.planner.open_picker(slot);
                    }
                }
                (Tab::Career, CareerTab::Race) => {
                    // The demo plan for this trainee, with its upcoming CM and auto-picked skills.
                    app.planner.demo(&app.roster, &app.kb, id);
                    if let Some(cm) = app.kb.upcoming_cms(&ui_util::today()).first().copied().cloned() {
                        app.planner.plan.apply_cm(&app.kb, &cm);
                    }
                    app.recommend_deck(true);
                    let ids: Vec<u32> = app.plan_skill_candidates().into_keys().collect();
                    app.planner.plan.skills = app.recommend_skills(&ids);
                    if let Err(e) = app.load_plan_into_sim() {
                        eprintln!("{e}");
                    }
                }
                _ if id >= 100_000 => {
                    app.tab = Tab::Collection;
                    app.collection_tab = CollectionTab::Umas;
                    app.umas.selected = Some(id);
                }
                _ => {
                    app.tab = Tab::Collection;
                    app.collection_tab = CollectionTab::Cards;
                    app.cards.selected = Some(id);
                }
            }
        }
        // UMALORD_SHOP=1: the planner's skill shop mode, with the demo plan's stats, 1800 SP and its
        // first dozen skills on offer at hint levels 0–3.
        if env("UMALORD_SHOP").as_deref() == Some("1") {
            let offered: Vec<u32> = app.pick_rows().into_keys().take(12).collect();
            let p = &mut app.planner.plan;
            p.shop.on = true;
            p.shop.stats = p.target;
            p.shop.sp = 1800;
            p.shop.offered = offered.into_iter().enumerate().map(|(i, id)| (id, (i % 4) as u8)).collect();
        }
        app
    }

    /// The app on this folder's game data, with the test roster (`app/tests/fixtures/`) in place
    /// of the account files, which may be empty.
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let mut app = App::new(Some(root));
        app.roster = Roster::sample();
        app.onboarding.open = false;
        app
    }

    fn reload(&mut self) {
        self.kb = Knowledge::load(self.root.as_deref());
        self.live.bundled = self.kb.umalator_presets.clone();
        self.apply_live();
        self.reload_roster();
        (self.sim_data, self.sim_error) = load_sim(self.root.as_deref());
        self.card_art = scan_art(self.root.as_deref(), "cards");
        self.uma_art = scan_art(self.root.as_deref(), "umas");
        self.planner.refresh_files(self.root.as_deref());
        self.planner.key_memo.set(None);
        self.planner.parent_sparks.replace(None);
        self.home_dirty();
        self.planner.rank_cache.borrow_mut().clear();
        self.notes.loaded = None;
        self.write_digest();
    }

    pub fn art_uri(&self, sub: &str, id: u32) -> Option<String> {
        let set = if sub == "cards" { &self.card_art } else { &self.uma_art };
        if !set.contains(&id) {
            return None;
        }
        let path = self.root.as_ref()?.join("app").join("assets").join(sub).join(format!("{id}.webp"));
        // file:///C:/… on Windows, file:///home/… elsewhere; a folder given as `.` must not
        // become file:///./….
        let path = std::path::absolute(&path).unwrap_or(path).to_string_lossy().replace('\\', "/");
        Some(format!("file://{}{path}", if path.starts_with('/') { "" } else { "/" }))
    }

    pub fn owned_card(&self, id: u32) -> Option<&SupportCard> {
        self.roster.cards.iter().find(|c| c.id == id)
    }

    pub fn owned_uma(&self, id: u32) -> Option<&Uma> {
        self.roster.umas.iter().find(|u| u.id == id)
    }

    /// Whether the user can get a skill from this source right now (potential skills need the
    /// potential level unlocked).
    pub fn source_available(&self, src: SkillSource) -> bool {
        match src {
            SkillSource::Potential(u, level) => self.owned_uma(u).is_some_and(|o| o.potential >= level),
            s => s.uma().is_some_and(|u| self.owned_uma(u).is_some()) || s.card().is_some_and(|c| self.owned_card(c).is_some()),
        }
    }

    pub fn skill_available(&self, id: u32) -> bool {
        let mut cache = self.available_skills.borrow_mut();
        let set = cache.get_or_insert_with(|| {
            self.kb.sources.iter().filter(|(_, srcs)| srcs.iter().any(|s| self.source_available(*s))).map(|(id, _)| *id).collect()
        });
        set.contains(&id)
    }

    /// Re-read the roster files (after the app itself edits them).
    pub fn reload_roster(&mut self) {
        let (roster, source) = Roster::load(self.root.as_deref());
        self.roster = roster;
        self.source = source;
        *self.available_skills.borrow_mut() = None;
    }

    /// A skill as a chip coloured by tier (white, gold, unique), with its summary on hover;
    /// clicking opens it in the Skills tab. `dim` is for locked or off-target skills.
    pub fn skill_link(&self, ui: &mut egui::Ui, id: u32, dim: bool) {
        let Some(s) = self.kb.skills.get(&id) else {
            ui.label(format!("Skill {id}"));
            return;
        };
        let color = tier_color(ui, s.tier());
        let color = if dim { color.gamma_multiply(0.45) } else { color };
        let button = egui::Button::new(RichText::new(&s.name).size(13.0).color(color))
            .fill(color.gamma_multiply(0.10))
            .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
            .corner_radius(CornerRadius::same(12))
            .small();
        let hover = format!("{} · {} SP\n{}", s.tier(), s.sp, if s.summary.is_empty() { &s.desc } else { &s.summary });
        if ui.add(button).on_hover_text(hover).clicked() {
            self.jump.set(Some(Jump::Skill(id)));
        }
    }

    /// Open one of the Career tab's pages.
    pub fn go_career(&mut self, page: CareerTab) {
        self.tab = Tab::Career;
        self.career_tab = page;
    }

    fn apply_jump(&mut self) {
        match self.jump.take() {
            Some(Jump::Uma(id)) => {
                self.tab = Tab::Collection;
                self.collection_tab = CollectionTab::Umas;
                self.umas.selected = Some(id);
            }
            Some(Jump::Card(id)) => {
                self.tab = Tab::Collection;
                self.collection_tab = CollectionTab::Cards;
                self.cards.selected = Some(id);
            }
            Some(Jump::Skill(id)) => {
                self.tab = Tab::Collection;
                self.collection_tab = CollectionTab::Skills;
                self.skills.selected = Some(id);
                self.skills.available_only = false;
                self.skills.scroll_to = true;
            }
            None => {}
        }
    }

    // -----------------------------------------------------------------------
    // Chrome

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui);
        ui.horizontal(|ui| {
            ui.set_min_height(34.0);
            // The mascot's head as the logo.
            let logo = egui::Image::new(egui::include_image!("../assets/chibi/seiun_102001_0010.png"))
                .uv(egui::Rect::from_min_max(egui::pos2(0.22, 0.08), egui::pos2(0.78, 0.56)))
                .fit_to_exact_size(vec2(34.0, 34.0));
            ui.add(logo);
            ui.label(semibold("umaLord", 21.0));
            ui.add_space(14.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            for tab in TABS {
                let (icon, label, what) = tab.info();
                let text = RichText::new(format!("{icon} {label}")).size(14.5);
                let on = self.tab == tab && !self.onboarding.open;
                if ui.add(egui::Button::selectable(on, text).min_size(vec2(0.0, 30.0))).on_hover_text(what).clicked() {
                    self.tab = tab;
                    self.close_setup();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let cog = egui::Button::new(RichText::new("⚙").size(17.0).color(p.text)).min_size(vec2(32.0, 30.0));
                if ui.add(cog).on_hover_text("Settings (Ctrl+,): your profile, look and theme, game data, the AI assistant").clicked() {
                    self.open_settings(self.settings.page);
                }
                let help = egui::Button::new(RichText::new("?").size(16.0).color(p.text)).min_size(vec2(32.0, 30.0));
                if ui.add(help).on_hover_text("Help (?): what each tab does, how to plan a career, and the game's words").clicked() {
                    self.help_open = true;
                }
                if ui.add(egui::Button::new("🔄").min_size(vec2(32.0, 30.0))).on_hover_text("Reload (F5): re-read your roster, game data and plans from disk").clicked() {
                    self.reload();
                    self.toast("Reloaded your roster, game data and plans.", ui_util::ToastKind::Success);
                }
            });
        });
    }

    /// Open the notes page; its Back button returns to the page you were on.
    pub fn open_notes(&mut self) {
        if self.tab != Tab::Notes {
            self.notes_back = self.tab;
        }
        self.tab = Tab::Notes;
        self.onboarding.open = false;
    }

    /// Switch dark, light or System, and save it.
    fn set_mode(&mut self, ctx: &egui::Context, mode: theme::Mode) {
        self.mode = mode;
        theme::set_mode(ctx, mode);
        self.dark = theme::is_dark(ctx);
        settings::set("theme", serde_json::json!(mode.key()));
    }

    /// Show a short message at the bottom of the window (from code without a `Context`).
    pub fn toast(&self, text: impl Into<String>, kind: ui_util::ToastKind) {
        self.pending_toasts.borrow_mut().push((text.into(), kind, None));
    }

    /// A toast with a button, e.g. "Show" for a saved file.
    pub fn toast_action(&self, text: impl Into<String>, kind: ui_util::ToastKind, label: &str, action: ui_util::ToastAction) {
        self.pending_toasts.borrow_mut().push((text.into(), kind, Some((label.into(), action))));
    }

    /// A message with a flag for "this is a warning" (the old status-line pair), as a toast.
    pub fn toast_flag(&self, (text, warn): (String, bool)) {
        self.toast(text, if warn { ui_util::ToastKind::Warn } else { ui_util::ToastKind::Success });
    }

    /// A success or an error, as a toast.
    pub fn toast_result(&self, result: Result<String, String>) {
        match result {
            Ok(m) => self.toast(m, ui_util::ToastKind::Success),
            Err(e) => self.toast(e, ui_util::ToastKind::Error),
        }
    }

    /// Hand this frame's toasts to egui, draw them, and carry out a clicked button.
    fn show_toasts(&mut self, ctx: &egui::Context) {
        for (text, kind, action) in self.pending_toasts.borrow_mut().drain(..) {
            match action {
                Some((label, a)) => ui_util::toast_action(ctx, text, kind, &label, a),
                None => ui_util::toast(ctx, text, kind),
            }
        }
        match ui_util::show_toasts(ctx) {
            // Show the file selected in the file manager (or open a folder).
            Some(ui_util::ToastAction::Open(path)) => {
                if let Err(e) = ui_util::show_in_folder(&path) {
                    self.toast(format!("Couldn't open {}: {e}", path.display()), ui_util::ToastKind::Error);
                }
            }
            Some(ui_util::ToastAction::Undo) => {
                self.undo_setup();
                // Setup writes the digest when it closes; elsewhere, now.
                if !self.onboarding.open {
                    self.finish_setup_edits();
                }
            }
            Some(ui_util::ToastAction::OpenPlan(path)) => {
                if self.open_plan_file(&path) {
                    self.go_career(CareerTab::Plan);
                }
            }
            None => {}
        }
    }

    /// Keyboard shortcuts: Ctrl+1…4 tabs, Ctrl+F search, Ctrl+S save the plan, F5 reload,
    /// Esc close, ? help, + a pulled copy. Listed in Help (`SHORTCUTS`).
    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, KeyboardShortcut, Modifiers};
        let typing = ctx.egui_wants_keyboard_input();
        for (i, tab) in TABS.iter().enumerate() {
            let key = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5][i];
            if ctx.input_mut(|inp| inp.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key))) {
                self.tab = *tab;
                self.close_setup();
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma))) {
            self.open_settings(self.settings.page);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::F))) {
            self.focus_search.set(true);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S))) {
            if (self.tab, self.career_tab) == (Tab::Career, CareerTab::Plan) && !self.onboarding.open {
                self.save_plan();
            } else {
                self.toast("Ctrl+S saves the plan on Career → Plan.", ui_util::ToastKind::Info);
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
            self.reload();
            self.toast("Reloaded your roster, game data and plans.", ui_util::ToastKind::Success);
        }
        // `?` opens Help, unless a text box is taking the keys.
        if !typing && ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t == "?"))) {
            self.help_open = true;
        }
        // `+` on an open uma or card: pulled another copy. Not behind a dialog, and once per press
        // (a held key repeats its Text event).
        let dialog = self.help_open || self.settings.open || self.onboarding.paste.open || self.cards.add.open || self.cards.compare_open;
        let plus = ctx.input(|i| {
            i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t == "+"))
                && !i.events.iter().any(|e| matches!(e, egui::Event::Key { repeat: true, .. }))
        });
        if plus && !typing && !dialog && !self.onboarding.open && self.tab == Tab::Collection {
            match self.collection_tab {
                CollectionTab::Umas => {
                    if let Some(id) = self.umas.selected {
                        self.raise_uma(id);
                    }
                }
                CollectionTab::Cards => {
                    if let Some(id) = self.cards.selected {
                        self.raise_card(id);
                    }
                }
                CollectionTab::Skills => {}
            }
        }
        // Esc closes the topmost thing: a dropdown or text box handles its own, then Help, a
        // dialog, then the detail panel.
        if !typing && !egui::Popup::is_any_open(ctx) && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.close_topmost();
        }
    }

    /// Esc: close Help, an open dialog, or the selection's detail panel, in that order.
    fn close_topmost(&mut self) {
        if self.settings.open || self.onboarding.paste.open {
            // Modals close themselves on Esc.
        } else if self.help_open {
            self.help_open = false;
        } else if self.onboarding.confirm_start_over {
            self.onboarding.confirm_start_over = false;
        } else if self.cards.add.open {
            self.cards.add.open = false;
        } else if self.planner.picker.is_some() {
            self.planner.picker = None;
        } else if self.onboarding.open {
            // Setup: unpick the picked tiles, then close the editor.
            if !self.onboarding.multi.is_empty() {
                self.onboarding.multi.clear();
            } else {
                self.onboarding.selected = None;
            }
        } else if self.tab == Tab::Collection {
            match self.collection_tab {
                CollectionTab::Umas => self.umas.selected = None,
                CollectionTab::Cards => self.cards.selected = None,
                CollectionTab::Skills => self.skills.selected = None,
            }
        }
    }

    /// The chibi pose for the page that's open (see `ui_util::chibi_image`).
    fn pose(&self) -> u8 {
        match self.tab {
            Tab::Home => 0,
            Tab::Collection => 1,
            Tab::Career => match self.career_tab {
                CareerTab::Plan => 2,
                CareerTab::Race => 3,
                CareerTab::Results => 4,
            },
            Tab::Ask | Tab::Notes => 5,
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        let weak = theme::pal(ui).weak;
        ui.style_mut().override_text_style = Some(egui::TextStyle::Small);
        ui.style_mut().visuals.override_text_color = Some(weak);
        // On a narrow window the labels shorten and the full text moves to the hover, so the
        // line never runs off the edge.
        let narrow = ui.available_width() < 1250.0;
        ui.horizontal(|ui| {
            if let Some(name) = &self.roster.profile {
                ui.label(format!("👤 {name}")).on_hover_text("Whose account this is. ⚙ Settings → Profile renames it or switches accounts.");
                ui.separator();
            }
            if let Some(server) = &self.roster.server {
                let short = server.split(" (").next().unwrap_or(server);
                ui.label(if narrow { format!("🌐 {short}") } else { format!("🌐 Server: {server}") }).on_hover_text(format!("Server: {server}"));
                ui.separator();
            }
            match &self.source {
                Source::Folder(p) => ui.label(if narrow { "📁 Data folder" } else { "📁 Your data folder" }).on_hover_text(p.display().to_string()),
                Source::Embedded => ui.label(RichText::new("⚠ Using a built-in snapshot: the umaLord folder wasn't found").color(theme::pal(ui).warn)),
            };
            if let Some(d) = &self.roster.cards_updated {
                ui.separator();
                ui.label(if narrow { format!("Roster {d}") } else { format!("Roster last updated {d}") }).on_hover_text(format!("Your roster files were last updated {d}."));
            }
            ui.separator();
            match (&self.kb.error, &self.kb.built) {
                (Some(e), _) => {
                    ui.label(RichText::new("⚠ Game data missing: run tools/update_knowledge.py").color(theme::pal(ui).warn)).on_hover_text(e);
                }
                (None, built) => {
                    let day = built.as_ref().map(|b| b[..10.min(b.len())].to_string());
                    let text = match (&day, narrow) {
                        (Some(d), true) => format!("uma.guide {d}"),
                        (Some(d), false) => format!("Game data from uma.guide, {d}"),
                        (None, _) => "Game data from uma.guide".to_string(),
                    };
                    ui.label(text).on_hover_text(format!(
                        "Game data from uma.guide{}. Global server: {} umas · {} support cards · {} skills",
                        day.map(|d| format!(", {d}")).unwrap_or_default(),
                        self.kb.umas.len(),
                        self.kb.cards.len(),
                        self.kb.skills.len()
                    ));
                }
            }
            ui.separator();
            let (text, hover, warn) = self.live_status();
            let text = if narrow { text.replace(" (umamusume.com)", "") } else { text };
            let r = ui.label(if warn { RichText::new(text).color(theme::pal(ui).warn) } else { RichText::new(text) });
            if !hover.is_empty() {
                r.on_hover_text(hover);
            }
            if !self.roster.warnings.is_empty() {
                ui.separator();
                let n = self.roster.warnings.len();
                let text = if narrow { format!("⚠ {n} unread lines") } else { format!("⚠ {n} lines in your roster files couldn't be read") };
                ui.label(RichText::new(text).color(theme::pal(ui).warn)).on_hover_text(format!("{n} lines in your roster files couldn't be read:\n{}", self.roster.warnings.join("\n")));
            }
        });
    }

    /// A soft sky behind the pages, with the theme's decorations toward the bottom right. Painted
    /// before the pages, whose panels are see-through. Chibi Seiun Sky isn't painted here: she's
    /// laid out where nothing else is (`ui_util::chibi`), so she never covers content.
    fn paint_background(&self, ui: &egui::Ui, rect: egui::Rect) {
        let p = theme::palette(self.dark);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, p.bg);
        // Ink is high contrast: the plain page colour, nothing behind the content.
        if !self.mascot || theme::look() == theme::Look::Ink {
            return;
        }
        // Sky: the page colour at the top, fading toward the theme's colour at the bottom.
        let sky = p.sky;
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.left_top(), p.bg);
        mesh.colored_vertex(rect.right_top(), p.bg);
        mesh.colored_vertex(rect.right_bottom(), sky);
        mesh.colored_vertex(rect.left_bottom(), p.bg.lerp_to_gamma(sky, 0.45));
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));
        let size = (rect.height() * 0.36).clamp(170.0, 270.0);
        let base = rect.right_bottom() - vec2(size * 0.62, 18.0);
        // Fixed scatter positions (0..1 across the page), so the decorations don't move between frames.
        let scatter = |n: usize, seed: u32| {
            let mut s = seed;
            (0..n).map(move |_| {
                let mut next = || {
                    s ^= s << 13;
                    s ^= s >> 17;
                    s ^= s << 5;
                    (s % 10_000) as f32 / 10_000.0
                };
                (next(), next(), next())
            })
        };
        match theme::look() {
            theme::Look::Sky | theme::Look::Sakura => {
                // Soft clouds in the corner.
                let cloud = egui::Color32::from_white_alpha(if self.dark { 10 } else { 70 });
                for (dx, dy, r) in [(-0.95, -0.12, 0.30), (-0.62, -0.22, 0.38), (-0.25, -0.12, 0.30), (-1.6, -0.75, 0.16), (-1.36, -0.82, 0.20), (-1.12, -0.74, 0.15)] {
                    painter.circle_filled(base + vec2(dx * size, dy * size), r * size, cloud);
                }
            }
            theme::Look::Sunset => {
                // A low evening sun with a soft glow, behind her.
                let sun = rect.right_bottom() - vec2(size * 1.05, size * 0.55);
                let glow = if self.dark { egui::Color32::from_rgb(251, 146, 60) } else { egui::Color32::from_rgb(253, 186, 116) };
                for (r, a) in [(1.0, 0.05), (0.75, 0.08), (0.52, 0.14)] {
                    painter.circle_filled(sun, size * r, glow.gamma_multiply(a));
                }
                painter.circle_filled(sun, size * 0.34, glow.gamma_multiply(if self.dark { 0.35 } else { 0.6 }));
                let cloud = egui::Color32::from_white_alpha(if self.dark { 8 } else { 60 });
                for (dx, dy, r) in [(-0.4, -0.08, 0.26), (-0.08, -0.14, 0.32), (-1.7, -0.5, 0.14), (-1.5, -0.56, 0.18)] {
                    painter.circle_filled(base + vec2(dx * size, dy * size), r * size, cloud);
                }
            }
            theme::Look::Night => {
                // Stars over the upper page, brighter ones bigger, and a moon.
                let star = if self.dark { egui::Color32::from_rgb(226, 222, 255) } else { egui::Color32::from_rgb(124, 92, 220) };
                for (x, y, b) in scatter(70, 0x9e37_79b9) {
                    let pos = rect.left_top() + vec2(x * rect.width(), y * y * rect.height() * 0.9);
                    painter.circle_filled(pos, 0.8 + b * 1.6, star.gamma_multiply(if self.dark { 0.25 + b * 0.45 } else { 0.18 + b * 0.3 }));
                }
                let moon = rect.right_bottom() - vec2(size * 1.25, size * 1.05);
                let light = if self.dark { egui::Color32::from_rgb(240, 236, 210) } else { egui::Color32::WHITE };
                painter.circle_filled(moon, size * 0.36, light.gamma_multiply(0.05));
                painter.circle_filled(moon, size * 0.2, light.gamma_multiply(if self.dark { 0.55 } else { 0.9 }));
            }
            theme::Look::Track => {
                // The rails of a racetrack's far turn, sweeping across the bottom of the page.
                let rail = if self.dark { egui::Color32::from_white_alpha(22) } else { egui::Color32::from_rgb(146, 100, 0).gamma_multiply(0.22) };
                let center = rect.right_bottom() + vec2(-size * 0.2, size * 1.9);
                for (r, w) in [(size * 2.3, 2.0), (size * 2.55, 1.2)] {
                    let pts: Vec<egui::Pos2> = (0..=48)
                        .map(|i| {
                            let a = std::f32::consts::PI * (1.0 + 0.5 * i as f32 / 48.0);
                            center + vec2(a.cos() * r * 1.6, a.sin() * r)
                        })
                        .collect();
                    painter.add(egui::Shape::line(pts, egui::Stroke::new(w, rail)));
                }
                // Furlong posts on the inner rail.
                for i in 0..7 {
                    let a = std::f32::consts::PI * (1.0 + 0.5 * (i as f32 + 0.5) / 7.0);
                    let at = center + vec2(a.cos() * size * 2.3 * 1.6, a.sin() * size * 2.3);
                    painter.line_segment([at, at - vec2(0.0, 14.0)], egui::Stroke::new(2.0, rail));
                }
            }
            theme::Look::Ink => {}
        }
        if theme::look() == theme::Look::Sakura {
            // Petals drifting down the page.
            let petal = if self.dark { egui::Color32::from_rgb(244, 143, 177) } else { egui::Color32::from_rgb(236, 112, 158) };
            for (x, y, b) in scatter(34, 0x85eb_ca6b) {
                let pos = rect.left_top() + vec2(x * rect.width(), y * rect.height());
                let r = 3.0 + b * 4.0;
                painter.add(egui::Shape::ellipse_filled(pos, vec2(r, r * 0.6), petal.gamma_multiply(if self.dark { 0.16 + b * 0.18 } else { 0.18 + b * 0.22 })));
            }
        }
    }

    /// What each tab is for, how to get started, and the game words the app uses.
    fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.help_open;
        let mut go = None;
        let height = (ctx.content_rect().height() - 140.0).clamp(300.0, 720.0);
        egui::Window::new(semibold("❓ Help", 18.0))
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([720.0, height])
            .collapsible(false)
            .show(ctx, |ui| {
                let p = theme::pal(ui);
                // A fixed term column, and text that wraps in the rest of the width.
                let row = |ui: &mut egui::Ui, term: &mut dyn FnMut(&mut egui::Ui), text: RichText| {
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(vec2(160.0, 20.0), egui::Layout::left_to_right(egui::Align::Min), |ui| {
                            ui.set_min_width(160.0);
                            term(ui);
                        });
                        ui.add(egui::Label::new(text).wrap());
                    });
                    ui.add_space(4.0);
                };
                let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
                // UMALORD_SCROLL scrolls Help too, for screenshots of the word list.
                if let Some(y) = self.debug_scroll {
                    area = area.vertical_scroll_offset(y);
                }
                area.show(ui, |ui| {
                    ui.label(semibold("What each tab is for", 16.0));
                    ui.add_space(4.0);
                    for tab in TABS {
                        let (icon, label, what) = tab.info();
                        let mut open = |ui: &mut egui::Ui| {
                            if ui.link(semibold(format!("{icon} {label}"), 14.0)).on_hover_text("Go there").clicked() {
                                go = Some((tab, None));
                            }
                        };
                        row(ui, &mut open, RichText::new(what).color(p.weak));
                        // Career's own pages, one level in.
                        if tab == Tab::Career {
                            for page in [CareerTab::Plan, CareerTab::Race, CareerTab::Results] {
                                let (icon, label, what) = page.info();
                                let mut open = |ui: &mut egui::Ui| {
                                    ui.add_space(18.0);
                                    if ui.link(format!("{icon} {label}")).on_hover_text("Go there").clicked() {
                                        go = Some((Tab::Career, Some(page)));
                                    }
                                };
                                row(ui, &mut open, RichText::new(what).color(p.weak));
                            }
                        }
                    }
                    let mut notes = |ui: &mut egui::Ui| {
                        if ui.link(semibold("📝 Notes and files", 14.0)).on_hover_text("Go there").clicked() {
                            go = Some((Tab::Notes, None));
                        }
                    };
                    row(ui, &mut notes, RichText::new("Read the markdown files in this folder: insights, plans, results and your roster files. It's in ⚙ Settings → Data and updates.").color(p.weak));
                    ui.add_space(10.0);
                    ui.label(semibold("Planning a career, start to finish", 16.0));
                    for (n, step) in [
                        "Open 🏇 Career → 📋 Plan and pick a goal. For a Champions Meeting, click the upcoming one and the course fills in, and the 📈 Meta card shows what the community's PvP sheet recommends for it.",
                        "Pick a trainee. The ✨ suggestions are the sheet's picks you own (★), then your umas with the best aptitudes for that course.",
                        "Press ✨ Recommend whole deck, or click each slot to choose cards yourself. The sheet's cards come first (★); hover a card to see why it was picked.",
                        "Under Skills to buy, press ✨ Auto-pick. It buys the sheet's skills first, then the ones that gain the most on the course, within your skill points.",
                        "Press 💾 Save plan. ▶ Simulate races the plan on the 🏁 Race sim page.",
                        "After the career, log what happened on the 🏆 Results page.",
                    ]
                    .iter()
                    .enumerate()
                    {
                        ui.horizontal_top(|ui| {
                            ui.label(semibold(format!("{}.", n + 1), 14.0).color(p.accent));
                            ui.add(egui::Label::new(*step).wrap());
                        });
                    }
                    ui.add_space(10.0);
                    ui.label(semibold("Keyboard shortcuts", 16.0));
                    ui.add_space(4.0);
                    for (keys, what) in SHORTCUTS {
                        row(ui, &mut |ui| _ = ui.label(RichText::new(*keys).monospace().size(13.0)), RichText::new(*what));
                    }
                    ui.add_space(10.0);
                    ui.label(semibold("Words you'll see", 16.0));
                    ui.label(RichText::new("General game terms, in short, from knowledge/mechanics.md. Hover a term to see where it comes from.").size(12.5).color(p.weak));
                    ui.add_space(4.0);
                    if self.kb.terms.is_empty() {
                        for (term, meaning) in GLOSSARY {
                            row(ui, &mut |ui| _ = ui.label(semibold(*term, 13.5)), RichText::new(*meaning));
                        }
                    }
                    for t in &self.kb.terms {
                        let source = if t.source == "general" { "General knowledge of the game".to_string() } else { format!("Source: {}", t.source) };
                        row(ui, &mut |ui| _ = ui.label(semibold(&t.term, 13.5)).on_hover_text(&source), RichText::new(&t.meaning));
                    }
                });
            });
        if let Some((tab, page)) = go {
            if tab == Tab::Notes {
                self.open_notes();
            } else {
                self.tab = tab;
            }
            if let Some(page) = page {
                self.career_tab = page;
            }
        }
        self.help_open = open;
    }

    /// Dev aid for checking the UI without a person at the screen: with
    /// `UMALORD_SCREENSHOT=out.png` the app saves the window ~2.5 s after start, then quits.
    /// `UMALORD_TAB=<tab>`, `UMALORD_VIEW=table` and `UMALORD_SELECT=<id>` set the view;
    /// `UMALORD_SCROLL=<px>` holds the planner scrolled down.
    fn debug_screenshot(&mut self, ctx: &egui::Context) {
        // UMALORD_INSTALL=1: install OpenCode on start if it's missing (before any UMALORD_ASK).
        if self.debug_install {
            self.debug_install = false;
            if self.ask.oc.exe.is_none() {
                self.tab = Tab::Ask;
                self.start_opencode_install(ctx);
            }
        }
        if self.debug_run {
            self.debug_run = false;
            match (self.tab, self.career_tab) {
                (Tab::Career, CareerTab::Race) => self.sim_start(ctx),
                // The planner: ✨ Auto-pick, as if pressed (the optimiser, held until it finishes).
                (Tab::Career, CareerTab::Plan) => self.auto_pick(ctx),
                _ => {}
            }
        }
        if self.sim.job.is_none() && self.sim.compare.is_some()
            && let Some(t) = self.debug_replay.take()
        {
            self.sim.replay.t = t;
        }
        if self.debug_extra && self.sim.job.is_none() && self.sim.extra.is_none() && self.sim.compare.is_some() {
            if self.sim.contribution.is_none() {
                self.start_extra(ctx, tab_sim::ExtraKind::Contribution);
            } else if self.sim.sensitivity.is_none() {
                self.start_extra(ctx, tab_sim::ExtraKind::Sensitivity);
            } else {
                self.debug_extra = false;
            }
        }
        let simulating = self.sim.job.is_some() || self.planner.skill_job.is_some() || self.planner.pick_job.is_some() || self.sim.extra.is_some() || self.debug_extra;
        let installing = self.ask.oc.install_rx.is_some() || simulating;
        // UMALORD_ASK=<question>: ask it on start, and hold the screenshot until it's answered.
        if !installing && let Some(q) = self.debug_question.take() {
            self.tab = Tab::Ask;
            self.debug_asked = true;
            self.submit_question(ctx, q);
        }
        let Some(path) = self.screenshot_to.clone() else { return };
        // No tooltips from wherever the real mouse happens to be over the window.
        if ctx.input(|i| i.time) < 0.5 {
            ctx.all_styles_mut(|s| s.interaction.tooltip_delay = f32::INFINITY);
        }
        // UMALORD_APPROVE=ask: the shot once an approval card has shown for a second (the chat
        // scrolls down to it first).
        if self.ask.shot_on_approval && self.approval_showing() && self.debug_approval_at.is_none() {
            self.debug_approval_at = Some(ctx.input(|i| i.time));
        }
        let card_shown = self.debug_approval_at.is_some_and(|t| ctx.input(|i| i.time) - t > 1.0);
        let asking = self.debug_asked && self.ask_busy() && !card_shown;
        let waiting = (installing || asking) && ctx.input(|i| i.time) < 300.0;
        if !self.screenshot_requested && !waiting && ctx.input(|i| i.time) >= 2.5 {
            self.screenshot_requested = true;
            let ctx2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes)
                    && let Err(err) = buf.save(&path)
                {
                    eprintln!("couldn't save screenshot: {err}");
                }
                ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
                ctx2.request_repaint();
            });
        }
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }

    /// Closing the window mid-answer stops OpenCode (and the tools server it started) and the
    /// Claude request, rather than leaving them running.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_ask_processes();
    }
}

impl App {
    /// One frame of the whole window; separate from eframe so tests can time it headless.
    fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // System mode follows the OS, so what "dark" means can change while the app runs.
        self.dark = theme::is_dark(&ctx);
        ui_util::set_pose(self.pose());
        self.shortcuts(&ctx);
        self.live_poll(&ctx);
        let p = theme::palette(self.dark);
        egui::Panel::top("top_bar")
            .frame(egui::Frame::new().fill(p.surface).inner_margin(Margin::symmetric(14, 9)))
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status_bar")
            .frame(egui::Frame::new().fill(p.surface).inner_margin(Margin::symmetric(14, 5)))
            .show(ui, |ui| self.status_bar(ui));
        // The pages' panels are see-through (theme.rs), so this shows behind all of them.
        self.paint_background(ui, ui.available_rect_before_wrap());
        self.take_dropped_profile(&ctx);
        if self.onboarding.open {
            self.onboarding_page(ui);
        } else {
            match self.tab {
                Tab::Home => {
                    egui::CentralPanel::default().frame(page_frame()).show(ui, |ui| self.overview(ui));
                }
                Tab::Collection => self.collection_tab(ui),
                Tab::Career => self.career_page(ui),
                Tab::Notes => self.notes_tab(ui),
                Tab::Ask => self.ask_tab(ui),
            }
        }
        if self.help_open {
            self.help_window(&ctx);
        }
        self.settings_window(&ctx);
        self.start_over_dialog(&ctx);
        self.apply_jump();
        // Background simulations finish even while another tab is open.
        if (self.tab, self.career_tab) != (Tab::Career, CareerTab::Race) {
            self.sim_poll(&ctx);
        }
        self.planner_poll(&ctx);
        self.pick_poll(&ctx);
        self.ask_poll(&ctx);
        self.show_toasts(&ctx);
        self.debug_screenshot(&ctx);
    }
}
