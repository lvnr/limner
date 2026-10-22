class_name ToolsPanel
extends LimnerPanel
## The tools, as Quill's Tools panel has them: a row of keys to choose one, the name
## of the one in use, its settings, a row of guides, and its size.
##
## The panel never changes size: each tool's settings are swapped in place inside an
## area of `CONTEXT_HEIGHT`, and the size slider under the guides is the one of the
## tool in use. Tool settings and options are not part of the document, so changing
## them is not undoable and a drag on one of these sliders needs no undo step.

## Asks the shell to show the Text panel (see `TextPanel`).
signal text_requested

## The tools in the order of the row, with the title each is shown under. A tool's
## icon has the same name as the tool.
const TOOLS := {
	"paint": "Paint",
	"line": "Line",
	"colorize": "Colorize",
	"eraser": "Eraser",
	"select": "Selection",
	"grab": "Grab",
	"thicken": "Thicken / Thin",
	"nudge": "Nudge",
	"optimize": "Optimize",
}
## The brushes in the order of the panel, with what each one's tooltip says. The
## icon of a brush is its name after `shape_`.
const BRUSHES := {
	"capped_cylinder": "Capped cylinder: a round tube with rounded ends",
	"capped_ellipse": "Capped ellipse: a flattened tube with rounded ends",
	"rounded_ribbon": "Rounded ribbon: a flat strip with rounded ends",
	"cylinder": "Cylinder: a round tube with flat ends",
	"ellipse": "Ellipse: a flattened tube with flat ends",
	"ribbon": "Ribbon: a flat strip with open ends",
	"cube": "Cube: a square bar with flat ends",
}
## What can make a stroke's width or opacity vary along its length: the word written
## under the key and what it means.
const TAPERS := {
	"none": ["Off", "The same along the whole stroke"],
	"pressure": ["Press", "Follows pen or trigger pressure"],
	"auto": ["Auto", "Tapers both ends automatically"],
	"both": ["Both", "Follows pressure and tapers both ends"],
}
## The blend modes of Colorize in the order of the grid.
const BLEND_MODES := {
	"normal": "Normal (RGB)",
	"darken": "Darken",
	"multiply": "Multiply",
	"overlay": "Overlay",
	"burn": "Burn",
	"lighten": "Lighten",
	"screen": "Screen",
	"add": "Add",
	"dodge": "Dodge",
	"hue": "Hue (H)",
	"saturation": "Saturation (S)",
	"color": "Color (HS)",
	"luminosity": "Luminosity (V)",
	"opacity": "Opacity (A)",
}
## The mirror's settings: off, or across one of the paint layer's axes.
const MIRROR := [["off", "Mirror"], ["x", "Mirror X"], ["y", "Mirror Y"], ["z", "Mirror Z"]]
const MIN_SIZE := 0.0005
const MAX_SIZE := 0.5
## Width of the title in front of a control, the same as a slider's.
const TITLE_WIDTH := 84
## Space between rows of the panels that are not yet Quill-style.
const SEPARATION := 4
## The panel's size, and the height of the area that each tool's settings fill, in
## design pixels.
const DESIGN_SIZE := Vector2(470, 600)
const CONTEXT_HEIGHT := 316
const SIZE_NOTE := "The radius of the tool. Size is measured in the viewer's space, so zooming in makes a tool cover less of the artwork."
const STRENGTH_NOTE := "How strongly the tool acts on what it touches"
const HARD_CORE_NOTE := "The part of the sphere, from its centre, that acts at full strength. The effect fades from there to the edge."
const DIRECTIONAL_NOTE := "Directional: strokes fade when seen from another angle than they were drawn from"
const MIRROR_NOTE := "Mirror: what the tools do on one side of the mirror they also do on the other. The mirror stands at the pivot of the layer being painted on: move the pivot to move it."

var editor: LimnerEditor

var _strip: KeyGrid
var _title: Label
## The settings of each tool, by tool: only those of the current tool are shown.
var _contexts := {}
## The size slider of each tool, by tool: only that of the current tool is shown.
var _sizes := {}
## The control showing each tool setting, by "tool/key".
var _settings := {}
## The toggle key showing each editor option, by key.
var _options := {}
var _mirror: StepKey
var _selection_status: Label
## The keys that need a selection to act on.
var _selection_buttons: Array[Button] = []
var _selection_count := -1
var _opacity_ramps: Array[OpacityRamp] = []


