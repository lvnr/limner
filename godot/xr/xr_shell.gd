class_name XRShell
extends Node
## The headset presentation: the thin layer between a headset and the editing session.
##
## Everything a tool, a button or a stick does is decided in the core, exactly as on
## the desktop. This shell only supplies what a headset adds: it reads the head and
## the two controllers every frame, says which hand draws, hands the lot to the core,
## and acts on what the core answers (a buzz, a sound, a message, the palette). It
## also owns the palette of panels in the other hand and the pointer that works them.
##
## The rig that says where head and hands are is either a real headset (`OpenXRRig`)
## or a simulated one (`SimRig`); nothing here depends on which.
##
## Everything drawn is in viewer space, the room the user stands in. The rig sits at
## the room's origin and never moves: navigation changes where the artwork is.

## Starting in a headset did not work; the app should fall back to the desktop.
signal start_failed(reason: String)

## The trigger and grip levels at which the core takes hold of a hand and lets it go
## again, from 0 to 1. They match the core's own (`PRESS` and `RELEASE` in
## `session/input.rs`, `GRIP_PRESS` and `GRIP_RELEASE` in `session/grab.rs`).
const TRIGGER_PRESS := 0.06
const TRIGGER_RELEASE := 0.02
const GRIP_PRESS := 0.35
const GRIP_RELEASE := 0.2
## How long the menu button is held before the button map shows or hides, in seconds.
const MENU_HOLD_SECONDS := 1.0
## The buzz for a press on a panel: its strength from 0 to 1 and its length in
## seconds. Tune on real hardware; it should be felt, not heard.
const PRESS_PULSE_STRENGTH := 0.25
const PRESS_PULSE_SECONDS := 0.02
## The buzz for picking a panel up and for putting it down.
const GRAB_PULSE_STRENGTH := 0.4
const GRAB_PULSE_SECONDS := 0.03
## Size of the desktop window that mirrors the headset for onlookers, in pixels.
const MIRROR_SIZE := Vector2i(1280, 720)
## Where "frame everything" puts the artwork: around this point of the room, in
## metres, within this radius.
const FRAME_TARGET := Vector3(0.0, 1.3, -1.2)
const FRAME_RADIUS := 0.5
## Width of the button map, in its own pixels.
const BUTTON_MAP_WIDTH := 620
## The most room the button map's rows get, in pixels: enough to show them all, so
## that the sheet that explains the controls needs none of them to be read.
const BUTTON_MAP_MAX_HEIGHT := 900
## How far in front of the eyes the button map appears and how far below them, in
## metres, and how much larger than a hand-held panel it is shown, being farther off.
const BUTTON_MAP_DISTANCE := 0.9
const BUTTON_MAP_DROP := 0.2
const BUTTON_MAP_MAGNIFICATION := 1.3
const MIRROR_HINT := "Limner is running in the headset. This window shows what one eye sees."
const SIM_HINT := "Simulated headset.  Mouse: drawing hand   Wheel: nearer, farther   Buttons: trigger, grip   Q E: face buttons   Arrows: stick   Tab: other hand   M: palette   H: button map"

var world: LimnerWorld
var editor: LimnerEditor
## Which rig to use: "auto" (a headset if one is there or was asked for, else the
## simulated one), "headset" or "sim". Set it before the shell enters the tree.
var mode := "auto"
## True once the shell is running. False after `_ready` means starting failed.
var started := false
## Why starting failed, in words for the user.
var failure := ""
var rig: XRRig
var palette: XRPalette
var pointer: XRPointer
var sounds: Sounds
var autosave: Autosave
## The sheet that lists what every control does.
var button_map: XRPanelSurface
## True while the drawing hand belongs to a panel and so must not paint.
var over_panel := false

var _preferences: Preferences
var _markers: Array[XRControllerModel] = []
var _cursor_holder: Node3D
## True from a pull of the trigger (or grip) over a panel until it is let go, so that
## sliding off the panel with it still held does not start a stroke (or a grab).
var _trigger_withheld := false
var _grip_withheld := false
## True while the core has the trigger or the grip, when the panels must wait.
var _core_has_trigger := false
var _core_has_grip := false
var _menu_time := 0.0


