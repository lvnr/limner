extends LimnerTest
## The headset layer, exercised through the simulated rig: the same shell, palette
## and pointer that a real headset drives, fed scripted controller states.

const LEFT := XRRig.LEFT
const RIGHT := XRRig.RIGHT
## A settings file of the tests' own, so that they neither read nor change the user's.
const SETTINGS := "user://test_xr_settings.json"
## A place for the drawing hand that is far from every panel and points at none.
const OPEN_AIR := Vector3(0.45, 1.2, -0.3)

var shell: XRShell
var sim: SimRig


func _exit_tree() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(Preferences.DEFAULT_PATH)


## Starts the headset shell on a simulated rig, with the preferences at their
## defaults except that the button map stays away unless a test asks for it.
## Returns false, having recorded the failure, when the shell did not start; a test
## then stops at once instead of stumbling on.
func _start(button_map_at_start := false) -> bool:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(SETTINGS)
	Preferences.shared().set_value("show_button_map_at_start", button_map_at_start)
	shell = XRShell.new()
	shell.mode = "sim"
	shell.world = world
	shell.editor = editor
	add_child(shell)
	sim = shell.rig as SimRig
	if not shell.started or sim == null:
		failures.append("the headset shell did not start on the simulated rig: %s" % shell.failure)
		return false
	sim.listen_to_devices = false
	await frames(4)
	return true


## Takes the palette out with `title` in front and lets it lay itself out.
func _open(title: String) -> XRPanelSurface:
	shell.palette.set_shown(true)
	shell.palette.show_panel(title)
	await frames(4)
	var surface: XRPanelSurface = shell.palette.surfaces[title]
	return surface


## The point in the room at the middle of a control on a sheet.
func _point(surface: XRPanelSurface, control: Control) -> Vector3:
	return surface.world_point(surface.pixel_of(control))


func _normal(surface: XRPanelSurface) -> Vector3:
	return surface.global_basis.z.normalized()


## Presses a control with the tip of a hand: in front of it, through it, back out.
func _poke(surface: XRPanelSurface, control: Control, side := RIGHT) -> void:
	await sim.play(sim.poke_steps(side, _point(surface, control), _normal(surface)))
	await frames(2)


## The key of the on-screen keyboard with this caption, or with this tooltip.
func _key(caption: String) -> Button:
	for node: Node in shell.palette.keyboard.content.find_children("*", "Button", true, false):
		var key: Button = node
		if key.text == caption or key.tooltip_text == caption:
			return key
	failures.append("the keyboard has no key \"%s\"" % caption)
	return Button.new()


func _tool() -> String:
	return world.scene.tool_settings()["tool"]


func _strokes() -> int:
	return int(row_named("Paint").strokes)


