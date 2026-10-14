class_name Autosave
extends Node
## Keeps a copy of unsaved work and offers it back when the app did not close properly.
##
## Every few minutes (the `autosave_minutes` preference; 0 switches it off) a document
## with unsaved changes is copied to a recovery file, at a moment when nothing is
## being drawn or played. The copy is removed as soon as the work is safe: after a
## save, after starting or opening another document, and on quitting. If it is still
## there when the app starts, the last session ended badly and the copy is offered.

const FOLDER := "user://recovery"
const DOCUMENT := "user://recovery/unsaved.limnz"
const NOTE := "user://recovery/unsaved.json"

var editor: LimnerEditor

var _since_last := 0.0
## False while a copy from last time is waiting to be recovered, discarded or set
## aside: until then it must not be written over or cleared away.
var _settled := true


func _ready() -> void:
	_settled = pending().is_empty()
	editor.document_replaced.connect(_on_change)
	editor.state_changed.connect(_on_change)


## The work is safe as soon as nothing is unsaved: after a save, and in a new or
## newly opened document.
func _on_change() -> void:
	if _settled and not editor.scene.status().dirty:
		_since_last = 0.0
		clear()


func _process(delta: float) -> void:
	var minutes := Preferences.shared().number("autosave_minutes")
	if minutes <= 0.0 or not _settled:
		return
	_since_last += delta
	if _since_last < minutes * 60.0:
		return
	var status := editor.status
	if not status.get("dirty", false):
		_since_last = 0.0
		return
	# Wait for a quiet moment: a save in the middle of a stroke would be felt.
	if status.get("tool_active", false) or status.get("grabbing", false) or status.get("playing", false):
		return
	save_now()


## Writes the recovery copy now. Returns whether it worked.
func save_now() -> bool:
	_since_last = 0.0
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(FOLDER))
	var result := editor.scene.save_copy(ProjectSettings.globalize_path(DOCUMENT))
	if not result.ok:
		push_warning("Could not write the recovery copy: %s" % result.error)
		return false
	var note := FileAccess.open(NOTE, FileAccess.WRITE)
	if note == null:
		return false
	note.store_string(JSON.stringify({
		"path": editor.path,
		"native": editor.path_is_native,
		"title": editor.title(),
		"time": Time.get_datetime_string_from_system(false, true),
	}))
	return true


## Removes the recovery copy: the work is safe, or was given up on purpose.
static func clear() -> void:
	for file: String in [DOCUMENT, NOTE]:
		if FileAccess.file_exists(file):
			DirAccess.remove_absolute(ProjectSettings.globalize_path(file))


## What there is to recover: `{path, native, title, time}`, or an empty dictionary.
static func pending() -> Dictionary:
	if not FileAccess.file_exists(DOCUMENT) or not FileAccess.file_exists(NOTE):
		return {}
	var note: Variant = JSON.parse_string(FileAccess.get_file_as_string(NOTE))
	if note is Dictionary and (note as Dictionary).has("title"):
		return note
	return {"path": "", "native": false, "title": "Untitled", "time": ""}


## Gives up the copy from last time.
func discard() -> void:
	clear()
	_settled = true


## Puts the copy from last time aside under a name of its own, where File > Open can
## still reach it, and returns where. For when the user neither wants it now nor
## wants it gone.
func set_aside() -> String:
	var kept := "%s/kept %s.limnz" % [FOLDER, Time.get_datetime_string_from_system(false, true).replace(":", "-")]
	var target := ProjectSettings.globalize_path(kept)
	if DirAccess.rename_absolute(ProjectSettings.globalize_path(DOCUMENT), target) != OK:
		return ""
	clear()
	_settled = true
	return target


## Opens the recovery copy as the document it was a copy of, with its changes
## counted as unsaved. Returns whether it worked.
func recover() -> bool:
	var note := pending()
	if note.is_empty():
		return false
	_settled = true
	if not editor.open_document(ProjectSettings.globalize_path(DOCUMENT)):
		_settled = false
		return false
	# It is the unsaved state of that document, not a document of its own.
	editor.path = str(note.get("path", ""))
	editor.path_is_native = bool(note.get("native", false)) and editor.path != ""
	editor.scene.mark_edited()
	return true
