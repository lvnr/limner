class_name XRPalette
extends Node3D
## The palette held in the hand that does not draw: the same panels the desktop
## docks, one at a time, over a cluster of round buttons that choose among them.
##
## As in Quill, the palette rides above the hand: the round buttons just above it and
## the panel above them, leaning back toward the eyes. A panel's pin (the key at the
## right of its title bar) leaves it where it is in the room, and so does picking it
## up with the grip and putting it down somewhere; the same key, or its round
## button, sends it back. When a text box in any panel is being typed into, a
## keyboard appears beside that panel.
##
## This node follows the hand. Pinned panels and the keyboard are its children too,
## but keep their own place in the room.

## The Settings panel's "Show button map" was pressed.
signal button_map_requested

## The panels. A panel whose script is missing is left out, so the palette works
## while panels are still being written. `width` and `height` are in the panel's
## pixels; without a height the panel gets what it asks for. `row` says where its
## round button is, as in Quill: 0 the lower row (the panels used while painting), 1
## the upper. A panel with `coin` false has none: it is shown from a key of another
## panel (the Layer panel from a layer's kind key, Text from the Tools panel).
const PANELS := [
	{"title": "Tools", "script": "res://ui/tools_panel.gd", "icon": "paint", "row": 0},
	{"title": "Color", "script": "res://ui/color_panel.gd", "icon": "colorize", "row": 0},
	{"title": "Layers", "script": "res://ui/timeline_panel.gd", "icon": "layers", "row": 0},
	{"title": "Transform", "script": "res://ui/transform_panel.gd", "icon": "move", "row": 0},
	{"title": "Monitor", "script": "res://ui/monitor_panel.gd", "icon": "camera", "row": 0},
	{"title": "Document", "script": "res://ui/document_panel.gd", "icon": "document", "row": 1},
	{"title": "Settings", "script": "res://ui/settings_panel.gd", "icon": "settings", "row": 1},
	{"title": "Performance", "script": "res://ui/performance_panel.gd", "icon": "performance", "row": 1},
	{"title": "Export", "script": "res://ui/export_panel.gd", "icon": "export", "row": 1},
	{"title": "Layer", "script": "res://ui/properties_panel.gd", "icon": "paint_layer", "coin": false},
	{"title": "Text", "script": "res://ui/text_panel.gd", "icon": "text", "coin": false},
]
## The least width of a sheet on the palette, in the panel's pixels (31 cm); a panel
## that is wider makes its sheet wider.
const PANEL_WIDTH := 384
## Size of a round button and the gap between two, in pixels (40 and 7 mm).
const COIN := 44
const COIN_GAP := 8
## Size of the icon on a round button, in pixels.
const COIN_ICON := 26
## How far above the hand the middle of the round buttons is, in metres: clear of
## the disc and the layer's tag that ride on the hand (`XRHandDial`).
const COINS_ABOVE_HAND := 0.105
## How far the palette sits toward the middle of the body from the hand, in metres.
const TOWARD_MIDDLE := 0.03
## How far in front of the hand the palette is, in metres.
const FORWARD := 0.02
## How far the palette leans back from upright so that it faces the eyes, in degrees,
## and how far it turns in toward the middle of the body, which is where the eyes
## are from a hand held to one side. Tune on real hardware with the distances above.
const TILT_DEGREES := 20.0
const TURN_IN_DEGREES := 18.0
## Gap between the round buttons and the panel over them, in metres.
const TAB_GAP := 0.012
## Height of the tag that names the round button under the tip, in metres per pixel
## of its 26-pixel font.
const TAG_PIXEL := 0.0005
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
## The round buttons, on a sheet of their own that shows only its keys.
var tabs: XRPanelSurface
## The round buttons by the title of their panel.
var coins := {}
## The keyboard's sheet. Visible only while a text box is being typed into.
var keyboard: XRPanelSurface

var _keys: XRKeyboard
var _icons := {}
var _pinned: Array[String] = []
var _message: Label3D
var _tag: Label3D
var _message_time := 0.0
## 1 when the palette is held in the left hand and so lies to its right, -1 when mirrored.
var _side := 1.0
var _hand_seen := false


