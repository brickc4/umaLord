# Keeping umaLord's knowledge current

The game changes: new umas and cards every few weeks, a Champions Meeting every three, balance patches, and a community meta that moves with all of it. This is where every piece of the app's knowledge comes from, what refreshes itself, and how to refresh the rest. Do the checks under **When** whenever one of those events happens, and at least once a month.

## What refreshes itself

| Data | From | How often | Where it lands |
|---|---|---|---|
| Champions Meeting schedule, conditions, turned-off skills | umamusume.com's official news; Umalator's presets | on launch, when over 6 hours old (`live.rs`) | the app's cache (`events.json`) |
| Every uma, card, skill and race, and their art (the weekly pack) | `.github/workflows/data-pack.yml`: runs `tools/update_knowledge.py` and the tests every Monday, then `tools/make_data_pack.py` publishes the "data-pack" release | weekly, only files whose SHA-256 differs (`datapack.rs`; Settings → Data and updates → Weekly pack, Check now) | the data folder's `knowledge/data/` and `app/assets/` |
| Art for every released uma and card in the data | uma.guide (icons, card thumbnails) | once per launch, for what's missing (`tab_cards::sweep_art`) | the data folder's `app/assets/` |
| Affinity, spark and win-record tables | uma.guide's data chunk | weekly (`live.rs`, `spawn_affinity`) | the app's cache; `knowledge/data/{affinity,sparks,saddles}.json` are the bundled copies |

Settings → Data and updates shows each one's age and has Refresh now.

## What a script refreshes (run from the repo root)

`python -I -X utf8 tools/update_knowledge.py` downloads uma.guide's data and writes, as text, without running the site's code:

- `knowledge/data/umas.json`, `cards.json`, `skills.json`, `races.json`, `glossary.json`, `meta.json`: every uma, card, skill and race, and the art of any new uma or card into `app/assets/` (`cargo test every_uma_and_card_has_art` checks none is missing). Each outfit's unique is the game's own id (1, outfit − 1, the chara id's last three digits, 1), which the game's spark table confirms; `cargo test uniques_match_the_spark_table` checks it.
- `cm_schedule.json`, `umalator.json` and `sim/*.json` (Umalator's courses, presets and simulator data).
- `--only affinity`: affinity, sparks, saddles. `--only meta`: `pvp_meta.json`, the community CM sheet (decks, runners, stats, uniques per CM and style). `--only tiers`: `skill_tiers.json`, the community skill tier list. `--only cm`, `--only umalator`: one part.

Run the whole script after a release of new umas or cards, and `--only meta` when a new CM's sheet tab appears. Then `cargo test --release` (it parses everything), and look at `git diff --stat knowledge/data`: a big unexpected change means the site's format moved.

## What a person curates (research, then edit)

These files are written from community guides, each claim with its source and the date it was read. The app reads them; nothing regenerates them.

| File | What | Sources |
|---|---|---|
| `knowledge/data/tt_meta.json` | Team Trials: each outfit's usual races and styles, unreliable uniques, track lengths per race, stamina targets, the scenario, the inherit (564 Escapades), friend-card borrows, skills to prefer and avoid. The Team Trials planner (`parentage/tt.rs`) reads it. | uma.guide Team Trials; umareference PvP Team Trials (Uma Distances / Styles, one page per race); Game8 |
| `knowledge/modes.md` | What each mode rewards and how to plan for it (CM, Team Trials, League of Heroes, Legend Races, scenarios) | the same, plus GameTora and Game8 per mode |
| `knowledge/parenting.md` | How lineages work and how to plan parents; the planner's rules | uma.guide Sparks and Parenting; umareference; GameTora; Game8; Umamusume Wiki |
| `knowledge/mechanics.md` | Race, career, spark, Team Trials scoring and rating mechanics; the glossary and tips | as cited per section |
| `knowledge/scenarios.json` | Each scenario's training values and the career sim's assumptions, each marked sourced or assumed | GameTora's scenario pages; tune against real careers |

### How to research a refresh

1. **Search** for the current Global guides, not JP ones: say "Global" and the month in the query. With the exa tools: one `agent_run` (effort medium, about $0.10) with an output schema of rules, picks and builds, each with a source URL, then `fetch` the two or three pages that matter with a `query` and **check every claim on the page itself**. A search result is a lead, not evidence.
2. **Prefer** pages that state their date and server: uma.guide's guides (dated at the bottom), umareference (Global edition, dated), Game8 (dated), GameTora. Treat Reddit and videos as leads to check, not sources.
3. **Separate** game rules (points, slots, formulas) from community opinion (tiers, picks, decks). Write JP-only things as JP-only, or leave them out.
4. **Edit** the file: update the facts, the source list, and `read` (the date). In `tt_meta.json` the roles use outfit ids from `umas.json`, cards use ids from `cards.json`; check every id exists (the app skips unknown ones).
5. **Test**: `cargo test --release`, then the planner's stress suites on several kinds of account (below), and read a few of their plans against the guide you just read. A plan that contradicts the guide (a runner the guide calls unreliable as an Ace, a style the guide wouldn't use) is a bug in the data or the heuristics.
6. **Record** what changed, and why, in the commit.

### What to look for each time

- **New umas or outfits:** add them to `tt_meta.json`'s roles once the guides place them; note new uniques that fire unreliably. A new outfit can change the inherit (Summer Gold Ship's 564 Escapades is today's).
- **New cards:** new borrows by race or style; whether Light Hello is still the first Grand Concert borrow.
- **A new scenario or a balance patch:** the TT scenario pick, stat caps (`mechanics.md`), stamina targets, skill point values, affinity rules (the July 2026 patch removed G2/G3 and epithets from affinity).
- **Team Trials rule changes:** points, slots, Ace, bonuses. Rules belong in `modes.md` and `mechanics.md`; the planner's point values are in `parentage/tt.rs` (`skill_points`, `POSITIONING`).
- **Independent Training:** what it lets the player set (prioritized skills, focus, agenda), how it wins races, and how its results compare with manual careers; the career briefs (`parentage/brief.rs`) follow `mechanics.md`'s section on it.
- **A new CM:** the meta sheet's tab (`--only meta`); nothing else to curate.

## Checking the planner after any refresh

The stress suites run on any account folder (`UMALORD_ROOT`); run them on a mature account, one with no finished umas, a small one and a very large one:

- `cargo test --release -- --ignored --nocapture parentage_real_account cm_team_real_account tt_team_real_account parentage_play_through briefs_fit_their_runners`
- Invariants they check: every route meets its goal, no legacy is its trainee's character, the 20-career cap holds unless the plan says otherwise, three different characters in a CM team, each character once in a Team Trials team with one Ace per race, every deck has a borrowed MLB friend card, every runner's skills can fire in her race, and every parent career's skills fit the runner who'll inherit them as white sparks. The play-through compares predicted careers with careers played on rolled sparks.

## Sources (current as of 2026-10-28)

- uma.guide, Team Trials: https://uma.guide/guides/team-trials (updated 2026-10-05)
- umareference, PvP Team Trials: https://www.umareference.com/guide/pvp-team-trials (Global edition, updated 2026-08-26), with Uma Distances / Styles and the Sprint, Mile, Medium, Long and Dirt pages
- Game8, Team Trials: https://game8.co/games/Umamusume-Pretty-Derby/archives/536831 (updated 2026-08-14)
- GameTora, Team Trials scoring: https://gametora.com/umamusume/team-trials-pvp-scoring
- The community PvP sheet (CM decks and runners): see `CLAUDE.md`, Knowledge data
