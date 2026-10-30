"""Build knowledge/data/*.json from uma.guide's Global data bundles.

uma.guide ships its database as Vite chunks containing `NAME=JSON.parse(`...`)` blocks. This
script downloads those chunks and pulls the JSON out as text; it never executes site code.

usage:
    python -I -X utf8 tools/update_knowledge.py            # download current chunks
    python -I -X utf8 tools/update_knowledge.py --from DIR # use chunk files already in DIR
    python -I -X utf8 tools/update_knowledge.py --only umalator   # just Moomoolator's data
    python -I -X utf8 tools/update_knowledge.py --only cm         # just the CM schedule
    python -I -X utf8 tools/update_knowledge.py --only meta       # just the PvP meta sheet
    python -I -X utf8 tools/update_knowledge.py --only affinity   # just the affinity, spark and win tables

Writes (all Global server data):
    knowledge/data/umas.json      trainees: stats, growth, aptitudes, skills, objectives, events
    knowledge/data/cards.json     support cards: effect tables, unique effect, hints, events
    knowledge/data/skills.json    skills: effects, activation conditions, SP cost
    knowledge/data/races.json     races: track, distance, surface, direction
    knowledge/data/glossary.json  community terms
    knowledge/data/meta.json      source chunks, fetch time, counts
    knowledge/data/umalator.json  from umalator.app: loadable skill ids, courses, CM/LoH presets
    knowledge/data/sim/*.json     Moomoolator's Global data files, unchanged, for the app's simulator
    knowledge/data/cm_schedule.json  uma.guide's CM list with the scenario current at each CM
    knowledge/data/affinity.json  the game's affinity tables (relation groups, their points and
                                  members), as uma.guide ships them; the app also refreshes them itself
    knowledge/data/sparks.json    every spark (factor) id: name, kind (blue, pink, green, white, race,
                                  scenario, bonus) and stars, for decoding finished umas
    knowledge/data/saddles.json   every win record (saddle) id: its name and the races it stands for
    knowledge/data/pvp_meta.json  the community PvP Deck & Uma Recommendation sheet (Google Sheets
                                  CSV export): decks, umas, stats and skill advice per CM/LoH and style
"""
import datetime as dt
import json
import os
import re
import sys
import urllib.request

SITE = "https://uma.guide"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "knowledge", "data")
UA = {"User-Agent": "umaLord-knowledge-builder/1.0 (personal use)"}
CHUNKS = ("uma-data", "event-rewards-data", "jp-support-events", "jp-dict-data")

# ---------------------------------------------------------------------------
# Fetching and extraction


def fetch(url: str) -> str:
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=120) as resp:
        return resp.read().decode("utf-8")


def discover_chunks() -> dict:
    """Map logical name -> chunk URL, found via the characters page and app.js."""
    html = fetch(f"{SITE}/characters/")
    scripts = set(re.findall(r"/assets/[A-Za-z0-9_./-]+\.js", html))
    app = next(s for s in scripts if re.search(r"/assets/app\.[\w-]+\.js$", s))
    names = " ".join(scripts) + " " + fetch(SITE + app)
    found = {}
    for key in CHUNKS:
        m = re.search(rf"chunks/{key}\.[A-Za-z0-9_-]+\.js", names)
        if not m:
            raise SystemExit(f"couldn't find the {key} chunk; uma.guide may have changed layout")
        found[key] = f"{SITE}/assets/{m.group(0)}"
    return found


SIMPLE = {"n": "\n", "t": "\t", "r": "\r", "b": "\b", "f": "\f", "v": "\v", "0": "\0"}


def unescape_template(s: str) -> str:
    out, i, n = [], 0, len(s)
    while i < n:
        c = s[i]
        if c != "\\":
            out.append(c)
            i += 1
            continue
        nxt = s[i + 1]
        if nxt in SIMPLE:
            out.append(SIMPLE[nxt]); i += 2
        elif nxt == "u" and s[i + 2] == "{":
            j = s.index("}", i)
            out.append(chr(int(s[i + 3:j], 16))); i = j + 1
        elif nxt == "u":
            out.append(chr(int(s[i + 2:i + 6], 16))); i += 6
        elif nxt == "x":
            out.append(chr(int(s[i + 2:i + 4], 16))); i += 4
        elif nxt == "\n":
            i += 2
        else:
            out.append(nxt); i += 2
    return "".join(out)


def json_blocks(src: str) -> list:
    out = []
    # Template literals, and single-quoted strings (the affinity tables), which escape the same way.
    for m in re.finditer(r"[\w$]+=JSON\.parse\(([`'])", src):
        start = i = m.end()
        quote = m.group(1)
        while True:
            j = src.index(quote, i)
            k, bs = j - 1, 0
            while src[k] == "\\":
                bs += 1; k -= 1
            if bs % 2 == 0:
                break
            i = j + 1
        out.append(json.loads(unescape_template(src[start:j])))
    return out


def pick(blocks: list, *keys: str) -> list:
    """The list block whose first record has all `keys`."""
    for b in blocks:
        if isinstance(b, list) and b and isinstance(b[0], dict) and all(k in b[0] for k in keys):
            return b
    raise SystemExit(f"no data block with keys {keys}; uma.guide may have changed its format")


# ---------------------------------------------------------------------------
# Normalising

STATS = ["speed", "stamina", "power", "guts", "wit"]
APT = {
    "turf": "aptitudeTurf", "dirt": "aptitudeDirt",
    "sprint": "aptitudeShort", "mile": "aptitudeMile", "medium": "aptitudeMiddle", "long": "aptitudeLong",
    "front": "aptitudeRunner", "pace": "aptitudeLeader", "late": "aptitudeBetweener", "end": "aptitudeChaser",
}
CARD_TYPE = {"Intelligence": "Wit", "Friend": "Pal"}
RARITY = {1: "R", 2: "SR", 3: "SSR"}


# uma.guide's names for skill effect types the data only numbers (from its app.js).
EFFECT_TYPES = {
    "Type6": "Runaway", "Type8": "Increase FOV", "Type10": "Improve Start Reaction Time",
    "Type13": "Increase Rush Time", "Type14": "Increase Start Delay", "Type21": "Decrease Current Speed",
    "Type22": "Increase Current Speed", "Type28": "Increase Lane Movement Speed", "Type29": "Increased Rush Chance",
    "Type32": "All Stats Increase", "Type35": "Change Lane", "Type37": "Use Random Gold Skills",
    "Type38": "Debuff Immunity", "Type41": "Force Activation", "Type42": "Conditional Increase Duration",
    "Type48": "Special", "Type49": "Reactivate Unique Skill",
}


def effect_list(effects) -> list:
    return [{"type": EFFECT_TYPES.get(e["type"], e["type"]), "value": e["value"]} for e in effects or []]


def date(s):
    return (s or "")[:10]