func test_the_palette_shows_and_hides_on_the_menu_feedback() -> void:
	if not await _start():
		return
	check_eq(shell.started, true, "the shell started on the simulated rig")
	check_eq(shell.palette.shown, false, "the palette starts put away")
	check_eq(shell.palette.tabs.is_visible_in_tree(), false, "and its tabs with it")
	# The other hand's upper button is what the core answers with "menu".
	var press := [{"time": 0.0, "left": {"upper": true}}, {"time": 0.05, "left": {"upper": false}}, {"time": 0.1}]
	await sim.play(press)
	check_eq(shell.palette.shown, true, "shown after the other hand's upper button")
	check_eq(shell.palette.tabs.is_visible_in_tree(), true, "the tabs are out")
	var tools: XRPanelSurface = shell.palette.surfaces["Tools"]
	check_eq(tools.is_visible_in_tree(), true, "with the first panel under them")
	check_eq((shell.palette.surfaces["Color"] as XRPanelSurface).visible, false, "and only that one")
	# A panel is on the palette exactly when its script is there and loads, so the
	# palette keeps working while a panel is still being written.
	for entry: Dictionary in XRPalette.PANELS:
		var script: GDScript = load(entry.script) if ResourceLoader.exists(entry.script) else null
		var loads := script != null and script.can_instantiate()
		check_eq(shell.palette.surfaces.has(entry.title), loads, "the %s panel is on the palette exactly when it loads" % entry.title)
	for title: String in ["Tools", "Color", "Layers", "Transform", "Layer", "Document", "Settings"]:
		check(shell.palette.surfaces.has(title), "there is a %s panel" % title)
	# The palette is held beside the left hand, toward the right, and above it.
	var hand := sim.aims[LEFT].global_position
	check(tools.global_position.x > hand.x + XRPalette.SIDE_GAP, "the palette lies to the right of the left hand: %s vs %s" % [tools.global_position, hand])
	await sim.play(press)
	check_eq(shell.palette.shown, false, "hidden after pressing it again")
	check_eq(tools.is_visible_in_tree(), false, "the panel goes with it")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_poking_the_eraser_button_switches_the_tool() -> void:
	if not await _start():
		return
	var surface := await _open("Tools")
	var panel: ToolsPanel = surface.content
	var eraser: Button = panel._strip.buttons["eraser"]
	var target := _point(surface, eraser)
	# Hovering in front of the button does not press it.
	await sim.play([{"time": 0.0, "right": {"tip": target + _normal(surface) * 0.03}}, {"time": 0.7}])
	check_eq(_tool(), "paint", "hovering changes nothing")
	check_eq(shell.over_panel, true, "the hand belongs to the panel while it hovers")
	check_eq(surface.viewport.gui_get_hovered_control(), eraser, "the button under the tip is hovered")
	check_eq(surface._footer.text, "Eraser", "its tooltip is shown under the panel after a moment")
	check_eq(shell._cursor_holder.visible, false, "the tool cursor is hidden over a panel")
	check_eq(sim.pulses.size(), 0, "no buzz yet")
	await _poke(surface, eraser)
	check_eq(_tool(), "eraser", "pushing the tip through the button presses it")
	check_eq(sim.pulses.size(), 1, "one buzz for the press")
	if sim.pulses.size() == 1:
		check_eq(sim.pulses[0].side, RIGHT, "on the drawing hand")
		check_near(sim.pulses[0].strength, XRShell.PRESS_PULSE_STRENGTH, "a small one")
	check_eq(_strokes(), 0, "nothing was painted")
	# A tip that arrives from behind the panel presses nothing.
	var paint: Button = panel._strip.buttons["paint"]
	var behind := _point(surface, paint) - _normal(surface) * 0.05
	await sim.play([
		{"time": 0.0, "right": {"tip": OPEN_AIR}},
		{"time": 0.1, "right": {"tip": behind}},
		{"time": 0.2, "right": {"tip": behind + _normal(surface) * 0.045}},
		{"time": 0.3, "right": {"tip": behind}},
		{"time": 0.4},
	])
	check_eq(_tool(), "eraser", "coming through from the back does not press")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_haptics_preference_silences_the_buzz() -> void:
	if not await _start():
		return
	Preferences.shared().set_value("haptics", false)
	var surface := await _open("Tools")
	var panel: ToolsPanel = surface.content
	await _poke(surface, panel._strip.buttons["line"])
	check_eq(_tool(), "line", "the press still lands")
	check_eq(sim.pulses.size(), 0, "but nothing buzzes")


