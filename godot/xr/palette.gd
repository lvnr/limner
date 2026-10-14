class_name XRPalette
extends Node3D
## The palette held in the hand that does not draw: the same panels the desktop
## docks, one at a time, under a strip of tabs.
##
## The palette is held by its edge, like a clipboard: it sits beside the hand, toward
## the middle of the body, where the drawing hand can reach it. A panel that is
## picked up with the grip comes off the palette and stays where it is put down in
## the room ("pinned") until the button in its corner sends it back. When a text box
## in any panel is being typed into, a keyboard appears beside that panel.
##
## This node follows the hand. Pinned panels and the keyboard are its children too,
## but keep their own place in the room.

## The Settings panel's "Show button map" was pressed.
signal button_map_requested

## The panels, in the order of their tabs. A panel whose script is missing is left
## out, so the palette works while panels are still being written and gains the
## Timeline the day its script exists. `width` and `height` are in the panel's
## pixels; without a height the panel gets what it asks for.
const PANELS := [
	{"title": "Tools", "script": "res://ui/tools_panel.gd", "icon": "paint"},
	{"title": "Color", "script": "res://ui/color_panel.gd", "icon": "colorize"},
	{"title": "Layers", "script": "res://ui/layers_panel.gd", "icon": "stack", "height": 420},
	{"title": "Timeline", "script": "res://ui/timeline_panel.gd", "icon": "timeline", "width": 640},
	{"title": "Transform", "script": "res://ui/transform_panel.gd", "icon": "move"},
	{"title": "Layer", "script": "res://ui/properties_panel.gd", "icon": "paint_layer"},
	{"title": "Document", "script": "res://ui/document_panel.gd", "icon": "new_file"},
	{"title": "Export", "script": "res://ui/export_panel.gd", "icon": "export"},
	{"title": "Monitor", "script": "res://ui/monitor_panel.gd", "icon": "film"},
	{"title": "Performance", "script": "res://ui/performance_panel.gd", "icon": "chart"},
	{"title": "Settings", "script": "res://ui/settings_panel.gd", "icon": "gear"},
]
## Width of a panel on the palette, in the panel's pixels: a quarter of a metre.
const PANEL_WIDTH := 352
## Gap between the hand and the near edge of the palette, in metres.
const SIDE_GAP := 0.05
## How far above the hand the top of the palette is, in metres.
const TOP_ABOVE_HAND := 0.2
## How far in front of the hand the palette is, in metres.
const FORWARD := 0.03
## How far the palette leans back from upright so that it faces the eyes, in degrees.
## Tune on real hardware together with the three distances above.
const TILT_DEGREES := 25.0
## Gap between the strip of tabs and the panel under it, in metres.
const TAB_GAP := 0.004
## Gap between the top of the palette and the line of messages, in metres.
const MESSAGE_GAP := 0.012
## How long a message stays, in seconds, and how long one that reports a problem stays.
const MESSAGE_SECONDS := 2.5
const PROBLEM_SECONDS := 4.5
## Height of the message text in the room, in metres per pixel of a 28-pixel font.
const MESSAGE_PIXEL := 0.0005
## Gap between a panel and the keyboard beside it, in metres.
const KEYBOARD_GAP := 0.012
## How far the keyboard turns in toward the user from the plane of its panel, in degrees.
const KEYBOARD_TURN_DEGREES := 20.0
## Where the palette waits, in the room, until the hand that holds it is first seen.
const REST_POSITION := Vector3(-0.2, 1.1, -0.45)

var editor: LimnerEditor
## Whether the palette is out. Pinned panels show either way.
var shown := false
## Title of the panel on the palette now.
var current := ""
## Every panel's sheet, by title.
var surfaces := {}
## The strip of tabs.
var tabs: XRPanelSurface
## The keyboard's sheet. Visible only while a text box is being typed into.
var keyboard: XRPanelSurface

var _strip: ChoiceRow
var _keys: XRKeyboard
var _icons := {}
var _pinned: Array[String] = []
var _message: Label3D
var _message_time := 0.0
## 1 when the palette is held in the left hand and so lies to its right, -1 when mirrored.
var _side := 1.0
var _hand_seen := false


