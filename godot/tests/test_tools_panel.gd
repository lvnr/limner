extends LimnerTest
## The Tools panel shows the tool in use with its settings, changes them, and keeps
## in step when they are changed elsewhere.

## The settings each tool must offer.
const EXPECTED := {
	"paint": ["opacity", "size", "brush", "opacity_taper", "width_taper", "directional", "anim_duration"],
	"line": ["opacity", "size", "brush", "directional"],
	"colorize": ["strength", "size", "inner", "mode"],
	"eraser": ["strength", "size"],
	"select": ["size"],
	"grab": ["strength", "size", "inner"],
	"thicken": ["strength", "size"],
	"nudge": ["strength", "size"],
	"optimize": ["threshold", "size"],
}


## An editor that remembers the commands it was asked to run and can pretend that
## strokes are selected, for the commands the panel sends but a test cannot rely on.
class Recorder extends LimnerEditor:
	var sent: Array[Dictionary] = []
	var pretend_selected := -1

	func run(command: Dictionary) -> Variant:
		sent.append(command)
		return super(command)

	func _process(delta: float) -> void:
		super(delta)
		if pretend_selected >= 0:
			status["selection_count"] = pretend_selected


var recorder: Recorder


func _panel() -> ToolsPanel:
	recorder = Recorder.new()
	add_child(recorder)
	recorder.setup(world.scene)
	var panel := ToolsPanel.new(recorder)
	mount(panel)
	await frames(2)
	return panel


## A tool setting as the session has it now, without waiting for a change signal.
func _setting(tool: String, key: String) -> Variant:
	var of_tool: Dictionary = world.scene.tool_settings()[tool]
	return of_tool[key]


func _current_tool() -> String:
	return world.scene.tool_settings()["tool"]


func _button(panel: ToolsPanel, text: String) -> Button:
	for node: Node in panel.find_children("*", "Button", true, false):
		var button: Button = node
		if button.text == text:
			return button
	failures.append("there is no button \"%s\"" % text)
	var stand_in := Button.new()
	add_child(stand_in)
	return stand_in


func test_starts_on_the_current_tool() -> void:
	var panel := await _panel()
	check_eq(panel._strip.buttons.keys(), ["paint", "line", "colorize", "eraser", "select", "grab", "thicken", "nudge", "optimize"], "order of the strip")
	check_eq(panel._strip.get_value(), "paint", "tool shown pressed")
	check_eq(panel._title.text, "Paint", "title")
	for tool: String in ToolsPanel.TOOLS:
		check(Icons.has(tool), "there is an icon for %s" % tool)
		var expected: Array = EXPECTED[tool]
		for key: String in expected:
			check(panel._settings.has("%s/%s" % [tool, key]), "%s has a control for %s" % [tool, key])
	var count := 0
	for keys: Array in EXPECTED.values():
		count += keys.size()
	check_eq(panel._settings.size(), count, "no settings beyond the expected ones")