func test_dragging_the_tip_along_the_size_slider_changes_the_size() -> void:
	if not await _start():
		return
	var surface := await _open("Tools")
	var panel: ToolsPanel = surface.content
	var row: SliderRow = panel._settings["paint/size"]
	var slider := row.slider
	var rect := slider.get_global_rect()
	var normal := _normal(surface)
	var from := surface.world_point(Vector2(rect.position.x + rect.size.x * 0.3, rect.get_center().y))
	var to := surface.world_point(Vector2(rect.position.x + rect.size.x * 0.8, rect.get_center().y))
	var before: float = world.scene.tool_settings()["paint"]["size"]
	await sim.play([
		{"time": 0.0, "right": {"tip": from + normal * 0.03}},
		{"time": 0.1, "right": {"tip": from - normal * 0.01}},
		{"time": 0.5, "right": {"tip": to - normal * 0.01}},
		{"time": 0.6, "right": {"tip": to + normal * 0.03}},
		{"time": 0.7},
	])
	var after: float = world.scene.tool_settings()["paint"]["size"]
	check(after > before * 2.0, "the size grew: %s to %s" % [before, after])
	check_near(slider.get_as_ratio(), 0.8, "the knob is where the tip let go", 0.04)
	check_near(after, slider.value, "and the tool has the slider's size", 1e-5)
	check_eq(_strokes(), 0, "nothing was painted on the way")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_a_pinned_panel_stays_put_and_returns_with_its_button() -> void:
	if not await _start():
		return
	var surface := await _open("Tools")
	var normal := _normal(surface)
	# Take hold of the panel by its title bar and carry it to the right.
	var grip_point := surface.world_point(Vector2(120, 16)) + normal * 0.02
	var carried := Vector3(0.35, 0.05, -0.1)
	await sim.play([
		{"time": 0.0, "right": {"tip": grip_point}},
		{"time": 0.1, "right": {"grip": 1.0}},
		{"time": 0.2, "right": {"tip": grip_point}},
		{"time": 0.5, "right": {"tip": grip_point + carried}},
		{"time": 0.6, "right": {"tip": grip_point + carried, "grip": 0.0}},
		{"time": 0.7, "right": {"tip": OPEN_AIR}},
		{"time": 0.8},
	])
	check_eq(shell.palette.is_pinned("Tools"), true, "the panel came off the palette")
	check_eq(shell.palette.current, "Color", "the palette moved on to the next panel")
	check_eq(surface.is_visible_in_tree(), true, "the pinned panel is still shown")
	var where := surface.global_position
	var held_point := surface.world_point(Vector2(120, 16))
	check_near((held_point - (grip_point + carried)).length(), 0.02, "the spot that was held is still at the hand where it let go", 0.004)
	check_eq(surface._corner.visible, true, "it has a button to send it back")
	# The hand that holds the palette goes elsewhere and the palette is put away.
	await sim.play([
		{"time": 0.0, "left": {"tip": sim.tip_position(LEFT)}},
		{"time": 0.3, "left": {"tip": sim.tip_position(LEFT) + Vector3(-0.3, -0.2, 0.1)}},
		{"time": 0.35},
	])
	shell.palette.set_shown(false)
	await frames(3)
	check(surface.global_position.is_equal_approx(where), "the pinned panel did not move: %s, was %s" % [surface.global_position, where])
	check_eq(surface.is_visible_in_tree(), true, "and shows with the palette put away")
	# A second panel can be pinned while the first still is.
	shell.palette.pin("Layers", Transform3D(Basis.IDENTITY, Vector3(-0.6, 1.3, -0.8)))
	await frames(3)
	check_eq(shell.touchable().size(), 2, "two pinned panels can be touched")
	# Pinned panels can be pressed like any other.
	var panel: ToolsPanel = surface.content
	await _poke(surface, panel._strip.buttons["grab"])
	check_eq(_tool(), "grab", "a button on a pinned panel works")
	await _poke(surface, surface._corner)
	check_eq(shell.palette.is_pinned("Tools"), false, "the corner button sends it back")
	check_eq(shell.palette.is_pinned("Layers"), true, "the other one stays pinned")
	check_eq(shell.palette.current, "Tools", "it is in front on the palette again")
	check_eq(surface._corner.visible, false, "and has no return button there")
	shell.palette.set_shown(true)
	await frames(3)
	check_eq(surface.top_level, false, "it moves with the palette again")
	check_near(surface.position.x, shell.palette.tabs.position.x, "and sits under the tabs")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_a_held_panel_is_resized_with_the_stick() -> void:
	if not await _start():
		return
	var surface := await _open("Color")
	var hold := surface.world_point(Vector2(120, 16)) + _normal(surface) * 0.02
	var brush: float = world.scene.tool_settings()["paint"]["size"]
	await sim.play([
		{"time": 0.0, "right": {"tip": hold}},
		{"time": 0.1, "right": {"grip": 1.0}},
		{"time": 0.2, "right": {"stick": Vector2(0.0, 1.0)}},
		{"time": 0.6, "right": {"stick": Vector2.ZERO}},
		{"time": 0.7, "right": {"grip": 0.0}},
		{"time": 0.8},
	])
	check(surface.magnification > 1.3, "pushing the stick up made it larger: %s" % surface.magnification)
	check_near(surface.global_basis.get_scale().x, surface.magnification, "the sheet is drawn that size", 1e-3)
	check_near(world.scene.tool_settings()["paint"]["size"], brush, "the stick did not resize the brush meanwhile", 1e-6)
	shell.palette.unpin("Color")
	check_near(surface.magnification, 1.0, "back on the palette it is its normal size")