## True when a headset runtime is there and running, so that the shell could start in
## it. Cheap, and safe to call when there is none. When it returns true OpenXR has
## been started and stays started; ask only when a headset would be used.
static func is_available() -> bool:
	return OpenXRRig.initialise()


func _ready() -> void:
	_preferences = Preferences.shared()
	var arguments := OS.get_cmdline_user_args()
	var note := ""
	if mode == "headset" or (mode == "auto" and not arguments.has("--sim")):
		var headset := OpenXRRig.new()
		headset.name = "Rig"
		world.add_child(headset)
		var problem := headset.start(get_viewport())
		if problem == "":
			rig = headset
		else:
			world.remove_child(headset)
			headset.free()
			if mode == "headset" or arguments.has("--no-sim"):
				_fail(problem)
				return
			note = "No headset was found. This is the simulated one."
	if rig == null:
		rig = SimRig.new()
		world.add_child(rig)
	rig.attention_changed.connect(_on_attention_changed)
	rig.recentred.connect(_on_recentred)
	rig.closed.connect(func() -> void: get_tree().quit())

	for side in 2:
		rig.grips[side].add_child(XRControllerModel.handle())
		var marker := XRControllerModel.new()
		rig.aims[side].add_child(marker)
		_markers.append(marker)
	_cursor_holder = Node3D.new()
	_cursor_holder.name = "ToolCursorHolder"
	rig.add_child(_cursor_holder)
	var cursor := ToolCursor.new()
	cursor.name = "ToolCursor"
	cursor.scene = world.scene
	_cursor_holder.add_child(cursor)

	sounds = Sounds.new()
	sounds.name = "Sounds"
	add_child(sounds)
	world.scene.playback_event.connect(func(event: String) -> void:
		if event == "stop":
			sounds.play("stop")
	)
	# Unsaved work is copied aside every few minutes. In a headset there is no good
	# moment for a question at start-up, so work left from a session that ended badly
	# is simply opened again.
	autosave = Autosave.new()
	autosave.name = "Autosave"
	autosave.editor = editor
	add_child(autosave)
	if not Autosave.pending().is_empty() and not OS.get_cmdline_user_args().has("--no-recovery"):
		_recover.call_deferred()
	palette = XRPalette.new()
	palette.name = "Palette"
	palette.editor = editor
	rig.add_child(palette)
	palette.button_map_requested.connect(toggle_button_map)
	pointer = XRPointer.new()
	pointer.name = "Pointer"
	rig.add_child(pointer)
	pointer.clicked.connect(_on_panel_clicked)
	pointer.grabbed.connect(_on_panel_grabbed)
	pointer.dropped.connect(_on_panel_dropped)
	button_map = XRPanelSurface.new("Button map", XRButtonMap.new(), BUTTON_MAP_WIDTH)
	button_map.max_body_height = BUTTON_MAP_MAX_HEIGHT
	button_map.magnification = BUTTON_MAP_MAGNIFICATION
	button_map.visible = false
	rig.add_child(button_map)
	button_map.show_corner_button("close", "Put the button map away")
	button_map.corner_pressed.connect(set_button_map_shown.bind(false))
	# The sheet only knows its height once its text has been laid out; it is stood
	# in front of the eyes again when that changes.
	button_map.resized.connect(_on_button_map_resized)

	_show_mirror_hint(SIM_HINT if rig is SimRig else MIRROR_HINT)
	if rig is OpenXRRig and not OS.has_feature("mobile"):
		get_window().size = MIRROR_SIZE
	editor.message.connect(show_message)
	editor.state_changed.connect(_follow_document)
	editor.document_changed.connect(_follow_document)
	editor.document_replaced.connect(_follow_document)
	_preferences.changed.connect(_on_preference_changed)
	_preferences.apply_at_start(editor)
	_follow_document()
	_mark_hands()
	started = true
	if _preferences.flag("show_button_map_at_start"):
		set_button_map_shown(true)
	if note != "":
		show_message(note)
	if rig is SimRig:
		add_child(XRSimDirector.new(self))