def skill_record(s: dict) -> dict:
    rec = {
        "id": s["skillId"], "name": s["skillName"], "desc": s.get("skillDesc", ""),
        "category": s.get("skillCategory", ""), "rarity": s.get("rarity", 0),
        "sp": s.get("needSkillPoint", 0), "grade": s.get("gradeValue", 0),
        "wit_check": s.get("activateLot", 0) == 1,
        "conditions": s.get("activationCondition", ""), "pre": s.get("precondition", ""),
        "effects": effect_list(s.get("effects")),
        "duration": s.get("duration", 0), "cooldown": s.get("cooldownTime", 0),
        "cards": [int(x) for x in str(s.get("supportCardIds") or "").split(",") if x.strip()],
    }
    if s.get("activationCondition2"):
        rec["alt"] = {
            "conditions": s["activationCondition2"], "pre": s.get("precondition2", ""),
            "effects": effect_list(s.get("effects2")),
            "duration": s.get("duration2", 0), "cooldown": s.get("cooldownTime2", 0),
        }
    return rec


def objective_text(o: dict) -> dict:
    place = str(o.get("placement", ""))
    if place == "participate":
        goal = "Enter"
    elif place == "1":
        goal = "Win"
    elif place.isdigit():
        goal = f"Top {place}"
    else:
        goal = place
    return {"when": o.get("deadline") or "", "race": o.get("requirement") or "", "goal": goal or "",
            "type": o.get("type") or "", "race_id": o.get("raceId")}


def name_skill_ids(text: str, names: dict) -> str:
    """Character event outcomes say '200472 hint +1'; swap the id for the skill name."""
    return re.sub(r"\b(\d{6,9})\b(?= hint)", lambda m: names.get(int(m.group(1)), m.group(1)), text)


def skill_ids(field) -> list:
    """An outcome's skill_id field may list several ids: '200941, 200442'."""
    return [int(x) for x in re.findall(r"\d+", str(field or ""))]


def support_outcome_lines(o: dict, suffix: str, names: dict) -> list:
    g = lambda field: o.get(f"{field}_{suffix}")
    def signed(v):
        v = str(v).strip()
        return v if v[:1] in "+-" else f"+{v}"
    lines = []
    for stat, label in (("speed", "Speed"), ("stamina", "Stamina"), ("power", "Power"), ("guts", "Guts"), ("wit", "Wit")):
        if g(stat):
            lines.append(f"{label} {signed(g(stat))}")
    if g("random_stat_amount"):
        lines.append(f"Random stats {signed(g('random_stat_amount'))} x{g('random_stat_count') or '?'}")
    if g("energy"):
        lines.append(f"Energy {signed(g('energy'))}")
    if g("max energy"):
        lines.append(f"Max energy {signed(g('max energy'))}")
    if str(g("full_energy_recovery")).upper() == "TRUE":
        lines.append("Energy fully recovered")
    if g("skill_point"):
        lines.append(f"Skill points {signed(g('skill_point'))}")
    if g("mood"):
        lines.append(f"Mood {signed(g('mood'))}")
    if g("bond"):
        lines.append(f"Bond {signed(g('bond'))}")
    if g("condition"):
        lines.append(f"Status: {g('condition')}")
    if g("heal_status"):
        lines.append(f"Heals: {g('heal_status')}")
    lv = g("hint_lv")
    for sid in skill_ids(g("skill_id")):
        sname = names.get(sid, str(sid))
        lines.append(f"{sname} hint +{lv}" if lv else f"{sname} (learned)")
    if g("random"):
        lines.append(f"Random: {g('random')}")
    if g("scenario_resource"):
        lines.append(f"Scenario: {g('scenario_resource')}")
    if str(g("ends_chain")).upper() == "TRUE":
        lines.append("Ends the event chain")
    return lines


def support_event_options(ev: dict, names: dict) -> tuple:
    """-> (options, skill ids). Each option is a list of alternative outcomes (random pick)."""
    options, skills = [], []
    for n in range(1, 6):
        opt = ev.get(f"Option_{n}")
        if not isinstance(opt, dict):
            continue
        alts = []
        for m in range(1, 6):
            suffix = f"Option_{n}_Outcome_{m}"
            o = opt.get(suffix)
            if not isinstance(o, dict):
                continue
            lines = support_outcome_lines(o, suffix, names)
            skills += skill_ids(o.get(f"skill_id_{suffix}"))
            if lines:
                alts.append(lines)
        if alts:
            options.append(alts)
    return options, skills


def unique_effect(u: dict, effect_names: dict):
    if not u:
        return None
    name = lambda t: effect_names.get(t, f"Effect type {t}")
    t = u.get("type0")
    effects, cond = [], ""
    if t == 101:
        cond = f"At Bond {u['value0']}"
        for a, b in (("value01", "value02"), ("value03", "value04")):
            if u.get(a) and u.get(b):
                effects.append({"name": name(u[a]), "value": f"+{u[b]}"})
    elif t and t < 100:
        for k in ("0", "1"):
            if u.get(f"type{k}"):
                effects.append({"name": name(u[f"type{k}"]), "value": f"+{u[f'value{k}']}"})
    else:
        v = lambda k: u.get(k, 0)
        table = {
            102: (f"At Bond {v('value0')}, not on matching facility", [(name(8), v("value01"))]),
            103: (f"With {v('value0')} different cards in deck", [(name(8), v("value01"))]),
            104: (f"+1 per {v('value0'):,} fans", [(name(8), f"up to {v('value01')}")]),
            105: ("Per card in deck", [("Respective initial stats", v("value0")), ("All stats (Pal cards)", v("value01"))]),
            106: (f"Per {v('value01')} friendship trainings (max {v('value0')}x)", [(name(1), v("value02"))]),
            107: ("Lower energy, bigger bonus", [(name(v("value0")), f"up to {v('value03')}")]),
            108: ("Higher max energy, bigger bonus", [(name(v("value0")), f"{v('value03')}~{v('value04')}")]),
            109: ("Based on total bond", [(name(v("value0")), "up to 20")]),
            110: ("Per support card on the same facility", [(name(v("value0")), f"+{v('value01')}")]),
            111: ("Per training facility level", [(name(v("value0")), f"+{v('value01')}")]),
            112: ("Chance to nullify failure", [("Probability", f"{v('value0')}%")]),
            113: ("During friendship training", [(name(v("value0")), v("value01"))]),
            114: ("More energy, bigger bonus", [(name(v("value0")), f"up to {v('value02')}")]),
            115: ("All support cards", [(name(v("value0")), f"+{v('value01')}")]),
            116: (f"Per {({1: 'speed', 2: 'acceleration', 3: 'recovery'}).get(v('value0'), 'matching')} skill (max {v('value03')})",
                  [(name(v("value01")), f"+{v('value02')} each")]),
        }
        cond, pairs = table.get(t, (f"Special effect type {t}", []))
        effects = [{"name": n, "value": str(val)} for n, val in pairs]
        # A second plain bonus can ride along on type1.
        if u.get("type1"):
            effects.append({"name": name(u["type1"]), "value": f"+{u['value1']}"})
    return {"level": u.get("level", 0), "condition": cond, "effects": effects}


