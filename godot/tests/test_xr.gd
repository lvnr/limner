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
	# As in Quill, the palette rides above the hand that holds it: the round buttons
	# just above the hand, the panel above them, a little toward the middle.
	var tabs := shell.palette.tabs
	check_near(tabs.position.y, XRPalette.COINS_ABOVE_HAND, "the round buttons are just above the hand")
	check(tools.position.y - tools.metres().y * 0.5 > tabs.position.y, "the panel is above them: %s vs %s" % [tools.position, tabs.position])
	check(tools.position.x > 0.0, "a little toward the middle of the body, right of the left hand")
	check_eq(shell.palette.coins.keys(), ["Tools", "Color", "Layers", "Transform", "Monitor", "Document", "Settings", "Performance", "Export"], "the round buttons of Quill's Navigation, painting panels first")
	check(not shell.palette.coins.has("Layer") and not shell.palette.coins.has("Text"), "the Layer and Text panels are opened from keys of other panels")
	for title: String in shell.palette.surfaces:
		var sheet: XRPanelSurface = shell.palette.surfaces[title]
		check(sheet.content.find_children("*", "ScrollContainer", true, false).is_empty(), "nothing scrolls on the %s sheet" % title)
		check(sheet.find_children("*", "ScrollContainer", true, false).is_empty() and sheet.viewport.find_children("*", "ScrollContainer", true, false).is_empty(), "nor does the %s sheet itself" % title)
	check_eq((shell.palette.coins["Tools"] as Button).button_pressed, true, "the one of the panel in front is down")
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
	# The value beside the slider grows wider as the size does, so the slider itself
	# may end a little narrower: the knob is measured against where it is now.
	var now := slider.get_global_rect()
	var let_go_at := surface.locate(to).x
	check_near(slider.get_as_ratio(), clampf((let_go_at - now.position.x) / now.size.x, 0.0, 1.0), "the knob is where the tip let go", 0.04)
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
	check_eq(surface._corner.tooltip_text, "Send this panel back to the palette", "which says so")
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
	check_eq(surface._corner.tooltip_text, "Leave this panel here in the room", "and its corner key is a pin again")
	shell.palette.set_shown(true)
	await frames(3)
	check_eq(surface.top_level, false, "it moves with the palette again")
	check_near(surface.position.x, shell.palette.tabs.position.x, "and sits over the round buttons")
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
	var key_top := target + _normal(surface) * surface.rest_at(surface.pixel_of(eraser))
	check((shell.pointer._dot.global_position - key_top).length() < 0.004, "the dot is on the button's key")
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
	check(tools.position.x < 0.0, "which now rides toward the middle, left of the right hand")
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
	var eyes := sim.head.global_position
	check(keyboard.global_position.y < eyes.y - 0.2, "low, in front of the eyes")
	check(keyboard.global_basis.z.dot((eyes - keyboard.global_position).normalized()) > 0.99, "facing them")
	var placed := keyboard.global_transform
	# It stays where it is while the hand that holds the palette moves.
	await sim.play([{"time": 0.0, "left": {"tip": Vector3(-0.3, 1.05, -0.32)}}, {"time": 0.1}])
	check(keyboard.global_transform.is_equal_approx(placed), "it holds still in the room")
	for caption: String in ["space", "Capital letter next", "T", "w", "o", "2"]:
		await _poke(keyboard, _key(caption))
	check_eq(field.text, "Paint Two2", "the keys type into the box")
	check_eq(shell.palette._keys._typed.text, "Paint Two2|", "and what is typed shows over the keys")
	await _poke(keyboard, _key("Delete the character before the caret"))
	check_eq(field.text, "Paint Two", "backspace takes one away")
	await _poke(keyboard, _key("Enter"))
	await frames(3)
	check_eq(row_named("Paint Two").get("kind"), "paint", "Enter renames the layer")
	check_eq(keyboard.visible, false, "and the keyboard goes when typing ends")
	# The key that puts the keyboard away ends typing without Enter. (The panel moved
	# with the hand.)
	rect = field.get_global_rect()
	at = surface.world_point(Vector2(rect.position.x + rect.size.x * 0.85, rect.get_center().y))
	await sim.play(sim.poke_steps(RIGHT, at, _normal(surface)))
	await frames(3)
	check_eq(keyboard.visible, true, "it comes back for the next edit")
	await _poke(keyboard, _key("Put the keyboard away"))
	await frames(3)
	check_eq(keyboard.visible, false, "its own key puts it away")
	check_eq(field.is_editing(), false, "and the box is no longer being typed into")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_stick_pages_the_layer_rows_under_the_pointer() -> void:
	if not await _start():
		return
	for index in 30:
		run({"cmd": "add_layer", "kind": "paint", "name": "Extra %d" % index})
	run({"cmd": "select_layers", "layers": [int(row_named("Paint").id)]})
	var surface := await _open("Layers")
	var panel: TimelinePanel = surface.content
	var rows := panel.layer_rows
	check_eq(rows.pager.first, 0, "the rows start at the first page")
	var before: float = world.scene.tool_settings()["paint"]["size"]
	var name_key: Button = rows._lines[2].name
	await sim.play([
		{"time": 0.0, "right": {"tip": _point(surface, name_key) + _normal(surface) * 0.03}},
		{"time": 0.1, "right": {"stick": Vector2(0.0, -1.0)}},
		{"time": 0.6, "right": {"stick": Vector2.ZERO}},
		{"time": 0.7},
	])
	check(rows.pager.first > 0, "pulling the stick down stepped the page: %d" % rows.pager.first)
	var after: float = world.scene.tool_settings()["paint"]["size"]
	check_near(after, before, "and did not resize the brush", 1e-6)


