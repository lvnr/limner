# Limner: design

Date: 2026-10-14. Status: approved direction, building. "Limner" is a provisional working title (the owner may rename it); the project file extension is `.limn`.

Limner is an open-source VR painting and animation tool that succeeds Quill 2.9 (unmaintained since 2021). It keeps Quill's way of working, opens existing Quill projects, runs on a desktop screen and in a headset, and adds what Quill never had, starting with scene lights. The feature map it is measured against is `reports/Quill feature map and tech stack.md`; the notes behind that report are in `research_notes/`.

## 1. Decisions already made

These were settled with the project owner and are not to be reopened without a new reason.

| Decision | Choice | Why |
|---|---|---|
| Host engine | Godot 4.8 with its built-in Mobile renderer | Standalone headsets, PC VR, lights, UI, audio and packaging come with the engine. Pinned to an exact build (section 3). |
| App logic | Engine-independent Rust core, bridged with godot-rust | Everything except drawing pixels and reading devices can be unit-tested without Godot or a headset. |
| Fidelity | Quill's features and working habits, not its exact pixels | "Where we can, we should create a better product." When Quill's behaviour and a better design differ, take the better design and record the difference in `docs/quill-differences.md`. |
| File format | Own open format, plus a one-way Quill importer | Quill's format is undocumented and its EULA forbids reverse engineering the binaries. Format knowledge comes from user-owned files and Apache/MIT-licensed code only. |
| Targets | Desktop (macOS, Windows, Linux), PC VR through OpenXR, standalone Quest; Steam Frame, Pico, Android XR and visionOS later | Standalone creation is the community's first request. |
| Licence | Apache-2.0 | Patent grant; compatible with the IMM reference code and with Godot (MIT) and godot-rust (MPL-2.0). |

Constraints that hold throughout:

- `3d models&design/` holds proprietary assets from a Quill install. It is reference material only, is git-ignored, and nothing from it may be copied into the repository or shipped.
- `~/projects/quill-exporter/` holds the owner's private artwork and Quill's exporter. It is read-only test data. Tests that use it must skip cleanly when it is absent.
- Quill's own binaries are not run, decompiled or inspected.

## 2. What the product is

One application with two presentations of the same document and tools:

- **Desktop**: a 3D viewport with docked panels, mouse and pen input. Quill had no desktop workflow; this is new.
- **Headset**: the same panels on a palette held in the off hand, operated by poking, with tools on the dominant hand.

Feature areas, each mapped to a milestone in section 15:

1. View and play Quill projects: strokes, layers, frame-by-frame animation, keyed motion, sequences, sound, pictures, models, spawn areas, cameras.
2. Paint and edit: nine tools (Paint, Line, Colorize, Eraser, Selection, Grab, Thicken/Thin, Nudge, Optimize), seven brushes, 14 colorize modes, selection, gimbal and pivots, layers, universal undo.
3. Animate: frames, onion skin, clips, sequences, transform and opacity keys, recording, painting during playback.
4. Lights (new): point, spot and directional light layers, per-layer lit or unlit shading, bake to vertex colour.
5. Files: native `.limn` projects, Quill import, export to glTF, USD/USDZ, FBX and OBJ.
6. Capture: stills, 360 stills, video through camera layers.
7. Beyond Quill: smooth key interpolation, symmetry, text, scene budgets, persistent palettes, autosave.

Non-goals for the first complete version: IMM export and publishing to Meta's viewer, Alembic export, multi-user collaboration, rigging and lip sync. They are listed as future work in `docs/roadmap.md`.

## 3. Stack and versions

| Part | Version | Notes |
|---|---|---|
| Godot | 4.8 dev 7 (`4.8.dev7.official.c971f93e7`), moving to beta and stable as they ship | Feature freeze was 1 October 2026. Editor build is fetched by `scripts/fetch-godot.*` into `tools/` (git-ignored). Fallback is 4.7.2. |
| Renderer | Mobile (RenderingDevice): Metal on macOS, Vulkan on Windows, Linux and Android | One renderer everywhere so desktop and headset look the same. |
| Rust | stable, 1.94 or newer (built with 1.99) | |
| godot-rust | `godot` 0.5.5, features `api-4-7`, `lazy-function-tables` | Verified to load in 4.8 dev 7. The 4.7 API level keeps 4.7.2 usable as a fallback; `lazy-function-tables` avoids start-up failures when a pre-release removes an unrelated method. |
| Math | `glam` (f64 for hierarchy, f32 for points) | |
| Serialisation | `serde`, `serde_json`, `zstd`, `blake3`, `zip` | |

