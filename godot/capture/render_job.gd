class_name RenderJob
extends Node
## Renders a camera's view of a range of the timeline to a video or an image
## sequence, in a process of its own.
##
## The video file is written by Godot's Movie Maker, which records the window once
## per frame; this job decides what the window shows. On every frame it places the
## playhead, renders the picture off screen (several passes for the chosen quality,
## all within the frame) and shows the result full-window for Movie Maker to take.
##
## The options, after `--render`:
##
##   --open <project>             the document: a .limn folder, a .limnz file or a Quill project
##   --camera <id|path|view>      a camera layer by id or by path (Root/Shots/Wide). Default: the
##                                first camera layer, or without one the view the document was
##                                saved with, which is also what "view" asks for
##   --range <start_s> <end_s>    default: the document's working range
##   --fps <n>                    default: the document's frame rate
##   --size <w>x<h>               default: the document's camera resolution
##   --quality draft|good|best    default: good
##   --projection flat|equirect|equirect_stereo
##   --no-sound                   record silence
##   --out <file>                 the video to write: .avi, .png (a sequence) or .ogv
##   --status <file>              where to report progress as JSON, for a launcher
##   --minimized                  keep the window out of the way
##
## There are two ways to run it. With `--out`, and without Godot's `--write-movie`,
## the process does not render: it starts a second one with the right engine options
## and frame size, waits for it and passes on its progress, which makes one command
## enough (and `--headless` is fine for it):
##
##   godot --headless --path godot -- --render --open piece.limn --out piece.avi
##
## Started with `--write-movie <file> --fixed-fps <n>` it renders. The frame size is
## then whatever the process started with (see `RenderLauncher`), and `--size` must
## agree with it. Progress is printed as `render: frame N of M`.

const PROJECTIONS := ["flat", "equirect", "equirect_stereo"]

var world: LimnerWorld
var editor: LimnerEditor

var _options := {}
var _capture: LimnerCapture
var _launcher: RenderLauncher
## The canvas item that fills the window with the frame.
var _screen: RID
var _picture: ImageTexture
var _size := Vector2i.ZERO
## The camera layer rendered, or -1 for the view the document was saved with.
var _camera := -1
var _start := 0.0
var _fps := 24.0
var _frame := 0
var _frames := 0
var _rendering := false


## Reads the options that follow `--render`. Returns them by name, with `error`
## saying what is wrong when something is.
static func parse(arguments: PackedStringArray) -> Dictionary:
	var options := {"error": "", "quality": "good", "projection": "flat", "sound": true, "minimized": false}
	var values := {"--open": "open", "--camera": "camera", "--fps": "fps", "--size": "size", "--quality": "quality", "--projection": "projection", "--out": "out", "--status": "status"}
	var i := 0
	while i < arguments.size():
		var word := arguments[i]
		if values.has(word):
			if i + 1 >= arguments.size():
				options.error = "%s needs a value" % word
				return options
			i += 1
			options[values[word]] = arguments[i]
		elif word == "--range":
			if i + 2 >= arguments.size():
				options.error = "--range needs a start and an end, in seconds"
				return options
			options["range"] = [arguments[i + 1], arguments[i + 2]]
			i += 2
		elif word == "--no-sound":
			options.sound = false
		elif word == "--minimized":
			options.minimized = true
		elif word != "--render":
			options.error = "unknown render option %s" % word
			return options
		i += 1

	if not options.has("open"):
		options.error = "--open <project> is missing"
	elif options.quality not in LimnerCapture.QUALITIES:
		options.error = "--quality must be draft, good or best, not %s" % options.quality
	elif options.projection not in PROJECTIONS:
		options.error = "--projection must be flat, equirect or equirect_stereo, not %s" % options.projection
	if options.error == "" and options.has("range"):
		var span: Array = options.range
		if not (str(span[0]).is_valid_float() and str(span[1]).is_valid_float()) or float(span[0]) < 0.0 or float(span[1]) <= float(span[0]):
			options.error = "--range needs a start of zero or later and an end after it, in seconds"
		else:
			options.range = [float(span[0]), float(span[1])]
	if options.error == "" and options.has("fps"):
		var fps: String = options.fps
		if not fps.is_valid_float() or float(fps) < 1.0 or float(fps) > 480.0:
			options.error = "--fps must be between 1 and 480, not %s" % fps
		else:
			options.fps = float(fps)
	if options.error == "" and options.has("size"):
		var text: String = options.size
		var parts := text.to_lower().split("x")
		var limit := LimnerCapture.MAX_SIZE
		if parts.size() != 2 or not parts[0].is_valid_int() or not parts[1].is_valid_int() or int(parts[0]) < 16 or int(parts[1]) < 16 or int(parts[0]) > limit or int(parts[1]) > limit:
			options.error = "--size must be <width>x<height>, each from 16 to %d, not %s" % [limit, text]
		else:
			options.size = Vector2i(int(parts[0]), int(parts[1]))
	return options