func test_the_stick_zooms_and_moves_the_timeline_under_the_tip() -> void:
	if not await _start():
		return
	var surface := await _open("Layers")
	var panel: TimelinePanel = surface.content
	var view := panel.view
	var tracks := view.tracks_rect()
	var spot := surface.world_point(surface.pixel_of(view) - view.size * 0.5 + tracks.get_center())
	var zoom := view.axis.zoom
	var size_before: float = world.scene.tool_settings()["paint"]["size"]
	await sim.play([
		{"time": 0.0, "right": {"tip": spot + _normal(surface) * 0.03}},
		{"time": 0.1, "right": {"stick": Vector2(0.0, 1.0)}},
		{"time": 0.4, "right": {"stick": Vector2.ZERO}},
		{"time": 0.5},
	])
	check(view.axis.zoom > zoom * 1.5, "pushing the stick up over the tracks zooms in: %f to %f" % [zoom, view.axis.zoom])
	var start := view.axis.start
	await sim.play([
		{"time": 0.0, "right": {"stick": Vector2(1.0, 0.0)}},
		{"time": 0.3, "right": {"stick": Vector2.ZERO}},
		{"time": 0.4},
	])
	check(view.axis.start > start, "and pushing it right moves on along time")
	check_near(world.scene.tool_settings()["paint"]["size"], size_before, "the brush is not resized", 1e-6)
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_lower_button_over_a_panel_opens_nothing() -> void:
	if not await _start():
		return
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1)])
	var surface := await _open("Layers")
	var panel: TimelinePanel = surface.content
	var name_key: Button = panel.layer_rows._lines[2].name
	await sim.play([
		{"time": 0.0, "right": {"tip": _point(surface, name_key) + _normal(surface) * 0.03}},
		{"time": 0.1, "right": {"lower": true}},
		{"time": 0.15, "right": {"lower": false}},
		{"time": 0.2},
	])
	check(panel.layer_rows._context == null, "a headset's layer rows have no menu at all")
	check(surface.viewport.get_embedded_subwindows().is_empty(), "nothing at all")
	check_eq(_strokes(), 1, "and nothing is deleted, as it would be away from a panel")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_a_choice_is_a_key_poked_not_a_list_opened() -> void:
	if not await _start():
		return
	run({"cmd": "set_tool", "tool": "colorize"})
	var surface := await _open("Tools")
	var panel: ToolsPanel = surface.content
	var modes: KeyGrid = panel._settings["colorize/mode"]
	await _poke(surface, modes.buttons["multiply"])
	await frames(3)
	check_eq(world.scene.tool_settings()["colorize"]["mode"], "multiply", "a blend mode is chosen by poking its key")
	check(surface.viewport.get_embedded_subwindows().is_empty(), "nothing opened over the panel")
	check_eq(modes.get_value(), "multiply", "and its key stays down")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_button_map_shows_at_start_and_on_the_menu_button() -> void:
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
	# A press of the menu button shows it, as in Quill, and the next hides it; holding
	# the button does not toggle it again.
	await sim.play([{"time": 0.0, "left": {"menu": true}}, {"time": 0.1, "left": {"menu": false}}, {"time": 0.2}])
	check_eq(shell.button_map.visible, true, "a press of the menu button shows the sheet")
	await sim.play([{"time": 0.0, "left": {"menu": true}}, {"time": 1.2, "left": {"menu": false}}, {"time": 1.3}])
	check_eq(shell.button_map.visible, false, "the next press hides it, however long it is held")
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


