# Importing Quill projects

Limner opens a Quill project folder (`Quill.json`, `Quill.qbin`, `State.json`) and turns it into a Limner document. The import is one way: nothing is written back. The code is the crate `crates/limner-quill`.

```bash
cargo run -p limner-cli -- inspect <project folder>        # what the importer makes of a project
cargo test -p limner-quill                                  # synthetic projects, byte for byte
cargo test -p limner-quill --test samples -- --nocapture    # sweep over local/samples (skips when absent)
cargo run -p limner-quill --example synthetic_project -- local/out/quill-synthetic   # a project with every layer kind
```

Everything the importer repairs, approximates or leaves out is returned as a warning (`Imported::warnings`), one sentence each, starting with the layer path when it concerns one layer. A clean import has no warnings. The owner's private project imports with none.

Format knowledge comes from project files and from openly licensed code (the IMM sources, SharpQuill, quill-blender, Open Brush). Nothing comes from Quill's binaries.

## How sure each part is

Three grades are used below.

- **File**: checked on files written by Quill. Decoding the records that the JSON points at (gallery posters included) accounts for every byte of the `Quill.qbin` of all 21 public samples that have one, and of the owner's project: the records tile each file with no gaps and nothing left over.
- **Code**: taken from Quill's own open-sourced code (the IMM repository) or from a community reader. No file containing the feature could be found.
- **Guess**: no file and no code. The importer matches loosely and always warns.

| Part | Grade | Evidence |
|---|---|---|
| Layer tree, common layer fields, transforms as objects, pivots | File | All samples |
| Transforms as 16-float matrices (row-major, translation in the fourth column) | File | 8 non-identity matrices in two 2017 projects; all are rotation with uniform scale |
| Visibility and Offset keys, sequences, loop durations | File | All 2.x samples; the owner's project |
| Transform keys | File | The owner's project (18 layers) |
| Opacity keys, interpolation other than "None", Action keys | Code | SharpQuill, quill-blender, Open Brush |
| Paint: drawings, frames, strokes, vertices, brush types 1 to 4 | File | About 100,000 strokes and 1.3 million points across the samples |
| Paint: one drawing stored on the layer (2017) | File | Four 2017 projects |
| Spawn areas and their thumbnails | File | Spawn areas in 18 samples; 7 thumbnails in 5 of them |
| Cameras | File | The owner's project |
| Pictures: flat, RGB, rows top to bottom | File | Ten picture layers in two projects; a photograph extracted from one is upright with red and blue the right way round |
| Pictures: RGBA, viewer-locked | Code | Pixel format 7 in Quill's image enum; `ViewerLocked` is in every picture layer but always false |
| Pictures: "360 Equirectangular Mono" and "Stereo", "2DDepth" | Code | Type strings from quill-blender and the Icosa fork of SharpQuill. Stereo is top half left eye, as in the IMM player |
| Pictures: cubemaps | Guess | Quill has a cross and a vertical strip layout (IMM enum); the type strings are in no reader |
| Models: mesh record, triangles and quads, position, colour, normal | File | One OBJ import: 16,577 vertices, 32,482 triangles and 6 quads; the record ends on the last byte of the file. Attribute order is the one Quill's model shader declares |
| Models with a hierarchy (FBX), vertex colours other than white | Code | `IsModelTopLayer` is documented in SharpQuill; no file |
| Sound: layer fields (`Type`, `Gain`, `Loop`, `Attenuation`) | Code | quill-blender and SharpQuill |
| Sound: audio record | Code | Two readings exist; see below. No public project has a sound layer |
| Sound: cone and frustum parameters | Guess | Only the modifier type names are public |
| `State.json`, 2017 and 2.x shapes | File | All samples |
| Keep-alive behaviours, Quill 1.4 to 1.8 clip layout | none | Reported, not imported |