## 4. Repository layout

```
Cargo.toml                 workspace
crates/
  limner-core/             document model, time, transforms, timeline evaluation, tessellation,
                           tools, commands, undo, interaction. No I/O, no engine types.
  limner-format/           native .limn reader and writer
  limner-quill/            Quill project importer (Quill.json, Quill.qbin, State.json)
  limner-export/           glTF, USD/USDZ, OBJ, FBX writers
  limner/                  facade: App = Session + file commands; used by the CLI and the bridge
  limner-cli/              `limner` binary: inspect, convert, run command scripts
  limner-godot/            godot-rust extension (cdylib): bridge between App and Godot
godot/                     Godot project: scenes, GDScript shell, shaders, UI, tests
docs/                      design, format spec, differences from Quill, hand-off guide, roadmap
scripts/                   fetch-godot, build, test, run, export
research_notes/, reports/  the Quill feature map and its sources
local/                     git-ignored: private samples, reference sources, scratch output
tools/                     git-ignored: downloaded toolchains
```

## 5. Architecture

```
            +---------------- Godot project (GDScript) ----------------+
            | desktop shell | XR rig | UI panels | audio | capture     |
            +------------------------+---------------------------------+
                                     | InputFrame (typed), commands and queries (dictionaries)
            +------------------------v---------------------------------+
            | limner-godot (Rust): LimnerScene node                    |
            |   owns App; reconciles Godot nodes and meshes with it    |
            +------------------------+---------------------------------+
                                     |
            +------------------------v---------------------------------+
            | limner (facade): App = Session + open/save/import/export |
            +----+----------------+----------------+-------------------+
                 |                |                |
          limner-core      limner-format     limner-quill / limner-export
```

Rules:

- The core never depends on Godot. The bridge never contains product logic: it converts types and reconciles nodes.
- Every edit is a **command** (a serde enum). Godot UI, the CLI and tests all issue the same commands, as JSON-compatible values. Per-frame paths (input frames, render sync) use typed calls.
- All controller behaviour (grab the world, two-hand scale, stick shortcuts, tool gestures) lives in the core and consumes an **InputFrame**. A headset, the desktop's simulated hand and a test script are three producers of the same struct.

Per frame, in `LimnerScene._process`:

1. The shell builds an `InputFrame` and passes it with `dt`.
2. `Session::update` runs interaction, tools and playback, and returns events (haptics, sounds, UI revisions).
3. `Session::evaluate` produces a `RenderFrame`: for each layer its effective visibility, opacity, f64 world transform and content reference.
4. The bridge reconciles Godot nodes with the `RenderFrame` and rebuilds only meshes whose stroke chunks changed.

## 6. Core model

### Time

Integer **flicks**, 705,600,000 per second, in `i64`. Every common frame rate divides it, and Quill's 12,600 ticks per second convert exactly (one tick is 56,000 flicks).

### Transforms

`Xform { rotation: DQuat, scale: f64, translation: DVec3, flip: Flip }`, with `Flip` one of none, X, Y, Z, and `v' = R * (s * F * v) + t`. This is Quill's layer transform. Composition, inversion and interpolation are implemented on the struct; hierarchies are composed in `f64` and only the final layer-to-viewer matrix is cast to `f32`. Stroke points are `f32` in layer space.

### Document

The document is a value with shared, immutable parts:

```
Document { settings, root: LayerId, layers: Map<LayerId, Arc<Layer>>,
           drawings: Map<DrawingId, Arc<Drawing>>, media: Map<MediaId, Arc<Media>> }
Layer    { id, name, visible, locked, collapsed, show_bounds, opacity, transform, pivot, anim, kind }
LayerKind: Group { children, sequence: Option<Sequence> } | Paint | Picture | Model | Sound
           | Viewpoint | Camera | Light | Text
LayerAnim { visibility: Vec<Key<bool>>, offset: Vec<Key<Flicks>>, opacity: Vec<Key<f32>>,
            transform: Vec<Key<Xform>>, actions: Vec<ActionKey> (root only), extra channels by name }
Paint    { fps, drawings: Vec<DrawingId>, frames: Vec<u32>, repeat: Repeat, lit: bool }
Drawing  { chunks: Vec<Arc<StrokeChunk>> }          a rope: stroke order is concatenation order
StrokeChunk { strokes: Vec<Arc<Stroke>> }           at most about 60,000 expanded vertices
Stroke   { id: u32, brush: Brush, directional: bool, points: Vec<Point> }
Point    { pos, normal, dir: [f32; 3], color: [f32; 3] (linear), opacity: f32, width: f32 (half-width) }
```

Ids are `u64`, unique per document and persisted. Cloning a document copies three maps of `Arc`s, so it is cheap.

### Undo

Undo is snapshot-based. At the start of a gesture or command the session pushes a clone of the document; edits then use `Arc::make_mut`, which copies only the layers, chunks and strokes actually touched. Undo and redo swap documents. This makes every operation undoable with no per-operation revert code. History is capped by entry count and by an estimate of retained memory. Selection, tool settings, the view and playback position are session state, not document state, and are not undone.

### Change detection

The bridge keeps the document it last rendered. Two documents are compared by `Arc` pointer: an unchanged chunk has the same pointer and its mesh is reused. Mesh identity is the chunk, so duplicated frames that share chunks share meshes.

## 7. Timeline evaluation

Semantics follow the open-source IMM player (Apache-2.0), which is the same engine Quill uses:

- Every layer has Visibility keys (clips are true/false pairs) and Offset keys (left trim). The active Visibility key is the last one at or before the timeline time of the nearest enclosing sequence; before the first key the layer is hidden.
- Plain groups only gate visibility. **Sequences** (groups with their own clock) and **paint layers** restart local time when a clip starts: `local = (timeline_time - key.time) + offset`.
- A sequence with a loop point wraps its local time by that duration while its repeat rule allows; a duration of zero means no loop.
- A paint layer shows `frames[floor(local * fps) mod frame_count]`, or the last frame once a finite repeat count is reached.
- Transform and Opacity keys interpolate by the mode stored on the earlier key: step, linear, ease in (`t^3`), ease out (`1-(1-t)^3`), ease in-out (`t^2(3-2t)`), plus Limner's smooth mode. Rotation uses shortest-arc slerp; transforms blend about the pivot.
- Root action keys (Stop, Play, Loop) structure a film into chapters. The editor can honour or ignore stops during preview.
- Effective visibility and opacity are the AND and the product along the ancestor chain.

Limner extensions, all optional and absent from imported files: smooth (Catmull-Rom) interpolation, ping-pong and finite repeat counts in the UI, keyable light and camera properties through named channels.

## 8. Rendering

### Tessellation

One cross-section ring per stored point, no resampling or smoothing, exactly as Quill does:

- Frame per point: `T` is the normalised sum of the first non-degenerate forward and backward differences; `U = normalize(N x T)` with axis fallbacks; `V = normalize(T x U)`.
- Ribbon: 2 vertices `P -+ w U`. Cylinder: regular heptagon of radius `w`. Ellipse: the heptagon with the `V` term scaled by 0.3. Cube: 4 corners `P + w(+-U +-V)`.
- Tubes have no cap polygons; strokes end in zero-width points. Ribbons are double-sided, everything else single-sided.

Each chunk becomes one `ArrayMesh` with up to four surfaces: {tube, ribbon} x {opaque, see-through}. A stroke goes to the see-through surface if any point has opacity below 0.999 or it is directional. Vertices are stored relative to the chunk's centre and the centre is folded into the instance transform in `f64`, which removes the jitter Quill shows at extreme zoom.

Vertex attributes: position; normal (for lit layers); colour as sRGB-encoded 8-bit RGB plus opacity, decoded in the shader; `CUSTOM0` with the draw-time view direction and the stroke's dither seed.

### See-through strokes

