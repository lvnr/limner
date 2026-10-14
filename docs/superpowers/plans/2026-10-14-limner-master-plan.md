# Limner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build Limner, an open-source VR painting and animation tool that succeeds Quill 2.9, runs on the desktop and in headsets, opens Quill projects and adds scene lights.

**Architecture:** An engine-independent Rust core (document, timeline, tessellation, tools, commands, undo, interaction) behind a godot-rust bridge, with a Godot 4.8 project for rendering, UI, XR, audio and capture. Every edit is a serialisable command; all controller behaviour consumes an `InputFrame`.

**Tech Stack:** Rust stable (1.94+), `godot` 0.5.5 (`api-4-7`, `lazy-function-tables`), Godot 4.8 dev 7 Mobile renderer, `glam`, `serde`, `zstd`, `blake3`, `zip`.

**Design:** `docs/superpowers/specs/2026-10-14-limner-design.md`. Read it before any task.

**How this plan is used.** The project owner asked for unattended, end-to-end development, so this is a living plan and progress tracker, not a hand-off script: tasks state files, interfaces and the tests that prove them, and the code goes straight into the repository. Architecture-critical tasks are done inline; bounded tasks are given to subagents with a self-contained brief. Tick a task only when its tests pass and it is committed.

**Where it stands.** Every milestone is built and ticked except the two items marked open in M8 and M11, which need downloads or machines the development Mac did not have. A tick means built, tested and committed on that Mac; it does not mean tried in a headset or on Windows. `docs/handoff.md` says what has and has not been verified.

## Global Constraints

- Never commit, copy or ship anything from a Quill install (proprietary assets).
- `~/projects/quill-exporter/` is private, read-only test data; tests using it must skip when it is absent (`LIMNER_SAMPLE_DIR` overrides the path).
- Do not run, decompile or inspect Quill's binaries.
- `limner-core` has no I/O and no Godot types. `limner-godot` has no product logic.
- Time is integer flicks (705,600,000 per second). One Quill tick is 56,000 flicks.
- Hierarchy maths in `f64`; stroke points `f32` in layer space.
- Godot build is pinned: `4.8.dev7.official.c971f93e7`. Extension API level is 4.7.
- Licence MIT (Apache-2.0 until the repository went public). New third-party assets need a compatible licence and an entry in `THIRD_PARTY.md`.
- Commit after each task with the trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Commands

```bash
scripts/test.sh                             # Rust tests, then the interface tests (Windows: scripts\test.ps1)
cargo test --workspace                      # all Rust tests
cargo build -p limner-godot                 # build the extension (debug)
scripts/godot.sh --headless --path godot --script res://tests/run.gd                         # interface tests
scripts/godot.sh --path godot --script res://tests/run.gd -- capture export_panel monitor    # those that need a window
scripts/shot.sh <out.png> [app options]     # windowed run that saves a screenshot
cargo run -p limner-cli -- inspect <project>
```

## File structure

As built. `CLAUDE.md` has the same map with a line on each part.