func test_the_trigger_paints_away_from_a_panel_but_not_over_one() -> void:
	if not await _start():
		return
	var surface := await _open("Tools")
	var normal := _normal(surface)
	var over := surface.world_point(Vector2(176, 300)) + normal * 0.03
	await sim.play([
		{"time": 0.0, "right": {"tip": over}},
		{"time": 0.1, "right": {"trigger": 1.0}},
		{"time": 0.2, "right": {"tip": over}},
		{"time": 0.4, "right": {"tip": over + surface.global_basis.x.normalized() * 0.05}},
		# Still held, the hand slides off the panel's edge and well away from it.
		{"time": 0.6, "right": {"tip": over + surface.global_basis.x.normalized() * 0.3}},
		{"time": 0.8, "right": {"tip": OPEN_AIR}},
		{"time": 0.9, "right": {"tip": OPEN_AIR + Vector3(0.1, 0.0, 0.0)}},
		{"time": 1.0, "right": {"trigger": 0.0}},
		{"time": 1.1},
	])
	check_eq(_strokes(), 0, "a trigger pulled over a panel paints nothing, even after sliding off")
	check_eq(shell.over_panel, false, "the hand is free again")
	await sim.play([
		{"time": 0.0, "right": {"tip": OPEN_AIR}},
		{"time": 0.1, "right": {"trigger": 1.0}},
		{"time": 0.2, "right": {"tip": OPEN_AIR}},
		{"time": 0.5, "right": {"tip": OPEN_AIR + Vector3(0.15, 0.05, 0.0)}},
		{"time": 0.6, "right": {"trigger": 0.0}},
		{"time": 0.7},
	])
	check_eq(_strokes(), 1, "away from the panels the trigger paints")
	check_eq(shell._cursor_holder.visible, true, "and the tool cursor shows")
	# A stroke that runs into a panel is not cut short and presses nothing there.
	var eraser: Button = (surface.content as ToolsPanel)._strip.buttons["eraser"]
	var through := _point(surface, eraser)
	await sim.play([
		{"time": 0.0, "right": {"tip": OPEN_AIR}},
		{"time": 0.1, "right": {"trigger": 1.0}},
		{"time": 0.5, "right": {"tip": through + normal * 0.03}},
		{"time": 0.6, "right": {"tip": through - normal * 0.01}},
		{"time": 0.7, "right": {"tip": through + normal * 0.03}},
		{"time": 0.8, "right": {"trigger": 0.0}},
		{"time": 0.9},
	])
	check_eq(_strokes(), 2, "a stroke carried on across a panel")
	check_eq(_tool(), "paint", "and pressed nothing on it")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_pointing_from_afar_uses_a_ray_and_the_trigger() -> void:
	if not await _start():
		return
	var surface := await _open("Tools")
	var panel: ToolsPanel = surface.content
	var eraser: Button = panel._strip.buttons["eraser"]
	var target := _point(surface, eraser)
	var from := target + _normal(surface) * 0.35 + Vector3(0.1, 0.0, 0.0)
	await sim.play([{"time": 0.0, "right": {"tip": from, "toward": target}}, {"time": 0.1}])
	check_eq(shell.pointer.mode, XRPointer.Mode.RAY, "a hand pointing at a panel from afar shows the ray")
	check_eq(shell.pointer._ray.visible, true, "the ray is drawn")
	check_eq(shell.pointer._dot.visible, true, "with its dot")
	check((shell.pointer._dot.global_position - target).length() < 0.004, "the dot is on the button")
	check_eq(surface.viewport.gui_get_hovered_control(), eraser, "which is hovered")
	check_eq(_tool(), "paint", "nothing is pressed yet")
	await sim.play([{"time": 0.0, "right": {"trigger": 1.0}}, {"time": 0.1, "right": {"trigger": 0.0}}, {"time": 0.2}])
	check_eq(_tool(), "eraser", "the trigger is the button")
	check_eq(_strokes(), 0, "and it does not paint")
	# Beyond reach there is no ray, and the trigger is the tool's again.
	var far := target + _normal(surface) * (XRPointer.RAY_REACH + 0.3)
	await sim.play([{"time": 0.0, "right": {"tip": far, "toward": target}}, {"time": 0.1}])
	check_eq(shell.pointer.mode, XRPointer.Mode.NONE, "no ray from farther than its reach")
	check_eq(shell.over_panel, false, "the hand is free there")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_dominant_hand_preference_swaps_the_drawing_hand() -> void:
	if not await _start():
		return
	var left := Vector3(-0.3, 1.2, -0.6)
	var right := Vector3(0.3, 1.2, -0.6)
	await sim.play([{"time": 0.0, "left": {"tip": left}, "right": {"tip": right}}, {"time": 0.05}])
	check_eq(shell.drawing_side(), RIGHT, "the right hand draws by default")
	check(world.scene.tool_cursor().position.is_equal_approx(right), "the tool is at the right hand's tip")
	check_eq(shell._markers[RIGHT].drawing, true, "whose marker is the coloured one")
	Preferences.shared().set_value("dominant_hand", "left")
	await sim.play([{"time": 0.0}, {"time": 0.05}])
	check_eq(shell.drawing_side(), LEFT, "now the left hand draws")
	check(world.scene.tool_cursor().position.is_equal_approx(left), "the tool is at the left hand's tip")
	check_eq(shell._markers[LEFT].drawing, true, "the left marker is coloured")
	check_eq(shell._markers[RIGHT].drawing, false, "and the right one is not")
	# The left trigger paints; the right hand's upper button is the menu.
	await sim.play([
		{"time": 0.0, "left": {"trigger": 1.0}},
		{"time": 0.3, "left": {"tip": left + Vector3(0.0, 0.15, 0.0)}},
		{"time": 0.4, "left": {"trigger": 0.0}},
		{"time": 0.5, "right": {"upper": true}},
		{"time": 0.55, "right": {"upper": false}},
		{"time": 0.6},
	])
	check_eq(_strokes(), 1, "the left trigger painted")
	check_eq(shell.palette.shown, true, "the right hand's upper button brought the palette out")
	var tools: XRPanelSurface = shell.palette.surfaces["Tools"]
	check(tools.global_position.x < sim.aims[RIGHT].global_position.x - XRPalette.SIDE_GAP, "which now lies to the left of the right hand")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_keyboard_types_into_the_layer_name() -> void:
	if not await _start():
		return
	run({"cmd": "select_layers", "layers": [int(row_named("Paint").id)]})
	var surface := await _open("Layer")
	var panel: PropertiesPanel = surface.content
	var field := panel._name
	check_eq(field.text, "Paint", "the field shows the layer's name")
	check_eq(shell.palette.keyboard.visible, false, "no keyboard before a text box is touched")
	# Poke the box past the end of its text, which puts the caret there.
	var rect := field.get_global_rect()
	var at := surface.world_point(Vector2(rect.position.x + rect.size.x * 0.85, rect.get_center().y))
	await sim.play(sim.poke_steps(RIGHT, at, _normal(surface)))
	await frames(3)
	check_eq(field.is_editing(), true, "poking the box starts typing in it")
	var keyboard := shell.palette.keyboard
	check_eq(keyboard.visible, true, "and brings up the keyboard")
	check(keyboard.global_position.x > surface.global_position.x, "beside the panel, away from the hand that holds it")
	for caption: String in ["space", "Capital letter next", "T", "w", "o", "2"]:
		await _poke(keyboard, _key(caption))
	check_eq(field.text, "Paint Two2", "the keys type into the box")
	await _poke(keyboard, _key("Delete the character before the caret"))
	check_eq(field.text, "Paint Two", "backspace takes one away")
	await _poke(keyboard, _key("Enter"))
	await frames(3)
	check_eq(row_named("Paint Two").get("kind"), "paint", "Enter renames the layer")
	check_eq(keyboard.visible, false, "and the keyboard goes when typing ends")
	# The key that puts the keyboard away ends typing without Enter.
	await sim.play(sim.poke_steps(RIGHT, at, _normal(surface)))
	await frames(3)
	check_eq(keyboard.visible, true, "it comes back for the next edit")
	await _poke(keyboard, _key("Put the keyboard away"))
	await frames(3)
	check_eq(keyboard.visible, false, "its own key puts it away")
	check_eq(field.is_editing(), false, "and the box is no longer being typed into")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_stick_scrolls_a_list_under_the_pointer() -> void:
	if not await _start():
		return
	for index in 30:
		run({"cmd": "add_layer", "kind": "paint", "name": "Extra %d" % index})
	var surface := await _open("Layers")
	var panel: LayersPanel = surface.content
	var tree := panel._tree
	check_eq(tree.get_scroll().y, 0.0, "the list starts at its top")
	var before: float = world.scene.tool_settings()["paint"]["size"]
	await sim.play([
		{"time": 0.0, "right": {"tip": _point(surface, tree) + _normal(surface) * 0.03}},
		{"time": 0.1, "right": {"stick": Vector2(0.0, -1.0)}},
		{"time": 0.6, "right": {"stick": Vector2.ZERO}},
		{"time": 0.7},
	])
	check(tree.get_scroll().y > 20.0, "pulling the stick down scrolled the list: %s" % tree.get_scroll().y)
	var after: float = world.scene.tool_settings()["paint"]["size"]
	check_near(after, before, "and did not resize the brush", 1e-6)