Strokes are never blended and never sorted. A see-through fragment is kept or discarded by comparing its opacity with a blue-noise value, so strokes write depth and the result does not depend on draw order. The noise offset depends on a seed and on the frame number:

- for points that are themselves see-through, the seed is the stroke's, so overlapping faint strokes build up;
- for opaque points on a faded layer, the seed is the layer's, so the layer fades evenly.

This reproduces Quill's opacity behaviour at pixel granularity. It was chosen over hardware alpha-to-coverage after a test on Godot 4.8 dev 7: Godot's alpha-to-coverage mode also blends, needs alpha-to-one to avoid that (absent on Direct3D 12 and not guaranteed on mobile), and cannot build up. Offline renders average several noise frames per output frame and supersample, which converges to smooth, exact transparency.

Fully opaque chunks on fully opaque layers use a shader with no discard, which is the common case and the fast path on mobile GPUs.

Viewports use 4x MSAA for edges.

### Lights

A light is a layer (point, spot, directional) realised as a Godot light node. Paint layers are unlit by default, so imported artwork looks unchanged. A layer switched to lit uses a wrap-lighting shader on the tessellation normals; cube strokes get flat normals from screen-space derivatives. "Bake lights" writes the lit result into vertex colours as an ordinary, undoable edit, which also makes it exportable to tools that know nothing about lights.

### The viewer transform

The user's room space is "viewer space". Navigation (grab the world) changes `view: Xform`, the placement of the viewer in the world. The headset rig stays at the Godot origin; each layer node's global transform is `inverse(view) * world_from_layer`, computed in `f64`. The desktop works the same way with a fixed virtual head, so zoom changes the world scale and tool sizes behave as they do in a headset.

## 9. Input and tools

`InputFrame` carries, for the head and each hand, a pose in viewer space, the analog trigger and grip, buttons and the thumbstick. Handedness is resolved before the core sees it: the core speaks of the dominant and the off hand.

