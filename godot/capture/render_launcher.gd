class_name RenderLauncher
extends Node
## Starts a video render in a second copy of the application and watches it.
##
## The render runs in a process of its own so that the artist keeps working. That
## process is this same application started with Godot's Movie Maker options
## (`--write-movie`, `--fixed-fps`) and Limner's `--render` options; `RenderJob` is
## what runs inside it. It works from a saved copy of the document, so edits made
## after the start are not in the video.
##
## Godot decides the size of a movie from the project's window size when a process
## starts, before any script runs. The launcher therefore leaves a small settings
## file for the new process to pick up (see `override_path`), which the new process
## removes as soon as it has started.

## A frame was finished.
signal progressed(frame: int, frames: int)
## The render ended: finished, failed or cancelled. `text` says which, and where the file is.
signal finished(ok: bool, text: String)

## The video formats Godot's Movie Maker writes, by file extension.
const FORMATS := {
	"avi": "AVI (Motion JPEG)",
	"png": "PNG sequence",
	"ogv": "OGV (Theora)",
}
## What each format is good for, for the panel to show.
const FORMAT_NOTES := {
	"avi": "Every frame a JPEG, with sound. Opens almost anywhere and is easy to cut; several times larger than a modern video file.",
	"png": "One lossless picture per frame in a folder of its own, with the sound as a WAV beside them. The best source for editing and for encoding to other formats.",
	"ogv": "A small file with sound, in open formats. Visibly compressed; for previews.",
}
## The first line of the settings file, by which it is known as ours.
const OVERRIDE_MARK := "; Limner render settings. Written for a render in another process, which removes it."
## Seconds between looks at the render process.
const POLL := 0.25
## Seconds a cancelled render gets to close its file before it is stopped.
const CANCEL_GRACE := 5.0

## True from `launch` until `finished`.
var running := false

var _pid := -1
var _output := ""
var _status_path := ""
## A copy of the document made for this render, removed when it ends.
var _copy := ""
var _since_poll := 0.0
var _cancelled_for := -1.0
var _last_frame := -1


## Where the settings file for a new process goes: the file the project names in
## `application/config/project_settings_override`, or else `override.cfg` in the
## project folder, which Godot always reads.
static func override_path() -> String:
	var named: String = ProjectSettings.get_setting("application/config/project_settings_override", "")
	return named if named != "" else "res://override.cfg"


## Removes the settings file, if it is one of ours.
static func remove_override() -> void:
	var path := override_path()
	if not FileAccess.file_exists(path):
		return
	if FileAccess.get_file_as_string(path).begins_with(OVERRIDE_MARK):
		DirAccess.remove_absolute(ProjectSettings.globalize_path(path))


## What the settings file says to make the next process write movies of `size` pixels.
static func override_text(size: Vector2i) -> String:
	return "%s\n\n[display]\n\nwindow/size/window_width_override=%d\nwindow/size/window_height_override=%d\n" % [OVERRIDE_MARK, size.x, size.y]


## Leaves the settings file for the process started next.
static func write_override(size: Vector2i) -> Error:
	var file := FileAccess.open(override_path(), FileAccess.WRITE)
	if file == null:
		return FileAccess.get_open_error()
	file.store_string(override_text(size))
	file.close()
	return OK


## The size of the movies this process writes, decided when it started.
static func movie_size() -> Vector2i:
	var size := Vector2i(
		int(ProjectSettings.get_setting("display/window/size/window_width_override", 0)),
		int(ProjectSettings.get_setting("display/window/size/window_height_override", 0)))
	if size.x > 0 and size.y > 0:
		return size
	return Vector2i(
		int(ProjectSettings.get_setting("display/window/size/viewport_width")),
		int(ProjectSettings.get_setting("display/window/size/viewport_height")))


## The file Movie Maker is given for `output`. A PNG sequence gets a folder of its
## own, named like the file: `shots/take.png` becomes `shots/take/take.png`, and
## the frames `take00000000.png`, `take00000001.png` and so on inside it.
static func movie_path(output: String) -> String:
	if output.get_extension().to_lower() != "png":
		return output
	return output.get_basename().path_join(output.get_file())


## The command line that renders `document` as `settings` say, without the
## executable: Godot's own options, then `--` and the options `RenderJob` reads.
##
## `settings` holds `output` (the file; its extension chooses the format), `size`
## (Vector2i), `fps`, `quality`, `projection`, `sound`, and optionally `camera` (a
## layer id or path, or "view" for the view the document was saved with) and `range`
## (`[start, end]` in seconds). What is left out is decided by the job.
static func command_line(document: String, settings: Dictionary, status_path: String) -> PackedStringArray:
	var size: Vector2i = settings.size
	var fps: float = settings.fps
	var arguments := PackedStringArray()
	if OS.has_feature("editor"):
		# Not an exported application: the engine has to be told where the project is.
		arguments.append_array(["--path", ProjectSettings.globalize_path("res://")])
	# The window only shows progress; the picture is rendered at full size off screen.
	var window := Vector2i(Vector2(size) * minf(1.0, 480.0 / maxi(size.x, size.y)))
	arguments.append_array([
		"--write-movie", movie_path(settings.output),
		"--fixed-fps", str(roundi(fps)),
		"--disable-vsync",
		"--resolution", "%dx%d" % [maxi(window.x, 64), maxi(window.y, 64)],
		"--", "--render", "--open", document,
		"--fps", String.num(fps, 4),
		"--size", "%dx%d" % [size.x, size.y],
		"--quality", settings.get("quality", "good"),
		"--projection", settings.get("projection", "flat"),
		"--status", status_path,
	])
	if settings.has("camera"):
		arguments.append_array(["--camera", str(settings.camera)])
	if settings.has("range"):
		var span: Array = settings.range
		arguments.append_array(["--range", String.num(span[0], 6), String.num(span[1], 6)])
	if not settings.get("sound", true):
		arguments.append("--no-sound")
	if settings.get("minimized", false):
		arguments.append("--minimized")
	return arguments


