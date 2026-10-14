class_name ToolsPanel
extends VBoxContainer
## The tools: a strip to choose one, the settings of the one in use, and the guides
## shown in the view.
##
## Tool settings and options are not part of the document, so changing them is not
## undoable and a drag on one of these sliders needs no undo step.

## The tools in the order of the strip, with the title each is shown under. A tool's
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
## What can make a stroke's width or opacity vary along its length: the word on the
## button and its tooltip.
const TAPERS := {
	"none": ["None", "The same along the whole stroke"],
	"pressure": ["Press", "Follows pen or trigger pressure"],
	"auto": ["Auto", "Tapers both ends automatically"],
	"both": ["Both", "Follows pressure and tapers both ends"],
}
## The blend modes of Colorize in the order of the list.
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
## How the lines of a text line up, by the key of their button: the word on it, its
## tooltip and the alignment it stands for.
const TEXT_ALIGNMENTS := {
	"left": ["Left", "Line the rows of text up on the left", HORIZONTAL_ALIGNMENT_LEFT],
	"centre": ["Centre", "Centre each row of text", HORIZONTAL_ALIGNMENT_CENTER],
	"right": ["Right", "Line the rows of text up on the right", HORIZONTAL_ALIGNMENT_RIGHT],
}
## What the font list calls the font that comes with Limner.
const BUILT_IN_FONT := "Built-in font"
const FONT_NOTE := "The font the text is written in: the built-in one or one installed on this computer. The ones chosen last are at the top."
const MIN_SIZE := 0.0005
const MAX_SIZE := 0.5
## Width of the title in front of a control, the same as a slider's.
const TITLE_WIDTH := 84
## The titles of the taper rows are longer, and the two rows must line up.
const TAPER_TITLE_WIDTH := 94
## Space between rows. Tighter than the theme's, so that the Paint tool and the
## Color panel under it fit a 900 pixel window without scrolling.
const SEPARATION := 4
const SIZE_NOTE := "The radius of the tool. Size is measured in the viewer's space, so zooming in makes a tool cover less of the artwork."
const STRENGTH_NOTE := "How strongly the tool acts on what it touches"
const HARD_CORE_NOTE := "The part of the sphere, from its centre, that acts at full strength. The effect fades from there to the edge."
const DIRECTIONAL_NOTE := "Strokes fade when seen from another angle than they were drawn from"

var editor: LimnerEditor

var _strip: ChoiceRow
var _title: Label
## The settings of each tool, by tool: only those of the current tool are shown.
var _contexts := {}
## The control showing each tool setting, by "tool/key".
var _settings := {}
## The toggle showing each editor option, by key.
var _options := {}
var _mirror: ChoiceRow
var _text: TextEdit
var _text_align: ChoiceRow
## The font drop-down. Each entry's metadata is its family, empty for the built-in font.
var _text_font: OptionButton
var _font_search: LineEdit
var _text_bold: CheckBox
var _text_italic: CheckBox
var _text_height: SliderRow
var _text_solid: CheckBox
var _fonts_queued := false
var _selection_status: Label
## The buttons that need a selection to act on.
var _selection_buttons: Array[Button] = []
var _selection_count := -1


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Tools"
	add_theme_constant_override("separation", SEPARATION)

	_strip = ChoiceRow.new()
	for tool: String in TOOLS:
		var button := _strip.add_icon(tool, tool, TOOLS[tool])
		button.custom_minimum_size = Vector2(0, Kit.BUTTON)
		Kit.narrow(button)
	_strip.chosen.connect(func(tool: String) -> void: editor.run({"cmd": "set_tool", "tool": tool}))
	add_child(_strip)

	_title = Kit.label("")
	_title.add_theme_font_size_override("font_size", 15)
	add_child(_title)
	for tool: String in TOOLS:
		var context := Kit.column(_controls_for(tool))
		context.add_theme_constant_override("separation", SEPARATION)
		_contexts[tool] = context
		add_child(context)

	add_child(Kit.heading("Guides"))
	add_child(_option("show_grid", "Floor grid", "Show a grid on the floor of the world"))
	add_child(_option("show_brush_grid", "Brush grid", "Show a small grid in the air around the brush, lined up with the world"))
	add_child(_option("show_stroke_at_hand", "Blink the stroke to delete", "When the hand comes to a stroke, blink the one its lower button would delete"))
	_mirror = ChoiceRow.new()
	_mirror.add_text("off", "Off", "Tools work where the hand is and nowhere else")
	for axis: String in ["x", "y", "z"]:
		_mirror.add_text(axis, axis.to_upper(), "Paint and edit on both sides of a mirror square to the layer's %s axis" % axis.to_upper())
	_mirror.chosen.connect(func(key: String) -> void: editor.run({"cmd": "set_tool_settings", "values": {"symmetry": key}}))
	add_child(_titled("Mirror", "What the tools do on one side of the mirror they also do on the other. The mirror stands at the pivot of the layer being painted on: move the pivot to move it.", _mirror))

	add_child(Kit.heading("Text"))
	_build_text_section()


