# Limner

Limner is an open-source tool for painting and animating in 3D, in a VR headset or on a desktop screen. It is a successor to Quill 2.9, which has had no updates since 2021: it keeps Quill's way of working, opens existing Quill projects, and adds things Quill never had, such as lights that shade the paint, mirrored painting, text, smooth keys and export to current 3D formats.

"Limner" is a working title and may change.

**Status.** Every part of the plan is built and tested on a Mac without a headset. Nothing has yet run in a headset, on Windows or on a Quest. `docs/handoff.md` says exactly what has and has not been checked.

![The desktop interface with the demo strokes](docs/images/desktop.png)

## What it does

- **Paint** with nine tools (Paint, Line, Colorize, Eraser, Selection, Grab, Thicken/Thin, Nudge, Optimize) and seven brushes, with pressure, tapers and 14 colour modes.
- **Animate** frame by frame, with keys, by recording a hand's motion, or by painting while the piece plays; with onion skins, clips, looping sequences and a timeline.
- **Layers** of paint, groups, sequences, pictures and 360 pictures, models, sounds, spawn areas, cameras and lights.
- **In a headset** the controls follow Quill's: trigger to use the tool, grips to move the world, a palette in the other hand. **On a desktop** the same tools and panels work with a mouse or a pen.
- **Files.** Its own open format (`.limn` folders, `.limnz` single files), opening Quill projects directly, and export to glTF, USD/USDZ, FBX and OBJ. Stills, 360 stills and camera renders to video.

## Build and run

You need Rust (stable) and the pinned Godot build, which a script downloads.

```bash
scripts/fetch-godot.sh            # once per machine; Windows: scripts\fetch-godot.ps1
cargo build -p limner-godot       # the extension the Godot project loads
scripts/godot.sh --path godot     # the app; Windows: scripts\godot.ps1 --path godot
```

With a headset connected through OpenXR the app starts in it; add `-- --desktop` to stay on the desktop, or `-- --xr --sim` to try the headset interface with a simulated headset and hands.

```bash
scripts/test.sh                   # every test; Windows: scripts\test.ps1
scripts/package.sh                # a packaged app in local/out/builds/; Windows: scripts\package.ps1
cargo run -p limner-cli -- inspect <project>
cargo run -p limner-cli -- export <project> out.glb
```

## How it is built

A Rust core that knows nothing of any engine holds the document, the timeline, the tools and undo. Every edit is a small command, so the desktop interface, the headset interface, the command line and the tests all drive the same thing. A Godot 4.8 project draws the result and provides the interface, the headset layer, sound and capture.

| Read | For |
|---|---|
| `docs/handoff.md` | what is verified, moving to another machine, the first headset session |
| `docs/superpowers/specs/2026-10-14-limner-design.md` | the design and its reasons |
| `docs/quill-differences.md` | where it differs from Quill |
| `docs/format.md`, `docs/export.md`, `docs/capture.md`, `docs/quill-import.md`, `docs/xr.md` | each area in detail |
| `docs/interface-api.md` | writing interface code; every command |
| `docs/roadmap.md` | what could come next |

## Relationship to Quill

Limner is an independent project, not affiliated with or endorsed by Smoothstep or Meta. It reads Quill project folders so that existing work stays usable, and writes its own open format. It contains no code or assets from Quill.

## Licence

Apache-2.0. See `LICENSE`.