def build(chunks: dict, sources: dict) -> None:
    uma_blocks = json_blocks(chunks["uma-data"])
    chars = pick(uma_blocks, "talentSpeed", "aptitudeTurf")
    skills_raw = pick(uma_blocks, "skillId", "activationCondition", "needSkillPoint")
    cards_raw = pick(uma_blocks, "supportCardId", "effects", "skillHints")
    races_raw = pick(uma_blocks, "raceId", "trackName")
    glossary = pick(uma_blocks, "term", "description")
    evolved_raw = pick(uma_blocks, "baseSkill", "evolvedSkill")
    char_events = pick(uma_blocks, "card_id", "events")
    objectives = pick(uma_blocks, "charaId", "objectives")
    support_events = pick(json_blocks(chunks["event-rewards-data"]), "story_id", "Option_1")
    jp_support_events = pick(json_blocks(chunks["jp-support-events"]), "story_id", "Option_1")

    names = {s["skillId"]: s["skillName"] for s in skills_raw}
    # uma.guide's dictionary chunk maps each skill id twice: first to its display name (newer
    # translations, e.g. "Gluttonous Ruler" rather than "Hungry Commander"), then to a readable
    # summary of effect and activation conditions, with <b>/<color> markup.
    dict_names, dict_summaries = {}, {}
    for k, v in re.findall(r'"(\d{5,9})":"((?:[^"\\]|\\.)*)"', chunks["jp-dict-data"]):
        k, v = int(k), json.loads(f'"{v}"')
        if k not in dict_names:
            dict_names[k] = v
        elif v != dict_names[k] and k not in dict_summaries:
            dict_summaries[k] = re.sub(r"<[^>]+>", "", v).replace("\\n", "\n").strip()
    skills = sorted((skill_record(s) for s in skills_raw), key=lambda s: s["id"])

    def add_side_skill(e: dict, category: str):
        """Evolved skills, and some of their base skills, aren't in the main skill list."""
        if e["skillId"] in names:
            return
        name = dict_names.get(e["skillId"], e["name"])
        names[e["skillId"]] = name
        rec = {
            "id": e["skillId"], "name": name, "desc": "", "category": category, "rarity": 0,
            "sp": e.get("cost") or 0, "grade": 0, "wit_check": e.get("activateLot") == 1,
            "conditions": (e.get("conditions") or "").replace("\n", ""), "pre": (e.get("preconditions") or "").replace("\n", ""),
            "effects": effect_list(e.get("effects")),
            "duration": e.get("duration") or 0, "cooldown": e.get("cooldown") or 0, "cards": [],
        }
        if e.get("conditions2"):
            rec["alt"] = {
                "conditions": e["conditions2"].replace("\n", ""), "pre": (e.get("preconditions2") or "").replace("\n", ""),
                "effects": effect_list(e.get("effects2")),
                "duration": e.get("duration2") or 0, "cooldown": e.get("cooldown2") or 0,
            }
        skills.append(rec)

    for ev in evolved_raw:
        add_side_skill(ev["baseSkill"], "Normal")
        add_side_skill(ev["evolvedSkill"], "Evolved")
    for s in skills:
        s["summary"] = dict_summaries.get(s["id"], "")

    # --- umas
    events_by_card = {c["card_id"]: c["events"] for c in char_events}
    objectives_by_id = {o["charaId"]: o["objectives"] for o in objectives}
    umas = []
    for c in chars:
        skill_ids = [int(x) for x in str(c.get("skillIds") or "").split(",") if x.strip()]
        potential = {p["skillId"]: p["needRank"] for p in c.get("potentialSkills", [])}
        unique = next((s for s in skill_ids if s not in potential), None)
        events, event_skills = [], []
        for e in events_by_card.get(c["cardId"], []):
            opts = []
            for o in e.get("options") or []:
                raw = [x for x in o.get("outcomes") or [] if isinstance(x, str) and x.strip()]
                event_skills += [int(m) for x in raw for m in re.findall(r"\b(\d{6,9})\b(?= hint)", x)]
                opts.append({"label": o.get("option_name") or "", "outcomes": [name_skill_ids(x, names) for x in raw]})
            events.append({"name": e.get("event_name") or "", "type": e.get("event_type") or "", "options": opts})
        # Evolved skill ids start with the outfit's card id: 103301111 belongs to card 103301.
        evolved = [
            {"base": ev["baseSkill"]["skillId"], "evolved": ev["evolvedSkill"]["skillId"], "conditions": ev.get("evolutionConditions") or []}
            for ev in evolved_raw
            if str(ev["evolvedSkill"]["skillId"]).startswith(str(c["cardId"]))
        ]
        umas.append({
            "id": c["cardId"], "chara_id": c["charaId"], "name": c["charaName"],
            "outfit": c["cardTitle"].strip("[]"), "release": date(c.get("startDate")),
            "base": [c["baseSpeed"], c["baseStamina"], c["basePower"], c["baseGuts"], c["baseWisdom"]],
            "growth": [c["talentSpeed"], c["talentStamina"], c["talentPower"], c["talentGuts"], c["talentWisdom"]],
            "aptitude": {k: c[v] for k, v in APT.items()},
            "unique_skill": unique,
            "skills": [{"id": s, "potential": max(1, r)} for s, r in sorted(potential.items(), key=lambda kv: kv[1])],
            "event_skills": sorted(set(event_skills)),
            "evolved": evolved,
            "objectives": [objective_text(o) for o in objectives_by_id.get(c.get("uraObjectives"), [])],
            "events": events,
        })

    # --- cards
    effect_names = {}
    for c in cards_raw:
        for e in c["effects"]:
            effect_names[e["effectType"]] = e["effectTypeName"]
    ev_by_story = {str(e["story_id"]): e for e in jp_support_events}
    ev_by_story.update({str(e["story_id"]): e for e in support_events})  # prefer Global text
    cards = []
    for c in cards_raw:
        ctype = "Group" if c["supportCardType"] == 3 else CARD_TYPE.get(c["supportCardTypeName"], c["supportCardTypeName"])
        effects = []
        for e in c["effects"]:
            points = []
            if e["initValue"] != -1:
                points.append([1, e["initValue"]])
            for lvl in range(5, 55, 5):
                v = e.get(f"level{lvl}Value", -1)
                if v != -1:
                    points.append([lvl, v])
            effects.append({"type": e["effectType"], "name": e["effectTypeName"], "points": points})
        events, event_skills = [], []
        for e in c.get("events", []):
            src = ev_by_story.get(str(e["storyId"]))
            options, sk = support_event_options(src, names) if src else ([], [])
            event_skills += sk
            events.append({"story_id": e["storyId"], "title": e["eventTitle"], "type": e.get("eventType", ""),
                           "order": e.get("eventOrder", 0), "options": options})
        cards.append({
            "id": c["supportCardId"], "chara_id": c["charaId"], "uma": c["charaName"],
            "title": c["supportCardTitle"].strip("[]"), "rarity": RARITY.get(c["rarity"], str(c["rarity"])),
            "type": ctype, "release": date(c.get("startDate")),
            "effects": effects, "unique": unique_effect(c.get("uniqueEffect"), effect_names),
            "hints": [h["skillId"] for h in c.get("skillHints", []) if h.get("skillId")],
            "event_skills": sorted(set(event_skills)),
            "events": events,
        })

    races = [{
        "id": r["raceId"], "name": r["raceName"], "grade": r.get("gradeName", ""), "distance": r["distance"],
        "category": r.get("distanceCategory", ""), "surface": r.get("groundName", ""),
        "direction": r.get("turnName", ""), "track": r.get("trackName", ""), "track_id": r.get("trackId"),
    } for r in races_raw]

    os.makedirs(OUT, exist_ok=True)
    def write(name, data):
        # One record per line: still valid JSON, and grep/search tools return single records.
        with open(os.path.join(OUT, name), "w", encoding="utf-8") as f:
            if isinstance(data, list):
                f.write("[\n" + ",\n".join(json.dumps(r, ensure_ascii=False, separators=(",", ":")) for r in data) + "\n]\n")
            else:
                json.dump(data, f, ensure_ascii=False, indent=1)
    write("umas.json", umas)
    write("cards.json", cards)
    write("skills.json", skills)
    write("races.json", races)
    write("glossary.json", [{"term": g["term"], "aliases": g.get("aliases", []), "description": g["description"]} for g in glossary])
    meta = {
        "source": SITE, "server": "Global", "built": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "chunks": sources,
        "counts": {"umas": len(umas), "cards": len(cards), "skills": len(skills), "races": len(races), "glossary": len(glossary)},
        "notes": [
            "Support effect values between listed levels are linear interpolations rounded down, as uma.guide shows them.",
            "Event options with several outcomes pick one at random; the source data has no odds.",
        ],
    }
    write("meta.json", meta)
    print(json.dumps(meta["counts"]))


