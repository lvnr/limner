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
8. The same panels are shown on the desktop and on a hand-held palette in a headset, where a controller tip pokes them, and they follow Quill's rules (`docs/superpowers/specs/2026-10-22-quill-style-panels-design.md`): a panel is a `LimnerPanel` with one fixed size in design pixels, built with `Kit.u()` sizes; nothing in it scrolls and nothing opens a list over it, so it holds no `ScrollContainer`, `OptionButton`, `ItemList` or `Tree`; what changes with the tool or the layer kind is swapped in place within the size. A choice among a few is a `KeyGrid` (or a `ChoiceRow`), among many a `Cycler`; a long list shows a page at a time with a `Pager`. Keys (`Kit.key`, `Kit.text_key`) are at least `Kit.KEY` design pixels (25 mm in a headset). Never rely on hover, right-click or the keyboard as the only way to reach something; the keyboard is for names and text. A test calls `check_fits(panel)` for every state the panel can show (see `godot/tests/test_panels_fit.gd`).
9. GDScript is statically typed here and "inferred from Variant" is an error: give variables explicit types when the right-hand side is a Variant (`var row: Dictionary = ...`, `var id: int = item.get_metadata(0)`).
10. Use `Kit` for every control and `Icons.texture(name, color, pixels)` for icons, so the look stays in one place. Add an icon by adding an SVG body to `Icons.BODIES` (24 x 24 grid, stroke only, `{c}` for the colour). Icons must be original drawings.
11. Never write a colour of your own: there is a dark and a light scheme, and the user can switch between them while a panel is open. Read colours from `Kit` (`Kit.TEXT`, `Kit.TEXT_DIM`, `Kit.ACCENT`, `Kit.BG_SUNKEN` and the other roles listed at the top of `godot/ui/kit.gd`) at the moment you build or draw, never into a `const`. On a switch, the shell re-applies the theme to its root with `Kit.apply_theme`; that also redraws every control, so a `_draw` that reads `Kit` is right by itself, and it brings in line theme overrides and `Icons` icons set from a role's exact colour (`Kit.TEXT`, not `Kit.TEXT.darkened(0.2)`: a derived colour is not recognised; add a role instead). Anything else a panel built from colours, such as a `Tree` item's custom colour, it sets again in a method `apply_scheme()`, which is called after the switch. Colours that mean something whatever the scheme (a layer kind's clip colour, the white of a key drawn over a clip) may stay fixed.

## Building blocks of the panels

All in `godot/ui/`.

| Script | What it is |
|---|---|
| `limner_panel.gd` `LimnerPanel` | The base of every panel: a column of one fixed size (`design_size`, in design pixels; `pixel_size()` in the interface's pixels). `fits()` lists what keeps it from fitting: content larger than the size (the height only in a headset, where the panel is a sheet that must never scroll; on the desktop a dock gives it its height) and anything that scrolls or opens a list. `split_columns(first_right)` and the static `columns_of(container, first_right)` put a long column of controls into two side by side. |
| `kit.gd` `Kit` | `Kit.u(design_pixels)` is a size in the interface in use: as given in a headset, where a design pixel is two thirds of a millimetre, and `DESKTOP_SCALE` (0.8) of it on the desktop. `Kit.text_size(role)` gives text sizes ("title", "body", "caption", "small", "big"), which do not shrink on the desktop. `Kit.KEY` (38), `KEY_BIG` (52), `KEY_SMALL` (26), `SPACE`, `MARGIN` are design sizes. `Kit.key(icon, tooltip, on_pressed, toggle, size)` makes a square key whose white icon the theme tints for each state; `Kit.set_key_icon` changes it. `Kit.text_key(text, tooltip, on_pressed, icon, toggle)` makes one with words. `Kit.caption`, `Kit.title`, `Kit.key_row`, `Kit.key_column`. `Kit.choice(items, tooltip, on_selected)` makes a `Cycler` where `Kit.option` makes a drop-down. |
| `key_grid.gd` `KeyGrid` | Keys in rows of `columns`, one of which is down: `add_icon`, `add_text`, `set_value`, `get_value`, signal `chosen(key)`. |
| `cycler.gd` `Cycler` | A choice among many: the value with a key each side. `set_choices([[key, text], …])`, `set_value`, `get_value`, `set_enabled`, signal `chosen(key)`. It also answers to the parts of `OptionButton` panels use (`item_count`, `selected`, `select`, `add_item`, `get_item_text`, item metadata, `disabled`, signal `item_selected(index)`), so it takes a drop-down's place with nothing else changing. `LayerChoice` (layers of some kinds, signal `layer_chosen(layer)`) and `SizeChoice` are cyclers. |
| `pager.gd` `Pager` | The page keys of a list shown a page at a time: `per_page`, `count`, `first`, `set_count`, `show_item(index)` (the page holding it), `step(by)`, signal `page_changed(first)`. The owner draws the items `first` to `first + per_page`. |
| `step_key.gd` `StepKey` | A key that steps through a few settings, written small under it, and is down whenever the setting is not the first (off): the tapers, the mirror. |
| `opacity_ramp.gd` `OpacityRamp` | The paint colour fading out over a checker, dragged like a slider. |
| `layer_rows.gd` `LayerRows` | The page of layer rows of the Layers panel; see below. |

Icons: `Icons.texture(name, colour, pixels)` draws the filled icon called `name` (`docs/icons.md`) or a line icon.

## `LimnerEditor`

| Member | What it is |
|---|---|
| `run(command: Dictionary) -> Variant` | Runs a command. Returns its result (`true` when it has none) or `null` after emitting `message(text, true)` with the reason. A result that carries a `message` (adding the first light, see `add_layer`) is passed on as `message(text, false)`. |
| `try_run(command) -> bool` | Runs a command quietly; returns whether it worked. |
| `begin(label)`, `commit()`, `cancel()` | Wrap several commands into one undo step, or abandon them. |
| `undo()`, `redo()` | |
| `status: Dictionary` | Refreshed every frame. See below. |
| `get_rows() -> Array` | One dictionary per layer in list order. See below. |
| `catch_up()` | Reads `status`, the rows, info and settings again now. A command's change signals arrive on the next frame; code that runs a command and reads what it did in the same frame (a context menu built right after the click selected a row, several layers moved one after another, several wheel steps in one frame) calls this in between. |
| `row(id) -> Dictionary` | The row for a layer, or `{}`. |
| `current_layer() -> int` | The row last selected, or -1. |
| `get_info() -> Dictionary` | Document settings and counts, among them `paint_layers` and `lit_paint_layers` (how many paint layers scene lights shade). |
| `get_tools() -> Dictionary` | Tool settings (see `ToolSettings`). |
| `get_options() -> Dictionary` | Editor options (see `Options`). |
| `set_option(key, value)` | Shorthand for `set_options`. |
| `set_tool_setting(tool, key, value)` | Shorthand for `set_tool_settings`, e.g. `("paint", "size", 0.02)`. |
| `scene.layer_details(id) -> Dictionary` | Everything about one layer. See below. |
| `message(text, is_error)` signal | Emit it yourself to show the user a short note. |

### `status`

`doc_revision`, `state_revision`, `dirty`, `can_undo`, `can_redo`, `undo_label`, `redo_label`, `can_transform_again`, `time` (flicks), `seconds`, `frame`, `playing`, `waiting` (held at a Stop), `tool`, `current_layer`, `paint_layer` (the current layer when it is a paint layer, else `null`), `transform_target` (ids or `null`), `selection_count` (selected strokes), `tool_active` (a trigger gesture is under way, a handle of the gimbal included), `painting` (Paint or Line is laying down a stroke: what the hiss of paint follows), `grabbing` (a grip holds something), `copied_keys` (the track the keys waiting to be pasted came from, or `null`).

With nothing to redo, `redo` repeats the last move of a copy (see `transform_again`); `can_redo` and `redo_label` say so.

### Layer rows

`id`, `parent` (`null` for the root), `depth`, `name`, `kind` (`group`, `sequence`, `paint`, `picture`, `model`, `sound`, `viewpoint`, `camera`, `light`), `visible`, `locked`, `collapsed`, `show_bounds`, `opacity` (its own, now), `shown` (visible now, counting ancestors and clips), `in_clip`, `hidden_in_list` (inside a folded group), `children`, `strokes`, `frames`, `animated`, `selected`, `current`, `paint_target`, `transform_target`, `default_viewpoint`.

### Layer details

`id`, `path`, `name`, `kind`, `visible`, `locked`, `opacity`, `transform` and `pivot` (`{rotation: [x, y, z, w], scale, translation: [x, y, z], flip: "none"|"x"|"y"|"z"}`), `props` (the kind's own settings, exactly the fields `set_layer_kind` accepts), `paint` (`{frames, drawings, rate, max_repeat, frame}` or `null`; `frames` is the frame table, one drawing index per frame, and `drawings` a count), `timeline_time`, `local_time`, `key_time`, `clips` (`[{start, end|null}]`), `keys` (`{transform, opacity, offset, actions}`, each a list of `{time, value, interp}`).

`props` by kind:

| Kind | Fields |
|---|---|
| `paint` | `shading` (`"unlit"`, `"lit"`), `max_repeat` (0 loops forever, 1 plays once and holds) |
| `sequence` | `loop_duration` (flicks, 0 = no loop, at most 10,000 hours), `max_repeat` |
| `group` | none |
| `light` | `kind` (`"point"`, `"spot"`, `"directional"`), `color`, `energy`, `range`, `spot_angle` (degrees, half-angle), `spot_blend` (0 to 1), `shadows` |
| `camera` | `fov` (vertical, degrees) |
| `viewpoint` | `floor_level`, `color`, `volume` (`[x, y, z, radius]`), `allow_translation` (`[bool; 3]`), `exporting`, `show_volume` |
| `sound` | `kind` (`"flat"`, `"positional"`, `"ambisonic"`), `gain`, `looped`, `attenuation` (`{mode: "none"|"linear"|"logarithmic", min, max}`), `modifier` (`"none"`, `{cone: {inner, band, outside}}` or `{frustum: {inner_h, inner_v, band, outside}}`, angles in radians; a shape may name only some of its values: the rest keep what the sound has, or, when it changes shape, its `band` and `outside` and the new shape's starting angles) |
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
symmetry: "off" | "x" | "y" | "z"     mirrored working, across the pivot of the paint layer; turned on for a layer whose
                                      pivot is not set, it sets one 0.45 m ahead of the eyes and 0.25 m below them, square
                                      to the way they face, as its own undo step ("Place the mirror")
```

- `brush`: `"capped_cylinder"`, `"capped_ellipse"`, `"rounded_ribbon"`, `"cylinder"`, `"ellipse"`, `"ribbon"`, `"cube"` (the order of the Tools panel).
- `width_taper`, `opacity_taper`: `"none"`, `"pressure"`, `"auto"`, `"both"`.
- `mode`: `"normal"`, `"darken"`, `"multiply"`, `"overlay"`, `"burn"`, `"lighten"`, `"screen"`, `"add"`, `"dodge"`, `"hue"`, `"saturation"`, `"color"`, `"luminosity"`, `"opacity"` (the order of the Colorize panel).
- Sizes are radii in viewer-space metres, 0.0005 to 0.5; use an exponential slider. `inner` is a fraction of `size`. `opacity`, `strength`, `threshold` run 0 to 1. `anim_duration` is seconds.

### What the tools work on

The tools work on the current layer (the last row selected); there is no separate paint target. `status.paint_layer` and a row's `paint_target` name the current layer when it is a paint layer, and nothing otherwise.

- **Paint and Line.** A paint layer current: the stroke goes into the frame it shows. A group, a sequence or the root current (or nothing at all): a new paint layer is made inside it, at the top (with nothing current, at the end of the root), and becomes current; the layer and the stroke are one undo step. A locked group or paint layer refuses. A camera, light, spawn area, picture, model or sound current: the trigger does nothing and gives one message, such as `"Camera" is a camera layer. Choose a paint layer or a group to paint in.`
- **Colorize, Eraser, Thicken, Nudge, Optimize and the Grab tool.** With strokes selected they work on the selection, whatever is current. Otherwise only on a current, unlocked paint layer; anything else refuses with a message (`… Choose a paint layer to work on, or select strokes.`).
- **Selection tool.** Sweeps the current paint layer, or every visible unlocked one with `select_in_all_layers`. A plain press deselects whatever is current.
- The lower button's delete-at-hand and its blink, onion skins, the mirror plane, `optimize` with `scope: "layer"`, `copy_selection_to_frames` and `add_strokes` without a `layer` follow the same rule; with no paint layer current they do nothing (or fail, for commands; `add_strokes` makes a layer in a current group, as the brush does). The other hand's frame stick says why once per push.
- **Modifier with Paint:** the brush draws a free straight line in its own brush, size and opacity, and the drawing hand's stick resizes it while it is pulled. **Modifier with Line:** both ends of the line snap to the nearest crossings of the brush grid, in world space, at `LimnerScene.brush_grid_spacing()` (1, 2 or 5 times a power of ten, nearest to 5 cm as seen; the core's `brush_grid_spacing`). The brush grid shows while that is so, even with `show_brush_grid` off.
- A trigger pressed while the drawing hand's own grip holds something (the view, a layer, a handle) does not start the tool; a gesture already under way carries on.

### Options

`show_grid`, `onion_skin`, `onion_frames` (0 = all), `snap_to_frames`, `snap_to_clips`, `create_layers_at_playhead`, `loop_range`, `recording`, `select_in_all_layers`, `select_in_all_frames`, `auto_select` (`"none"`, `"layer"`, `"group"`), `show_gimbal`, `rotation_snap_steps`, `allow_translate`, `allow_rotate`, `allow_scale`, `level_horizon`, `animate_duplicates`, `mute`, `preview_spawn_area`, `show_extras`, `default_interp` (`"step"`, `"linear"`, `"ease_in"`, `"ease_out"`, `"ease_in_out"`, `"smooth"`), `painting_mode` (the other hand's stick and its click do nothing; the Layers panel sets it when its timeline is closed), `show_stroke_at_hand` (blink the stroke the lower button would delete), `show_brush_grid` (a small grid in the air around the drawing hand; the shells pass it on to `LimnerWorld.show_brush_grid`).

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
| `place_viewpoint_here` | `layer` (a spawn area): moves it to where the viewer stands now, upright and at the current zoom, so that standing in it gives this view back |
| `set_palette_color` | `index` (0 to 15), `color` |

### Layers
| Command | Fields | Result |
|---|---|---|
| `add_layer` | `kind` (`group`, `sequence`, `paint`, `viewpoint`, `camera`, `light`), optional `parent`, `index`, `name` | `{layer}`, and `message` when there is something to tell the user. A light is put where it shows (below). A paint layer added while the document has a light and some lit paint is lit itself. |
| `delete_layers`, `duplicate_layers` | `layers` (ids) | `{layers}` for duplicate |
| `set_layer` | `layer`, optional `name`, `visible`, `locked`, `collapsed`, `show_bounds`, `opacity`, `transform`, `pivot` | A `transform` or `pivot` whose rotation has (nearly) zero length, or whose scale is not above zero, is refused; a rotation of any other length is normalised. |
| `set_layer_kind` | `layer`, `values` (fields of `props` to change) | |
| `move_layer` | `layer`, `parent`, `index` | |
| `step_layer` | `layer`, `up` | |
| `group_layers` | `layers`, optional `name` | `{layer}` |
| `merge_down`, `flatten_group` | `layer` | `{layer}` |
| `set_group_is_sequence` | `layer`, `sequence` | |
| `flip_layer` | `layer`, `axis` (`"x"`, `"y"`, `"z"`) | |
| `reset_transform`, `reset_pivot`, `make_pivot_origin` | `layer` | |

Opacity and transform edits write a key at the playhead when the layer is already animated on that channel or when recording is on; otherwise they set the resting value.

**A new light** (`add_layer` with `kind: "light"`, from any interface) is placed where it can be seen and lights the work, from what the session knows: the eyes (`input_head`), the hands and the view. It goes 0.8 m ahead of the eyes and 0.3 m above them, in the room, and points at a spot 1.3 m ahead and 0.2 m below the eyes: 45 degrees down. "Ahead" is level with the floor, toward the middle of the tracked hands, or straight ahead of the room (-Z, where the desktop's camera looks) when no hand is tracked. The layer keeps a scale of one in its parent, and a point or spot light reaches 3 m of the room as seen when it was added (`range` in the parent's units). The constants are in `crates/limner-core/src/session/lights.rs`. When no paint layer in the document is lit, adding a light also sets every paint layer that is not locked to `lit`, in the same undo step, and the result carries `message`: "Paint layers now take light. A layer set to Unlit in its properties stays flat."

### What is being worked on (not undoable)
| Command | Fields |
|---|---|
| `select_layers` | `layers`, optional `mode` (`replace`, `add`, `remove`, `toggle`). Fails, changing nothing, when one of them does not exist. The last row selected is the current layer, and the tools work on it (see below). |
| `set_transform_target` | `layer` (id, or `null` for the view) |
| `set_options` | `values` |
| `set_tool` | `tool` |
| `set_tool_settings` | `values`, nested by tool: `{"paint": {"size": 0.02}}` |
| `set_color` | `color` |

### Playback (not undoable)
`play`, `pause`, `toggle_play`, `seek` (`time`; kept between 0 and 10,000 hours), `step_frames` (`frames`), `rewind`, `fast_forward` (to the chapter mark before or after, else the start or end of the range), `go_to_key_frame` (`layer`, `forward`: the paint layer's key frame before or after), `go_to_key` (`layer`, `track`, `forward`).

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
| `add_strokes` | `strokes` (a list of `{points: [[x, y, z], ...], width, brush?, color?, opacity?, normal?}`), optional `layer` (the paint layer when left out), `space` (`"layer"` or `"viewer"`), `select`. Adds ready-made strokes to the frame the layer shows; returns `{strokes, layer}`. `editor.add_text(text, height, solid, font, align)` builds on it: line breaks start new lines, the block is centred in front of the viewer, `align` is a `HorizontalAlignment` (left, centre, right), and `font` comes from `TextStrokes.font_named(family, bold, italic)` (empty family: the built-in font; `null` when the family is not installed; a family without its own bold or italic face is thickened or slanted). |
| `set_shading` | `shading` (`"lit"` or `"unlit"`), optional `layers`: sets it on each paint layer listed and on every paint layer inside each group listed, or with no `layers` on every paint layer in the document, as one undo step ("Change shading"). Locked layers and layers in a locked group are left alone. Returns `{changed}`. The Layer panel's "Light all paint layers" sends it. |
| `bake_lighting` | `layer`: paints the light falling on the lit paint layers in and under it into their strokes and makes them unlit, with the shadows of lights whose `shadows` is on. Returns `{layers, points}`. |
| `transform_again` | After a copy was moved (a grab or a gimbal drag with the modifier held): makes another copy, moved as far again. With the option `animate_duplicates`, stroke copies land on a new frame each. Fails when there is nothing to repeat; see `status.can_transform_again`. |

## Hands, grabs and the gimbal

Shells feed the scene what the hands do, once per frame, and the core decides what that means. Panels do not need any of this; it is here for the shells (`godot/desktop`, `godot/xr`) and for anything drawn at the hands.

Only one shell runs at a time, but which one can change while the app runs (`LimnerApp.switch_to` in `godot/main.gd`; `docs/xr.md`, "Changing to the desktop and back"). The world, the editor and the document stay; a shell puts nothing in the world that its `stop()` does not take away again, and a panel keeps no state that only its shell's lifetime would clear. The Settings panel's `switch_requested(to)` is how a panel asks for the change.

| `LimnerScene` method | What it does |
|---|---|
| `input_hand(which, position, rotation, trigger, grip, stick, buttons)` | One hand this frame, in viewer space. `which` 0 is the drawing hand. |
| `input_hand_ray(which, origin, direction)` | For a hand steered from a screen: the line of sight through the pointer. Call after `input_hand`. Handles of the gimbal are then picked along it. |
| `input_hand_absent(which)`, `input_head(position)`, `input_head_facing(forward)` | Where the eyes are and which way they look, in viewer space; the facing is kept until given again (it places the mirror). |
| `input_commit(delta) -> Array` | Applies the frame. Returns feedback: `{type: "haptic", main, strength, seconds}`, `{type: "sound", name}`, `{type: "message", text}`, `{type: "menu"}`. |
| `input_cancel()` | Abandons the gesture or grab under way and undoes what it did. |
| `tool_cursor() -> Dictionary` | `{tracked, position, mirror_position, rotation, tool, radius, inner, color, opacity, active, painting, blocked, grid_snap, alt, at_handle}`. `active`: the trigger is working the tool or a handle. `painting`: Paint or Line is laying down a stroke. `blocked`: the message the trigger would give because the current layer is not one the tool can work on, or `""`; `ToolCursor` then draws a hollow grey ring. `grid_snap`: the Line tool is in hand with the modifier held. |
| `brush_grid_spacing() -> float` | The size of a cell of the brush grid at this zoom, in world units: what `LimnerWorld` draws and the Line tool snaps to. |
| `gimbal() -> Dictionary` | Empty when there is none; else `{subject, layer, position, rotation, size, hovered, active, arrow_start, arrow_end, plane_near, plane_far, center_radius}`. `GimbalView` draws it. |
| `is_grabbing(which) -> bool` | |
| `mirror_plane() -> Dictionary` | Empty unless mirrored working is on; else `{position, rotation, size}` of the mirror in viewer space. `LimnerWorld` draws it. |
| `stroke_at_hand() -> Dictionary` | Empty, or `{layer, drawing, stroke}`: the stroke the lower button would delete if it were pressed now. The scene draws a blink over it by itself. Nothing is named while a tool or a grip is at work, nor after it until the hand has been away from every stroke, so what was just painted does not blink under the brush. |

Things that help while working and are not part of the picture (the grid, the gimbal, the mirror, the tool at the hand, gizmos, onion skins, the selection highlight, the blink on the stroke at the hand) are drawn on render layers other than 1 (`LimnerWorld.HELPER_LAYERS`, and layer 20 for gizmos). Cameras that take pictures see layer 1 only; anything new of that kind should go on the helper layer too.

A grip takes hold of, in this order: a handle of the gimbal the hand is at; the selected strokes, when the Selection tool is in hand and its sphere reaches them; with `auto_select` on, the layer (or its group) of the stroke at the hand; the layer chosen with `set_transform_target`; otherwise the view. A second grip adds scale, the held hand's stick zooms, and `allow_translate`, `allow_rotate`, `allow_scale` and `level_horizon` limit the motion. Holding the modifier when the grab begins moves a copy.

Selected strokes carried by a grip, or turned by the trigger on a ring of the gimbal, stay where they are in the document until the grip or trigger lets go; the scene shows them, and their highlight, moved, so a frame of the carry costs the same however much is selected. The move is then written into the strokes once, as one undo step. Anything that reads stroke positions (commands, `layer_details`, saving) sees them where they were until then; the gimbal and the selection's pivot follow the hand all along. In the core this is `Session::selection_motion()`, and `SelectionMotion::split` divides a drawing into what stays and what moves.

The gimbal shows when `show_gimbal` is on, for: the selection, when the Selection tool is in hand and strokes are selected; else the layer that is the transform object; else, while the view is the transform object, the current layer, unless that is the root or locked. A grip on a handle moves or turns the thing, and so does the trigger, except that on a ring the trigger turns it in `rotation_snap_steps` steps. With the modifier held, the trigger on an arrow, a square or the middle moves the pivot, and on a ring it turns a copy (as the modifier with a grip moves a copy). On the desktop the left button over a handle is the grip; with Alt it moves the pivot, or on a ring turns in steps. A click of the drawing hand's stick shows or hides it; shown with nothing to move, it gives the message "Nothing to transform: choose a layer, or select strokes". Giving the transform object back to the view (`set_transform_target` with no layer) puts the gimbal away (`show_gimbal` off) rather than moving it to the current layer, and so does `deselect` when the gimbal was the selection's.

A layer's gimbal sits at its pivot. A pivot that was never set (the layer's origin, unturned: the identity, as a new layer and most painted ones have) puts the gimbal at the middle of the layer's content instead, as `reset_pivot` would, or at the origin when the layer has no content of its own. The moment the gimbal starts to move, turn or re-pivot such a layer, that middle is written as the layer's pivot, in the same undo step, so the layer turns about it and its animation interpolates about it from then on; painting more does not move it. The file format is unchanged: a pivot that is the identity is simply one not yet set. One consequence: after `make_pivot_origin` the pivot is the identity again, so the gimbal goes back to the middle of the content.

## Files and media

| Call | What it does |
|---|---|
| `editor.new_document()`, `editor.open_document(path) -> bool`, `editor.save_document(path, thumbnail) -> bool` | A path ending in `.limnz` saves one packed file; any other path is a project folder. Opening also takes a Quill project folder. |
| `editor.import_file(path) -> int` | Brings a picture (`.png`, `.jpg`), a sound (`.wav`, `.ogg`, `.mp3`) or a model (`.obj`, `.glb`, `.gltf`, `.fbx`) in as a new layer after the current one, as one undo step. Returns the new layer's id or -1, and tells the user what the file had that was not brought in. |

| `scene.export_file(path, options) -> Dictionary` | Writes the document as `.glb`, `.usda`, `.usdz`, `.fbx` or `.obj`, chosen by the path. Returns `{ok, error, nodes, meshes, vertices, triangles, animation_frames, bytes}`. Options are described in `docs/export.md` and `docs/capture.md`; an unknown option is named in the error. |
| `scene.save_copy(path) -> Dictionary` | Saves the document and the editor's state to another place without making that the document's home or clearing "edited": what a render in a second process is started from. |

Without system file dialogs (in a headset, and from the Document panel anywhere) documents start in one folder, `Documents.folder()` (`godot/editor/documents.gd`): `Limner` in the user's Documents folder, or `user://documents` on Android. It is made when something is first saved there. The panel can browse to any other folder (see below).

| Call | What it does |
|---|---|
| `Documents.path_for(title, in_folder) -> String` | The `.limnz` file a document named `title` is saved as in `in_folder` (by default the documents folder), with what a file name cannot hold left out; empty when nothing is left. |
| `Documents.projects(in_folder) -> Array[Dictionary]` | The projects in a folder (by default the documents folder), newest first: `{path, name, time, kind}`, `kind` one of `limnz`, `limn` (a folder) and `quill` (a Quill project folder). `time` is seconds since 1970; `Documents.local_time(time)` writes it as the user's clock does. |
| `Documents.importable(in_folder) -> Array[Dictionary]` | Pictures, sounds and models in a folder, and the projects, newest first, with `kind` `picture`, `sound` or `model` for the files. |
| `Documents.folders(in_folder) -> Array[Dictionary]` | The folders in a folder that can be gone into, by name, with `kind` `folder`: not hidden ones, and not project folders, which `projects` lists. |
| `Documents.places() -> Array[Array]` | `[path, name]` pairs of the usual places to look: the documents folder, the user's Documents, Desktop, Downloads, Pictures, Videos and home folders that exist, and each drive on Windows. On Android only the documents folder. |
| `Documents.parent_of(path)`, `Documents.browsing()`, `Documents.last_folder` | The folder above (the same path at the top of a drive); the folder the panel shows first, the one last browsed to in this run while it exists. |
| `Documents.thumbnail(editor) -> PackedByteArray` | A 512 x 288 PNG of the artwork from the current view, taken off screen (no panels in it), or nothing without a display. |
| `Documents.use_folder(path)` | For tests: another folder; `""` goes back. |

The Document panel's File section is built on these: **Save** (to the document's file, or a name first when it has none of its own), **Save as** (a name box; a name already taken is replaced only on a second press; **Folder** chooses where, with "Save here"), **New**, **Open** (the projects, newest first, with their dates) and **Import** (anything importable, as a new layer). Open, Import and Folder show a `FolderBrowser` (`godot/ui/folder_browser.gd`): a key to the folder above, the places on a cycler, the folder shown, then its folders (which open when pressed) and its files, a page at a time. The panel shows the folder last browsed to first, and Save as saves there unless another is chosen. In the Export panel, a headset chooses the folder among the places on a cycler (`SaveTarget`); the desktop has its save dialog. Before New or Open replaces a document with unsaved changes the panel asks, in itself: Save (which asks for a name first when there is no file, then goes on), Don't save, Cancel. It emits `document_opened(native)` after opening, so that a shell can frame a Quill project. Both shells mount it; the desktop keeps its File menu and system dialogs besides.

Pictures and video are taken through `LimnerCapture.of(editor)` and `RenderLauncher`; see `docs/capture.md`.

Materials are made once by `LimnerWorld` and handed to the scene (`set_stroke_materials`, `set_lit_stroke_materials`, `set_layer_materials`, `set_highlight_material`, `set_at_hand_material`); nothing else should call those.

## The Layers panel and its timeline

Quill's Layers panel holds the timeline, and so does Limner's. Four files in `godot/ui/`: `timeline_panel.gd` (`TimelinePanel`, the panel called Layers: playback, the keys that add and arrange layers, the timeline's options and commands, what is selected, every command), `layer_rows.gd` (`LayerRows`, the page of layer rows), `timeline_view.gd` (the ruler, the tracks of the rows on the page, the playhead, and all presses and drags on them) and `timeline_scale.gd` (time to pixels and back, zoom, the frame grid). The same panel is the desktop shell's bottom dock and a sheet on the headset palette.

- Playback runs across the top. On the left: keys that add a paint layer, group, sequence, spawn area, camera or light, and Import (signal `import_requested`), then New frame and Copy frame (`add_frame` on the current paint layer, there with the timeline closed too); rename, duplicate, delete, up, down, group, merge, flatten; the opacity of the current layer; then the rows. On the right, when the timeline is open: its options, the commands for the current layer, and the tracks.
- `LayerRows` shows `per_page` rows (ten in a headset; on the desktop as many as the panel's height holds, which its grip sets). Each row: a fold key for a group, the name as a key (indented by depth; with Ctrl, or the other hand's trigger in a headset, it joins or leaves the selection; with Shift it selects from the current layer to it), then keys for shown, kind (signal `settings_requested(layer)`: the shells show the Layer panel), moved when grabbing, locked and bounds. Rename turns the name into a box. The page follows the current layer; the wheel or the controller's stick over the rows steps the pages, as do page keys under them. The desktop's right click on a name opens a menu of shortcuts; a headset has none.
- The tracks draw the rows on the page and no others (`TimelinePanel.rows`), each `LayerRows.pitch()` tall, under a ruler as tall as the space above the first row, so that rows and tracks line up. The band of keys over the rows is kept as tall as the one over the tracks, whose keys change with the current layer and can wrap to more lines (`_match_bands`); in a headset that band is planned four lines tall (`BAND_HEIGHT`, against `DESKTOP_BAND_HEIGHT`). `test_rows_and_tracks_line_up_in_both_interfaces_for_every_kind` checks it. The view has no column of names and no scroll bars there. In a headset keys and frame marks are drawn 1.7 times as large (`TimelineView.mark_scale`), and so is the reach of a press on one.
- Closing the timeline (the timeline key at the end of the top row) leaves the layers, and sets the `painting_mode` option, as collapsing the timeline does in Quill: the other hand's stick no longer makes, holds, deletes or steps frames, and its click no longer plays. The panel follows the option, so both shells agree.

How it behaves, where that is not obvious:

- The tracks read `timeline_rows` when the document changes and `timeline_clock` when only the playhead or the editor's state did. The playhead line is its own overlay, so moving it redraws nothing else.
- A drag on a clip or on keys is replayed from where it began on every pointer move, inside one edit, and committed on release: one undo step, and none when the drag ends where it started. A clip cannot be dragged over its neighbours.
- Pressing a clip or a key makes its layer current. Pressing a key of another kind switches the key type; the root switches it to chapter marks. Keys of several layers can be selected together; the clip selection is dropped when another layer becomes current.
- A drag that starts on empty track space selects the keys it passes over. Clips cover most tracks, so Shift, or the "select a range" switch in the Keys group, makes a drag that starts on a clip do the same.
- While any drag (a clip, keys, an end of the range, scrubbing, a rectangle of keys) holds the pointer within 28 px of the left or right edge of the tracks, or past it, the view moves along time by itself, at 450 px a second on the edge and faster the further in or past (up to 5,000). What is dragged stays at the time under the pointer, because drags are worked out in time from the press, not in pixels. Turning the wheel or using the trackpad during a drag does the same. `TimelineView.auto_scroll(delta)` is the step, called every frame; `edge_speed` is the rule. Besides, the earlier and later keys step the view along time by most of its width.
- Escape during a drag on the tracks or the ruler puts back what the drag did and ends it (`TimelineView.cancel_drag()`): the document (the edit is cancelled, so nothing is left to undo), the keys a rectangle selected, the playhead a scrub moved. What the press selected stays selected. The view takes the key in `_input`, so the desktop shell's own Escape (cancel a stroke, deselect) does not also fire; with no drag under way the key goes on to the shell as before.
- Trackpad: pinching zooms about the fingers. Two fingers move along time, about one pixel of view per point of finger travel on macOS, and up and down step the page of rows (once for every two rows' worth); with Ctrl or Cmd up and down zooms. This follows Godot's events on macOS: `InputEventMagnifyGesture.factor` is 1 plus the system's magnification for each event, and `InputEventPanGesture.delta` is the system's scroll delta times 0.03 with its sign turned, so positive is later and lower whichever way "natural scrolling" is set. A mouse wheel, and a Windows trackpad (which Godot reports as a wheel with a fractional `factor`), pan along time, with Shift step the page of rows and with Ctrl or Cmd zoom. In a headset, where the wheel is the drawing hand's stick, up and down over the tracks zoom about the pointer and left and right (with the tip close) pan.
- The context row shows the Frames group for a paint layer, Sequence for a sequence and Chapters for the root. What does not apply is greyed out, not hidden, so buttons stay where they are.
- On the desktop a grip along the top edge makes the panel taller or shorter, and more rows show. In a headset the panel has one size.

Left rough:

- Pinch and two-finger panning are tested with synthetic events written to match what Godot sends on macOS, but have never been driven by a real trackpad. The speed of a two-finger pan (`PAN_PIXELS`, scaled by the screen's scale) is the number to adjust by feel; on Linux under Wayland the units of a pan are not known here.
- The auto-scroll speeds are guesses, not tuned by hand.
- In the development build, scrubbing a document with some 300 layers and 9,000 keys took 32 ms a frame before the clock query existed. It has not been measured again since.

## Trying things out

```bash
cargo build -p limner-godot
scripts/godot.sh --headless --path godot --import                 # reports script errors
scripts/shot.sh local/out/shot.png --open <project> --frame-all   # windowed run, saves a screenshot
scripts/shot.sh local/out/shot.png --exec '{"cmd":"add_layer","kind":"light"}'
```

`--exec` runs a command at start and may be repeated. See the top of `godot/main.gd` for all options.