func test_each_tool_button_switches_tool_and_context() -> void:
	var panel := await _panel()
	for tool: String in ToolsPanel.TOOLS:
		var button: Button = panel._strip.buttons[tool]
		check_eq(button.tooltip_text, ToolsPanel.TOOLS[tool], "tooltip of %s" % tool)
		button.pressed.emit()
		check_eq(_current_tool(), tool, "tool after pressing %s" % tool)
		await frames(2)
		check_eq(panel._strip.get_value(), tool, "button shown pressed for %s" % tool)
		check_eq(panel._title.text, ToolsPanel.TOOLS[tool], "title for %s" % tool)
		for other: String in panel._contexts:
			var context: Control = panel._contexts[other]
			check_eq(context.visible, other == tool, "settings of %s shown while using %s" % [other, tool])
		check(panel.get_combined_minimum_size().x <= 264.0, "%s fits a panel 280 wide, needs %s" % [tool, panel.get_combined_minimum_size().x])
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_sliders_change_their_settings() -> void:
	var panel := await _panel()
	var size: SliderRow = panel._settings["paint/size"]
	size.slider.value = 0.05
	check_near(_setting("paint", "size"), 0.05, "paint size after moving the slider")
	check_eq(size._value.text, "50 mm", "size is shown in millimetres")
	size.slider.value = 0.0025
	check_eq(size._value.text, "2.5 mm", "a small size keeps a decimal")
	var opacity: SliderRow = panel._settings["paint/opacity"]
	opacity.slider.value = 0.4
	check_near(_setting("paint", "opacity"), 0.4, "paint opacity")
	var duration: SliderRow = panel._settings["paint/anim_duration"]
	duration.slider.value = 0.25
	check_near(_setting("paint", "anim_duration"), 0.25, "animated brush duration")
	check_eq(duration._value.text, "0.25 s", "duration is shown in seconds")
	duration.slider.value = 1.0
	check_eq(duration._value.text, "1 s", "a whole number of seconds")
	var core: SliderRow = panel._settings["colorize/inner"]
	core.slider.value = 0.6
	check_near(_setting("colorize", "inner"), 0.6, "hard core of colorize")
	var threshold: SliderRow = panel._settings["optimize/threshold"]
	threshold.slider.value = 0.1
	check_near(_setting("optimize", "threshold"), 0.1, "optimize threshold")
	for tool: String in ToolsPanel.TOOLS:
		var slider: HSlider = (panel._settings[tool + "/size"] as SliderRow).slider
		check(slider.exp_edit, "size of %s is exponential" % tool)
		check_near(slider.min_value, 0.0005, "smallest size of %s" % tool)
		check_near(slider.max_value, 0.5, "largest size of %s" % tool)
		check(slider.tooltip_text.contains("viewer's space"), "the size tooltip of %s says where size is measured" % tool)
	check_eq(editor.status.can_undo, false, "tool settings are not undo steps")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_brush_and_taper_controls() -> void:
	var panel := await _panel()
	var brushes: ChoiceRow = panel._settings["paint/brush"]
	check_eq(brushes.buttons.keys(), ["capped_cylinder", "capped_ellipse", "rounded_ribbon", "cylinder", "ellipse", "ribbon", "cube"], "order of the brushes")
	check_eq(brushes.get_value(), "ellipse", "the brush in use is shown pressed")
	for brush: String in brushes.buttons:
		check(Icons.has("shape_" + brush), "there is an icon for the %s brush" % brush)
		(brushes.buttons[brush] as Button).pressed.emit()
		check_eq(_setting("paint", "brush"), brush, "paint brush")
		check_eq(brushes.get_value(), brush, "only the chosen brush is down")
	(panel._settings["line/brush"] as ChoiceRow).buttons["ribbon"].pressed.emit()
	check_eq(_setting("line", "brush"), "ribbon", "line brush")
	check_eq(_setting("paint", "brush"), "cube", "the paint brush is its own")

	for key: String in ["opacity_taper", "width_taper"]:
		var taper: ChoiceRow = panel._settings["paint/" + key]
		check_eq(taper.buttons.keys(), ["none", "pressure", "auto", "both"], "states of %s" % key)
		for state: String in taper.buttons:
			(taper.buttons[state] as Button).pressed.emit()
			check_eq(_setting("paint", key), state, key)
	check_eq((panel._settings["paint/width_taper"] as ChoiceRow).buttons["pressure"].text, "Press", "pressure is called Press")
	check(not panel._settings.has("line/width_taper"), "the line tool has no tapers")

	var directional: CheckBox = panel._settings["paint/directional"]
	directional.button_pressed = true
	check_eq(_setting("paint", "directional"), true, "paint directional")
	check_eq(_setting("line", "directional"), false, "line directional is its own")
	(panel._settings["line/directional"] as CheckBox).button_pressed = true
	check_eq(_setting("line", "directional"), true, "line directional")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_blend_mode() -> void:
	var panel := await _panel()
	var option: OptionButton = panel._settings["colorize/mode"]
	var labels := []
	for index in option.item_count:
		labels.append(option.get_item_text(index))
	check_eq(labels, ["Normal (RGB)", "Darken", "Multiply", "Overlay", "Burn", "Lighten", "Screen", "Add", "Dodge", "Hue (H)", "Saturation (S)", "Color (HS)", "Luminosity (V)", "Opacity (A)"], "blend modes")
	var modes := ["normal", "darken", "multiply", "overlay", "burn", "lighten", "screen", "add", "dodge", "hue", "saturation", "color", "luminosity", "opacity"]
	for index in range(modes.size() - 1, -1, -1):
		option.select(index)
		option.item_selected.emit(index)
		check_eq(_setting("colorize", "mode"), modes[index], "mode chosen from row %d" % index)
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_option_toggles() -> void:
	var panel := await _panel()
	var grid: CheckBox = panel._options["show_grid"]
	check_eq(grid.text, "Floor grid", "the guides toggle")
	check_eq(grid.button_pressed, true, "the grid starts shown")
	grid.button_pressed = false
	check_eq(world.scene.options().show_grid, false, "grid switched off")
	for key: String in ["select_in_all_layers", "select_in_all_frames"]:
		var toggle: CheckBox = panel._options[key]
		check_eq(toggle.button_pressed, false, "%s starts off" % key)
		toggle.button_pressed = true
		var options: Dictionary = world.scene.options()
		check_eq(options[key], true, key)
	run({"cmd": "set_options", "values": {"show_grid": true, "select_in_all_frames": false}})
	await frames(2)
	check_eq(grid.button_pressed, true, "the grid toggle follows the option")
	check_eq((panel._options["select_in_all_frames"] as CheckBox).button_pressed, false, "a selection toggle follows its option")
	check_eq((panel._options["select_in_all_layers"] as CheckBox).button_pressed, true, "the other toggle stays")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_selection_and_optimize_buttons_send_the_documented_commands() -> void:
	var panel := await _panel()
	var commands := {
		"Select all": {"cmd": "select_all"},
		"Deselect all": {"cmd": "deselect"},
		"Move selection to new layer": {"cmd": "selection_to_new_layer", "copy": false},
		"Copy selection to new layer": {"cmd": "selection_to_new_layer", "copy": true},
		"Delete selection": {"cmd": "delete_selection"},
		"Optimize current layer": {"cmd": "optimize", "scope": "layer"},
		"Optimize all layers": {"cmd": "optimize", "scope": "all"},
	}
	for text: String in commands:
		_button(panel, text).pressed.emit()
		check_eq(recorder.sent.back(), commands[text], "command sent by \"%s\"" % text)