UMALATOR = "https://umalator.app"
SURFACE = {1: "Turf", 2: "Dirt"}
LAYOUT = {2: "inner", 3: "outer", 4: "outer→inner"}
TURN = {1: "Right", 2: "Left", 4: "Straight"}
EVENT_TYPE = {0: "Champions Meeting", 1: "League of Heroes"}
# Umalator's condition codes, 1-based, in the order the planner's condition chips use.
GROUND = ["Firm", "Good", "Soft", "Heavy"]
WEATHER = ["Sunny", "Cloudy", "Rainy", "Snowy"]
SEASON = ["Spring", "Summer", "Autumn", "Winter"]
TIME = {2: "Midday", 3: "Evening", 4: "Night"}
PRESET = re.compile(
    r'\{id:(\d+),type:(\d+),name:"([^"]+)",date:"([\d-]+)",courseId:(\d+),season:(\d),ground:(\d),weather:(\d),time:(\d),confirmed:!([01])\}'
)


def course_name(c: dict) -> str:
    layout = f" ({c['layout']})" if c["layout"] else ""
    return f"{c['track']} {c['surface']} {c['distance']}m{layout}"


def presets(bundle: str, courses: dict) -> list:
    """Champions Meeting / League of Heroes presets: course and conditions per event date."""
    out = []
    for m in PRESET.findall(bundle):
        course, season, ground, weather, time = int(m[4]), int(m[5]), int(m[6]), int(m[7]), int(m[8])
        named = [GROUND[ground - 1] if 0 < ground <= 4 else "", WEATHER[weather - 1] if 0 < weather <= 4 else "",
                 SEASON[season - 1] if 0 < season <= 4 else "", TIME.get(time, "")]
        out.append({
            "id": int(m[0]), "event": EVENT_TYPE.get(int(m[1]), f"type {m[1]}"), "name": m[2], "date": m[3],
            "course": course, "course_name": course_name(courses[course]) if course in courses else "",
            "season": season, "ground": ground, "weather": weather, "time": time,
            "conditions": " · ".join(n for n in named if n), "confirmed": m[9] == "0",
        })
    return out


SIM_FILES = ("skill_data.json", "skill_meta.json", "course_data.json", "umas.json", "skillnames.json", "tracknames.json", "not-in-game.json")


def write_sim_data(data: str) -> dict:
    """Moomoolator's Global data files, saved unchanged in knowledge/data/sim/ for the app's
    native simulator (a port of its engine). Returns the parsed files."""
    out_dir = os.path.join(OUT, "sim")
    os.makedirs(out_dir, exist_ok=True)
    files = {}
    for name in SIM_FILES:
        text = fetch(data + name)
        files[name] = json.loads(text)
        with open(os.path.join(out_dir, name), "w", encoding="utf-8", newline="") as f:
            f.write(text)
    return files


def write_umalator() -> None:
    """What Moomoolator (umalator.app) can load: skill ids, courses, and its event presets.

    Its import drops skill ids missing from skill_data.json. That file is a JP superset;
    not-in-game.json lists the ids that aren't on Global yet.
    """
    data = UMALATOR + "/umalator-global/"
    files = write_sim_data(data)
    skills = sorted(int(k) for k in files["skill_data.json"])
    not_in_game = files["not-in-game.json"]
    try:
        with open(os.path.join(OUT, "races.json"), encoding="utf-8") as f:
            tracks = {r["track_id"]: r["track"] for r in json.load(f) if r.get("track_id")}
    except OSError:
        tracks = {}
    courses = [
        {"id": int(k), "track_id": c["raceTrackId"], "track": tracks.get(c["raceTrackId"], str(c["raceTrackId"])),
         "distance": c["distance"], "surface": SURFACE.get(c["surface"], "?"), "layout": LAYOUT.get(c["course"], ""),
         "direction": TURN.get(c["turn"], "?")}
        for k, c in sorted(files["course_data.json"].items(), key=lambda kv: int(kv[0]))
    ]
    out = {
        "source": UMALATOR, "built": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "skills": skills, "not_in_game": sorted(int(s) for s in not_in_game.get("skills", [])),
        "courses": courses, "presets": [],
    }
    try:  # presets only live in the minified app bundle; the rest is still useful without them
        app = re.search(r'/assets/index-[\w-]+\.js', fetch(UMALATOR + "/"))
        by_id = {c["id"]: c for c in courses}
        out["presets"] = presets(fetch(UMALATOR + app.group(0)), by_id) if app else []
    except Exception as e:
        print(f"couldn't read Moomoolator presets: {e}")
    # One course or preset per line, so a search for "Capricorn" finds the whole record.
    fields = [f"{json.dumps(k)}: " + (
        "[\n" + ",\n".join(json.dumps(x, ensure_ascii=False) for x in v) + "\n]" if k in ("courses", "presets")
        else json.dumps(v, ensure_ascii=False)
    ) for k, v in out.items()]
    with open(os.path.join(OUT, "umalator.json"), "w", encoding="utf-8") as f:
        f.write("{\n" + ",\n".join(fields) + "\n}\n")
    print(f"umalator: {len(skills)} skills ({len(out['not_in_game'])} not on Global), {len(courses)} courses, {len(out['presets'])} presets")


CM_FIELD = re.compile(r'(\w+):("(?:[^"\\]|\\.)*"|-?[\d.]+(?:e\d+)?|!0|!1)')