func _ready() -> void:
	var rows: Array[Array] = [[], []]
	for entry: Dictionary in PANELS:
		var panel := _make_panel(entry)
		if panel == null:
			continue
		var title: String = entry.title
		var surface := XRPanelSurface.new(title, panel, entry.get("width", PANEL_WIDTH), entry.get("height", 0))
		surface.visible = false
		surface.corner_pressed.connect(_on_corner_pressed.bind(title))
		add_child(surface)
		surfaces[title] = surface
		_icons[title] = entry.icon
		if entry.get("coin", true):
			var coin := _make_coin(title, entry.icon)
			coins[title] = coin
			rows[int(entry.get("row", 0))].append(coin)
		if panel.has_signal("button_map_requested"):
			panel.connect("button_map_requested", func() -> void: button_map_requested.emit())
		if panel.has_signal("text_requested"):
			panel.connect("text_requested", func() -> void: show_panel("Text"))
		if panel.has_signal("settings_requested"):
			panel.connect("settings_requested", func(_layer: int) -> void: show_panel("Layer"))
		if panel.has_signal("import_requested"):
			panel.connect("import_requested", _show_import)
		if current == "":
			current = title
	var cluster := _make_cluster(rows)
	tabs = XRPanelSurface.new("Tabs", cluster, int(cluster.custom_minimum_size.x), int(cluster.custom_minimum_size.y), false)
	tabs.movable = false
	tabs.show_sheet = false
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
	_tag = _make_tag()
	add_child(_tag)
	Kit.events.scheme_changed.connect(_on_scheme_changed)

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
	_follow_hovered_coin()
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
		var lean := Basis(Vector3.UP, _side * deg_to_rad(TURN_IN_DEGREES)) * Basis(Vector3.RIGHT, -deg_to_rad(TILT_DEGREES))
		global_transform = hand.orthonormalized() * Transform3D(lean, Vector3(0.0, 0.0, -FORWARD))
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
	_message.modulate = Kit.DANGER_TEXT if is_problem else Kit.TEXT
	_message.outline_modulate = Color(Kit.BG_SUNKEN, _message.outline_modulate.a)
	_message.set_meta("problem", is_problem)
	_message_time = PROBLEM_SECONDS if is_problem else MESSAGE_SECONDS
	_message.visible = true


## The colour scheme changed: the message above the palette and the round buttons
## take the new colours. The panels re-theme themselves.
func _on_scheme_changed() -> void:
	var alpha := _message.modulate.a
	_message.modulate = Color(Kit.DANGER_TEXT if _message.get_meta("problem", false) else Kit.TEXT, alpha)
	_message.outline_modulate = Color(Kit.BG_SUNKEN, _message.outline_modulate.a)
	_tag.modulate = Kit.TEXT_ON_ACCENT
	_tag.outline_modulate = Kit.TEXT
	for title: String in coins:
		_style_coin(coins[title])
	_layout()


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
	current = title
	_layout()


## The key at the right of a panel's title bar: a panel on the palette is left where
## it is in the room; one left in the room comes back.
func _on_corner_pressed(title: String) -> void:
	if _pinned.has(title):
		unpin(title)
	else:
		var surface: XRPanelSurface = surfaces[title]
		pin(title, surface.global_transform)


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
	if current == surface.title:
		current = ""
		for title: String in surfaces:
			if not _pinned.has(title):
				current = title
				break
	_layout()


# ---- inside ----

## Shows the Document panel at the list of files that can be imported.
func _show_import() -> void:
	if not surfaces.has("Document"):
		return
	show_panel("Document")
	var document: Object = (surfaces["Document"] as XRPanelSurface).content
	if document.has_method("show_file_part"):
		document.call("show_file_part", "import")


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


