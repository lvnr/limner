# Exporting from Limner

Limner writes four interchange formats, all from the same intermediate scene:

- **glTF 2.0 binary (`.glb`)**: hierarchy, animation, lights and cameras in one compact file. The first choice where the receiving tool reads it.
- **USD (`.usda` text, or a `.usdz` package)**: the same content for USD pipelines, and the only format here that also carries sound layers.
- **FBX (binary 7.4, `.fbx`)**: the same content for tools that want FBX, with the limits FBX has.
- **Wavefront OBJ (`.obj`)**: a still mesh.

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
export_usda(&doc, &options, &UsdOptions) -> Result<String, ExportError>
export_usda_with_assets(&doc, &options, &UsdOptions) -> Result<UsdaOutput, ExportError>
write_usda(&doc, path, &options, &UsdOptions) -> Result<ExportReport, ExportError>
export_usdz(&doc, &options, &UsdOptions) -> Result<Vec<u8>, ExportError>
write_usdz(&doc, path, &options, &UsdOptions) -> Result<ExportReport, ExportError>
export_fbx(&doc, &options, &FbxOptions) -> Result<Vec<u8>, ExportError>
write_fbx(&doc, path, &options, &FbxOptions) -> Result<ExportReport, ExportError>
```

Each `export_*` has an `export_*_with_report` twin that returns the data together with the report; `scene_to_glb`, `scene_to_obj`, `scene_to_usda`, `scene_to_usda_with_assets`, `scene_to_usdz` and `scene_to_fbx` write a scene you built yourself. `ExportReport` counts nodes, meshes, vertices, triangles, sampled animation frames, animation channels, sounds written and bytes, and lists **warnings**: what was left out and why, in plain words (sounds a format cannot hold, audio a package may not contain). An export with warnings has still succeeded. `limner export` prints them; the editor's `export_file` returns them as `warnings`.

The `write_*` functions write each file whole or not at all: the bytes go to a hidden file beside the target (`.<name>.<process>-<n>.part`), which is flushed to disk and then renamed over it. A failure part way, such as a full disk, leaves an earlier export of the same name as it was.

### Rules shared by all formats

| Topic | Rule |
|---|---|
| Units and axes | Metres, +Y up, right-handed. This is Limner's own space, so nothing is converted or rescaled. |
| Sampling | Frame `k` is evaluated at `start + rate.frame_start(k)`, for as long as that is not past `end`. A range of 0 to 10 s at 24 fps gives 241 samples. At most `MAX_FRAMES` (100,000) frames: every frame evaluates the whole document, so a slip such as 24,000 fps is refused with `ExportError::TooManyFrames` instead of keeping the machine busy for minutes. |
| Hidden layers | A layer whose eye toggle is off is left out with everything below it, unless `include_hidden` is set. Clips always apply: a layer outside its clip is hidden either way. This holds for sound layers too, although in Limner the eye toggle of a sound only hides its gizmo. |
| Opacity zero | Limner does not draw a layer at opacity zero, so a frame on which a layer's effective opacity is zero does not count as on screen. A layer at opacity zero on every sampled frame is left out with everything below it, except lights and cameras (which ignore opacity) and the groups that place them. |
| What is exported | Only what is on screen on at least one sampled frame. A drawing nobody sees in the range, and a paint layer with no visible triangle, produce nothing. |
| Geometry | The renderer's tessellation: one cross-section ring per stored point (2 vertices for ribbons and legacy lines, 7 for cylinders and ellipses, 4 for cubes), no resampling, no caps, no welding between strokes. Triangles are counter-clockwise seen from outside. |
| Normals | Always available. Outward for tubes; for ribbons, the side the stored normal points to. |
| Colour | The scene holds linear RGB and opacity per vertex as floats. Each format encodes them as its readers expect (below): linear in glTF and USD, sRGB-encoded in OBJ and, by default, in FBX. |
| Meshes per drawing | Up to four: single- or double-sided (tubes or flat strokes), opaque or see-through. Keeping see-through strokes apart means only they pay for alpha blending. |
| Opacity | The layer's opacity multiplied by that of every ancestor, because no interchange format inherits opacity. Where a format holds one opacity per layer (glTF without `animate_opacity`, USD, FBX), it is the **largest** the layer has in the range, so a layer that fades in is shown at its fullest rather than invisible; for a layer whose opacity does not change, that is simply its opacity. |
| Sounds | A sound layer carries its audio (the file as it was imported, or WAV written from the samples the document holds) and the stretches of the range in which it is heard, worked out the way Limner's player does it: from the frame its clip starts (or the range starts) until its clip ends, its sequence loops back, or, for WAV audio that does not loop, the audio runs out. Only USD can hold sound; the other formats leave it out with a warning (below). |
| Dropped strokes | Strokes erased completely (every point at zero opacity), strokes with fewer than two points, and strokes containing a non-finite number. |

### Baking transforms

With `bake_transforms` the hierarchy is flattened: groups and spawn areas disappear and every remaining node is a root.

- A paint layer whose world placement is the same on every sampled frame has its vertices moved into world space and no transform at all. If the placement mirrors (an odd number of flips up the tree), the winding of every triangle is reversed so that faces still point outward, and normals go through the inverse transpose.
- A paint layer whose world placement **changes** in the range cannot be turned into static vertices. Its node carries the sampled world transform instead, and its mesh stays in layer space. Nothing is lost; only the hierarchy is.
- Lights, cameras, pictures, models and sounds keep their world transform on a root node.

With the hierarchy kept, a mirrored layer keeps its flip in the node transform and the winding is **not** touched. glTF and USD both say that a node with a negative determinant reverses the facing of its triangles, and FBX importers treat a negative scale the same way, so consumers handle it; pre-reversing would make them face inward.

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
| Layer opacity | `baseColorFactor` alpha of the layer's material, and `alphaMode: "BLEND"` when it is below 1. The value is the largest effective opacity in the range; with `animate_opacity`, the first frame's, which the animation then changes. |
| Light layer | `KHR_lights_punctual`: `point`, `spot` or `directional`, colour, `intensity`, `range`, and for spots `innerConeAngle` and `outerConeAngle`. |
| Camera layer | A perspective camera: `yfov` from the layer's vertical field of view, `aspectRatio` from the document's camera resolution, `znear` 0.01 m, `zfar` 1000 m. |
| Picture, model layer | An empty node whose `extras.limner` carries `media` (file name), `mediaId` and `sourcePath`. The pixels or mesh are not embedded. |
| Sound layer | The same: an empty node with the audio's file name. The sound itself is left out, with a warning naming the layers (see "Why glTF has no sound"). |
| Spawn area | An empty node carrying its transform and kind. |

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
| Opacity keys | Only with `animate_opacity`; otherwise the largest opacity in the range. |
| Step keys and loop restarts | Sampled once per frame with linear interpolation, so a jump takes one frame instead of being instant when played back faster than the sampling rate. |
| Keyed light and camera properties (named channels) | The static values are exported. |
| Light energy | Written unchanged as `intensity`. Limner's energy is a unit-less multiplier, not candela or lux, so importers that convert physical units make lights look dimmer or brighter. The original values are in the light's `extras.limner`. |
| Light range under animated scale | `range` is in world metres on the first frame (the layer's range times its world scale), because glTF lights ignore node scale. |
| Spot softness | Mapped to the inner cone angle: `inner = outer * (1 - blend)`. Cones wider than 90 degrees are clamped, as glTF requires. |
| Shadows flag | Kept only in `extras`. |
| Lit shading's wrap lighting, and flat shading of cube strokes | A standard PBR material; cube strokes have smoothed corner normals. |
| Pictures, models | Not embedded; nodes carry a reference. |
| Sounds | Left out, with a warning; the node keeps the audio's file name. |
| Stops, chapters and loops on the root timeline | Not exported. The animation is the plain range from `start` to `end`. |
| Colours above 1.0 | Clamped. |
| Layer lock, collapse, bounds display, palette | Editor state; not exported. |

### Why glTF has no sound

glTF has no ratified extension for sound. Checked in October 2026: Khronos's extension registry lists no audio extension as ratified or as a release candidate; its only audio entry is `KHR_audio_graph`, at the proposal stage. `KHR_audio_emitter`, the merger of `MSFT_audio_emitter` and the Open Metaverse group's `OMI_audio_emitter`, is still an open pull request (KhronosGroup/glTF #2137, opened in 2022): its node property was renamed in April 2026, and reviews in August and September 2026 asked for changes to the channel rules, playback control and the formats it allows (MP3 only, with Opus proposed). `OMI_audio_emitter` is the draft it replaces. Writing either would give files that today's readers ignore and tomorrow's may read differently, so the `.glb` holds no sound and the export says which sound layers were left out. When `KHR_audio_emitter` reaches release-candidate status it can be written behind a `GltfOptions` switch: the audio in the binary buffer, sources from gain and looping, emitters on the sound nodes with the cone and distance from the layer, listed in `extensionsUsed` only.

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
| Hidden layers and clips | A layer hidden by its eye toggle is left out unless `include_hidden`; a layer outside its clip at `time`, or at opacity zero then, is always left out. |

No `.mtl` file is written; there is nothing for it to say.

### What is lost in OBJ

Everything OBJ cannot hold: animation, hierarchy, opacity (see-through strokes come out solid), the difference between single- and double-sided strokes (enable two-sided rendering for ribbons downstream), lights, cameras, pictures, models and sounds. Sound layers heard at `time` are named in a warning. Colour above 1.0 is clamped.

### Importing the `.obj`

**Blender.** File > Import > Wavefront (.obj). Set Forward to "-Z" and Up to "Y" (the defaults), and the mesh arrives upright at the right size. Vertex colours arrive as a colour attribute; connect a Color Attribute node to an Emission shader to see them. **MeshLab** shows them at once.

**Unity** and **Unreal** import OBJ geometry but ignore the colour columns, so use the `.glb` there.

## USD (`.usda` and `.usdz`)

A `.usda` file is one USD layer as text. A `.usdz` file is a package: a single file that holds that same layer. USD can be told to use exactly Limner's space (metres, +Y up, right-handed), so nothing is converted or rescaled.

### What is written

| Limner | USD |
|---|---|
| Document | Layer metadata: `upAxis = "Y"`, `metersPerUnit = 1`, `defaultPrim`, `timeCodesPerSecond` and `framesPerSecond` (the sampling rate), `doc` (the generator), and `customLayerData.limner` with the title, frame rate, start time in seconds, frame count, whether transforms were baked, and the background colour. |
| Default prim | One `Xform` that holds everything, named after the document title ("Limner_scene" if untitled). It lets another USD file reference the export as one thing. |
| Layer | One prim under the default prim, in the layer tree's shape. `Xform` for groups, sequences, paint layers, pictures, models, sounds and spawn areas; a light or camera prim for those layers. The document's root layer is a prim too (`/Scene/Root/...`). |
| Layer name | USD prim names may hold only ASCII letters, digits and `_`, and may not start with a digit. Other characters become `_` ("Left arm" is `Left_arm`); a name that clashes with a sibling gets the layer id appended. The real name is kept in `customData.limner.name`, beside `layerId` and `kind`. |
| Layer transform | `xformOp:translate` (`double3`), `xformOp:orient` (`quatf`, real part first as USD writes quaternions) and `xformOp:scale` (`float3`), listed in `xformOpOrder`. An op that would do nothing is left out, so a layer that only moves has only a translate op. |
| Mirrored layer | A negative scale on the flip axis, for example `(-2, 2, 2)`. |
| Paint layer showing one drawing | `Mesh` prims directly under the layer's prim, one per kind of surface: `Tubes`, `Flat`, `TubesSeeThrough`, `FlatSeeThrough`. |
| Frame-by-frame paint layer | One child `Xform` per distinct drawing, named `Layer_000`, `Layer_001`, ... by the drawing's position in the layer, with `drawing`, `drawingId` and `layerId` in `customData.limner`. Its `Mesh` prims are inside. Each child is switched on and off by `visibility` (below). |
| Mesh | `points`, `faceVertexCounts` (all 3), `faceVertexIndices`, `normals` (interpolation `vertex`), `extent` (the exact bounding box), `subdivisionScheme = "none"` so nothing smooths the surface, `doubleSided`, `orientation`. |
| Winding | `orientation = "rightHanded"`. That is USD's name for "the front is the side from which the corners run counter-clockwise", which is how Limner's triangles are ordered. It stays `rightHanded` under a mirrored layer: USD itself turns the facing round under a transform that mirrors. A baked export arrives at the writer with its winding already reversed where needed. |
| Colour | `primvars:displayColor`, interpolation `vertex`, **linear** floats as painted. USD defines `displayColor` as linear, so nothing is encoded and colours above 1.0 are kept. |
| Opacity | `primvars:displayOpacity`, interpolation `vertex`: each vertex's own opacity multiplied by the layer's largest effective opacity in the range. USD has no other place for a layer's opacity that viewers agree on. |
| Flat strokes (ribbons, lines) | `doubleSided = 1`. Tubes have `doubleSided = 0`. |
| Materials | A `Scope` named `Materials` beside the layers, holding up to four `Material` prims: `unlit`, `unlit_see_through`, `lit`, `lit_see_through`. Each mesh is bound to one (`material:binding`, with `MaterialBindingAPI` applied). |
| Light layer | UsdLux. Point: `SphereLight` with `treatAsPoint = 1`. Spot: the same with `ShapingAPI`, `inputs:shaping:cone:angle` (degrees off the axis, as in Limner) and `inputs:shaping:cone:softness` (the blend). Directional: `DistantLight`. All have `inputs:color`, `inputs:intensity` and `inputs:shadow:enable` (`ShadowAPI`). |
| Camera layer | A `Camera` prim: `projection = "perspective"`, `verticalAperture = 24`, `horizontalAperture` = 24 times the aspect ratio of the document's camera resolution, `focalLength` chosen so the vertical field of view is the layer's, `clippingRange = (0.01, 1000)`. |
| Picture, model layer | An `Xform` whose `customData.limner` carries `media` (file name), `mediaId` and `sourcePath`. The pixels or mesh are not embedded. |
| Sound layer | An `Xform` carrying its transform, with one UsdMedia `SpatialAudio` prim inside for each stretch of the range in which the sound is heard (below). Its `customData.limner` keeps what `SpatialAudio` has no place for: `soundKind`, `gain`, `looped`, `soundAttenuation` (`none`, `linear`, `logarithmic`) with `soundAttenuationMin` and `soundAttenuationMax` (layer units), `soundModifier` (`none`, `cone`, `frustum`) with its angles in radians, and `media`, `mediaId`, `sourcePath`. |
| Spawn area | An `Xform` carrying its transform and kind. |

Lights and cameras point along their local -Z axis with +Y up in USD, as in Limner, so their prims carry the layer's rotation unchanged.

### The materials

Many viewers show `displayColor` only on meshes that have no material; others show nothing sensible without one. So every mesh is bound to a small `UsdPreviewSurface` material that reads the vertex colours through a `UsdPrimvarReader_float3` named `Color`:

- **Unlit layers** (the default, and every imported Quill piece): the colour goes to `emissiveColor`, and `diffuseColor` is black, with the specular colour black as well (`useSpecularWorkflow = 1`). A preview surface has no "unlit" switch. Emitting the paint and leaving nothing for lights to act on is the closest it gets: the stroke shows the colour that was painted whatever lights are in the scene, and is not brightened a second time by them.
- **Lit layers**: the colour goes to `diffuseColor`, with `roughness = 1` and `metallic = 0`: a matte surface that scene lights shade.
- **See-through** variants also connect `opacity` to a `UsdPrimvarReader_float` that reads `displayOpacity`. Opaque paint uses the variants without it, so it is never blended.

`UsdOptions { materials: false, .. }` leaves the materials and bindings out, for pipelines that want bare `displayColor`.

### Sounds

Each sound layer's prim holds a `SpatialAudio` prim named `Audio`, or `Audio_000`, `Audio_001`, ... when the sound is heard in more than one stretch (a clip that starts again, a sequence that loops back). The prims follow the UsdMedia schema:

| Property | Value |
|---|---|
| `uniform asset filePath` | `@audio/<file>@`: the audio, relative to the layer. The file name is the media item's, made safe (ASCII letters, digits, `-` and `_`) with the extension of the encoding; two layers playing the same media share one file. |
| `uniform token auralMode` | `spatial` for positional sounds; `nonSpatial` for flat and ambisonic ones (an ambisonic recording surrounds the listener and has no single place). |
| `uniform timecode startTime` | The first frame on which the sound is heard, in the layer's time codes (frame `k` is time code `k`). |
| `uniform double mediaOffset` | Seconds into the audio on that frame: the clip's trim, or how far the clip had run when the range began. For a looping WAV sound it is reduced to a place inside the audio. |
| `uniform token playbackMode` | `onceFromStart` or `loopFromStart` when the sound is still heard on the last frame of the range, so it goes on as it would in Limner; `onceFromStartToEnd` or `loopFromStartToEnd` when it stops earlier. |
| `uniform timecode endTime` | Only with the `...ToEnd` modes: the first frame on which the sound is no longer heard. |
| `double gain` | Limner's gain times the layer's effective opacity, as Limner's player does it, with time samples when the opacity changes. |

**Where the audio goes.** `write_usda` writes the files into a folder named `audio` beside the `.usda`, replacing files of the same name there; `export_usda_with_assets` returns them in memory. A `.usdz` holds them after the root layer, each aligned like every file in the package.

**What a package may hold.** The USDZ specification allows only M4A, MP3 and WAV audio. Limner's sounds arrive as WAV, MP3 or Ogg Vorbis (audio the document holds as samples is written as WAV), so an Ogg sound is left out of a `.usdz` with a warning that names the layer; its prim stays, with no `SpatialAudio` and so no reference to a missing file. A `.usda` keeps Ogg sounds, since USD itself does not restrict audio formats, but players may not read them. `usdchecker --arkit` rejects a package that holds an Ogg file (`UnsupportedFileExtensionInPackage`; tried by hand), which is why the writer does not try.

**What is lost.** UsdMedia has no fall-off with distance and no directional cone, so a positional sound is heard at its place with the player's own rules; the attenuation and the cone or frustum are kept in `customData` only. The exact moment a clip ends is rounded to the next sampled frame. Gain above 1 is written as it is; USD does not say whether players amplify.

### Animation

When `animation` is on and the range holds more than one frame, changing values are written as time samples. Time code `k` is sampled frame `k`, so codes start at 0 at `start`; `startTimeCode = 0` and `endTimeCode` is the last frame. A still has no time samples and no start or end code.

| Limner | USD |
|---|---|
| Transform keys, easing, pivots, sequence loops | Time samples on `xformOp:translate`, `xformOp:orient` and `xformOp:scale`: the evaluated transform of each frame. |
| Clips (visibility keys) | Time samples on `visibility`: `inherited` while the layer is in a clip, `invisible` outside. USD has real visibility, and a hidden prim hides everything below it, so nothing is scaled to zero here and a group's clip needs samples on the group only. |
| Frame-by-frame drawings | `visibility` time samples on each per-drawing child. |
| First frame | Every sampled property also has a plain value, the state at `start`, so a reader that ignores time samples shows the first frame. |

As in the glTF writer, constant properties get no samples, runs where nothing changes are merged (both ends of a hold are kept for transforms, which USD interpolates linearly; one sample per change for `visibility`, which USD holds), the last frame is always sampled, and consecutive quaternions stay on one hemisphere.

### The `.usdz` package

The package follows the USDZ rules exactly: a zip archive, every file stored without compression, no encryption, and every file's data starting on a multiple of 64 bytes from the start of the archive. The padding that achieves the alignment is an extra field in the file's local header (record id `0x1986`, as OpenUSD's own writer does it). The first file is the root layer, named after the default prim, for example `Lantern.usda`; the audio files of its sounds follow under `audio/`. A document without sound gives a package with the root layer alone. The zip is written by hand, CRC-32 included; dates in it are fixed, so the same document always gives the same bytes.

**The root layer is text (`usda`).** The USDZ specification allows that, but some consumers accept only binary `usdc` layers; older versions of Apple's AR Quick Look are the known case. A binary `usdc` writer is future work. Until then, on a Mac the layer can be converted with Apple's own tools: `usdcat scene.usda -o scene.usdc`, then `usdzip scene.usdz scene.usdc`.

`usdz_package(&[(name, bytes), ...])` is public and packs any list of files by the same rules.

### Options (`UsdOptions`)

```rust
pub struct UsdOptions {
    pub normals: bool,     // default true
    pub materials: bool,   // default true
}
```

### What is lost in USD

Everything in the glTF list that concerns the scene itself applies here too: dithered see-through strokes become alpha blending, the directional fade is dropped, step keys are sampled once per frame, keyed light and camera properties export their static values, pictures and models are references, and stops, chapters and editor state are not exported. Sounds are kept (see "Sounds" above, for what of them is lost). In addition, or differently:

| Limner feature | What happens |
|---|---|
| Opacity keys | Not exported. Opacity is in the vertices, at its largest value in the range. |
| A drawing shown by two layers | Written twice, once under each layer. (USD could share it with a reference; plain copies are read by every consumer, including simple ones.) |
| Light range | UsdLux lights have no cut-off distance. `range` and the range in world metres are kept in `customData.limner`. |
| Light energy | Written unchanged as `inputs:intensity`, with `inputs:normalize = 1` so it does not depend on the light's radius. Limner's energy is not a physical unit, so renderers that use physical units show lights dimmer or brighter. |
| Layer names | Cleaned to USD identifiers; the real name is in `customData`. |
| File size | Text is large: the owner's sample is 75 MiB as `.usda` or `.usdz` against 23 MiB as `.glb`. |

Colours above 1.0, which glTF clamps, are kept.

### Importing the `.usda` or `.usdz`

**Blender (4.2 or later).** File > Import > Universal Scene Description. Blender composes the file with the OpenUSD library, so the hierarchy, transforms, time samples, visibility, lights and cameras come in as USD defines them. `displayColor` arrives as a colour attribute. Whether the imported material shows it depends on the version's support for `UsdPrimvarReader_float3`; if strokes come in black or white, connect a Color Attribute node named `displayColor` to the material's emission.

**Unity.** Use the USD Importer package (`com.unity.importer.usd`); Unity has no built-in USD support. For artwork, the `.glb` is the better-travelled route into Unity.

**Unreal Engine (5.x).** Enable the USD Importer plugin and open the file through a USD Stage actor (Window > Virtual Production > USD Stage) or File > Import Into Level. Unreal converts to centimetres and Z-up from the layer's `metersPerUnit` and `upAxis`.

**macOS.** `usdchecker`, `usdcat`, `usdtree` and `usdrecord` in `/usr/bin` read both files; `usdrecord --camera Shot scene.usda out.png` renders a frame through an exported camera.

## FBX (`.fbx`)

Binary FBX 7.4 (version 7400). FBX has no public specification, so the writer keeps to the subset that every importer is known to read, and follows the layout of Blender's exporter where a choice had to be made. The binary form is not optional: Blender does not read ASCII FBX. Arrays are stored raw (uncompressed), which every reader accepts and which needs no compression code. Version 7.4 is the newest with 32-bit offsets, so a file must stay below 4 GiB.

The file's creation date and id are fixed values, so the same document always gives the same bytes.

### Units and axes

**Axes.** `GlobalSettings` declares +Y up, +Z to the front, +X to the right: a right-handed space, the same as Limner's (and Maya's default). Coordinates are written as they are. Importers for Z-up or left-handed tools (Blender, Unreal, Unity) convert from the declaration.

**Units.** FBX's native unit is the centimetre, and a file says how many centimetres one of its units is (`UnitScaleFactor`). By default Limner writes its coordinates unchanged, in metres, and declares `UnitScaleFactor = 100`: "one unit in this file is 100 cm". What each importer does with that (only the Godot row has been run; the others are those tools' documented behaviour):

| Importer | With the default (`FbxUnit::Metres`) |
|---|---|
| Blender | Honours the declared unit ("Apply Unit" is on by default): one Limner metre is one Blender metre, with object scales of 1. |
| Unity | Honours it ("Convert Units" is on by default): file scale 1, one metre is one Unity unit. |
| Godot (ufbx) | Honours it: one metre is one unit. |
| Unreal, Interchange FBX importer | Converts to centimetres from the declared unit: one metre becomes 100 Unreal units, which is one metre. |
| Unreal, classic FBX importer | Reads the numbers as centimetres unless "Convert Scene Unit" is ticked in the import dialog, where it is off by default. Tick it, or the artwork arrives 100 times too small. |

For pipelines that cannot tick that box, `FbxUnit::Centimetres` multiplies every length by 100 (vertices, translations, light reach, camera clip planes) and declares `UnitScaleFactor = 1`, FBX's own unit. Every importer then gets the right size without settings, at the price that Blender scales the top-level objects by 0.01 instead of leaving them at 1.

This is the reverse of Quill's own FBX, which holds metres but declares centimetres, so every importer reads it 100 times too small.

### What is written

| Limner | FBX |
|---|---|
| Document | `FBXHeaderExtension` (creator, title), `GlobalSettings` (axes, unit, frame rate, time span), `Documents`, `Definitions` (object counts), `Objects`, `Connections`, `Takes`. |
| Layer | One `Model`, connected to its parent's model (or to the scene root). Sub-type `Null` for groups, sequences, pictures, models, sounds, spawn areas and frame-by-frame paint layers; `Mesh` for a paint layer showing one drawing; `Light`; `Camera`. |
| Layer transform | `Lcl Translation`, `Lcl Rotation` (Euler angles in degrees, order XYZ), `Lcl Scaling`. `InheritType` is 1 on every model, which makes a parent apply to its child as a plain matrix product; FBX's default differs under a mirrored parent. |
| Mirrored layer | A negative `Lcl Scaling` on the flip axis. The winding is not touched. |
| Layer identity | User properties on the model: `limnerKind`, `limnerLayerId`; on pictures and models also `limnerMedia`, `limnerSourcePath`, `limnerMediaId`. Blender shows these as custom properties. |
| Drawing | One `Geometry`: all the drawing's surfaces joined into one vertex list. `Vertices` (doubles), `PolygonVertexIndex` (triangles; the last corner of each is stored as its bitwise complement, FBX's end-of-polygon mark). Two layers that show the same drawing share one geometry. |
| Normals | `LayerElementNormal`, mapped `ByVertice`, `Direct`: one normal per vertex, which is what Limner has. |
| Colour | `LayerElementColor` named `Color`, RGBA doubles, mapped `ByPolygonVertex` with `IndexToDirect`: every polygon corner points into a colour list through `ColorIndex`. This is the layout Blender's own exporter writes, so it is the one every importer (Blender's, Unity's, Unreal's) has met most often; the per-vertex layout (`ByVertice`) is rarer for colours. The index is the vertex index, so each vertex's colour is still stored once. |
| Colour encoding | **sRGB-encoded** RGB by default, as in the OBJ writer. FBX does not say what colour numbers mean; Blender's importer assumes sRGB by default, and Unity and Unreal keep vertex colours in 8 bits per channel, where linear values band in the darks. `FbxColorSpace::Linear` writes Limner's linear values unchanged (and unclamped) instead. Alpha is never encoded. |
| Opacity | The alpha of the vertex colour: each vertex's own opacity multiplied by the layer's largest effective opacity in the range. |
| Kinds of surface | One material slot per kind in the drawing (`LayerElementMaterial`, `ByPolygon`, or `AllSame` when there is one), and one `Material` connected to the model per slot. |
| Materials | Named `unlit` or `lit`, then `double-sided`, then `see-through`: for example `unlit double-sided see-through`. Shared by every layer with the same look, so a file has at most eight. An FBX material cannot say "unlit", "two-sided", "blend" or "use the vertex colours", so each is a plain white Lambert whose **name** says what to set up after import; the same facts are in the user properties `limnerShading`, `limnerDoubleSided` and `limnerSeeThrough`. |
| Frame-by-frame paint layer | A `Null` model for the layer and one child `Mesh` model per distinct drawing, named `Layer.000`, `Layer.001`, ... as in the `.glb`, with `limnerDrawing`, `limnerDrawingId` and `limnerLayerId`. Each child is switched on and off (below). |
| Light layer | A `NodeAttribute` of type `Light`: `LightType` (point, directional, spot), `Color`, `Intensity` (a percentage: energy times 100), `DecayType` 2 (inverse square), `CastShadows`, for point and spot lights `FarAttenuationEnd` (the range in world units), for spots `OuterAngle` (the whole cone, twice Limner's angle) and `InnerAngle` (`outer * (1 - blend)`). The original `limnerEnergy` and `limnerRange` are user properties on the model. |
| Camera layer | A `NodeAttribute` of type `Camera`: the lens as `FocalLength` in millimetres over a 24 mm high film back (`FilmHeight` and `FilmWidth` in inches, `ApertureMode` 3), `FieldOfView`, `FieldOfViewX` and `FieldOfViewY` repeating it as angles, `AspectWidth` and `AspectHeight` from the document's camera resolution, `NearPlane` 0.01 m, `FarPlane` 1000 m. |
| Picture, model, sound, spawn area | A `Null` model with the user properties above. Nothing is embedded. Sound layers are named in a warning. |

**Lights and cameras are turned.** An FBX light shines along its local -Y and an FBX camera looks along its local +X, where Limner (and glTF and USD) use -Z for both. The writer adds a quarter turn to the rotation of those models (about X for lights, about Y for cameras), so they point the right way in every importer; importers that use -Z themselves, such as Blender and Godot, take the quarter turn out again.

### Animation

When `animation` is on and the range holds more than one frame, and anything changes, the file has one `AnimationStack` with one `AnimationLayer`, both named after the document. Time zero is `start`. FBX counts time in units of 1/46,186,158,000 of a second; frame times of all common rates are whole numbers of them.

| Limner | FBX |
|---|---|
| Transform keys, easing, pivots, sequence loops | An `AnimationCurveNode` each for `Lcl Translation`, `Lcl Rotation` and `Lcl Scaling`, with one `AnimationCurve` per axis: the evaluated transform of each frame, with linear keys. |
| Rotation | Euler angles in degrees. Of the many angle triples that mean the same rotation, each frame uses the one nearest the frame before, so a curve never jumps by 180 or 360 degrees; a layer that turns twice ends at 720 degrees. |
| Clips (visibility keys) | Written **twice**. First as a `Visibility` curve (1 or 0, constant keys), which is FBX's own way and hides children too (`Visibility Inheritance`). Second as the `Lcl Scaling` curve going to `(0, 0, 0)` while hidden, the same trick the `.glb` uses, because many importers ignore `Visibility`: Godot's does, and in others it depends on an import setting. |
| Frame-by-frame drawings | A `Visibility` curve and a `Lcl Scaling` curve (1 or 0) on each per-drawing child. |
| First frame | Each model's properties hold the state at `start`, including a `Visibility` of 0 and a scale of zero for what is hidden then. |

Constant properties get no curves, and runs where nothing changes are merged. Unlike in the `.glb`, **both ends of every hold are kept as keys even on stepped curves**. Stepped keys are marked constant, but an importer may ignore that mark and interpolate linearly between whatever keys it finds. With a key on the last frame before a change and one on the frame of the change, such an importer still shows the right value on every frame; it only ramps within the one frame in between.

### Options (`FbxOptions`)

```rust
pub struct FbxOptions {
    pub unit: FbxUnit,               // Metres (default) or Centimetres
    pub color_space: FbxColorSpace,  // Srgb (default) or Linear
    pub normals: bool,               // default true
}
```

### What is lost in FBX

The glTF list applies here as well (blending instead of dithering, no directional fade, sampled step keys, static light and camera properties, media by reference, no stops or chapters). In addition, or differently:

| Limner feature | What happens |
|---|---|
| Unlit shading, sidedness, blending | Not expressible in an FBX material. They are in the material's name and user properties, to be set up once per material after import. |
| Vertex colour in the material | Not expressible either: after import, the material has to be told to use the vertex colour (below). |
| Opacity keys | Not exported. Opacity is in the vertex alpha, at its largest value in the range. |
| Sounds | Left out, with a warning. The FBX SDK has audio objects, but no importer this page covers reads them, and the format has no public description of them to write from. |
| Colours above 1.0 | Clamped with the default sRGB encoding; kept with `FbxColorSpace::Linear`. |
| Light colour space | Written as Limner's linear numbers. FBX does not say whether light colours are linear, and importers differ (Godot shows them unconverted). |
| Light range | `FarAttenuationEnd` is written, but most importers ignore it. |
| Camera and light under a mirrored layer | The flip is carried through the quarter turn correctly, but importers differ on mirrored cameras; avoid mirroring a camera. |
| Quaternion rotation | Euler angles, interpolated per axis between keys. With a key on every frame that changes, the difference from Limner's own interpolation is nil on frames and tiny between them. |
| File size | Doubles and no compression: the owner's sample is 59 MiB against 23 MiB as `.glb`. |

### Importing the `.fbx`

**Blender.** File > Import > FBX. Leave "Apply Unit" on. Vertex colours arrive as the colour attribute `Color`; the importer's "Vertex Colors" setting should stay on sRGB (the default) unless the file was written with `FbxColorSpace::Linear`. Connect a Color Attribute node to an Emission shader for the materials named `unlit`, and to Base Color for `lit`; turn backface culling off for `double-sided` and blending on for `see-through`. Frame-by-frame layers play through the scale curves. Set the scene frame rate to the export rate. Blender 4.5 and later read FBX through the ufbx library, the same one Godot uses.

**Unity.** Drop the file into `Assets`. Leave "Convert Units" on. Vertex colours import as mesh colours; assign an unlit shader that shows them (in a project set to linear colour space the shader has to convert them from sRGB, which is how they are written by default). Use a two-sided variant for the `double-sided` materials. The scale curves play frame-by-frame layers; the model importer's "Import Visibility" setting decides whether the `Visibility` curves are used as well.

**Unreal Engine (5.x).** With the classic FBX importer, tick "Convert Scene Unit" (or export with `FbxUnit::Centimetres`) and set "Vertex Color Import Option" to Replace. File > Import Into Level keeps the hierarchy; importing as static meshes keeps the geometry and colours only, and the animation of plain nodes comes in through Sequencer's FBX import rather than with the meshes. Make one material that feeds the Vertex Color node into Emissive Color (for `unlit`) or Base Color (for `lit`), and set Two Sided where the name says `double-sided`.

**Godot 4.** Drag the file into the project. Meshes, vertex colours, material slots by name, lights, cameras and the animation arrive; the `Visibility` curves are ignored and the scale curves do the switching. For every node that is hidden on the first frame the importer logs "Basis ... must be normalized in order to be casted to a Quaternion", as it does for the `.glb`; it is harmless.

## Compared with Quill's exporters

Measured on exports Quill 2.9 made of the owner's own project.

| | Quill 2.9 | Limner |
|---|---|---|
| Normals | None in any format | Always written; can be left out of glTF, USD and FBX on request |
| Units | FBX holds metres but declares centimetres | Metres, and every file says so: glTF is defined in metres, USD has `metersPerUnit = 1`, FBX has `UnitScaleFactor = 100` (or real centimetres on request) |
| Transform keys | Not exported in FBX or Alembic; USD has no `xformOp`s even when layers are keyed | Sampled per frame, with easing, pivots and sequence loops |
| Clips and sequence looping | Not exported | Exported as stepped visibility: a zero scale in glTF, `visibility` in USD, both in FBX |
| Frame-by-frame animation | Per-frame meshes | One mesh per distinct drawing, switched by a stepped channel; frames that show the same drawing share it |
| Opacity keys | Not exported | Optional in glTF, through `KHR_animation_pointer`; not in USD or FBX |
| Mirrored layers | USD keeps the winding wrong under flipped transforms | Negative scale with the hierarchy kept; reversed winding when baked |
| See-through strokes | Mixed into the same mesh as opaque ones | Separate primitives, meshes or material slots, so opaque paint is never blended |
| Materials | None in USD; optional single- and double-sided Lambert pairs per layer in FBX | Shared by look: unlit or lit, single- or double-sided, opaque or see-through. In USD they read the vertex colours |
| Cameras and spawn areas | Produce nothing | Cameras are exported; spawn areas keep a node |
| Lights | Do not exist in Quill | `KHR_lights_punctual`, UsdLux, FBX light attributes |
| File references | USD uses backslashes, which fail outside Windows | A single self-contained file in every format; the `.usdz` holds one layer and references nothing |
| USD layout | A text root layer plus one binary layer per paint layer | One text layer; no binary (`usdc`) writer yet |
| FBX version | Binary 7500, written with the FBX SDK | Binary 7400, written by hand; arrays uncompressed |
| Faces | Quads | Triangles (what glTF requires; the same surface) |

Quill still exports things Limner does not yet: stroke centre lines as curves, UV sets along and around each stroke, and Alembic's per-stroke attributes.

## What has and has not been verified

Verified:

- **glTF structure.** Every `.glb` in the test suite goes through the validator in `crates/limner-export/tests/support/mod.rs`: container layout and alignment, every accessor inside its view and every view inside the buffer, type and count consistency, exact `POSITION` bounds, unit normals, indices below the vertex count, an acyclic node tree with every node reachable, strictly increasing animation times with matching output counts, pointer targets that resolve, and `extensionsUsed` listing exactly what is used. `tests/validator.rs` damages good files and checks that the validator objects.
- **USD structure.** `tests/support/usda.rs` reads the layer back by its layout (it is not a general USD parser) and `tests/usd.rs` checks the metadata, every prim type and path, mesh attributes and their interpolation, exact extents, material networks and bindings, ops and their order, time samples, lights and cameras.
- **USDZ structure.** `tests/support/zip.rs` is a zip reader that shares no code with the writer, down to its own CRC-32. It checks both headers of every file against each other, the CRCs, that files are stored, unencrypted and packed back to back, the central directory and end record, and the 64-byte alignment; `tests/usdz.rs` runs it over name lengths that need every padding size from 0 to 63 bytes, and damages good packages to check that it objects.
- **FBX structure.** `tests/support/fbx.rs` is a binary FBX reader that shares no code with the writer. It walks every record checking end offsets, value counts and byte lengths, sentinels, array headers and the footer, then checks that definitions count the objects, that every connection has both ends, that the model tree is a tree, that every geometry's arrays agree with each other and with the material slots of the models that use it, and that every curve is wired to a curve node, a property and the layer, with increasing key times. `tests/fbx.rs` damages good files and checks that the reader objects.
- **Geometry and animation values, in every format.** Positions against the renderer's `ring_positions`, triangles against the scene, world placement against `Document::world_from_layer` (including five combinations of mirrored layers), animation samples against `evaluate` frame by frame, clip and frame-by-frame schedules against `evaluate`. For FBX also: Euler angles that rebuild the rotation and never jump, the quarter turns of lights and cameras, and exact key times.
- **Against Quill's own exporter.** The owner's sample project exports 52 paint meshes, 485,292 vertices and 948,032 triangles. Quill's own USDZ export of the same project holds 52 meshes, 485,292 points and 474,016 quads, which is the same surface. Limner's `.usdz` and `.fbx` of that project hold the same 485,292 vertices and 948,032 triangles as its `.glb`.
- **glTF in an independent reader.** The pinned Godot build (4.8 dev 7) loads the sample and fixture files through `GLTFDocument`, with the expected mesh, light, camera and animation track counts, and plays the animation with the expected node transforms.
- **USD in Apple's tools** (Apple USD Tools 0.25.2 on macOS 26.5, in `/usr/bin`). `usdchecker` reports "Success!" for the fixture `.usda` files (still, animated, baked), the fixture `.usdz` and the sample `.usdz`, also with `--arkit`. `usdcat` loads all of them and prints the same number of prims as were written; `usdtree` shows the expected tree; `usdzip --dump` lists the package's file at offset 64 with equal stored and real sizes, the same as for a package `usdzip` made itself. `usdrecord` renders the animated fixture through its exported camera with the Metal renderer: vertex colours show through the materials, the keyed layer moves, the clip appears on the right frames and the frame-by-frame layer changes drawing. `usdcat scene.usda -o scene.usdc` followed by `usdzip` gives a binary package that `usdchecker --arkit` also accepts. (These tools print "Coding Error ... RegisterBehaviorForPrimTypeId" lines for every file, including an empty one; that is the tools, not the export.) Two ignored tests rerun `usdchecker` and `usdcat`: `cargo test -p limner-export --test usd --test usdz -- --ignored`.
- **USDZ in other zip readers.** Info-ZIP `unzip -t` (6.00) and Python 3.14's `zipfile.testzip()` accept the packages and agree on the CRC.
- **Sounds in USD.** `tests/sound.rs` reads the `SpatialAudio` prims back: file path, aural mode, playback mode, start and end time codes, media offset and gain (with its samples), for clips, a looping sequence, a range that starts inside a clip, audio held as samples, shared and same-named files, baking, and the package's contents and its Ogg warning; `scene.rs` and `audio.rs` test when a sound is heard and the WAV writer. In Apple's tools: `usdchecker` reports "Success!" for a `.usda` with sounds (audio written beside it) and for the `.usdz`, also with `--arkit`; `usdcat --flatten` reads the prims back as `SpatialAudio` with the written values; `usdtree` shows them under their layers; `usdzip --dump` lists the audio files at 64-byte offsets with equal stored and real sizes; `afinfo` reads the WAV files written from samples with the right format and length. `cargo test -p limner-export --test sound -- --ignored` reruns the tool checks. Files without sound layers come out byte for byte as before this was added (the fixture files and the owner's sample in every format were compared).
- **FBX in an independent reader.** The pinned Godot build reads FBX through the ufbx library (`FBXDocument`). The fixture files (still, animated, baked, centimetres) and the sample load without errors. For the animated fixture it reports the same 6 mesh instances, 225 vertices, 334 triangles, 3 lights and 1 camera as for the `.glb` of the same scene; material slots arrive with the right names per surface; vertex colours arrive sRGB-encoded with their alpha; every triangle's winding agrees with its normals; the mirrored group has a negative determinant; lights and the camera point the same way as in the `.glb`, with the same field of view and clip planes; the centimetre variant arrives at the same size as the metre one. The keyed layer's translation, rotation and scale match the exporter's own samples on all 25 frames to four decimals, and the scale tracks switch the clip and the three frame-by-frame drawings on exactly the expected frames. The sample arrives with 948,032 triangles (Godot's importer merges identical vertices, so it counts 478,549 of the 485,292).

Not verified:

- **No sound has been played from an export.** Nothing has opened the `.usda` or `.usdz` in a player that plays `SpatialAudio` (AR Quick Look or RealityKit on a device, Omniverse). Whether players honour `mediaOffset`, the `...ToEnd` modes, time-sampled `gain` and `nonSpatial`, and how loud and how far a `spatial` sound carries without Limner's fall-off, is not known.

- The Khronos glTF validator has not been run on these files. The `gltf-validator` npm package is a library without a command-line entry, so it needs a three-line Node script around `validateBytes`; `cargo test -p limner-export --test fixtures -- --ignored` writes artwork-free files in every format to `local/out/export/` to feed it and the tools below, including `fixture-sound.usda` (audio in `local/out/export/audio/`), `fixture-sound.usdz` and `fixture-sound.glb`, with two tones made in code.
- **Blender, Unity and Unreal have not been run on any of these files** (Blender is not installed on the development Mac). The import steps for all formats, and the table of what each FBX importer does with units, are written from those tools' documented behaviour. The FBX files in particular have been read by one parser family only (ufbx, which recent Blender versions also use); Autodesk's FBX SDK, which Unity and Unreal use, has not read them. The writer omits two things the SDK itself writes and Blender's exporter copies, property templates in `Definitions` and `NodeAttribute` objects for empty nodes; readers are expected to fall back to defaults, and ufbx does.
- **USD `orientation`.** That `rightHanded` is the correct value follows from USD's definition and is checked against the normals in the tests, but no renderer has confirmed it: `usdrecord` draws both sides of every face, so a wrong value would look the same there.
- **USD in AR Quick Look and RealityKit.** `usdchecker --arkit` accepts the text-layer package, but nothing has been opened in Quick Look on a device. Whether it accepts a text root layer, and whether it shows vertex colours through a primvar reader, is not known.
- **USD materials in Blender, Unity and Unreal**: whether their importers follow the `UsdPrimvarReader_float3` connection to the vertex colours.
- **FBX `Visibility` curves** have been read back only by the test reader; Godot ignores them, and no tool that uses them has been tried.
- Light and camera layers point along their local -Z axis with +Y up, which is glTF's convention, USD's and Godot's. Whether Quill's camera layers use the same axis has not been checked against a real Quill render.
