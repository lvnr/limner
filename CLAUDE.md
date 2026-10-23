# Limner: working notes for Claude

Limner is an open-source successor to Quill 2.9 (VR painting and animation). "Limner" is a provisional name. This file is the project's memory: session memory does not travel between machines, so anything a future session needs belongs here or in `docs/`.

Everything in the plan is built. It was built on a Mac with no headset; it now runs on Windows, and one tester has tried it in a Quest 3 over Link (what worked, what did not and what was fixed: `docs/handoff.md`, "The first headset tester"). It has not run on a Quest by itself. The next work is to prove the fixes on real hardware: start with `docs/handoff.md`.

## Read first

1. `docs/handoff.md`: what is built, what has and has not been verified, setting up on Windows, the first session with a headset.
2. `docs/superpowers/specs/2026-10-14-limner-design.md`: the design and the decisions that are settled.
3. `docs/interface-api.md`: how interface code talks to the document, and every command. Read it before touching anything under `godot/`.
4. `docs/xr.md`: the headset layer, its constants and the order to test it in.
5. `docs/quill-differences.md`: where Limner departs from Quill and which numbers are guesses to be measured.
6. The research behind the project (a Quill feature map with sources, and a table of Quill behaviours still to be measured) is private and not in the repository. If `reports/` and `research_notes/` exist in the working folder, read `reports/Quill feature map and tech stack.md`; never commit either folder.
7. `docs/superpowers/plans/2026-10-14-limner-master-plan.md`: the milestones and what each delivered. `docs/roadmap.md`: what could come next.

## Settled, do not reopen

- Godot 4.8 with the built-in Mobile renderer, plus an engine-independent Rust core. No custom renderer.
- Quill's features and habits matter; its exact pixels do not. Where a better design exists, take it and record the difference in `docs/quill-differences.md`.
- Own open file format (`.limn`) with a one-way Quill importer.
- Explain trade-offs to the owner in plain terms before graphics terms.

## Hard rules

- Never add material from a Quill install (icons, sounds, models, fonts, screenshots) to the repository, and never ship any.
- `~/projects/quill-exporter/` is the owner's private artwork and Quill's exporter. Read-only. Never copy it into the repository. Tests that use it must skip when it is missing (`LIMNER_SAMPLE_DIR` overrides the path) and print only totals.
- Do not run, decompile or inspect Quill's binaries (its EULA forbids reverse engineering). Format knowledge comes from user-owned files and from the Apache/MIT-licensed sources under `local/ref/`. Measuring how Quill behaves by using it, as any user would, is fine and wanted.
- `limner-core` has no I/O and no Godot types. `limner-godot` holds no product logic.
- The repository is public at github.com/lvnr/limner (MIT). Before committing, check new files for local paths, private material and anything from the research notes. Commits use the GitHub noreply address: on a fresh clone, run `git config user.name lvnr` and `git config user.email 3449973+lvnr@users.noreply.github.com` in the repository before the first commit, so that no personal address is published.

## Layout

```
crates/limner-core     no I/O, no engine types
  model/               document, layers, keys, drawings (strokes in chunks), media, settings
  eval/                the document at a time: transforms, opacity, clips, frames
  tess/                strokes to triangles, one mesh per chunk
  ops/                 edits to a document: layers, frames, clips, keys, strokes, simplify, lighting
  session/             the editing session: commands, undo, playback, input, tools, grabs, gimbal, queries
crates/limner-format   the native .limn folder and .limnz file
crates/limner-quill    Quill importer
crates/limner-export   glTF, USD/USDZ, OBJ, FBX
crates/limner          facade: files, media decoding; what the CLI and the bridge use
crates/limner-cli      the `limner` command: inspect, convert, export
crates/limner-godot    godot-rust extension: the LimnerScene node
godot/
  main.gd              start-up, command-line options (listed at its top)
  world/               LimnerWorld: scene host, environment, gimbal, tool cursor
  editor/              LimnerEditor (the interface's handle on the session), preferences, autosave, sounds, text
  ui/                  panels and widgets, built in code with Kit and Icons
  desktop/             desktop shell and the mouse-driven hand
  xr/                  headset shell, rig, palette, pointer, keyboard, simulated rig
  capture/             stills, 360, camera renders
  shaders/             stroke, picture, lit, helper shaders
  tests/               run.gd and test_*.gd
scripts/               fetch-godot, godot, test, shot, package (.sh and .ps1)
local/                 git-ignored: reference sources (local/ref), scratch output (local/out)
tools/                 git-ignored: downloaded Godot builds
```