func test_selection_status_follows_the_selection() -> void:
	var panel := await _panel()
	var needing_selection := ["Move selection to new layer", "Copy selection to new layer", "Delete selection"]
	recorder.pretend_selected = 0
	await frames(2)
	check_eq(panel._selection_status.text, "No selection", "status with nothing selected")
	for text: String in needing_selection:
		check(_button(panel, text).disabled, "\"%s\" is disabled with nothing selected" % text)
	check(not _button(panel, "Select all").disabled, "\"Select all\" is always available")

	recorder.pretend_selected = 6
	await frames(2)
	check_eq(panel._selection_status.text, "6 strokes selected", "status with six strokes selected")
	for text: String in needing_selection:
		check(not _button(panel, text).disabled, "\"%s\" is enabled with a selection" % text)

	recorder.pretend_selected = 1
	await frames(2)
	check_eq(panel._selection_status.text, "1 stroke selected", "status with one stroke selected")
	recorder.pretend_selected = 0
	await frames(2)
	check_eq(panel._selection_status.text, "No selection", "status after deselecting")
	check(_button(panel, "Delete selection").disabled, "disabled again")


func test_follows_changes_made_behind_its_back() -> void:
	var panel := await _panel()
	run({"cmd": "set_tool", "tool": "colorize"})
	run({"cmd": "set_tool_settings", "values": {
		"colorize": {"size": 0.1, "inner": 0.5, "strength": 0.3, "mode": "hue"},
		"paint": {"brush": "cube", "width_taper": "both", "directional": true, "anim_duration": 0.5},
	}})
	await frames(2)
	check_eq(panel._strip.get_value(), "colorize", "tool shown pressed")
	check_eq(panel._title.text, "Colorize", "title")
	check((panel._contexts["colorize"] as Control).visible, "colorize settings are shown")
	check(not (panel._contexts["paint"] as Control).visible, "paint settings are hidden")
	var size: SliderRow = panel._settings["colorize/size"]
	check_near(size.get_value(), 0.1, "size slider")
	check_eq(size._value.text, "100 mm", "size text")
	check_near((panel._settings["colorize/inner"] as SliderRow).get_value(), 0.5, "hard core slider")
	check_near((panel._settings["colorize/strength"] as SliderRow).get_value(), 0.3, "strength slider")
	check_eq((panel._settings["colorize/mode"] as OptionButton).selected, 9, "blend mode row")
	check_eq((panel._settings["paint/brush"] as ChoiceRow).get_value(), "cube", "brush")
	check_eq((panel._settings["paint/width_taper"] as ChoiceRow).get_value(), "both", "width taper")
	check_eq((panel._settings["paint/directional"] as CheckBox).button_pressed, true, "directional")
	check_near((panel._settings["paint/anim_duration"] as SliderRow).get_value(), 0.5, "animated brush duration")
	check_eq(recorder.sent.size(), 0, "showing a change sends no commands")
	check(messages.is_empty(), "no errors: %s" % [messages])
