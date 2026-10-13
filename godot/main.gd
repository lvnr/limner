extends Node
## Entry point. Builds the world and the desktop shell, and handles the command-line
## options used for automated runs:
##
##   -- --open <path>        open a document at start
##   -- --frame-all          fit the document in view
##   -- --time <seconds>     move the playhead
##   -- --orbit <yaw,pitch>  turn the view, in degrees, after framing
##   -- --static-dither      keep the dither pattern still (for repeatable screenshots)
##   -- --shot <file.png>    save a screenshot after a few frames, then quit
##   -- --bench <frames>     print the average frame time over that many frames, then quit

var world: LimnerWorld
var shell: DesktopShell

var _shot_path := ""
var _frames_until_shot := -1
var _bench_frames := 0
var _bench_left := -1
var _bench_start_usec := 0


func _ready() -> void:
	world = LimnerWorld.new()
	world.name = "World"
	add_child(world)
	shell = DesktopShell.new()
	shell.name = "Desktop"
	shell.world = world
	add_child(shell)
	_apply_arguments.call_deferred()


func _apply_arguments() -> void:
	var args := OS.get_cmdline_user_args()
	var i := 0
	while i < args.size():
		match args[i]:
			"--open":
				i += 1
				var result := world.open_document(args[i])
				if not result.ok:
					push_error("Could not open %s: %s" % [args[i], result.error])
				for warning: String in result.warnings:
					print("warning: ", warning)
			"--frame-all":
				shell.frame_all()
			"--time":
				i += 1
				world.scene.set_time(float(args[i]))
			"--orbit":
				i += 1
				var parts := args[i].split(",")
				world.scene.view_rotate(DesktopShell.FOCUS, Vector3.UP, deg_to_rad(float(parts[0])))
				if parts.size() > 1:
					world.scene.view_rotate(DesktopShell.FOCUS, Vector3.RIGHT, deg_to_rad(float(parts[1])))
			"--static-dither":
				world.animate_dither = false
			"--shot":
				i += 1
				_shot_path = args[i]
				_frames_until_shot = 6
			"--bench":
				i += 1
				_bench_frames = int(args[i])
				# Let loading and shader compilation settle first.
				_bench_left = _bench_frames + 30
		i += 1


func _process(_delta: float) -> void:
	if _bench_left >= 0:
		if _bench_left == _bench_frames:
			_bench_start_usec = Time.get_ticks_usec()
		if _bench_left == 0:
			var ms := (Time.get_ticks_usec() - _bench_start_usec) / 1000.0 / _bench_frames
			print("bench: %.2f ms per frame (%.0f fps) %s" % [ms, 1000.0 / ms, world.scene.stats()])
			get_tree().quit()
		_bench_left -= 1
	if _frames_until_shot < 0:
		return
	_frames_until_shot -= 1
	if _frames_until_shot == 0:
		await RenderingServer.frame_post_draw
		var image := get_viewport().get_texture().get_image()
		var error := image.save_png(_shot_path)
		print("shot ", _shot_path, " ", image.get_size(), " ", error_string(error), " ", world.scene.stats())
		get_tree().quit(0 if error == OK else 1)