```
crates/limner-core/src/
  time.rs, dither.rs, json.rs
  math/{xform,bounds}.rs                Xform (similarity + flip), Aabb
  model/{ids,document,layer,keys,drawing,stroke,media,settings}.rs
  eval/mod.rs                           the document at a time -> RenderFrame
  tess/{frame,brush,chunk}.rs           strokes -> ChunkMesh
  ops/{layers,frames,clips,keys,strokes,color,simplify,lighting}.rs
  session/{mod,command,history,playback,input,options,props,query,selection,strokes,media,
           animation,gesture,grab,gimbal}.rs
  session/tools/{settings,space,paint,stroke_builder,sphere,select}.rs
crates/limner-format/src/{manifest,schema,blob,hash,store,save,load,pack,container,media,mesh,wav,state}.rs
crates/limner-quill/src/{json,qbin,convert,state,mesh,sound}.rs
crates/limner-export/src/{scene,common,gltf,usd,obj,fbx}.rs
crates/limner/src/{lib,files,media}.rs
crates/limner-cli/src/{main,inspect,dump,export}.rs
crates/limner-godot/src/{lib,convert,meshes}.rs, scene/{mod,nodes,strokes,pictures,models,sounds,lights,gizmos,overlays,import,export}.rs
godot/
  main.gd                               start-up and command-line options
  world/{world,gimbal,tool_cursor}.gd
  editor/{editor,preferences,autosave,sounds,text_strokes}.gd
  ui/*.gd                               panels and widgets, built in code
  desktop/{shell,hand}.gd
  xr/{xr_shell,rig,openxr_rig,sim_rig,sim_director,palette,panel_surface,pointer,keyboard,button_map,...}.gd
  capture/{capture,capture_rig,camera_view,render_job,render_launcher}.gd
  shaders/, tests/{run.gd,test_*.gd}
docs/{handoff,roadmap,quill-differences,interface-api,format,quill-import,export,capture,xr}.md
scripts/{fetch-godot,godot,test,shot}.{sh,ps1}
```

---

## M0: Foundations

- [x] **0.1 Workspace and core scaffolding.** Crates `limner-core`, `limner-quill`, `limner`, `limner-cli` created; `time.rs`, `model/ids.rs`. Test: `cargo test -p limner-core time` (flick conversions exact for 12, 24, 25, 30, 60, 90 fps and Quill ticks).
- [x] **0.2 `Xform`.** `math/xform.rs` with `IDENTITY`, `transform_point`, `transform_vector`, `to_affine`, `from_affine`, `mul`, `inverse`, `interpolate`, `interpolate_about_pivot`. Tests: random transforms with every flip agree with `DAffine3` composition and inversion to 1e-9; `interpolate` takes the shortest arc; flip comes from the earlier key.
- [x] **0.3 Document model.** `model/*`: `Document`, `Layer`, `LayerKind`, `LayerAnim`, `Key<T>`, `Interp`, `Drawing` rope with `push_stroke`, `stroke_mut`, `retain_strokes`, `strokes()`, chunk splitting at 60,000 expanded vertices, `Stroke`, `Point` (56 bytes, `bytemuck::Pod`), `Brush`, bounds. Tests: rope invariants; clone is shallow (`Arc::ptr_eq`); editing one stroke leaves other chunks pointer-equal.
- [x] **0.4 Quill importer.** `limner-quill`: `import_project(dir) -> Result<Imported, ImportError>` with `Imported { document, state, warnings }`. Covers Group, Sequence, Paint, Viewpoint, Camera layers, all key channels, string or numeric times, hex offsets, shared drawings, 16-float transforms, single-drawing paint layers. Picture, Model and Sound layers are imported as layers with raw media records in this task and decoded in M2. Tests: a synthetic project written by test support code; local test on the sample: 94 layers (37 Group, 55 Paint, 1 Viewpoint, 1 Camera), 70 drawings, 2,198 strokes, 82,524 points.
- [x] **0.5 CLI inspect.** `limner inspect <dir> [--json]` prints the layer tree, counts and warnings. Test: snapshot of the synthetic project's output.
- [x] **0.6 Hierarchy parity.** `Document::world_from_layer(id) -> DAffine3`. Local test: baked points of every paint layer of the sample match Quill's Alembic export within 1e-5 (`scripts/verify/alembic_points.py` extracts the reference).

## M1: Desktop viewer