## The frame rate Movie Maker writes at in this process: Godot's `--fixed-fps`
## among `arguments` (it takes whole numbers only), or else the project's movie rate.
static func movie_fps(arguments := OS.get_cmdline_args()) -> int:
	var at := arguments.find("--fixed-fps")
	if at >= 0 and at + 1 < arguments.size() and arguments[at + 1].is_valid_int():
		return maxi(1, int(arguments[at + 1]))
	return maxi(1, int(ProjectSettings.get_setting("editor/movie_writer/fps", 60)))


## How fast sound must play for a video written at `movie` frames a second, one
## frame of sound each, to stay with a playhead that steps `1 / fps` seconds a frame.
static func sound_speed(movie: int, fps: float) -> float:
	return float(movie) / fps


## Starts the job described by the command-line arguments after `--render`.
func start(arguments: PackedStringArray) -> void:
	# The settings a launcher left for this process were read when it started.
	RenderLauncher.remove_override()
	_options = parse(arguments)
	if _options.error != "":
		_fail(_options.error)
		return
	if not editor.open_document(_options.open):
		_fail("could not open %s" % _options.open)
		return
	var info := world.scene.document_info()
	_fps = _options.get("fps", float(info.fps))
	var span: Array = _options.get("range", [LimnerEditor.to_seconds(int(info.range_start)), LimnerEditor.to_seconds(int(info.range_end))])
	_start = span[0]
	_frames = maxi(1, roundi((float(span[1]) - _start) * _fps))
	if not _find_camera():
		return
	if Engine.get_write_movie_path() == "":
		_start_launcher(info)
	else:
		_start_rendering(info)


## Finds the layer `--camera` names. Returns false after reporting that there is none.
func _find_camera() -> bool:
	var wanted: String = _options.get("camera", "")
	var cameras: Array = world.scene.layer_rows().filter(func(row: Dictionary) -> bool: return row.kind == "camera")
	if wanted == "view":
		return true
	if wanted == "":
		if not cameras.is_empty():
			_camera = int(cameras[0].id)
		return true
	for row: Dictionary in cameras:
		var path: String = world.scene.layer_details(int(row.id)).get("path", "")
		if (wanted.is_valid_int() and int(wanted) == int(row.id)) or wanted == path:
			_camera = int(row.id)
			return true
	_fail("there is no camera layer %s" % wanted)
	return false


## The size of picture the options ask for: `--size`, or the document's camera
## resolution. A 360 picture takes its width from that and its shape from the layout.
func _wanted_size(info: Dictionary) -> Vector2i:
	var resolution: Array = info.camera_resolution
	var size: Vector2i = _options.get("size", Vector2i(int(resolution[0]), int(resolution[1])))
	if _options.projection != "flat":
		size = LimnerCapture.panorama_size(_options.projection, size.x)
	return size.clamp(Vector2i(16, 16), Vector2i(LimnerCapture.MAX_SIZE, LimnerCapture.MAX_SIZE))


# ---- starting the process that renders ----

func _start_launcher(info: Dictionary) -> void:
	if not _options.has("out"):
		_fail("--out <file> is missing (or start the process with Godot's --write-movie to render in it)")
		return
	var settings := {
		"output": _options.out,
		"size": _wanted_size(info),
		"fps": _fps,
		"quality": _options.quality,
		"projection": _options.projection,
		"sound": _options.sound,
		"minimized": _options.minimized,
	}
	for key: String in ["camera", "range"]:
		if _options.has(key):
			settings[key] = _options[key]
	_launcher = RenderLauncher.new()
	_launcher.name = "Launcher"
	add_child(_launcher)
	# The rendering process prints its own progress lines to this same output.
	_launcher.progressed.connect(func(frame: int, frames: int) -> void: _report("rendering", frame, frames))
	_launcher.finished.connect(func(ok: bool, text: String) -> void:
		print("render: ", text)
		_report("done" if ok else "failed", _frames if ok else 0, _frames, "" if ok else text)
		get_tree().quit(0 if ok else 1)
	)
	var problem := _launcher.launch(_options.open, settings)
	if problem != "":
		_fail(problem)


