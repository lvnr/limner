class_name ColorField
extends ColorPickerButton
## A colour swatch that opens a picker.
##
## Everything picked while the picker is open is reported between `edit_started` and
## `edit_ended`, so the owner can make it one undo step. Colours here are the ones
## on screen; convert with `LimnerEditor.linear_rgb` before they go in a command.

signal changed(color: Color)
signal edit_started
signal edit_ended

var _editing := false


func _init(tooltip := "") -> void:
	tooltip_text = tooltip
	edit_alpha = false
	focus_mode = Control.FOCUS_NONE
	custom_minimum_size = Vector2(64, Kit.BUTTON)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	add_theme_stylebox_override("normal", Kit._box(Kit.BG_RAISED))
	add_theme_stylebox_override("hover", Kit._box(Kit.BG_HOVER))
	add_theme_stylebox_override("pressed", Kit._box(Kit.BG_HOVER))
	# Colours here are never see-through, so the chequered backing is left out; it
	# would show through a dimmed swatch.
	add_theme_icon_override("bg", ImageTexture.create_from_image(Image.create_empty(1, 1, false, Image.FORMAT_RGBA8)))
	color_changed.connect(_on_color_changed)
	popup_closed.connect(_on_popup_closed)


## Shows `value` without reporting it as a change. Ignored while the user is picking.
func set_value(value: Color) -> void:
	if not _editing:
		color = value


## A swatch that is not editable is dimmed and takes no clicks.
func set_editable(editable: bool) -> void:
	mouse_filter = Control.MOUSE_FILTER_STOP if editable else Control.MOUSE_FILTER_IGNORE
	modulate.a = 1.0 if editable else 0.45


func _on_color_changed(value: Color) -> void:
	if not _editing:
		_editing = true
		edit_started.emit()
	changed.emit(value)


func _on_popup_closed() -> void:
	if _editing:
		_editing = false
		edit_ended.emit()