func _process(delta: float) -> void:
	if not started:
		return
	var step := rig.poll(delta)
	var main_side := drawing_side()
	var off_side := XRRig.opposite(main_side)
	var main := rig.states[main_side]
	var off := rig.states[off_side]
	var main_tip := rig.tip(main_side)
	var off_tip := rig.tip(off_side)
	palette.follow(rig.aims[off_side].global_transform, off_side == XRRig.LEFT, off.tracked)

	# A hand that is painting or holding the artwork keeps at it; the panels wait.
	pointer.enabled = rig.has_attention and not (_core_has_trigger or _core_has_grip)
	over_panel = pointer.update(main_tip, main, step, touchable())
	_trigger_withheld = main.trigger > TRIGGER_RELEASE and (over_panel or _trigger_withheld)
	_grip_withheld = main.grip > GRIP_RELEASE and (over_panel or _grip_withheld)

	var scene := world.scene
	var trigger := 0.0 if over_panel or _trigger_withheld else main.trigger
	var grip := 0.0 if over_panel or _grip_withheld else main.grip
	if not main.tracked:
		scene.input_hand_absent(0)
		trigger = 0.0
		grip = 0.0
	elif over_panel:
		# Over a panel the hand is there but does nothing: its trigger, grip, stick and
		# buttons belong to the panel.
		scene.input_hand(0, main_tip.origin, main_tip.basis.get_rotation_quaternion(), 0.0, 0.0, Vector2.ZERO, 0)
	else:
		scene.input_hand(0, main_tip.origin, main_tip.basis.get_rotation_quaternion(), trigger, grip, main.stick, main.buttons())
	_core_has_trigger = trigger > TRIGGER_PRESS or (_core_has_trigger and trigger > TRIGGER_RELEASE)
	_core_has_grip = grip > GRIP_PRESS or (_core_has_grip and grip > GRIP_RELEASE)
	if off.tracked:
		scene.input_hand(1, off_tip.origin, off_tip.basis.get_rotation_quaternion(), off.trigger, off.grip, off.stick, off.buttons())
	else:
		scene.input_hand_absent(1)
	scene.input_head(rig.head.global_position)
	for feedback: Dictionary in scene.input_commit(step):
		_react(feedback, main_side, off_side)
	sounds.follow_tool(scene)
	_cursor_holder.visible = not over_panel
	_watch_menu_button(main.menu or off.menu, step)


## The side of the hand that draws: `XRRig.LEFT` or `XRRig.RIGHT`.
func drawing_side() -> int:
	return XRRig.RIGHT if _preferences.text("dominant_hand") == "right" else XRRig.LEFT


## Fits everything visible in front of the viewer.
func frame_all() -> void:
	world.scene.view_frame_all(FRAME_TARGET, FRAME_RADIUS)


## Shows a line of text near the palette for a few seconds.
func show_message(text: String, is_problem := false) -> void:
	palette.show_message(text, is_problem)


func toggle_button_map() -> void:
	set_button_map_shown(not button_map.visible)


## Shows the button map in front of the eyes, or puts it away.
func set_button_map_shown(on: bool) -> void:
	if on and not button_map.visible:
		_place_button_map()
	button_map.visible = on


# ---- what the core answers ----

func _react(feedback: Dictionary, main_side: int, off_side: int) -> void:
	match feedback.type:
		"haptic":
			_pulse(main_side if feedback.main else off_side, feedback.strength, feedback.seconds)
		"sound":
			sounds.play(feedback.name)
		"message":
			show_message(feedback.text, true)
		"menu":
			palette.toggle()


func _pulse(side: int, strength: float, seconds: float) -> void:
	if _preferences.flag("haptics"):
		rig.pulse(side, strength, seconds)


# ---- panels ----

## Every sheet the pointer can touch now: the palette's, and the button map.
func touchable() -> Array[XRPanelSurface]:
	var list := palette.touchable()
	if button_map.visible:
		list.append(button_map)
	return list