func _ready() -> void:
	editor.state_changed.connect(refresh)
	editor.document_replaced.connect(refresh)
	Preferences.shared().changed.connect(_on_preference_changed)
	refresh()


func _process(_delta: float) -> void:
	var count: int = editor.status.get("selection_count", 0)
	if count != _selection_count:
		_show_selection(count)


## Brings the strip, the settings and the toggles in line with the editor.
func refresh() -> void:
	var tools := editor.get_tools()
	var current: String = tools.tool
	_strip.set_value(current)
	_title.text = TOOLS.get(current, "")
	for tool: String in _contexts:
		var context: Control = _contexts[tool]
		context.visible = tool == current

	for path: String in _settings:
		var of_tool: Dictionary = tools[path.get_slice("/", 0)]
		var value: Variant = of_tool[path.get_slice("/", 1)]
		var control: Control = _settings[path]
		if control is SliderRow:
			(control as SliderRow).set_value(value)
		elif control is ChoiceRow:
			(control as ChoiceRow).set_value(value)
		elif control is OptionButton:
			Kit.set_quiet(control, BLEND_MODES.keys().find(value))
		else:
			Kit.set_quiet(control, value)

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
				_fraction(tool, "opacity", "Opacity", "How opaque a new stroke is"),
				_size(tool),
				Kit.heading("Brushes"),
				_brushes(tool),
				Kit.heading("Options"),
				_taper(tool, "opacity_taper", "Opacity taper", "What makes a stroke's opacity vary along its length"),
				_taper(tool, "width_taper", "Width taper", "What makes a stroke's width vary along its length"),
				_directional(tool),
				Kit.heading("Animated brush duration"),
				_slider(tool, "anim_duration", SliderRow.new("Visible for", 0.0, 1.0, 0.01, "How long each piece of a stroke stays visible when painting during playback", _seconds)),
			]
		"line":
			return [
				_fraction(tool, "opacity", "Opacity", "How opaque a new line is"),
				_size(tool),
				Kit.heading("Brushes"),
				_brushes(tool),
				Kit.heading("Options"),
				_directional(tool),
			]
		"colorize":
			return [
				_fraction(tool, "strength", "Strength", STRENGTH_NOTE),
				_size(tool),
				_fraction(tool, "inner", "Hard core", HARD_CORE_NOTE),
				_blend_mode(tool),
			]
		"eraser", "nudge":
			return [
				_fraction(tool, "strength", "Strength", STRENGTH_NOTE),
				_size(tool),
			]
		"select":
			_selection_status = Kit.label("", true)
			_selection_buttons = [
				_command("Move selection to new layer", "Take the selected strokes out of their layers and put them in a new one", {"cmd": "selection_to_new_layer", "copy": false}),
				_command("Copy selection to new layer", "Put a copy of the selected strokes in a new layer", {"cmd": "selection_to_new_layer", "copy": true}),
				_command("Delete selection", "Delete the selected strokes", {"cmd": "delete_selection"}, "trash"),
			]
			var controls: Array = [
				_selection_status,
				_size(tool),
				Kit.row([
					_command("Select all", "Select every stroke the two choices below allow", {"cmd": "select_all"}),
					_command("Deselect all", "Select nothing", {"cmd": "deselect"}),
				]),
				_option("select_in_all_layers", "Select in all visible unlocked layers", "Select in every layer that is shown and not locked, not just the current one"),
				_option("select_in_all_frames", "Select in all frames", "Select in every frame of an animated layer, not just the one shown"),
			]
			controls.append_array(_selection_buttons)
			return controls
		"grab":
			return [
				_fraction(tool, "strength", "Strength", STRENGTH_NOTE),
				_size(tool),
				_fraction(tool, "inner", "Hard core", HARD_CORE_NOTE),
			]
		"thicken":
			return [
				_fraction(tool, "strength", "Strength", STRENGTH_NOTE),
				_size(tool),
				_hint("Hold the modifier to thin instead: Alt on the desktop, the other hand's trigger in a headset."),
			]
		"optimize":
			return [
				_fraction(tool, "threshold", "Threshold", "How much detail is removed: points that change a stroke's shape by less than this go"),
				_size(tool),
				_command("Optimize current layer", "Remove needless points from every stroke of the current layer", {"cmd": "optimize", "scope": "layer"}),
				_command("Optimize all layers", "Remove needless points from every stroke of the document", {"cmd": "optimize", "scope": "all"}),
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


func _size(tool: String) -> SliderRow:
	return _slider(tool, "size", SliderRow.new("Size", MIN_SIZE, MAX_SIZE, 0.0001, SIZE_NOTE, _millimetres, true))


## Registers a row of buttons as the control of one tool setting.
func _choice(tool: String, key: String, choice: ChoiceRow) -> ChoiceRow:
	choice.chosen.connect(func(value: String) -> void: editor.set_tool_setting(tool, key, value))
	_settings["%s/%s" % [tool, key]] = choice
	return choice


func _brushes(tool: String) -> ChoiceRow:
	var choice := ChoiceRow.new()
	for brush: String in BRUSHES:
		choice.add_icon(brush, "shape_" + brush, BRUSHES[brush])
	return _choice(tool, "brush", choice)


func _taper(tool: String, key: String, title: String, tooltip: String) -> HBoxContainer:
	var choice := ChoiceRow.new()
	for taper: String in TAPERS:
		var words: Array = TAPERS[taper]
		var button := choice.add_text(taper, words[0], words[1])
		# Four words beside a title have to fit a panel 280 pixels wide.
		button.add_theme_font_size_override("font_size", 13)
		Kit.narrow(button)
	return _titled(title, tooltip, _choice(tool, key, choice), TAPER_TITLE_WIDTH)


func _directional(tool: String) -> CheckBox:
	var toggle := Kit.toggle("Directional", DIRECTIONAL_NOTE, func(on: bool) -> void: editor.set_tool_setting(tool, "directional", on))
	_settings["%s/directional" % tool] = toggle
	return toggle


func _blend_mode(tool: String) -> HBoxContainer:
	var tooltip := "How the paint colour is mixed with the colour a stroke already has"
	var option := Kit.option(BLEND_MODES.values(), tooltip, func(index: int) -> void: editor.set_tool_setting(tool, "mode", BLEND_MODES.keys()[index]))
	_settings["%s/mode" % tool] = option
	return _titled("Blend mode", tooltip, option)


## A toggle for an editor option.
func _option(key: String, text: String, tooltip: String) -> CheckBox:
	var toggle := Kit.toggle(text, tooltip, func(on: bool) -> void: editor.set_option(key, on))
	toggle.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_options[key] = toggle
	return toggle


# ---- text ----

func _build_text_section() -> void:
	_text = TextEdit.new()
	_text.placeholder_text = "Words to write in strokes\nEnter starts a new line"
	# Three lines show; more scroll.
	_text.custom_minimum_size = Vector2(0, Kit.ROW * 3)
	_text.wrap_mode = TextEdit.LINE_WRAPPING_NONE
	_text.tooltip_text = "Text is written in strokes of the paint colour on the paint layer, in front of you, and selected so that it can be moved into place. Enter starts a new line; Ctrl+Enter (Cmd+Enter on a Mac) adds the text."
	_text.gui_input.connect(_on_text_input)
	add_child(_text)

	_text_align = ChoiceRow.new()
	for key: String in TEXT_ALIGNMENTS:
		var words: Array = TEXT_ALIGNMENTS[key]
		_text_align.add_text(key, words[0], words[1])
	_text_align.set_value("centre")
	add_child(_titled("Align", "How lines of different lengths line up under each other", _text_align))

	_text_font = Kit.fitted_option([], FONT_NOTE, _on_font_selected)
	add_child(_titled("Font", FONT_NOTE, _text_font))
	_font_search = LineEdit.new()
	_font_search.placeholder_text = "Find a font"
	_font_search.tooltip_text = "Show only the fonts whose name holds these letters. Enter takes the first of them."
	_font_search.custom_minimum_size = Vector2(0, Kit.ROW)
	_font_search.clear_button_enabled = true
	_font_search.text_changed.connect(func(_typed: String) -> void: _fill_fonts())
	_font_search.text_submitted.connect(func(_typed: String) -> void: _take_first_font())
	add_child(_titled("", _font_search.tooltip_text, _font_search))
	_text_bold = Kit.toggle("Bold", "Heavier letters: the font's own bold, or the letters thickened when it has none", Callable())
	_text_italic = Kit.toggle("Italic", "Slanted letters: the font's own italic, or the letters leant when it has none", Callable())
	_text_bold.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_text_italic.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	add_child(Kit.row([_text_bold, _text_italic]))
	_fill_fonts()

	_text_height = SliderRow.new("Height", 0.01, 1.0, 0.001, "How tall the letters are", _millimetres, true)
	_text_height.set_value(0.1)
	add_child(_text_height)
	_text_solid = Kit.toggle("Filled letters", "Fill the letters with paint; otherwise only their outlines are drawn", Callable())
	_text_solid.button_pressed = true
	add_child(_text_solid)
	var add_text := Kit.button("Add text", "Write the text in strokes", _add_text, "plus")
	add_text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	add_child(add_text)


## Fills the font list: the built-in font, then the one in use and the others chosen
## last, then every font installed, or only those whose name holds what is in the
## search box. Hundreds of fonts are common, hence the recent ones and the search.
func _fill_fonts() -> void:
	var preferences := Preferences.shared()
	var current := preferences.text("text_font")
	var search := _font_search.text.strip_edges()
	_text_font.clear()
	_add_font(BUILT_IN_FONT, "")
	var listed := {"": true}
	var recent: Array = [current]
	# While searching, only the font in use stays on top, so the matches come first.
	if search.is_empty():
		recent.append_array(preferences.value("recent_fonts"))
	for family: String in recent:
		if listed.has(family):
			continue
		listed[family] = true
		if TextStrokes.is_installed(family):
			_add_font(family, family)
		else:
			_add_font("%s (not installed)" % family, family)
	var matches := PackedStringArray()
	for family in TextStrokes.system_fonts():
		if search.is_empty() or family.containsn(search):
			matches.append(family)
	if search.is_empty():
		_text_font.add_separator("All fonts")
	elif matches.is_empty():
		_text_font.add_separator("No font matches")
	else:
		_text_font.add_separator("Found %d" % matches.size())
	for family in matches:
		_add_font(family, family)
	_text_font.select(_font_index(current))


func _add_font(text: String, family: String) -> void:
	_text_font.add_item(text)
	_text_font.set_item_metadata(_text_font.item_count - 1, family)


## The entry of the list that stands for `family`: the first one, which is at the top.
func _font_index(family: String) -> int:
	for index in _text_font.item_count:
		if not _text_font.is_item_separator(index) and str(_text_font.get_item_metadata(index)) == family:
			return index
	return 0


func _on_font_selected(index: int) -> void:
	Preferences.shared().choose_font(str(_text_font.get_item_metadata(index)))


## Chooses the first font the search found.
func _take_first_font() -> void:
	var after_separator := false
	for index in _text_font.item_count:
		if _text_font.is_item_separator(index):
			after_separator = true
		elif after_separator:
			Preferences.shared().choose_font(str(_text_font.get_item_metadata(index)))
			return


func _on_preference_changed(key: String) -> void:
	# Refilled after the drop-down has finished reporting the choice that changed it.
	if (key == "text_font" or key == "recent_fonts") and not _fonts_queued:
		_fonts_queued = true
		_refill_fonts.call_deferred()


func _refill_fonts() -> void:
	_fonts_queued = false
	_fill_fonts()


## Ctrl+Enter (Cmd+Enter on a Mac) in the text box adds the text; Enter alone starts a
## new line, as it does from the headset's keyboard.
func _on_text_input(event: InputEvent) -> void:
	var key := event as InputEventKey
	if key != null and key.pressed and (key.keycode == KEY_ENTER or key.keycode == KEY_KP_ENTER) and key.is_command_or_control_pressed():
		_text.accept_event()
		_add_text()


## The font chosen for text, in bold or italic as the switches say. When it is not
## installed here, the built-in font stands in and `fell_back` is set.
func _text_font_now(fell_back: Array[bool]) -> Font:
	var family := Preferences.shared().text("text_font")
	var font := TextStrokes.font_named(family, _text_bold.button_pressed, _text_italic.button_pressed)
	if font == null:
		fell_back[0] = true
		font = TextStrokes.font_named("", _text_bold.button_pressed, _text_italic.button_pressed)
	return font


func _add_text() -> void:
	var fell_back: Array[bool] = [false]
	var font := _text_font_now(fell_back)
	var align: HorizontalAlignment = TEXT_ALIGNMENTS.get(_text_align.get_value(), TEXT_ALIGNMENTS["centre"])[2]
	var count := editor.add_text(_text.text, _text_height.get_value(), _text_solid.button_pressed, font, align)
	if count == 0:
		return
	if fell_back[0]:
		editor.message.emit("\"%s\" is not installed here, so the text is in the built-in font: %d strokes, selected" % [Preferences.shared().text("text_font"), count], false)
	else:
		editor.message.emit("Added the text as %d strokes; it is selected, ready to move" % count, false)


## A button that runs one command.
func _command(text: String, tooltip: String, command: Dictionary, icon := "") -> Button:
	var button := Kit.button(text, tooltip, func() -> void: editor.run(command), icon)
	button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return button


## A control with its title in front of it, lined up with the sliders.
func _titled(title: String, tooltip: String, control: Control, title_width := TITLE_WIDTH) -> HBoxContainer:
	var label := Kit.label(title)
	label.custom_minimum_size = Vector2(title_width, Kit.ROW)
	label.tooltip_text = tooltip
	label.mouse_filter = Control.MOUSE_FILTER_PASS
	control.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return Kit.row([label, control])


## A line or two of small print under the controls it explains.
func _hint(text: String) -> Label:
	var label := Kit.label(text, true)
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.add_theme_font_size_override("font_size", 12)
	return label


static func _millimetres(metres: float) -> String:
	var millimetres := metres * 1000.0
	if millimetres >= 10.0:
		return "%d mm" % roundi(millimetres)
	return "%s mm" % ("%.1f" % millimetres).trim_suffix(".0")


static func _seconds(seconds: float) -> String:
	return "%s s" % ("%.2f" % seconds).rstrip("0").trim_suffix(".")