## Commands

macOS and Linux; on Windows use the `.ps1` twin of each script.

```bash
scripts/fetch-godot.sh                 # once per machine
scripts/test.sh                        # Rust tests, then the interface tests
scripts/test.sh timeline               # only interface test files whose name contains "timeline"
cargo test -p limner-core
cargo clippy --workspace --all-targets && cargo fmt --all -- --check
cargo build -p limner-godot            # the Godot project loads target/debug/
scripts/godot.sh --headless --path godot --import                    # after adding a script with class_name, or an asset
scripts/godot.sh --path godot --script res://tests/run.gd -- capture export_panel monitor   # the tests that need a window
scripts/godot.sh --path godot                                         # the app
scripts/godot.sh --path godot -- --xr --sim                           # the headset interface with a simulated headset
scripts/shot.sh local/out/shot.png --demo --frame-all                 # run, save a screenshot, quit; then look at the PNG
scripts/package.sh                                                    # packaged app for this system in local/out/builds/ (--debug for a debug one)
cargo run -p limner-cli -- inspect <project>
```

On Windows the project starts OpenXR whenever the app starts. Put `--xr-mode off` before the `--` to keep the headset's software asleep for desktop work; `test.ps1` and `shot.ps1` already do. In PowerShell the `--` must be quoted, `'--'`: a bare one is dropped before the script sees it.

## Conventions

- Every change to a document is a `Command` (`crates/limner-core/src/session/command.rs`) run through `Session::exec`, so it is undoable, scriptable and testable. A drag or gesture is one transaction and one undo step. New behaviour goes in the core with a test first; the Godot side only presents it.
- Interface code never touches the document: it calls `editor.run({...})`, reads the cached queries on `LimnerEditor`, and redraws on its signals. Panels are built in code with `Kit` and `Icons` (`godot/ui/`).
- Panels follow Quill's rules: a `LimnerPanel` of one fixed size in design pixels (`Kit.u`), nothing that scrolls or opens a list (no `ScrollContainer`, `OptionButton`, `ItemList`, `Tree`), keys first; long lists page with `Pager`, choices use `KeyGrid` or `Cycler`. Every new state of a panel gets a `check_fits` in `test_panels_fit.gd`. See rule 8 in `docs/interface-api.md`.
- GDScript is fully typed. A variable whose type would be inferred from a Variant is an error in this project; write the type.
- A new script with `class_name` is not seen by tests until `--import` has run once.
- A new document's layers have ids 1 (root), 2 (spawn area), 4 (paint layer); 3 is the paint layer's drawing. The next layer added is 5.
- Rust: `cargo fmt`, `cargo clippy --workspace --all-targets` clean. Unit tests beside the code, cross-crate tests in `tests/`. glam's debug assertions are on: normalise vectors before `from_rotation_arc` and the like.
- Anything a person can see gets looked at: take a screenshot with `scripts/shot.sh` and read the PNG. Screenshot runs need a window (`--headless` has no renderer).
- When behaviour changes, change the page that describes it in the same commit: `docs/interface-api.md` for commands and queries, `docs/quill-differences.md` for anything a Quill user would notice, `docs/format.md` for the file format.
- Commit after each task. Trailer: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Shell: prefer absolute paths; do not `cd` into subdirectories in long-lived shells.

## Environment facts

- Godot is pinned in `scripts/godot-version.txt` (4.8 dev 7). The extension is built at the 4.7 API level, so Godot 4.7.2 is a fallback.
- The first headless `--import` on a fresh checkout may print a crash trace at exit in dev 7; a second run is clean.
- The Mac this was built on was short of disk: release builds of the Rust code were never made there, and its git-ignored `.cargo/config.toml` turns incremental builds off. Neither limit applies elsewhere.
- That Mac has no headset and no OpenXR runtime. Headset code has only been exercised through the simulated rig.
