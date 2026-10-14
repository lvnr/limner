# The Limner project format

Format version 1. This document is the specification: another implementation should be able to read and write Limner projects from it without reading the code in `crates/limner-format`, which is the reference implementation. Where the two disagree, that is a bug in one of them; report it.

The key words "must", "must not", "should" and "may" are used as in RFC 2119.

## 1. Goals

In priority order:

1. **An interrupted save never corrupts a project.** At every instant the folder holds one complete project: the one from the last completed save, or the new one.
2. **A save rewrites only what changed.** Stroke data and media are stored in immutable files named by the hash of their content. A file that already exists is never written again.
3. **The folder works well under git and Perforce.** The scene description is readable JSON in a stable layout, so a change is a small diff. Binary files never change once written.
4. **Nothing in the document is lost.** Stroke points come back bit for bit, transforms as the same `f64` values, times as the same integers, ids and the id counter unchanged.

## 2. Layout of a project

A project is a folder. Its name ends in `.limn` by convention.

```
Name.limn/
  limn.json                      the manifest: settings, the layer tree, the drawing and media tables
  drawings/ab/abcdef….lstk       stroke blobs, one per distinct drawing
  media/<hash>.<ext>             pictures, sounds and models
  state.json                     optional: editor state, owned by the application
  thumbnail.png                  optional: a PNG picture of the project
```

| Path | Required | Written by a document save |
|---|---|---|
| `limn.json` | yes | yes, by atomic replacement, and only when its content changes |
| `drawings/<h2>/<hash>.lstk` | one per entry of the drawing table | only when missing |
| `media/<hash>.<ext>` | one per distinct file of the media table | only when missing |
| `state.json` | no | never |
| `thumbnail.png` | no | never |

`<hash>` is 64 lowercase hexadecimal digits: the content hash of section 3. `<h2>` is its first two digits. A reader must not assume the `drawings` or `media` folders exist when their tables are empty.

A folder is a Limner project if and only if it contains a file named `limn.json`. Files whose names end in `.tmp` are leftovers of an interrupted save and are never part of a project. Readers must ignore every file the manifest does not refer to, so users and tools may keep other files in the folder (notes, `.gitattributes`, exports).

All paths in this document use `/`. The folder contains no symbolic links that a reader needs to follow.

### Packed projects

A packed project is a single file, by convention `Name.limnz`: a ZIP archive whose entries are the files of a project folder, with `limn.json` at the root of the archive (no enclosing folder).

- Entry names are the project-relative paths with `/` separators.
- Every entry must be **stored** (ZIP compression method 0) and unencrypted. Stroke blobs and most media are already compressed, and stored entries let a reader use the data in place.
- ZIP64 is allowed and is needed for entries of 4 GiB or more and for more than 65,535 entries.
- Entry order is not significant. Directory entries may be present and are ignored. Entries the manifest does not refer to are ignored, apart from `state.json` and `thumbnail.png`.
- A reader recognises a packed project by its content (a ZIP archive with a root entry `limn.json`), not by its extension.

When it packs a folder, the reference writer writes `limn.json`, `state.json`, `thumbnail.png`, then the stroke blobs and the media files in name order, each with the time stamp 1980-01-01 00:00:00 and the same permissions, so packing the same folder twice gives identical bytes. Only the files the manifest refers to are packed. A packed file is written under `Name.limnz.tmp`, flushed, and renamed over the final name.

## 3. Content hashes

The hash function is **BLAKE3** with its default 256-bit output, written as 64 lowercase hexadecimal digits.

Every content file has a **canonical form**, a byte string that depends only on what the file means and not on how it was compressed or encoded. The hash of the canonical form is the file's name. Consequences:

- Two drawings with the same strokes are one file, however many layers and frames show them.
- Recompressing a file, or writing it with a different compressor version, does not change its name.
- A reader verifies a file by rebuilding the canonical form from what it read and comparing its hash with the name. A mismatch means the file is damaged and the project must not be loaded.

| Content | Canonical form |
|---|---|
| Stroke blob | The `LIMNSTK1` container of section 5 with `flags = 0` (header followed by the uncompressed payload) |
| Mesh | The `LIMNMSH1` container of section 6.4 with `flags = 0` |
| Raw image | The `LIMNIMG1` container of section 6.2 with `flags = 0` |
| Raw audio | The `LIMNAUD1` container of section 6.3 with `flags = 0` |
| Embedded file | The bytes of the file |

### The container

The four binary formats defined here share one shape. All integers are little-endian. All floating-point values are IEEE 754 binary32, little-endian.

```
offset  size  field
0       8     signature, eight ASCII bytes
8       4     flags (u32)
12      …     the rest of the header, fixed size per signature
H       …     body
```

