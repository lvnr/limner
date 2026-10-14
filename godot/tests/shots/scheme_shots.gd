extends SceneTree
## Pictures of the desktop interface in both colour schemes, for checking them by eye.
## Run it windowed:
##
##   scripts/godot.sh --path godot --resolution 1440x900 --script res://tests/shots/scheme_shots.gd
##
## It paints the demo sheet, fills the timeline with every kind of thing it draws
## (clips of each kind of layer, frames, keys, a selected key, chapter marks, a working
## range) and makes the timeline tall. Then, starting in the dark scheme and switching
## to the light one while running, as the Settings panel does, it writes
## `local/out/scheme_<scheme>_<tab>.png` for every tab of the details dock (with the
## timeline put away) and `scheme_<scheme>_timeline.png`. The user's
## preferences are not touched.

const OUT := "res://../local/out/"
const SECOND := LimnerEditor.FLICKS_PER_SECOND

var main: Node
var editor: LimnerEditor
var shell: DesktopShell


func _initialize() -> void:
	_run.call_deferred()


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _run() -> void:
	main = load("res://main.tscn").instantiate()
	root.add_child(main)
	await _frames(4)
	editor = main.editor
	shell = main.shell
	if shell == null:
		push_error("scheme_shots needs the desktop shell")
		quit(1)
		return
	LimnerDemo.run(editor)
	_fill_timeline()
	await _frames(4)
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(OUT))
	for scheme: String in ["dark", "light"]:
		Kit.use_scheme(scheme)
		# The panels with the timeline put away, so that they have the height of the window.
		shell.timeline.set_collapsed(true)
		for index in shell.details.get_tab_count():
			var tab_name := shell.details.get_tab_control(index).name
			shell.show_details(tab_name)
			await _frames(4)
			_save("scheme_%s_%s.png" % [scheme, tab_name.to_lower()])
		# Then the timeline, tall.
		shell.timeline.set_collapsed(false)
		shell.show_details("Layer")
		await _frames(4)
		_save("scheme_%s_timeline.png" % scheme)
	Kit.use_scheme("dark")
	quit(0)


func _save(file: String) -> void:
	var path := ProjectSettings.globalize_path(OUT + file)
	root.get_texture().get_image().save_png(path)
	print("wrote ", path)


func _fill_timeline() -> void:
	var scene := editor.scene
	scene.exec({"cmd": "set_document", "rate": {"num": 6, "den": 1}})
	var walk := int(scene.exec({"cmd": "add_layer", "kind": "paint", "name": "Walk"}).result.layer)
	scene.exec({"cmd": "add_frame", "layer": walk, "kind": "blank"})
	scene.exec({"cmd": "add_frame", "layer": walk, "kind": "duplicate"})
	scene.exec({"cmd": "add_frame", "layer": walk, "kind": "blank"})
	for time: int in [SECOND / 2, 2 * SECOND, 7 * SECOND / 2]:
		scene.exec({"cmd": "set_key", "layer": walk, "track": "transform", "time": time})
	scene.exec({"cmd": "set_layer", "layer": walk, "opacity": 0.3})
	scene.exec({"cmd": "set_key", "layer": walk, "track": "opacity", "time": 3 * SECOND})
	scene.exec({"cmd": "set_layer", "layer": walk, "opacity": 1.0})
	scene.exec({"cmd": "set_key", "layer": walk, "track": "opacity", "time": 5 * SECOND})
	for kind: String in ["sequence", "camera", "light", "sound", "group", "viewpoint"]:
		scene.exec({"cmd": "add_layer", "kind": kind, "name": kind.capitalize()})
	scene.exec({"cmd": "set_action", "time": 4 * SECOND, "action": "stop"})
	scene.exec({"cmd": "set_action", "time": 6 * SECOND, "action": "loop"})
	scene.exec({"cmd": "set_action", "time": 8 * SECOND, "action": "play"})
	scene.exec({"cmd": "set_document", "range_start": SECOND / 3})
	scene.exec({"cmd": "select_layers", "layers": [walk]})
	scene.exec({"cmd": "seek", "time": 2 * SECOND + SECOND / 6})
	var timeline := shell.timeline
	timeline.set_height(470.0)
	timeline.key_track = "transform"
	timeline.selected_keys = {walk: [2 * SECOND]}
	timeline.view.queue_redraw()