func _ready() -> void:
	_strip = ChoiceRow.new()
	for entry: Dictionary in PANELS:
		var panel := _make_panel(entry)
		if panel == null:
			continue
		var title: String = entry.title
		var surface := XRPanelSurface.new(title, panel, entry.get("width", PANEL_WIDTH), entry.get("height", 0))
		surface.visible = false
		surface.corner_pressed.connect(unpin.bind(title))
		add_child(surface)
		surfaces[title] = surface
		_icons[title] = entry.icon
		var tab := _strip.add_icon(title, entry.icon, title)
		tab.custom_minimum_size = Vector2(24, Kit.BUTTON)
		Kit.narrow(tab, 1)
		if panel.has_signal("button_map_requested"):
			panel.connect("button_map_requested", func() -> void: button_map_requested.emit())
		if current == "":
			current = title
	_strip.chosen.connect(_on_tab_chosen)
	tabs = XRPanelSurface.new("Tabs", _strip, PANEL_WIDTH, Kit.BUTTON, false)
	tabs.movable = false
	add_child(tabs)

	_keys = XRKeyboard.new()
	keyboard = XRPanelSurface.new("Keyboard", _keys, PANEL_WIDTH, 0, false)
	keyboard.movable = false
	keyboard.top_level = true
	keyboard.visible = false
	add_child(keyboard)
	_keys.done.connect(_stop_typing)

	_message = Label3D.new()
	_message.name = "Message"
	_message.font_size = 28
	_message.outline_size = 8
	_message.pixel_size = MESSAGE_PIXEL
	_message.width = PANEL_WIDTH / XRPanelSurface.PIXELS_PER_METRE / MESSAGE_PIXEL
	_message.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_message.vertical_alignment = VERTICAL_ALIGNMENT_BOTTOM
	_message.shaded = false
	_message.double_sided = true
	_message.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS_ANISOTROPIC
	_message.outline_modulate = Color(Kit.BG_SUNKEN, 1.0)
	_message.visible = false
	add_child(_message)

	editor.document_changed.connect(_wake_all)
	editor.state_changed.connect(_wake_all)
	editor.document_replaced.connect(_wake_all)
	Preferences.shared().changed.connect(_on_preference_changed)
	global_transform = Transform3D(Basis.IDENTITY, REST_POSITION)
	# From here on a sheet that changes size is put back in its place.
	tabs.resized.connect(_layout)
	for title: String in surfaces:
		var surface: XRPanelSurface = surfaces[title]
		surface.resized.connect(_layout)
	_layout()


func _process(delta: float) -> void:
	_place_keyboard()
	# While the piece plays, panels that show the time must keep up with it.
	if editor.status.get("playing", false):
		_wake_all()
	if _message.visible:
		_message_time -= delta
		_message.modulate.a = clampf(_message_time * 2.0, 0.0, 1.0)
		_message.outline_modulate.a = _message.modulate.a
		_message.visible = _message_time > 0.0


# ---- showing and hiding ----

func set_shown(on: bool) -> void:
	shown = on
	_layout()


func toggle() -> void:
	set_shown(not shown)


## Brings a panel to the front of the palette. A pinned panel stays where it is.
func show_panel(title: String) -> void:
	if not surfaces.has(title) or _pinned.has(title):
		return
	current = title
	_layout()


## Moves the palette with the hand that holds it. `hand` is that hand's aim pose in
## the room. While the hand is not tracked the palette stays where it last was.
func follow(hand: Transform3D, hand_is_left: bool, tracked: bool) -> void:
	var side := 1.0 if hand_is_left else -1.0
	if side != _side:
		_side = side
		_layout()
	if tracked:
		_hand_seen = true
		global_transform = hand.orthonormalized() * Transform3D(Basis(Vector3.RIGHT, -deg_to_rad(TILT_DEGREES)), Vector3(0.0, 0.0, -FORWARD))
	elif not _hand_seen:
		global_transform = Transform3D(Basis.IDENTITY, Vector3(REST_POSITION.x * _side, REST_POSITION.y, REST_POSITION.z))


## Every sheet the pointer can touch now.
func touchable() -> Array[XRPanelSurface]:
	var list: Array[XRPanelSurface] = []
	if keyboard.visible:
		list.append(keyboard)
	if shown:
		list.append(tabs)
		if surfaces.has(current) and not _pinned.has(current):
			list.append(surfaces[current])
	for title in _pinned:
		list.append(surfaces[title])
	return list


## Shows a line of text above the palette for a few seconds.
func show_message(text: String, is_problem := false) -> void:
	_message.text = text
	_message.modulate = Kit.DANGER.lightened(0.35) if is_problem else Kit.TEXT
	_message_time = PROBLEM_SECONDS if is_problem else MESSAGE_SECONDS
	_message.visible = true


## The message showing now, or an empty string.
func message() -> String:
	return _message.text if _message.visible else ""


# ---- pinning ----

func is_pinned(title: String) -> bool:
	return _pinned.has(title)


## Takes a panel off the palette and leaves it at `pose` in the room.
func pin(title: String, pose: Transform3D) -> void:
	if not surfaces.has(title):
		return
	_detach(surfaces[title])
	var surface: XRPanelSurface = surfaces[title]
	surface.place(pose)