- [x] **1.1 Tessellation.** `tess`: `tessellate_chunk(&StrokeChunk, ...) -> ChunkMesh` with four surfaces, chunk-centre-relative positions, normals, sRGB8 colour + opacity, `CUSTOM0` direction + seed. Tests: vertex counts 2/7/7/4 per point; ring radii and ellipse axes; zero-width ends; local parity with Alembic ring geometry.
- [x] **1.2 Evaluation skeleton.** `eval`: `evaluate(&Document, time, &view) -> RenderFrame` with static visibility, opacity and transforms (animation arrives in M2).
- [x] **1.3 Bridge.** `LimnerScene` node: `import_quill(path)`, `set_view/get_view`, per-frame reconcile of layer nodes and chunk meshes from the `RenderFrame`; mesh cache keyed by chunk pointer.
- [x] **1.4 Stroke shaders.** Opaque and see-through variants, cull back and cull disabled, blue-noise dither texture generated by `scripts/gen_blue_noise.py`, per-instance layer opacity and seed, directional opacity.
- [x] **1.5 Desktop shell.** Window, environment (background colour from the document), orbit/pan/zoom navigation that edits the view, open dialog, status bar. Screenshot test of the sample from three views.

## M2: Player

- [x] **2.1 Timeline evaluation.** Clips, offsets, sequences with loop points, paint frames with repeat rules, key interpolation about the pivot, action keys. Unit tests for each rule with hand-computed cases.
- [x] **2.2 Playback.** `session/playback.rs`: play, pause, seek, step, working range, loop, stops and chapter marks; a frame-accurate clock in flicks.
- [x] **2.3 Pictures, models, sounds from Quill.** qbin records decoded; media table; Godot nodes for flat, 360 and cubemap pictures, model meshes, positional and flat sounds with attenuation and cone/frustum modifiers. Ambisonic sound plays its first channels only.
- [x] **2.4 Spawn areas and cameras.** Gizmos, jump to a spawn area, camera preview in the Monitor.
- [x] **2.5 Transport UI.** Play controls, scrubbing, timecode, frame rate (in the Timeline panel).

## M3: Native format

- [x] **3.1 Blob and manifest.** `.lstk` stroke blobs (zstd, named by BLAKE3 hash), `limn.json`. Round-trip tests.
- [x] **3.2 Project store.** Crash-safe save, writes only what changed, garbage collection, media in their original encoding, `state.json`, thumbnail.
- [x] **3.3 Pack/unpack** `.limnz`. **3.4 CLI convert** Quill to `.limn`. **3.5 Open/save/autosave** in the app. **3.6 `docs/format.md`.**

## M4: Editing core and desktop shell

- [x] **4.1 Commands and history.** `Command` enum (serde), `Session::exec`, snapshot undo (200 steps), transactions, revision counters.
- [x] **4.2 Layer operations.** Add, delete, move, duplicate, rename, visibility, lock, opacity, merge down, flatten, group/sequence conversion, flip, reset transform, pivot operations.
- [x] **4.3 Bridge command and query API.** `exec(Dictionary)`, typed queries, signals on revision change. Documented in `docs/interface-api.md`.
- [x] **4.4 Shell UI.** Theme, layout, Layers, Transform, Layer properties, Document and Settings panels.
- [x] **4.5 Selection and transforms.** Selection set and highlight, gimbal with axis, plane and ring handles, pivot editing, rotation snapping, duplicate-and-repeat.

## M5: Painting

- [x] **5.1 Input and interaction.** `InputFrame`, gesture state machine, navigation (grab, two-hand scale, rapid zoom), stick shortcuts, button actions.
- [x] **5.2 Paint and Line.** Raw sampling, pressure to width and opacity, tapers, caps, seven brushes, animated-brush duration, axis snap, origin grab.
- [x] **5.3 Colorize** (14 modes) and Eraser. **5.4 Selection tool, Grab, Thicken/Thin, Nudge.** **5.5 Optimize** (attribute-aware simplification, tool and layer/document commands).
- [x] **5.6 Desktop simulated hand, cursor and brush preview. 5.7 Tools and Colour panels.**

## M6: Animation authoring

