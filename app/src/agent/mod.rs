//! The assistant's hands: tools that read the account, draft the working plan and the Race sim's
//! setup (every draft can be undone), run simulations in the background, move the app's pages,
//! and write files once the user approves.
//!
//! Both Ask backends use this one registry: the Claude API calls it from `ask.rs`, and OpenCode
//! reaches it over MCP (`mcp.rs`, `umalord.exe --mcp`). `exec.rs` holds the handlers; `bridge.rs`
//! the files the MCP process and the window share (approvals, page requests, the draft).

pub(crate) mod bridge;
pub(crate) mod exec;
pub(crate) mod mcp;

use std::sync::OnceLock;

use serde_json::{Value, json};

use crate::tab_planner::Plan;
use crate::tab_sim::ExtraKind;

/// What a tool may do, which decides how it runs and whether the user is asked first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    /// Looks things up; changes nothing.
    Read,
    /// Changes the working plan or the Race sim's setup; undoable from the chat.
    Draft,
    /// A simulation that runs in the background; the turn waits for it.
    Long,
    /// Moves the app to a page or selection.
    Ui,
    /// Writes a file: asks the user first unless Settings → AI assistant allows it.
    Write,
}

pub(crate) struct ToolSpec {
    pub name: &'static str,
    pub kind: Kind,
    pub icon: &'static str,
    /// What the chat card says ran, e.g. "Recommended a deck".
    pub label: &'static str,
    pub description: &'static str,
    /// The input's JSON schema (an object schema).
    pub schema: Value,
}

/// A page the chat can open, from a card's button or `open_page`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OpenAt {
    Home,
    Plan,
    Race,
    Results(usize),
    Uma(u32),
    Card(u32),
    Skill(u32),
    Umas,
    Cards,
    Skills,
    /// The OpenCode assistant's draft plan (`bridge::Draft`), loaded into the Planner.
    DraftPlan,
    /// Its Race sim setup, loaded and run again with the same seed.
    DraftRace,
}

impl OpenAt {
    /// "the Race sim", for "It opens the Race sim when this answer finishes."
    pub fn page(&self) -> &'static str {
        match self {
            OpenAt::Home => "Home",
            OpenAt::Plan | OpenAt::DraftPlan => "the Planner",
            OpenAt::Race | OpenAt::DraftRace => "the Race sim",
            OpenAt::Results(_) => "the Results page",
            OpenAt::Uma(_) => "the uma in Collection",
            OpenAt::Card(_) => "the card in Collection",
            OpenAt::Skill(_) => "the skill in Collection",
            OpenAt::Umas => "Collection → Umas",
            OpenAt::Cards => "Collection → Cards",
            OpenAt::Skills => "Collection → Skills",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            OpenAt::Home => "Open Home",
            OpenAt::Plan | OpenAt::DraftPlan => "Open in Planner",
            OpenAt::Race | OpenAt::DraftRace => "Open in Race sim",
            OpenAt::Results(_) => "Open Results",
            OpenAt::Uma(_) | OpenAt::Umas => "Open in Umas",
            OpenAt::Card(_) | OpenAt::Cards => "Open in Cards",
            OpenAt::Skill(_) | OpenAt::Skills => "Open in Skills",
        }
    }
}

/// A small picture of a tool's result in the chat.
#[derive(Clone, Debug, Default)]
pub(crate) enum Visual {
    #[default]
    None,
    /// Five own cards and the friend card.
    Deck { cards: Vec<u32>, friend: Option<u32> },
    /// Who won, and every race's gap for a tiny histogram.
    Verdict { line: String, gaps: Vec<f64> },
    Skills(Vec<u32>),
    /// Labelled values in lengths, best first.
    Bars(Vec<(String, f64)>),
}

/// What the chat shows for a finished tool call.
#[derive(Clone, Debug, Default)]
pub(crate) struct Card {
    pub summary: String,
    pub visual: Visual,
    pub open: Vec<OpenAt>,
    /// The undo snapshot this call took (`App::agent_undo`).
    pub undo: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ToolOut {
    /// What the model reads.
    pub text: String,
    pub is_error: bool,
    pub card: Card,
}

impl ToolOut {
    pub fn ok(text: impl Into<String>, summary: impl Into<String>) -> Self {
        ToolOut { text: text.into(), is_error: false, card: Card { summary: summary.into(), ..Default::default() } }
    }
    pub fn err(text: impl Into<String>) -> Self {
        let text = text.into();
        let summary = text.lines().next().unwrap_or_default().to_string();
        ToolOut { text, is_error: true, card: Card { summary, ..Default::default() } }
    }
    pub fn visual(mut self, v: Visual) -> Self {
        self.card.visual = v;
        self
    }
    pub fn open(mut self, at: OpenAt) -> Self {
        self.card.open.push(at);
        self
    }
    pub fn undo(mut self, idx: Option<usize>) -> Self {
        self.card.undo = idx;
        self
    }
}

/// A background job a Long tool is waiting on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Wait {
    /// The auto-pick optimiser (`planner.pick_job`).
    Pick { undo: Option<usize> },
    /// The planner's skill simulation (`planner.skill_job`).
    SkillValues { top_n: usize },
    /// The Race sim's run (`sim.job`).
    Compare { undo: Option<usize> },
    Chart { top_n: usize, undo: Option<usize> },
    Stamina { undo: Option<usize> },
    Extra(ExtraKind),
}

/// A Write tool waiting for the user.
#[derive(Clone, Debug)]
pub(crate) struct Approval {
    pub question: String,
    /// Asks even when Settings allows the tool without asking (roster changes).
    pub always_ask: bool,
}

pub(crate) enum Exec {
    Done(ToolOut),
    Wait(Wait),
    Ask(Approval),
}

