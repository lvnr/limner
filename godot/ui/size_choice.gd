class_name SizeChoice
extends VBoxContainer
## A choice of picture size: the document's camera resolution, one of the usual
## sizes, or a width and a height typed in.

signal changed

var editor: LimnerEditor

var _menu: Cycler
var _custom: HBoxContainer
var _width: NumberField
var _height: NumberField


func _init(limner_editor: LimnerEditor, largest: int, tooltip: String) -> void:
	editor = limner_editor
	var sizes: Array = DocumentPanel.RESOLUTIONS.map(func(size: Array) -> String: return "%d × %d" % size)
	_menu = Kit.choice(["Document"] + sizes + ["Custom…"], tooltip, _on_chosen)
	add_child(_menu)
	_width = NumberField.new("Width in pixels, 16 to %d" % largest, 16, largest, true)
	_height = NumberField.new("Height in pixels, 16 to %d" % largest, 16, largest, true)
	_width.set_value(1920)
	_height.set_value(1080)
	_width.committed.connect(func(_value: float) -> void: changed.emit())
	_height.committed.connect(func(_value: float) -> void: changed.emit())
	_custom = Kit.row([_width, Kit.label("×", true), _height, Kit.label("pixels", true)])
	_custom.visible = false
	add_child(_custom)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL


func _ready() -> void:
	editor.document_changed.connect(_show_document_size)
	editor.document_replaced.connect(_show_document_size)
	_show_document_size()


## The size chosen, in pixels.
func get_size_chosen() -> Vector2i:
	var index := _menu.selected
	if index == 0:
		var resolution: Array = editor.get_info().camera_resolution
		return Vector2i(int(resolution[0]), int(resolution[1]))
	if index <= DocumentPanel.RESOLUTIONS.size():
		var listed: Array = DocumentPanel.RESOLUTIONS[index - 1]
		return Vector2i(listed[0], listed[1])
	return Vector2i(int(_width.get_value()), int(_height.get_value()))


## Chooses a typed size, as a user would by picking Custom and filling the boxes.
func set_custom(size: Vector2i) -> void:
	_menu.select(_menu.item_count - 1)
	_custom.visible = true
	_width.set_value(clampi(size.x, int(_width.minimum), int(_width.maximum)))
	_height.set_value(clampi(size.y, int(_height.minimum), int(_height.maximum)))
	changed.emit()


func _show_document_size() -> void:
	var resolution: Array = editor.get_info().camera_resolution
	_menu.set_item_text(0, "%d × %d (document)" % [int(resolution[0]), int(resolution[1])])
	if _menu.selected == 0:
		changed.emit()


func _on_chosen(index: int) -> void:
	_custom.visible = index == _menu.item_count - 1
	changed.emit()