`flags` bit 0 (value 1) set means the body is the payload compressed as **one zstd frame**; clear means the body is the payload itself. All other bits are reserved: a writer must write them as zero and a reader must reject a file in which any is set.

- The zstd frame must record its content size in the frame header, and that size must equal the payload length implied by the header. It must not use a dictionary. The compression level is the writer's choice.
- The canonical form is the header with the four `flags` bytes set to zero, followed by the uncompressed payload.
- The body must be exactly one frame (or exactly the payload); trailing bytes are an error.
- A reader must compute the payload length from the header before decompressing and must not allocate more than that.

The reference writer compresses at zstd level 3 and stores the payload uncompressed when compression would not make it smaller. Both choices are free: they do not affect the hash.

## 4. `limn.json`

### 4.1 Encoding and layout

`limn.json` is a JSON document (RFC 8259) encoded as UTF-8 without a byte-order mark.

**Naming.** Every member name and every enumeration value in the schema is `snake_case` (lowercase ASCII letters, digits and underscores): `next_id`, `loop_duration`, `"ease_in_out"`, `"equirect_stereo"`. The only exceptions are user data: layer and media names, paths, the document title and description, and the names of animation channels (section 4.9), which are free text.

**Reading.**

- A reader must ignore members it does not know, at every level. This is how later minor additions stay readable by older software.
- A reader must accept members in any order and any JSON whitespace.
- Members documented with a default may be absent; the default applies. Members marked *required* must be present.
- A reader must reject the document if `format` is not `"limn"`, and must reject it with a message saying that newer software is needed if `version` is greater than the version it implements. It must make this check before interpreting anything else, since a newer version may have changed anything else.

**Numbers.**

- Integers (ids, times, counts) are JSON integers and must be read exactly. Ids and counters are unsigned 64-bit; times are signed 64-bit. Implementations whose JSON library reads every number as binary64 are exact only up to 2^53; that is enough for ids in practice but not for arbitrary times.
- Real numbers are written with the shortest decimal representation that reads back to the same value, so `0.1` is written `0.1`. A value that is a 64-bit float in the model (transforms, channel keys) must read back to the same binary64; a reader must use a correctly rounding parser. A value that is a 32-bit float in the model (colours, opacities, angles; marked *f32* below) is written with the shortest decimal that, read as binary64 and then rounded to binary32, gives the original bits.
- JSON cannot spell three values. Where a real number is expected, the strings `"NaN"`, `"Infinity"` and `"-Infinity"` stand for them. NaN payloads are not preserved. A healthy document contains none of these; the spelling exists so that a document holding one can still be saved.

**Writing.** The reference writer produces a fixed layout, and a writer should do the same, because the layout is what keeps diffs small and lets a save detect that nothing changed:

- members in the order given in this section; two-space indentation; `\n` line ends; a final `\n`;
- an array whose elements are all scalars (or empty arrays or objects) is written on one line: `[0.0, 0.0, 0.0, 1.0]`;
- an object nested at least two levels below the top, whose members are all scalars or such one-line arrays, is written on one line: animation keys, transforms, table rows and most layer-kind records are one line each;
- everything else is written one member or element per line.

The layout depends only on the shape of the data, never on its values, so editing a value changes one line.

Recommended `.gitattributes` for a repository holding projects:

```
*.limn/limn.json   text eol=lf
*.lstk             binary
*.lmsh             binary
*.limg             binary
*.laud             binary
```

### 4.2 Top level

```json
{
  "format": "limn",
  "version": 1,
  "generator": "limner-format 0.1.0",
  "next_id": 1046,
  "settings": { … },
  "root": { … },
  "drawings": [ … ],
  "media": [ … ]
}
```

| Member | Type | Default | Meaning |
|---|---|---|---|
| `format` | string | *required* | Always `"limn"`. |
| `version` | integer | *required* | Format version, 1 for this document. See section 8. |
| `generator` | string | `""` | Name and version of the software that wrote the file. Informational. |
| `next_id` | integer | 0 | The id counter: the next id the document will hand out. Greater than every id in use. A reader must use the larger of this and one more than the largest id present. Ids are never reused, so this must be preserved even when the highest-numbered items have been deleted. |
| `settings` | object | all defaults | Section 4.3. |
| `root` | object | *required* | The root layer (section 4.4). It must have `type` `"group"`, and its id must be 1. |
| `drawings` | array | `[]` | The drawing table (section 4.10), in increasing id order. |
| `media` | array | `[]` | The media table (section 4.11), in increasing id order. |

**Ids.** Layers, drawings and media items share one id space: positive integers handed out by one counter, unique across all three kinds within a document. The reference reader rejects two layers, two drawings or two media items with the same id, and warns when items of different kinds share one.

**Time.** Every time and duration is an integer number of **flicks**: 1/705,600,000 of a second. Every common frame rate divides it exactly. Times on a layer are measured on the clock of the nearest enclosing sequence (section 4.6).