# ---- rendering ----

func _start_rendering(info: Dictionary) -> void:
	_size = RenderLauncher.movie_size()
	var wanted := _wanted_size(info)
	if wanted != _size:
		print("render: the video is %dx%d, the size this process was started with, not %dx%d; use --out to choose the size" % [_size.x, _size.y, wanted.x, wanted.y])
	# The window is only a progress display. Its content is drawn at the size of the
	# video whatever size the window is, so a video can be larger than the screen.
	var window := get_window()
	window.content_scale_mode = Window.CONTENT_SCALE_MODE_VIEWPORT
	window.content_scale_aspect = Window.CONTENT_SCALE_ASPECT_KEEP
	window.content_scale_size = _size
	if _options.minimized:
		window.mode = Window.MODE_MINIMIZED
	# The picture is handed to the rendering server directly and drawn on request,
	# so that every frame is on screen before Movie Maker takes it, whether or not
	# the window is visible and the engine draws of its own accord.
	_picture = ImageTexture.create_from_image(Image.create_empty(_size.x, _size.y, false, Image.FORMAT_RGB8))
	_screen = RenderingServer.canvas_item_create()
	RenderingServer.canvas_item_set_parent(_screen, get_viewport().find_world_2d().canvas)
	RenderingServer.canvas_item_add_texture_rect(_screen, Rect2(Vector2.ZERO, Vector2(_size)), _picture.get_rid())

	AudioServer.set_bus_mute(0, not _options.sound)
	# Movie Maker takes one frame's worth of sound at its own whole-number rate for
	# every frame, while the job steps the playhead at the document's exact rate. At
	# 23.976 frames a second in a file labelled 24 the picture would run 0.1% ahead of
	# the sound; played that much faster, the sound keeps up (by 1.7 cents of pitch).
	AudioServer.playback_speed_scale = sound_speed(movie_fps(), _fps)
	world.scene.exec({"cmd": "pause"})
	world.scene.exec({"cmd": "set_options", "values": {"loop_range": false}})
	_capture = LimnerCapture.of(editor)
	# Held for the whole render: grid, gizmos and onion skins stay hidden throughout.
	_capture.begin()
	_report("rendering", 0, _frames)
	_rendering = true


func _process(_delta: float) -> void:
	if not _rendering:
		return
	if _options.has("status") and FileAccess.file_exists(str(_options.status) + ".cancel"):
		_stop("cancelled", "")
		return
	# The job, not the clock, says which instant each frame shows.
	var time := _start + _frame / _fps
	if absf(world.scene.get_time() - time) > 1e-7:
		world.scene.set_time(time)
	_capture.settle()
	var view := _capture.current_view() if _camera < 0 else _capture.layer_view(_camera)
	var image: Image
	if _options.projection == "flat":
		image = _capture.still(view, _size, _options.quality)
	else:
		image = _capture.panorama(view, _options.projection, _size.x, _options.quality)
		if image.get_size() != _size:
			image.resize(_size.x, _size.y, Image.INTERPOLATE_LANCZOS)
	_picture.update(image)
	RenderingServer.force_draw(false)
	if _frame == 0 and _options.sound:
		# Sound layers are heard while the document plays. Playing moves the playhead
		# on by one frame per frame, which is where the job puts it anyway.
		world.scene.exec({"cmd": "play"})
	_frame += 1
	print("render: frame %d of %d" % [_frame, _frames])
	if _frame == _frames:
		_stop("done", "")
	else:
		_report("rendering", _frame, _frames)


func _notification(what: int) -> void:
	if what == NOTIFICATION_PREDELETE and _screen.is_valid():
		RenderingServer.free_rid(_screen)


## Ends the job. The frame on screen is still recorded; Movie Maker then closes the file.
func _stop(state: String, error: String) -> void:
	_rendering = false
	_report(state, _frame, _frames, error)
	get_tree().quit(0 if state == "done" else 1)


func _fail(error: String) -> void:
	push_error("render: %s" % error)
	_stop("failed", error)


## Tells a launcher how far the job is, through the file named by `--status`.
func _report(state: String, frame: int, frames: int, error := "") -> void:
	if not _options.has("status"):
		return
	# Written beside the file and moved over it, so that a reader never finds half a report.
	var path: String = _options.status
	var file := FileAccess.open(path + ".part", FileAccess.WRITE)
	if file != null:
		file.store_string(JSON.stringify({"state": state, "frame": frame, "frames": frames, "error": error}))
		file.close()
		DirAccess.rename_absolute(path + ".part", path)