/// The Race sim's setup, as an undo snapshot or a draft file holds it.
#[derive(Clone)]
pub(crate) struct SimSetup {
    pub course: u32,
    pub ground: i32,
    pub weather: i32,
    pub season: i32,
    pub time: i32,
    pub umas: [crate::sim::run::UmaBuild; 2],
    pub opts: crate::sim::run::SimOptions,
    pub from_plan: bool,
}

/// What a Draft tool changed, to put back.
#[derive(Clone)]
pub(crate) struct Snapshot {
    pub label: String,
    pub plan: Plan,
    pub open_file: Option<std::path::PathBuf>,
    pub sim: SimSetup,
}

#[derive(Default)]
pub(crate) struct AgentState {
    pub undo: Vec<Snapshot>,
    /// A page `open_page` asked for, opened when the answer finishes (so the chat stays in view).
    pub open_after: Option<OpenAt>,
    /// In the MCP server: the folder the window watches (`UMALORD_AGENT_DIR`), for approvals,
    /// page requests and the draft. None when no window is attached (Claude Code, Claude Desktop).
    pub bridge: Option<std::path::PathBuf>,
    /// Simulations run since the user's last question (the prompt allows four).
    pub sims: u32,
    /// This app is the MCP server's (UI tools go through the bridge, or do nothing).
    pub mcp: bool,
}

// ---------------------------------------------------------------------------
// The registry

fn obj(properties: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false})
}

fn race_arg() -> Value {
    json!({
        "type": "object",
        "description": "Optional race setup for the Race sim. Give a Champions Meeting, or a course and conditions. Left out: the sim keeps its current setup.",
        "properties": {
            "cm": {"type": "string", "description": "A Champions Meeting: cup name ('Capricorn Cup'), number ('22') or 'next'. Sets its course and conditions and drops the skills it turns off."},
            "course": {"type": "string", "description": "A Umalator course id, a course name like 'Nakayama Turf 2500m', or a race name like 'Arima Kinen'."},
            "ground": {"type": "string", "enum": ["Firm", "Good", "Soft", "Heavy"]},
            "weather": {"type": "string", "enum": ["Sunny", "Cloudy", "Rainy", "Snowy"]},
            "season": {"type": "string", "enum": ["Spring", "Summer", "Autumn", "Winter"]},
            "time": {"type": "string", "enum": ["Midday", "Evening", "Night"]}
        },
        "additionalProperties": false
    })
}

