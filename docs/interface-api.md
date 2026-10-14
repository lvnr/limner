# How the interface talks to the document

Everything a panel, a menu or a controller does goes through one small API. This page is the reference for anyone writing interface code in `godot/`.

The pieces:

- **`LimnerScene`** (Rust, `crates/limner-godot`): the node that owns the editing session and draws the artwork. It contains no product logic.
- **`LimnerEditor`** (`godot/editor/editor.gd`): the GDScript wrapper panels use. It runs commands, reports failures, caches what panels read and re-emits change signals.
- **Commands** (`crates/limner-core/src/session/command.rs`): every edit, as data. The same commands are used by the desktop interface, the in-headset interface, the command-line tool and the tests.

## Rules for panel code

1. Never change the document any other way than `editor.run({...})`. Never keep a copy of document state that can go stale: read it from the editor when a signal fires.
2. Redraw on signals, not every frame: `editor.document_changed` (layers, strokes, keys, settings), `editor.state_changed` (tool, options, selection, playback state), `editor.document_replaced` (another document was opened). `editor.status` is refreshed every frame and is the only thing to read in `_process`.
3. A drag is one undo step: call `editor.begin("Label")` when it starts, `editor.run(...)` for each change, `editor.commit()` when it ends. `SliderRow` gives you the three signals for this.
4. When you set a control from document state, do it without re-triggering the command: use `Kit.set_quiet(control, value)` or `SliderRow.set_value(value)`.
5. Colours in commands and queries are **linear RGB** lists `[r, g, b]`. Convert at the screen edge with `LimnerEditor.screen_color(list) -> Color` and `LimnerEditor.linear_rgb(color) -> Array`.
6. Times are **flicks** (integers, 705,600,000 per second). Convert with `LimnerEditor.to_flicks(seconds)` and `LimnerEditor.to_seconds(flicks)`. Frame length is `editor.get_info().frame_duration`.
7. Layers are named by integer id. `-1` in GDScript helpers means "none"; in commands use `null`.
8. The same panels are shown on the desktop and, later, on a hand-held palette in a headset where a controller tip pokes them. So: lay out correctly from 280 px wide; make every control at least `Kit.ROW` (30 px) tall; never rely on hover, right-click or the keyboard as the only way to reach something; no fixed pixel positions.
9. GDScript is statically typed here and "inferred from Variant" is an error: give variables explicit types when the right-hand side is a Variant (`var row: Dictionary = ...`, `var id: int = item.get_metadata(0)`).
10. Use `Kit` for every control and `Icons.texture(name, color, pixels)` for icons, so the look stays in one place. Add an icon by adding an SVG body to `Icons.BODIES` (24 x 24 grid, stroke only, `{c}` for the colour). Icons must be original drawings.

## `LimnerEditor`

| Member | What it is |
|---|---|
| `run(command: Dictionary) -> Variant` | Runs a command. Returns its result (`true` when it has none) or `null` after emitting `message(text, true)` with the reason. |
| `try_run(command) -> bool` | Runs a command quietly; returns whether it worked. |
| `begin(label)`, `commit()`, `cancel()` | Wrap several commands into one undo step, or abandon them. |
| `undo()`, `redo()` | |
| `status: Dictionary` | Refreshed every frame. See below. |
| `get_rows() -> Array` | One dictionary per layer in list order. See below. |
| `row(id) -> Dictionary` | The row for a layer, or `{}`. |
| `current_layer() -> int` | The row last selected, or -1. |
| `get_info() -> Dictionary` | Document settings and counts. |
| `get_tools() -> Dictionary` | Tool settings (see `ToolSettings`). |
| `get_options() -> Dictionary` | Editor options (see `Options`). |
| `set_option(key, value)` | Shorthand for `set_options`. |
| `set_tool_setting(tool, key, value)` | Shorthand for `set_tool_settings`, e.g. `("paint", "size", 0.02)`. |
| `scene.layer_details(id) -> Dictionary` | Everything about one layer. See below. |
| `message(text, is_error)` signal | Emit it yourself to show the user a short note. |

### `status`