GitHub code search finds no `Quill.json` with a sound layer, a 360 or cubemap picture, a viewer-locked picture, an RGBA picture, an FBX model or a non-"None" keep-alive (searched 14 October 2026). If such a project turns up, drop it into `local/samples/` and run the sweep.

## What each layer kind imports

| Quill layer | Becomes | Imported | Not imported |
|---|---|---|---|
| Group | Group | Children, visibility, lock, collapse, bounds flag, opacity, transform, pivot, all keys | |
| Group with `Timeline` | Sequence | The same, plus loop duration and repeat count | The root's one-hour `Duration` (it is a timeline length, not a loop) |
| Paint | Paint | Drawings (shared records stay shared), the frame table, frame rate, repeat count, every stroke and point bit for bit | Stroke and drawing bounding boxes (recomputed); the two stroke header fields that are always zero |
| Viewpoint | Viewpoint | Floor or eye level, colour, volume sphere, allowed axes, export and volume flags, thumbnail | |
| Camera | Camera | Field of view | |
| Picture | Picture | Decoded pixels (RGB or RGBA, top row first), projection, viewer lock, source path | Depth of "2DDepth" pictures |
| Model | Model | Triangles (quads split along their first diagonal, winding kept), positions, normals, colours, shading (`Unlit`, or `Smooth` as lit), wireframe flag, source path | Extra vertex streams; point clouds draw nothing |
| Sound | Sound | Audio, kind (flat, positional, ambisonic), gain, loop, attenuation mode and radii, source path; the layer's opacity is its volume | Directional cone and frustum unless their parameters are recognised |

A model imported with its hierarchy is a group flagged `IsModelTopLayer` holding model layers; it imports as a group of model layers and the flag is dropped. If a `Model` layer has children as well as a mesh, it becomes a group whose first child is a model layer of the same name holding the mesh.

Records are read once per file offset: two layers that point at the same drawing, picture, mesh or sound share one drawing or media item.

Media items are named after the file in `ImportFilePath`, or after the layer when no path is stored.

## Binary records

All little-endian. `DataFileOffset` is an absolute offset in hexadecimal, padded to 16 digits by Quill 2.x and bare in 2017 files.

**Drawing** (File): `u32` stroke count, then strokes: a 40-byte header (`u32` id, `i32` 0, six `f32` bounds, `i16` brush type, `u8` 0 for directional opacity or 1 for always visible, `u8` 0, `i32` point count) and 56-byte points.

