# Limner

Limner is an open-source tool for painting and animating in 3D, on a desktop screen or in a VR headset. It is a successor to Quill 2.9, which has had no updates since 2021: it keeps Quill's way of working, opens existing Quill projects, and adds features Quill never had, starting with scene lights.

"Limner" is a working title and may change.

Status: early development. See `docs/superpowers/plans/2026-10-14-limner-master-plan.md` for progress.

## Build and run

Requirements: Rust (stable, 1.94 or newer) and the pinned Godot build.

```bash
scripts/fetch-godot.sh            # downloads Godot into tools/ (Windows: scripts\fetch-godot.ps1)
cargo build -p limner-godot       # builds the Rust extension
scripts/godot.sh --path godot     # runs the app
```

Tests:

```bash
cargo test --workspace
scripts/godot.sh --headless --path godot -s res://tests/run_tests.gd
```

## How it is built

An engine-independent Rust core holds the document, timeline, tools and undo. A Godot 4.8 project renders it and provides the interface, XR, audio and capture. The design is in `docs/superpowers/specs/2026-10-14-limner-design.md`.

## Relationship to Quill

Limner is an independent project. It is not affiliated with or endorsed by Smoothstep or Meta. It reads Quill project folders so that existing work stays usable, and it writes its own open format.

## Licence

Apache-2.0. See `LICENSE`.