func _on_panel_clicked(_surface: XRPanelSurface) -> void:
	sounds.play("click")
	_pulse(drawing_side(), PRESS_PULSE_STRENGTH, PRESS_PULSE_SECONDS)


func _on_panel_grabbed(surface: XRPanelSurface) -> void:
	palette.on_grabbed(surface)
	_pulse(drawing_side(), GRAB_PULSE_STRENGTH, GRAB_PULSE_SECONDS)


func _on_panel_dropped(_surface: XRPanelSurface) -> void:
	_pulse(drawing_side(), GRAB_PULSE_STRENGTH, GRAB_PULSE_SECONDS)


## Holding the menu button for a moment shows or hides the button map, once per hold.
func _watch_menu_button(down: bool, step: float) -> void:
	if not down:
		_menu_time = 0.0
		return
	var before := _menu_time
	_menu_time += step
	if before < MENU_HOLD_SECONDS and _menu_time >= MENU_HOLD_SECONDS:
		toggle_button_map()


func _on_button_map_resized() -> void:
	if button_map.visible:
		_place_button_map()


## Stands the button map in front of the eyes, upright and facing them.
func _place_button_map() -> void:
	var eyes := rig.head.global_position
	var ahead := -rig.head.global_basis.z
	ahead.y = 0.0
	ahead = ahead.normalized() if ahead.length() > 0.01 else Vector3.FORWARD
	var centre := eyes + ahead * BUTTON_MAP_DISTANCE + Vector3.DOWN * BUTTON_MAP_DROP
	button_map.place(Transform3D(Basis.looking_at(ahead, Vector3.UP), centre))


# ---- keeping in step ----

func _follow_document() -> void:
	world.show_grid = editor.get_options().show_grid
	world.show_brush_grid = editor.get_options().show_brush_grid
	get_window().title = "%s%s — Limner (headset)" % [editor.title(), " (edited)" if editor.status.get("dirty", false) else ""]


func _on_preference_changed(key: String) -> void:
	if key == "dominant_hand":
		_mark_hands()


## Colours the marker of the hand that draws.
func _mark_hands() -> void:
	for side in 2:
		_markers[side].drawing = side == drawing_side()


func _on_attention_changed(has_attention: bool) -> void:
	# The head is only known once the session has the user's attention, so a button
	# map that was shown before then is put in front of the eyes now.
	if has_attention and button_map.visible:
		_place_button_map()


## The user recentred the headset. That is the system's affair: it changes where the
## room's origin is, and the artwork stays where it was in the room. Going back to
## the initial spawn area is something the user asks for in Settings.
func _on_recentred() -> void:
	show_message("Headset recentred. The view was left as it is.")


# ---- starting and failing ----

func _fail(reason: String) -> void:
	failure = reason
	set_process(false)
	editor.message.emit(reason, true)
	start_failed.emit(reason)


## Puts one line of text at the bottom of the desktop window, for whoever looks on.
func _show_mirror_hint(text: String) -> void:
	var layer := CanvasLayer.new()
	layer.name = "MirrorHint"
	add_child(layer)
	var hint := Kit.label(text)
	hint.theme = Kit.make_theme()
	hint.add_theme_font_size_override("font_size", 13)
	hint.add_theme_stylebox_override("normal", Kit._box(Kit.BG_SUNKEN, 6, 1, Kit.BORDER, 8))
	hint.set_anchors_preset(Control.PRESET_CENTER_BOTTOM)
	hint.grow_horizontal = Control.GROW_DIRECTION_BOTH
	hint.grow_vertical = Control.GROW_DIRECTION_BEGIN
	hint.offset_bottom = -12
	hint.mouse_filter = Control.MOUSE_FILTER_IGNORE
	layer.add_child(hint)


func _recover() -> void:
	var title := str(Autosave.pending().get("title", "Untitled"))
	if autosave.recover():
		editor.message.emit("Recovered unsaved work on %s. Save it to keep it." % title, false)
