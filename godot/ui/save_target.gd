class_name SaveTarget
extends VBoxContainer
## Where a file is going to be written: a name and a folder.
##
## Both have a default, so that a file can be written without any dialog, as it has
## to be in a headset: the name is the document's title with the date and time, and
## the folder is one named Limner among the user's own. On the desktop a button
## opens the system's save dialog to choose both.

var editor: LimnerEditor
## The folder files go to.
var folder := ""

var _name: TextField
var _folder: Label
var _browse: Button
var _dialog: FileDialog
var _extension := ""


## `default_folder` is where files go until another folder is chosen.
func _init(limner_editor: LimnerEditor, default_folder: String) -> void:
	editor = limner_editor
	folder = default_folder
	_name = TextField.new("Title, date and time", "The name of the file, without its ending. Left empty, the file is named after the document and the time.")
	add_child(Form.row("File name", [_name]))
	_folder = Form.value()
	_browse = Kit.icon_button("folder_open", "Choose the folder and the name in a save dialog", _on_browse)
	add_child(Form.row("Folder", [_folder, _browse]))
	_show_folder()


func _ready() -> void:
	# A headset has no system dialogs; there the default folder is what there is.
	# On the headset's palette the viewport is the panel's own, so the window is asked:
	# a system file dialog is no use to someone who cannot see the desktop.
	_browse.visible = not get_tree().root.use_xr


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