func _init(limner_editor: LimnerEditor) -> void:
	super(DESIGN_SIZE)
	editor = limner_editor
	name = "Tools"

	_strip = KeyGrid.new(TOOLS.size())
	for tool: String in TOOLS:
		_strip.add_icon(tool, tool, TOOLS[tool])
	_strip.chosen.connect(func(tool: String) -> void: editor.run({"cmd": "set_tool", "tool": tool}))
	add_child(_strip)

	_title = Kit.title("")
	add_child(_title)
	# Stacked: the area takes the size of the one showing, so that one too big for it
	# makes the panel too big, which the tests catch.
	var contexts := _stack()
	contexts.name = "Context"
	contexts.custom_minimum_size = Vector2(0, Kit.u(CONTEXT_HEIGHT))
	add_child(contexts)
	for tool: String in TOOLS:
		var context := Kit.key_column(_controls_for(tool))
		context.name = tool.capitalize()
		_contexts[tool] = context
		contexts.add_child(context)

	add_child(Kit.caption("Guides"))
	_mirror = StepKey.new("flip", MIRROR_NOTE, MIRROR)
	_mirror.chosen.connect(func(key: String) -> void: editor.run({"cmd": "set_tool_settings", "values": {"symmetry": key}}))
	add_child(Kit.key_row([
		_labelled(_option("show_grid", "grid", "Floor grid", "Show a grid on the floor of the world"), "Floor"),
		_labelled(_option("show_brush_grid", "brush_grid", "Brush grid", "Show a small grid in the air around the brush, lined up with the world"), "Brush grid"),
		_labelled(_option("show_stroke_at_hand", "blink", "Blink the stroke to delete", "When the hand comes to a stroke, blink the one its lower button would delete"), "Blink"),
		_mirror,
		_labelled(Kit.key("text", "Text: write words in strokes", func() -> void: text_requested.emit()), "Text"),
	]))

	var sizes := _stack()
	sizes.name = "Size"
	sizes.custom_minimum_size = Vector2(0, Kit.ROW)
	add_child(sizes)
	for tool: String in TOOLS:
		var size := _slider(tool, "size", SliderRow.new("Size", MIN_SIZE, MAX_SIZE, 0.0001, SIZE_NOTE, _millimetres, true))
		_sizes[tool] = size
		sizes.add_child(size)


func _ready() -> void:
	editor.state_changed.connect(refresh)
	editor.document_replaced.connect(refresh)
	refresh()


func _process(_delta: float) -> void:
	var count: int = editor.status.get("selection_count", 0)
	if count != _selection_count:
		_show_selection(count)


## Brings the keys, the settings and the guides in line with the editor.
func refresh() -> void:
	var tools := editor.get_tools()
	var current: String = tools.tool
	_strip.set_value(current)
	_title.text = TOOLS.get(current, "")
	for tool: String in _contexts:
		(_contexts[tool] as Control).visible = tool == current
		(_sizes[tool] as Control).visible = tool == current

	for path: String in _settings:
		var of_tool: Dictionary = tools[path.get_slice("/", 0)]
		var value: Variant = of_tool[path.get_slice("/", 1)]
		var control: Control = _settings[path]
		if control is SliderRow:
			(control as SliderRow).set_value(value)
		elif control is OpacityRamp:
			(control as OpacityRamp).set_value(value)
		elif control is KeyGrid:
			(control as KeyGrid).set_value(value)
		elif control is StepKey:
			(control as StepKey).set_value(value)
		else:
			Kit.set_quiet(control, value)
	var paint := LimnerEditor.screen_color(tools.color)
	for ramp: OpacityRamp in _opacity_ramps:
		ramp.color = paint

	var options := editor.get_options()
	for key: String in _options:
		Kit.set_quiet(_options[key], options[key])
	_mirror.set_value(tools.symmetry)


func _show_selection(count: int) -> void:
	_selection_count = count
	match count:
		0:
			_selection_status.text = "No selection"
		1:
			_selection_status.text = "1 stroke selected"
		_:
			_selection_status.text = "%d strokes selected" % count
	for button: Button in _selection_buttons:
		button.disabled = count == 0


