# Exporting from Limner

Limner writes two interchange formats so far: **glTF 2.0 binary (`.glb`)** for anything with hierarchy, animation, lights or cameras, and **Wavefront OBJ (`.obj`)** for a still mesh. USD and FBX are planned and will be written from the same intermediate scene.

The code is in `crates/limner-export`. This page says what each file contains, how each Limner concept is mapped, what is lost, and how to bring the result into Blender, Unity and Unreal.

## How an export is made

Every writer consumes an **export scene** (`limner_export::ExportScene`), built by `build_scene(&Document, &ExportOptions)`. The scene is the document evaluated over a time range: `build_scene` calls the core's `evaluate` at each frame time and records what came out. Writers therefore contain no timeline logic, and easing, pivots, sequence loops, clips and trims are honoured by the one implementation that defines them.

```rust
pub struct ExportOptions {
    pub start: Flicks, pub end: Flicks,   // inclusive; start == end is a still
    pub rate: FrameRate,                  // sampling rate for animation
    pub animation: bool,                  // false: the state at `start` only
    pub include_hidden: bool,             // layers hidden by their eye toggle
    pub bake_transforms: bool,            // flatten the hierarchy into world-space meshes
    pub lights: bool, pub cameras: bool,
}
```

Public entry points:

```rust
export_glb(&doc, &options, &GltfOptions) -> Result<Vec<u8>, ExportError>
write_glb(&doc, path, &options, &GltfOptions) -> Result<ExportReport, ExportError>
export_obj(&doc, time, include_hidden) -> Result<String, ExportError>
write_obj(&doc, path, time, include_hidden) -> Result<ExportReport, ExportError>
```

`export_glb_with_report` and `export_obj_with_report` return the data together with the report; `scene_to_glb` and `scene_to_obj` write a scene you built yourself. `ExportReport` counts nodes, meshes, vertices, triangles, sampled animation frames, animation channels and bytes.

### Rules shared by all formats

| Topic | Rule |
|---|---|
| Units and axes | Metres, +Y up, right-handed. This is Limner's own space, so nothing is converted or rescaled. |
| Sampling | Frame `k` is evaluated at `start + rate.frame_start(k)`, for as long as that is not past `end`. A range of 0 to 10 s at 24 fps gives 241 samples. |
| Hidden layers | A layer whose eye toggle is off is left out with everything below it, unless `include_hidden` is set. Clips always apply: a layer outside its clip is hidden either way. |
| What is exported | Only what is on screen on at least one sampled frame. A drawing nobody sees in the range, and a paint layer with no visible triangle, produce nothing. |
| Geometry | The renderer's tessellation: one cross-section ring per stored point (2 vertices for ribbons and legacy lines, 7 for cylinders and ellipses, 4 for cubes), no resampling, no caps, no welding between strokes. Triangles are counter-clockwise seen from outside. |
| Normals | Always available. Outward for tubes; for ribbons, the side the stored normal points to. |
| Colour | The scene holds linear RGB and opacity per vertex as floats. Each format encodes them as its readers expect (below). |
| Meshes per drawing | Up to four: single- or double-sided (tubes or flat strokes), opaque or see-through. Keeping see-through strokes apart means only they pay for alpha blending. |
| Opacity | The layer's opacity multiplied by that of every ancestor, because no interchange format inherits opacity. |
| Dropped strokes | Strokes erased completely (every point at zero opacity), strokes with fewer than two points, and strokes containing a non-finite number. |

### Baking transforms

With `bake_transforms` the hierarchy is flattened: groups, sounds and spawn areas disappear and every remaining node is a root.

- A paint layer whose world placement is the same on every sampled frame has its vertices moved into world space and no transform at all. If the placement mirrors (an odd number of flips up the tree), the winding of every triangle is reversed so that faces still point outward, and normals go through the inverse transpose.
- A paint layer whose world placement **changes** in the range cannot be turned into static vertices. Its node carries the sampled world transform instead, and its mesh stays in layer space. Nothing is lost; only the hierarchy is.
- Lights, cameras, pictures and models keep their world transform on a root node.

