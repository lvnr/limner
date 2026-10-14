class_name LayerChoice
extends OptionButton
## A drop-down of the layers of some kinds, by name: the camera layers, say.
##
## It can start with an entry that stands for no layer, such as "Current view". The
## list follows the document; the choice is kept as long as its layer exists.

signal chosen(layer: int)

var editor: LimnerEditor
## The kinds of layer listed.
var kinds: Array = []

var _first := ""
var _layer := -1
## What the list was last built from, so that it is rebuilt only when that changes.
var _listed := []


## `first`, when given, is an entry before the layers that stands for none of them.
func _init(limner_editor: LimnerEditor, layer_kinds: Array, first := "", tooltip := "") -> void:
	editor = limner_editor
	kinds = layer_kinds
	_first = first
	tooltip_text = tooltip
	focus_mode = Control.FOCUS_NONE
	custom_minimum_size = Vector2(0, Kit.BUTTON)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	fit_to_longest_item = false
	clip_text = true
	item_selected.connect(_on_selected)


func _ready() -> void:
	editor.document_changed.connect(refresh)
	editor.document_replaced.connect(refresh)
	refresh()


## Lists other kinds of layer from now on.
func set_kinds(layer_kinds: Array) -> void:
	kinds = layer_kinds
	refresh()


## The layer chosen, or -1 for the first entry or when there is nothing to choose.
func get_layer() -> int:
	return _layer


## Shows `layer` as the choice, without reporting it as a change.
func set_layer(layer: int) -> void:
	_layer = layer
	refresh()


## The layers listed, in list order.
func layers() -> Array[int]:
	var ids: Array[int] = []
	for row: Dictionary in editor.get_rows():
		if row.kind in kinds:
			ids.append(int(row.id))
	return ids


## Brings the list in line with the document.
func refresh() -> void:
	var rows := editor.get_rows().filter(func(row: Dictionary) -> bool: return row.kind in kinds)
	var listing := rows.map(func(row: Dictionary) -> Array: return [row.id, row.name, row.kind])
	if listing != _listed or item_count == 0:
		_listed = listing
		clear()
		if _first != "":
			add_item(_first)
			set_item_metadata(0, -1)
		for row: Dictionary in rows:
			add_icon_item(Icons.texture(LayersPanel.KIND_ICONS.get(row.kind, "dot"), Kit.TEXT, 16), row.name)
			set_item_metadata(item_count - 1, int(row.id))
	var found := -1
	for index in item_count:
		if int(get_item_metadata(index)) == _layer:
			found = index
	if found < 0 and item_count > 0:
		found = 0
	_layer = int(get_item_metadata(found)) if found >= 0 else -1
	select(found)
	disabled = item_count == 0


func _on_selected(index: int) -> void:
	_layer = int(get_item_metadata(index))
	chosen.emit(_layer)
