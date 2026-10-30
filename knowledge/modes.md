# Goals by mode (Global)

What each part of the game asks of the player's umas and parents, and how the assistant plans for it. The rules behind each (scoring, formats, sparks) are in `knowledge/mechanics.md`; parents follow `knowledge/parenting.md` whatever the goal.

Sources, read 2026-10-27: uma.guide, Team Trials, Team Trials for beginners, Champions Meeting Basics, Next Steps, Parenting in Umamusume, Grand Concert and Updated URA & Unity; Game8, Champions Meeting Guide, Legend Race Guide and July 2026 Balance Adjustments; GameTora, League of Heroes and July 2026 rebalance. Global only; JP-only is marked.

## Champions Meeting (CM)

- **What it optimises:** winning one 3v3v3 race on a known course (any one of your three finishing first wins). Every CM is its own course, conditions and meta.
- **Beginner goal:** play every entry, reach a final, build one reliable ace and two serviceable runners. **Competitive:** Group A finals with three course-specialised runners.
- **Plan:** the course and conditions (`list_upcoming_events`) → the meta sheet's runners, styles and stats (`get_cm_meta`) → a runner per style from what's owned → her lineage (pinks for the course's surface, distance and her style; blues for the stats short of the sheet's; the green the sheet names) → deck and skills (Career → Plan, `auto_pick_skills` on the CM course) → simulate (`run_compare`).
- **Team of three:** an ace plus two that either also threaten to win or support (pace, debuffs); no single best composition is sourced.
- **League:** Graded League takes any uma; Open League caps the rank (A+ or below by the newer source). Open only makes sense when you deliberately build under the cap.
- **Cadence:** about every three weeks on Global in 2026 (registration, two rounds, final over 11 days).

## Team Trials (TT)

- **What it optimises:** one weekly score across five races (Sprint, Mile, Medium, Long, Dirt), up to three runners each once Class 3 unlocks (one each at first, a second row at Class 2). Points come from placements, team bonuses, **skill activations** (gold 1,200, white or green 500, inherited unique 500, own unique 1,500–3,000), beating the reference time, winning margin, good positioning per running style, Strong Start and a long shot, multiplied by the Ace, card levels, win streaks and the opponent's rating (always pick the strongest opponent offered).
- **So TT wants different umas from CM:** consistent runners with many skills that reliably fire and a reliable unique, styles mixed within each distance (positioning points per style), enough stamina for random tracks, and an A in each one's surface, distance and style. A winner that fires few skills scores less than a top-6 runner that fires many.
- **Stamina guide (random tracks):** Sprint 400–450; Mile 600–650; Medium 700–800 (or 600–700 plus a gold recovery); Long 800–900 plus a gold recovery (or about 700 plus two). Rare 3600 m Long tracks need more.
- **Beginner goal:** fill one competent runner per distance, promote and retain each week. **Competitive:** all 15 slots, three styles per distance, high skill counts, strongest opponents.
- **Plan:** `find_umas` per distance and surface (A or better) → assign up to three per category with different styles → for each, the lineage: the pinks her category needs, blues for stamina where she's short, and **564 Escapades** (Summer Gold Ship's unique, a green spark that fires a random gold: the community's favourite inherit for every TT runner) or other reliable greens → deck for stats and many affordable skills; Step 5's "Team Trials" aim weights skill gains by how often they fire.
- **Scenario:** Grand Concert is uma.guide's pick for TT builds until most of the roster is UF rank or better.
- **Weekly loop:** after each week, look at where points were thin (the game's Score Info) and replace the weakest runner or skill.

## League of Heroes (LoH)

- **Global status as of 2026-10-27: not confirmed.** The meta sheet in `pvp_meta.json` has LoH tabs, but no Global announcement was found; say so before planning for it.
- **JP-only format (for orientation):** three runners scored on every one of their placements in 12-runner races over a week of tickets, a Hero gauge and hero skills, top 96 to an Extra Stage. So it rewards three consistent high placements, not one ace. Don't use JP thresholds or dates for Global.

## Legend Races

- **What it optimises:** beating one fixed opponent on a fixed track (PvE). First clears give the big rewards; Daily Legend Races repeat for star pieces of one character.
- **Plan:** a finished uma with the track's surface, distance and style at A, stamina with margin and reliable acceleration; a CM runner often fits. Clear every unclaimed race first.

## Training scenarios (which to train in)

| Scenario | On Global | Best for |
|---|---|---|
| URA Finale | since launch (updated July 2026) | fast careers; URA-only skills; parents when time matters more than rank |
| Unity Cup | since 2025 (overhauled July 2026) | many hints and two free gold scenario skills: seeding white-skill loops; front runners |
| Trackblazer | 2026 | no forced races and consistent SS: most parent runs, and lining up shared G1s for affinity; the longest careers (35–40 min) |
| Grand Concert | since 2026-07-22 (current) | the strongest stat builds now: CM and TT runners; Light Hello's link card is mandatory |

- July 2026 raised stat caps per scenario and made gains past 1200 count about half (not 1/8; Game8, July 2026 Balance Adjustments); Power past 1200 enables Fully Charged. Exact caps per scenario: GameTora's July 2026 rebalance guide.
- Later JP scenarios (Grandmasters and on) aren't on Global; a future CM's listed JP scenario is a projection.

## How the assistant picks the mode

When the user names a goal, use its section. When they ask broadly ("help me get competitive"), cover the next CM (the deadline), Team Trials (the weekly score, the steady reward), and the parenting that serves both, since one good parent line feeds every mode. Say which mode each recommendation is for.