With the hierarchy kept, a mirrored layer keeps its flip in the node transform and the winding is **not** touched. glTF and USD both say that a node with a negative determinant reverses the facing of its triangles, so consumers handle it; pre-reversing would make them face inward.

## glTF 2.0 binary (`.glb`)

One self-contained file: a JSON description and one binary buffer. No axis or unit conversion is needed.

### What is written

| Limner | glTF |
|---|---|
| Document | One scene named after the document title ("Limner scene" if untitled). `scenes[0].extras.limner` records the frame rate, the start time in seconds, the frame count, whether transforms were baked, and the background colour. |
| Layer | One node, named after the layer, with `translation`, `rotation`, `scale`. `extras.limner` holds `layerId` and `kind` (`Group`, `Sequence`, `Paint`, `Picture`, `Model`, `Sound`, `Viewpoint`, `Camera`, `Light`). |
| Layer transform | Rotation and translation as they are; the uniform scale on all three axes. |
| Mirrored layer | A negative scale on the flip axis, for example `[-2, 2, 2]` for a layer scaled by 2 and flipped on X. |
| Group, sequence | An empty node with children. A sequence's own clock is not a glTF concept; its effect is already in the samples of its children. |
| Paint layer showing one drawing | The mesh is on the layer's node. |
| Frame-by-frame paint layer | One child node per distinct drawing, named `Layer.000`, `Layer.001`, ... by the drawing's position in the layer, with `extras.limner.drawingId`. Each child is switched on and off by its `scale` (below). Two frames that show the same drawing share one child. |
| Drawing | One mesh with up to four primitives. `POSITION` (float, with exact `min`/`max`), `NORMAL` (float), `COLOR_0`, and `indices` (16-bit when the mesh has at most 65,535 vertices, otherwise 32-bit). |
| Colour | `COLOR_0` is **linear** RGBA as normalised 16-bit integers. See "Why 16-bit colour". |
| See-through strokes | Their own primitive, with opacity in the alpha of `COLOR_0` and a material with `alphaMode: "BLEND"`. Opaque strokes never use blending. |
| Flat strokes (ribbons, lines) | Their own primitive with a `doubleSided` material. |
| `Shading::Unlit` | A material with `KHR_materials_unlit`: what was painted is what is shown. Its fallback for readers without the extension is a matte, non-metallic PBR surface. |
| `Shading::Lit` | A plain PBR material (non-metallic, fully rough) that takes its colour from the vertices. |
| Layer opacity | `baseColorFactor` alpha of the layer's material, and `alphaMode: "BLEND"` when it is below 1. The value is the effective opacity on the first frame. |
| Light layer | `KHR_lights_punctual`: `point`, `spot` or `directional`, colour, `intensity`, `range`, and for spots `innerConeAngle` and `outerConeAngle`. |
| Camera layer | A perspective camera: `yfov` from the layer's vertical field of view, `aspectRatio` from the document's camera resolution, `znear` 0.01 m, `zfar` 1000 m. |
| Picture, model layer | An empty node whose `extras.limner` carries `media` (file name), `mediaId` and `sourcePath`. The pixels or mesh are not embedded. |
| Sound, spawn area | An empty node carrying its transform and kind. |

Materials are shared: every layer with the same shading, sidedness, blending and opacity uses the same one, so a typical file has between one and four.

`asset.generator` is `Limner <version> (limner-export)`. Every extension that appears is listed in `extensionsUsed`; nothing is in `extensionsRequired`, so any glTF reader opens the file.

### Animation

When `animation` is on and the range holds more than one frame, the file has **one** animation, named after the document. Time zero is `start`.