func test_the_document_panel_saves_and_opens_in_the_headset() -> void:
	if not await _start():
		return
	var folder := LayerTestKit.scratch_dir().path_join("Limner")
	Documents.use_folder(folder)
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1)])
	await frames(2)
	var surface := await _open("Document")
	var panel: DocumentPanel = surface.content
	for key: String in ["save", "save_as", "new", "open", "import"]:
		check(panel._file_buttons[key].is_visible_in_tree(), "the %s button is on the palette's Document panel" % key)

	await _poke(surface, panel._file_buttons.save)
	await frames(3)
	check_eq(panel.file_mode, "save_as", "Save on a new document asks for a name, in the panel")
	var field := panel._save_name
	var rect := field.get_global_rect()
	await sim.play(sim.poke_steps(RIGHT, surface.world_point(Vector2(rect.position.x + rect.size.x * 0.9, rect.get_center().y)), _normal(surface)))
	await frames(3)
	check_eq(shell.palette.keyboard.visible, true, "the name box brings up the keyboard")
	# Whatever the box holds is cleared with the keyboard's own backspace.
	while field.text != "" and field.caret_column > 0:
		await _poke(shell.palette.keyboard, _key("Delete the character before the caret"))
	for caption: String in ["Capital letter next", "S", "k", "y"]:
		await _poke(shell.palette.keyboard, _key(caption))
	check_eq(field.text, "Sky", "a name typed with the keyboard")
	await _poke(shell.palette.keyboard, _key("Enter"))
	await frames(3)
	check(FileAccess.file_exists(folder.path_join("Sky.limnz")), "Enter saves it in the documents folder")
	check_eq(editor.path, folder.path_join("Sky.limnz"), "as the document's file")

	await _poke(surface, panel._file_buttons.new)
	await frames(3)
	check_eq(_strokes(), 0, "New starts an empty document; nothing was unsaved, so nothing was asked")
	await _poke(surface, panel._file_buttons.open)
	await frames(3)
	var entry: Button = null
	for node: Node in panel._drawer.find_children("*", "Button", true, false):
		if (node as Button).text == "Sky" and (node as Button).is_visible_in_tree():
			entry = node
	check(entry != null, "Open lists the project saved")
	if entry != null:
		await _poke(surface, entry)
		await frames(3)
		check_eq(editor.path, folder.path_join("Sky.limnz"), "poking it opens it")
		check_eq(_strokes(), 1, "with its stroke")
	Documents.use_folder("")
	LayerTestKit.remove(folder.get_base_dir())
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_a_panel_on_a_hidden_sheet_does_no_work() -> void:
	run({"cmd": "add_layer", "kind": "camera", "name": "Shot"})
	if not await _start():
		return
	var surface := await _open("Monitor")
	var panel: MonitorPanel = surface.content
	await frames(2)
	check(panel.is_visible_in_tree(), "the panel on the sheet in front is shown")
	check(panel.viewfinder.is_live(), "and the Monitor's picture is rendered")
	shell.palette.show_panel("Tools")
	await frames(2)
	check(not surface.visible, "another panel in front hides the Monitor's sheet")
	check(not panel.is_visible_in_tree(), "and the panel on it, though it is in a viewport of its own")
	check(not panel.viewfinder.is_live(), "so its picture is not rendered")
	shell.palette.show_panel("Monitor")
	await frames(3)
	check(panel.viewfinder.is_live(), "until it is in front again")
	shell.palette.set_shown(false)
	await frames(2)
	check(not panel.viewfinder.is_live(), "putting the palette away stops it too")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_unsaved_work_is_kept_when_the_session_ends_without_asking() -> void:
	if not await _start():
		return
	Autosave.clear()
	check(not shell.keep_unsaved_work(), "nothing to keep in a clean document")
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1)])
	await frames(2)
	check(shell.keep_unsaved_work(), "unsaved work is copied before the app closes")
	check(not Autosave.pending().is_empty(), "and will be opened again at the next start")
	Autosave.clear()


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