## Sends a pinned panel back to the palette and brings it to the front.
func unpin(title: String) -> void:
	if not _pinned.has(title):
		return
	var surface: XRPanelSurface = surfaces[title]
	surface.pointer_leave()
	_pinned.erase(title)
	surface.top_level = false
	surface.magnification = 1.0
	surface.hide_corner_button()
	current = title
	_layout()


## The pointer picked a sheet up. One of the palette's panels comes off the palette.
func on_grabbed(surface: XRPanelSurface) -> void:
	if surfaces.values().has(surface):
		_detach(surface)


func _detach(surface: XRPanelSurface) -> void:
	if _pinned.has(surface.title):
		return
	_pinned.append(surface.title)
	surface.top_level = true
	surface.visible = true
	surface.show_corner_button("dock", "Send this panel back to the palette")
	if current == surface.title:
		current = ""
		for title: String in surfaces:
			if not _pinned.has(title):
				current = title
				break
	_layout()


# ---- inside ----

## Makes the panel an entry of `PANELS` names, or returns `null` when its script is
## not there or does not load.
func _make_panel(entry: Dictionary) -> Control:
	var path: String = entry.script
	if not ResourceLoader.exists(path):
		return null
	var script: GDScript = load(path)
	if script == null or not script.can_instantiate():
		push_warning("The %s panel does not load; the palette goes without it" % entry.title)
		return null
	var made: Object = script.new(editor)
	return made as Control


func _on_tab_chosen(title: String) -> void:
	if _pinned.has(title):
		unpin(title)
	else:
		show_panel(title)


## Puts the tabs and the panel in front where they belong on the palette, and shows
## which tab is which.
func _layout() -> void:
	var top := TOP_ABOVE_HAND
	var tab_size := tabs.metres()
	tabs.visible = shown
	tabs.position = Vector3(_side * (SIDE_GAP + tab_size.x * 0.5), top - tab_size.y * 0.5, 0.0)
	var below := top - tab_size.y - TAB_GAP
	for title: String in surfaces:
		if _pinned.has(title):
			continue
		var surface: XRPanelSurface = surfaces[title]
		var size := surface.metres()
		surface.visible = shown and title == current
		surface.transform = Transform3D(Basis.IDENTITY, Vector3(_side * (SIDE_GAP + size.x * 0.5), below - size.y * 0.5, 0.0))
	_message.position = Vector3(_side * (SIDE_GAP + tab_size.x * 0.5), top + MESSAGE_GAP, 0.0)
	_strip.set_value(current)
	for title: String in _strip.buttons:
		var tab: Button = _strip.buttons[title]
		var is_pinned_tab := _pinned.has(title)
		tab.icon = Icons.texture(_icons[title], Kit.ACCENT.lightened(0.35) if is_pinned_tab else Kit.TEXT, 20)
		tab.tooltip_text = "%s (pinned in the room; press to bring it back)" % title if is_pinned_tab else title
	tabs.wake()


## Shows the keyboard beside the panel that is being typed into, level with the text
## box, on the side away from the hand that holds the palette.
func _place_keyboard() -> void:
	var typing: XRPanelSurface = null
	for candidate in touchable():
		if candidate != keyboard and candidate.typing_control() != null:
			typing = candidate
			break
	if typing == null:
		if keyboard.visible:
			keyboard.pointer_leave()
			keyboard.visible = false
		_keys.target = null
		return
	var box := typing.typing_control()
	_keys.target = box.get_viewport()
	keyboard.visible = true
	var size := typing.metres() * typing.magnification
	var keys_size := keyboard.metres()
	var box_height := (typing.pixel_size.y * 0.5 - typing.pixel_of(box).y) / XRPanelSurface.PIXELS_PER_METRE * typing.magnification
	var reach := maxf(0.0, (size.y - keys_size.y) * 0.5)
	var hinge := Transform3D(
		Basis(Vector3.UP, -_side * deg_to_rad(KEYBOARD_TURN_DEGREES)),
		Vector3(_side * (size.x * 0.5 + KEYBOARD_GAP), clampf(box_height, -reach, reach), 0.0)
	)
	keyboard.place(typing.global_transform.orthonormalized() * hinge * Transform3D(Basis.IDENTITY, Vector3(_side * keys_size.x * 0.5, 0.0, 0.0)))


func _stop_typing() -> void:
	for surface in touchable():
		surface.stop_typing()


func _on_preference_changed(_key: String) -> void:
	_wake_all()


func _wake_all() -> void:
	for surface in touchable():
		surface.wake()