| Limner | glTF |
|---|---|
| Transform keys, easing, pivots | The evaluated transform of each frame, as `translation`, `rotation` and `scale` channels with `LINEAR` interpolation. Easing and pivots are in the samples, so no reader needs to know Limner's curves. |
| Sequence loops, repeats, trims | Also in the samples: a child of a looping sequence simply repeats in the sampled values. |
| Clips (visibility keys) | The node's `scale` channel, `STEP` interpolation: the layer's real scale while it is in a clip, `[0, 0, 0]` outside. The glTF specification names a scale of zero on all three axes as the way to animate visibility, and it needs no extension. A group's clip hides its children through the hierarchy, so only the group gets a channel. |
| Frame-by-frame drawings | The `scale` channel of each per-drawing child, `STEP`, `[1, 1, 1]` on the frames where that drawing is the one shown and `[0, 0, 0]` otherwise. |
| First frame | The node's static `translation`, `rotation`, `scale` are the state at `start`, so a reader that ignores animation shows the first frame, including which drawings are hidden. |

Three things keep the animation small and well behaved:

- **Constant channels are dropped.** A layer that only slides has a translation channel and nothing else. A layer with keys that never change the value has no channels. There is one deliberate exception: a layer that is rotated and has clips keeps a two-key rotation channel even though the rotation never changes. A node hidden by a zero scale has a matrix with no rotation left in it, and importers that store matrices (Godot's does) showed such a layer unrotated when its clip started. Stating the rotation in the animation fixes that.
- **Runs where nothing changes are merged.** For `LINEAR` channels both ends of a hold are kept, so the hold is not interpolated across; for `STEP` channels one key per change is kept. The first and last frame are always kept, so every channel spans the whole range and all viewers agree on the animation's length.
- **Rotations stay continuous.** Each quaternion is negated if needed to stay on the same hemisphere as the previous sample, so interpolation never goes the long way round.

A `scale` channel that carries visibility is stepped as a whole. So a layer that both changes size and has clips gets a stepped scale (exact on every sampled frame, not interpolated between frames), while its translation and rotation stay linear.

### Optional extensions (`GltfOptions`)

```rust
pub struct GltfOptions {
    pub node_visibility: bool,   // default false
    pub animate_opacity: bool,   // default false
    pub normals: bool,           // default true
}
```

- `node_visibility` additionally writes `KHR_node_visibility` on every node whose visibility changes (or that is hidden throughout), animated through `KHR_animation_pointer` with one unsigned byte per key and `STEP` interpolation. The stepped scale is written as well, so the file still plays correctly in viewers that know neither extension. With the extension, hidden **lights** are really switched off; a scale of zero does not do that.
- `animate_opacity` animates opacity keys through `KHR_animation_pointer` on `/materials/N/pbrMetallicRoughness/baseColorFactor`. A layer whose opacity changes then gets a material of its own, with blending on. Without it, a layer keeps the opacity it has on the first frame.
- `normals: false` leaves out `NORMAL`. Unlit materials do not need normals; files become about a third smaller, at the cost of relighting downstream.

Both pointer extensions are off by default because support is still uneven and some importers reject animation channels they do not understand.

### Why 16-bit colour

`COLOR_0` may be float, normalised 8-bit or normalised 16-bit. Limner stores linear colour, and linear values in 8 bits band visibly in the darks. Floats are exact but cost 16 bytes per vertex. Normalised 16-bit costs 8 bytes, is 256 times finer than 8-bit, and every conforming reader must accept it. The price is that colours brighter than 1.0 are clamped.

### What is lost in glTF

| Limner feature | What happens |
|---|---|
| Dithered see-through strokes | Become ordinary alpha blending. Limner never blends or sorts strokes; a glTF viewer does, so overlapping faint strokes can show sorting artefacts and do not build up the way they do in Limner. |
| Directional strokes | Their fade with viewing direction is dropped. A stroke that is opaque head-on is exported opaque. |
| Opacity keys | Only with `animate_opacity`; otherwise the first frame's opacity. |
| Step keys and loop restarts | Sampled once per frame with linear interpolation, so a jump takes one frame instead of being instant when played back faster than the sampling rate. |
| Keyed light and camera properties (named channels) | The static values are exported. |
| Light energy | Written unchanged as `intensity`. Limner's energy is a unit-less multiplier, not candela or lux, so importers that convert physical units make lights look dimmer or brighter. The original values are in the light's `extras.limner`. |
| Light range under animated scale | `range` is in world metres on the first frame (the layer's range times its world scale), because glTF lights ignore node scale. |
| Spot softness | Mapped to the inner cone angle: `inner = outer * (1 - blend)`. Cones wider than 90 degrees are clamped, as glTF requires. |
| Shadows flag | Kept only in `extras`. |
| Lit shading's wrap lighting, and flat shading of cube strokes | A standard PBR material; cube strokes have smoothed corner normals. |
| Pictures, models, sounds | Not embedded; nodes carry a reference. |
| Stops, chapters and loops on the root timeline | Not exported. The animation is the plain range from `start` to `end`. |
| Colours above 1.0 | Clamped. |
| Layer lock, collapse, bounds display, palette | Editor state; not exported. |

### Importing the `.glb`

**Blender (4.2 or later).** File > Import > glTF 2.0. The hierarchy, vertex colours (attribute `Color`), lights, cameras and the animation arrive as they are. Unlit layers come in as an emission-only material, which is the right look. Frame-by-frame layers play because the scale curves import with constant interpolation; set the scene frame rate to the export rate first, or keys land between frames. For see-through layers set the material's blend mode if the viewport shows them opaque. Lights: pick the importer's unit-less lighting mode or adjust strength by hand, since Limner's energy is not in physical units.

**Unity.** Use glTFast (`com.unity.cloud.gltfast`) or UnityGLTF; Unity has no built-in glTF importer. Drop the file into `Assets`. Both read vertex colours, `KHR_materials_unlit` and `KHR_lights_punctual`, and create a legacy or Mecanim clip from the animation; scale curves keep their stepped tangents, so frame-by-frame plays. Unity is left-handed, and the importers convert for you. For Quill-like rendering use forward rendering with MSAA, as Quill's own Unity guidance recommends.

**Unreal Engine (5.x).** Import the `.glb` through the Interchange glTF importer (File > Import Into Level keeps the hierarchy and the animation; importing as separate static meshes discards the per-drawing nodes). Unreal converts to centimetres and Z-up itself. Check two things after import: that the material of unlit layers samples the vertex colour, and that the scale tracks of frame-by-frame layers use constant interpolation in Sequencer.

**Godot 4.** Drag the file into the project; the scene imports with an `AnimationPlayer`, and `KHR_animation_pointer` channels become tracks too. For every node that is hidden on the first frame the importer logs "Basis ... must be normalized in order to be casted to a Quaternion"; that is its reaction to the zero scale and is harmless, the node imports hidden and plays correctly.

## Wavefront OBJ (`.obj`)

A still: the document at one time, in world space. `export_obj(&doc, time, include_hidden)`.

### What is written

```
# Limner OBJ export
# <document title>
# metres, +Y up, right-handed, triangles counter-clockwise from outside
# vertex colours: v x y z r g b, sRGB-encoded 0..1
o Left_arm
g Left_arm
v 0.1 2.05 -1 1.0000 0.7354 0.0000
vn 0.00000 1.00000 0.00000
f 1//1 2//2 8//8
```

| Limner | OBJ |
|---|---|
| Paint layer | One `o` and one `g` with the same name: the layer name with whitespace replaced by `_`, made unique by appending the layer id when two layers share a name. |
| Drawing | The one drawing the layer shows at `time`. |
| Positions | `v x y z r g b`, world space, full single precision. |
| Colour | Appended to each `v` line, **sRGB-encoded** in 0..1 with four decimals. This is the de facto vertex-colour extension, and Blender and MeshLab treat these numbers as sRGB. Limner stores linear colour, so it is encoded on the way out; a linear 0.5 is written as 0.7354. |
| Normals | One `vn` per vertex. |
| Triangles | `f a//a b//b c//c`, one-based indices running through the whole file. |
| Layer transforms | Baked into the vertices. Under a mirror the winding is reversed so faces point outward. |
| Hidden layers and clips | A layer hidden by its eye toggle is left out unless `include_hidden`; a layer outside its clip at `time` is always left out. |

No `.mtl` file is written; there is nothing for it to say.

### What is lost in OBJ

Everything OBJ cannot hold: animation, hierarchy, opacity (see-through strokes come out solid), the difference between single- and double-sided strokes (enable two-sided rendering for ribbons downstream), lights, cameras, pictures, models and sounds. Colour above 1.0 is clamped.

### Importing the `.obj`

**Blender.** File > Import > Wavefront (.obj). Set Forward to "-Z" and Up to "Y" (the defaults), and the mesh arrives upright at the right size. Vertex colours arrive as a colour attribute; connect a Color Attribute node to an Emission shader to see them. **MeshLab** shows them at once.

**Unity** and **Unreal** import OBJ geometry but ignore the colour columns, so use the `.glb` there.

## Compared with Quill's exporters

Measured on the owner's own exports (see `reports/Quill feature map and tech stack.md`, "What Quill exports, and what the files really contain").

| | Quill 2.9 | Limner |
|---|---|---|
| Normals | None in any format | Always written; can be left out of glTF on request |
| Units | FBX holds metres but declares centimetres | Metres, declared by the format itself (glTF is defined in metres) |
| Transform keys | Not exported in FBX or Alembic; USD has no `xformOp`s even when layers are keyed | Sampled per frame, with easing, pivots and sequence loops |
| Clips and sequence looping | Not exported | Exported as stepped visibility |
| Frame-by-frame animation | Per-frame meshes | One mesh per distinct drawing, switched by a stepped channel; frames that show the same drawing share it |
| Opacity keys | Not exported | Optional, through `KHR_animation_pointer` |
| Mirrored layers | USD keeps the winding wrong under flipped transforms | Negative scale with the hierarchy kept; reversed winding when baked |
| See-through strokes | Mixed into the same mesh as opaque ones | Separate primitives, so opaque paint is never blended |
| Cameras and spawn areas | Produce nothing | Cameras are exported; spawn areas keep a node |
| Lights | Do not exist in Quill | `KHR_lights_punctual` |
| File references | USD uses backslashes, which fail outside Windows | A single self-contained file |
| Faces | Quads | Triangles (what glTF requires; the same surface) |

Quill still exports things Limner does not yet: stroke centre lines as curves, UV sets along and around each stroke, and Alembic's per-stroke attributes.

## What has and has not been verified

Verified:

- **Structure.** Every export in the test suite goes through the validator in `crates/limner-export/tests/support/mod.rs`: container layout and alignment, every accessor inside its view and every view inside the buffer, type and count consistency, exact `POSITION` bounds, unit normals, indices below the vertex count, an acyclic node tree with every node reachable, strictly increasing animation times with matching output counts, pointer targets that resolve, and `extensionsUsed` listing exactly what is used. `tests/validator.rs` damages good files and checks that the validator objects.
- **Geometry and animation values.** Positions against the renderer's `ring_positions`, world placement against `Document::world_from_layer`, animation samples against `evaluate` frame by frame.
- **Against Quill's own exporter.** The owner's sample project exports 52 paint meshes, 485,292 vertices and 948,032 triangles. Quill's own USDZ export of the same project holds 52 meshes, 485,292 points and 474,016 quads, which is the same surface.
- **An independent reader.** The pinned Godot build (4.8 dev 7) loads the sample and fixture files through `GLTFDocument`, with the expected mesh, light, camera and animation track counts, and plays the animation with the expected node transforms.

Not verified:

- The Khronos glTF validator has not been run on these files. The `gltf-validator` npm package is a library without a command-line entry, so it needs a three-line Node script around `validateBytes`; `cargo test -p limner-export --test fixtures -- --ignored` writes artwork-free files to `local/out/export/` to feed it.
- The import steps above for Blender, Unity and Unreal are written from those tools' documented behaviour and have not been run against these files.
- Light and camera layers point along their local -Z axis with +Y up, which is glTF's convention and Godot's. Whether Quill's camera layers use the same axis has not been checked against a real Quill render.