func test_buttons_stand_off_the_sheet_as_keys() -> void:
	if not await _start():
		return
	var surface := await _open("Tools")
	var panel: ToolsPanel = surface.content
	await frames(4)
	var eraser: Button = panel._strip.buttons["eraser"]
	var paint: Button = panel._strip.buttons["paint"]
	check(surface.keys.count > 10, "the panel's buttons stand up as keys: %d" % surface.keys.count)
	check_near(surface.rest_at(surface.pixel_of(eraser)), XRPanelKeys.HEIGHT, "a button stands its full height", 1e-5)
	check_near(surface.rest_at(surface.pixel_of(paint)), XRPanelKeys.HEIGHT * XRPanelKeys.LATCHED, "the chosen tool is latched half down", 1e-5)
	eraser.disabled = true
	surface.wake()
	await frames(4)
	check_eq(surface.rest_at(surface.pixel_of(eraser)), 0.0, "a disabled button is flush with the sheet")
	eraser.disabled = false
	surface.wake()
	await frames(4)
	# The round buttons under the panel are keys too.
	var coin: Button = shell.palette.coins["Color"]
	check_near(shell.palette.tabs.rest_at(shell.palette.tabs.pixel_of(coin)), XRPanelKeys.ROUND_HEIGHT, "a round button stands taller", 1e-5)
	# Check boxes stay flat, as Quill's do.
	var settings := await _open("Settings")
	await frames(4)
	var flat: CheckBox = null
	for node: Node in settings.content.find_children("*", "CheckBox", true, false):
		if (node as CheckBox).is_visible_in_tree():
			flat = node
			break
	check(flat != null, "the Settings panel has a check box")
	if flat != null:
		check_eq(settings.rest_at(settings.pixel_of(flat)), 0.0, "a check box stays flat")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_a_key_goes_down_with_the_tip_and_presses_at_its_top() -> void:
	if not await _start():
		return
	var surface := await _open("Tools")
	var panel: ToolsPanel = surface.content
	await frames(4)
	var eraser: Button = panel._strip.buttons["eraser"]
	var pixel := surface.pixel_of(eraser)
	var normal := _normal(surface)
	var face := surface.world_point(pixel)
	# The tip resting 1 mm into the key: the key goes down to meet it, nothing presses.
	var touching := XRPanelKeys.HEIGHT - 0.001
	await sim.play([{"time": 0.0, "right": {"tip": face + normal * 0.03}}, {"time": 0.2, "right": {"tip": face + normal * touching}}, {"time": 0.3}])
	await frames(2)
	check_near(surface.keys.top_at(pixel), touching, "the key went down with the tip", 2e-4)
	check_eq(_tool(), "paint", "a key pushed 1 mm does not press")
	# 3.5 mm down presses, though the tip is still above the sheet.
	var pushed := XRPanelKeys.HEIGHT - 0.0035
	await sim.play([{"time": 0.0, "right": {"tip": face + normal * pushed}}, {"time": 0.1}])
	await frames(2)
	check(pushed > 0.0, "still in front of the sheet")
	check_eq(surface._pressed, true, "pushed 3.5 mm down, the key presses")
	await sim.play([{"time": 0.0, "right": {"tip": face + normal * 0.03}}, {"time": 0.1}])
	check_eq(_tool(), "eraser", "and the button acts when the tip comes back")
	# On the flat of the sheet the same height presses nothing.
	var label_pixel := Vector2(surface.pixel_size.x * 0.5, surface.pixel_of(eraser).y + 60.0)
	var flat_rest := surface.rest_at(label_pixel)
	if flat_rest == 0.0:
		var plain := surface.world_point(label_pixel)
		await sim.play([{"time": 0.0, "right": {"tip": plain + normal * 0.03}}, {"time": 0.2, "right": {"tip": plain + normal * pushed}}, {"time": 0.3}])
		check_eq(surface._pressed, false, "over the flat sheet, 1.5 mm in front of it is not a press")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_round_buttons_choose_the_panel_and_the_pin_leaves_it_in_the_room() -> void:
	if not await _start():
		return
	var surface := await _open("Tools")
	var tabs := shell.palette.tabs
	await _poke(tabs, shell.palette.coins["Color"])
	check_eq(shell.palette.current, "Color", "the Color button brings its panel to the front")
	check_eq((shell.palette.coins["Color"] as Button).button_pressed, true, "and stays down")
	check_eq((shell.palette.coins["Tools"] as Button).button_pressed, false, "the Tools button comes up")
	var color: XRPanelSurface = shell.palette.surfaces["Color"]
	await frames(3)
	var where := color.global_transform
	check_eq(color._corner.visible, true, "a panel on the palette has a pin")
	await _poke(color, color._corner)
	check_eq(shell.palette.is_pinned("Color"), true, "the pin leaves it in the room")
	check(color.global_position.distance_to(where.origin) < 0.002, "where it was")
	check(not surface.is_visible_in_tree() or shell.palette.current != "Color", "the palette moved on to another panel")
	# The hand that holds the palette moves away; the pinned panel stays.
	await sim.play([{"time": 0.0, "left": {"tip": sim.tip_position(LEFT) + Vector3(-0.2, -0.2, 0.1)}}, {"time": 0.2}])
	check(color.global_position.distance_to(where.origin) < 0.002, "and stays when the hand moves")
	await _poke(color, color._corner)
	check_eq(shell.palette.is_pinned("Color"), false, "its key sends it back")
	check_eq(shell.palette.current, "Color", "to the front of the palette")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_pointing_at_a_panel_from_afar_leaves_the_grip_to_the_artwork() -> void:
	if not await _start():
		return
	var surface := await _open("Tools")
	var target := surface.world_point(surface.pixel_size * 0.5)
	var from := target + _normal(surface) * 0.4 + Vector3(0.15, -0.05, 0.0)
	await sim.play([{"time": 0.0, "right": {"tip": from, "toward": target}}, {"time": 0.1}])
	check_eq(shell.pointer.mode, XRPointer.Mode.RAY, "the hand points at the panel from afar")
	var before := world.scene.viewer_from_world()
	var panel_before := surface.global_transform
	await sim.play([
		{"time": 0.0, "right": {"grip": 1.0}},
		{"time": 0.1, "right": {"tip": from}},
		{"time": 0.4, "right": {"tip": from + Vector3(0.0, 0.2, 0.0)}},
		{"time": 0.5, "right": {"grip": 0.0}},
		{"time": 0.6},
	])
	check(not world.scene.viewer_from_world().is_equal_approx(before), "the grip took hold of the view and moved it")
	check_eq(shell.palette.is_pinned("Tools"), false, "the panel was not picked up")
	check(surface.global_transform.origin.distance_to(panel_before.origin) < 0.05, "and did not go with the hand")
	# With the modifier held, presses on a panel come with Ctrl.
	await sim.play([{"time": 0.0, "left": {"trigger": 1.0}, "right": {"tip": from, "toward": target}}, {"time": 0.1}])
	check_eq(surface.modifier, true, "the other hand's trigger is Ctrl on the panel")
	await sim.play([{"time": 0.0, "left": {"trigger": 0.0}}, {"time": 0.1}])
	check_eq(surface.modifier, false, "and lets go of it")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_palette_hand_shows_the_layer_the_frame_and_the_stick() -> void:
	if not await _start():
		return
	await frames(2)
	var dial := shell.dial
	check_eq(dial.visible, true, "the disc rides on the hand that holds the palette")
	check_eq(dial.layer_name, "Paint", "its tag names the current layer")
	check_eq(dial.frame_text, "1", "and the ring shows the frame it is on")
	check_eq(dial.stick_shown, false, "the stick's icons wait for the thumb")
	var eyes := sim.head.global_position
	check(dial.global_basis.z.dot((eyes - dial.global_position).normalized()) > 0.99, "the disc faces the eyes")
	# The thumb on the stick shows what it does; pushing lights one direction.
	await sim.play([{"time": 0.0, "left": {"stick_touch": true}}, {"time": 0.1}])
	check_eq(dial.stick_shown, true, "a thumb on the stick shows its icons")
	check_eq(dial.meaning("up"), "New frame", "up makes a new frame")
	check_eq(dial.meaning("left"), "Frame before", "left goes to the frame before")
	await sim.play([{"time": 0.0, "left": {"trigger": 1.0}}, {"time": 0.1}])
	check_eq(dial.meaning("up"), "Copy frame", "with the modifier, up copies the frame")
	check_eq(dial.meaning("down"), "Delete frame", "and down deletes it")
	await sim.play([{"time": 0.0, "left": {"trigger": 0.0, "stick": Vector2(0.9, 0.0)}}, {"time": 0.05, "left": {"stick": Vector2.ZERO}}, {"time": 0.3}])
	check_eq(dial.meaning("right"), "Frame after", "back to the plain meanings")
	await sim.play([{"time": 0.0, "left": {"stick": Vector2(0.0, 0.9)}}, {"time": 0.05}])
	check_eq(dial.lit, "up", "the direction pushed lights up")
	await sim.play([{"time": 0.0, "left": {"stick": Vector2.ZERO, "stick_touch": false}}, {"time": 0.3}])
	check_eq(dial.stick_shown, false, "lifting the thumb puts the icons away")
	check_eq(dial.frame_text, "2", "the new frame the stick made is the one shown")
	# Choosing another layer names it.
	run({"cmd": "add_layer", "kind": "group"})
	await frames(3)
	check_eq(dial.layer_name, row_named("Group").get("name", "?"), "the tag follows the current layer")
	check_eq(dial.frame_text, "", "a group has no frame number")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_every_action_the_rig_reads_is_made_and_bound() -> void:
	# Godot renames the pose actions as it makes them ("grip_pose" becomes "grip") and
	# then refuses an action whose name is taken, with every binding to it. A grip
	# action called "grip" was refused so, and no grip worked in a headset.
	var map := load("res://xr/action_map.tres") as OpenXRActionMap
	var renamed := {"default_pose": "default", "aim_pose": "aim", "grip_pose": "grip"}
	var made: Array[String] = []
	for action_set: OpenXRActionSet in map.action_sets:
		for action: OpenXRAction in action_set.actions:
			var stored: String = renamed.get(action.resource_name, action.resource_name)
			check(not made.has(stored), "the action %s is made as %s, a name already taken" % [action.resource_name, stored])
			made.append(stored)
	var read: Array[StringName] = [OpenXRRig.ACTION_TRIGGER, OpenXRRig.ACTION_GRIP, OpenXRRig.ACTION_STICK, OpenXRRig.ACTION_STICK_CLICK, OpenXRRig.ACTION_STICK_TOUCH, OpenXRRig.ACTION_TRACKPAD, OpenXRRig.ACTION_TRACKPAD_CLICK, OpenXRRig.ACTION_LOWER, OpenXRRig.ACTION_UPPER, OpenXRRig.ACTION_MENU, OpenXRRig.ACTION_HAPTIC, &"aim", &"grip"]
	for wanted: StringName in read:
		check(made.has(String(wanted)), "the rig reads %s and the action map makes it" % wanted)
	# Every controller has its grip bound; a bare hand's profile need not.
	for profile: OpenXRInteractionProfile in map.interaction_profiles:
		if OpenXRRig.HAND_PROFILES.has(profile.interaction_profile_path):
			continue
		var bound := false
		for binding: OpenXRIPBinding in profile.bindings:
			if binding.action != null and binding.action.resource_name == String(OpenXRRig.ACTION_GRIP):
				bound = true
		check(bound, "%s binds the grip" % profile.interaction_profile_path)


func test_bare_hands_count_only_when_asked_for() -> void:
	check_eq(Preferences.DEFAULTS["hand_tracking"], false, "bare hands are off unless asked for")
	check(OpenXRRig.HAND_PROFILES.has("/interaction_profiles/khr/simple_controller"), "the simple controller, the profile a hand is given, counts as a bare hand")


func test_the_window_gets_its_own_size_back_from_the_headset() -> void:
	# Marked for a headset, the window's viewport takes the headset's size (here, with
	# no headset, none at all) and does not take its own back when the mark goes.
	var window := get_window()
	var own := Vector2i(window.get_visible_rect().size)
	window.use_xr = true
	await frames(2)
	check(Vector2i(window.get_visible_rect().size) != own, "marked for a headset, the viewport is not the window's size")
	OpenXRRig.give_back(window)
	await frames(2)
	check_eq(window.use_xr, false, "the window is no longer drawn for the headset")
	check_eq(Vector2i(window.get_visible_rect().size), own, "and its viewport is the window's size again")
