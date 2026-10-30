# Sources

The user supplied these. Claude notes what each covers, which server's data it shows, and how to read it. Last checked: 2026-10-18 (community resources read for `knowledge/mechanics.md`, `skills.md` and `racecourses.md`).

## The community resource list

- **Umamusume Guides and Resources** (originally a Steam guide, id 3540114219, since removed by Steam: "visible only to the owner"), mirrored at https://rentry.co/uma-resources, with its changelog at https://rentry.co/uma-resources-changelog (last entry 2026-07-22 when read). A hub of guides, tools, friend databases, scenario guides and an FAQ. The FAQ answers common questions (Open vs Graded League, 9★ parents, SR vs SSR, ◎ upgrades, blue spark odds, rushing). Read with `get_page_text`; links come out of the page with a small `querySelectorAll('article a')` script.
- **How to read each kind of source:**
  - JS-rendered sites (uma.guide, GameTora): the built-in browser's `get_page_text`.
  - Google Sheets: `/gviz/tq?tqx=out:csv&gid=<gid>` (tab names and gids are in the `/htmlview` page's script). `curl.exe` fetches them.
  - Google Docs: `/document/d/<id>/export?format=txt`.
- umamusu.wiki's search is behind a Cloudflare check; its New Player Guide (https://umamusu.wiki/Game:New_Player_Guide) opens normally.

## Used for knowledge/ (read 2026-10-18)

| Resource | What it covers | Used in |
|---|---|---|
| GameTora, Race Mechanics Handbook (robflop, Gertas), https://gametora.com/umamusume/race-mechanics | Stats in races, phases, ground, speed and acceleration, start delay, last spurt, position keep, rushing, spot struggle, duels, the anniversary mechanics, corner numbers, gate brackets. Global and JP. | `mechanics.md`, cross-checked against `app/src/sim/` |
| uma.guide guides (https://uma.guide/guides/): Team Trials, Parenting in Umamusume, Umalator 101, CM21 pre-guide, Racecourse Analysis, Skills Explained, Debuff Skills, Career Mechanics, Champions Meeting Basics (`beginners-cm`), Grand Concert, Race Mechanics (archived) | TT scoring and course odds; spark and affinity rates; simulation advice; CM 21's course and stats; accel zones and skill timing; activation and cooldowns; debuffs; training formulas and career events; the CM format; Grand Concert's lessons and SP | `mechanics.md`, `skills.md`, `racecourses.md`, `knowledge/scenarios.json` |
| uma.guide glossary (`knowledge/data/glossary.json`) | Community terms, the CM and LoH formats | the glossary in `mechanics.md` |
| Uma Musume Skills Spreadsheet (Dylan @dylank0, formatted by sayaduck; sheet updated 2025-12-12), https://docs.google.com/spreadsheets/d/1oB3eTvKqREtJDWJL0q80O_VjBcpOmRl5xE0z5fZKgFY | Every skill rated for Team Trials and CM, with Team Trials score per SP and a reason. Tabs: README, Speed, Accel, Other, Stamina, Debuff, Greens, Tierlist, References. Community opinion; predates the 2026-07-01 balance changes | `knowledge/data/skill_tiers.json` (`update_knowledge.py --only tiers`), `skills.md` |
| Hidden Guaranteed Post-Race Skill Hints (Causew and others), https://docs.google.com/spreadsheets/d/1RnkYfATh3-I9Y_HK_GK7-LEbk1wlvuGDtwDhBaCNZfY | Which race wins give which hint (43 rows); the "Random Skills" tab was empty | `skills.md` |
| Spark Proc Chances (IceDynamix), https://docs.google.com/spreadsheets/d/1mT_uH79lZwEth6qGvjBOvrRKxwfKixr6vfhgzoW1s-U | Spark proc odds by stars and affinity, affinity statistics. Its pink 1★ rate (1%) differs from uma.guide's (2%) | `mechanics.md`, Sparks |
| uma.guide's data chunk (`uma-data`) and Roster Viewer (https://uma.guide/roster-viewer/) | The game's affinity tables (relation groups, points, members) and how each slot's affinity is summed | `knowledge/data/affinity.json`, `affinity.rs` |
| umalab, `affinity.py` (https://github.com/iurker1758/umalab) | Affinity rules checked against the Global client (2026-07-30): a slot of the trainee's own character scores no groups but keeps its shared G1s; +3 per shared G1 | `affinity.rs` |
| UmaExtractor (https://github.com/xancia/UmaExtractor, no licence: studied, not copied) and umalab's ingest (https://github.com/iurker1758/umalab) | The Veteran List's msgpack key and the data.json format: spark ids, win records, lineage positions 10/20 and 11–12/21–22, rarity = stars, talent_level = potential | `veterans.rs`, `tools/read_veterans.py` |
| umareference, Affinity (Global edition, https://www.umareference.com/guide/legacies/affinity) and GachaData's affinity table (https://gacha-data.com/umamusume/affinity/) | The symbol thresholds and the July 2026 race-affinity change; published pair values the tests check against | `mechanics.md`, `affinity.rs` tests |
| Umamusume Rating Calculator (dak, Neon, IceDynamix; v3.1.0), https://docs.google.com/spreadsheets/d/1AAZCVDMiCozDNKte-5JYSm7SWUeVJ0o87i1iDY1bnWM | Stat score multipliers; skill scores in hidden sheets, depending on aptitude. The author now recommends daftuyda.moe/optimizer | `mechanics.md`, Rating |

## Listed, not used yet

- **Reference Document (Global)** (Erzzy), https://docs.google.com/document/d/11X2P7pLuh-k9E7PhRiD20nDX22rNWtCpC1S4IMx_8pQ: card evaluations, inheritance, builds, banner reviews. Large; read it for card notes (`knowledge/support_cards/`).
- **Support Card Encyclopedia** (Luh), https://docs.google.com/spreadsheets/d/1zZOBxjcgJEKna7ftYKq0AlZS25Q3hE_occ7kXB-Q6MQ; **Pull Guide 2.0** (rdr), https://docs.google.com/spreadsheets/d/1KudiJHMUOXJaRrh4zYjDorWLqb6Scvi50M2CRyT_Aqo; **Support Card Tier List** (Euophrys), https://euophrys.github.io/uma-tiers/#/global: card value opinions, for `knowledge/support_cards/*.md`.
- **Inheritance/Parenting Guide** (Crazyfellow), https://docs.google.com/document/d/1Q3IJKbtkplmuY-PAJMNjYiLtasv0eU0aIBEqp8_C3tg; **Breeding Tree Planner** (Depore), https://umamily.moe/; **Spark Farming Calculator**, https://docs.google.com/spreadsheets/d/1c0n6kl1BnSX3MEsX8a3UEmWkSBppXgbERLC56UEVrq4; GameTora's **Affinity Calculator**, https://gametora.com/umamusume/compatibility: parent planning.
- **Estimated Timeline** (SoulEC), https://docs.google.com/spreadsheets/d/e/2PACX-1vQ1bJUx-NQHJvwYE3Jcs9UP5aFVdWwoPVZc5Z7GN3oUGyB7-Rl8YZ48y-lJEvFWTGIrQmOa-yTehPkK/pubhtml; **Another Umamusume Timeline** (Phoenix), https://docs.google.com/spreadsheets/d/1ClwQpAPpVJDnHWYAkbAsk7k9DfvkQ8CJJvjQy3jg4Wc: estimated banners, CMs and features. The app's Home timeline uses the official news instead.
- **Required Stamina Calculator**, https://docs.google.com/spreadsheets/d/1x6go-4Er40nbC3WgsXqHD5GkyzC_zbJ141C96OodVew (tabs in Japanese); the app's HP calculator and Stamina mode cover the same ground. **Stamina Threshold and Home Turf Advantage in Team Trials** (Charles), https://docs.google.com/document/d/1fXDspwdXJJ-aQ-Al1GezQRSrc_6gTjwZaSNUYrGzggk: gone (HTTP 410 on 2026-10-18).
- **Skill Optimizer and Rating Calculator** (Void), https://daftuyda.moe/optimizer, and **Skill Hint Finder**, https://daftuyda.moe/hints: feature ideas for Step 5 (hint levels, Fast Learner, aims); not data.
- **Simulators to cross-check against:** VFalator (https://kachi-dev.github.io/uma-tools/umalator-global), alpha123's Umalator and Skill Activation Visualizer (https://alpha123.github.io/uma-tools/), mee1080's race simulator in English (https://kachi-dev.github.io/umasim-en/race). Record disagreements with the app in `insights.md`.
- **Friend databases** for finding a borrow card: uma.moe (https://uma.moe/), ChronoGenesis (https://chronogenesis.net/friend_search), UmaPure (https://uma-global.pure-db.com/). The Planner's friend-slot picker links the first two.
- GameTora's **Race Scheduler**, **Training Event Helper** and **Banner Planner**; uma.guide's **Agenda Planner**; Kiwizer's Canva; the **Gameplans** sheet; the **Team Trials Tracker** (AndrewZorz).

## Guide and database sites

### uma.guide — https://uma.guide/ (primary database, Global)

The main source for uma and card data. Its scenario filter lists the Global scenarios: URA Finale, Unity Cup, Trackblazer and Grand Concert.

- **Pages render with JavaScript.** WebFetch gets an empty shell. Open them in the built-in browser and use `get_page_text`. Event outcomes sit behind `+` toggles, so click those to read the choices.
- **Characters:** `/characters/` lists 271 trainees, and each outfit has its own page at `/characters/<id>` (Special Week [Special Dreamer] is `100101`). Each page shows:
  - base stats, stat growth, and track, distance and style aptitudes
  - the unique skill, innate skills, and skills unlocked by potential level, with SP costs
  - event skills and the event that grants each one
  - evolved skills and their conditions
  - career objectives, the unique epithet, and every costume, character, date and secret event
- **Support cards:** `/support-cards/` lists 258 cards. Detail pages are at `/support-cards/detail?id=<id>`. Each shows:
  - the unique effect and the level it unlocks at
  - an effects table at Lv30, 35, 40, 45 and 50, which is the SSR cap at each LB
  - specialty priority as a percentage
  - skill hints and event skills
  - chain events and random events
- **Tools:**
  - Deck Builder: `/support-cards/deck-builder`
  - Card comparison: `/support-cards/compare`
  - Track Browser: `/tracks/`
  - Champions Meeting schedule: `/cm-schedule/`
  - Pull Planner: `/pull-planner/`
  - Roster Viewer: `/roster-viewer/`, which imports an "UmaExtractor" JSON. Not yet checked.
- **Guides:**
  - Team Trials: `/guides/team-trials`
  - Parenting: `/guides/parenting-in-umamusume`
  - Current CM (CM20, Sagittarius Cup): `/guides/cm20-guide`
  - Grand Concert: `/guides/grand-concert`
  - Umalator 101: `/guides/umalator-101`
  - Selectors: `/guides/selector-overview`
  - Banner reviews: `/banner-reviews/`

### U-tools (ウマ娘.攻略.tools) — https://xn--gck1f423k.xn--1bvt37a.tools/ (Japanese, JP server)

A Japanese tool hub. **Its data comes from the JP server, which is ahead of Global**, so before using a recommendation, check the uma, card or skill exists on Global through uma.guide. It is best for mechanics, which work the same on both servers.

- Inheritance compatibility (相性) calculator: `/characters/<uma>/succession/simulator`. Compatibility tables and rankings: `/characters/<uma>/succession`, `/characters/rankings/succession/total`.
- Skill effect ranking per course: `/race/courses/<id>/effects/<distance type>`. Umas that suit each course: `/race/courses/<id>/effects`.
- Spark (factor) farming support: `/race/vsevents/chm/factor/leader`.
- Tool directory: `/tools`. It links JP race tools: the mee1080 race emulator (https://mee1080.github.io/umasim/race/), a stamina calculator (https://stacalc.hf-uma.net/), and a corner-loss calculator sheet.
- The newest site announcement is dated 2024-02-03. Check that data pages are current before relying on them.

### Henry Handsome Derby's Banner Timeline Carat Calculator v5.92 — [Google Sheet](https://docs.google.com/spreadsheets/d/100t3hnYl5Qm2UR8RtPlH-8Xd9KQbBlxEdXUOIR4d394/edit?gid=607505096#gid=607505096) (Global)

For pull planning, not career data. It holds a projected Global banner timeline through early 2029, carat income estimates, and the odds of reaching each LB on a banner. It also tracks pull tickets from Champions Meeting and League of Heroes. It is also reachable from https://umacaratcalculator.com/.

- Read it as CSV through `.../gviz/tq?tqx=out:csv&gid=<gid>`. The plain `/export` URL redirects and fails. The tab list (name and gid) is in the `/htmlview` page.
- Use it to answer "should I pull or save for X", together with the account files.
- **Tabs (checked 2026-10-17):** Carat Calculator (607505096, the link above), Timeline (0), Selector Planner (1419006109), PvP Deck & Uma Recommendation (1679229029), PvP List (1195900779), PvP Rankings (1843247484), Daily Legend Race Schedule (525698929).
- **PvP Deck & Uma Recommendation** is the reference the user gave for deck choices. Per CM it lists:
  - the course, conditions and current scenario, recommended stats (e.g. CM 21 Capricorn Cup, Chukyo 1200m, Grand Live: 1600 / 450 / 1100+ / 900+ / 1100+), and a style ranking
  - recommended umas per style, acceleration skills and must-have skills
  - Beginner, Standard and Whale decks per style, plus a borrow slot, each card tagged with why it's there: its stats, its gold skills for this race (e.g. Concentration, Left-Handed on a left track), hints, or "Link" for the scenario's linked character
  - It's community opinion cross-referenced from JP, KR, EN and TH guides, marked as rough beyond CM 21. The app's card scoring follows the same idea (target stats, the card's skills on the course, scenario link) but computes it.

## Scenario and training mechanics

Used for `knowledge/scenarios.json`, the career simulation and card scoring. Checked 2026-10-17.

- **GameTora scenario pages** (Global): base training values at facility level 1 (stats, SP and energy per facility), scenario link characters, stat caps, and scenario-specific SP sources. They render with JavaScript, so read them in the built-in browser with `get_page_text`.
  - URA Finale: https://gametora.com/umamusume/ura-finals (values after the July 2026 Global update; Aoi Kiryuin link; fan-milestone SP; Happy Meek duels)
  - Unity Cup: https://gametora.com/umamusume/unity-cup (Global values differ from JP's; unity training and Spirit Burst SP tables; story characters)
  - Trackblazer: https://gametora.com/umamusume/trackblazer (races-centred; Grade Points; no scenario link; shop items)
  - Grand Concert (Grand Live): https://gametora.com/umamusume/our-grand-concert (link characters Silence Suzuka, Agnes Tachyon, Smart Falcon, Mihono Bourbon, Light Hello; lesson and live SP; lyrics-event golds)
- **umareference**, training gain formula: https://www.umareference.com/guide/support-cards-in-detail/calculating-training-stat-gain (mood 0.1 per step, +5% per support, friendship from orange bond). Appearance weights (100 per facility, 50 absent, specialty added): https://www.umareference.com/guide/support-cards-in-detail/stat-stick-passives (community reverse-engineering).
- **umamusumedb** training calculator: https://umamusumedb.com/tools/training-calculator/ (facility level multipliers 1.0 to 2.0, friendship at bond 80). An approximation.
- **Game8**: race rewards at 1st place, https://game8.co/games/Umamusume-Pretty-Derby/archives/572831 (G1 45 SP, G2/G3 35, OP 30; Trackblazer 35/25/20); bond gains, https://game8.co/games/Umamusume-Pretty-Derby/archives/542672 (+7 per training, +5 per hint).
- **uma.guide skills guide**: https://uma.guide/guides/skills (hint discounts 10/20/30/35/40% for levels 1–5).
- **uma.guide CM schedule:** a list in the `uma-data` chunk with each CM's number, course and scenario. `update_knowledge.py` saves it as `knowledge/data/cm_schedule.json`. CM 17–22 are Grand Concert; Grandmasters starts at CM 23.
- **uma.guide banner reviews** (`/banner-reviews/<scenario>/`) rate banners per budget (must, good, consider, niche, skip). That's pull advice, not a card's worth in a deck, so the app doesn't use it.

## Race simulators

### Moomoolator — https://umalator.app/ (primary simulator, Global; JP too)

The user supplied it through https://github.com/TheCing/uma-tools#live-tools. It's TheCing's fork of alpha123's Umalator and kachi-dev's VFalator, under GPL-3.0. Global data comes from Global's `master.mdb`, and skill data from GameTora. Checked 2026-10-17.

**The app runs it natively.** `app/src/sim/` is a Rust port of its engine v2 (the repo's `uma-skill-tools/`, `umalator/compare.ts`, `umalator/hpcalc.ts`, `hp-calculator/calculations.ts`), reading the site's own data files from `knowledge/data/sim/`. On 2026-10-17 the same setup (seed 12345, 1000 runs) gave the same mean and median gap, full-spurt and rushed rates on the site and in the app. The live tools the repo README lists (`/umalator-global`, `/umalator`, `/hp-calculator`) map to the Race sim tab's Compare, Skill chart, Stamina and HP calc modes; the JP simulator isn't ported, since the account is on Global.

The repo README lists `/umalator-global`, `/umalator` and `/hp-calculator`. The first two now redirect to the root.

#### Simulator: `/`

- **Modes:** Compare (two umas over N runs), Skill (the skill chart) and Stamina.
- **Header:** an event preset (CM16–CM24 as of 2026-10-17), then track and course, ground, weather, season and time of day.
- **Options:**
  - samples (default 500) and seed
  - Sync RNG, Engine v2, Wit Check and Rush/Kakari (all on by default)
  - Spot Struggle and Dueling, with editable rates (off by default)
  - Lane Movement (on by default)
  - In-Game Only, which hides skills not on Global yet
- **Per uma:** outfit, stars, unique level, the five stats, surface, distance and style aptitude, style (including Oonige/Runaway), mood, skills, and forced skill positions.
- **Outputs:**
  - the length gap: min, max, mean and median. 1 L = 2.5 m, and a negative gap means Uma 1 finishes ahead.
  - per uma: finish time, max speed, full-spurt rate, HP survival rate, start delay, and rushed rate with its location
  - each skill's activation window
  - a race timeline of the median run
- **Limits** (from the repo's CLAUDE.md):
  - it simulates one uma at a time against a virtual field
  - position keep is approximated
  - order conditions are assumed to be met
  - `accumulatetime` skills can fire early

  uma.guide's Umalator 101 warning still applies: treat the Skill-mode chart as an upper bound.
- **Skills:**
  - It loads 928 of uma.guide's 954 Global skills, including evolved skills.
  - The 26 it lacks are mostly scenario skills (`3000xx`).
  - Its import drops unknown skill IDs without saying so. `knowledge/data/umalator.json` lists the ones it knows.
- **Event presets:**
  - Each Champions Meeting preset has a date, course and conditions, plus a `confirmed` flag. Future CMs are unconfirmed guesses.
  - `update_knowledge.py` copies them into `knowledge/data/umalator.json` → `presets`, with readable `course_name` and `conditions`.
  - Global has no League of Heroes presets yet.
  - Cross-check uma.guide's `/cm-schedule/`.

**Loading builds.** Use a share link: `https://umalator.app/#` + `encodeURIComponent(base64(gzip(JSON)))`.

- **The JSON:** `{courseId, ground, weather, season, time, samples, uma1, uma2}`.
  - Both `uma1` and `uma2` must be valid, or the whole link is ignored.
  - Missing fields default to course 10506, Firm, Sunny, Winter, Midday and 500 samples.
- **An uma:** `{"outfitId": "100101", "starCount": 5, "uniqueLv": 6, "speed": 1200, "stamina": 800, "power": 900, "guts": 400, "wisdom": 600, "strategy": "Sasi", "distanceAptitude": "A", "surfaceAptitude": "A", "strategyAptitude": "A", "mood": 2, "skills": ["100011", "200512"]}`.
  - `strategy` is one of Nige, Senkou, Sasi, Oikomi or Oonige.
  - Aptitudes are S to G, and mood runs from -2 to 2.
  - The Planner's "Copy for Umalator" JSON already has this shape.
- **Condition codes:**
  - ground: 1 Firm, 2 Good, 3 Soft, 4 Heavy
  - weather: 1 Sunny, 2 Cloudy, 3 Rainy, 4 Snowy
  - season: 1 Spring, 2 Summer, 3 Autumn, 4 Winter
  - time: 2 Midday, 3 Evening, 4 Night
- **Course IDs:** listed in `knowledge/data/umalator.json` → `courses`. Some distances have inner and outer layouts; races.json can't tell them apart.
- **Making a link in the built-in browser,** on any umalator.app page:

  ```js
  const state = {courseId: 10701, ground: 1, weather: 1, season: 4, time: 2, samples: 1000, uma1, uma2};
  const gz = new Blob([JSON.stringify(state)]).stream().pipeThrough(new CompressionStream('gzip'));
  let s = ''; for (const b of new Uint8Array(await new Response(gz).arrayBuffer())) s += String.fromCharCode(b);
  location.href = 'https://umalator.app/#' + encodeURIComponent(btoa(s)); location.reload();
  ```

  The page reads the hash only when it loads, hence the reload.
- **After loading:** click RUN. 1000 runs take a few seconds. Read the results with `get_page_text` from "Stat thresholds" onward.
- **Saving the setup:** the ⚙ settings menu's Copy Link makes the same kind of link. Save it in the plan.
- **Other ways in:** in the ▶ UMA panel, pick Uma 1 or Uma 2, then the import (↑) icon in the panel header. It takes a pasted build, a JSON file, or a screenshot (OCR through Google Gemini). Don't send the user's screenshots there unless they ask.

#### Other Moomoolator tools

- **HP Calculator** (`/hp-calculator/`, GL/JP toggle):
  - **Inputs:** course, Speed, Stamina, Guts, Wit, heal % and style.
  - **Outputs:** max HP, HP needed (first two thirds, then the spurt), surplus, whether the uma gets a full spurt, and the minimum Stamina for one. It gives each output with and without expected downhill savings, plus a table of all five styles.
  - **Limits:** it has no mood, aptitude or ground inputs. Use it for a quick Stamina floor, then confirm in the simulator.
- **Mechanics Explorer** (`/mechanics-explorer/`):
  - **Inputs:** stats, aptitudes, mood, style, skills, course and ground.
  - **Outputs:** target speed per phase, last-spurt speed, acceleration, max HP, HP drain per second, HP needed for a full spurt, skill activation chance from Wit, and downhill trigger rate. It also charts one mechanic against one stat.
  - **Use:** cite it in `knowledge/mechanics.md` as "Moomoolator's formulas".
- **Skill Activation Visualizer** (`/skill-visualizer/#cid=<courseId>`): where skills can trigger on a course. It shows phases, corners, straights, slopes and stat thresholds.
- **Team Trials planner** (`/team-trials/`): a lineup sheet with three slots for each of Sprint, Mile, Medium, Long and Dirt. Each slot holds an uma and a style, and Copy Link shares the lineup. It doesn't simulate.
- **Release timeline** (`/release-timeline/`): JP release dates for every card and uma, grouped by scenario, with "Show entries not yet on Global". It helps judge what Global gets next, but the dates are JP's, and Global's order can differ.
- **Events** (`/events/`): a banner tracker fed from GameTora. It showed "No upcoming events" on 2026-10-17, so use the carat calculator sheet instead.
- **Docs** (`/docs/`): short guides to stats and stamina.
- **Build Planner** (`/build-planner/`): blank on 2026-10-17.

### Umalator Global (alpha123) — https://alpha123.github.io/uma-tools/umalator-global/ (Global, fallback)

The original that Moomoolator forks. Use it only to cross-check a Moomoolator result, and say so. It loads 712 skills and no evolved skills. The notes below are from before Moomoolator.

Runs two umas head to head on one course.

- **Inputs, per uma:** uma, stars, Speed, Stamina, Power, Guts, Wit, surface, distance and style aptitudes, style, mood, and skills.
- **Shared inputs:** course and conditions.
- **Options:** simulate position keep, spot struggle, wit checks for skills, and show HP consumption.
- **Running it:** COMPARE runs it. **Copy link** encodes the whole setup, so save that link in plan files to make a run reproducible.
- **Outputs**, according to uma.guide's Umalator 101: the length difference, a skill chart of average lengths gained per skill, survival rate (whether the uma lasts to the finish) and spurt rate (whether she gets a full spurt at the start of late-race).
- **Limitations,** also from Umalator 101: the skill chart ignores some activation conditions and assumes the best case. For example, Unyielding Spirit ranks high but rarely fires at a useful moment. Treat the skill chart as an upper bound. For skill value, prefer full compare runs with wit checks turned on.
- **Fork:** uma.guide says the kachi-dev fork (VFalator) has the most features: https://kachi-dev.github.io/uma-tools/umalator-global/. Moomoolator builds on it, so prefer Moomoolator.
