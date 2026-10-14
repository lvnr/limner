class_name LimnerApp
extends Node
## Entry point. Builds the world, the editor and the desktop or headset shell, changes
## between the two when the user asks (`switch_to`), and handles the command-line
## options used for automated runs:
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
##   -- --text <words>       write some text in strokes; a literal \n starts a new line
##   -- --render ...         render a camera to a video and quit; see godot/capture/render_job.gd
##   -- --static-dither      keep the dither pattern still (for repeatable screenshots)
##   -- --scheme light|dark  use that colour scheme for this run, without saving it
##   -- --shot <file.png>    save a screenshot after a few frames, then quit
##   -- --bench <frames>     print the average frame time over that many frames, then quit

## The interface in use changed: "desktop" or "headset".
signal mode_changed(mode: String)

var world: LimnerWorld
var editor: LimnerEditor
## The desktop presentation, when that is what is running.
var shell: DesktopShell
## The headset presentation, when that is what is running.
var xr_shell: XRShell
## The interface in use: "desktop" or "headset" (real or simulated).
var mode := ""
## Keeps a copy of unsaved work. There is one for the whole run, handed to each
## shell, so that changing interface never takes the work in hand for work left over
## from a run that ended badly.
var autosave: Autosave
## What the headset shows while the desktop is in use, once OpenXR has run.
var holding_view: XRHoldingView

## The window's size, place and mode before a headset took it over, to give back.
var _desktop_window := {}

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
	autosave = Autosave.new()
	autosave.name = "Autosave"
	autosave.editor = editor
	add_child(autosave)
	# A headset is used when one is there, unless the command line says which to use.
	# `--desktop` is looked at first because asking whether a headset is there starts
	# the headset's own software.
	var problem := ""
	if args.has("--xr") or (not args.has("--desktop") and XRShell.is_available()):
		problem = _start_headset("auto", false)
	if xr_shell == null:
		_start_desktop(false, problem)
	_apply_arguments.call_deferred()


## Carries on in the other interface: `to` is "desktop" or "headset". The world and the
## editor stay, so the document, its undo history, the selection and the view come
## along; the interface around them is taken down and the other one built. A stroke
## or grab in progress is finished or taken back first. When the headset cannot start,
## the desktop is built again and says why.
func switch_to(to: String) -> void:
	if to == mode or not to in ["desktop", "headset"]:
		return
	var target := ModeSwitch.headset_target()
	if to == "headset" and target == "":
		editor.message.emit(ModeSwitch.unavailable_reason(), true)
		return
	world.scene.input_cancel()
	_tear_down()
	if to == "headset":
		var problem := _start_headset(target, true)
		if xr_shell == null:
			_start_desktop(true, problem)
	else:
		_start_desktop(true, "")
	mode_changed.emit(mode)


## A shell asked for the change. It is made once the shell's own handling of the
## press is over, since the shell is taken down by it.
func _on_switch_requested(to: String) -> void:
	switch_to.call_deferred(to)


## Builds the headset shell. Returns an empty string when it started, otherwise why
## not, with nothing of it left behind.
func _start_headset(rig_mode: String, switched: bool) -> String:
	_desktop_window = {"size": get_window().size, "position": get_window().position, "mode": get_window().mode}
	# The headset's frames go back to the rig: the holding view lets go of them first.
	_drop_holding_view()
	var headset := XRShell.new()
	headset.name = "Headset"
	headset.world = world
	headset.editor = editor
	headset.mode = rig_mode
	headset.switched = switched
	headset.autosave = autosave
	headset.switch_requested.connect(_on_switch_requested)
	add_child(headset)
	if not headset.started:
		var problem := headset.failure
		headset.stop()
		remove_child(headset)
		headset.free()
		return problem
	xr_shell = headset
	mode = "headset"
	return ""


## Builds the desktop shell; `problem`, when there is one, is shown in it.
func _start_desktop(switched: bool, problem: String) -> void:
	shell = DesktopShell.new()
	shell.name = "Desktop"
	shell.world = world
	shell.editor = editor
	shell.switched = switched
	shell.autosave = autosave
	shell.switch_requested.connect(_on_switch_requested)
	add_child(shell)
	mode = "desktop"
	if switched and not _desktop_window.is_empty():
		var window := get_window()
		window.mode = _desktop_window.mode
		if window.mode == Window.MODE_WINDOWED:
			window.size = _desktop_window.size
			window.position = _desktop_window.position
	if problem != "":
		shell.show_message.call_deferred(problem, true)


## Takes the interface in use down, with everything it put in the world. When it was a
## real headset, the holding view takes the headset's frames from then on.
func _tear_down() -> void:
	if shell != null:
		shell.stop()
		remove_child(shell)
		shell.free()
		shell = null
	if xr_shell != null:
		var real := xr_shell.rig is OpenXRRig
		xr_shell.stop()
		remove_child(xr_shell)
		xr_shell.free()
		xr_shell = null
		if real:
			_hold_headset()
	mode = ""


func _hold_headset() -> void:
	holding_view = XRHoldingView.new()
	holding_view.return_requested.connect(_on_switch_requested.bind("headset"))
	holding_view.ended.connect(_on_headset_ended)
	add_child(holding_view)


func _drop_holding_view() -> void:
	if holding_view != null:
		remove_child(holding_view)
		holding_view.free()
		holding_view = null


## The headset's software ended the session while the desktop was in use: the app
## carries on there, and stops offering the headset.
func _on_headset_ended() -> void:
	_drop_holding_view.call_deferred()
	ModeSwitch.mark_headset_ended()
	editor.message.emit("The headset's session ended. Limner carries on here on the desktop.", false)


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
				elif xr_shell != null:
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
				editor.add_text(args[i].replace("\\n", "\n"), 0.25)
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
