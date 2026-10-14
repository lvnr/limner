class_name LimnerEditor
extends Node
## The interface's handle on the editing session.
##
## Panels and shells never talk to the document directly. They send commands through
## `run`, read the values cached here, and redraw when a signal says something changed.

signal document_changed
signal state_changed
signal document_replaced
## Something the user should read: the reason a command failed, or a confirmation.
signal message(text: String, is_error: bool)

const FLICKS_PER_SECOND := 705_600_000

var scene: LimnerScene
## Revisions, undo state, playhead and current tool. Refreshed every frame.
var status: Dictionary = {}
var info: Dictionary = {}
var tools: Dictionary = {}
var options: Dictionary = {}
## One dictionary per layer, in list order.
var rows: Array = []
## Where the open document was loaded from or last saved to. Empty for a new document.
var path := ""
## True when `path` is a file Limner can save back to (not an imported Quill project).
var path_is_native := false

var _rows_stale := true
var _info_stale := true
var _state_stale := true


func setup(limner_scene: LimnerScene) -> void:
	scene = limner_scene
	scene.document_changed.connect(_on_document_changed)
	scene.state_changed.connect(_on_state_changed)
	scene.document_replaced.connect(_on_document_replaced)
	status = scene.status()
	refresh()


func _process(_delta: float) -> void:
	status = scene.status()


## Runs a command. Returns its result (`true` when it has none), or `null` after
## reporting the failure through `message`.
func run(command: Dictionary) -> Variant:
	var reply := scene.exec(command)
	if not reply.ok:
		message.emit(reply.error, true)
		return null
	return true if reply.result == null else reply.result


## Runs a command and returns whether it worked, without reporting a failure.
func try_run(command: Dictionary) -> bool:
	return scene.exec(command).ok


## Starts an edit made of many small commands (a slider drag). Finish with `commit`.
func begin(label: String) -> void:
	scene.begin_edit(label)


func commit() -> void:
	scene.commit_edit()


func cancel() -> void:
	scene.cancel_edit()


func undo() -> void:
	var result = run({"cmd": "undo"})
	if result is Dictionary and result.undone != null:
		message.emit("Undid: %s" % result.undone, false)


func redo() -> void:
	var result = run({"cmd": "redo"})
	if result is Dictionary and result.redone != null:
		message.emit("Redid: %s" % result.redone, false)


## Reads everything again. Panels call the `get_*` accessors, which do this lazily.
func refresh() -> void:
	_rows_stale = true
	_info_stale = true
	_state_stale = true


func get_rows() -> Array:
	if _rows_stale:
		rows = scene.layer_rows()
		_rows_stale = false
	return rows


func get_info() -> Dictionary:
	if _info_stale:
		info = scene.document_info()
		_info_stale = false
	return info


func get_tools() -> Dictionary:
	_refresh_state()
	return tools


func get_options() -> Dictionary:
	_refresh_state()
	return options


func _refresh_state() -> void:
	if _state_stale:
		tools = scene.tool_settings()
		options = scene.options()
		_state_stale = false


## The row of the layer with this id, or an empty dictionary.
func row(id: int) -> Dictionary:
	for candidate: Dictionary in get_rows():
		if candidate.id == id:
			return candidate
	return {}


func current_layer() -> int:
	var id = status.get("current_layer")
	return -1 if id == null else int(id)


func set_option(key: String, value: Variant) -> void:
	run({"cmd": "set_options", "values": {key: value}})


func set_tool_setting(tool: String, key: String, value: Variant) -> void:
	run({"cmd": "set_tool_settings", "values": {tool: {key: value}}})


# ---- documents ----

func new_document() -> void:
	scene.new_document()
	path = ""
	path_is_native = false


## Opens a document. Returns true when it worked.
func open_document(file_path: String) -> bool:
	var result := scene.open(file_path)
	if not result.ok:
		message.emit("Could not open %s: %s" % [file_path.get_file(), result.error], true)
		return false
	path = file_path
	path_is_native = result.kind != "Quill project"
	for warning: String in result.warnings:
		print("warning: ", warning)
	if result.warnings.size() > 0:
		message.emit("Opened with %d notes; see the log" % result.warnings.size(), false)
	return true


## Saves the document at `file_path` (a `.limn` folder or a `.limnz` file), with
## `thumbnail` (PNG data, may be empty) as its picture. Returns true when it worked.
func save_document(file_path: String, thumbnail := PackedByteArray()) -> bool:
	var result := scene.save(file_path, thumbnail)
	if not result.ok:
		message.emit("Could not save: %s" % result.error, true)
		return false
	path = file_path
	path_is_native = true
	message.emit("Saved %s" % file_path.get_file(), false)
	return true


## Brings a picture, a sound or a model into the document as a new layer.
## Returns the new layer's id, or -1 after reporting why it failed.
func import_file(file_path: String) -> int:
	var result := scene.import_file(file_path)
	if not result.ok:
		message.emit("Could not import %s: %s" % [file_path.get_file(), result.error], true)
		return -1
	return int(result.layer)


func title() -> String:
	var name: String = get_info().get("title", "")
	if name == "":
		name = path.get_file().get_basename() if path != "" else "Untitled"
	return name


# ---- units and colours ----

static func to_flicks(seconds: float) -> int:
	return roundi(seconds * FLICKS_PER_SECOND)


static func to_seconds(flicks: int) -> float:
	return float(flicks) / FLICKS_PER_SECOND


## A colour from the document (linear RGB list) as a Godot colour for the screen.
static func screen_color(linear: Array) -> Color:
	return Color(linear[0], linear[1], linear[2]).linear_to_srgb()


## A colour picked on screen as the linear RGB list the document stores.
static func linear_rgb(color: Color) -> Array:
	var linear := color.srgb_to_linear()
	return [linear.r, linear.g, linear.b]


# Each handler reads the status first, so that panels reacting to the signal see the
# state that caused it and not last frame's.

func _on_document_changed() -> void:
	status = scene.status()
	_rows_stale = true
	_info_stale = true
	document_changed.emit()


func _on_state_changed() -> void:
	status = scene.status()
	_rows_stale = true
	_state_stale = true
	state_changed.emit()


func _on_document_replaced() -> void:
	status = scene.status()
	refresh()
	document_replaced.emit()