Controller mapping follows Quill 2.x (see the report's mapping table): trigger applies the tool, grips grab the world, both grips scale, dominant stick is size and undo/redo, off stick is frames and playback, A deletes, B picks colour, X toggles the Selection tool, Y shows the palette, off trigger (ALT) modifies.

Tools act on points inside a sphere in viewer space and never change stroke topology, except Paint and Line (which add strokes), Optimize (which removes points) and the Eraser (which removes a stroke once every point is at zero opacity). Each trigger press is one undo step. Constants that Quill never published (falloff and taper curves, strength rates) have documented defaults in `limner-core::tools::tuning` and are meant to be tuned on a headset against the real application.

The desktop's simulated hand places the cursor where the mouse ray meets a work plane facing the camera, or the nearest stroke surface. Pen pressure drives the trigger.

## 10. User interface

Panels are Godot `Control` scenes written once. On the desktop they dock around the viewport. In a headset each is rendered to a `SubViewport` and shown on the palette, and the dominant-hand cursor produces pointer events by poking the panel plane.

Panels: Tools, Colour, Layers and Timeline, Transform, Lights, Documents, Export and Capture, Monitor, Performance, Settings. The default theme is dark. Icons are from an openly licensed set or drawn for the project; no Quill asset is used.

Panels learn about changes through revision counters exposed by the bridge and re-query only what changed.

## 11. XR

OpenXR through Godot's XR nodes, with one action map for Touch, Index, Vive, WMR, Pico and Steam Frame controllers. The XR layer only turns controller state into an `InputFrame` and places the palette; nothing else is headset-specific. The Godot OpenXR Vendors plugin supplies the Quest and Pico loaders for Android export.

No OpenXR runtime exists for this project's development Mac beyond Meta's simulator, so headset code paths are kept thin and are exercised on the desktop through a simulated rig that feeds the same poses. First real-headset testing happens on the owner's Windows PC; `docs/handoff.md` lists what to check.

## 12. Native format

A project is a folder, `Name.limn/`:

```
limn.json                  scene graph, settings, drawing and media tables (readable JSON)
drawings/ab/abcdef....lstk stroke data, one file per distinct drawing, named by content hash
media/<hash>.<ext>         pictures, sounds and models in their original encoding
state.json                 editor state: palette, last tool, view (optional)
thumbnail.png
```

Saving writes new content-addressed files first, then replaces `limn.json` atomically, so an interrupted save never corrupts a project, unchanged drawings are never rewritten, and the folder works well with git and Perforce. A project can also be packed into a single `Name.limnz` (a zip of the folder) for sharing; the app opens both. The byte-level specification is `docs/format.md`.

## 13. Import and export

- **Quill import** reads every layer kind, all key channels, legacy schema variants (matrix transforms, single-drawing paint layers, numeric or string times) and editor state. It is lossless for strokes.
- **glTF 2.0 (.glb)**: vertex-coloured unlit meshes, hierarchy, keyed transforms sampled per frame, frame-by-frame animation as stepped visibility, lights (`KHR_lights_punctual`), cameras.
- **USD (.usda) and USDZ**: meshes with normals and display colour, time-sampled transforms and visibility, lights (UsdLux), cameras. Forward-slash references and correct units, which Quill's exporter gets wrong.
- **OBJ** for static meshes; **FBX** binary with vertex colours for pipelines that still need it.
- **Images and video**: stills up to 8K, 360 stills, PNG sequences and video rendered through camera layers in a separate process using Godot's movie writer, with sound.

## 14. Testing

- `cargo test` covers the core: transforms (against matrix composition), timeline evaluation, tessellation, each tool, commands and undo, the native format (round trips, interrupted saves), the importer (synthetic projects built in tests), exporters (structural validation).
- Local-only tests run against the owner's sample project and Quill's own Alembic and USD exports of it; they skip when the files are absent.
- Command scripts drive whole sessions headlessly, including simulated controller gestures.
- Godot runs headless for bridge and UI logic tests, and windowed to save screenshots that are inspected by eye and compared against stored references.
- Headset behaviour, latency and feel cannot be verified on the development Mac. `docs/handoff.md` is the checklist for the first session on real hardware, and the report's table of unknowns lists what to measure in real Quill.

## 15. Milestones

Each leaves something that runs. Order differs slightly from the list agreed with the owner because nothing can be tried in a headset on the development machine: the headset rig is built alongside the desktop shell and exercised through simulation.

| # | Milestone | Done when |
|---|---|---|
| M0 | Foundations: workspace, core model, transforms, Quill importer, CLI `inspect` | The sample project imports with exact stroke and point counts; transforms reproduce Quill's Alembic positions. |
| M1 | Desktop viewer | The sample renders correctly in Godot with navigation; screenshots checked. |
| M2 | Player | Frames, clips, sequences, keys, sound, pictures, models, spawn areas and cameras evaluate and play. |
| M3 | Native format | Save, load, pack; round-trip tests; autosave. |
| M4 | Editing core and desktop shell | Commands, undo, layers panel, selection, transforms, gimbal. |
| M5 | Painting | All nine tools, brushes, colour, simulated hand on the desktop. |
| M6 | Animation authoring | Timeline panel, frame commands, onion skin, keys, recording, painting during playback. |
| M7 | Lights | Light layers, lit shading, bake. |
| M8 | Headset | XR rig, action map, palette, poke UI, simulated-rig tests, Windows and Quest export presets. |
| M9 | Export and capture | glTF, USD/USDZ, OBJ, FBX; stills, 360, video. |
| M10 | Beyond Quill | Smooth keys, symmetry, text, budgets panel, persistent palettes. |
| M11 | Hand-off | Packaged desktop builds, `docs/handoff.md`, README, contributor notes. |

## 16. Risks

| Risk | Handling |
|---|---|
| Headset code is unverified on the development machine | Thin XR layer, simulated rig, explicit checklist for the PC. |
| godot-rust on Android is labelled experimental | Build the Android library early in M8; if it fails, the report and hand-off say so plainly. The core has no Android-specific code. |
| Pre-release Godot regressions | Pinned build; extension built at the 4.7 API level so 4.7.2 remains a fallback. |
| Pixel-level dither is grainier than Quill's sample-level dither | Temporal variation in real time; accumulation and supersampling offline. |
| Stroke meshes use more memory than Quill's point buffers | Chunk meshes are built on demand and evicted; a point-buffer path can be added later behind the same chunk interface. |
| Feel constants are guesses | Centralised in one tuning module and listed for measurement. |