func test_the_lower_button_opens_a_context_menu_over_a_panel() -> void:
	if not await _start():
		return
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1)])
	var surface := await _open("Layers")
	var panel: LayersPanel = surface.content
	var tree := panel._tree
	var row := tree.get_root().get_child(tree.get_root().get_child_count() - 1)
	var rect := tree.get_item_area_rect(row, 0)
	var pixel := tree.get_global_transform() * (rect.position + Vector2(40.0, rect.size.y * 0.5))
	await sim.play([
		{"time": 0.0, "right": {"tip": surface.world_point(pixel) + _normal(surface) * 0.03}},
		{"time": 0.1, "right": {"lower": true}},
		{"time": 0.15, "right": {"lower": false}},
		{"time": 0.2},
	])
	check_eq(panel._context.visible, true, "the lower button opens the menu of the row under the tip")
	check_eq(panel._context.is_embedded(), true, "inside the panel")
	check_eq(_strokes(), 1, "and deletes nothing, as it would away from a panel")
	# A press beside the menu puts it away without pressing what is there.
	var beside := surface.world_point(Vector2(surface.pixel_size.x - 30.0, surface.pixel_size.y - 60.0))
	await sim.play(sim.poke_steps(RIGHT, beside, _normal(surface)))
	await frames(2)
	check_eq(panel._context.visible, false, "a press beside a menu closes it")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_a_pop_up_opens_inside_its_panel() -> void:
	if not await _start():
		return
	run({"cmd": "set_tool", "tool": "colorize"})
	var surface := await _open("Tools")
	var panel: ToolsPanel = surface.content
	var option: OptionButton = panel._settings["colorize/mode"]
	await _poke(surface, option)
	var popup := option.get_popup()
	check_eq(popup.visible, true, "poking a drop-down opens it")
	check_eq(popup.is_embedded(), true, "inside the panel, not as a window of the desktop")
	check(surface.viewport.get_embedded_subwindows().has(popup), "it belongs to the panel's own viewport")
	check(Rect2i(Vector2i.ZERO, surface.pixel_size).encloses(Rect2i(popup.position, popup.size)), "and fits on the sheet: %s in %s" % [Rect2i(popup.position, popup.size), surface.pixel_size])
	# Choose "Multiply", the third entry, by poking it.
	var pixel := Vector2(popup.position) + Vector2(popup.size.x * 0.5, popup.size.y * 2.5 / popup.item_count)
	await sim.play(sim.poke_steps(RIGHT, surface.world_point(pixel), _normal(surface)))
	await frames(3)
	check_eq(world.scene.tool_settings()["colorize"]["mode"], "multiply", "an entry is chosen by poking it")
	check_eq(popup.visible, false, "and the drop-down closes")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_button_map_shows_at_start_and_on_a_long_press() -> void:
	if not await _start(true):
		return
	check_eq(shell.button_map.visible, true, "the sheet is shown at start when the preference says so")
	var eyes := sim.head.global_position
	var sheet := shell.button_map.global_position
	check_near(Vector2(sheet.x - eyes.x, sheet.z - eyes.z).length(), XRShell.BUTTON_MAP_DISTANCE, "in front of the eyes", 0.01)
	var content: XRButtonMap = shell.button_map.content
	var text := ""
	for node: Node in content.find_children("*", "Label", true, false):
		text += (node as Label).text + "\n"
	check(text.contains("A (lower button)"), "the right hand's lower button is called A")
	check(text.contains("Y (upper button)"), "the left hand's upper button is called Y")
	check(text.to_lower().contains("drawing hand (right)"), "it says which hand draws")
	# The close button in its corner puts it away.
	await _poke(shell.button_map, shell.button_map._corner)
	check_eq(shell.button_map.visible, false, "its corner button puts it away")
	# A short press of the menu button does nothing; holding it for a second does.
	await sim.play([{"time": 0.0, "left": {"menu": true}}, {"time": 0.5, "left": {"menu": false}}, {"time": 0.6}])
	check_eq(shell.button_map.visible, false, "a short press of the menu button is not enough")
	await sim.play([{"time": 0.0, "left": {"menu": true}}, {"time": 1.2, "left": {"menu": false}}, {"time": 1.3}])
	check_eq(shell.button_map.visible, true, "holding it for a second shows the sheet")
	await sim.play([{"time": 0.0, "left": {"menu": true}}, {"time": 1.2, "left": {"menu": false}}, {"time": 1.3}])
	check_eq(shell.button_map.visible, false, "and again hides it")
	# Settings asks for it too, and it follows the drawing hand.
	var settings := await _open("Settings")
	var button: Button = null
	for node: Node in settings.content.find_children("*", "Button", true, false):
		if (node as Button).text == "Show button map":
			button = node
	check(button != null, "Settings has a Show button map button")
	if button != null:
		button.pressed.emit()
		check_eq(shell.button_map.visible, true, "which shows the sheet")
	Preferences.shared().set_value("dominant_hand", "left")
	await frames(2)
	text = ""
	for node: Node in content.find_children("*", "Label", true, false):
		if not node.is_queued_for_deletion():
			text += (node as Label).text + "\n"
	check(text.contains("X (lower button)") and text.to_lower().contains("drawing hand (left)"), "the sheet is rewritten for a left-handed user")


