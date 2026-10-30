# Game mechanics

Sourced notes on how the game works, for the Global server unless a section says otherwise. Each section names its source and the date it was read (all 2026-10-18 unless noted). Labels: **data** (game data, or a formula the community decoded from it), **engine** (the app's port of Moomoolator's race engine in `app/src/sim/`, used to cross-check), **community** (a guide's advice or a community data set). Where sources disagree, both are given.

The **Glossary** and **Did you know?** sections at the end are read by the app: Help's "Words you'll see" and Home's "Did you know?" card. Keep their format (a table, and one tip per line ending with its source in brackets).

## Stats in races

Source: GameTora, Race Mechanics Handbook (robflop, Gertas; created 2023-12-26), https://gametora.com/umamusume/race-mechanics. Cross-checked with the engine (`sim/params.rs`, `sim/hp.rs`).

- Stats run 1–2000, but **past 1200 they count half**: 1500 Speed works as 1350 (engine: `adjust_overcap`). Mood moves every stat by 2% per step from Normal (engine: the same). In career races every stat gets a temporary +400.
- **Speed** only sets target speed in the late race and the last spurt, and the extra speed the last spurt gives. It doesn't make an uma faster earlier.
- **Power** sets acceleration everywhere, lane changes and how much speed an uphill costs, and feeds the newer mechanics (stamina contest, repositioning, power conservation).
- **Stamina** sets maximum HP, worked out once at the start: **max HP = 0.8 × style factor × Stamina + distance in metres**, with style factors Front 0.95, Pace 0.89, Late 1.0, End 0.995, Runaway 0.86 (engine: `hpcalc.rs`; GameTora: Late and End convert best, Pace worst, 11% apart). So 1000 Stamina on 2000 m is about 2760 HP for a Front Runner, 2712 for a Pace Chaser and 2800 for a Late Surger. Recovery skills heal a share of max HP (Breath of Fresh Air: 5.5%), so they're worth more with more Stamina and in longer races.
- **Guts** sets the last spurt's extra speed, the strength of spot struggles, duels and repositioning, and cuts HP use in the late race: the drain there is multiplied by 1 + 200 / √(600 × Guts) (engine).
- **Wit** sets skill activation, the chance of being rushed, downhill mode, and some position-keep checks. It does **not** affect late starts. Wit is scaled by the style aptitude: S +10%, A as is, lower grades less (D −40%, G −90%); aptitude sparks and skills don't change skill activation.

## Wit in numbers

Source: the engine's formulas (`sim/solver.rs`), which match uma.guide's Skills Explained (catbomb, peter; https://uma.guide/guides/skills) for skill activation. Computed here; Wit means Wit after the style-aptitude scaling.

| Wit | Skill activation, max(20, 100 − 9000 / Wit) % | Chance of rushing, (6.5 / log10(Wit / 10 + 1))² % |
|---|---|---|
| 400 | 77.5% | 16.2% |
| 600 | 85.0% | 13.3% |
| 800 | 88.8% | 11.6% |
| 1000 | 91.0% | 10.5% |
| 1200 | 92.5% | 9.7% |

- Green (passive) skills skip the Wit check, except Lucky Seven and Super Lucky Seven (uma.guide).
- Self-Control takes a flat 3% off the rushing chance (GameTora; engine).

## Phases and sections

Sources: GameTora's handbook; uma.guide, Race Mechanics (archived page; catbomb, Sparks and others, after KuromiAK's race mechanics doc), https://uma.guide/guides/race-mechanics. Engine: `sim/course.rs`.

- Every race has **24 equal sections** and four phases: early race (sections 1–4, the first sixth), mid race (5–16, to two thirds), late race (17–20, the accel zone) and last spurt (21–24, the last sixth). In skill conditions the phases are 0–3. For 2400 m: 0–400, 400–1600, 1600–2000, 2000–2400.
- The **last spurt mechanic** starts at the start of the late race (two thirds in), not at the last-spurt phase. If the uma has the HP to finish at spurt speed she spurts at once; if not, her spurt is weaker, later or both, or doesn't happen.
- The game runs **15 frames a second** (engine: `DT = 1/15`).
- Baseline speeds: about 3 m/s at the gate, 17 m/s in the start dash, 20 m/s through the early and mid race, 25 m/s in the last spurt. +0.45 m/s from a mid-race skill is +2.25%.
- Deceleration toward a lower target speed: −1.2 m/s² in the early race, −0.8 in the mid race, −1.0 in the late race, −0.5 in Pace Down mode, −1.2 when out of HP (uma.guide; the engine has the same values).

## Acceleration and speed skills

Source: GameTora's handbook; uma.guide, Racecourse Analysis (Hjax, catbomb, Lussless), https://uma.guide/guides/racecourse-analysis.

- A target-speed buff raises the speed she accelerates toward; she still has to accelerate to it, so a short buff she can't reach is partly wasted. Current-speed buffs act at once.
- From the mid race to the spurt her target jumps from about 20 to 25 m/s. With A aptitudes and 1200 Power she accelerates at about 0.46 m/s² and takes about 11 s; an acceleration skill (+0.2 to +0.66 m/s²) cuts that to about 7.5 s, worth about 8.75 m (3.5 lengths).
- **Carry-over:** a speed skill that fires at the end of the mid race and lasts into the spurt starts her spurt faster.
- Being blocked caps her target speed at roughly the blocking uma's.

## HP, stamina and the full spurt

Sources: GameTora's handbook; the engine (`sim/hp.rs`, `sim/hpcalc.rs`); uma.guide, Umalator 101 (vinje, catbomb), https://uma.guide/guides/umalator-101.

- HP drain per second = 20 × (speed − base speed + 12)² / 144, times: the Guts factor in the late race; ×0.4 in downhill mode; ×0.6 in Pace Down; ×1.6 while rushed; ×1.4 in a spot struggle (×3.5 Runaway, ×3.6 rushed Front, ×7.7 rushed Runaway). Base speed = 20 − (distance − 2000) / 1000 m/s.
- **Ground:** soft and heavy ground raise HP use by 2% (GameTora). The engine has ×1.02 for soft and heavy turf and heavy dirt, but ×1.01 for soft dirt.
- **Rushing** costs about 45 to 180 base Stamina worth of HP, depending on the race length (GameTora).
- Survival rate (did she finish without running dry) and spurt rate (did she get a full spurt) are what the simulator's Stamina checks show. To raise a low spurt rate, add a recovery skill or a stamina green skill.
- **Simulate at Normal mood for a Champions Meeting's rounds; Great mood only for the finals** (uma.guide, Umalator 101; community). The app's plan builds use Great mood (Umalator's default), so their gaps and spurt rates are a little optimistic for the rounds.

## Ground, weather and course bonuses

Source: GameTora's handbook. Engine: `sim/params.rs` (`GROUND_SPEED`, `GROUND_POWER`), `sim/course.rs` (`speed_modifier`).

- Heavy ground: Speed −50. Turf: Power −50 on any ground but firm. Dirt: Power −100, except good (−50).
- Courses can list one or two **speed bonus stats**: +5% Speed per 300 of the stat (up to 901, so 20% at most), averaged over the listed stats. The generated sections of `racecourses.md` list them per course.
- Green skills that fit the conditions (Firm Conditions, Sunny Days, Winter Runner, a racecourse's own) add stats, and can push them past 1200 (rentry FAQ; uma.guide).

## Position keep and the field

Source: GameTora's handbook. Engine: `sim/solver.rs` (position keep is approximated against a virtual field).

- From the start to about halfway through the mid race (section 10), umas keep position with **modes**, checked every 2 s, each lasting at most 1/24 of the race (Runaway 3/24): Normal; Pace Up Ex (any style, when a style that should be behind is ahead); Front only: Speed Up (leading by too little: under 4.5 m, or 12.5 m with no other Front Runner, 17.5 m for Runaway) and Overtake (not first among Front Runners), both after a Wit check; others: Pace Up (too far behind the pacemaker, after a Wit check) and Pace Down (too close; skipped while a speed skill is on, and ended by one).
- **Rushing (掛かり):** decided once per race by Wit; it starts between the middle of the early race and the middle of the mid race; every 3 s there's a chance it ends (GameTora: 60%; the engine: 55%), and it ends by itself after 12 s (debuffs can extend it). While rushed: HP use ×1.6, Wit checks always pass, and the uma runs further forward (Pace acts like Front; Late like Front 75% or Pace 25%; End like Front 70%, Pace 20%, Late 10%).
- **Spot struggle** (Global since 2025-11-11): two or more Front Runners within 3.75 m (Runaways within 5 m) from 150 m in until just after the mid race starts; they gain Guts-based speed and burn HP faster.
- **Duel** (Global since 2025-11-11): on the final straight, umas within 3 m for over 2 s at similar speed (within 0.6 m/s) with at least 15% HP gain Guts-based speed and acceleration; below 5% HP an uma drops out.
- Newer mechanics (from JP's 2nd and 2.5th anniversaries): power conservation and release (Power over 1200 adds acceleration when the spurt starts), stamina contest (Stamina over 1200, races over 2100 m, at full spurt speed: more speed to the finish), repositioning, stamina conservation and securing the lead (mid race, Wit- and Guts-based, cost HP). GameTora says their details are only partly known.

## Start delay and Strong Start

Source: GameTora's handbook.

- Every uma rolls a start delay of up to 0.1 s; **Wit doesn't affect it**. 0.066 s or more is a late start (the "Late Start" sign shows from 0.08 s) and loses the start's extra acceleration, which hurts Front Runners most.
- Concentration and Focus scale the roll down; Gate Trouble scales it up; Dilly-Dally and Don't Tell Me What to Do! add a fixed delay.
- Under 0.02 s is a **Strong Start**, worth 1000 points in Team Trials.

## Corners and gates

Source: GameTora's handbook.

- Corner numbers count back from the finish: the last corner is 4, then 3, 2, 1, then 4 again. A skill on "corner 3" fires on corners numbered 3; a random-corner condition uses the last one with that number. `racecourses.md` lists each course's numbers (`course_notes.rs` numbers them the same way).
- Gate brackets 1–8: brackets 1–3 are "inner", 6–8 "outer". With more than 8 runners, extra gates fill brackets from 8 backwards.

## Champions Meeting format

Sources: uma.guide, Champions Meeting Basics (catbomb), https://uma.guide/guides/beginners-cm; uma.guide's glossary (`knowledge/data/glossary.json`); rentry.co/uma-resources FAQ (community).

- Each CM runs on a course and conditions taken from a real G1 that ran around that time on JP. 3v3v3 races: three umas per player.
- **Leagues:** Graded League takes any uma. Open League is for lower-ranked umas: uma.guide's glossary and the rentry FAQ say rank A+ or below, the Champions Meeting Basics page says B or below (they disagree; the glossary is the newer). The league can't be changed once Round 1 starts; you can still register late during the first two rounds.
- **Schedule (11 days):** 4 days of registration (registering gives 3 Toughness 30 and 3 Alarm Clocks); Round 1 for 2 days; Round 2 for 2 days; then the finals.
- **Rounds:** up to 4 entries a day, each 5 races. The first entry is free, 2 more are free for taking part that day, the 4th costs 30 carats (the guide says always play all 4). 3 or more wins in any one entry of Round 1 puts you in Group A for Round 2, else Group B. After Round 2: from Group B, 1 win reaches the Group B finals; from Group A, 1 win reaches the B finals and 3+ wins in one entry the A finals; no wins, no finals.
- **Finals:** 24 hours to lock the team (else the last one used), then a single race. The winner gets a title.
- The app shows each CM's dates, course, conditions and the skills it turns off from the official news (`live.rs`).

## Team Trials scoring

Source: uma.guide, Team Trials (vinje, catbomb, Jaytsu and others; updated 2026-10-05), https://uma.guide/guides/team-trials. Data as decoded by the community.

- Five races (Sprint, Mile, Medium, Long, Dirt), twelve runners each; more slots unlock at Class 2 and Class 3. A weekly high score above the promotion line moves you up a class; above the retention line keeps it.
- **Placement:** 1st 10,000; 2nd 8,000; 3rd 7,000; 4th 6,000; 5th 5,000; 6th and 7th 4,000; 8th and 9th 3,000; 10th–12th 2,000. **Team:** 1st-2nd-3rd 5,000; all in the top 5 4,000.
- **Skills:** gold 1,200; white or green 500; inherited unique 500; own unique 1,500–2,500 at 1–2★ and 2,000–3,000 at 3★+ by its level. This is why consistent skills (and a consistent unique) matter more than winning.
- **Other:** beating the course's reference time, 100 per 0.1 s (at most 2,000); winning margin, 1,000 (½ L) to 3,000 (10 L), and 2,000 / 3,000 / 5,000 for a neck / head / nose win; good positioning, 1,000 in each of early, mid and late race per running style on the team (so mix styles); Strong Start 1,000; long shot (8th favourite or lower finishing top 3) 4,000; rushed −500 and −100 per 0.1 s rushed.
- **Multipliers (added together):** the Ace +10%, the support bonus from card levels, win streaks +2% to +5%, and the biggest, the **opponent rating bonus = opponent rating / (your rating + 200,000 − opponent rating)**: always pick the strongest opponent offered.
- Summer Gold Ship's unique, 564 Escapades, is a white skill that fires a random gold in the second half: up to 1,700 points a race (community favourite to inherit on every member).

## Career training

Source: uma.guide, Career Mechanics (KoYu1, Jaytsu, Luseless, Sayaduck, Petermagician), https://uma.guide/guides/career-mechanics. The app's career simulation (`career.rs`) uses the same shape; `knowledge/scenarios.json` has each scenario's values.

- **Training gain** = ⌊(base + stat bonus) × Π(1 + friendship bonus / 100) × (1 + mood × (1 + Σ mood effect / 100)) × (1 + Σ training effectiveness / 100) × (1 + 0.05 × cards on the training) × (1 + growth / 100)⌋.
- URA facilities level up after 4 trainings each; Unity Cup's follow the team's stat ranks (G, E, C, A, S for levels 1–5). **Summer camp** (early July to late August of the Classic and Senior years) raises every facility to level 5, and its Rest & Recreation gives 40 energy and +1 mood and cures conditions.
- **Unique skill level-ups** in the Senior year: Valentine's (after late January) needs 60,000 fans (dirt umas 40,000); Fan Festival (after late March) 70,000 (dirt 60,000) and, in URA, director bond above 60; Holiday (after early December) 120,000 (dirt 80,000). Missing one gives SP instead: +20, +25 (and −5 energy), +20.
- **New Year:** Classic year: +10 to one stat, +20 energy or +20 SP; Senior year: +30 energy, +5 to every stat or +35 SP. **Raffle** (Senior, before late January): tissue 10% (mood down), one carrot 45% (+20 energy), a bushel 30%, Hamburg steak 10%, hot spring ticket 5%.
- Conditions: Practice Perfect (−2% failure), Fast Learner (skills 10% cheaper), Charming (+2 bond with that card), Hot Topic (+40% bond with NPCs). Negative: Practice Poor, Night Owl, Skin Outbreak, Slow Metabolism, Migraine, Slacker.
- The acupuncturist's options succeed about 30% (+20 all stats), 50% (Corner Recovery and Straightaway Recovery), 70% (energy and max energy) and 90% (energy, mood, Charming) of the time.

## Grand Concert

Source: uma.guide, Grand Concert Guide (ayan, Secretariat, KoYu1; updated 2026-07-20), https://uma.guide/guides/grand-concert. Used to calibrate `knowledge/scenarios.json`.

- Stat caps 1600 / 1300 / 1300 / 1500 / 1300. Blue sparks raise caps at the start: 1★ +4, 2★ +9, 3★ +16, 6★ +32, 9★ +48; a green spark's unique raises up to three caps by the parent's stat bonuses.
- Training earns **performance points** of five kinds (60% the facility's main kind, 30% its second, 10% the rest), ⌊(S + facility level) × 1.15^cards + 2 × linked cards⌋ with S = 9 (Wit 5). Light Hello's training has a 45% chance of 20 extra points of the kind you have least of.
- Points buy **lessons**: techniques (energy, SP, stats, hints) and songs (an instant bonus and a bonus after the next concert). Songs unlock after a number of techniques that resets each concert (before the first: 1, 2, 3, 4…; later: 2, 2, 2, 4…).
- Five **concerts**: late December of each year and late June of the Classic and Senior years. Three songs before a concert give a Great Success (+10 all stats, else +3). **Each lesson learned since the last concert pays SP at the concert: song +25, technique +5.**
- SP songs: Run n' Run! (+22 SP at once), Run for Our Dream! (+2 SP per training, after the first concert), Grow Up and Shine! (+3, after the second).
- 18 songs before Girls' Legend U (after early December, Senior year) unlock the gold "I Wanna Win with You"; with 16 by early November, a linked character (Smart Falcon, Mihono Bourbon, Agnes Tachyon or Silence Suzuka, uma or card) gives her gold hint (Full Speed!, Concentration, Come What May, Trackblazer).
- Races give no points, so the guide skips optional races. The guide suggests at least 2 Speed and 2 Wit cards plus Light Hello.

## Skill costs

Source: uma.guide glossary ("Fast Learner", in `knowledge/data/glossary.json`) and Skills Explained (https://uma.guide/guides/skills). Global.

- **Hints** take 10/20/30/35/40% off a skill's base SP at hint levels 1–5.
- **Fast Learner** (Sharp, Kiremono) is a career condition that "decreases the cost of all Skills by 10% of its base cost". Read as 10% of the base price, so it adds to the hint discount (Lv3 + Fast Learner = 40% off). The planner's Step 5 has a Fast Learner box that does this.
- A **gold (or ◎) skill** needs its white (or ○) bought first; the white's price counts toward the gold.

## Sparks and inheritance

Sources: uma.guide, Sparks & Inheritance (https://uma.guide/guides/sparks) and Parenting in Umamusume (JJAY, Pik, Apol; https://uma.guide/guides/parenting-in-umamusume); IceDynamix, Spark Procs sheet (https://docs.google.com/spreadsheets/d/1mT_uH79lZwEth6qGvjBOvrRKxwfKixr6vfhgzoW1s-U, Overview and Affinity tabs; community data). Global.

- Sparks activate three times in a career: at the start, and at the two Inspiration events (the beginning of April in the Classic and Senior years).
- **Kinds:** blue (a stat and its cap; given at the start and very likely at inspiration), pink (an aptitude; an uma makes one only for an aptitude she has at A or better), green (a parent's unique, given at the start; hints from parents and grandparents at inspiration; only umas at 3★ or more make green sparks), white (skills, races, scenarios; only at the two inspirations; a hint already held gains a level, up to 5; a skill already learned gives a few stats instead).
- **At the start of a career (initial inspiration), nothing is rolled:** blue sparks add their stat at once, +5 / +12 / +21 for each 1★ / 2★ / 3★ spark on any of the six (both legacies and all four of their legacies). Pink sparks add up by aptitude across all six: 1–3 stars raise that aptitude 1 grade, 4–6 stars 2, 7–9 stars 3, 10 or more 4, never past A (so Dirt B needs one Dirt star anywhere in the lineage to start at A). Only the April inspirations can take an A to S (Umamusume Wiki, Inspiration; uma.guide, Sparks; both read 2026-10-27).
- **Who can be in the lineage:** a legacy can't be the trainee's own character, but a legacy's own legacy (a grandparent) can be (GameTora, Legacies).
- **Making pink sparks:** a finished uma's one pink spark comes at 1★ / 2★ / 3★ 20 / 70 / 10% (uma.guide, Sparks). Which aptitude: uma.guide says evenly among her A or S aptitudes; the Umamusume Wiki weights every grade (A 100%, S 105–110%, B 85–90%, C 75–80%, D 60–70%, E 40–50%, F 20–30%, G 10%), so B and C aptitudes dilute it too. Either way an uma with few high aptitudes is the surest way to make a chosen pink: Haru Urara, A only in Dirt, Sprint and Late, makes a Dirt pink at best one time in three.
- **Base chance per inspiration at 0 affinity, for 1★ / 2★ / 3★:** blue 70 / 80 / 90%; pink 2 / 3 / 5% (uma.guide's Parenting guide) or 1 / 3 / 5% (uma.guide's Sparks page and IceDynamix's sheet; the app uses 1% at 1★); green 5 / 10 / 15%; race 1 / 2 / 3%; white skill and scenario 3 / 6 / 9%. **Chance = base × (1 + affinity / 100)**, rolled separately for each parent and grandparent, so grandparents proc less.
- **Affinity** = base affinity + race affinity. **Base:** the game files every character into relation groups (style, distance, surface, dorm, classmates, the real horses' history…), each worth points (mostly 1 or 2; 7 or 8 for the big aptitude-like groups); two umas score the groups they share, and a grandparent scores only the groups shared by the trainee, its parent and itself, so it can't beat its parent. A slot holding the trainee's own character scores no groups. **Race:** +3 for every G1 both members of a link won (parent with each of its parents, and the two parents with each other); G2/G3 wins and epithets stopped counting with the Global balance patch of July 2026, and a race won in both years counts once (uma.guide, Sparks, read 2026-10-27; umareference, Affinity, Global edition updated 2026-08-26). **Each slot rolls with its own affinity:** a parent's covers its link with the trainee, the trios through its own parents, its link with the other parent and its shared G1s; a grandparent's is its trio plus the G1s it shares with its parent (uma.guide's Roster Viewer; umalab, github.com/iurker1758/umalab, checked against the Global client 2026-07-30). The game's symbol is for the whole lineage: ◎ over 150, ○ 51–150, △ 50 or less, though affinity counts linearly. IceDynamix's sample: a median base affinity of about 17 between two umas; a decent lineage reaches about 95 for a parent and 21 for a grandparent.
- **Spark reroll (Global, since 2026-07-01):** after a career, 30 TP rerolls its sparks once and you keep whichever set you prefer; it doesn't change the odds of 3★. Picking sparks directly (Spark Select, 100 carats) is JP-only so far (Game8, How to Reroll Sparks, 2026-06-30).
- **Making sparks:** blue's stat is random (20% each); its stars follow the stat: 1100+ gives 1★/2★/3★ at 20/70/10%, 600–1099 at 45/50/5%, under 600 at 90/10/0%. White and green stars follow the uma's rank: SS or above 20/70/10%, below SS 45/50/5% (so SS rank is worth more than a few extra shared races).
- **White spark generation:** a learned skill makes its white spark with a base chance of 20% (○ white), 25% (◎) or 40% (gold), times 1.1 for each time that white spark appears in the 6-uma lineage (all six: 35.4 / 44.3 / 70.9%; uma.guide's Parenting guide), or +2.5 / +2.5 / +5 points a copy (all six: 35 / 40 / 70%; its Sparks page). Skills merely learned in the lineage don't count; only sparks do.
- **Looping:** building a lineage around white sparks; uma.guide suggests keeping aptitudes at least Mile B+, Medium A, Long C+, Dirt C+, Turf B+.
- **What umaLord does with it:** finished umas imported from the game (`account/veterans.json`) list their sparks, parents and G1 wins in parents.md; `knowledge/data/affinity.json` holds the game's relation tables (`affinity.rs` does the sums; the assistant's `get_affinity` and `rank_legacies` use them). Skills from the plan's parents' white sparks (account/parents.md) are listed in Step 5 with "only if a parent's spark passes it on", and auto-pick buys them only when pinned, because a spark may never proc. Step 5's "Rating / parent" aim picks the skills worth the most evaluation points, for the rank that decides spark stars.

## Rating

Sources: uma.guide's skill data (`grade` in `knowledge/data/skills.json`); Umamusume Rating Calculator (dak, Neon, IceDynamix; v3.1.0, 2026-10-19), https://docs.google.com/spreadsheets/d/1AAZCVDMiCozDNKte-5JYSm7SWUeVJ0o87i1iDY1bnWM (community).

- An uma's rank comes from points for her stats (the calculator's "Maths and Formulas" tab lists the multipliers per stat band, up to 1800) plus each skill's **evaluation points** (`grade`: Warning Shot, a unique, is 240; purple skills subtract).
- The calculator's changelog shows a skill's points also depend on the uma's aptitude for the distance or style it's tied to (it groups aptitudes as S–A, B–C, D–F and G); the multipliers aren't published there, so the app's Rating aim counts `grade` as is.
- The calculator's author now points to daftuyda.moe/optimizer (Void's Skill Optimizer and Rating Calculator).

## Glossary

Help's "Words you'll see" shows this table. Keep three columns: the term, a plain meaning for someone new to the game, and where it comes from.

| Term | Meaning | Source |
|---|---|---|
| Uma, trainee | A horse girl. You train one in each career; that uma is the trainee. | general |
| Outfit | A version of an uma, like [Reeling in the Big One]. Each outfit has its own unique skill and aptitudes, and is trained separately. | rentry FAQ |
| Stars ★ | An uma's star rank, 1 to 5. Raising it makes her stronger, including her unique skill. Only 3★ umas pass their unique on as a green spark. | uma.guide, Parenting |
| Potential (P1–P5) | Unlocks an uma's own skills in steps. At P5 all of them are available to buy. | general |
| Aptitude | A letter grade, S down to G, for a surface, a distance and a running style. A or better is what you want; lower grades slow her down, and the style grade also scales her Wit. | GameTora, Race Mechanics |
| Running style | Front Runner leads from the start. Pace Chaser runs just behind the leaders. Late Surger waits mid-pack and moves up late. End Closer stays at the back and sprints at the end. | general |
| Stats | Speed, Stamina, Power, Guts and Wit. Past 1200 a stat counts half in races. | GameTora, Race Mechanics |
| Growth | A trainee's bonus to training gains in some stats, like Wit +20%. | uma.guide, Career Mechanics |
| Support card | Up to six cards go with you into a career: five of yours and one borrowed. They boost training and teach skills. | general |
| SSR, SR, R | Card rarity, best first. A maxed SR usually beats an SSR at LB0. | rentry FAQ |
| LB, limit break | Merging copies of a card raises its level cap. LB0 is one copy; LB4, also called MLB, is fully limit broken. | general |
| MLB | Max limit break (LB4): the card at full strength. | general |
| Friend card | A card borrowed from another player for one career. It can be any card, at their LB. | general |
| Hint | A card or event offering a skill at a discount: 10/20/30/35/40% off at levels 1–5. | uma.guide, Skills Explained |
| SP, skill points | Earned during a career and spent on skills at the end. | general |
| White, gold skills | Ordinary and stronger skills. A gold skill needs its white version first. | uma.guide, Skills Explained |
| Green skill | A passive skill that adds stats when its condition fits (ground, weather, season, racecourse). It needs no Wit check and can push a stat past 1200. | uma.guide, Skills Explained |
| Unique skill | Each outfit's own skill. As a parent, an uma can pass a weaker copy on. | general |
| Evolved skill | A stronger version of a skill some umas can unlock during a career by meeting its conditions. | general |
| Wit check | The roll every non-green skill makes to fire: max(20, 100 − 9000 / Wit) %. | uma.guide, Skills Explained |
| Base duration | How long a skill lasts in a 1000 m race; it scales with the distance (2.4 s becomes 5.76 s at 2400 m). | uma.guide, Skills Explained |
| Cooldown | How soon a skill can fire again: usually never in the same race; some can, after 30 s × distance / 1000. | uma.guide, Racecourse Analysis |
| Phases | Early race (first sixth), mid race (to two thirds), late race and last spurt (last sixth). Skills often fire only in one. | GameTora, Race Mechanics |
| Accel zone | Where the last spurt starts, at two thirds of the race: acceleration skills that fire here win races. | uma.guide, Racecourse Analysis |
| Last spurt | The final push from two thirds of the race, at top speed if she has the HP for it. | GameTora, Race Mechanics |
| Full spurt | Running the whole last spurt at top speed. It needs enough HP, which comes from Stamina. | GameTora, Race Mechanics |
| Carry-over | A speed skill that fires just before the spurt and lasts into it, so the spurt starts faster. | GameTora, Race Mechanics |
| Gambling accel | An acceleration skill that fires at a random point in the late race, so it isn't always useful. | uma.guide, Racecourse Analysis |
| Position keep | How umas hold their place in the early and mid race, by switching between speed modes. | GameTora, Race Mechanics |
| Pace Down mode (PDM) | A position-keep mode that slows an uma that is too close to the leaders. | uma.guide, Race Mechanics |
| Rushed | When an uma gets over-eager in the early or mid race: she burns HP 1.6 times as fast for up to 12 s. Higher Wit makes it rarer. | GameTora, Race Mechanics |
| Spot struggle | Front Runners side by side early in the race fight for the lead: faster, but they burn HP. | GameTora, Race Mechanics |
| Duel | Umas side by side on the final straight push each other: more speed, from Guts. | GameTora, Race Mechanics |
| Late start | A start delay of 0.066 s or more: the uma loses the start's acceleration. Wit doesn't change it. | GameTora, Race Mechanics |
| Strong Start | A start delay under 0.02 s; worth 1000 points in Team Trials. | GameTora, Race Mechanics |
| Lengths (L) | How far ahead one uma finishes. One length is 2.5 m. | GameTora, Race Mechanics |
| Conditions | The ground (Firm to Heavy), weather and season of a race. Some skills only work in some conditions. | general |
| Sparks | What a parent passes to the next trainee: blue for stats, pink for aptitudes, green for her unique skill, white for skills and races. | uma.guide, Sparks |
| Parent | A saved, finished uma used as a legacy for a new career. Two parents give their sparks. | general |
| Inspiration | The events at the start of April in the Classic and Senior years when parents' sparks can proc. | uma.guide, Sparks |
| Affinity | How well a parent and her own parents match. It multiplies every spark's chance: × (1 + affinity / 100). | uma.guide, Parenting |
| Rank | An uma's rating grade after a career (G up to SS and beyond), from her stats and skills. SS makes better spark stars. | uma.guide, Parenting |
| Scenario | The career's training mode, like URA Finale or Grand Concert. Each one trains differently. | general |
| Scenario link | A character the scenario favours: her card or outfit earns extra bonuses in that scenario. | uma.guide, Career Mechanics |
| Fast Learner | A career condition that makes every skill 10% of its base price cheaper. | uma.guide glossary |
| Summer camp | July and August of the Classic and Senior years, when every training facility is at its best level. | uma.guide, Career Mechanics |
| Champions Meeting (CM) | A monthly player-versus-player event on one course with set conditions: 3v3v3 races over two rounds and a final. | uma.guide glossary |
| Open and Graded League | The two leagues of a Champions Meeting: Open for lower-ranked umas, Graded for any. | uma.guide glossary |
| Group A, Group B | Champions Meeting brackets: 3 wins in one entry of Round 1 puts you in Group A for Round 2. | uma.guide, Champions Meeting Basics |
| Ace | Your main uma for a PvP event; in Team Trials the top slot of each distance, which scores 10% more. | uma.guide, Team Trials |
| Team Trials | Weekly races of your team of umas against other players' teams, scored on placing and skills. | uma.guide, Team Trials |
| League of Heroes | A ranked player-versus-player event: daily 3v3 races with NPCs, for points. | uma.guide glossary |
| Meta | What most strong players currently run. The app's ★ marks come from a community sheet of PvP decks, umas and skills per Champions Meeting. | general |

## Did you know?

Home's "Did you know?" card shows one of these a day. One tip per line, ending with its source in brackets.

- Past 1200, a stat counts half in races: 1500 Speed works like 1350. [GameTora, Race Mechanics Handbook]
- Speed only matters in the last third of a race; early on, everyone runs at about the same pace. [GameTora, Race Mechanics Handbook]
- Wit doesn't affect late starts: Concentration and Focus do. [GameTora, Race Mechanics Handbook]
- A rushed uma burns HP 1.6 times as fast for up to 12 seconds. More Wit makes it rarer. [GameTora, Race Mechanics Handbook]
- Pace Chasers turn Stamina into the least HP of any style, about 11% less than Late Surgers. [GameTora, Race Mechanics Handbook]
- Recovery skills heal a share of maximum HP, so they're worth more on an uma with more Stamina. [GameTora, Race Mechanics Handbook]
- An acceleration skill when the last spurt starts can be worth 3.5 lengths; the same skill mid-race is worth almost nothing. [GameTora, Race Mechanics Handbook]
- At 1000 Wit a skill fires 91% of the time; at 400 Wit, 77.5%. Green skills always fire. [uma.guide, Skills Explained]
- The last corner of every course is "corner 4", counting back from the finish. [GameTora, Race Mechanics Handbook]
- A Champions Meeting's 4th daily entry costs 30 carats, and three wins in one entry put you in Group A. [uma.guide, Champions Meeting Basics]
- In Team Trials a gold skill firing is worth 1200 points: a 2nd place with skills can outscore a win without them. [uma.guide, Team Trials]
- Mixing running styles in a Team Trials team earns up to 3000 extra points per uma for good positioning. [uma.guide, Team Trials]
- In Team Trials, always pick the strongest opponent: the opponent rating bonus multiplies every point. [uma.guide, Team Trials]
- A spark's chance is multiplied by 1 + affinity / 100, and grandparents roll separately, so they proc less. [uma.guide, Parenting in Umamusume]
- An SS-rank uma makes 2★ white sparks 70% of the time; below SS, 50%. [uma.guide, Parenting in Umamusume]
- Learning the gold version of a skill doubles the chance it becomes a white spark (40% instead of 20%). [uma.guide, Parenting in Umamusume]
- Missing a unique-skill level-up event in the Senior year still gives 20–25 SP. [uma.guide, Career Mechanics]
- In Grand Concert each song learned before a concert pays 25 SP there, and each technique 5 SP. [uma.guide, Grand Concert Guide]
- In Grand Concert, races give no performance points, so extra races cost you lessons. [uma.guide, Grand Concert Guide]
- For a Champions Meeting's rounds, simulate at Normal mood; save Great mood for the finals. [uma.guide, Umalator 101]
- Chukyo 1200m's final straight starts 12 m before the late race, so a skill that fires on it carries speed into the spurt. [uma.guide, CM21 pre-guide]
- A course-specific skill is only worth it in Team Trials if it works on about 70% of that slot's courses. [uma.guide, Team Trials]