**Colour.** Every colour is linear-light RGB, three *f32* values, nominally 0 to 1.

### 4.3 `settings`

| Member | Type | Default | Meaning |
|---|---|---|---|
| `title` | string | `""` | Title of the piece. |
| `description` | string | `""` | Free text. |
| `background` | [r, g, b] *f32* | `[0.8, 0.8, 0.8]` | Background colour. |
| `ambient_light` | [r, g, b] *f32* | `[0.35, 0.35, 0.35]` | Light that reaches lit paint and models from everywhere, as a share of their own colour: how bright they are where no light layer reaches. |
| `rate` | [num, den] integers | `[24, 1]` | Document frame rate in frames per second, as the fraction num/den. `[24000, 1001]` is 23.976. |
| `range_start` | flicks | 0 | Start of the working range: what loops in the editor and what cameras render. |
| `range_end` | flicks | 7056000000 (10 s) | End of the working range. |
| `default_viewpoint` | layer id or null | null | The viewpoint layer a viewer starts at. |
| `camera_resolution` | [width, height] integers | `[1920, 1080]` | Pixel size of camera renders. |
| `palette` | array of [r, g, b] *f32* | 16 × `[0.82, 0.82, 0.82]` | Colour swatches. The application uses 16; the format keeps however many are present. |

### 4.4 Layers

The layer tree is nested: a group's children are inside it, in display order. A layer's parent is the group that contains it.

```json
{
  "id": 24,
  "name": "Body",
  "type": "group",
  "visible": true,
  "locked": false,
  "collapsed": true,
  "show_bounds": true,
  "opacity": 0.35,
  "transform": {"rotation": [0.0, 0.0, 0.0, 1.0], "scale": 1.0, "translation": [0.0, 0.0, 0.0], "flip": "x"},
  "pivot": {"rotation": [0.0, 0.0, 0.0, 1.0], "scale": 1.0, "translation": [0.0, 0.0, 0.0], "flip": "none"},
  "anim": { … },
  "children": [ … ]
}
```

| Member | Type | Default | Meaning |
|---|---|---|---|
| `id` | integer | *required* | The layer's id. |
| `name` | string | `""` | Display name. Need not be unique. |
| `type` | string | *required* | `"group"`, `"paint"`, `"picture"`, `"model"`, `"sound"`, `"viewpoint"`, `"camera"` or `"light"`. A reader must reject a type it does not know. |
| `visible` | boolean | true | The eye toggle. Independent of clips (section 4.9). |
| `locked` | boolean | false | Protected from editing. |
| `collapsed` | boolean | false | Shown folded in the layer list. |
| `show_bounds` | boolean | false | Draw the layer's bounding box in the editor. |
| `opacity` | *f32* | 1 | Opacity of a visual layer; volume of a sound layer. |
| `transform` | transform | identity | Placement relative to the parent layer (section 4.5). |
| `pivot` | transform | identity | The point and frame, in layer space, that keyed transforms turn about. |
| `anim` | object | no keys | Animation keys (section 4.9). |
| `paint`, `picture`, `model`, `sound`, `viewpoint`, `camera`, `light` | object | | The record for the layer's type (sections 4.7 and 4.8). The member named by `type` must be present for every type except `"group"`; the others must be absent. |
| `sequence` | object | absent | Groups only. Present when the group is a sequence (section 4.6). |
| `children` | array of layers | `[]` | Groups only: the child layers in display order. Must be absent or empty on any other type. |

Layers may be nested at most **48** levels below the root. The limit keeps the manifest within the recursion depth JSON readers commonly allow (128 levels; a layer costs two). A writer must refuse to save a deeper tree rather than write a file readers cannot open.

### 4.5 Transforms

```json
{"rotation": [x, y, z, w], "scale": s, "translation": [x, y, z], "flip": "none"}
```

| Member | Type | Default | Meaning |
|---|---|---|---|
| `rotation` | 4 reals | `[0, 0, 0, 1]` | Unit quaternion, in the order x, y, z, w. |
| `scale` | real | 1 | Uniform scale. |
| `translation` | 3 reals | `[0, 0, 0]` | In the parent's units (metres at the root). |
| `flip` | string | `"none"` | Mirror applied before rotation: `"none"`, `"x"`, `"y"` or `"z"`. |

A point `v` maps to `rotation * (scale * F * v) + translation`, where `F` negates the coordinate named by `flip`. Rotation, scale and translation are binary64 in the model and must round-trip exactly; a writer must not normalise the quaternion or otherwise adjust them. What the axes mean is the application's convention; the format stores the numbers.

### 4.6 Groups and sequences

A layer of type `"group"` holds `children`. If it has a `sequence` member it is a **sequence**: a nested timeline with its own clock, which restarts whenever one of the group's clips starts.