# ---- what each tool shows ----

func _controls_for(tool: String) -> Array:
	match tool:
		"paint":
			return [
				Kit.caption("Opacity"),
				_opacity(tool, "How opaque a new stroke is"),
				Kit.caption("Brushes"),
				_brushes(tool),
				Kit.caption("Options"),
				Kit.key_row([
					_taper(tool, "opacity_taper", "opacity", "Opacity taper: what makes a stroke's opacity vary along its length"),
					_taper(tool, "width_taper", "taper", "Width taper: what makes a stroke's width vary along its length"),
					_labelled(_directional(tool), "Direction"),
				]),
				Kit.caption("Animated brush duration"),
				_slider(tool, "anim_duration", SliderRow.new("Visible for", 0.0, 1.0, 0.01, "How long each piece of a stroke stays visible when painting during playback", _seconds)),
			]
		"line":
			return [
				Kit.caption("Opacity"),
				_opacity(tool, "How opaque a new line is"),
				Kit.caption("Brushes"),
				_brushes(tool),
				Kit.caption("Options"),
				Kit.key_row([_labelled(_directional(tool), "Direction")]),
			]
		"colorize":
			return [
				_fraction(tool, "strength", "Strength", STRENGTH_NOTE),
				_fraction(tool, "inner", "Hard core", HARD_CORE_NOTE),
				_blend_modes(tool),
			]
		"eraser", "nudge":
			return [_fraction(tool, "strength", "Strength", STRENGTH_NOTE)]
		"select":
			_selection_status = Kit.label("", true)
			_selection_buttons = [
				_command("Move to new layer", "Move selection to new layer: take the selected strokes out of their layers and put them in a new one", {"cmd": "selection_to_new_layer", "copy": false}, "merge"),
				_command("Copy to new layer", "Copy selection to new layer: put a copy of the selected strokes in a new layer", {"cmd": "selection_to_new_layer", "copy": true}, "duplicate"),
				_command("Delete", "Delete selection: delete the selected strokes", {"cmd": "delete_selection"}, "delete"),
			]
			return [
				_selection_status,
				_pair(
					_command("Select all", "Select all: every stroke the two choices below allow", {"cmd": "select_all"}, "select_all"),
					_command("Deselect", "Deselect all: select nothing", {"cmd": "deselect"}, "deselect")
				),
				_pair(
					_option_text("select_in_all_layers", "All layers", "Select in all visible unlocked layers: in every layer that is shown and not locked, not just the current one"),
					_option_text("select_in_all_frames", "All frames", "Select in all frames: in every frame of an animated layer, not just the one shown")
				),
				_pair(_selection_buttons[0], _selection_buttons[1]),
				_pair(_selection_buttons[2], Control.new()),
			]
		"grab":
			return [
				_fraction(tool, "strength", "Strength", STRENGTH_NOTE),
				_fraction(tool, "inner", "Hard core", HARD_CORE_NOTE),
			]
		"thicken":
			return [
				_fraction(tool, "strength", "Strength", STRENGTH_NOTE),
				_hint("Hold the modifier to thin instead: Shift on the desktop, the other hand's trigger in a headset."),
			]
		"optimize":
			return [
				_fraction(tool, "threshold", "Threshold", "How much detail is removed: points that change a stroke's shape by less than this go"),
				_pair(
					_command("Optimize layer", "Optimize current layer: remove needless points from every stroke of the current layer", {"cmd": "optimize", "scope": "layer"}, "optimize"),
					_command("Optimize all", "Optimize all layers: remove needless points from every stroke of the document", {"cmd": "optimize", "scope": "all"}, "optimize")
				),
				_hint("Optimizing removes detail for good. Locked layers are skipped."),
			]
	return []


## Registers a slider as the control of one tool setting.
func _slider(tool: String, key: String, slider: SliderRow) -> SliderRow:
	slider.changed.connect(func(value: float) -> void: editor.set_tool_setting(tool, key, value))
	_settings["%s/%s" % [tool, key]] = slider
	return slider


## A slider for a setting that runs from 0 to 1, shown as a percentage.
func _fraction(tool: String, key: String, title: String, tooltip: String) -> SliderRow:
	return _slider(tool, key, SliderRow.new(title, 0.0, 1.0, 0.01, tooltip))


