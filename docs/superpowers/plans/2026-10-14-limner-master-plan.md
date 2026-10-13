# Limner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build Limner, an open-source VR painting and animation tool that succeeds Quill 2.9, runs on the desktop and in headsets, opens Quill projects and adds scene lights.

**Architecture:** An engine-independent Rust core (document, timeline, tessellation, tools, commands, undo, interaction) behind a godot-rust bridge, with a Godot 4.8 project for rendering, UI, XR, audio and capture. Every edit is a serialisable command; all controller behaviour consumes an `InputFrame`.

**Tech Stack:** Rust stable (1.94+), `godot` 0.5.5 (`api-4-7`, `lazy-function-tables`), Godot 4.8 dev 7 Mobile renderer, `glam`, `serde`, `zstd`, `blake3`, `zip`.

**Design:** `docs/superpowers/specs/2026-10-14-limner-design.md`. Read it before any task.

**How this plan is used.** The project owner asked for unattended, end-to-end development, so this is a living plan and progress tracker, not a hand-off script: tasks state files, interfaces and the tests that prove them, and the code goes straight into the repository. Architecture-critical tasks are done inline; bounded tasks are given to subagents with a self-contained brief. Tick a task only when its tests pass and it is committed.

## Global Constraints

- Never commit, copy or ship anything from `3d models&design/` (proprietary Quill assets).
- `~/projects/quill-exporter/` is private, read-only test data; tests using it must skip when it is absent (`LIMNER_SAMPLE_DIR` overrides the path).
- Do not run, decompile or inspect Quill's binaries.
- `limner-core` has no I/O and no Godot types. `limner-godot` has no product logic.
- Time is integer flicks (705,600,000 per second). One Quill tick is 56,000 flicks.
- Hierarchy maths in `f64`; stroke points `f32` in layer space.
- Godot build is pinned: `4.8.dev7.official.c971f93e7`. Extension API level is 4.7.
- Licence Apache-2.0. New third-party assets need a compatible licence and an entry in `THIRD_PARTY.md`.
- Commit after each task with the trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Commands

```bash
cargo test --workspace                      # all Rust tests
cargo build -p limner-godot                 # build the extension (debug)
scripts/godot.sh --headless --path godot -s res://tests/run_tests.gd   # Godot-side tests
scripts/shot.sh <scene-or-script> <out.png> # windowed run that saves a screenshot to local/out/
cargo run -p limner-cli -- inspect <project>
```

## File structure