fn stats_arg(what: &str) -> Value {
    json!({"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 2000}, "minItems": 5, "maxItems": 5, "description": format!("{what}: Speed, Stamina, Power, Guts, Wit")})
}

fn spec(name: &'static str, kind: Kind, icon: &'static str, label: &'static str, description: &'static str, schema: Value) -> ToolSpec {
    ToolSpec { name, kind, icon, label, description, schema }
}

fn build_registry() -> Vec<ToolSpec> {
    use Kind::*;
    let style = json!({"type": "string", "enum": ["Front", "Pace", "Late", "End"], "description": "Running style: Front Runner, Pace Chaser, Late Surger, End Closer"});
    let distance = json!({"type": "string", "enum": ["Sprint", "Mile", "Medium", "Long"]});
    let surface = json!({"type": "string", "enum": ["Turf", "Dirt"]});
    let target = json!({"distance": distance, "surface": surface, "style": style});
    let mut find_umas = target.clone();
    find_umas["min_grade"] = json!({"type": "string", "enum": ["S", "A", "B", "C"], "description": "Minimum aptitude grade for the given filters (default A)"});
    let mut search = target;
    search["query"] = json!({"type": "string", "description": "Text to match in name or effect summary; may be empty"});
    search["owned_only"] = json!({"type": "boolean", "description": "Only skills this account can get now (default true)"});
    search["category"] = json!({"type": "string", "enum": ["Speed Boost", "Acceleration", "Recovery", "Passive", "Debuff", "Lane Effect", "Vision", "Unique", "Evolved"]});
    let query = || obj(json!({"query": {"type": "string"}}), &["query"]);
    let skills = || json!({"type": "array", "items": {"type": "string"}, "description": "Skill names or ids"});
    let purposes = json!({"type": "string", "enum": ["Champions Meeting", "Team Trials", "Parent run", "League of Heroes", "Other"]});
    let wins = json!({"type": "array", "items": {"type": "string"}, "description": "The G1 races it won, by their exact names in the game data (find_races finds them, e.g. 'Tokyo Yushun (Japanese Derby)'); leave out for none"});
    let sparks = json!({"type": "array", "items": {"type": "string"}, "description": "For a hypothetical uma (a parent run not done yet): its sparks as the game names them, like \"Dirt 2★\", \"Speed 3★\" or a unique's name with its stars"});
    let ancestor = obj(json!({"uma": {"type": "string"}, "g1_wins": wins, "sparks": sparks}), &["uma"]);
    let legacy = obj(
        json!({"finished": {"type": "integer", "description": "One of the account's finished umas by its #id (list_finished_umas): its sparks, G1 wins and parents come from the import; leave uma, g1_wins, sparks and parents out"}, "uma": {"type": "string"}, "g1_wins": wins, "sparks": sparks, "parents": {"type": "array", "maxItems": 2, "items": ancestor}}),
        &[],
    );
    vec![
        // --- Read
        spec("get_account_summary", Read, "👤", "Read the account", "The account at a glance: profile, server, umas by stars and potential, cards by type and limit break, maxed SSRs, saved parents and plans, and the working plan in one line.", obj(json!({}), &[])),
        spec("find_umas", Read, "🔎", "Searched the umas", "List the account's owned umas whose aptitudes fit a distance, surface and/or style, best fit first, with stars, potential and aptitude grades.", obj(find_umas, &[])),
        spec("get_uma", Read, "🐴", "Read a trainee", "Full knowledge for one trainee outfit: base stats, growth rates, aptitudes, unique/innate/potential skills (with which are locked at the owned potential), evolved skills and their conditions, event skills, career objectives, and whether the account owns it. Query by name, outfit or id.", query()),
        spec("get_card", Read, "🃏", "Read a card", "Full knowledge for one support card: support effects at the owned level and at max level, unique effect, skill hints, event skills, and every event with its outcomes. Query by uma name, card title or id.", query()),
        spec("search_skills", Read, "🔎", "Searched skills", "Search skills. Returns name, rarity, SP cost, effect, what it is restricted to, uma.guide's trigger summary, and where this account can get it.", obj(search, &[])),
        spec("get_skill", Read, "✨", "Read a skill", "Full detail for one skill: effects, trigger conditions, cooldown, and every source (owned or not).", query()),
        spec("find_races", Read, "🏟", "Searched races", "Find races/courses by race name or racecourse: track, surface, distance, direction, grade.", query()),
        spec("list_upcoming_events", Read, "🏆", "Read the CM schedule", "The next Champions Meetings from the app's live schedule (official news first, then Umalator's estimates): cup, number, date, announced or estimated, course, conditions, the training scenario, the skills it turns off, and whether the meta sheet covers it.", obj(json!({"count": {"type": "integer", "minimum": 1, "maximum": 8, "description": "How many (default 4)"}}), &[])),
        spec("get_cm_meta", Read, "📈", "Read the meta sheet", "The community PvP meta sheet for a Champions Meeting: style ranking, recommended stats, recommended trainees (owned marked), decks by tier with which cards the account owns and at what LB, honorable mentions, skill advice and parents to look for. Community opinion, not game data.", obj(json!({"cm": {"type": "string", "description": "Cup name, number or 'next' (default: the working plan's CM, else the next one)"}, "style": style}), &[])),
        spec("list_plans", Read, "📋", "Listed saved plans", "The saved career plans in plans/, newest first, with their titles.", obj(json!({}), &[])),
        spec("get_plan", Read, "📋", "Read a plan", "The working plan in the Planner ('current', the default): goal, CM, scenario, trainee and aptitudes, target stats, deck, skills to buy, SP budget, pins and bans. Or a saved plan's file by name.", obj(json!({"name": {"type": "string", "description": "'current' or part of a saved plan's file name or title"}}), &[])),
        spec("get_slot_ranking", Read, "🏅", "Ranked cards for a slot", "Rank the cards for one deck slot of the working plan, given the rest of the deck: the meta sheet's backing (★), the simulated score in stat points, and why. Slot 1–5 are your own cards, 6 the borrowed friend card. Model output: suggestions, not game data.", obj(json!({"slot": {"type": "integer", "minimum": 1, "maximum": 6}, "top_n": {"type": "integer", "minimum": 1, "maximum": 15}}), &["slot"])),
        spec("explain_card", Read, "🃏", "Explained a card", "Why a card is or isn't a good pick for the working plan: its rank and score for the slot it would fill, the meta sheet's backing, and the reasons.", query()),
        spec("recommend_trainees", Read, "🐴", "Suggested trainees", "The owned trainees that fit the working plan best: the meta sheet's picks you own first (★), then the best aptitude fits, with stars, potential and aptitudes for the goal.", obj(json!({"top_n": {"type": "integer", "minimum": 1, "maximum": 12}}), &[])),
        spec(
            "get_affinity",
            Read,
            "🌱",
            "Worked out affinity",
            "Affinity between a trainee and one or two legacies (parents), each with up to two of its own parents (the trainee's grandparents) and the G1 races each won, from the game's own relation tables. Gives each slot's own affinity, which its sparks roll with (chance = base × (1 + affinity/100)), its parts (relation groups shared with the trainee, the trios through each grandparent, the two legacies' link, +3 per G1 both members of a link won), and the lineage total with the game's symbol (◎ over 150, ○ 51–150, △ 50 or less; the symbol is for the lineage, not one slot). Any outfit of a character gives the same affinity. Wins left out count as none.",
            obj(json!({"trainee": {"type": "string", "description": "The uma to train: name, outfit or id"}, "legacies": {"type": "array", "minItems": 1, "maxItems": 2, "items": legacy.clone()}}), &["trainee", "legacies"]),
        ),
        spec(
            "parent_run_odds",
            Read,
            "🌱",
            "Worked out a parent run's odds",
            "What one parent career of an owned uma makes, with the odds and expected careers: her pink spark (picked evenly among the aptitudes she finishes at A or S: give `legacies` to count the ones her lineage raises at the start), her blue (stat 1 in 5, stars by its final value: give `final_stats`), her green if she's 3★+ (stars by rank: `rank_ss`, default true), and how white sparks are made. Each with the careers to expect, plain and with the 30 TP reroll. Use it for every run in a parent-run queue.",
            obj(
                json!({"uma": {"type": "string", "description": "The owned uma to run as the parent"}, "legacies": {"type": "array", "maxItems": 2, "items": legacy.clone()}, "final_stats": stats_arg("Her expected final stats"), "rank_ss": {"type": "boolean"}}),
                &["uma"],
            ),
        ),
        spec(
            "plan_lineage",
            Read,
            "🌱",
            "Worked out a lineage",
            "What a lineage gives a trainee, computed exactly: at the start (fixed) each aptitude after the six umas' pink stars (1–3 stars +1 grade, 4–6 +2, 7–9 +3, 10+ +4, never past A), the stats their blue sparks add, and the legacies' inherited uniques; then each slot's chance of firing each pink and green spark over the two April inspirations, and its expected white hints. Legacies are the account's finished umas ({\"finished\": id}) or hypothetical ones with `sparks` (to test a parent run before doing it). `target` marks the aptitudes the goal needs and says how many pink stars reach A. Use it instead of doing spark arithmetic.",
            obj(
                json!({"trainee": {"type": "string"}, "legacies": {"type": "array", "minItems": 1, "maxItems": 2, "items": legacy.clone()}, "target": {"type": "object", "properties": {"surface": surface.clone(), "distance": distance.clone(), "style": style.clone()}, "additionalProperties": false}}),
                &["trainee", "legacies"],
            ),
        ),
        spec(
            "rank_legacies",
            Read,
            "🌱",
            "Ranked legacy partners",
            "The characters with the most base affinity to a trainee, from the game's relation tables, with the account's own umas marked (stars: 3★ or more passes on her unique as a green spark). With `with`, ranks a second legacy by its affinity to both the trainee and that first legacy. Base affinity only: grandparents and shared G1 wins add more (get_affinity). With `finished: true`, ranks the account's own finished umas instead, by their whole affinity as a legacy (their parents and G1 wins included), with their sparks.",
            obj(json!({"trainee": {"type": "string"}, "with": {"type": "string", "description": "A legacy already chosen"}, "owned_only": {"type": "boolean"}, "finished": {"type": "boolean"}, "top_n": {"type": "integer", "minimum": 1, "maximum": 30}}), &["trainee"]),
        ),
        spec(
            "list_finished_umas",
            Read,
            "🌱",
            "Listed finished umas",
            "The account's finished umas (veterans: the parents it can use), imported from UmaExtractor's data.json: #id, uma, rank, sparks by colour with stars (blue = stat, pink = aptitude, green = unique, white = skills, races and scenario), G1 wins and their own parents. Best rank first. Filter by uma name, by a spark's name (e.g. 'Turf', 'Speed', a skill), its colour and its minimum stars.",
            obj(
                json!({"uma": {"type": "string"}, "spark": {"type": "string"}, "kind": {"type": "string", "enum": ["blue", "pink", "green", "white", "race", "scenario"]}, "min_stars": {"type": "integer", "minimum": 1, "maximum": 3}, "top_n": {"type": "integer", "minimum": 1, "maximum": 60}}),
                &[],
            ),
        ),
        spec("list_notes", Read, "📝", "Listed notes", "List the project's markdown notes: insights.md (Claude's evidence-backed opinions), sources.md, plans/*.md (career plans), results/*.md (real results logs), account/*.md.", obj(json!({}), &[])),
        spec("read_note", Read, "📝", "Read a note", "Read one note file by its path from list_notes, e.g. insights.md, results/careers.md, or the sourced knowledge notes: knowledge/mechanics.md (race mechanics, Team Trials scoring, the CM format, sparks, Grand Concert), knowledge/racecourses.md (every CM course's layout and stamina by style) and knowledge/skills.md (skill advice, debuffs, post-race hints).", obj(json!({"path": {"type": "string"}}), &["path"])),
        spec("glossary", Read, "📖", "Looked up a term", "Look up a community term (e.g. 'MLB', 'spark', 'CM').", obj(json!({"term": {"type": "string"}}), &["term"])),
        // --- Draft
        spec("new_plan", Draft, "📋", "Started a plan", "Start a new working plan in the Planner, replacing the one open there (undoable). For a Champions Meeting this sets its course, conditions, scenario and disabled skills.", obj(json!({
            "goal": purposes, "cm": {"type": "string", "description": "Cup name, number or 'next' (Champions Meeting goal; default 'next')"},
            "style": style, "trainee": {"type": "string", "description": "An owned uma's name and outfit, or id"},
            "scenario": {"type": "string"}, "title": {"type": "string"}
        }), &[])),
        spec("set_plan", Draft, "✏", "Changed the plan", "Change fields of the working plan (undoable). Give only what changes.", obj(json!({
            "trainee": {"type": "string", "description": "An owned uma's name and outfit, or id"},
            "style": style, "distance": distance, "surface": surface,
            "course": {"type": "string", "description": "A race name ('Arima Kinen') or race id; 'none' clears it"},
            "cm": {"type": "string", "description": "Link a Champions Meeting (cup name, number or 'next'); 'none' unlinks it"},
            "ground": {"type": "string", "enum": ["Firm", "Good", "Soft", "Heavy"]},
            "weather": {"type": "string", "enum": ["Sunny", "Cloudy", "Rainy", "Snowy"]},
            "season": {"type": "string", "enum": ["Spring", "Summer", "Autumn", "Winter"]},
            "target_stats": stats_arg("Target stats"),
            "parents": {"type": "array", "items": {"type": "string"}, "maxItems": 2, "description": "Parent names as account/parents.md has them"},
            "notes": {"type": "string"}, "title": {"type": "string"}, "scenario": {"type": "string"},
            "sp_budget": {"type": "integer", "minimum": 0, "maximum": 6000, "description": "SP to spend; 0 goes back to the career simulation's estimate"},
            "meta_influence": {"type": "string", "enum": ["off", "tie-break", "strong"], "description": "How much the meta sheet's skill advice counts in auto-pick"},
            "pin_skills": skills(), "ban_skills": skills(),
            "friend_lb": {"type": "integer", "minimum": 0, "maximum": 4},
            "fast_learner": {"type": "boolean"}
        }), &[])),
        spec("recommend_deck", Draft, "🃏", "Recommended a deck", "Fill the working plan's deck with the best cards for its goal (undoable): the meta sheet's picks first (★), then simulated careers. Model output: suggestions.", obj(json!({"replace_all": {"type": "boolean", "description": "Replace every slot (default true), or only fill empty ones"}}), &[])),
        spec("set_deck", Draft, "🃏", "Set the deck", "Set the working plan's deck (undoable): up to five owned cards, one per character and not the trainee's own, plus the borrowed friend card.", obj(json!({
            "cards": {"type": "array", "items": {"type": "string"}, "maxItems": 5, "description": "Owned cards by uma name and title, or id"},
            "friend": {"type": "string", "description": "The borrowed card; 'none' leaves the slot empty"},
            "friend_lb": {"type": "integer", "minimum": 0, "maximum": 4}
        }), &["cards"])),
        spec("set_skills", Draft, "✨", "Set the skills", "Set the working plan's skills to buy (undoable).", obj(json!({"skills": skills(), "mode": {"type": "string", "enum": ["replace", "add", "remove"]}}), &["skills"])),
        spec("load_plan_into_sim", Draft, "🏁", "Loaded the plan into the Race sim", "Put the working plan into the Race sim (undoable): its course and conditions, the trainee without her skills to buy as Uma 1 and with them as Uma 2.", obj(json!({}), &[])),
        spec("set_sim_build", Draft, "🏁", "Changed a Race sim build", "Change Uma 1 or Uma 2 in the Race sim (undoable). Give only what changes.", obj(json!({
            "uma": {"type": "integer", "enum": [1, 2]},
            "from_plan": {"type": "string", "enum": ["with_skills", "without_skills"], "description": "Start from the working plan's trainee"},
            "outfit": {"type": "string", "description": "Any uma outfit by name or id (owned or not)"},
            "stats": stats_arg("Stats"),
            "style": style,
            "mood": {"type": "string", "enum": ["Awful", "Bad", "Normal", "Good", "Great"]},
            "skills": {"type": "array", "items": {"type": "string"}, "description": "Replace her skills (the unique stays)"},
            "add_skills": skills(), "remove_skills": skills()
        }), &["uma"])),
        // --- Long
        spec("auto_pick_skills", Long, "✨", "Auto-picked skills", "Choose the working plan's skills to buy: a quick pick, then the optimiser races sets of skills within the SP budget and verifies the result over 1000 races (about 20 s). Needs a trainee, target stats and a course or CM. Simulation output.", obj(json!({"objective": {"type": "string", "enum": ["lengths", "consistency", "rating"], "description": "Most lengths on average (default); Team Trials consistency; or rating: the most skill evaluation points (for a parent's rank), no racing"}}), &[])),
        spec("simulate_skills", Long, "📊", "Simulated the skills on offer", "Race every skill the trainee, the owned cards and the friend card can teach on the plan's course (the skill chart), so the deck ranking and auto-pick use simulated gains. Returns the best. Simulation output.", obj(json!({"top_n": {"type": "integer", "minimum": 1, "maximum": 30}}), &[])),
        spec("run_compare", Long, "🏁", "Raced two builds", "Race Uma 1 against Uma 2 many times in the Race sim (Moomoolator's engine) and report who wins, by how much, how often, full-spurt, HP-survival and rushed rates, and Uma 2's skill activations. builds 'plan' (default) loads the working plan first: the trainee without vs with her skills to buy. Simulation output.", obj(json!({
            "builds": {"type": "string", "enum": ["plan", "current"], "description": "'plan' loads the working plan; 'current' races the builds as they are"},
            "runs": {"type": "integer", "minimum": 100, "maximum": 2000, "description": "Races (default 1000)"},
            "seed": {"type": "integer", "minimum": 1, "maximum": 2147483646, "description": "Default 12345, so results can be reproduced"},
            "race": race_arg()
        }), &[])),
        spec("run_skill_chart", Long, "📊", "Charted skills", "How many lengths each skill adds to Uma 1 on the sim's course (Moomoolator's skill chart). Put the build you want in Uma 1 first. Simulation output.", obj(json!({
            "pool": {"type": "string", "enum": ["account", "all"], "description": "Skills this account can get (default), or every Global skill"},
            "top_n": {"type": "integer", "minimum": 1, "maximum": 30}, "race": race_arg()
        }), &[])),
        spec("run_stamina", Long, "❤", "Checked stamina", "Whether Uma 1 has the HP for a full spurt on the sim's course: HP left at the finish across races, the full-spurt rate, and the closed-form estimate of the Stamina needed. Simulation output.", obj(json!({"runs": {"type": "integer", "minimum": 100, "maximum": 2000}, "race": race_arg()}), &[])),
        spec("stat_sensitivity", Long, "📈", "Tested what matters", "On the last run_compare: 'stats' races Uma 2 with +50 in each stat (where her next training points matter), 'skills' races her without each skill in turn (what each is worth). Simulation output.", obj(json!({"what": {"type": "string", "enum": ["stats", "skills"]}}), &[])),
        spec("career_sim", Long, "🎓", "Simulated the career", "The working plan's career simulation (400 careers in its scenario with its deck): expected skill points with the 10–90% range and sources, and final stats. Model output built on knowledge/scenarios.json's assumptions.", obj(json!({}), &[])),
        // --- UI
        spec("open_page", Ui, "📌", "Opened a page", "Open a page of the app when this answer finishes, so the user sees what you did.", obj(json!({"page": {"type": "string", "enum": ["home", "umas", "cards", "skills", "plan", "race", "results"]}}), &["page"])),
        spec("select", Ui, "📌", "Selected", "Show one uma, card or skill in the Collection when this answer finishes, or open a saved plan as the working plan now (undoable).", obj(json!({"kind": {"type": "string", "enum": ["uma", "card", "skill", "plan"]}, "query": {"type": "string"}}), &["kind", "query"])),
        // --- Write
        spec("save_plan", Write, "💾", "Saved the plan", "Save the working plan to plans/ (asks the user first). With with_simulation (default true) the last run_compare of the plan goes into its ## Simulation section.", obj(json!({"title": {"type": "string"}, "with_simulation": {"type": "boolean"}}), &[])),
        spec("log_result", Write, "🏆", "Logged a result", "Add a real result to a results log, newest first (asks the user first). fields maps the log's column names to values; read the log with read_note to see its columns.", obj(json!({
            "log": {"type": "string", "enum": ["careers", "team_trials", "champions_meeting", "league_of_heroes", "sparks"]},
            "fields": {"type": "object", "additionalProperties": {"type": "string"}}
        }), &["log", "fields"])),
        spec("update_owned", Write, "✏", "Changed the roster", "Change the account's roster (always asks the user): a card's limit break and level, an uma's stars and potential, add one, or remove one.", obj(json!({
            "kind": {"type": "string", "enum": ["uma", "card"]}, "query": {"type": "string"},
            "lb": {"type": "integer", "minimum": 0, "maximum": 4}, "level": {"type": "integer", "minimum": 1, "maximum": 50},
            "stars": {"type": "integer", "minimum": 1, "maximum": 5}, "potential": {"type": "integer", "minimum": 1, "maximum": 5},
            "remove": {"type": "boolean"}
        }), &["kind", "query"])),
    ]
}

/// Every tool, in the order the model sees them.
pub(crate) fn registry() -> &'static [ToolSpec] {
    static REG: OnceLock<Vec<ToolSpec>> = OnceLock::new();
    REG.get_or_init(build_registry)
}

pub(crate) fn find(name: &str) -> Option<&'static ToolSpec> {
    // OpenCode prefixes MCP tools with the server's name.
    let name = name.strip_prefix("umalord_").unwrap_or(name);
    registry().iter().find(|s| s.name == name)
}