| Member of `sequence` | Type | Default | Meaning |
|---|---|---|---|
| `loop_duration` | flicks | 0 | Loop point of the sequence's clock. 0: the sequence does not loop. |
| `max_repeat` | integer | 0 | How many times the loop plays before holding at its end. 0: forever. |

A group without `sequence` is a plain group: it places and gates its children and shares its parent's clock. The root is normally a sequence.

### 4.7 Paint layers

```json
"paint": {"rate": [24, 1], "drawings": [13, 16, 18], "frames": [0, 0, 1, 2, 1], "max_repeat": 0, "shading": "unlit"}
```

| Member | Type | Default | Meaning |
|---|---|---|---|
| `rate` | [num, den] | `[24, 1]` | Frame rate of this layer's frame-by-frame animation. Both numbers must be positive. |
| `drawings` | array of drawing ids | *required* | The layer's drawings in creation order. Must not be empty. Each id must be in the drawing table. |
| `frames` | array of integers | *required* | One entry per frame: an index into `drawings`. Must not be empty; every entry must be less than the length of `drawings`. Several frames may show the same drawing. |
| `max_repeat` | integer | 0 | How many times the frames play before holding on the last. 0: forever. |
| `shading` | string | `"unlit"` | `"unlit"`: vertex colour only. `"lit"`: scene lights shade the strokes. |

