# Parenting playbook (Global)

How to plan parents (legacies) from a player's real account toward competitive runners. This is the assistant's method for "map out my parenting" questions; the numbers behind it are in `knowledge/mechanics.md` (Sparks and inheritance), and the tools do the arithmetic (`plan_lineage`, `get_affinity`, `rank_legacies`, `list_finished_umas`).

Sources, read 2026-10-27 unless dated otherwise: uma.guide, Parenting in Umamusume (JJAY, Pik, Apol) and Sparks & Inheritance (Bachoo, catbomb); umareference, Legacies and Advanced Breeding Strategy (Global edition); GameTora, Legacies; Umamusume Wiki, Inspiration; Game8, Legacy and Sparks Guide (2026-08-24) and How to Reroll Sparks (2026-06-30). Global only: anything JP-only is marked.

## What a lineage does

- A lineage is the trainee, two legacies (parents) and each legacy's two legacies (grandparents): six umas. Their sparks pay out three times: at the start (fixed, nothing rolled) and at the two April inspirations (rolled, scaled by affinity).
- **The start is where aptitudes are fixed.** Pink stars of one aptitude, added across all six, raise it 1 grade per 1–3 stars, 2 per 4–6, 3 per 7–9, 4 per 10+, never past A. One Dirt star anywhere turns Dirt B into A for the whole career. Plan aptitude fixes here, not on inspiration luck.
- **Blue sparks at the start** add +5 / +12 / +21 per 1★ / 2★ / 3★ spark of that stat; a "9★ blue" lineage of three 3★ sparks of one stat adds 63 to it (nine stars spread as 2+2+2+1+1+1 add 51) and raises its cap.
- **Green (unique) sparks** of the two legacies give the trainee that inherited unique from the start; grandparents' greens can only proc at inspirations. Only umas at 3★ or more make a green spark.
- **White sparks** (skills, races, scenario) only proc at inspirations, as hints (or stats if the skill is already learned).
- **Inspirations:** each spark rolls base × (1 + slot affinity / 100), per slot; base 1★/2★/3★: blue 70/80/90%, pink 1–2/3/5% (sources differ at 1★), green 5/10/15%, race 1/2/3%, white and scenario 3/6/9%. An A aptitude can become S only here.

## What a parent run makes

- One blue spark (stat picked evenly among five; stars by that stat's final value: 1100+ gives 2★ 70%, 3★ 10%).
- One pink spark, 20/70/10% for 1/2/3★. uma.guide says it's picked evenly among her A/S aptitudes; the Umamusume Wiki weights every grade (A 100%, S 105–110%, B 85–90%, C 75–80%, lower 10–70%), which lets B and C aptitudes into the draw, so treat "1 in (number of A/S aptitudes)" as the best case. **To make a chosen pink, train an uma with few A aptitudes that include it**, and don't raise other aptitudes to A with the lineage's own pinks.
- One green spark if she's 3★+, and white sparks for skills she learned (20% for a white skill, 25% for ◎, 40% for gold, rising with each copy of that white spark already in her six-uma lineage to about 35 / 40–44 / 70% at six copies; uma.guide's Sparks page steps it +2.5 / +2.5 / +5 points a copy, its Parenting guide ×1.1 a copy), races she won and the scenario. White and green stars follow her rank: **SS or better gives 2★ 70% of the time**, below SS 50%.
- **Spark reroll** (Global since 2026-07-01): after the career, 30 TP rerolls its sparks once and you keep the better set. It doesn't change the odds, so it's one more roll for 30 TP, cheaper than a career's time. Spark Select (choosing sparks for carats) is JP-only so far.

## The method (start to end)

1. **Goal from the calendar.** List the next Champions Meetings (`list_upcoming_events`; announced vs estimated) and League of Heroes if any. A CM under ~5 days away gets no new parents: race it with what exists. Later ones are targets.
2. **Runner per target** from the account: the meta sheet's recommended umas that are owned (`get_cm_meta`, `recommend_trainees`), else the best aptitude fit. Prefer runners already A (or one grade short) in the course's surface, distance and her style; a two-grade gap needs 4+ pink stars.
3. **The lineage she needs:**
   - pink: every aptitude below A for the target, with enough stars (1–3 per grade);
   - blue: the stats her deck and growth leave short of the meta's targets (usually Stamina, Power, Wit);
   - green: one inherited unique the meta names for that course and style, or a universal one;
   - white: the meta's key skills she can't get from her own kit or deck;
   - affinity: aim for ◎ (over 150) on the lineage; shared G1 wins add 3 each.
