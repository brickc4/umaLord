<p align="center">
  <img src="docs/readme/hero.png" alt="umaLord: your Uma Musume (Global) companion. Plan careers on the community meta, race them in a real simulator, and keep your whole collection at hand. Free and open source, for Windows 10 and 11, runs offline." width="100%">
</p>

<p align="center">
  <a href="https://github.com/brickc4/umaLord/releases/latest"><img src="https://img.shields.io/github/v/release/brickc4/umaLord?style=for-the-badge&label=download&logo=windows&logoColor=white&color=60a5fa&labelColor=1e3454" alt="Download the latest release"></a>
  <a href="https://github.com/brickc4/umaLord/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/brickc4/umaLord/ci.yml?branch=main&style=for-the-badge&label=tests&logo=githubactions&logoColor=white&labelColor=1e3454" alt="Build and tests"></a>
  <img src="https://img.shields.io/badge/rust-egui-b7410e?style=for-the-badge&logo=rust&logoColor=white&labelColor=1e3454" alt="Built with Rust and egui">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-a78bfa?style=for-the-badge&labelColor=1e3454" alt="GPL-3.0-or-later"></a>
</p>

<h3 align="center">Plan it. Race it. Log what happened.</h3>

<p align="center">
  A desktop companion for your <b>Uma Musume: Pretty Derby</b> account on the Global server. It knows your umas and cards at their real limit breaks, reads the community meta for every Champions Meeting, and races your builds in a port of Moomoolator's engine before you spend a single career on them.
</p>

<br>

<table>
  <tr>
    <td width="50%"><img src="docs/readme/home.png" alt="Home: the next Champions Meeting at a glance, live from the game's news, with the course drawn, what the meta recommends and your best umas for it."></td>
    <td width="50%"><img src="docs/readme/plan.png" alt="Plan: careers built on the community meta. Pick a CM and a trainee; decks start from the PvP meta sheet and simulated careers rank the rest."></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/readme/autopick.png" alt="Auto-pick: skills that win together. It races every skill on offer, keeps the set that gains the most, and checks it in 1,000 races."></td>
    <td width="50%"><img src="docs/readme/race.png" alt="Race sim: Moomoolator's engine in Rust. Race two builds 1,000 times and see where the lengths are won, speed and HP, and what each skill is worth."></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/readme/collection.png" alt="Collection: everything you own at your level, with effects at your limit break, the skills each card teaches, and every skill that can trigger on a course."></td>
    <td width="50%"><img src="docs/readme/ask.png" alt="Ask: an assistant that works the app. It plans, picks and races a career in plain English, and asks before it saves. Free models built in."></td>
  </tr>
</table>

<details>
<summary><b>…and everything else it does</b></summary>

<br>

- 📅 **Live Champions Meeting schedule** from the game's official news and Umalator's presets: announced or estimated, countdowns in your time zone, and the skills each CM turns off (auto-pick never buys those).
- 🛒 **Skill shop mode**: at the end of a real career, enter your final stats and SP and tick what's on offer; auto-pick works on exactly that.
- 🎯 **Three aims for skills**: win the CM race in lengths, Team Trials (gains weighted by how often they trigger), or rating points for a parent's spark stars.
- ⚠️ **Aptitude warnings**: a grade below B for the goal shows what it costs her, from the simulator's own coefficients.
- 📈 **Race sim charts**: the gap of every race, where along the course the lengths are won, speed and HP of the typical race with a replay, a timeline of every skill, what each skill is worth and where the next training points matter.
- 🃏 **Compare two cards** side by side at your levels, filter cards by a skill they teach or an effect's minimum, and see the community tier list on every skill.
- 🧩 **Setup in minutes**: click what you own on art grids, paste a list, or load a profile file. ⚙ Settings → Save my data puts your whole account in one file.
- 🌱 **Your finished umas as parents**: import UmaExtractor's data.json (or run `tools/read_veterans.py` on Linux) and every finished uma lands with its sparks, parents and G1 wins; affinity for any lineage comes from the game's own tables.
- ⌨️ **Keyboard shortcuts**: Ctrl+1–4 for the tabs, Ctrl+F to search, Ctrl+S to save the plan, F5 to reload, ? for help, + for a pulled copy.
- 🤖 **MCP server**: the assistant's tools work in Claude Code and Claude Desktop too ([below](#-use-it-with-claude-code)).