The frame shown at local time `t` (flicks since the layer's clip started, plus its offset) is `frames[floor(t * num / (den * 705600000)) mod len(frames)]`, or the last frame once `max_repeat` passes are complete.

### 4.8 Other layer kinds

**`picture`**

| Member | Type | Default | Meaning |
|---|---|---|---|
| `media` | media id or null | null | The image. |
| `projection` | string | `"flat"` | `"flat"`, `"equirect"`, `"equirect_stereo"` (top half left eye, bottom half right eye), `"cubemap_cross"` or `"cubemap_strip"`. |
| `viewer_locked` | boolean | false | Stays fixed in front of the viewer, ignoring navigation. |
| `source_path` | string | `""` | The path the picture was imported from, as the user's system spelled it. Informational. |

**`model`**

| Member | Type | Default | Meaning |
|---|---|---|---|
| `media` | media id or null | null | The mesh. |
| `shading` | string | `"unlit"` | `"unlit"` or `"lit"`. |
| `wireframe` | boolean | false | Draw as wireframe. |
| `source_path` | string | `""` | Import path. Informational. |

**`sound`**

| Member | Type | Default | Meaning |
|---|---|---|---|
| `media` | media id or null | null | The audio. |
| `kind` | string | `"flat"` | `"flat"` (the same everywhere), `"positional"` (a point source) or `"ambisonic"`. |
| `gain` | *f32* | 1 | Linear gain. |
| `looped` | boolean | false | Restart when the audio ends. |
| `attenuation` | object | below | Distance falloff. |
| `modifier` | object | `{"type": "none"}` | Directional shaping. |
| `source_path` | string | `""` | Import path. Informational. |

`attenuation` is `{"mode": m, "min": a, "max": b}`: `mode` is `"none"`, `"linear"` or `"logarithmic"` (default `"none"`); the sound is at full volume within `min` layer units of its origin (*f32*, default 0.1) and silent beyond `max` (*f32*, default 0.5).

`modifier` is one of the following; the members other than `type` are *f32*, required for their type, and angles are in radians:

```json
{"type": "none"}
{"type": "cone", "inner": 0.5, "band": 0.25, "outside": 0.1}
{"type": "frustum", "inner_h": 1.2, "inner_v": 0.8, "band": 0.3, "outside": 0.0}
```

`inner` (or `inner_h` and `inner_v`, horizontal and vertical) is the angle of full volume, `band` the width of the transition, `outside` the gain beyond it.

**`viewpoint`** (a spawn area: where a viewer stands, and at what scale)

| Member | Type | Default | Meaning |
|---|---|---|---|
| `floor_level` | boolean | false | True: the ring maps to the viewer's real floor. False: the headset maps to eye height. |
| `color` | [r, g, b] *f32* | `[0.26, 0.61, 0.95]` | Colour of the marker. |
| `volume` | [x, y, z, radius] *f32* | `[0, 1, 0, 2]` | Centre and radius of the volume the viewer may move in. |
| `allow_translation` | 3 booleans | all true | Whether the viewer may move along x, y, z. |
| `exporting` | boolean | true | Included when the piece is exported for viewers. |
| `show_volume` | boolean | false | Draw the volume in the editor. |
| `thumbnail` | media id or null | null | A picture of the view from here. |

**`camera`**

| Member | Type | Default | Meaning |
|---|---|---|---|
| `fov` | *f32* | 45 | Vertical field of view in degrees. |

**`light`**

| Member | Type | Default | Meaning |
|---|---|---|---|
| `kind` | string | `"point"` | `"point"`, `"spot"` or `"directional"`. |
| `color` | [r, g, b] *f32* | `[1, 1, 1]` | |
| `energy` | *f32* | 1 | Brightness multiplier. |
| `range` | *f32* | 5 | Reach of point and spot lights, in layer units. |
| `spot_angle` | *f32* | 35 | Half-angle of a spot light's cone, in degrees. |
| `spot_blend` | *f32* | 0.15 | Softness of the cone edge, 0 to 1. |
| `shadows` | boolean | false | Casts shadows. |

### 4.9 Animation

`anim` holds one array of keys per channel. A channel with no keys is omitted.

```json
"anim": {
  "visibility": [
    {"time": 0, "value": true},
    {"time": 705600000, "value": false}
  ],
  "offset": [
    {"time": 0, "value": 0}
  ],
  "opacity": [
    {"time": 0, "interp": "linear", "value": 0.0},
    {"time": 29400000, "value": 1.0}
  ],
  "transform": [
    {"time": 0, "interp": "ease_in_out", "rotation": [0.0, 0.0, 0.0, 1.0], "scale": 1.0, "translation": [0.0, 0.0, 0.0], "flip": "none"}
  ],
  "actions": [
    {"time": 705600000, "value": "stop"}
  ],
  "channels": {
    "light.energy": [
      {"time": 0, "interp": "smooth", "value": 0.0},
      {"time": 705600000, "value": 2.5}
    ]
  }
}
```

Every key has:

| Member | Type | Default | Meaning |
|---|---|---|---|
| `time` | flicks | *required* | When the key takes effect, on the clock of the nearest enclosing sequence. May be negative. |
| `interp` | string | `"step"` | How the value moves from this key to the next: `"step"` (hold), `"linear"`, `"ease_in"` (t³), `"ease_out"` (1 − (1 − t)³), `"ease_in_out"` (t²(3 − 2t)) or `"smooth"` (a curve through the neighbouring keys). Omitted by the writer when it is `"step"`. Stored on every key of every channel, including those where only stepping is meaningful. |
| `value` | per channel | *required* | Except on `transform` keys, which carry the four members of a transform (section 4.5) directly beside `time` and `interp`, each with its usual default. |

Within a channel the keys must be in increasing `time` order with no two at the same time. A reader given unsorted keys must sort them, and given two keys at one time must keep the later one in file order.

| Channel | `value` | Meaning |
|---|---|---|
| `visibility` | boolean | Clips. A `true` key starts a clip, a `false` key ends it. The layer is present when the last key at or before the current time is `true`; before the first key it is absent. With no keys the layer is always present. |
| `offset` | flicks | Left trim of the clip that starts at the same time: how far into the layer's content the clip begins. |
| `opacity` | *f32* | Multiplies the layer's `opacity`. |
| `transform` | transform | Replaces the layer's `transform` while keys apply; blended about `pivot`. |
| `actions` | string | Story structure, meaningful only on the root layer: `"stop"` (pause until the viewer continues), `"play"` (a chapter start: where rewinding lands) or `"loop"` (restart the piece). |
| `channels` | object | Named scalar channels: each member is a channel name mapped to an array of keys whose `value` is a real (binary64). Names are free text chosen by the application, such as `"light.energy"` or `"camera.fov"`; a reader must keep channels whose names it does not know, including channels with no keys. Members are written in ascending order of name (by Unicode scalar value). |

How these channels combine during playback is defined in section 7 of the design document (`docs/superpowers/specs/2026-10-14-limner-design.md`); the file format only stores them.

### 4.10 The drawing table

```json
"drawings": [
  {"id": 13, "blob": "a3c03293256ee0be8a8afd7a673a247e1a2a5180ac27c0b1d6547ffe029ac933"},
  {"id": 14, "blob": "a3c03293256ee0be8a8afd7a673a247e1a2a5180ac27c0b1d6547ffe029ac933"}
]
```

| Member | Type | Meaning |
|---|---|---|
| `id` | integer, *required* | The drawing's id, referred to by `paint.drawings`. |
| `blob` | string, *required* | The content hash of the drawing's stroke blob: exactly 64 lowercase hexadecimal digits. The blob is the file `drawings/<first two digits>/<blob>.lstk`. |

Several drawings may name the same blob. The table lists every drawing of the document, including any that no layer currently shows; a paint layer must not refer to an id that is not in the table.

### 4.11 The media table

```json
"media": [
  {"id": 3, "name": "overlay.png", "kind": "image", "storage": "raw_image", "file": "37e1…c07c.png"},
  {"id": 5, "name": "reference.jpg", "kind": "image", "storage": "encoded", "format": "jpg", "file": "0560…1a64.jpg"}
]
```

| Member | Type | Meaning |
|---|---|---|
| `id` | integer, *required* | The media item's id, referred to by `picture.media`, `model.media`, `sound.media` and `viewpoint.thumbnail`. |
| `name` | string, default `""` | The original file name, for display. |
| `kind` | string, *required* | What the item is used as: `"image"`, `"audio"` or `"mesh"`. |
| `storage` | string, *required* | How the file is to be read: `"encoded"`, `"raw_image"`, `"raw_audio"` or `"mesh"` (section 6). |
| `format` | string | For `"encoded"` only: the file's format as the application names it (`png`, `jpg`, `wav`, `ogg`, `mp3`, `glb`, `obj`, `fbx`), kept exactly. Absent for other storages. |
| `file` | string, *required* | The file's name inside `media/`: `<hash>.<extension>`, where `<hash>` is the content hash (64 lowercase hexadecimal digits) and `<extension>` is one or more ASCII letters and digits. A reader must reject any other shape, in particular names containing `/`, `\` or `..`. |

Several items may name the same file.

## 5. Stroke blobs (`.lstk`)

One blob holds the strokes of one drawing, in drawing order. A drawing with no strokes has a blob too (24 bytes).

### 5.1 Header

```
offset  size  type  field
0       8           signature "LIMNSTK1"  (4C 49 4D 4E 53 54 4B 31)
8       4     u32   flags                 (section 3)
12      4     u32   S: number of strokes
16      8     u64   P: total number of points in all strokes
24                  body
```

The payload is `12·S + 56·P` bytes: the stroke table followed by the point data.

### 5.2 Stroke table

`S` records of 12 bytes, at payload offset `12·i` for stroke `i`:

```
offset  size  type  field
0       4     u32   id            unique within the drawing; seeds the dither pattern of see-through strokes
4       4     u32   point count   may be 0
8       1     u8    brush         0 line, 1 ribbon, 2 cylinder, 3 ellipse, 4 cube
9       1     u8    flags         bit 0: directional (fades as the view departs from the drawing direction)
10      2     u16   reserved, zero
```

The point counts must sum to `P`. A reader must reject a brush code above 4, any flag bit other than bit 0, and a non-zero reserved field. Brush 0 is a legacy thin line, drawn as a ribbon.

### 5.3 Point data

A point has 14 components, each a binary32:

| Index | Component | Meaning |
|---|---|---|
| 0, 1, 2 | position x, y, z | In layer space, metres. |
| 3, 4, 5 | normal x, y, z | Unit vector that orients the brush cross-section. |
| 6, 7, 8 | direction x, y, z | Unit view direction at the moment of drawing; used by directional strokes. |
| 9, 10, 11 | colour r, g, b | Linear RGB. |
| 12 | opacity | 0 to 1. |
| 13 | width | Half-width of the stroke at this point, in layer space. |

Points are numbered 0 to `P − 1` through the whole drawing: the points of stroke 0 in order, then those of stroke 1, and so on.

The point data is **56 byte planes** of `P` bytes each, starting at payload offset `12·S`. Plane `4c + b` holds byte `b` of component `c` of every point, where byte 0 is the least significant byte of the little-endian binary32:

```
byte b of component c of point n   is at payload offset   12·S + (4·c + b)·P + n
```

So the file holds the low bytes of every x position, then the next byte of every x position, and so on through the fourth byte of every width. Keeping each byte of each component together is what lets a general compressor find the regularity in floating-point data: on real artwork it makes blobs about a quarter smaller than compressing whole points.

Every bit pattern is stored as it is; writers must not normalise values (for example, −0 stays −0).

### 5.4 What is not stored

- **Chunking.** How an implementation groups strokes in memory for rendering is not part of a drawing. Two drawings with the same strokes in the same order are the same content. The reference reader regroups strokes greedily into chunks of at most 60,000 tessellated vertices (2 per point for brushes 0 and 1, 7 for 2 and 3, 4 for 4), never splitting a stroke.
- **Bounds.** Recomputed on load.

## 6. Media

`storage` in the media table says which of four ways a media file is read. Whatever the storage, the file is named by the hash of its canonical form (section 3).

### 6.1 `encoded`

The file is an image, sound or model in its own format, kept byte for byte. Canonical form: the file itself. The extension is the lowercase `format` when that is 1 to 8 ASCII letters and digits, and `bin` otherwise; the exact `format` string is in the manifest.

### 6.2 `raw_image`

Decoded 8-bit pixels: rows top to bottom, pixels left to right, `channels` bytes per pixel (1 grey, 2 grey and alpha, 3 RGB, 4 RGBA).

Canonical form, the `LIMNIMG1` container:

```
offset  size  type  field
0       8           signature "LIMNIMG1"
8       4     u32   flags
12      4     u32   width in pixels
16      4     u32   height in pixels
20      4     u32   channels (0 to 255)
24      8     u64   N: number of pixel bytes
32      N           pixels
```

The file in `media/` is one of:

- **a PNG file** (extension `png`), when `width` and `height` are at least 1, `channels` is 1 to 4 and `N = width · height · channels`: 8 bits per sample, colour type greyscale, greyscale with alpha, truecolour or truecolour with alpha by channel count, not interlaced, no palette, no colour-management chunks. This is the normal case, and it means other tools can open the image;
- **the container itself** (extension `limg`), compressed or not as `flags` says, when PNG cannot carry the image exactly.

A reader tells the two apart by the first eight bytes. To verify a PNG it decodes the pixels without any transformation, builds the canonical form from the decoded width, height, channel count and pixels, and compares the hash with the file name. The PNG encoding itself (filters, compression level) is therefore free and does not affect the name.

### 6.3 `raw_audio`

Interleaved samples, little-endian, as the application holds them.

Canonical form, the `LIMNAUD1` container:

```
offset  size  type  field
0       8           signature "LIMNAUD1"
8       4     u32   flags
12      4     u32   sample rate in Hz
16      2     u16   channels
18      2     u16   bits per sample
20      4     u32   sample format: 0 integer PCM, 1 IEEE float. Other values are reserved.
24      8     u64   N: number of sample bytes
32      N           samples
```

The file in `media/` is one of:

- **a RIFF/WAVE file** (extension `wav`), when the layout is one WAVE carries exactly: at least one channel, a sample rate of at least 1, integer samples of 8, 16, 24 or 32 bits or float samples of 32 or 64 bits, `N` a multiple of `channels · bits / 8`, and `N` below 4 GiB. Integer audio uses format tag 1 with a 16-byte `fmt ` chunk; float audio uses format tag 3 with an 18-byte `fmt ` chunk and a `fact` chunk. The sample bytes are the `data` chunk, unchanged;
- **the container itself** (extension `laud`) otherwise.

A reader tells the two apart by the first bytes (`RIFF` … `WAVE`). To verify a WAVE file it takes the channel count, sample rate, bits per sample and format tag (1 or 3; for tag 0xFFFE, the first two bytes of the sub-format) from the `fmt ` chunk and the bytes of the `data` chunk, builds the canonical form, and compares the hash with the file name. Other chunks are ignored.

### 6.4 `mesh` (`.lmsh`)

Triangle data for model layers. Canonical form and stored file are the `LIMNMSH1` container:

```
offset  size  type  field
0       8           signature "LIMNMSH1"
8       4     u32   flags
12      4     u32   V: number of positions
16      4     u32   Nn: number of normals   (0, or V)
20      4     u32   Nc: number of colours   (0, or V)
24      4     u32   I: number of indices    (a multiple of 3)
28      4     u32   reserved, zero
32                  body
```

The payload is `12·(V + Nn + Nc) + 4·I` bytes:

```
positions   V  × 3 binary32   x, y, z in layer space
normals     Nn × 3 binary32   unit vectors, one per position
colours     Nc × 3 binary32   linear RGB, one per position
indices     I  × u32          triangle list, three indices per triangle, each below V
```

The four counts are stored independently, and a reader must accept any combination and return it as stored; the parenthesised constraints describe a well-formed mesh, which is the application's concern.

### 6.5 Reading media back

A reader restores each item as the variant its `storage` names: a `raw_image` comes back as pixels with its width, height and channel count, a `raw_audio` as samples with its layout, a `mesh` as arrays, an `encoded` item as bytes with its `format`. Because the name of a raw item is the hash of its pixels or samples and not of the PNG or WAVE bytes, loading a project and saving it again finds every media file already present and writes nothing.

## 7. Saving and loading

### 7.1 The save protocol

To save a document into a project folder:

1. **Check** that the document is consistent and within the format's limits (section 4.4). If not, stop: nothing has been written.
2. **Write new content.** For every drawing and media item, compute the content hash. If the file it names exists, leave it. Otherwise write the bytes to `<final name>.tmp` in the same folder, flush that file to the disk (`fsync`; `F_FULLFSYNC` on macOS), and rename it to its final name. Then flush each folder that received a file, so the renames themselves are durable.
3. **Replace the manifest.** Build the new `limn.json`. If the existing file has exactly these bytes, leave it. Otherwise write `limn.json.tmp`, flush it, rename it over `limn.json`, and flush the project folder.
4. **Collect garbage** (optional). Delete, in `drawings/*/`, files named `<64 hex digits>.lstk` that the new manifest does not name; in `media/`, files whose name starts with 64 hexadecimal digits and a dot that the manifest does not name; files ending in `.tmp` in those folders; and `limn.json.tmp`, `state.json.tmp` and `thumbnail.png.tmp` beside the manifest. Remove shard folders left empty. Delete nothing else.

Why this is safe: until the rename in step 3, the old manifest is in place and every file it names is untouched, because content files are never modified or deleted before that point; new files only add names nothing refers to. The rename is atomic, so afterwards the new manifest is in place and every file it names was completely written and flushed in step 2. Step 4 deletes only what the manifest now in place does not name. A crash at any point therefore leaves a project that loads: the old one before the rename, the new one after. Leftover `.tmp` files and unreferenced content files are harmless and are removed by the next save that collects garbage.

A file under its final name is always complete, which is why step 2 may trust that an existing file need not be rewritten.

Notes for implementers:

- One writer at a time. The protocol does not coordinate two programs saving into the same folder.
- On Windows, renaming over an existing file must use a replacing rename (`MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`). Folders cannot be flushed there; that step is skipped.
- A writer may skip the flushes for speed. The save is then still safe against a crash of the program, since renames are atomic, but not against power loss.
- `state.json` and `thumbnail.png` are written the same way (temporary name, flush, rename) and independently of the document.

The reference implementation reports, for each save, how many blobs and media files were written and how many were already present, whether the manifest was written, and how many files were removed.

### 7.2 Loading

1. Read `limn.json`. Check `format` and `version` (section 4.1), then parse the rest.
2. For each entry of the drawing table, read the blob and verify its hash. A blob named by several entries need be read only once.
3. For each entry of the media table, read the file as its `storage` says and verify its hash.
4. Rebuild the layer tree with the stored ids, and check that it is a consistent document: ids are unique, every paint layer has at least one frame and one drawing, every frame index is in range, every drawing and media id referred to exists, frame rates are positive.

A missing file, a file that fails verification, or an inconsistent document must fail the load with an error that names the file. A reader must not substitute empty content.

A reader should warn, but not fail, when `limn.json.tmp` exists beside the manifest (an earlier save was interrupted; the last completed save is what loads) and when `next_id` is not greater than every id in use (it uses the corrected value).

A packed project is loaded by the same steps, reading each entry from the archive.

### 7.3 Editor state

`state.json` is a JSON document owned by the application: the palette in use, the last tool, the view. This format assigns it no schema; implementations keep and return it as it is. It is separate from the manifest so that looking around a scene does not produce a diff in the document.

## 8. Versioning

`version` is a single integer.

- **Additions that an older reader may safely ignore do not change the version**: new optional members with defaults, new named animation channels, new optional files in the folder. This is why readers must ignore unknown members and unknown files.
- **Anything else increments the version**: a new layer type, a new enumeration value, a new brush code, a new container flag, a new media storage, a change to the meaning or encoding of an existing member, a change to hashing. A reader must refuse a higher version than it implements, and must say that newer software is needed.
- A reader of version N should keep reading every earlier version.
- The binary containers carry their own version in the last character of their signatures (`LIMNSTK1`). A changed binary layout gets a new signature and a new format version.

`generator` is informational and must not be used to decide how to read a file.

## 9. Limits and reserved values

| Quantity | Limit |
|---|---|
| Layer nesting below the root | 48 levels |
| Strokes per drawing | 2^32 − 1 |
| Points per stroke | 2^32 − 1 |
| Mesh positions, normals, colours, indices | 2^32 − 1 each |
| Ids | 1 to 2^64 − 1; the root layer is 1 |
| Times | −2^63 to 2^63 − 1 flicks |

Reserved fields and flag bits must be written as zero and rejected when non-zero, so that they can be given meaning in a later version without older readers misreading new files.

## 10. Worked example

The smallest project is a folder holding one file:

```json
{
  "format": "limn",
  "version": 1,
  "root": {"id": 1, "type": "group"}
}
```

It loads as an empty scene with default settings. The smallest stroke blob is the empty drawing, 24 bytes:

```
4C 49 4D 4E 53 54 4B 31   00 00 00 00   00 00 00 00   00 00 00 00 00 00 00 00
"LIMNSTK1"                flags = 0     S = 0         P = 0
```

Its canonical form is the same 24 bytes, so its name is their BLAKE3 hash:

```
drawings/bd/bd5cbcc212c3a8c48180d006a35627ac743984463db7a5d3aaa4ea8746c74ac6.lstk
```

A drawing with one ribbon stroke (id 7, not directional) of two points has `S = 1`, `P = 2` and a payload of `12 + 112` bytes: the table record `07 00 00 00  02 00 00 00  01  00  00 00`, then 56 planes of 2 bytes. If the two x positions are 1.0 (`00 00 80 3F`) and 2.0 (`00 00 00 40`), planes 0 to 3 are `00 00`, `00 00`, `80 00`, `3F 40`.