def write_cm_schedule(uma_data: str) -> None:
    """uma.guide's Champions Meeting list, which names the training scenario that was current for
    each CM (e.g. CM 20 Sagittarius Cup: Grand Concert). Read as text from the data chunk."""
    m = re.search(r"=\[(\{cmNumber:1,.*?\})\]", uma_data)
    if not m:
        print("couldn't find uma.guide's CM schedule")
        return
    out = []
    for obj in re.findall(r"\{cmNumber:[^{}]*\}", m.group(0)):
        rec = {}
        for k, v in CM_FIELD.findall(obj):
            if v.startswith('"'):
                rec[k] = json.loads(v)
            elif v in ("!0", "!1"):
                rec[k] = v == "!0"
            else:
                rec[k] = int(float(v)) if float(v).is_integer() else float(v)
        out.append({
            "series": rec.get("series", "CM"), "number": rec.get("cmNumber"), "name": rec.get("name", ""),
            "venue": rec.get("venue", ""), "surface": rec.get("surface", ""), "distance": rec.get("distance", 0),
            "category": rec.get("distanceCategory", ""), "direction": rec.get("direction", ""),
            "ground": rec.get("condition", ""), "season": rec.get("season", ""), "weather": rec.get("weather", ""),
            "scenario": rec.get("scenario", ""),
        })
    with open(os.path.join(OUT, "cm_schedule.json"), "w", encoding="utf-8") as f:
        f.write("[\n" + ",\n".join(json.dumps(r, ensure_ascii=False) for r in out) + "\n]\n")
    print(f"cm schedule: {len(out)} events")


def write_affinity(uma_data: str) -> None:
    """The game's affinity tables, which uma.guide's data chunk carries as-is: every relation group
    with its points (`succession_relation`) and which characters belong to it
    (`succession_relation_member`). Two umas' base affinity is the points of the groups they share;
    the app does the sums (`affinity.rs`), and refreshes this file's contents itself (`live.rs`)."""
    blocks = json_blocks(uma_data)
    groups = pick(blocks, "relationType", "relationPoint")
    members = pick(blocks, "charaId", "relationType")
    by_chara: dict = {}
    for m in members:
        by_chara.setdefault(m["charaId"], []).append(m["relationType"])
    out = {
        "source": "uma.guide's data (the game's succession_relation and succession_relation_member tables)",
        "fetched": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "points": {str(g["relationType"]): g["relationPoint"] for g in sorted(groups, key=lambda g: g["relationType"])},
        "members": {str(c): sorted(set(t)) for c, t in sorted(by_chara.items())},
    }
    with open(os.path.join(OUT, "affinity.json"), "w", encoding="utf-8") as f:
        json.dump(out, f, separators=(",", ":"))
        f.write("\n")
    print(f"affinity: {len(out['points'])} relation groups, {len(out['members'])} characters")


SPARK_KINDS = {1: "blue", 2: "pink", 3: "green", 4: "white", 5: "race", 6: "scenario", 7: "bonus"}


def write_sparks(uma_data: str) -> None:
    """Every spark (the game's factors) by id, as uma.guide's Roster Viewer decodes them: a
    finished uma's `factor_info_array` holds these ids (UmaExtractor's data.json)."""
    rows = pick(json_blocks(uma_data), "id", "name", "rarity", "grade", "type")
    out = [{"id": r["id"], "name": r["name"], "kind": SPARK_KINDS.get(r["type"], str(r["type"])), "stars": r["rarity"]} for r in sorted(rows, key=lambda r: r["id"])]
    with open(os.path.join(OUT, "sparks.json"), "w", encoding="utf-8") as f:
        f.write("[\n" + ",\n".join(json.dumps(r, ensure_ascii=False) for r in out) + "\n]\n")
    print(f"sparks: {len(out)}")


SADDLE = re.compile(r'(\d+):\{races:\[(.*?)\],name:"((?:[^"\\]|\\.)*)"\}')