- [x] **6.1 Frame commands** (new, hold, duplicate, delete, remove key, double, reverse, loop, copy selection to frames). **6.2 Clips and sequences** (split, trim, move, loop point, bake). **6.3 Keys** (add, delete, move, interpolation, copy/paste, reverse, keys in children, recording). **6.4 Onion skin.** **6.5 Timeline panel.** **6.6 Painting and tools during playback.**

## M7: Lights

- [x] **7.1 Light layers** in the model, format and commands. **7.2 Lit stroke shaders** and Godot light nodes, with an ambient light setting. **7.3 Bake lights to vertex colour** (no shadows in the bake). **7.4 Light settings in the Layer panel, and gizmos.**

## M8: Headset

- [x] **8.1 XR rig and action map. 8.2 Palette, panel viewports and poke input. 8.3 Simulated rig and scripted gesture tests. 8.4 Haptics and UI sounds.** All of it exercised only on the simulated rig; see `docs/xr.md`.
- [ ] **8.5 Export presets.** Presets for Windows, Linux, macOS and Quest are written (`godot/export_presets.cfg`). The Android build of the extension and a Quest export were not made: they need the Android SDK and NDK and the OpenXR vendors plugin. Steps in `docs/xr.md`.

## M9: Export and capture

- [x] **9.1 Export scene builder** (baked meshes, sampled animation). **9.2 glTF. 9.3 USD/USDZ. 9.4 OBJ. 9.5 FBX. 9.6 Stills and 360. 9.7 Camera render to video. 9.8 Export panel and CLI.** Exports are checked by reading them back, not yet in Blender, Unity or Unreal.

## M10: Beyond Quill

- [x] **10.1 Smooth key interpolation. 10.2 Symmetry painting** (seven of the nine tools). **10.3 Text**, written in strokes rather than as a layer kind. **10.4 Performance panel and budgets. 10.5 Persistent palettes and preferences. 10.6 Autosave and recovery.** Also: importing another project as a group.

## M11: Hand-off

- [x] **11.1 Packaged builds.** `scripts/package.sh` and `scripts\package.ps1` fetch Godot's export templates, build the extension and export with the preset for the system they run on. The macOS package was made, debug and release, and the packaged app was run. Packages for Windows, Linux and Quest have not been made; they need those systems (and for Quest the Android tools).
- [x] **11.2 `docs/handoff.md`, `docs/quill-differences.md`, `docs/roadmap.md`, README, `CLAUDE.md`.**
- [x] **11.3 Full verification pass and final report.** Numbers below.

## State at hand-over

Measured on the development Mac (M1 Max, Godot 4.8 dev 7) with everything above committed.

| Check | Result |
|---|---|
| `cargo test --workspace` | 517 passed, 0 failed, 3 ignored; 232 of them are in `limner-core` |
| The three ignored tests | Two run Apple's USD tools on exported files and pass when asked for with `-- --ignored`; the third writes sample files for outside tools |
| `cargo clippy --workspace --all-targets`, `cargo fmt --all -- --check` | Clean |
| Interface tests, headless (`scripts/test.sh`) | 177 passed, 0 failed |
| Interface tests that need a window (`-- capture export_panel monitor`) | 26 passed, 0 failed |
| `scripts/package.sh` | A 74 MB macOS package. The packaged app starts, loads the extension, draws, and runs the simulated headset interface |
| Frame time on the owner's sample project (53 layers, 948,032 triangles) at 1440 x 900, V-Sync off | 3.2 ms in the packaged release build, 3.3 ms in the development build |

About 53,000 lines of Rust and 19,000 of GDScript.

Not covered by any of this: a headset, Windows, Linux, a Quest, and other 3D programs opening the exports. `docs/handoff.md` has the list.

## Self-review notes

Every section of the design maps to a milestone: sections 6 to 7 to M0 to M2, section 8 to M1 and M7, section 9 to M5, section 10 to M4 to M6, section 11 to M8, section 12 to M3, section 13 to M0, M3 and M9.
