class_name XRHandDial
extends Node3D
## The signs that ride on the hand holding the palette, as Quill's do.
##
## A see-through grey disc faces the eyes from just above the hand, with a dark ring
## and the frame number of the current layer in it (blank unless that is a paint
## layer), and over it a tag with the current layer's name. While the thumb rests on
## that hand's stick, or pushes it, icons around the ring say what the stick does:
## up a new frame, down hold the frame, left and right the frame before and after,
## a press play or stop. With the modifier held (that hand's trigger) they become copy
## the frame, delete it, back to the start and on to the end. The direction being
## pushed lights up. While the piece plays the middle shows the play mark.
##
## It only shows; the core decides what the stick does (`session/gesture.rs`).

## Size of the disc across, in metres.
const DIAMETER := 0.07
## How far above the hand the middle of the disc is, in metres, along the hand's up.
const LIFT := 0.014
## How far the icons are from the middle, and how large they are, in metres.
const ICON_RADIUS := 0.0275
const ICON_SIZE := 0.015
## Pixels of an icon's picture.
const ICON_PIXELS := 64
## Gap between the disc's top and the tag over it, in metres.
const TAG_GAP := 0.003
## Height of the tag's text in metres per pixel of its 28-pixel font, and of the number's.
const TAG_PIXEL := 0.00036
const NUMBER_PIXEL := 0.00042
## How far the stick must be pushed to count as touched on a controller that cannot
## tell a resting thumb, and to light up the direction it is pushed.
const STICK_SEEN := 0.15
const STICK_CHOSEN := 0.5
## How much larger a lit icon is drawn.
const LIT_SCALE := 1.35

## What each direction shows, without and with the modifier: icon and words.
const ACTIONS := {
	"up": [["tl_frame_blank", "New frame"], ["tl_frame_duplicate", "Copy frame"]],
	"down": [["tl_hold_after", "Hold frame"], ["tl_frame_delete", "Delete frame"]],
	"left": [["prev_frame", "Frame before"], ["rewind", "Back to the start"]],
	"right": [["next_frame", "Frame after"], ["forward", "On to the end"]],
}
## Where each direction's icon sits around the ring.
const PLACES := {
	"up": Vector2(0.0, 1.0),
	"down": Vector2(0.0, -1.0),
	"left": Vector2(-1.0, 0.0),
	"right": Vector2(1.0, 0.0),
}

## The current layer's name on the tag, and the number in the ring, as shown now.
var layer_name := ""
var frame_text := ""
## True while the stick's icons are shown.
var stick_shown := false
## The direction lit now: "up", "down", "left", "right" or "".
var lit := ""
## True while the modifier's icons are shown in place of the plain ones.
var modified := false

var _disc: MeshInstance3D
var _material: ShaderMaterial
var _number: Label3D
var _tag: Label3D
var _middle: Sprite3D
var _icons := {}
var _textures := {}
var _playing := false


func _ready() -> void:
	name = "HandDial"
	var quad := QuadMesh.new()
	quad.size = Vector2(DIAMETER, DIAMETER)
	_material = ShaderMaterial.new()
	_material.shader = preload("res://xr/dial.gdshader")
	_material.render_priority = -1
	_disc = MeshInstance3D.new()
	_disc.name = "Disc"
	_disc.mesh = quad
	_disc.material_override = _material
	_disc.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(_disc)
	_number = _label("Number", 28, NUMBER_PIXEL)
	_number.position = Vector3(0.0, 0.0, 0.0008)
	add_child(_number)
	_tag = _label("Tag", 28, TAG_PIXEL)
	_tag.outline_size = 14
	_tag.vertical_alignment = VERTICAL_ALIGNMENT_BOTTOM
	_tag.position = Vector3(0.0, DIAMETER * 0.5 + TAG_GAP, 0.0008)
	add_child(_tag)
	_middle = _sprite("Middle")
	_middle.position = Vector3(0.0, 0.0, 0.0004)
	add_child(_middle)
	for direction: String in PLACES:
		var icon := _sprite(direction.capitalize())
		var place: Vector2 = PLACES[direction]
		icon.position = Vector3(place.x * ICON_RADIUS, place.y * ICON_RADIUS, 0.0008)
		add_child(icon)
		_icons[direction] = icon
	Kit.events.scheme_changed.connect(_apply_colors)
	_apply_colors()