def write_saddles(uma_data: str) -> None:
    """Every win record (saddle) by id: its name and the races it stands for (race ids as in
    races.json; a title like the Triple Crown stands for several). A finished uma's
    `win_saddle_id_array` holds these ids. uma.guide writes the table as a plain object literal,
    read here as text."""
    m = re.search(r"=\{1:\{races:\[", uma_data)
    if not m:
        raise SystemExit("couldn't find uma.guide's win table; its format may have changed")
    depth, end = 0, None
    for k in range(m.end() - len("{1:{races:["), len(uma_data)):
        c = uma_data[k]
        depth += c == "{"
        depth -= c == "}"
        if depth == 0:
            end = k + 1
            break
    out = {}
    for sid, races, name in SADDLE.findall(uma_data[m.start():end]):
        out[sid] = {"name": unescape_template(name), "races": sorted({int(r) // 100 for r in re.findall(r"id:(\d+)", races)})}
    with open(os.path.join(OUT, "saddles.json"), "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False, separators=(",", ":"))
        f.write("\n")
    print(f"saddles: {len(out)}")


# ---------------------------------------------------------------------------
# PvP meta: the community "PvP Deck & Uma Recommendation" Google Sheet (Lightning, Daptrius,
# Henry). Its CSV exports are read as text. Card IDs come from the PvP Rankings tab; notes, LB
# minimums, umas, stats and skill advice from the PvP List tab, matched by position.

META_SHEET = "100t3hnYl5Qm2UR8RtPlH-8Xd9KQbBlxEdXUOIR4d394"
META_TABS = {"list": 1195900779, "rankings": 1843247484}
META_STYLES = ("Front", "Pace", "Late", "End")
META_TIERS = ("Beginner", "Standard", "Whale")
# Placeholder slots ("any Wit card").
META_ANY = {"Speed", "Stamina", "Power", "Guts", "Gut", "Wit", "Pal"}
# Card-note words that aren't skills.
META_GENERIC = re.compile(r"^(\d ?SP|Gold|\d Golds|Link|Hints|Stats|Events|U|Any [\w ]+|Ini F[.,]?|-|or .*|stam|Stam|high FS|\d+ spe rate)$", re.I)


def sheet_csv(gid: int) -> list:
    import csv
    import io
    text = fetch(f"https://docs.google.com/spreadsheets/d/{META_SHEET}/export?format=csv&gid={gid}")
    return list(csv.reader(io.StringIO(text)))


def read_jsonl(name: str) -> list:
    out = []
    with open(os.path.join(OUT, name), encoding="utf-8") as f:
        for line in f:
            line = line.strip().rstrip(",")
            if line.startswith("{"):
                out.append(json.loads(line))
    return out


def norm_name(s: str) -> str:
    return re.sub(r"[^a-z0-9]+", " ", s.lower().replace("○", "").replace("◎", "")).strip()


def initials(name: str) -> str:
    words = [w for w in norm_name(name).split() if w not in ("of", "the", "a", "and", "s")]
    return "".join(w[0] for w in words)


def subsequence(abbr: str, name: str) -> bool:
    n = norm_name(name).replace(" ", "")
    if not abbr or not n or n[0] != abbr[0]:
        return False
    it = iter(n)
    return all(ch in it for ch in abbr)


def resolve_note(token: str, card_skills: list, skills: dict):
    """A card note word ("Left-Handed", "LS", "GW*") as one of that card's skills, or None."""
    gold = bool(re.search(r"\bgold\b", token, re.I))
    t = re.sub(r"\bgold\b|\*", "", token, flags=re.I).strip()
    if not t:
        return None
    key = norm_name(t)
    abbr = key.replace(" ", "")
    cands = [s for s in dict.fromkeys(card_skills) if s in skills]
    # A white's gold upgrade counts too (Focus → Concentration): the id ending in 1 for one ending in 2.
    cands += [s - 1 for s in cands if s % 10 == 2 and s - 1 in skills and skills[s - 1]["rarity"] >= 2 and s - 1 not in cands]
    for test in (
        lambda s: norm_name(skills[s]["name"]) == key,
        lambda s: norm_name(skills[s]["name"]).startswith(key) and len(key) >= 4,
        lambda s: initials(skills[s]["name"]) == abbr and len(abbr) >= 2,
        lambda s: subsequence(abbr, skills[s]["name"]) and 2 <= len(abbr) <= 4,
    ):
        hits = [s for s in cands if test(s)]
        if gold:
            hits = [s for s in hits if skills[s]["rarity"] >= 2] or hits
        if len(hits) == 1:
            return hits[0]
        if hits:
            # A gold and its white share a name stem: take the rarer one when asked, else the white.
            hits.sort(key=lambda s: -skills[s]["rarity"] if gold else skills[s]["rarity"])
            if len({norm_name(skills[s]["name"]) for s in hits}) == 1:
                return hits[0]
    return None


def write_pvp_meta() -> None:
    lst = sheet_csv(META_TABS["list"])
    rank = sheet_csv(META_TABS["rankings"])
    skills = {s["id"]: s for s in read_jsonl("skills.json")}
    cards = {c["id"]: c for c in read_jsonl("cards.json")}
    umas = read_jsonl("umas.json")
    cell = lambda rows, i, j: rows[i][j].strip() if 0 <= i < len(rows) and j < len(rows[i]) else ""
    event_key = lambda s: (lambda m: (m.group(1), int(m.group(2))) if m else None)(re.match(r"\s*(CM|LoH) #(\d+)", s))

    # Rankings: ordered card entries per (event, style, tier), with IDs.
    hdr = rank[1]
    c0 = next(i for i, h in enumerate(hdr) if h == "Scenario" and i > 40)
    entries, scenario_of, title_of = {}, {}, {}
    for r in rank[2:]:
        if len(r) <= c0 + 8 or not r[c0 + 5].strip():
            continue
        key = event_key(r[c0 + 5])
        if not key:
            continue
        scenario_of[key] = r[c0 + 4].strip()
        title_of[key] = re.sub(r"^\s*(CM|LoH) #\d+\s*", "", r[c0 + 5]).strip()
        entries.setdefault((key, r[c0 + 6].strip(), r[c0 + 8].strip()), []).append(r[c0 + 7].strip())

    def card_entry(raw: str, note: str = "", lb: str = "") -> dict:
        m = re.match(r"(\d+)\s+(.*?)(?:\s+(?:CM|LoH) #\d+)?$", raw)
        if m and int(m.group(1)) in cards:
            cid = int(m.group(1))
            e = {"id": cid, "name": cards[cid]["uma"], "type": cards[cid]["type"]}
        else:
            name = m.group(2) if m else raw
            e = {"id": None, "name": name}
            if name in META_ANY:
                e["any"] = "Guts" if name == "Gut" else name
        if note:
            e["note"] = note
        if "◆" in lb or "◇" in lb:
            e["min_lb"] = lb.count("◆")
            alt = re.sub(r"^[◆◇\s+]+", "", lb).strip()
            if alt:
                e["or"] = alt
        elif lb:
            e["remark"] = lb
        return e

    def uma_ref(text: str):
        """'King Halo (Cheerleader)' → (id or None, name)."""
        m = re.match(r"(.*?)\s*(?:\((.*?)\))?$", text.strip())
        name, desc = m.group(1).strip(), (m.group(2) or "").strip()
        outfits = sorted((u for u in umas if u["name"] == name), key=lambda u: u["id"])
        if not outfits:
            return None, text.strip()
        if desc and desc.upper() != "OG":
            hit = [u for u in outfits if desc.lower() in u["outfit"].lower()]
            alts = outfits[1:]
            pick = hit[0] if len(hit) == 1 else (alts[0] if len(alts) == 1 else None)
        else:
            pick = outfits[0]
        return (pick["id"] if pick else None), text.strip()

    # Short names umas go by in skill advice: "Pasa", "Machan", "King (Cheer)", "OG Maru".
    # Words that are also ordinary English, or part of skill names, don't count.
    stop = {"super", "special", "silence", "nice", "smart", "twin", "gold", "light", "wonder", "fine", "motion", "rice",
            "shower", "sweep", "dream", "star", "spirit", "speed", "power", "flash", "victory", "ticket", "sunday", "lucky",
            "seeking", "pearl", "city", "ship", "dancer", "dance", "heart", "brian", "beauty", "royce", "symboli"}
    short = {}
    for u in umas:
        words = u["name"].split()
        keys = {words[0], words[-1]} if len(words) > 1 else {u["name"], u["name"][:4]}
        for k in keys:
            if len(k) >= 4 and k.lower() not in stop:
                short.setdefault(k.lower(), set()).add(u["name"])

    def parents_in(text: str) -> list:
        out = []
        for m in re.finditer(r"\b(OG\s+)?([A-Za-z]{4,})\b(?:\s*\(([^)]+)\))?", text):
            names = short.get(m.group(2).lower())
            if not names or len(names) != 1:
                continue
            name = next(iter(names))
            outfits = sorted((u for u in umas if u["name"] == name), key=lambda u: u["id"])
            desc = (m.group(3) or "").strip()
            pick = None
            if m.group(1) or not desc:
                pick = outfits[0]
            else:
                hit = [u for u in outfits if desc.lower() in u["outfit"].lower()]
                alts = outfits[1:]
                pick = hit[0] if len(hit) == 1 else (alts[0] if len(alts) == 1 else None)
            if not pick:
                continue
            inherited = 900000 + pick["unique_skill"] % 100000
            if inherited in skills and all(p["id"] != pick["id"] for p in out):
                label = f"{name} ({desc})" if desc and not m.group(1) else name
                out.append({"uma": label, "id": pick["id"], "skill": inherited})
        return out

    # Skill names worth finding in free text: buyable, not unique, Title Case in the text.
    by_name = {}
    for s in skills.values():
        if s["sp"] > 0 and s["category"] != "Unique" and 1 <= s["rarity"] <= 2:
            by_name.setdefault(norm_name(s["name"]), []).append(s["id"])
    ignited = {n: ids for n, ids in by_name.items() if n.startswith("ignited spirit ")}

    def skills_in(text: str, aliases: dict) -> list:
        found = []
        add = lambda sid, why: found.append({"id": sid, "from": why}) if all(f["id"] != sid for f in found) else None
        for n, ids in by_name.items():
            if len(n) < 5:
                continue
            pattern = r"(?<![A-Za-z0-9])(" + r"[\W_]*".join(re.escape(w) for w in n.split()) + r")(?![A-Za-z0-9])"
            # Capitalised like a name: "extra acceleration" isn't the skill Acceleration.
            if any(m.group(1)[0].isupper() for m in re.finditer(pattern, text, re.I)):
                # "Left-Handed ○" and "◎" share a name; the ○ (higher id) is the one hints give.
                add(max(ids), "advice")
        for m in re.finditer(r"Ignited\s+(PWR|POW|WIT|SPD|SPE|STA|GUTS?)", text, re.I):
            stat = {"pwr": "pwr", "pow": "pwr", "wit": "wit", "spd": "spd", "spe": "spd", "sta": "sta", "gut": "guts", "guts": "guts"}[m.group(1).lower()]
            for n, ids in ignited.items():
                if n.endswith(" " + stat):
                    add(ids[0], "advice")
        if re.search(r"\b3 lanes\b", text, re.I):
            # The sheet: "3 lanes means the three movement combo skills, which is dodging danger + prudent position + ignited WIT".
            for n in ("dodging danger", "prudent positioning", "ignited spirit wit"):
                if n in by_name:
                    add(by_name[n][0], "advice (3 Lanes)")
        for abbr, sid in aliases.items():
            if re.search(r"(?<![A-Za-z])" + re.escape(abbr) + r"(?![A-Za-z])", text):
                add(sid, "advice")
        return found

    events = []
    starts = [i for i, r in enumerate(lst) if r and re.match(r"^(CM|LoH) #\d+\s*$", r[0])] + [len(lst)]
    mismatches = 0
    for b, e in zip(starts, starts[1:]):
        key = event_key(lst[b][0])
        ev = {"series": key[0], "number": key[1], "name": title_of.get(key, "").title(), "scenario": scenario_of.get(key, "")}
        styles = {s: {"decks": {}, "honorable": []} for s in META_STYLES}
        for i in range(b, e):
            c1 = cell(lst, i, 1)
            if re.match(r"^(CM|LoH) #\d+", c1) and cell(lst, i, 4) == "Distance":
                ev["distance"] = int(re.sub(r"\D", "", cell(lst, i + 1, 4)) or 0)
                ev["category"] = cell(lst, i + 1, 6)
                ev["stats"] = [cell(lst, i + 1, j) for j in range(9, 14)]
                ev["analysis"] = cell(lst, i, 23)
            elif c1 == "Terrain":
                ev.update(zip(("surface", "track", "direction", "season", "weather", "ground"), (cell(lst, i + 1, j) for j in range(1, 7))))
                ev["description"] = cell(lst, i, 9)
                ev["style_ranking"] = cell(lst, i + 1, 24)
            elif c1.startswith("Recommended Front"):
                for si, s in enumerate(META_STYLES):
                    out = []
                    for k in range(6):
                        name = cell(lst, i + 2, 1 + si * 7 + k)
                        if name:
                            uid, label = uma_ref(name)
                            out.append({"name": label, "id": uid, "rating": cell(lst, i + 3, 1 + si * 7 + k)})
                    styles[s]["umas"] = out
            elif c1 in ("Rec. Accel", "Rec. Skills"):
                for si, s in enumerate(META_STYLES):
                    styles[s]["accel" if c1 == "Rec. Accel" else "advice"] = cell(lst, i, 2 + si * 7)
            elif re.match(r"^(BEGINNER|STANDARD|WHALE) - FRONT", c1):
                tier = c1.split()[0].title()
                for si, s in enumerate(META_STYLES):
                    raw = entries.get((key, s, tier), [])
                    deck = []
                    for k in range(6):
                        col = 1 + si * 7 + k
                        listed = cell(lst, i + 1, col)
                        if k >= len(raw):
                            continue
                        entry = card_entry(raw[k], cell(lst, i + 3, col), cell(lst, i + 4, col))
                        if listed and norm_name(entry["name"]) != norm_name(listed) and not entry.get("any") and entry.get("id"):
                            mismatches += 1
                        deck.append(entry)
                    if deck:
                        styles[s]["decks"][tier] = {"cards": deck[:5], "borrow": deck[5] if len(deck) > 5 else None}
            elif c1 == "HONORABLE MENTIONS":
                for si, s in enumerate(META_STYLES):
                    raw = entries.get((key, s, "Honorable"), [])
                    styles[s]["honorable"] = [card_entry(x, cell(lst, i + 6, 1 + si * 7 + k)) for k, x in enumerate(raw)]
        ev["new_cards"] = [card_entry(x) for x in entries.get((key, "Front", "Yada"), []) if re.match(r"\d", x)]
        ev["styles"] = styles
        events.append(ev)

    # What each recommended card is brought for: its note's words as that card's skills. Shorthand
    # that always means the same skill ("GW", "NSM") also reads the advice text of other events.
    def note_skills(c: dict):
        if not c.get("id") or not c.get("note"):
            return
        info = cards[c["id"]]
        own = info.get("hints", []) + info.get("event_skills", [])
        for tok in re.split(r"\s*[+/]\s*", c["note"]):
            tok = tok.strip()
            if re.match(r"^(Gold|\d Golds)\*?$", tok, re.I):
                for sid in dict.fromkeys(own):
                    if sid in skills and skills[sid]["rarity"] >= 2:
                        yield None, sid, f"{info['uma']}'s gold"
            elif tok and not META_GENERIC.match(tok.replace("*", "").strip()):
                sid = resolve_note(tok, own, skills)
                if sid:
                    yield tok.replace("*", "").strip(), sid, f"{info['uma']}: {tok}"

    seen = {}
    for ev in events:
        for st in ev["styles"].values():
            for c in [c for d in st["decks"].values() for c in d["cards"] + [d["borrow"]] if c] + st["honorable"]:
                for tok, sid, _ in note_skills(c):
                    if tok:
                        seen.setdefault(tok, set()).add(sid)
    shared = {tok: next(iter(ids)) for tok, ids in seen.items() if len(ids) == 1}
    for ev in events:
        for st in ev["styles"].values():
            found = []
            local = {}
            for tier, deck in st["decks"].items():
                for c in deck["cards"] + ([deck["borrow"]] if deck["borrow"] else []):
                    for tok, sid, why in note_skills(c):
                        if tok:
                            local[tok] = sid
                        if all(f["id"] != sid for f in found):
                            found.append({"id": sid, "from": f"{why} ({tier} deck)"})
            text = " ".join(x for x in (st.get("accel", ""), st.get("advice", "")) if x)
            for f in skills_in(text, {**shared, **local}):
                same = [g for g in found if g["id"] == f["id"]]
                if same:
                    same[0]["from"] = "advice; " + same[0]["from"]
                else:
                    found.insert(sum(1 for g in found if g["from"].startswith("advice")), f)
            st["skills"] = found
            st["parents"] = parents_in(text)
    out = {
        "source": f"https://docs.google.com/spreadsheets/d/{META_SHEET}/ (PvP Deck & Uma Recommendation; PvP List and PvP Rankings tabs)",
        "authors": "Lightning (author), Daptrius (sheet), Henry (revisor); Henry Handsome Derby discord",
        "fetched": dt.date.today().isoformat(),
        "caveat": "Community opinion, not game data. The sheet says to take skill advice and gold notes beyond CM #21 with a grain of salt; later events use JP cards and names.",
    }
    with open(os.path.join(OUT, "pvp_meta.json"), "w", encoding="utf-8") as f:
        head = json.dumps(out, ensure_ascii=False)[:-1]
        f.write(head + ',"events":[\n' + ",\n".join(json.dumps(ev, ensure_ascii=False) for ev in events) + "\n]}\n")
    print(f"pvp meta: {len(events)} events, {mismatches} name mismatches between the sheet's tabs")


# The community skill tier list: "Uma Musume Skills Spreadsheet" by Dylan (@dylank0), formatted
# by sayaduck; its Tierlist tab ranks every skill for Team Trials and for Champions Meeting.
TIER_SHEET = "1oB3eTvKqREtJDWJL0q80O_VjBcpOmRl5xE0z5fZKgFY"
TIER_TABS = {"readme": 2142834421, "tierlist": 1499223104}
# What each template placeholder can stand for (others match any words).
TIER_FILLS = {
    "Run Style": "Front Runner|Pace Chaser|Late Surger|End Closer",
    "Distance": "Sprint|Mile|Medium|Long",
    "Rotation": "Right|Left",
    "Season": "Spring|Summer|Fall|Autumn|Winter",
    "Weather": "Sunny|Cloudy|Rainy|Snowy",
}
TIER_SCALE = {
    "⍟": "essential for its distance or style",
    "◎": "among the best for its distance or style",
    "◯": "good, but short of ◎ in strength or conditions",
    "▲": "situational: anywhere from ⍟ to ✕ depending on the race",
    "△": "only with spare SP or nothing better",
    "✕": "useless or very inconsistent",
}


def write_skill_tiers() -> None:
    """knowledge/data/skill_tiers.json: each skill's Team Trials and CM tier from the sheet, by id.

    The sheet writes families as templates ("[Run Style] Corners ◯", "Flustered [Run Style]s"); a
    template matches every skill whose name fits it. Community opinion, not game data."""
    import csv
    import io

    def tab(gid: int) -> list:
        text = fetch(f"https://docs.google.com/spreadsheets/d/{TIER_SHEET}/gviz/tq?tqx=out:csv&gid={gid}")
        return list(csv.reader(io.StringIO(text)))

    readme = tab(TIER_TABS["readme"])
    updated = ""
    for row in readme:
        for c in row:
            m = re.search(r"Last Update:\s*([A-Z][a-z]+ \d{1,2})(?:st|nd|rd|th)?, (\d{4})", c)
            if m and not updated:
                updated = dt.datetime.strptime(f"{m.group(1)} {m.group(2)}", "%B %d %Y").date().isoformat()
    rows = tab(TIER_TABS["tierlist"])
    skills = read_jsonl("skills.json")
    fold = lambda s: re.sub(r"\s+", " ", s.replace("◯", "○").replace("〇", "○")).strip()
    names = {}
    for s in skills:
        names.setdefault(fold(s["name"]), []).append(s["id"])
    out, unmatched, seen = [], [], set()
    # Both halves of the tab list the same skills (sorted for Team Trials, then for CM), each with
    # its Team Trials tier and its CM tier; read the first half.
    for r in rows[1:]:
        if len(r) < 8 or not r[1].strip():
            continue
        written = fold(r[1])
        if "[" in written:
            fill = lambda m: "(" + TIER_FILLS.get(m.group(1).replace("\\", ""), ".+?") + ")"
            pattern = re.compile("^" + re.sub(r"\\\[([^\]]+)\\\]", fill, re.escape(written)) + "$")
            # Each match with what fills the template ("Pace Chaser" for "[Run Style] Corners ◯").
            ids = [(i, m.group(1)) for n, v in names.items() if (m := pattern.match(n)) for i in v]
        else:
            ids = [(i, "") for i in names.get(written, [])]
        if not ids:
            unmatched.append(r[1].strip())
            continue
        score = r[4].strip()
        for sid, filled in ids:
            if sid in seen:
                continue
            seen.add(sid)
            target = r[6].strip()
            out.append({
                "id": sid,
                "tt": r[2].strip(),
                "cm": r[3].strip(),
                "score_per_sp": float(score) if re.fullmatch(r"\d+(\.\d+)?", score) else None,
                "for": filled or ("" if target in ("/", "") or target.startswith("[") else target),
                "why": re.sub(r"\s+", " ", r[7]).strip(),
                "as_written": r[1].strip(),
            })
    head = {
        "source": f"https://docs.google.com/spreadsheets/d/{TIER_SHEET}/ (Uma Musume Skills Spreadsheet, Tierlist tab)",
        "authors": "Dylan (@dylank0), formatted by sayaduck (@sayaduck)",
        "sheet_updated": updated,
        "fetched": dt.date.today().isoformat(),
        "caveat": "Community opinion, not game data. Tiers depend on the track (▲ above all), and the sheet predates later skill balance changes such as Global's 2026-07-01 adjustment. Score/SP is the sheet's Team Trials points per skill point.",
        "scale": TIER_SCALE,
        "unmatched": unmatched,
    }
    with open(os.path.join(OUT, "skill_tiers.json"), "w", encoding="utf-8") as f:
        f.write(json.dumps(head, ensure_ascii=False)[:-1] + ',"skills":[\n' + ",\n".join(json.dumps(s, ensure_ascii=False) for s in out) + "\n]}\n")
    print(f"skill tiers: {len(out)} skills (sheet updated {updated}), {len(unmatched)} names not on Global: {', '.join(unmatched)}")


def main():
    if "--only" in sys.argv:
        # Refresh just one part: `--only umalator` (Moomoolator data), `--only cm` (CM schedule), `--only affinity`,
        # `--only meta` (the PvP recommendation sheet) or `--only tiers` (the skill tier list).
        part = sys.argv[sys.argv.index("--only") + 1]
        if part == "umalator":
            write_umalator()
        elif part == "cm":
            write_cm_schedule(fetch(discover_chunks()["uma-data"]))
        elif part == "affinity":
            data = fetch(discover_chunks()["uma-data"])
            write_affinity(data)
            write_sparks(data)
            write_saddles(data)
        elif part == "meta":
            write_pvp_meta()
        elif part == "tiers":
            write_skill_tiers()
        else:
            raise SystemExit(f"unknown part {part}; use umalator, cm, affinity, meta or tiers")
        return
    if "--from" in sys.argv:
        d = sys.argv[sys.argv.index("--from") + 1]
        chunks, sources = {}, {}
        for key in CHUNKS:
            f = next(x for x in os.listdir(d) if x.startswith(key + "."))
            chunks[key] = open(os.path.join(d, f), encoding="utf-8").read()
            sources[key] = f
    else:
        urls = discover_chunks()
        chunks = {k: fetch(u) for k, u in urls.items()}
        sources = {k: u.rsplit("/", 1)[-1] for k, u in urls.items()}
    build(chunks, sources)
    write_cm_schedule(chunks["uma-data"])
    for write in (write_affinity, write_sparks, write_saddles):
        try:
            write(chunks["uma-data"])
        except SystemExit as e:  # the rest of the build doesn't need them
            print(f"couldn't write {write.__name__[6:]}: {e}")
    try:
        write_umalator()
    except Exception as e:  # the knowledge build is still useful without it
        print(f"couldn't refresh Umalator data: {e}")


if __name__ == "__main__":
    main()