**Picture** (File for RGB): bytes `00 00 00 01`, image type (2 is a flat image), pixel format (2 grey, 6 RGB, 7 RGBA in Quill's image enum), `00 00`, then `u32` width, height and depth (1), then 8-bit pixels, rows top to bottom. Grey is expanded to RGB. Other formats and volumes are refused with a warning.

**Model** (File): eight zero bytes, then Quill's mesh serializer: `u32` version 0, six `f32` bounds, `u32` stream count; per stream `u32` vertex count, stride, divisor and element count, then per element `u32` type (1 is `f32`), component count, normalize flag and byte offset, then the vertex bytes; then `u32` face type (0 polygons, 1 points), `u32` index depth (1 is 32-bit), `u32` index array count; per array `u32` face count and 20 bytes per face: `i32` corner count (3 or 4) and four `u32` indices. A triangle's fourth index holds leftover memory and is ignored. The three `f32` triples of a vertex are position, colour and normal.

**Sound** (Code). Two readings exist and no file settles it, so the importer looks at the first bytes instead of assuming:

| First bytes | Read as | Result | Source |
|---|---|---|---|
| `00 00 00 00` | `u32` version 0, `u16` 0, `u8` channels, `u8` bits per sample, `u32` sample rate, `u64` frame count, then interleaved PCM | `MediaData::RawAudio`; 32-bit samples are floats (Quill's own encoders treat them so) and 8-bit samples are unsigned, as in a WAV file | Quill's wave serializer in the IMM sources; quill-blender extracts WAV files from projects this way |
| A positive `i32`, then `RIFF…WAVE`, `OggS`, `ID3` or an MPEG frame header | A length and a whole audio file | `MediaData::Encoded` (`wav`, `ogg`, `mp3`) | The Icosa fork of SharpQuill |
| `RIFF…WAVE` | A WAV file with nothing in front | `MediaData::Encoded` (`wav`) | Quill's audio page says the data is stored as WAV |

The three cannot be confused: each needs different first four bytes. Anything else is "the record matches no known sound layout" and the layer stays silent.

## What differs by Quill version

`Version` is 1 in every file, so the importer detects features instead.

| Era | What the files look like | How it is imported |
|---|---|---|
| January to May 2017 (before 1.0) | `HomePosition` (16 floats) and `TrackingOrigin` instead of a spawn area; no `Framerate`, export range or `DefaultViewpoint`; layers have a matrix `Transform` and `AnimOffset`, and no `Pivot`, `Animation` or `KeepAlive`; a paint layer is `{BoundingBox, AnimSpeed, PlaybackReduce, DataFileOffset}`; the `.qbin` header holds the next stroke id and ids count across the file; strokes have no zero-width end points; state has `ToolID`, top-level `ShowGrid` and boolean tapers | Matrices become transforms; every layer is present for the whole timeline; each paint layer has one drawing and one frame; stroke ids are kept (they stay unique within a drawing); document rate 24 fps and a 10 s range; no default spawn area. `AnimSpeed` and `PlaybackReduce` (stroke replay of the first builds) are dropped without a warning. A `HomePosition` other than identity, a `TrackingOrigin` other than 1 and a non-zero `AnimOffset` are reported |
| 1.0 to 1.3 (August 2017 to early 2018) | Not found. Readers describe one drawing stored on the paint layer and matrix transforms | Covered by the 2017 path |
| 1.4 to 1.8 (2018 to early 2019) | Not found. quill-blender mentions animation blocks holding `Frames` or `Spans` | The layer is shown for the whole timeline, with a warning. `Drawings` and `Frames` on the paint layer are read as in 2.x |
| 2.x, 2019 to 2021 | `Metadata` and `Gallery`; times and frame indices as strings; `DirectManipulation` a boolean; no `CameraResolution` before 2021 | Title and description are kept unless they are the placeholders "Untitled" and "Shared from Quill"; gallery posters are reported and dropped |
| 2022 to 2025 | No `Metadata` or `Gallery`; spawn areas have a 512 by 512 thumbnail; `DirectManipulation` is 0 or 1 | Imported in full |
| Other writers (SharpQuill, quill-blender) | Numbers where Quill writes strings; upper-case or short offsets; `DefaultViewpoint` null | Accepted |

A document's title is the metadata title, else the folder name.

## Editor state

`State.json` is optional and never needed. `QuillState` carries what a new app can use:

| Field | From |
|---|---|
| `palette` (16 swatches; also copied into the document) | `ColorPalette` |
| `paint_layer`, `move_layer`, `camera_layer` (layer paths) | `PaintLayer`, `MoveLayer`, `CameraLayer` |
| `show_grid`, `grid_texture`, `grid_scale` | `Rulers.ShowGrid` (2.x) or `ShowGrid` (2017); `DetailRender.Surface` |
| `direct_manipulation` | A boolean, or 0 and 1 in recent files |
| `tool_name`, `tool_id` | `ToolName` (2.x) or `ToolID` (2017). With only a number, the name is taken from settings that one tool alone has: tapers mean Paint, `SelectInAllLayers` means Select, `InnerSize` means Colorize. In files, 0 was Paint and 4 was Select; the rest of the numbering is unknown |
| `settings`: brush id, colour, opacity, size, inner size, directional stroke, width and transparency taper, select-in-all-layers | `Tool`. Tapers are strings in 2.x ("None", "Pressure") and booleans in 2017; true is read as pressure |
| `tool` | The `Tool` object as stored |

`brush_id` is the brush's place in Quill's brush panel (seven brushes), not the stored stroke type (four).

## Robustness

- File names are matched without regard to case, and a byte-order mark in front of either JSON file is skipped.
- Integers and times may be numbers or strings; numbers that are not finite read as missing.
- Every read of the binary file is bounds-checked. A `Quill.qbin` cut at any byte still imports, with a warning for each record that no longer fits. A missing or empty `Quill.qbin` gives one warning and an otherwise complete layer tree.
- Unit tests corrupt random bytes of the binary file and replace random values of the JSON with values of other shapes; the importer returns a document or an error and never panics.
- The result is checked with `Document::validate()` before it is returned.

## Warnings

`{path}` is the layer path, such as `Root/Body/Arm`.

Whole project:

- `Quill.qbin is missing; all drawings and media are empty`
- `Quill.qbin is empty; all drawings and media are empty`
- `State.json could not be read; editor state was not imported`
- `Quill.json declares format version N; this importer was written for version 1`
- `the default spawn area "…" was not found`
- `N gallery poster(s) saved for Quill's sharing feature were not imported`
- `the home position of this 2017 file is not imported; viewers start at the origin`
- `the tracking origin of this 2017 file (N) is not imported; floor level is assumed`
- `N drawing(s) point outside Quill.qbin and were imported empty`
- `N stroke(s) with no points or non-finite values were skipped`
- `stroke data ran past the end of Quill.qbin; the affected drawing(s) were cut short`
- `N stroke(s) used an unknown brush type and were read as cylinders`
- `some drawings had repeated stroke ids; their strokes were renumbered`

Any layer:

- `{path}: unknown layer type "…" imported as a group` (it had children) or `{path}: layer of unknown type "…" was skipped`
- `{path}: unreadable transform; identity used` (also `pivot`)
- `{path}: the transform matrix has uneven scale, shear or perspective; the nearest uniform transform is used`
- `{path}: an unreadable transform key was skipped`, `{path}: an unreadable action key was skipped`
- `{path}: unknown interpolation "…" read as linear`
- `{path}: the keep-alive behaviour "…" is not imported`
- `{path}: the animation offset of this 2017 layer (N) is not imported`
- `{path}: the clips of this Quill 1.x layer are in a layout this importer does not know; the layer is shown for the whole timeline`

Paint:

- `{path}: a drawing has no readable file offset and is empty`
- `{path}: some frames pointed at missing drawings and now show the last one`

Pictures and spawn areas:

- `{path}: the picture could not be read (reason)`, `{path}: the spawn area thumbnail could not be read (reason)`
- `{path}: the picture has no readable file offset and is empty`
- `{path}: the picture is stored as image type N, not a flat image; it was read as one grid of pixels`
- `{path}: picture type "2DDepth" is shown as a flat image; its depth is not imported`
- `{path}: picture type "…" is not a name this importer knows; it is shown as a cubemap cross` (or `a cubemap strip`, `a 360 image`, `a stereo 360 image`, `a flat image`)

Models:

- `{path}: the model's geometry could not be read (reason); the layer is kept empty`
- `{path}: the model has no readable file offset; the layer is kept empty`
- `{path}: unknown shading model "…" read as unlit`
- `{path}: N face(s) of the model were skipped (not a triangle or quad, or pointing at a missing vertex)`
- `{path}: N vertex position(s) of the model were not finite; the faces using them were skipped`
- `{path}: N non-finite normal(s) or colour(s) of the model were replaced`
- `{path}: the model's vertex attributes are not the position, colour, normal set Quill writes; normals and colours may be missing or swapped`
- `{path}: the model has more than one vertex stream; only the first was imported`
- `{path}: the model is a point cloud; its N point(s) are kept but nothing is drawn`

Sounds:

- `{path}: the sound's audio could not be read (reason); the layer is kept silent`
- `{path}: the sound has no readable file offset; the layer is kept silent`
- `{path}: unknown sound type "…" read as flat`
- `{path}: unknown attenuation mode "…"; the sound does not fade with distance`
- `{path}: unknown sound modifier "…" was not imported`
- `{path}: the sound has N directional modifiers; only the first is imported`
- `{path}: the sound's directional cone has no parameters this importer recognises; the sound plays in all directions` (or `frustum`)
- `{path}: the sound's directional cone was read from parameter names no Quill file has confirmed (angles taken as radians); check it` (or `frustum`, `degrees`)

Reasons for an unreadable record: `the record runs past the end of Quill.qbin`, `pixel format N is not supported`, `the picture has no pixels`, `the picture is a volume of N slices`, `mesh version N is not supported`, `the mesh claims N vertex streams`, `a vertex stream claims N attributes`, `face type N is not supported`, `index depth N is not supported`, `the mesh claims N index arrays`, `the mesh has no floating-point positions`, `the header describes N channel(s) of N-bit audio at N Hz`, `the record matches no known sound layout`.

How a directional cone or frustum is read: the numeric fields of the modifier, and of objects one level inside it, are matched by name without regard to case or punctuation. A name containing `anglein` or `inner` is the inner angle (ending in `x` and `y`, or containing `horizontal` and `vertical`, for a frustum), `band` is the falloff band, and `atten` (or `out` without `angle`) is the level outside. If any angle is larger than a full turn in radians, all are taken as degrees.

## Dropped without a warning

These exist in files and have no place in a Limner document. None affects what is drawn or heard.

- `AnimSpeed` and `PlaybackReduce` on 2017 paint layers.
- `Metadata.ThumbnailCropPosition`, `Gallery.Thumbnails`, and the placeholder title and description.
- Bounding boxes in the JSON and in stroke and mesh records (Limner computes its own).
- The stroke-id counter in the header of 2017 `.qbin` files.
- `IsModelTopLayer`.

## Public samples tested

Downloaded on 14 October 2026 into `local/samples/<name>/` (git-ignored, about 210 MB) at the commits below. Most of these repositories carry no licence, so the files are test input only: never commit them or copy their content into the repository. Counts are from the sweep test. Dates are the commit that added the project or the date in its folder name, so they are the latest the file can have been written.

| Name | Source | Dated | Layers | Strokes | Points | Warnings | What it exercises |
|---|---|---|---|---|---|---|---|
| `floatingink_bg_blue` | [darknoon/floating.ink](https://github.com/darknoon/floating.ink/tree/f546a574eaf1c72c59607d3ceff075a5127b897d/client/public/data/bg_blue) | Jan 2017 | 2 | 0 | 0 | 0 | 2017 schema, empty drawing |
| `floatingink_bg_black_1white_1grey_1pink` | [darknoon/floating.ink](https://github.com/darknoon/floating.ink/tree/f546a574eaf1c72c59607d3ceff075a5127b897d/client/public/data/bg_black_1white_1grey_1pink) | Jan 2017 | 2 | 3 | 322 | 0 | 2017 schema, stroke counter in the header |
| `floatingink_happy_new_year` | [darknoon/floating.ink](https://github.com/darknoon/floating.ink/tree/f546a574eaf1c72c59607d3ceff075a5127b897d/client/public/data/happy_new_year) | Jan 2017 | 2 | 32 | 2,307 | 0 | 2017 schema, a matrix with a translation, global stroke ids |
| `mountainwaterlove_1scene01` | [mingzexiao6/MountainWaterLove](https://github.com/mingzexiao6/MountainWaterLove/tree/c314af012b86ce13e660707c9413a9fa06019db5/Deprecated/1scene01) | May 2017 | 15 | 778 | 48,819 | 0 | 2017 schema with groups, 7 rotated and scaled matrices, all four brush types, `ToolID` 4 |
| `quillustrate_Base` | [btparker/quillustrate](https://github.com/btparker/quillustrate/tree/8739b6ec0f72c0eaf66dd3cb18d56f400fc30d0c/assets/QuillExamples/Base) | 2020 | 3 | 0 | 0 | 0 | Quill 2.x, `Metadata`, empty `Gallery`, no `CameraResolution` |
| `quillustrate_CBrushLine` | [same, CBrushLine](https://github.com/btparker/quillustrate/tree/8739b6ec0f72c0eaf66dd3cb18d56f400fc30d0c/assets/QuillExamples/CBrushLine) | 2020 | 3 | 1 | 8 | 0 | One cube stroke; Liner tool state |
| `quillustrate_CBrushLineRed` | [same, CBrushLineRed](https://github.com/btparker/quillustrate/tree/8739b6ec0f72c0eaf66dd3cb18d56f400fc30d0c/assets/QuillExamples/CBrushLineRed) | 2020 | 3 | 1 | 8 | 0 | Colorize tool state |
| `quillustrate_CCBrushStroke` | [same, CCBrushStroke](https://github.com/btparker/quillustrate/tree/8739b6ec0f72c0eaf66dd3cb18d56f400fc30d0c/assets/QuillExamples/CCBrushStroke) | 2020 | 3 | 1 | 31 | 0 | One cylinder stroke |
| `quillplay_Assets_root`, `quillplay_Flowers00`, `quillplay_flowers_00` | [cabbibo/QuillPlay](https://github.com/cabbibo/QuillPlay/tree/076593c39602bc602fb51521a63a89a7c9c1ea35/Assets) (`Assets`, `Assets/Flowers00`, `Assets/flowers_00`) | 2021, apparently written by SharpQuill | 2 each | 0 | 0 | 0 | A third-party writer: numeric times, null `DefaultViewpoint`, 8-byte `.qbin` |
| `quillplay_grass_00` | [cabbibo/QuillPlay](https://github.com/cabbibo/QuillPlay/tree/076593c39602bc602fb51521a63a89a7c9c1ea35/Assets/grass_00) | 2021 | 3 | 234 | 1,858 | 1 | 5 gallery posters; Optimizer tool state |
| `meshcutter_isaac_lighthouse_00` | [cabbibo/MeshCutter](https://github.com/cabbibo/MeshCutter/tree/f83715cb072b3e8c8558e6a650272f8bb9cea68f/Assets/Ocean/isaac_lighthouse_00) | 2021 | 11 | 20,667 | 200,406 | 1 | 5 picture layers, 7 gallery posters, 63 MB |
| `bab_isaac_gates_00` | [cabbibo/b-a-b](https://github.com/cabbibo/b-a-b/tree/b5adda18300989ca784bfb891e7afc11d45d9790/Assets/Models/TutorialLevel/isaac_gates_00) | 2021 | 11 | 13,122 | 219,363 | 1 | 5 picture layers, frame animation, nested spawn area |
| `bab_isaac_gates_09252021` | [cabbibo/b-a-b](https://github.com/cabbibo/b-a-b/tree/b5adda18300989ca784bfb891e7afc11d45d9790/Assets/Models/Ether/isaac_gates_09252021) | Sep 2021 | 40 | 31,973 | 334,123 | 1 | 31 paint layers, 58 drawings, ribbons |
| `meshonstring_tent_00` | [cabbibo/MeshOnString](https://github.com/cabbibo/MeshOnString/tree/74fc3b358996f80457ff15482334bc43fa7dcc64/Assets/tent_00%20(1)/tent_00) | 2022 or earlier | 3 | 634 | 17,303 | 1 | 2 gallery posters |
| `bab_isaac_desert_09062022` | [cabbibo/b-a-b](https://github.com/cabbibo/b-a-b/tree/b5adda18300989ca784bfb891e7afc11d45d9790/Assets/Models/WesWrenAssets/Desert/isaac_desert_09062022) | Sep 2022 | 22 | 0 | 0 | 1 | No `Quill.qbin` in the repository: 13 paint and 6 picture layers import empty with one warning |
| `bab_isaac_cave_04012024` | [cabbibo/b-a-b](https://github.com/cabbibo/b-a-b/tree/b5adda18300989ca784bfb891e7afc11d45d9790/Assets/Models/WesTests/isaac_cave_04012024) | Apr 2024 | 5 | 3,026 | 33,341 | 0 | The only model layer: an OBJ, 16,577 vertices, 32,494 triangles after splitting quads |
| `bab_isaac_tree_04012024` | [cabbibo/b-a-b](https://github.com/cabbibo/b-a-b/tree/b5adda18300989ca784bfb891e7afc11d45d9790/Assets/Models/WesTests/isaac_tree_04012024) | Apr 2024 | 16 | 2,538 | 37,582 | 0 | Two spawn areas with thumbnails |
| `bab_isaac_tree_04092024` | [cabbibo/b-a-b](https://github.com/cabbibo/b-a-b/tree/b5adda18300989ca784bfb891e7afc11d45d9790/Assets/Models/WesTests/isaac_tree_04092024) | Apr 2024 | 6 | 21,570 | 299,632 | 0 | 18,868 ellipse strokes |
| `aivaneer_pcvr-game-shell` | [AIVaneer/PCVR-game-shell-](https://github.com/AIVaneer/PCVR-game-shell-/tree/aa185144797af9d98c11f467151ca6583eb1faf8) | Jan 2025 | 13 | 3,854 | 116,954 | 0 | Frame animation of 10 drawings, two spawn areas |
| `b2renger_quill2glb_fire` | [b2renger/Quill2Glb](https://github.com/b2renger/Quill2Glb/tree/ba18232ef3d0ad8326893bb0bd4b1b98b05ae865/Quill_scene_Fire) | Nov 2025 | 3 | 1,438 | 23,598 | 0 | 50 drawings on one layer, looping forever |

The single warning on six of them is the gallery-poster one, except for the desert project, where it is the missing `Quill.qbin`.

To fetch a sample again, download the three files from the same path on `raw.githubusercontent.com`, for example:

```bash
base=https://raw.githubusercontent.com/cabbibo/b-a-b/b5adda18300989ca784bfb891e7afc11d45d9790/Assets/Models/WesTests/isaac_cave_04012024
mkdir -p local/samples/bab_isaac_cave_04012024
for f in Quill.json Quill.qbin State.json; do curl -fL -o "local/samples/bab_isaac_cave_04012024/$f" "$base/$f"; done
```

The sweep test takes any folder name, so the names above only matter to the three tests in `tests/samples.rs` that pin numbers for the cave, gates and 2017 projects.

## Still to verify on a Windows PC with Quill

Each of these needs a project saved by Quill; a few seconds of work per item. Import the saved folder and read the warnings.

1. A sound layer from a WAV, an OGG and an MP3: which of the three audio layouts Quill writes, and whether 32-bit samples are floats.
2. A positional sound with a cone and with a frustum: the JSON names and units of the parameters.
3. A 360 mono, a 360 stereo and both cubemap pictures: the `Type` strings, and whether a cubemap's record is one flat image.
4. A PNG with transparency: that the record uses pixel format 7, and whether colour is premultiplied by alpha.
5. An FBX with a hierarchy and vertex colours: where `IsModelTopLayer` sits, whether any node has both a mesh and children, and that colours are linear.
6. A layer with a wiggle or blink keep-alive: the JSON, so it can be imported.
7. Opacity keys with each interpolation mode, and Stop, Play and Loop actions on the root.
8. A 2017 project opened and saved by Quill 2.9: what Quill does with `HomePosition`, `TrackingOrigin` and `AnimSpeed`.