</details>

<br>

<p align="center">
  <img src="docs/readme/themes.png" alt="Make it yours: six themes, dark or light. Seiun Sky (dark), Sakura (light), Sunset (dark), Starry night (dark), Track day (light) and Ink (high contrast)." width="100%">
</p>

<p align="center">
  <a href="https://github.com/brickc4/umaLord/releases/latest"><img src="docs/readme/start.png" alt="Get started, up and running in a minute: 1. Download the zip from Releases and unzip it anywhere. 2. Run umalord.exe; Windows may ask first, so click More info, then Run anyway. 3. Load a profile file, or click the umas and cards you own." width="100%"></a>
</p>

<p align="center">
  <a href="https://github.com/brickc4/umaLord/releases/latest"><b>⬇️ Download the latest release</b></a>
  &nbsp;·&nbsp;
  <a href="CHANGELOG.md">What's new</a>
  &nbsp;·&nbsp;
  <a href="https://github.com/brickc4/umaLord/issues/new/choose">Report a bug or suggest an idea</a>
</p>

<p align="center"><sub>Windows 10 or 11, 64-bit. Your data stays on your computer, in the <code>account</code> folder next to the app.<br>Every release is built and tested by <a href="https://github.com/brickc4/umaLord/actions">GitHub Actions</a> from the tagged source.</sub></p>

## 🧠 How the picks work

Every pick is labelled for what it is: game data, simulator output, community meta, or opinion.

- **Meta first for cards.** For a plan aimed at a Champions Meeting, each card gets a rating (★ to ★★★) from how many of the community sheet's decks for that CM and running style include it: Whale, Standard and Beginner decks, and honorable mentions. A copy at a low limit break counts for less.
- **Races for skills.** Auto-pick races each skill on offer in the simulator, then builds the set step by step, adding whichever skill gains the most on top of those already chosen per skill point, and checks the final set in 1,000 races. The sheet's skill advice breaks ties by default; in Step 5 you can make it count more, or not at all.
- **Simulation for the rest.** Among cards with the same rating, and for anything the sheet doesn't cover, each card is scored by what it adds to simulated careers in the CM's scenario (stats toward your targets, skill points, scenario links), plus the lengths its skills gain on the course in the race simulator.
- **Suggestions, not a solved optimum.** The career model's assumptions are listed, each marked sourced or assumed, in [`knowledge/scenarios.json`](knowledge/scenarios.json). The race engine is checked against umalator.app on every test run.

## 🤖 Use it with Claude Code

Open the folder in Claude Code. [`CLAUDE.md`](CLAUDE.md) tells Claude how to keep your roster up to date from screenshots, simulate saved plans, log real results, and keep evidence-backed opinions in `insights.md`.

`umalord.exe --mcp <folder>` serves the same tools the Ask tab uses (your roster, the game data, the meta sheet, the CM schedule, the planner and the race simulator) as an MCP server over stdio. Point it at your umaLord folder.

**Claude Code:**

```
claude mcp add umalord -- C:\path\to\umaLord\umalord.exe --mcp C:\path\to\umaLord
```

**Claude Desktop**, in `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "umalord": { "command": "C:\\path\\to\\umaLord\\umalord.exe", "args": ["--mcp", "C:\\path\\to\\umaLord"] }
  }
}
```

Without the app's window, the tools that write files (`save_plan`, `log_result`, `update_owned`) run when called, so leave your client's per-tool approval on. Plans it saves appear in the app's Planner under Saved plans.

