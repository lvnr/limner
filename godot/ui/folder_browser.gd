class_name FolderBrowser
extends VBoxContainer
## A folder's contents a page at a time, to choose a file, or a folder, in a panel:
## there is no system file dialog in a headset, and nothing on a panel may scroll.
##
## Over the list, a key to the folder above, the usual places (`Documents.places`)
## on a cycler, and the folder shown. Then the folders in it, which open when
## pressed, and the files that `files` gives, one key each, with page keys under
## them. With no `files` it chooses a folder: a key under the list takes the one shown.

## A file was chosen, or, when choosing a folder, the folder shown was taken.
signal chosen(path: String)
## Another folder is shown.
signal folder_changed(folder: String)

## Icons of the entries, by their `kind`.
const ICONS := {"folder": "folder", "limnz": "new_file", "limn": "folder_open", "quill": "folder_open", "picture": "image", "sound": "sound", "model": "model"}

## The folder shown.
var folder := ""
## Gives the files of a folder that can be chosen, as `{path, name, time, kind}`;
## not valid when a folder is being chosen.
var files := Callable()
## What the list says when a folder has nothing to choose; `%s` is the folder.
var empty_text := "Nothing here."

var _up: Button
var _places: Cycler
var _where: Label
var _rows: VBoxContainer
var _pager: Pager
var _entries: Array[Dictionary] = []
var _per_page := 6


## `start` is the folder shown first, `per_page` how many entries a page has, and
## `file_lister` what lists the files (none to choose a folder). `take_text`, when a
## folder is chosen, is the words on the key that takes it.
func _init(start: String, per_page: int, file_lister := Callable(), take_text := "Use this folder") -> void:
	name = "FolderBrowser"
	files = file_lister
	_per_page = per_page
	add_theme_constant_override("separation", Kit.u(4))
	_up = Kit.key("up", "Up: the folder this one is in", func() -> void: show_folder(Documents.parent_of(folder)))
	_places = Cycler.new("The usual places to look: the Limner documents folder, your own folders and the drives")
	_places.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_places.chosen.connect(func(path: String) -> void:
		if path != folder:
			show_folder(path)
	)
	add_child(Kit.row([_up, _places]))
	_where = Kit.label("", true)
	_where.clip_text = true
	_where.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	_where.custom_minimum_size.x = Kit.u(40)
	_where.mouse_filter = Control.MOUSE_FILTER_PASS
	add_child(_where)
	_rows = Kit.column()
	add_child(_rows)
	_pager = Pager.new(per_page, false)
	_pager.page_changed.connect(func(_first: int) -> void: _show_page())
	add_child(_pager)
	if not files.is_valid():
		var take := Kit.text_key(take_text, "Takes the folder shown", func() -> void: chosen.emit(folder), "check")
		take.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		add_child(take)
	show_folder(start)


## Shows `path`: its folders, then its files, from the first page.
func show_folder(path: String) -> void:
	folder = path
	_entries = Documents.folders(path)
	if files.is_valid():
		var listed: Array[Dictionary] = files.call(path)
		_entries.append_array(listed)
	var choices: Array = []
	var known := false
	for place: Array in Documents.places():
		choices.append(place)
		known = known or place[0] == path
	if not known:
		choices.push_front([path, path.get_file() if path.get_file() != "" else path])
	_places.set_choices(choices)
	_places.set_value(path)
	_up.disabled = Documents.parent_of(path) == path
	var empty := empty_text % _short(path) if empty_text.contains("%s") else empty_text
	_where.text = empty if _entries.is_empty() else _short(path)
	# A folder's name stays on one line; what to do in an empty one is read whole.
	var wrap := _entries.is_empty()
	_where.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART if wrap else TextServer.AUTOWRAP_OFF
	_where.text_overrun_behavior = TextServer.OVERRUN_NO_TRIMMING if wrap else TextServer.OVERRUN_TRIM_ELLIPSIS
	_where.clip_text = not wrap
	_where.tooltip_text = path
	_pager.set_count(_entries.size())
	_pager.show_item(0)
	_pager.visible = _entries.size() > _per_page
	_show_page()
	folder_changed.emit(path)


## The entries listed for the folder shown, folders first.
func entries() -> Array[Dictionary]:
	return _entries


## The last two folders of a path: enough to recognise, short enough for a panel.
static func _short(path: String) -> String:
	var above := path.get_base_dir().get_file()
	return above.path_join(path.get_file()) if above != "" and path.get_file() != "" else path


func _show_page() -> void:
	for child: Node in _rows.get_children():
		_rows.remove_child(child)
		child.queue_free()
	for entry: Dictionary in _entries.slice(_pager.first, _pager.first + _per_page):
		var path: String = entry.path
		var label := str(entry.name)
		if entry.kind == "quill":
			label += " (Quill)"
		var is_folder: bool = entry.kind == "folder"
		var press := show_folder.bind(path) if is_folder else (func() -> void: chosen.emit(path))
		var button := Kit.text_key(label, path, press, ICONS.get(entry.kind, "new_file"))
		button.alignment = HORIZONTAL_ALIGNMENT_LEFT
		button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		button.custom_minimum_size.x = Kit.u(40)
		button.clip_text = true
		var when := Kit.label(Documents.local_time(int(entry.time)), true)
		when.add_theme_font_size_override("font_size", Kit.text_size("small"))
		_rows.add_child(Kit.row([button, when]))