4. **What she has:** check the account's finished umas (`list_finished_umas`, `rank_legacies` with `finished: true`) and score real lineages with `plan_lineage` (aptitudes after the start, blue stats added, each spark's inspiration odds). Grandparents come with each finished uma: choosing two legacies chooses all six.
5. **Gaps → parent runs.** For each missing piece, the cheapest run that makes it: the trainee whose A aptitudes make the needed pink likely, whose 3★ green is the wanted unique, and whose deck carries the wanted white skills (gold where possible). Give the odds per career and the expected careers: 1 ÷ p, or with a reroll 1 ÷ (1 − (1 − p)²).
6. **Borrow before you build.** Five guest legacies a day (uma.guide, Daily tasks): a friend's parent with the needed 3★ blue or pink fills a slot today. Build your own for what you'll reuse.
7. **Order and budget.** A career costs 30 TP; TP refills about 4.8 careers a day. Put the runs that unlock the nearest target first, and say how many days the queue takes.
8. **Check and loop.** After each parent run, import the finished umas, re-score the lineage, and drop or keep the next run. A good finished runner is next season's parent.

## How to run a parent career

- **Scenario:** Trackblazer for most parent runs (no forced races, consistent SS, and the G1s you choose raise affinity; about 35–40 min). Unity Cup to seed many white skills (lots of hints, gold scenario skills). URA Finale is fast but makes SS and SP harder. Grand Concert (Global since 2026-07-22) is playable but guides rank it behind those two for parenting.
- **Deck:** built for the run's job, not the strongest six: enough SP and stats for **SS rank**, and the white skills to pass on (gold versions). Step 5's "Rating / parent" aim picks skills for rating.
- **Races:** win the G1s the planned partner lineage also won (each shared G1 is +3 affinity on that link).
- **Keep:** a run with the target pink at 2★+, or a 3★ blue of a needed stat, or the wanted green at 2★+, is a keeper even if the rest is ordinary. Don't throw away the right package chasing 9★ blue.

## Progression for a new player

1. Reuse finished aces as parents, with one borrowed guest legacy covering the weak spot.
2. Repeat one trainee with your best parent and the same borrow until she has the needed pink and a 2–3★ blue.
3. Dedicated parent careers (scenario, deck and races above).
4. Alternate two lines (a new parent from line A builds line B, and back).
5. Loop white skills across all six slots (6/6 copies of a key skill), keeping aptitude sparks under control.

## Mistakes to avoid

- Chasing 9★ blue and discarding the right pink, green or white package.
- Raising extra aptitudes to A in a parent run, which dilutes the pink you want.
- Ignoring affinity, or using a 3★ parent with △ affinity over a 2★ one with ◎.
- Rerolling by reflex: it's 30 TP for one more roll, worth it on runs that are close.
- Planning around JP-only features (Spark Select, later scenarios, the JP calendar).

## Inherited uniques the community rates (community meta, 2026-10-27)

Opinion from uma.guide's Unique Skill guide, Game8 and translated JP CM guides; check each CM's sheet (`get_cm_meta`), since course and style decide them. Names are as the game data spells them, so the tools find them.

| Unique | From | Rated for |
|---|---|---|
| Angling and Scheming | Seiun Sky [Reeling in the Big One] | Front runners whose late race starts in a corner |
| Victoria por plancha ☆ | El Condor Pasa [El☆Número 1] | Front and Pace on short and mile courses |
| Moving Past, and Beyond | Mejiro Dober | Mid-pack acceleration |
| Let's Pump Some Iron! | Mejiro Ryan [Down the Line] | Back runners in the late race (narrow rank window) |
| Louder! Tracen Cheer! | King Halo [Cheerleader in Noble White] | Late and End on short and mile courses |
| Superior Heal | Grass Wonder [Saintly Jade Cleric] | Long races (recovery) |
| Pure Heart | Super Creek | Recovery near the front |
| U=ma2 | Agnes Tachyon [tach-nology] | Recovery backup (Pisces Cup Oguri builds) |
| 564 Escapades | Gold Ship [Run!Run!Classic] (summer) | Team Trials score, not CM racing (a green that fires a random gold) |

Guidance for CMs past the next one comes from JP's runs of the same cup (Uma Musume Hub, mihonostudios.com/champions-meeting/<n>): label it a projection. The scenario uma.guide lists for a future CM is JP's; Global's current scenario is Grand Concert until announced otherwise.