## Stands the disc over the hand, facing the eyes. `hand` is the hand's aim pose in
## the room; a hand that is not tracked hides it.
func follow(hand: Transform3D, eyes: Vector3, tracked: bool) -> void:
	visible = tracked
	if not tracked:
		return
	var at := hand.origin + hand.basis.y.normalized() * LIFT
	var away := at - eyes
	if away.length() < 0.01:
		return
	global_transform = Transform3D(Basis.looking_at(away.normalized(), Vector3.UP), at)


## What the stick shows: `stick` where it is, `touched` whether the thumb rests on it,
## `modifier` whether the modifier is held, `enabled` whether the stick works the
## frames at all (painting mode takes it away).
func show_stick(stick: Vector2, touched: bool, modifier: bool, enabled: bool) -> void:
	var shown := enabled and (touched or stick.length() > STICK_SEEN)
	var direction := ""
	if shown and stick.length() > STICK_CHOSEN:
		if absf(stick.x) > absf(stick.y):
			direction = "right" if stick.x > 0.0 else "left"
		else:
			direction = "up" if stick.y > 0.0 else "down"
	if shown == stick_shown and direction == lit and modifier == modified:
		return
	stick_shown = shown
	lit = direction
	modified = modifier
	_draw_stick()


func _draw_stick() -> void:
	for each: String in _icons:
		var icon: Sprite3D = _icons[each]
		icon.visible = stick_shown
		var pair: Array = ACTIONS[each]
		var action: Array = pair[1 if modified else 0]
		icon.texture = _texture(action[0], each == lit)
		icon.scale = Vector3.ONE * (LIT_SCALE if each == lit else 1.0)
	_show_middle()


## The current layer's name and the frame it shows, and whether the piece plays.
## `frame` is the number of the frame (from 1), or 0 to show none.
func show_layer(text: String, frame: int, playing: bool) -> void:
	layer_name = text
	frame_text = str(frame) if frame > 0 else ""
	_tag.text = text
	_tag.visible = text != ""
	_number.text = frame_text
	_playing = playing
	_show_middle()


## Words for what each direction does now, for tests and the button map.
func meaning(direction: String) -> String:
	var pair: Array = ACTIONS[direction]
	var action: Array = pair[1 if modified else 0]
	return action[1]


func _show_middle() -> void:
	# The middle shows the play mark while the piece plays, or what a press of the
	# stick would do while the thumb is on it; otherwise only the frame number.
	_middle.visible = _playing or stick_shown
	_middle.texture = _texture("pause" if _playing and stick_shown else "play", _playing)
	_middle.modulate.a = 0.35 if not _playing else 0.5
	_number.visible = frame_text != ""


func _apply_colors() -> void:
	_textures.clear()
	_material.set_shader_parameter("disc_color", Color(Kit.SCHEMES["light"].BG_SUNKEN, 0.5))
	_material.set_shader_parameter("ring_color", Color(0.06, 0.06, 0.06, 1.0))
	_number.modulate = Kit.ACCENT_TEXT.lightened(0.2)
	_number.outline_modulate = Color(0.06, 0.06, 0.06)
	_tag.modulate = Color(1.0, 1.0, 1.0)
	_tag.outline_modulate = Color(0.06, 0.06, 0.06)
	_draw_stick()


func _texture(icon: String, bright: bool) -> Texture2D:
	var key := "%s/%s" % [icon, bright]
	if not _textures.has(key):
		var color := Kit.PLAYHEAD if bright and (icon == "play" or icon == "pause") else (Kit.ACCENT.lightened(0.25) if bright else Kit.ACCENT)
		_textures[key] = Icons.texture(icon, color, ICON_PIXELS)
	return _textures[key]


func _label(label_name: String, font_size: int, pixel: float) -> Label3D:
	var label := Label3D.new()
	label.name = label_name
	label.font_size = font_size
	label.pixel_size = pixel
	label.outline_size = 8
	label.shaded = false
	label.double_sided = true
	label.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS_ANISOTROPIC
	return label


func _sprite(sprite_name: String) -> Sprite3D:
	var sprite := Sprite3D.new()
	sprite.name = sprite_name
	sprite.pixel_size = ICON_SIZE / ICON_PIXELS
	sprite.shaded = false
	sprite.double_sided = true
	sprite.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS_ANISOTROPIC
	return sprite