func test_a_hand_that_is_not_tracked_is_absent() -> void:
	if not await _start():
		return
	await sim.play([{"time": 0.0, "right": {"tip": OPEN_AIR}}, {"time": 0.05}])
	check_eq(world.scene.tool_cursor().tracked, true, "the drawing hand is there")
	await sim.play([{"time": 0.0, "right": {"tracked": false, "trigger": 1.0}}, {"time": 0.2, "right": {"trigger": 0.0}}, {"time": 0.25}])
	check_eq(world.scene.tool_cursor().tracked, false, "a controller that lost tracking is gone for the core")
	check_eq(_strokes(), 0, "and cannot paint")
	# The palette stays where the other hand last held it.
	shell.palette.set_shown(true)
	await frames(2)
	var where := shell.palette.global_position
	await sim.play([{"time": 0.0, "left": {"tracked": false, "tip": Vector3(0.0, 0.5, -0.2)}}, {"time": 0.1}])
	check(shell.palette.global_position.is_equal_approx(where), "the palette waits where it was when its hand is lost")


func test_feedback_from_the_core_reaches_sounds_and_messages() -> void:
	if not await _start():
		return
	check_eq(shell.sounds != null, true, "the shell has its sounds")
	run({"cmd": "set_layer", "layer": int(row_named("Paint").id), "locked": true})
	await sim.play([
		{"time": 0.0, "right": {"tip": OPEN_AIR}},
		{"time": 0.1, "right": {"trigger": 1.0}},
		{"time": 0.2, "right": {"trigger": 0.0}},
		{"time": 0.3},
	])
	check_eq(shell.palette.message(), "Layer \"Paint\" is locked", "why the tool did nothing is shown near the palette")


func test_asking_for_a_headset_leaves_nothing_half_started() -> void:
	var openxr := OpenXRRig.find_interface()
	check(openxr != null, "this build of Godot has OpenXR")
	if openxr == null:
		return
	var available := XRShell.is_available()
	check_eq(openxr.is_initialized(), available, "OpenXR is running exactly when a headset is said to be there")
	# Without one, asking for the headset alone fails with a reason and starts nothing.
	if not available:
		shell = XRShell.new()
		shell.mode = "headset"
		shell.world = world
		shell.editor = editor
		add_child(shell)
		check_eq(shell.started, false, "the shell did not start")
		check(shell.failure.contains("No headset"), "and says why: %s" % shell.failure)
		check_eq(messages.size(), 1, "the reason goes to the user through the editor's messages")
		check_eq(get_viewport().use_xr, false, "the window is still an ordinary one")
		messages.clear()