/// The tools in the Claude API's format. Streaming requests set `eager_input_streaming` (the
/// input is then validated here, `validate`); the last tool carries a cache breakpoint.
pub(crate) fn api_tools(eager: bool) -> Value {
    let mut tools: Vec<Value> = registry()
        .iter()
        .map(|s| {
            let mut t = json!({"name": s.name, "description": s.description, "input_schema": s.schema});
            if eager {
                t["eager_input_streaming"] = json!(true);
            }
            t
        })
        .collect();
    if let Some(last) = tools.last_mut() {
        last["cache_control"] = json!({"type": "ephemeral"});
    }
    Value::Array(tools)
}

/// The tools in MCP's `tools/list` format.
pub(crate) fn mcp_tools() -> Value {
    let tools: Vec<Value> = registry()
        .iter()
        .map(|s| {
            let read_only = matches!(s.kind, Kind::Read);
            json!({
                "name": s.name,
                "description": s.description,
                "inputSchema": s.schema,
                "annotations": {"title": s.label, "readOnlyHint": read_only, "destructiveHint": s.kind == Kind::Write, "openWorldHint": false},
            })
        })
        .collect();
    Value::Array(tools)
}

/// Check a tool's input against its schema: the JSON Schema subset the registry uses (object
/// properties, required, additionalProperties, types, enums, array items and lengths, integer
/// bounds). Streamed input isn't validated by the API, so every call goes through this first.
pub(crate) fn validate(schema: &Value, v: &Value, at: &str) -> Result<(), String> {
    let here = if at.is_empty() { "input".to_string() } else { at.to_string() };
    if let Some(t) = schema["type"].as_str() {
        let ok = match t {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "integer" => v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|f| f.fract() == 0.0),
            "number" => v.is_number(),
            "boolean" => v.is_boolean(),
            _ => true,
        };
        if !ok {
            return Err(format!("{here} should be {t}, got {v}"));
        }
    }
    if let Some(options) = schema["enum"].as_array()
        && !options.contains(v)
    {
        return Err(format!("{here} must be one of {}", Value::Array(options.clone())));
    }
    if let Some(n) = v.as_f64() {
        if let Some(min) = schema["minimum"].as_f64()
            && n < min
        {
            return Err(format!("{here} must be at least {min}"));
        }
        if let Some(max) = schema["maximum"].as_f64()
            && n > max
        {
            return Err(format!("{here} must be at most {max}"));
        }
    }
    if let Some(items) = v.as_array() {
        if let Some(min) = schema["minItems"].as_u64()
            && (items.len() as u64) < min
        {
            return Err(format!("{here} needs at least {min} items"));
        }
        if let Some(max) = schema["maxItems"].as_u64()
            && items.len() as u64 > max
        {
            return Err(format!("{here} takes at most {max} items"));
        }
        if schema["items"].is_object() {
            for (i, item) in items.iter().enumerate() {
                validate(&schema["items"], item, &format!("{here}[{i}]"))?;
            }
        }
    }
    if let Some(map) = v.as_object() {
        let props = schema["properties"].as_object();
        for key in schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if map.get(key).is_none_or(Value::is_null) {
                return Err(format!("{here} is missing \"{key}\""));
            }
        }
        for (k, val) in map {
            // A null is a field left out.
            if val.is_null() {
                continue;
            }
            match props.and_then(|p| p.get(k)) {
                Some(sub) => validate(sub, val, &format!("{here}.{k}"))?,
                None if schema["additionalProperties"] == json!(false) => {
                    let known: Vec<&String> = props.map(|p| p.keys().collect()).unwrap_or_default();
                    return Err(format!("{here} has no field \"{k}\"; it takes {known:?}"));
                }
                None if schema["additionalProperties"].is_object() => validate(&schema["additionalProperties"], val, &format!("{here}.{k}"))?,
                None => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use eframe::egui;
    use serde_json::json;

    use super::*;

    /// Every tool runs headless against the test roster, Long tools by polling their jobs, Write
    /// tools after the approval they ask for. Writes land in a scratch account.
    #[test]
    fn every_tool_runs_headless() {
        let ctx = egui::Context::default();
        let (mut a, dir) = crate::onboarding::tests::scratch_app("agent-tools");
        // Two made-up finished umas for list_finished_umas, rank_legacies and get_affinity.
        std::fs::copy(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/veterans_sample.json"), dir.join(crate::veterans::FILE)).unwrap();
        a.roster = crate::data::Roster::sample();
        let headers = a.result_headers(0);
        let log_fields: serde_json::Map<String, Value> = headers.iter().filter(|h| !h.starts_with("Date")).take(2).map(|h| (h.clone(), json!("from the agent test"))).collect();
        let calls: Vec<(&str, Value)> = vec![
            ("get_account_summary", json!({})),
            ("find_umas", json!({"distance": "Long", "style": "Pace"})),
            ("get_uma", json!({"query": "Oguri Cap [Starlight Beat]"})),
            ("get_card", json!({"query": "Kitasan Black"})),
            ("search_skills", json!({"query": "corner"})),
            ("get_skill", json!({"query": "Homestretch Haste"})),
            ("find_races", json!({"query": "Arima"})),
            ("list_upcoming_events", json!({"count": 2})),
            ("get_cm_meta", json!({"cm": "21", "style": "Pace"})),
            ("list_notes", json!({})),
            ("read_note", json!({"path": "insights.md"})),
            ("glossary", json!({"term": "spark"})),
            ("new_plan", json!({"goal": "Champions Meeting", "cm": "next", "trainee": "Oguri Cap [Starlight Beat]"})),
            ("recommend_trainees", json!({"top_n": 3})),
            ("get_affinity", json!({"trainee": "Oguri Cap", "legacies": [{"uma": "Silence Suzuka", "g1_wins": ["Arima Kinen", "Japan Cup"], "parents": [{"uma": "Mejiro McQueen", "g1_wins": ["Arima Kinen"]}]}, {"uma": "Gold Ship"}]})),
            ("get_affinity", json!({"trainee": "Oguri Cap", "legacies": [{"finished": 7}, {"finished": 8}]})),
            ("rank_legacies", json!({"trainee": "Oguri Cap", "with": "Gold Ship", "top_n": 5})),
            ("rank_legacies", json!({"trainee": "Oguri Cap", "finished": true, "top_n": 3})),
            ("list_finished_umas", json!({"kind": "pink", "min_stars": 2})),
            ("parent_run_odds", json!({"uma": "Oguri Cap [Starlight Beat]", "legacies": [{"finished": 8}], "final_stats": [1200, 600, 900, 500, 700]})),
            ("plan_lineage", json!({"trainee": "Oguri Cap [Starlight Beat]", "legacies": [{"finished": 7}, {"uma": "Haru Urara", "sparks": ["Dirt 2 ★", "Speed ★★★", "Escapades 2★", "Victoria por plancha☆ 1★"], "parents": [{"uma": "Gold Ship", "sparks": ["Dirt 1★"]}]}], "target": {"surface": "Dirt", "distance": "Mile", "style": "End"}})),
            ("set_plan", json!({"style": "Pace", "target_stats": [1200, 1000, 1000, 600, 1000], "ground": "Good", "meta_influence": "tie-break"})),
            ("get_plan", json!({})),
            ("set_deck", json!({"cards": ["Kitasan Black"], "friend": "none"})),
            ("recommend_deck", json!({"replace_all": false})),
            ("get_slot_ranking", json!({"slot": 1, "top_n": 3})),
            ("explain_card", json!({"query": "Kitasan Black"})),
            ("career_sim", json!({})),
            ("simulate_skills", json!({"top_n": 5})),
            ("auto_pick_skills", json!({})),
            ("set_skills", json!({"skills": ["Homestretch Haste"], "mode": "add"})),
            ("load_plan_into_sim", json!({})),
            ("run_compare", json!({"runs": 200})),
            ("stat_sensitivity", json!({"what": "stats"})),
            ("set_sim_build", json!({"uma": 1, "from_plan": "without_skills", "mood": "Good"})),
            ("run_skill_chart", json!({"top_n": 5})),
            ("run_stamina", json!({"runs": 100})),
            ("open_page", json!({"page": "race"})),
            ("save_plan", json!({"title": "Agent tools test"})),
            ("list_plans", json!({})),
            ("select", json!({"kind": "plan", "query": "Agent tools test"})),
            ("select", json!({"kind": "card", "query": "Kitasan Black"})),
            ("log_result", json!({"log": "careers", "fields": log_fields})),
            ("update_owned", json!({"kind": "card", "query": "Kitasan Black", "lb": 3})),
        ];
        let mut seen = BTreeSet::new();
        for (name, input) in calls {
            seen.insert(name);
            let out = match a.agent_call_blocking(&ctx, name, &input, std::time::Duration::from_secs(180)) {
                Exec::Done(out) => out,
                Exec::Ask(q) => {
                    assert_eq!(find(name).unwrap().kind, Kind::Write, "{name} asked for approval");
                    assert!(!q.question.is_empty());
                    a.agent_write(name, &input)
                }
                Exec::Wait(_) => panic!("{name} still running"),
            };
            assert!(!out.is_error, "{name}: {}", out.text);
            assert!(!out.text.trim().is_empty() && !out.card.summary.is_empty(), "{name} said nothing");
            eprintln!("{name}: {}", out.card.summary);
        }
        let all: BTreeSet<&str> = registry().iter().map(|s| s.name).collect();
        let missed: Vec<&&str> = all.difference(&seen).collect();
        assert!(missed.is_empty(), "not exercised: {missed:?}");
        assert!(a.plan_files().iter().any(|(_, t)| t == "Agent tools test"));
        let kita = a.find_card_id("Kitasan Black").unwrap();
        assert_eq!(a.owned_card(kita).map(|c| c.lb), Some(3));
        assert!(std::fs::read_to_string(dir.join("results/careers.md")).unwrap().contains("from the agent test"));
        assert_eq!(a.agent.open_after, Some(OpenAt::Card(kita)), "the last page asked for opens when the answer ends");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A Draft tool's change, then Undo from its card: the plan is as it was. Bad input changes
    /// nothing and says why.
    #[test]
    fn drafts_undo_and_bad_input_changes_nothing() {
        let ctx = egui::Context::default();
        let mut a = crate::App::for_tests();
        a.planner.demo(&a.roster, &a.kb, 100601);
        let before = serde_json::to_value(&a.planner.plan).unwrap();
        let Exec::Done(out) = a.agent_call(&ctx, "set_plan", &json!({"style": "End", "target_stats": [900, 900, 900, 900, 900]})) else { panic!("set_plan finishes at once") };
        assert!(!out.is_error, "{}", out.text);
        assert_eq!(a.planner.plan.style, crate::knowledge::Style::End);
        let idx = out.card.undo.expect("an undo point");
        let Exec::Done(deck) = a.agent_call(&ctx, "recommend_deck", &json!({})) else { panic!("recommend_deck finishes at once") };
        assert!(matches!(deck.card.visual, Visual::Deck { .. }));
        // Undo the first change: it and the deck after it go.
        let msg = a.agent_undo(idx).unwrap();
        assert!(msg.contains("1 change"), "{msg}");
        assert_eq!(serde_json::to_value(&a.planner.plan).unwrap(), before);
        assert!(a.agent_undo(idx).is_err(), "already undone");
        // Not owned, or two of one character: an error, and the deck stays.
        let deck_before = a.planner.plan.deck;
        for input in [json!({"cards": ["no such card at all"]}), json!({"cards": ["Kitasan Black", "Kitasan Black"]})] {
            let Exec::Done(out) = a.agent_call(&ctx, "set_deck", &input) else { panic!("set_deck finishes at once") };
            assert!(out.is_error, "{input}");
        }
        assert_eq!(a.planner.plan.deck, deck_before);
        let Exec::Done(out) = a.agent_call(&ctx, "set_plan", &json!({"style": "Sideways"})) else { panic!("finishes at once") };
        assert!(out.is_error && out.text.contains("must be one of"), "{}", out.text);
        let Exec::Done(out) = a.agent_call(&ctx, "set_plan", &json!({"stlye": "Pace"})) else { panic!("finishes at once") };
        assert!(out.is_error && out.text.contains("no field"), "{}", out.text);
        assert!(a.agent.undo.is_empty(), "failed calls leave no undo points");
    }

    /// Every icon the chat draws is in the app's fonts (a missing one draws as an empty box).
    #[test]
    fn tool_icons_render() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, crate::theme::Mode::Dark);
        let _ = crate::ui_util::test_frame(&ctx, Default::default(), |_| {});
        let font = egui::FontId::proportional(14.0);
        // 🗐 is egui_commonmark's copy button on code blocks in answers; ＋ is on Add, New plan and the raise buttons.
        let extra = ["💭", "✳", "✔", "✖", "↶", "🔧", "📋", "■", "➤", "⚠", "★", "🗐", "＋"];
        let missing: Vec<String> = registry()
            .iter()
            .map(|s| s.icon)
            .chain(extra)
            .filter(|icon| !ctx.fonts_mut(|f| f.has_glyphs(&font, icon)))
            .map(String::from)
            .collect();
        assert!(missing.is_empty(), "no glyph for {missing:?}");
        // The community tier list's marks, shown on the Skills page and in Step 5.
        for sym in ["⍟", "◎", "◯", "▲", "△", "✕"] {
            assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, sym)), "no glyph for the tier mark {sym}");
        }
        // The check can tell: a private-use code point no font draws. (It was 🧭, an empty box with
        // Windows' fonts, until Linux's Noto Emoji turned out to have it.)
        assert!(!ctx.fonts_mut(|f| f.has_glyphs(&font, "\u{10FFFD}")));
    }

    #[test]
    fn schemas_validate() {
        for s in registry() {
            assert_eq!(s.schema["type"], "object", "{}", s.name);
            assert!(s.description.len() > 20, "{}", s.name);
        }
        let names: BTreeSet<&str> = registry().iter().map(|s| s.name).collect();
        assert_eq!(names.len(), registry().len(), "tool names are unique");
        let schema = &find("run_compare").unwrap().schema;
        assert!(validate(schema, &json!({"runs": 500, "race": {"cm": "next", "ground": "Soft"}}), "").is_ok());
        assert!(validate(schema, &json!({"runs": 50}), "").is_err());
        assert!(validate(schema, &json!({"race": {"ground": "Mud"}}), "").is_err());
        assert!(validate(schema, &json!({"runs": null}), "").is_ok(), "null means left out");
        assert!(validate(&find("set_skills").unwrap().schema, &json!({}), "").is_err(), "required field");
        assert_eq!(find("umalord_get_plan").map(|s| s.name), Some("get_plan"), "OpenCode's MCP prefix");
        let api = api_tools(true);
        assert!(api.as_array().unwrap().iter().all(|t| t["eager_input_streaming"] == true));
        assert!(api.as_array().unwrap().last().unwrap()["cache_control"].is_object());
    }
}