`doc_revision`, `state_revision`, `dirty`, `can_undo`, `can_redo`, `undo_label`, `redo_label`, `can_transform_again`, `time` (flicks), `seconds`, `frame`, `playing`, `waiting` (held at a Stop), `tool`, `current_layer`, `paint_layer`, `transform_target` (ids or `null`), `selection_count` (selected strokes), `tool_active` (a trigger gesture is under way), `grabbing` (a grip holds something), `copied_keys` (the track the keys waiting to be pasted came from, or `null`).

With nothing to redo, `redo` repeats the last move of a copy (see `transform_again`); `can_redo` and `redo_label` say so.

### Layer rows

`id`, `parent` (`null` for the root), `depth`, `name`, `kind` (`group`, `sequence`, `paint`, `picture`, `model`, `sound`, `viewpoint`, `camera`, `light`), `visible`, `locked`, `collapsed`, `show_bounds`, `opacity` (its own, now), `shown` (visible now, counting ancestors and clips), `in_clip`, `hidden_in_list` (inside a folded group), `children`, `strokes`, `frames`, `animated`, `selected`, `current`, `paint_target`, `transform_target`, `default_viewpoint`.

### Layer details

`id`, `path`, `name`, `kind`, `visible`, `locked`, `opacity`, `transform` and `pivot` (`{rotation: [x, y, z, w], scale, translation: [x, y, z], flip: "none"|"x"|"y"|"z"}`), `props` (the kind's own settings, exactly the fields `set_layer_kind` accepts), `paint` (`{frames, drawings, rate, max_repeat, frame}` or `null`; `frames` is the frame table, one drawing index per frame, and `drawings` a count), `timeline_time`, `local_time`, `key_time`, `clips` (`[{start, end|null}]`), `keys` (`{transform, opacity, offset, actions}`, each a list of `{time, value, interp}`).

`props` by kind:

| Kind | Fields |
|---|---|
| `paint` | `shading` (`"unlit"`, `"lit"`), `max_repeat` (0 loops forever, 1 plays once and holds) |
| `sequence` | `loop_duration` (flicks, 0 = no loop), `max_repeat` |
| `group` | none |
| `light` | `kind` (`"point"`, `"spot"`, `"directional"`), `color`, `energy`, `range`, `spot_angle` (degrees, half-angle), `spot_blend` (0 to 1), `shadows` |
| `camera` | `fov` (vertical, degrees) |
| `viewpoint` | `floor_level`, `color`, `volume` (`[x, y, z, radius]`), `allow_translation` (`[bool; 3]`), `exporting`, `show_volume` |
| `sound` | `kind` (`"flat"`, `"positional"`, `"ambisonic"`), `gain`, `looped`, `attenuation` (`{mode: "none"|"linear"|"logarithmic", min, max}`), `modifier` (`"none"`, `{cone: {inner, band, outside}}` or `{frustum: {inner_h, inner_v, band, outside}}`, angles in radians) |
| `picture` | `projection` (`"flat"`, `"equirect"`, `"equirect_stereo"`, `"cubemap_cross"`, `"cubemap_strip"`), `viewer_locked` |
| `model` | `shading`, `wireframe` |

### Timeline rows

`scene.timeline_rows() -> Array` has one dictionary per layer, in the order of the layer rows, with what a timeline draws. Ask for it when the document changes (`status.doc_revision` moved on). For the part that moves with the playhead ask `scene.timeline_clock() -> Array` instead: one `{id, origin, time, local_time, frame}` per layer in the same order, cheap enough for every frame.

| Field | What it is |
|---|---|
| `id`, `kind` | As in the layer rows. |
| `origin` | The root-timeline time at which this layer's own clock reads zero. Zero for everything outside sequences; inside a sequence, where that sequence's clip starts (less its trim). Draw a key or clip at `origin + its time`. |
| `time` | The playhead on this layer's own clock: the time its keys and clips are read at. |
| `local_time` | Time since the layer's clip began, including its trim: what a paint layer's frames run on. |
| `clips` | `[{start, end or null, trim}]`, sorted, never overlapping. `end: null` is a clip that never stops. `trim` is how much of the content is cut off at the start. |
| `transform_keys` | `[{time, interp}]` |
| `opacity_keys` | `[{time, value, interp}]` |
| `actions` | `[{time, action}]` with `action` one of `"stop"`, `"play"`, `"loop"`. Chapter marks; they live on the root. |
| `paint` | For a paint layer `{frames, frame_duration, max_repeat, frame, empty}`: `frames` is the frame table (one drawing index per frame, so a run of equal numbers is a key frame and its holds), `frame` the index showing now, `empty` the drawing indices that hold no strokes. `null` otherwise. |
| `sequence` | For a sequence `{loop_duration, max_repeat}`; `loop_duration` 0 means it does not loop. `null` otherwise. |

A paint layer's frame `i` of clip `c` covers `origin + c.start - c.trim + i * frame_duration` for one `frame_duration`; when `max_repeat` is 0 the frames repeat to the end of the clip, otherwise the last frame holds.

Drawing indices are places in the layer's list of drawings, and commands that reorder frames may renumber them: after `reverse_frames` the table of a two-frame layer still reads `[0, 1]`, with the two drawings swapped. Read `empty` again with the table instead of remembering it.

### Document info

`title`, `description`, `background`, `rate` (`{num, den}`), `fps`, `frame_duration`, `range_start`, `range_end`, `default_viewpoint`, `camera_resolution`, `palette` (16 colours), `root`, `layers`, `strokes`, `points`, `flicks_per_second`.

### Tool settings

```
tool: "paint" | "line" | "colorize" | "eraser" | "select" | "grab" | "thicken" | "nudge" | "optimize"
color: [r, g, b]                      the paint colour, shared by Paint, Line and Colorize
paint:    {brush, size, opacity, width_taper, opacity_taper, directional, anim_duration}
line:     {brush, size, opacity, directional}
colorize: {size, inner, strength, mode}
eraser:   {size, inner, strength}
select:   {size}
grab:     {size, inner, strength}
thicken:  {size, inner, strength}
nudge:    {size, inner, strength}
optimize: {size, threshold}
symmetry: "off" | "x" | "y" | "z"     mirrored working, across the pivot of the paint layer
```

- `brush`: `"capped_cylinder"`, `"capped_ellipse"`, `"rounded_ribbon"`, `"cylinder"`, `"ellipse"`, `"ribbon"`, `"cube"` (the order of the Tools panel).
- `width_taper`, `opacity_taper`: `"none"`, `"pressure"`, `"auto"`, `"both"`.
- `mode`: `"normal"`, `"darken"`, `"multiply"`, `"overlay"`, `"burn"`, `"lighten"`, `"screen"`, `"add"`, `"dodge"`, `"hue"`, `"saturation"`, `"color"`, `"luminosity"`, `"opacity"` (the order of the Colorize panel).
- Sizes are radii in viewer-space metres, 0.0005 to 0.5; use an exponential slider. `inner` is a fraction of `size`. `opacity`, `strength`, `threshold` run 0 to 1. `anim_duration` is seconds.

### Options

`show_grid`, `onion_skin`, `onion_frames` (0 = all), `snap_to_frames`, `snap_to_clips`, `create_layers_at_playhead`, `loop_range`, `recording`, `select_in_all_layers`, `select_in_all_frames`, `auto_select` (`"none"`, `"layer"`, `"group"`), `show_gimbal`, `rotation_snap_steps`, `allow_translate`, `allow_rotate`, `allow_scale`, `level_horizon`, `animate_duplicates`, `mute`, `preview_spawn_area`, `show_extras`, `default_interp` (`"step"`, `"linear"`, `"ease_in"`, `"ease_out"`, `"ease_in_out"`, `"smooth"`).

## Commands

A command is a dictionary with a `cmd` entry. Fields marked optional may be left out.

### History
| Command | Fields | Result |
|---|---|---|
| `undo`, `redo` | | `{undone: label|null}` / `{redone: ...}` |

### Document
| Command | Fields |
|---|---|
| `set_document` | optional `title`, `description`, `background`, `ambient_light` (the light lit paint gets from everywhere), `rate` (`{num, den}`), `range_start`, `range_end`, `camera_resolution` (`[w, h]`) |
| `set_default_viewpoint` | `layer` (id or `null`) |
| `set_palette_color` | `index` (0 to 15), `color` |

### Layers
| Command | Fields | Result |
|---|---|---|
| `add_layer` | `kind` (`group`, `sequence`, `paint`, `viewpoint`, `camera`, `light`), optional `parent`, `index`, `name` | `{layer}` |
| `delete_layers`, `duplicate_layers` | `layers` (ids) | `{layers}` for duplicate |
| `set_layer` | `layer`, optional `name`, `visible`, `locked`, `collapsed`, `show_bounds`, `opacity`, `transform`, `pivot` | |
| `set_layer_kind` | `layer`, `values` (fields of `props` to change) | |
| `move_layer` | `layer`, `parent`, `index` | |
| `step_layer` | `layer`, `up` | |
| `group_layers` | `layers`, optional `name` | `{layer}` |
| `merge_down`, `flatten_group` | `layer` | `{layer}` |
| `set_group_is_sequence` | `layer`, `sequence` | |
| `flip_layer` | `layer`, `axis` (`"x"`, `"y"`, `"z"`) | |
| `reset_transform`, `reset_pivot`, `make_pivot_origin` | `layer` | |

Opacity and transform edits write a key at the playhead when the layer is already animated on that channel or when recording is on; otherwise they set the resting value.

### What is being worked on (not undoable)
| Command | Fields |
|---|---|
| `select_layers` | `layers`, optional `mode` (`replace`, `add`, `remove`, `toggle`). Fails, changing nothing, when one of them does not exist. |
| `set_transform_target` | `layer` (id, or `null` for the view) |
| `set_options` | `values` |
| `set_tool` | `tool` |
| `set_tool_settings` | `values`, nested by tool: `{"paint": {"size": 0.02}}` |
| `set_color` | `color` |

### Playback (not undoable)
`play`, `pause`, `toggle_play`, `seek` (`time`), `step_frames` (`frames`), `rewind`, `fast_forward` (to the chapter mark before or after, else the start or end of the range), `go_to_key_frame` (`layer`, `forward`: the paint layer's key frame before or after), `go_to_key` (`layer`, `track`, `forward`).

### Frames of a paint layer
Each acts on the frame the layer shows at the playhead.

| Command | Fields | Result |
|---|---|---|
| `add_frame` | `layer`, `kind` (`"blank"` or `"duplicate"`) | `{frame}`; the playhead moves on to the new frame |
| `add_hold_frame` | `layer`, optional `after` (default true) | `{frame}`; shows the same drawing one frame longer |
| `delete_frame` | `layer` | the animation gets one frame shorter |
| `remove_key_frame` | `layer` | the frame becomes a hold of the drawing before it |
| `clear_frames`, `double_frames`, `reverse_frames` | `layer` | |
| `copy_selection_to_frames` | optional `all` | `{copied}`; selected strokes into the frame shown, or into every frame |

### Clips
`clip` is an index into the layer's `clips`. Times are on the layer's own clock; when `time` is left out the playhead is used.

| Command | Fields |
|---|---|
| `toggle_clip` | `layer`, optional `time`: ends the clip there, or starts a new one |
| `split_clip` | `layer`, optional `time` |
| `move_clip` | `layer`, `clip`, `start`: the layer's own keys inside the clip move with it |
| `trim_clip` | `layer`, `clip`, optional `start`, `end`, `open_end` (true removes the end). The content stays where it is in time. A start pulled out past the beginning of the content stops there, so trimming in and out again is exact. |
| `delete_clip` | `layer`, `clip` |
| `duplicate_clip` | `layer`, `clip`, `start` |

### Keys
`track` is `"transform"`, `"opacity"` or `"action"`. `times` are key times on the layer's own clock.

| Command | Fields | Result |
|---|---|---|
| `set_key` | `layer`, `track`, optional `time`, `interp`, `children` (also key every layer inside) | `{keys}` written; keys the value the layer has at that moment |
| `delete_keys` | `layer`, `track`, optional `times` (none: the key at the playhead) | |
| `move_keys` | `layer`, `track`, `times`, `delta` | |
| `set_key_interp` | `layer`, `track`, `times`, `interp` | |
| `reverse_keys` | `layer`, `track`, `times` | |
| `copy_keys` (not undoable) | `layer`, `track`, `times` | |
| `paste_keys` | `layer`, optional `time` | the first copied key lands at `time` |
| `set_action` | optional `time`, `action` (`"stop"`, `"play"`, `"loop"` or `null` to remove) | chapter marks on the root timeline |
| `set_loop_point` | `layer` (a sequence), `enabled` | `{loop_duration}`; the sequence loops from the playhead back to its start |
| `bake_sequence` | `layer` (a looping sequence) | `{layer, frames}`; a frame-by-frame paint layer of one loop, the sequence kept hidden |

With `recording` on, any transform or opacity change writes a key at the playhead, including a layer carried by the hand while the piece plays.

### Strokes and selection
| Command | Fields |
|---|---|
| `select_all`, `deselect` | |
| `delete_selection` | |
| `selection_to_new_layer` | `copy` (bool) |
| `optimize` | `scope` (`"layer"` or `"all"`) |
| `add_strokes` | `strokes` (a list of `{points: [[x, y, z], ...], width, brush?, color?, opacity?, normal?}`), optional `layer` (the paint layer when left out), `space` (`"layer"` or `"viewer"`), `select`. Adds ready-made strokes to the frame the layer shows; returns `{strokes, layer}`. `editor.add_text(text, height, solid)` builds on it. |
| `bake_lighting` | `layer`: paints the light falling on the lit paint layers in and under it into their strokes and makes them unlit. Returns `{layers, points}`. |
| `transform_again` | After a copy was moved (a grab or a gimbal drag with the modifier held): makes another copy, moved as far again. With the option `animate_duplicates`, stroke copies land on a new frame each. Fails when there is nothing to repeat; see `status.can_transform_again`. |

## Hands, grabs and the gimbal

Shells feed the scene what the hands do, once per frame, and the core decides what that means. Panels do not need any of this; it is here for the shells (`godot/desktop`, `godot/xr`) and for anything drawn at the hands.

| `LimnerScene` method | What it does |
|---|---|
| `input_hand(which, position, rotation, trigger, grip, stick, buttons)` | One hand this frame, in viewer space. `which` 0 is the drawing hand. |
| `input_hand_ray(which, origin, direction)` | For a hand steered from a screen: the line of sight through the pointer. Call after `input_hand`. Handles of the gimbal are then picked along it. |
| `input_hand_absent(which)`, `input_head(position)` | |
| `input_commit(delta) -> Array` | Applies the frame. Returns feedback: `{type: "haptic", main, strength, seconds}`, `{type: "sound", name}`, `{type: "message", text}`, `{type: "menu"}`. |
| `input_cancel()` | Abandons the gesture or grab under way and undoes what it did. |
| `tool_cursor() -> Dictionary` | `{tracked, position, rotation, tool, radius, inner, color, opacity, active, alt, at_handle}` |
| `gimbal() -> Dictionary` | Empty when there is none; else `{subject, layer, position, rotation, size, hovered, active, arrow_start, arrow_end, plane_near, plane_far, center_radius}`. `GimbalView` draws it. |
| `is_grabbing(which) -> bool` | |
| `mirror_plane() -> Dictionary` | Empty unless mirrored working is on; else `{position, rotation, size}` of the mirror in viewer space. `LimnerWorld` draws it. |

Things that help while working and are not part of the picture (the grid, the gimbal, the mirror, the tool at the hand, gizmos, onion skins, the selection highlight) are drawn on render layers other than 1 (`LimnerWorld.HELPER_LAYERS`, and layer 20 for gizmos). Cameras that take pictures see layer 1 only; anything new of that kind should go on the helper layer too.

A grip takes hold of, in this order: a handle of the gimbal the hand is at; the selected strokes, when the Selection tool is in hand and its sphere reaches them; with `auto_select` on, the layer (or its group) of the stroke at the hand; the layer chosen with `set_transform_target`; otherwise the view. A second grip adds scale, the held hand's stick zooms, and `allow_translate`, `allow_rotate`, `allow_scale` and `level_horizon` limit the motion. Holding the modifier when the grab begins moves a copy.

The gimbal shows when `show_gimbal` is on and either a layer is the transform object or the Selection tool has a selection. A grip on a handle moves or turns the thing; the trigger on an arrow or the middle moves the pivot, and on a ring turns in `rotation_snap_steps` steps. On the desktop the left button over a handle is the grip, and with Alt the trigger.

## Files and media

| Call | What it does |
|---|---|
| `editor.new_document()`, `editor.open_document(path) -> bool`, `editor.save_document(path, thumbnail) -> bool` | A path ending in `.limnz` saves one packed file; any other path is a project folder. Opening also takes a Quill project folder. |
| `editor.import_file(path) -> int` | Brings a picture (`.png`, `.jpg`), a sound (`.wav`, `.ogg`, `.mp3`) or a model (`.obj`, `.glb`, `.gltf`, `.fbx`) in as a new layer after the current one, as one undo step. Returns the new layer's id or -1, and tells the user what the file had that was not brought in. |

| `scene.export_file(path, options) -> Dictionary` | Writes the document as `.glb`, `.usda`, `.usdz`, `.fbx` or `.obj`, chosen by the path. Returns `{ok, error, nodes, meshes, vertices, triangles, animation_frames, bytes}`. Options are described in `docs/export.md` and `docs/capture.md`; an unknown option is named in the error. |
| `scene.save_copy(path) -> Dictionary` | Saves the document and the editor's state to another place without making that the document's home or clearing "edited": what a render in a second process is started from. |

Pictures and video are taken through `LimnerCapture.of(editor)` and `RenderLauncher`; see `docs/capture.md`.

Materials are made once by `LimnerWorld` and handed to the scene (`set_stroke_materials`, `set_lit_stroke_materials`, `set_layer_materials`, `set_highlight_material`); nothing else should call those.

## The Timeline panel

Three files in `godot/ui/`: `timeline_panel.gd` (the two rows of buttons, what is selected, every command), `timeline_view.gd` (names, ruler, tracks, the playhead, scroll bars, and all presses and drags) and `timeline_scale.gd` (time to pixels and back, zoom, the frame grid). The same panel is mounted in the desktop shell's bottom dock and on the headset palette.

How it behaves, where that is not obvious:

- The tracks read `timeline_rows` when the document changes and `timeline_clock` when only the playhead or the editor's state did. The playhead line is its own overlay, so moving it redraws nothing else.
- A drag on a clip or on keys is replayed from where it began on every pointer move, inside one edit, and committed on release: one undo step, and none when the drag ends where it started. A clip cannot be dragged over its neighbours.
- Pressing a clip or a key makes its layer current. Pressing a key of another kind switches the key type; the root switches it to chapter marks. Keys of several layers can be selected together; the clip selection is dropped when another layer becomes current.
- A drag that starts on empty track space selects the keys it passes over. Clips cover most tracks, so Shift, or the "select a range" switch in the Keys group, makes a drag that starts on a clip do the same.
- The context row shows the Frames group for a paint layer, Sequence for a sequence and Chapters for the root. What does not apply is greyed out, not hidden, so buttons stay where they are.
- Besides the thin grip along its top edge the panel has a resize button, which a controller tip can hit.

Left rough:

- The view does not scroll by itself when a drag nears its edge, and Escape does not cancel a drag on the tracks.
- Pinch and two-finger panning are written but have never been driven by a real trackpad.
- The two button rows wrap, so a narrow panel is taller: about 290 px at 900 wide against 230 at 1440.
- In the development build, scrubbing a document with some 300 layers and 9,000 keys took 32 ms a frame before the clock query existed. It has not been measured again since.

## Trying things out

```bash
cargo build -p limner-godot
scripts/godot.sh --headless --path godot --import                 # reports script errors
scripts/shot.sh local/out/shot.png --open <project> --frame-all   # windowed run, saves a screenshot
scripts/shot.sh local/out/shot.png --exec '{"cmd":"add_layer","kind":"light"}'
```

`--exec` runs a command at start and may be repeated. See the top of `godot/main.gd` for all options.
