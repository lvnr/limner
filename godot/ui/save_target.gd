class_name SaveTarget
extends VBoxContainer
## Where a file is going to be written: a name and a folder.
##
## Both have a default, so that a file can be written without any dialog, as it has
## to be in a headset: the name is the document's title with the date and time, and
## the folder is one named Limner among the user's own. On the desktop a button
## opens the system's save dialog to choose both; in a headset, which has none, the
## folder is chosen among the usual places on a cycler (`Documents.places`).

var editor: LimnerEditor
## The folder files go to.
var folder := ""

var _name: TextField
var _folder: Label
var _browse: Button
## In a headset: the default folder and the usual places, to choose among.
var _places: Cycler
var _default := ""
var _dialog: FileDialog
var _extension := ""


## `default_folder` is where files go until another folder is chosen.
func _init(limner_editor: LimnerEditor, default_folder: String) -> void:
	editor = limner_editor
	folder = default_folder
	_default = default_folder
	_name = TextField.new("Title, date and time", "The name of the file, without its ending. Left empty, the file is named after the document and the time.")
	add_child(Form.row("File name", [_name]))
	_folder = Form.value()
	_browse = Kit.icon_button("folder_open", "Choose the folder and the name in a save dialog", _on_browse)
	_places = Cycler.new("The folder the file goes to: the usual one for this kind of file, or one of your own folders")
	_places.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_places.chosen.connect(func(path: String) -> void:
		folder = path
		_show_folder()
	)
	add_child(Form.row("Folder", [_folder, _browse, _places]))
	var choices: Array = [[_default, "%s (usual)" % _default.get_base_dir().get_file().path_join(_default.get_file())]]
	for place: Array in Documents.places():
		if place[0] != _default:
			choices.append(place)
	_places.set_choices(choices)
	_places.set_value(_default)
	_folder.visible = not Kit.headset
	_browse.visible = not Kit.headset
	_places.visible = Kit.headset
	_show_folder()


## The file to write, ending in `extension`. Without a typed name, each call gives
## a new name with the time in it.
func path(extension: String) -> String:
	var typed := _name.text.strip_edges().validate_filename()
	if typed == "":
		return folder.path_join(LimnerCapture.stamped_name(editor.title(), extension))
	if typed.get_extension().to_lower() == extension:
		typed = typed.get_basename()
	return folder.path_join("%s.%s" % [typed, extension])


## The ending the save dialog offers, for the kind of file being written now.
func set_extension(extension: String) -> void:
	_extension = extension


## Sets the name and the folder, as choosing them in the save dialog does.
func set_path(chosen: String) -> void:
	folder = chosen.get_base_dir()
	_name.set_value(chosen.get_file().get_basename())
	_show_folder()


func _show_folder() -> void:
	# The last two folders say enough; the tooltip has the whole path.
	_folder.text = folder.get_base_dir().get_file().path_join(folder.get_file())
	_folder.tooltip_text = folder
	_places.tooltip_text = folder


func _on_browse() -> void:
	if _dialog == null:
		_dialog = FileDialog.new()
		_dialog.access = FileDialog.ACCESS_FILESYSTEM
		_dialog.file_mode = FileDialog.FILE_MODE_SAVE_FILE
		_dialog.use_native_dialog = true
		_dialog.file_selected.connect(set_path)
		add_child(_dialog)
	DirAccess.make_dir_recursive_absolute(folder)
	_dialog.title = "Choose where to save"
	_dialog.filters = PackedStringArray(["*.%s" % _extension]) if _extension != "" else PackedStringArray()
	_dialog.current_dir = folder
	_dialog.current_file = path(_extension).get_file()
	_dialog.popup_centered_ratio(0.7)
