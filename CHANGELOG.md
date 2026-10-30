# Changelog

What changed in each release. Releases are built from the tagged source by GitHub Actions; download them from [Releases](https://github.com/brickc4/umaLord/releases).

## [Unreleased]

### Added

- **Lineage, a new tab, answers "what career should I run next, and how?"** for a Champions Meeting team, a Team Trials team or one runner. Its Next career card names who to train and why, then gives:
  - the parents, scenario, deck with a borrowed MLB friend card, and skills
  - how to run it: Independent Training for parent careers, with a setup panel to copy into the game (training focus, up to 10 prioritized skills to tick off, and the race agenda with each race's win chance); a manual career for runners
  - what to keep her for, as spark chips drawn like the game's, and the odds
- **Team plans are saved, and nothing simulates until you ask.** The tab opens on an Overview of where each plan stands (Champions Meeting, Team Trials, one runner) with what you did lately and Undo. A CM or Team Trials view shows its saved plan: mark careers done as you finish them, keep or leave out runners (the edits wait for a re-plan), re-plan or reset. The app tells you when something changed under a plan, such as a newly imported uma that meets a career's must-haves or a newer meta sheet, and lets you choose what to do. One plan per Champions Meeting.
- **A plan is laid out step by step, in plain words.** Each team plan starts with what it's for (the event, your score today and after the plan), then every career in order: who to train, her own career or a parent career, why, how many tries it usually takes and which race it helps. Show how opens a step's details. "Your team, before and after" shows, race by race, your best finished umas racing now beside the team after the plan. Game terms explain themselves when you point at them.
- **Click a card or an uma's picture for everything about it.** Anywhere in Lineage, on Home, and on the planner's trainee (right-click a deck slot), a pop-up shows the large art, what you own of it, why it's shown there, and its effects, skills, events or aptitudes, plus an uma's finished careers and support cards. Click outside it, press Esc or ✕ to close it; a card opened from inside it has ← Back. An uma's pop-up shows her full-body art (downloaded the first time you open her, from GameTora) beside her aptitudes and stats.
- **Re-plan asks first, and the old plan steps aside while the new one is made.** The confirmation says what Re-plan keeps (careers marked done, runners kept or left out, edits waiting) and that Reset starts over without them.
- **A runner's career card lists her stat goals as a table:** what to aim for in each stat, the Stamina she must reach, and the order to train them. A parent career's table marks the stats her blue spark wants at 1,100.
- **The career card gets the room on a wide window:** it grows to 40% of the width (up to 1,200 px) and splits into two columns when it can, while the plan beside it stops at a readable width. Drag the gap between them to resize; double-click it to reset.
- **A runner's parents say which is yours and which to borrow,** and 564 Escapades explains itself where it's asked for.
- **A runner's career card lists her races:** her objectives (the career ends if she misses one), then the G1s that fit her for fans. The card's "Start this career" button is now "Open the skill shop helper" at its end, for picking her skills with her real stats and SP.
- **Every plan says how to raise its ceiling:** after planning, umaLord tries the umas and cards you don't have, cards you haven't limit broken, and better parents to borrow, measures each in the same simulations the plan used, and lists what would raise it most: points a round for Team Trials, lengths a race for Champions Meeting, careers or stats for one runner.
- **Planning waits until your account is set up:** with no umas or support cards entered yet, the Lineage tab says what to add and keeps Create plan off. With no finished umas imported, it warns that plans will be for a fresh account and offers the import.
- **You can watch a plan being made:** each stage ticks off with what it found and why (the runners chosen, who beat whom head to head, the parent routes, decks and the gain over today's team), and Cancel stops it without touching your saved plan. The log stays with the plan under "How this plan was chosen".
- **Teams are compared with what you can field today:** your finished umas as they are, raced in the simulator. Champions Meeting shows each runner's head-to-head share against today's best. Team Trials puts today and the plan side by side for each race, with the gain.
- **One runner's parent plan is tracked step by step.** If a step won't come, the Stuck panel offers to settle with the closest uma, try another route, or keep going. Each choice is previewed first and can be undone.
- **The layout suits any window:** two panes at 1200 px and wider (the card beside the overview, each scrolling on its own), one column below. Small text keeps 4.5:1 contrast in both themes.
- **Game data refreshes every week:** new umas and cards, with their art, arrive without a new release. The pack is built and tested by GitHub Actions; the app downloads only the files that changed, and checks each one.
- **Every uma and card has its picture:** the 90 cards that were missing art now ship with it, and anything new downloads in the background.
- The Ask tab's Claude backend can run on your Claude account: with Claude Code installed, questions go through it, signed in with its own login, a setup token from `claude setup-token`, or an API key. It runs isolated from your Claude Code settings, keeps one process per conversation so follow-ups start at once, and streams the answer.
- Builds and runs on Linux (Wayland) and macOS from source, tested in CI on all three. Settings live in the platform's own folder, times show in your local time zone, and the Ask tab uses `opencode` from your PATH.
- The app bundles small symbol fonts, so every icon draws whatever fonts the system has.
- Pulled another copy? A small ＋ beside a card's LB diamonds or an uma's stars in their details (or the + key) raises it by one and saves it, with Undo.
- The assistant plans parenting and Team Trials from your account: a sourced playbook for both, and exact calculations of what a lineage gives a trainee at the start, each spark's odds at the inspirations, and what a parent run will produce and how many careers it should take.
- Import your finished umas from UmaExtractor's data.json (or `tools/read_veterans.py` on Linux): each lands in your parents list with its sparks, parents and G1 wins, the assistant can rank them as parents for any trainee, and umas you've trained but hadn't listed are added.
- Affinity between a trainee and her parents and grandparents, from the game's own relation tables plus +3 per shared G1. The assistant can work out each slot's affinity and rank the best parents for a trainee, and the app refreshes the tables from uma.guide every week.

### Changed

- Ctrl+4 opens the new Lineage tab; Ask moved to Ctrl+5.

### Fixed

- **Parent careers recommended skills the runner couldn't use.** A parent's skills become her runner's white sparks, and 56% of the picks were tied to another style, distance or racecourse. Now every pick fits the runner, and tests check every brief.
- **Parent plans could cost more careers than they needed.** A route where the second career only has to finish was considered only when nothing else fitted, and re-planning could swap your next step for a costlier one. Predictions now match play-throughs within a few percent.
- Twelve second and third outfits (Grass Wonder [Saintly Jade Cleric], Oguri Cap [Ashen Miracle], Maruzensky [Hot☆Summer Night] and others) showed their first outfit's unique skill; each now has its own, matching the game's spark table.
- On Linux, Downloads and Pictures follow the desktop's own folders (`~/.config/user-dirs.dirs`).
- The planner's Upcoming CM legend puts ✔ announced and ? estimated on their own lines instead of wrapping mid-phrase.

## [0.2.0] - 2026-10-18

A big overhaul. Home becomes the hub for the next Champions Meeting, with its live schedule from the game's news; auto-pick races skills to find the set that gains the most together; the Race sim gets real charts; the assistant can plan, pick and simulate a career for you; and the collection, setup and settings are redesigned, with notes from the community's guides throughout.

### Added

- **Home is now the hub:** a greeting with quick actions; the next Champions Meeting with its course drawn (phases, corners, slopes), what the community meta sheet recommends, your best umas for it and how far your plan has got ("Continue plan" opens it); a timeline of the next three weeks from the game's news; and widgets: where you left off, your stable, cards by type, who can run where, an uma of the day, a trainer card with badges, your best and missing cards for the next CM, and a game term a day.
- **Live Champions Meeting schedule:** on launch the app reads the game's official news (umamusume.com) and Umalator's latest presets in the background. Home shows whether the next CM is announced, with countdowns to league selection, Round 1 and the end in your time zone, and the skills it turns off. The planner and Race sim chips read "Capricorn Cup · Oct 31". The status bar says how fresh the schedule is, and Settings → Data and updates can turn it off or refresh it. Without internet it uses the last fetch, or the bundled data.
- **Skills a CM turns off** are never auto-picked for it, are struck through in the plan's skill list, and are taken out of builds when a CM or a plan for it is loaded into the Race sim.
- **Auto-pick that optimises the set:** it races the candidates to find the skills that gain the most together, not one by one: each step adds the skill whose measured gain on top of those already chosen is best for its cost, then it tries swaps and checks the set in 1000 realistic races (wit checks and rushing on). Step 5 shows each skill's gain alone and in the set, how often it triggered, why it was or wasn't bought, and an Explain log, with the set's verified gain and its 95% range. Pin or ban skills; choose how much the meta sheet counts (Off, Tie-break, Strong); Fast Learner; and a Team Trials aim.
- **Skill shop mode:** at the end of a real career, enter your final stats and SP and tick the skills on offer with their hint levels (or paste the list); auto-pick works on exactly those.
- **Rating / parent aim for auto-pick:** picks the skills worth the most evaluation points within your skill points, for the rank that decides a parent's spark stars.
- **Aptitude warning:** Step 2 says when the trainee's grade is below B for the goal and what it costs her, from the race simulator's coefficients.
- **Race sim with real charts:** a compact Race setup panel; builds side by side with Swap, stat templates and the meta sheet's stats; and results in tabs. A bigger verdict with its 95% range and a note when it's within noise. New charts, all with axes, legends and hover: the gap of every race (or the cumulative share) with mean, median, P10 and P90; where along the course the lengths are won; speed with skill markers, the last spurt and where HP ran out; HP with rushed stretches and recovery skills; a timeline of where each skill fired, and why some never did; what each skill is worth and where the next training points matter (run on demand in the background); and stamina at the finish against what a full spurt needs. Plus a replay of the typical race, the last 10 runs with one pinned for comparison, Copy summary and Save chart as PNG.
- **The assistant works the app:** ask "Plan the next Champions Meeting for my best Pace uma and simulate it" and it starts a plan for the CM, picks the trainee, deck and skills, races the build in the Race sim and reports the result, with each step shown as a card in the chat (the deck's pictures, the verdict with a small chart, the skills) and buttons to open it in the Planner or Race sim or undo it. Saving a plan, adding a result or changing your roster asks first (Settings → AI assistant can allow saves and results). Claude's answers now stream in as they're written, with short notes between steps. It works with both backends: the free OpenCode models get the same tools through umaLord's MCP server (`umalord.exe --mcp`), which Claude Code and Claude Desktop can use too.
- **Collection redesigned:** filters as chips in a sidebar that folds away, quick views (built in, and your own), the filters that are on listed above the results, and sorting beside the results that matches the table headers. Umas filter by an aptitude grid (A or better, or S, at any surface, distance or style). Cards filter by limit break, by a skill they teach, by an effect's minimum, and by scenario links; hover a card to put it in your plan's deck or to compare two side by side. Tables show aptitudes as coloured grades under Surface · Distance · Style, and effects with units and a heat tint; they can hide columns or copy as CSV. Pictures come in three sizes, with group headers that stay on screen while you scroll.
- **Skills:** "Ones I can get | All skills", a course filter that keeps only skills that can trigger at a Champions Meeting or on any course (checked by the race simulator), owners shown as small pictures, durations at each race distance, the white → gold upgrade path, and "Test on a course" in the Race sim.
- **Add cards** shows every card's picture in a taller window; click a diamond on a card to save it at that limit break, and missing pictures download by themselves.
- **A faster, friendlier setup:** a step bar that stays on screen, a "Make it yours" step for your name, mode and theme, and quicker grids: set "New picks start as" (stars and potential, or a limit break) and every click adds an item that way; hover an added uma to set her stars and potential on the picture, or a card to click its limit-break diamonds; Ctrl- and Shift-click to change several at once; "Add all" per card group; keys for everything; Ctrl+Z to undo; and "Paste a list" for typing or pasting what you own. Outfits show their name under the uma, cards are grouped by rarity and type, and setup ends with a summary and the next Champions Meeting.
- **Game knowledge from the community's guides:** notes on race mechanics, Team Trials scoring, the Champions Meeting format, careers, Grand Concert, sparks and rating (from GameTora's and uma.guide's guides and community sheets, each with its source), a page per Champions Meeting course with its layout and how much Stamina each style needs, and skill advice. Help's word list and Home's "Did you know?" now come from these notes and say where each fact comes from. The community skill tier list shows on the Skills page and next to the planner's "Not simulated" skills.
- The planner's friend-card picker links to uma.moe and ChronoGenesis, to find someone lending the card.
- **Settings** is now a proper window (⚙ or Ctrl+,) with five sections: Profile (rename the account, save, load, start over), Appearance (mode, themes with live previews, Seiun Sky, size and spacing), Data and updates (how old the game data and meta sheet are, and how to refresh them), AI assistant (the Ask tab's backend, model and key, moved out of the Ask tab), and About. Help has its own ? button.
- **Two new themes:** Track day (trophy gold with a racetrack's rails) and Ink (high contrast black and white).
- **Size and spacing:** make everything 90–125% as big, and switch to compact spacing.
- **System mode:** the app can follow Windows' dark or light setting.
- **Keyboard shortcuts:** Ctrl+1 to Ctrl+4 switch tabs, Ctrl+F jumps to the search box, Ctrl+S saves the plan, F5 reloads, Esc closes things, and ? opens Help. Help lists them.
- Messages like "Saved" now pop up at the bottom of the window and fade away, instead of pushing the page around. A saved profile's message has a "Show" button that opens its folder.
- `tools/screenshots.ps1` takes screenshots of every page in each theme and window size, for checking the look.

### Changed

- Grand Concert's skill-point estimate follows uma.guide's guide: 25 SP per song and 5 per technique at each concert, about 20 songs and 50 techniques a career.
- Debuffs and skills that depend on where other umas are (which the simulator always counts as working out) are no longer auto-picked; they're listed under "Not simulated" for you to pin. Skills from parents' white sparks are listed but bought only when pinned, since a spark may never pass on.
- The planner's skill simulation now races with wit checks and rushing on, so single gains are realistic rather than an upper bound.
- The Notes tab is gone from the top bar; open the notes and data files from ⚙ → Advanced → Browse notes and data files. Help links to it too.
- **Chibi Seiun Sky no longer covers your data.** By default she now stands beside Home's welcome and on empty pages; in ⚙ you can also show her beside every page title, or hide her. The background switch now only turns the sky and decorations on and off.
- Light mode is easier to read: green, orange, gold, pink, grade letters and card-type names are darker on white, and every status colour passes the WCAG AA contrast check (a test keeps it that way).
- **Small windows work better (960×600):** the Collection filters fold away while a table is shown, the planner's saved plans fold into a slim rail with a menu, the Ask and Results side panels narrow, and the status bar shortens its labels (the full text is on hover).
- Course drawings number corners the way skills count them: the last corner is 4 (Chukyo 1200's two corners are 3 and 4, not 1 and 2).
- Small buttons are at least 24 px square, so they're easier to hit, and every icon-only button says what it does on hover.
- Settings → Data and updates shows the skill tier list's date, and About credits the tier list and the guides behind the glossary.
- The planner's Upcoming CM row explains both marks: ✔ announced, ? estimated.

### Fixed

- The next Champions Meeting's date was Umalator's estimate (Capricorn Cup on Oct 25); it's now the official Round 1 (Oct 31), and plans saved with the old date still find it.
- The Claude API key badge said a key was set even when none was.
- Save my data no longer opens Explorer by itself; its message has a Show button instead.
- Showing a profile with `UMALORD_PROFILE` (for screenshots) no longer overwrites the Ask tab's roster digest with that profile's roster.
- Umas and cards Global launched with no longer say "Released 2017-01-01" (uma.guide's JP date); they show Global's launch day.
- Unticked checkboxes were nearly invisible in dark mode; they have an outline now.
- In the evening (anywhere west of UTC) a new result's date, a new plan's date and the roster's "Last updated" said tomorrow; they use your computer's date now.
- The Results form ran off the right edge on a narrow window, and long plan and file names wrapped onto two lines; they now fit or shorten with "…".

## [0.1.1] - 2026-10-18

### Added

- Every push is built and tested by GitHub Actions, and releases are built from the tagged source, so the download always matches the code.
- A README with screenshots, this changelog, and forms for bug reports and ideas.

### Changed

- A new install starts with an empty account: the app writes the blank account files on first run, and the release zip never contains anyone's data.

### Fixed

- Race sim: the "Quick stamina estimate" button showed a box instead of its icon.
- Card picks: the background ranking could look busy for a moment after it had ranked every slot.

## [0.1.0] - 2026-10-18

First public release.

- **Collection:** your umas and support cards, with effects at your limit break and level, the skills they teach, and their events.
- **Career → Plan:** goals, trainees, decks and skills to buy. Picks start from the community PvP meta sheet for the next Champions Meeting, then simulated careers rank the rest; Auto-pick buys skills within your simulated SP budget.
- **Career → Race sim:** a Rust port of Moomoolator's race engine. Compare two builds, chart skills, check stamina.
- **Career → Results**, **Notes**, and **Ask** (questions about your roster in plain English).
- **Setup:** load a profile file, or click what you own. ⚙ → Save my data, Load a profile, Start over.
- **Themes:** Seiun Sky, Sakura, Sunset and Starry night, in dark or light.

[0.2.0]: https://github.com/brickc4/umaLord/releases/tag/v0.2.0
[0.1.1]: https://github.com/brickc4/umaLord/releases/tag/v0.1.1
[0.1.0]: https://github.com/brickc4/umaLord/releases/tag/v0.1.0
