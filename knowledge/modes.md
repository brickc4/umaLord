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

Sources, read 2026-10-28: uma.guide, Team Trials (Global, updated 2026-10-05); umareference, PvP Team Trials (Global edition, Erzzy, updated 2026-08-26, with its Uma Distances / Styles page and one page per race); Game8, Team Trials (Global, updated 2026-08-14). The planner's copy of the community picks is `knowledge/data/tt_meta.json`; `knowledge/UPDATING.md` says how to refresh both.

- **Format (rules):** five races, Sprint, Mile, Medium, Long and Dirt (Dirt runs 1600 to 1800 m). One runner per race at first, a second from Class 2, a third from Class 3: 15 at most, every uma once (two outfits of one uma count as the same uma). The top slot of each race is its **Ace**, with +10% of her score, so a full team has five Aces. Winning three of the five races adds 10,000.
- **Points (rules):** placement 10,000 / 8,000 / 7,000 / 6,000 / 5,000 / 4,000 ×2 / 3,000 ×2 / 2,000 ×3; team bonus 5,000 when all three finish top 3, else 4,000 when all three finish top 5; **skill activations** (gold 1,200; white or green 500; inherited unique 500; her own unique 2,000 (level 1) to 2,600 (level 6) at 3★ or more, 1,500 to 2,000 below); Strong Start 1,000; good positioning 1,000 in each of early, mid and late race **per running style on the race's team**; beating the reference time, winning margin and Dark Horse (`knowledge/mechanics.md`). Multipliers (added together): the Ace, card levels, win streaks (+2% to +5%) and the opponent rating bonus (always pick the strongest opponent you can still score against). The weekly high score decides promotion; the lines move every week.
- **So TT wants different umas from CM:** consistent runners whose unique and skills reliably fire, an A in their race's surface and distance and their style, three different styles in each race (a repeated style loses that runner's positioning points, up to 3,000), and enough stamina for random tracks. A runner that places 4th and fires many skills can outscore the winner. Uniques with strict place conditions score poorly: Mejiro Ryan's, King Halo's Prideful King (unrushed, 4th to 8th with exactly 200 m left), Air Groove (Wedding)'s. Consistent ones: Special Week's Shooting Star; unique recoveries (Super Creek's Pure Heart, Mayano Top Gun's Breath of Fresh Air) help Long.
- **Tracks (umareference, Global):** Sprint 1200 m 54%, 1400 m 38%, 1000 m 8%; Mile 1800 m 58%, 1600 m 37%; Medium 2000 m 63%, 2200 m 20%, 2400 m 13%; Long 2600 m 37%, 2500 m 18%, 3000 m 17%, 3400 m 11%, 3600 m 9%, 3200 m 8% (44% of Long races are 3000 m or more); Dirt 1800 m 55%, 1700 m 35%, 1600 m 10%.
- **Stamina guide (uma.guide):** Sprint 400–450; Mile 600–650; Medium 700–800 (or 600–700 plus a gold recovery); Long 800–900 plus a gold recovery (or about 700 plus two). Long tracks are mostly 2600 m or shorter but reach 3600 m.
- **Builds (uma.guide, community):** Grand Concert is the scenario for TT runners until most of the support grid is UF or above. Inherit **564 Escapades** (Summer Gold Ship's unique, a white skill that fires a random gold in the second half: up to 1,700 points) on every member; without a Summer Gold Ship of your own, borrow a friend's finished one as one parent. Skills: generic ones that fire on most tracks (Professor of Curvature, Lane Legerdemain), Concentration for Strong Start; avoid track-dependent ones (Blinding Flash, Super Lucky Seven). Borrows: Light Hello first in Grand Concert (even the R, uncapped); by race Curren Chan (Sprint), Taiki Shuttle (Mile, Mile Maven), Oguri Cap (Medium); by style Maruzensky (Front), Agnes Tachyon or Fine Motion (Pace), Eishin Flash (Late), Mr. C.B. (End); Mihono Bourbon's free card for Concentration.
- **Runners by race (umareference, community):** Sprint: Sakura Bakushin O (Front), Curren Chan (Pace), Air Groove (Late or Pace), King Halo (Late or End; unreliable unique). Mile: Taiki Shuttle (Pace), Maruzensky (Front), Silence Suzuka (Front), Vodka (Late), Daiwa Scarlet (Front), El Condor Pasa (Pace or Front), Gold City (Late). Medium: Agnes Tachyon (Pace), Special Week (Pace or Late), Winning Ticket (Late), Tokai Teio (Late; Beyond the Horizon Pace), Narita Taishin (End), Nice Nature (Late), Seiun Sky (Front). Long: Gold Ship (End), Super Creek (Pace), Mayano Top Gun (Front or Pace), Mejiro McQueen (Front or Pace), Biwa Hayahide (Pace), Matikanefukukitaru (Late). Dirt: Smart Falcon (Front), Haru Urara (End or Late), El Condor Pasa (Pace or Front), Oguri Cap (Late; Pace with a strong Front Runner ahead), Taiki Shuttle (Pace), Maruzensky (Front). Summer Maruzensky fills Medium, Mile, Dirt or Long as a Front Runner. The full list with outfits is `tt_meta.json`.
- **New player:** first one usable runner per race (five), climb to Class 3, then fill toward 15. **Weekly loop:** check Score Info, set each race's highest average scorer as its Ace (subtract the current Ace's 10% when comparing), and replace the weakest runner or skill.
- **What the planner does (`parentage/tt.rs`):** candidates per race from the account (the community's runners for that race first, then A grades, then B ones whose parents can lift them), each built for TT (Grand Concert, the stamina above, the meta's borrow, skills weighted for consistency), raced head to head on the race's most common track, and scored with the points above (inconsistent uniques at 40%); the team fills the races with the fewest candidates first, mixes styles, makes each race's top scorer its Ace, gives every runner a lineage plan with 564 Escapades (or how to borrow it), and names the next career that raises the team most.

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

## Manual or Independent Training

Global's Independent Training (`mechanics.md`) runs a career by itself: use it for parent careers, sparks and farming; run CM aces and Team Trials runners by hand, since manual careers come out much stronger (Cygames). umaLord's career briefs say which, and what to set for an Independent Training run.

## How the assistant picks the mode

When the user names a goal, use its section. When they ask broadly ("help me get competitive"), cover the next CM (the deadline), Team Trials (the weekly score, the steady reward), and the parenting that serves both, since one good parent line feeds every mode. Say which mode each recommendation is for.
