extends Node
## Entry point. Builds the world, the editor and the desktop shell, and handles the
## command-line options used for automated runs:
##
##   -- --open <path>        open a document at start
##   -- --frame-all          fit the document in view
##   -- --time <seconds>     move the playhead
##   -- --orbit <yaw,pitch>  turn the view, in degrees, after framing
##   -- --exec <json>        run a command, e.g. '{"cmd":"add_layer","kind":"light"}'
##   -- --save <path>        save as a .limn folder or .limnz file
##   -- --demo               paint a sample sheet with every brush
##   -- --no-panels          hide the docks
##   -- --panel <name>       bring a tabbed panel to the front: Transform, Layer, Document, Export, Monitor, Performance, Settings
##   -- --xr | --desktop     start in a headset or on the desktop, whatever is connected
##   -- --sim | --no-sim     with --xr: use the simulated headset and hands (the default when
##                           no headset answers), or fail instead of falling back to them;
##                           the --sim-... options that pose them are in godot/xr/sim_rig.gd
##   -- --text <words>       write some text in strokes
##   -- --render ...         render a camera to a video and quit; see godot/capture/render_job.gd
##   -- --static-dither      keep the dither pattern still (for repeatable screenshots)
##   -- --shot <file.png>    save a screenshot after a few frames, then quit
##   -- --bench <frames>     print the average frame time over that many frames, then quit

var world: LimnerWorld
var editor: LimnerEditor
## The desktop presentation, when that is what is running.
var shell: DesktopShell
## The headset presentation, when that is what is running.
var xr_shell: XRShell

var _shot_path := ""
var _frames_until_shot := -1
var _bench_frames := 0
var _bench_left := -1
var _bench_start_usec := 0


func _ready() -> void:
	world = LimnerWorld.new()
	world.name = "World"
	add_child(world)
	editor = LimnerEditor.new()
	editor.name = "Editor"
	add_child(editor)
	editor.setup(world.scene)
	var args := OS.get_cmdline_user_args()
	if args.has("--render"):
		# A render runs in a process of its own, without any interface.
		var job := RenderJob.new()
		job.name = "Render"
		job.world = world
		job.editor = editor
		add_child(job)
		job.start(args)
		return
	# A headset is used when one is there, unless the command line says which to use.
	# `--desktop` is looked at first because asking whether a headset is there starts
	# the headset's own software.
	var problem := ""
	if args.has("--xr") or (not args.has("--desktop") and XRShell.is_available()):
		xr_shell = XRShell.new()
		xr_shell.name = "Headset"
		xr_shell.world = world
		xr_shell.editor = editor
		add_child(xr_shell)
		if not xr_shell.started:
			problem = xr_shell.failure
			xr_shell.queue_free()
			xr_shell = null
	if xr_shell == null:
		shell = DesktopShell.new()
		shell.name = "Desktop"
		shell.world = world
		shell.editor = editor
		add_child(shell)
		if problem != "":
			shell.show_message.call_deferred(problem, true)
	_apply_arguments.call_deferred()


func _apply_arguments() -> void:
	var args := OS.get_cmdline_user_args()
	# Let the interface lay itself out first: framing depends on the size of the free area.
	await get_tree().process_frame
	await get_tree().process_frame
	var i := 0
	while i < args.size():
		match args[i]:
			"--open":
				i += 1
				if not editor.open_document(args[i]):
					push_error("Could not open %s" % args[i])
			"--frame-all":
				if shell != null:
					shell.frame_all()
				else:
					xr_shell.frame_all()
			"--time":
				i += 1
				world.scene.set_time(float(args[i]))
			"--orbit":
				i += 1
				var parts := args[i].split(",")
				var pivot := shell.focus_point() if shell != null else Vector3(0.0, 1.4, -1.5)
				world.scene.view_rotate(pivot, Vector3.UP, deg_to_rad(float(parts[0])))
				if parts.size() > 1:
					world.scene.view_rotate(pivot, Vector3.RIGHT, deg_to_rad(float(parts[1])))
			"--exec":
				i += 1
				var command = JSON.parse_string(args[i])
				if command is Dictionary:
					var reply := world.scene.exec(command)
					print("exec ", args[i], " -> ", reply)
				else:
					push_error("--exec needs a JSON object, got: %s" % args[i])
			"--save":
				i += 1
				editor.save_document(args[i])
			"--demo":
				LimnerDemo.run(editor)
			"--text":
				i += 1
				editor.add_text(args[i], 0.25)
			"--no-panels":
				if shell != null:
					shell.set_panels_visible(false)
			"--panel":
				i += 1
				if shell != null:
					shell.show_details(args[i])
			"--xr", "--desktop":
				pass
			"--static-dither":
				world.animate_dither = false
			"--shot":
				i += 1
				_shot_path = args[i]
				_frames_until_shot = 8
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
