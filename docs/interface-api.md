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

`doc_revision`, `state_revision`, `dirty`, `can_undo`, `can_redo`, `undo_label`, `redo_label`, `time` (flicks), `seconds`, `frame`, `playing`, `waiting` (held at a Stop), `tool`, `current_layer`, `paint_layer`, `transform_target` (ids or `null`), `selection_count` (selected strokes).

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
| `set_document` | optional `title`, `description`, `background`, `rate` (`{num, den}`), `range_start`, `range_end`, `camera_resolution` (`[w, h]`) |
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
| `select_layers` | `layers`, optional `mode` (`replace`, `add`, `remove`, `toggle`) |
| `set_transform_target` | `layer` (id, or `null` for the view) |
| `set_options` | `values` |
| `set_tool` | `tool` |
| `set_tool_settings` | `values`, nested by tool: `{"paint": {"size": 0.02}}` |
| `set_color` | `color` |

### Playback (not undoable)
`play`, `pause`, `toggle_play`, `seek` (`time`), `step_frames` (`frames`), `rewind`, `fast_forward`.

### Strokes and selection
| Command | Fields |
|---|---|
| `select_all`, `deselect` | |
| `delete_selection` | |
| `selection_to_new_layer` | `copy` (bool) |
| `optimize` | `scope` (`"layer"` or `"all"`) |
| `transform_again` | |

## Trying things out

```bash
cargo build -p limner-godot
scripts/godot.sh --headless --path godot --import                 # reports script errors
scripts/shot.sh local/out/shot.png --open <project> --frame-all   # windowed run, saves a screenshot
scripts/shot.sh local/out/shot.png --exec '{"cmd":"add_layer","kind":"light"}'
```

`--exec` runs a command at start and may be repeated. See the top of `godot/main.gd` for all options.