func _opacity(tool: String, tooltip: String) -> OpacityRamp:
	var ramp := OpacityRamp.new()
	ramp.tooltip_text = tooltip
	ramp.changed.connect(func(value: float) -> void: editor.set_tool_setting(tool, "opacity", value))
	_settings["%s/opacity" % tool] = ramp
	_opacity_ramps.append(ramp)
	return ramp


func _brushes(tool: String) -> KeyGrid:
	var grid := KeyGrid.new(BRUSHES.size())
	for brush: String in BRUSHES:
		grid.add_icon(brush, "shape_" + brush, BRUSHES[brush])
	grid.chosen.connect(func(value: String) -> void: editor.set_tool_setting(tool, "brush", value))
	_settings["%s/brush" % tool] = grid
	return grid


func _taper(tool: String, key: String, icon: String, tooltip: String) -> StepKey:
	var choices: Array = []
	for taper: String in TAPERS:
		choices.append([taper, TAPERS[taper][0]])
	var step := StepKey.new(icon, tooltip, choices)
	step.chosen.connect(func(value: String) -> void: editor.set_tool_setting(tool, key, value))
	_settings["%s/%s" % [tool, key]] = step
	return step


func _directional(tool: String) -> Button:
	var key := Kit.key("direction", DIRECTIONAL_NOTE, Callable(), true)
	key.toggled.connect(func(on: bool) -> void: editor.set_tool_setting(tool, "directional", on))
	_settings["%s/directional" % tool] = key
	return key


func _blend_modes(tool: String) -> KeyGrid:
	var grid := KeyGrid.new(3)
	for mode: String in BLEND_MODES:
		grid.add_text(mode, BLEND_MODES[mode], "Blend mode: how the paint colour is mixed with the colour a stroke already has")
	grid.chosen.connect(func(value: String) -> void: editor.set_tool_setting(tool, "mode", value))
	_settings["%s/mode" % tool] = grid
	return grid


## A key with an icon that switches an editor option on and off.
func _option(key: String, icon: String, text: String, tooltip: String) -> Button:
	var button := Kit.key(icon, "%s: %s" % [text, tooltip], Callable(), true)
	button.toggled.connect(func(on: bool) -> void: editor.set_option(key, on))
	_options[key] = button
	return button


## A key with words that switches an editor option on and off.
func _option_text(key: String, text: String, tooltip: String) -> Button:
	var button := Kit.text_key(text, tooltip, Callable(), "", true)
	button.toggled.connect(func(on: bool) -> void: editor.set_option(key, on))
	_options[key] = button
	return button


## A key with words that runs one command.
func _command(text: String, tooltip: String, command: Dictionary, icon := "") -> Button:
	return Kit.text_key(text, tooltip, func() -> void: editor.run(command), icon)


## A container whose children lie on top of each other, each filling it.
static func _stack() -> MarginContainer:
	var stack := MarginContainer.new()
	for side: String in ["left", "right", "top", "bottom"]:
		stack.add_theme_constant_override("margin_" + side, 0)
	return stack


## Two controls side by side, sharing the width.
static func _pair(first: Control, second: Control) -> HBoxContainer:
	first.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	second.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	first.size_flags_stretch_ratio = 1.0
	second.size_flags_stretch_ratio = 1.0
	return Kit.key_row([first, second])


## A key with a word written small under it.
static func _labelled(key: Button, word: String) -> VBoxContainer:
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", Kit.u(2))
	key.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	var label := Kit.label(word, true)
	label.add_theme_font_size_override("font_size", Kit.text_size("small"))
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	column.add_child(key)
	column.add_child(label)
	return column


## A line or two of small print under the controls it explains.
func _hint(text: String) -> Label:
	var label := Kit.label(text, true)
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.add_theme_font_size_override("font_size", Kit.text_size("caption"))
	return label


static func _millimetres(metres: float) -> String:
	var millimetres := metres * 1000.0
	if millimetres >= 10.0:
		return "%d mm" % roundi(millimetres)
	return "%s mm" % ("%.1f" % millimetres).trim_suffix(".0")


static func _seconds(seconds: float) -> String:
	return "%s s" % ("%.2f" % seconds).rstrip("0").trim_suffix(".")