## 🛠️ Build from source

<details>
<summary>Toolchain, tests, game data and releases</summary>

<br>

Needs [Rust](https://rustup.rs) with the `stable-x86_64-pc-windows-gnu` toolchain and its `llvm-tools` component.

```powershell
cd app
powershell -ExecutionPolicy Bypass -File setup-toolchain.ps1
cargo build --release
```

The exe is `app/target/release/umalord.exe`. `cargo test --release` runs the tests, including a check of the race engine against umalator.app.

**Linux (Wayland) and macOS:** build it the same way with the standard Rust toolchain (`cargo build --release` in `app/`, then `cargo run --release`). Linux needs a Wayland session and `curl`, `xdg-open` and fontconfig; the app bundles its symbol fonts, and a Japanese font (such as Noto Sans CJK JP) shows event text. Settings live in `~/.config/umaLord` on Linux and `~/Library/Application Support/umaLord` on macOS. The Ask tab's free backend uses `opencode` from your PATH there, since the in-app download is the Windows build. CI builds and tests all three; macOS hasn't been tried on a real Mac yet.

**Nix (Linux):** `nix run github:benashby/umaLord` runs it, and the flake's `packages.default` installs it with a desktop entry. The package keeps its data in `~/.local/share/umaLord` (`$UMALORD_ROOT` overrides it), refreshing the game data there when the package changes and leaving your account alone. `nix develop` (or `direnv allow` with a `use flake` `.envrc`) gives a dev shell with the toolchain, the Wayland and OpenGL libraries, a Japanese font, Python and `opencode`.

Game data lives in `knowledge/data/`. To refresh it from uma.guide, umalator.app, the meta sheet and the skill tier list (needs Python 3):

```powershell
python -I -X utf8 tools/update_knowledge.py
```

Add `--only meta` to refresh just the meta sheet. `tools/package.ps1 -Build` makes the release zip locally; it never includes your account. `tools/readme_art.ps1` redraws the art on this page from fresh screenshots.

**Releasing:** add the version's section to [CHANGELOG.md](CHANGELOG.md), bump `version` in `app/Cargo.toml`, then push a tag:

```powershell
git tag v0.2.1
git push origin v0.2.1
```

The [Release workflow](.github/workflows/release.yml) builds, tests and packages it, and publishes the release with that changelog section as its notes.

</details>

## 🐛 Bugs and ideas

[Open an issue](https://github.com/brickc4/umaLord/issues/new/choose). There's a short form for bug reports and one for ideas. Issues are public, so don't attach your profile file unless you're happy to share it.

## 💙 Credits

- **Game data:** [uma.guide](https://uma.guide): umas, cards, skills, races and events.
- **Race simulator:** a port of [Moomoolator](https://umalator.app) ([TheCing/uma-tools](https://github.com/TheCing/uma-tools)), which builds on alpha123's Umalator and kachi-dev's VFalator.
- **Meta picks:** the community "PvP Deck & Uma Recommendation" sheet by Lightning, Daptrius and Henry.
- **Skill tiers:** the community "Uma Musume Skills Spreadsheet" tier list by Dylan, formatted by sayaduck.
- **Guides:** GameTora, uma.guide and other community guides, each cited in [`knowledge/mechanics.md`](knowledge/mechanics.md).
- **Chibi art:** [GameTora](https://gametora.com).

**License:** [GPL-3.0-or-later](LICENSE), because the race simulator ports Moomoolator, which is GPL-3.0-or-later.

<p align="center">
  <img src="app/assets/chibi/seiun_102002_0010.png" width="110" alt="Chibi Seiun Sky">
</p>

<p align="center"><sub>Uma Musume: Pretty Derby is © Cygames, Inc. umaLord is an unofficial fan project, not affiliated with or endorsed by Cygames.</sub></p>