## Renders the document that is open in `editor`, as it is now (unsaved changes and
## the view included), from a copy in `user://renders/` that is removed when the
## render ends. The user's own file is left alone: saving it again here would
## replace its picture, which only the shell can take. Returns an empty string when
## the render has started, or the reason it could not.
func render_document(editor: LimnerEditor, settings: Dictionary) -> String:
	if running:
		return "A render is already running"
	var folder := ProjectSettings.globalize_path("user://renders")
	DirAccess.make_dir_recursive_absolute(folder)
	var document := folder.path_join(LimnerCapture.stamped_name(editor.title(), "limnz"))
	var saved := editor.scene.save_copy(document)
	if not saved.ok:
		return "The document could not be copied for rendering: %s" % saved.error
	_copy = document
	var problem := launch(document, settings)
	if problem != "":
		_remove_copy()
	return problem


## Starts a process that renders the saved document at `document`; see
## `command_line` for `settings`. Returns an empty string, or the reason it could
## not start.
func launch(document: String, settings: Dictionary) -> String:
	if running:
		return "A render is already running"
	var output: String = settings.output
	if not FORMATS.has(output.get_extension().to_lower()):
		return "The file name must end in .avi, .png or .ogv"
	# Movie Maker does not make folders, and stops the process when one is missing.
	if DirAccess.make_dir_recursive_absolute(movie_path(output).get_base_dir()) != OK:
		return "The folder %s could not be made" % movie_path(output).get_base_dir()
	var folder := ProjectSettings.globalize_path("user://renders")
	DirAccess.make_dir_recursive_absolute(folder)
	_status_path = folder.path_join("status_%d_%d.json" % [OS.get_process_id(), Time.get_ticks_msec()])
	if write_override(settings.size) != OK:
		return "The render settings could not be written to %s" % override_path()
	_pid = OS.create_process(OS.get_executable_path(), command_line(document, settings, _status_path))
	if _pid <= 0:
		remove_override()
		return "The render process could not be started"
	_output = output
	_since_poll = 0.0
	_cancelled_for = -1.0
	_last_frame = -1
	running = true
	return ""


## Asks the render to stop. It closes its file first, so what was rendered so far
## stays playable.
func cancel() -> void:
	if not running or _cancelled_for >= 0.0:
		return
	var file := FileAccess.open(_status_path + ".cancel", FileAccess.WRITE)
	if file != null:
		file.close()
	_cancelled_for = 0.0


func _process(delta: float) -> void:
	if not running:
		return
	_since_poll += delta
	if _cancelled_for >= 0.0:
		_cancelled_for += delta
		if _cancelled_for > CANCEL_GRACE and OS.is_process_running(_pid):
			OS.kill(_pid)
	if _since_poll < POLL:
		return
	_since_poll = 0.0
	var status := _read_status()
	if not status.is_empty():
		# The process is up and has read its settings.
		remove_override()
		var frame := int(status.get("frame", 0))
		if frame != _last_frame:
			_last_frame = frame
			progressed.emit(frame, int(status.get("frames", 0)))
	if not OS.is_process_running(_pid):
		_finish(_read_status())


## What the render process last said about itself: `{state, frame, frames, error}`,
## or nothing when it has not said anything yet.
func _read_status() -> Dictionary:
	if not FileAccess.file_exists(_status_path):
		return {}
	var json := JSON.new()
	if json.parse(FileAccess.get_file_as_string(_status_path)) == OK and json.data is Dictionary:
		return json.data
	return {}


func _finish(status: Dictionary) -> void:
	running = false
	remove_override()
	_remove_copy()
	for path: String in [_status_path, _status_path + ".cancel"]:
		if FileAccess.file_exists(path):
			DirAccess.remove_absolute(path)
	var frames := int(status.get("frames", 0))
	var frame := int(status.get("frame", 0))
	match status.get("state", ""):
		"done":
			finished.emit(true, "Rendered %d frames to %s" % [frames, movie_path(_output)])
		"cancelled":
			finished.emit(false, "Cancelled after %d of %d frames; they are in %s" % [frame, frames, movie_path(_output)])
		"failed":
			finished.emit(false, "The render failed: %s" % status.get("error", "no reason given"))
		_:
			if _cancelled_for >= 0.0:
				finished.emit(false, "Cancelled; the render process had to be stopped, so %s may not play" % movie_path(_output))
			else:
				finished.emit(false, "The render process stopped after %d of %d frames without saying why" % [frame, frames])


func _remove_copy() -> void:
	if _copy != "" and FileAccess.file_exists(_copy):
		DirAccess.remove_absolute(_copy)
	_copy = ""