## Puts the round buttons and the panel in front where they belong on the palette,
## and shows which panel is in front and which are pinned.
func _layout() -> void:
	var middle := _side * TOWARD_MIDDLE
	var tab_size := tabs.metres()
	tabs.visible = shown
	tabs.position = Vector3(middle, COINS_ABOVE_HAND, 0.0)
	var base := COINS_ABOVE_HAND + tab_size.y * 0.5 + TAB_GAP
	var top := base
	for title: String in surfaces:
		var surface: XRPanelSurface = surfaces[title]
		var pinned := _pinned.has(title)
		surface.show_corner_button("dock" if pinned else "pin", "Send this panel back to the palette" if pinned else "Leave this panel here in the room")
		if pinned:
			continue
		var size := surface.metres()
		surface.visible = shown and title == current
		surface.transform = Transform3D(Basis.IDENTITY, Vector3(middle, base + size.y * 0.5, 0.0))
		if surface.visible:
			top = base + size.y
	_message.position = Vector3(middle, top + MESSAGE_GAP, 0.0)
	_tag.position = Vector3(middle, COINS_ABOVE_HAND + tab_size.y * 0.5 + 0.002, 0.006)
	for title: String in coins:
		var coin: Button = coins[title]
		var is_pinned_coin := _pinned.has(title)
		coin.set_pressed_no_signal(title == current and not is_pinned_coin)
		coin.icon = Icons.texture(_icons[title], Kit.ACCENT_TEXT if is_pinned_coin else Kit.TEXT, COIN_ICON)
		coin.tooltip_text = "%s (left in the room; press to bring it back)" % title if is_pinned_coin else title
	tabs.wake()


# ---- the round buttons ----

func _make_coin(title: String, icon: String) -> Button:
	var coin := Button.new()
	coin.name = title.validate_node_name()
	coin.toggle_mode = true
	coin.focus_mode = Control.FOCUS_NONE
	coin.custom_minimum_size = Vector2(COIN, COIN)
	coin.size = Vector2(COIN, COIN)
	coin.icon = Icons.texture(icon, Kit.TEXT, COIN_ICON)
	coin.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	coin.expand_icon = false
	coin.tooltip_text = title
	coin.set_meta("key_shape", "round")
	_style_coin(coin)
	coin.pressed.connect(_on_tab_chosen.bind(title))
	return coin


## Round looks for a round button, in the colours of the scheme in use.
func _style_coin(coin: Button) -> void:
	var looks := {
		"normal": Kit.BG_RAISED,
		"hover": Kit.BG_HOVER,
		"pressed": Kit.PRESSED.lerp(Kit.ACCENT, 0.35),
		"hover_pressed": Kit.PRESSED_HOVER.lerp(Kit.ACCENT, 0.35),
		"disabled": Kit.BG_DISABLED,
	}
	for state: String in looks:
		coin.add_theme_stylebox_override(state, Kit._box(looks[state], COIN / 2, 2, Color(Kit.TEXT, 0.85), 0))
	coin.add_theme_stylebox_override("focus", StyleBoxEmpty.new())


## The round buttons in two rows, the upper one set half a button along, as Quill
## lays them out. Placed by hand: containers cannot stagger.
func _make_cluster(rows: Array[Array]) -> Control:
	var widest := 0
	for row: Array in rows:
		widest = maxi(widest, row.size())
	var step := COIN + COIN_GAP
	var cluster := Control.new()
	cluster.name = "Coins"
	cluster.custom_minimum_size = Vector2(widest * step - COIN_GAP, COIN * 2 + COIN_GAP)
	for index in rows.size():
		var row: Array = rows[index]
		# The lower row is the first; the upper one is centred over it.
		var y := COIN + COIN_GAP if index == 0 else 0
		var x0 := (widest - row.size()) * step * 0.5
		for at in row.size():
			var coin: Button = row[at]
			coin.position = Vector2(x0 + at * step, y)
			cluster.add_child(coin)
	return cluster


func _make_tag() -> Label3D:
	var tag := Label3D.new()
	tag.name = "Tag"
	tag.font_size = 26
	tag.outline_size = 12
	tag.pixel_size = TAG_PIXEL
	tag.vertical_alignment = VERTICAL_ALIGNMENT_BOTTOM
	tag.shaded = false
	tag.double_sided = true
	tag.no_depth_test = false
	tag.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS_ANISOTROPIC
	tag.modulate = Kit.TEXT_ON_ACCENT
	tag.outline_modulate = Kit.TEXT
	tag.visible = false
	return tag


## Names the round button under the tip in the tag over the cluster.
func _follow_hovered_coin() -> void:
	var hovered: Control = tabs.viewport.gui_get_hovered_control() if tabs.is_visible_in_tree() else null
	var coin := hovered as Button
	_tag.visible = coin != null and coins.values().has(coin)
	if _tag.visible:
		_tag.text = coin.tooltip_text.get_slice(" (", 0)


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
