# Limner: working notes for Claude

Limner is an open-source successor to Quill 2.9 (VR painting and animation). "Limner" is a provisional name. This file is the project's memory: session memory does not travel between machines, so anything a future session needs belongs here or in `docs/`.

## Read first

1. `docs/superpowers/specs/2026-10-14-limner-design.md`: the design and the decisions that are settled.
2. `docs/superpowers/plans/2026-10-14-limner-master-plan.md`: milestones, tasks and progress (ticked boxes are done and committed).
3. `reports/Quill feature map and tech stack.md`: what Quill does, with evidence grades and a table of unknowns to measure.
4. `docs/handoff.md`: what has and has not been verified, and what to test on real hardware.

## Settled, do not reopen

- Godot 4.8 with the built-in Mobile renderer, plus an engine-independent Rust core. No custom renderer.
- Quill's features and habits matter; its exact pixels do not. Where a better design exists, take it and record the difference in `docs/quill-differences.md`.
- Own open file format (`.limn`) with a one-way Quill importer.
- Explain trade-offs to the owner in plain terms before graphics terms.

## Hard rules

- `3d models&design/` is proprietary material from a Quill install. Reference only. Never commit, copy or ship anything from it.
- `~/projects/quill-exporter/` is the owner's private artwork and Quill's exporter. Read-only. Never copy it into the repository. Tests that use it must skip when it is missing.
- Do not run, decompile or inspect Quill's binaries (its EULA forbids reverse engineering). Format knowledge comes from user-owned files and from the Apache/MIT-licensed sources under `local/ref/`.
- `limner-core` has no I/O and no Godot types. `limner-godot` holds no product logic.

## Layout

```
crates/limner-core     model, timeline, tessellation, tools, commands, undo, interaction
crates/limner-format   native .limn format
crates/limner-quill    Quill importer
crates/limner-export   glTF, USD, OBJ, FBX
crates/limner          facade used by the CLI and the bridge
crates/limner-cli      `limner` command-line tool
crates/limner-godot    godot-rust extension
godot/                 Godot project (GDScript shell, shaders, UI, tests)
local/                 git-ignored: reference sources (local/ref), samples, scratch output (local/out)
tools/                 git-ignored: downloaded Godot builds
```

## Commands

```bash
scripts/fetch-godot.sh                 # once per machine (Windows: scripts\fetch-godot.ps1)
cargo test --workspace
cargo build -p limner-godot            # the Godot project loads target/debug/liblimner_godot.*
scripts/godot.sh --headless --path godot --import                      # after adding assets
scripts/godot.sh --headless --path godot -s res://tests/run_tests.gd   # Godot-side tests
scripts/godot.sh --path godot                                          # run the app
cargo run -p limner-cli -- inspect <project>
```

Screenshots for visual checks are written to `local/out/` by scripts under `godot/tests/shots/`; run them windowed (not `--headless`, which has no renderer) and look at the PNG.

## Conventions

- Rust: `cargo fmt`, `cargo clippy --workspace` clean. Tests live beside the code (`#[cfg(test)]`) for units and in `tests/` for cross-crate behaviour.
- GDScript: typed where practical, tabs, one scene with one script.
- Every edit to a document goes through a core command so it is undoable and scriptable.
- Commit after each task. Trailer: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Shell: prefer absolute paths; do not `cd` into subdirectories in long-lived shells.

## Environment facts

- Godot is pinned in `scripts/godot-version.txt` (4.8 dev 7 when this was written). The extension is built at the 4.7 API level, so Godot 4.7.2 is a fallback.
- The first headless `--import` on a fresh checkout may print a crash trace at exit in dev 7; a second run is clean.
- The development Mac has no headset and no OpenXR runtime. Headset code is exercised through the simulated rig only; see `docs/handoff.md`.