```
crates/limner-core/src/
  lib.rs
  time.rs                    Flicks, conversions
  math/{mod,xform,bounds}.rs Xform (similarity + flip), Aabb
  model/{mod,ids,document,layer,keys,drawing,stroke,media,settings}.rs
  eval/{mod,clock,keys,frame}.rs        timeline evaluation -> RenderFrame
  tess/{mod,frame,brush,chunk}.rs       stroke tessellation -> ChunkMesh
  ops/{mod,layers,frames,keys,strokes,selection,optimize,lights}.rs  document edits
  tools/{mod,tuning,paint,line,colorize,blend,eraser,select,grab,thicken,nudge,optimize}.rs
  session/{mod,command,history,playback,interact,input,view,events}.rs
crates/limner-format/src/{lib,manifest,blob,store,pack}.rs
crates/limner-quill/src/{lib,json,qbin,convert,state,error}.rs
crates/limner-export/src/{lib,scene,gltf,usd,usdz,obj,fbx}.rs
crates/limner/src/{lib,app,files}.rs
crates/limner-cli/src/main.rs
crates/limner-godot/src/{lib,scene,convert,render,meshes,nodes,media}.rs
godot/
  project.godot, limner.gdextension
  main.tscn, main.gd                    mode selection (desktop, XR, test)
  world/{world.tscn,world.gd}           LimnerScene host, environment
  shaders/{stroke.gdshaderinc,stroke_*.gdshader,...}
  desktop/{shell.tscn,shell.gd,navigation.gd,sim_hand.gd}
  xr/{rig.tscn,rig.gd,action_map.tres,palette.gd,poke.gd,sim_rig.gd}
  ui/{theme.tres, panels/*.tscn+*.gd, widgets/*}
  audio/, capture/, assets/{icons,noise}
  tests/{run_tests.gd, test_*.gd, shots/*.gd}
docs/{format.md,quill-differences.md,handoff.md,roadmap.md}
scripts/{fetch-godot.sh,fetch-godot.ps1,godot.sh,shot.sh,build.sh,test.sh,export.sh}
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
- [ ] **2.2 Playback.** `session/playback.rs`: play, stop, seek, step, range, loop; frame-accurate clock in flicks.
- [ ] **2.3 Pictures, models, sounds from Quill.** Decode qbin records; media table; Godot nodes for flat, 360 and cubemap pictures, model meshes, positional and flat sounds with attenuation and cone/frustum modifiers.
- [ ] **2.4 Spawn areas and cameras.** Gizmos, jump to spawn area, camera preview.
- [ ] **2.5 Transport UI.** Play controls, scrubber, timecode, fps.

## M3: Native format

- [ ] **3.1 Blob and manifest.** `.lstk` stroke blobs (zstd, BLAKE3-addressed), `limn.json`. Round-trip property tests.
- [ ] **3.2 Project store.** Atomic save, incremental writes, garbage collection, media in original encoding, `state.json`, thumbnail.
- [ ] **3.3 Pack/unpack** `.limnz`. **3.4 CLI convert** Quill to `.limn`. **3.5 Open/save/autosave** in the app. **3.6 `docs/format.md`.**

## M4: Editing core and desktop shell

- [ ] **4.1 Commands and history.** `Command` enum (serde), `Session::exec`, snapshot undo with memory cap, revision counters.
- [ ] **4.2 Layer operations.** Add, delete, move, duplicate, rename, visibility, lock, opacity, merge down, flatten, group/sequence conversion, flip, reset transform, pivot operations.
- [ ] **4.3 Bridge command and query API.** `exec(name, args)`, `query(name, args)`, signals on revision change.
- [ ] **4.4 Shell UI.** Theme, layout, Layers panel, Transform panel, Documents panel, Settings.
- [ ] **4.5 Selection and transforms.** Selection set, overlay rendering, gimbal with axis, plane and ring handles, pivot editing, rotation snapping, duplicate-and-repeat.

## M5: Painting

- [ ] **5.1 Input and interaction.** `InputFrame`, gesture state machine, navigation (grab, two-hand scale, rapid zoom), stick shortcuts, button actions.
- [ ] **5.2 Paint and Line.** Raw sampling, pressure to width and opacity, tapers, caps, seven brushes, animated-brush duration, axis snap, origin grab.
- [ ] **5.3 Colorize** (14 modes) and Eraser. **5.4 Selection tool, Grab, Thicken/Thin, Nudge.** **5.5 Optimize** (attribute-aware simplification, tool and layer/document commands).
- [ ] **5.6 Desktop simulated hand, cursor and brush preview. 5.7 Tools and Colour panels.**

## M6: Animation authoring

- [ ] **6.1 Frame commands** (new, hold, duplicate, delete, remove key, double, reverse, loop, copy selection to frames). **6.2 Clips and sequences** (split, trim, move, loop point, bake). **6.3 Keys** (add, delete, move, interpolation, copy/paste, reverse, keys in children, recording). **6.4 Onion skin.** **6.5 Timeline panel.** **6.6 Painting and tools during playback.**

## M7: Lights

- [ ] **7.1 Light layers** in the model, format and commands. **7.2 Lit stroke shaders** and Godot light nodes. **7.3 Bake lights to vertex colour.** **7.4 Lights panel and gizmos.**

## M8: Headset

- [ ] **8.1 XR rig and action map. 8.2 Palette, panel viewports and poke input. 8.3 Simulated rig and scripted gesture tests. 8.4 Haptics and UI sounds. 8.5 Export presets: Windows, Linux, macOS, Quest (Android library build of the extension).**

## M9: Export and capture

- [ ] **9.1 Export scene builder** (baked meshes, sampled animation). **9.2 glTF. 9.3 USD/USDZ. 9.4 OBJ. 9.5 FBX. 9.6 Stills and 360. 9.7 Camera render to video. 9.8 Export panel and CLI.**

## M10: Beyond Quill

- [ ] **10.1 Smooth key interpolation. 10.2 Symmetry painting. 10.3 Text layers. 10.4 Performance panel and budgets. 10.5 Persistent palettes and preferences. 10.6 Autosave and recovery.**

## M11: Hand-off

- [ ] **11.1 Packaged builds. 11.2 `docs/handoff.md`, `docs/quill-differences.md`, `docs/roadmap.md`, README. 11.3 Full verification pass and final report.**

## Self-review notes

Every section of the design maps to a milestone: sections 6 to 7 to M0 to M2, section 8 to M1 and M7, section 9 to M5, section 10 to M4 to M6, section 11 to M8, section 12 to M3, section 13 to M0, M3 and M9. Tasks from M3 onward are refined in this file when their milestone starts, once the interfaces they consume exist.
