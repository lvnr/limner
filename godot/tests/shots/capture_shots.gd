extends SceneTree
## Pictures for checking capture and the Export panel by eye. Run it windowed:
##
##   scripts/godot.sh --path godot --resolution 1440x900 --script res://tests/shots/capture_shots.gd -- <what> ...
##
## and look at what it writes to `local/out/`. `<what>` is any of:
##
##   panels            the Export panel in each of its three sections (cap_export_<section>.png)
##   stills            the demo sheet at Draft and at Best, and with a transparent background
##                     (cap_still_draft.png, cap_still_best.png, cap_still_clear.png), with a
##                     close-up of the see-through strokes of each (cap_detail_<quality>.png)
##   around <project>  a 360 picture of a project from where it opens, in every layout
##                     (cap_around_<layout>.png)
##   render            a one-second video of a small animated document, started from the
##                     Export panel as a user would, as an AVI and as a PNG sequence; prints
##                     what the files hold and keeps two frames (cap_render_first.png,
##                     cap_render_last.png). The videos themselves are removed again.

const OUT := "res://../local/out/"

var main: Node
var editor: LimnerEditor
var world: LimnerWorld
var panel: ExportPanel


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	main = load("res://main.tscn").instantiate()
	root.add_child(main)
	for i in 4:
		await process_frame
	editor = main.editor
	world = main.world
	panel = root.find_children("*", "ExportPanel", true, false)[0]
	DirAccess.make_dir_recursive_absolute(_out(""))
	var args := OS.get_cmdline_user_args()
	var i := 0
	while i < args.size():
		match args[i]:
			"panels":
				await _panels()
			"stills":
				await _stills()
			"around":
				i += 1
				await _around(args[i])
			"render":
				await _render()
			_:
				push_error("unknown: %s" % args[i])
		i += 1
	quit()


func _out(file: String) -> String:
	return ProjectSettings.globalize_path(OUT).simplify_path().path_join(file)


func _save(image: Image, file: String) -> void:
	var error := image.save_png(_out(file))
	print("wrote ", _out(file), " ", image.get_size(), " ", error_string(error))


func _panels() -> void:
	LimnerDemo.run(editor)
	editor.run({"cmd": "add_layer", "kind": "camera", "name": "Wide shot"})
	main.shell.frame_all()
	main.shell.show_details("Export")
	for section: String in ["image", "video", "model"]:
		panel.show_section(section)
		for i in 6:
			await process_frame
		# Drawn on request, so that the picture is there even when the window is covered.
		RenderingServer.force_draw(false)
		_save(root.get_texture().get_image(), "cap_export_%s.png" % section)


func _stills() -> void:
	editor.new_document()
	LimnerDemo.run(editor)
	await process_frame
	var capture := LimnerCapture.of(editor)
	var view := {"transform": Transform3D(Basis.IDENTITY, Vector3(0.0, 1.45, 0.0)), "fov": 32.0, "near": 0.01, "far": 2000.0}
	var close := {"transform": Transform3D(Basis.IDENTITY, Vector3(0.45, 1.6, -0.9)), "fov": 30.0, "near": 0.01, "far": 2000.0}
	for quality: String in ["draft", "good", "best"]:
		var started := Time.get_ticks_msec()
		var image := capture.still(view, Vector2i(1280, 720), quality)
		print("%s: %d ms" % [quality, Time.get_ticks_msec() - started])
		if quality != "good":
			_save(image, "cap_still_%s.png" % quality)
		_save(capture.still(close, Vector2i(480, 360), quality), "cap_detail_%s.png" % quality)
	_save(capture.still(view, Vector2i(1280, 720), "best", true), "cap_still_clear.png")
	var started := Time.get_ticks_msec()
	var large := capture.still(view, Vector2i(8192, 8192), "best")
	print("8192 x 8192 at Best: %d ms, %s" % [Time.get_ticks_msec() - started, large.get_size()])


func _around(project: String) -> void:
	if not editor.open_document(project):
		return
	world.scene.view_go_to_default_viewpoint()
	for i in 4:
		await process_frame
	var capture := LimnerCapture.of(editor)
	var view := LimnerCapture.level(capture.current_view())
	for layout: String in LimnerCapture.LAYOUTS:
		var started := Time.get_ticks_msec()
		var image := capture.panorama(view, layout, 2048, "good")
		print("%s: %d ms" % [layout, Time.get_ticks_msec() - started])
		_save(image, "cap_around_%s.png" % layout)


## Builds a document of two paint layers, one of which moves, and a camera.
func _animated_document() -> void:
	editor.new_document()
	var scene := world.scene
	scene.input_head(Vector3(0.0, 1.4, 0.0))
	editor.run({"cmd": "set_color", "color": LimnerEditor.linear_rgb(Color("29335c"))})
	editor.set_tool_setting("paint", "size", 0.03)
	LimnerDemo.stroke(scene, [Vector3(-0.8, 1.0, -2.0), Vector3(0.0, 1.05, -2.0), Vector3(0.8, 1.0, -2.0)])
	editor.set_tool_setting("paint", "opacity", 0.45)
	editor.run({"cmd": "set_color", "color": LimnerEditor.linear_rgb(Color("669bbc"))})
	LimnerDemo.stroke(scene, [Vector3(-0.8, 1.9, -2.0), Vector3(0.0, 1.8, -2.0), Vector3(0.8, 1.9, -2.0)])
	editor.set_tool_setting("paint", "opacity", 1.0)
	var added: Dictionary = editor.run({"cmd": "add_layer", "kind": "paint", "name": "Mover"})
	var mover := int(added.layer)
	editor.run({"cmd": "select_layers", "layers": [mover]})
	editor.run({"cmd": "set_color", "color": LimnerEditor.linear_rgb(Color("e4572e"))})
	editor.set_tool_setting("paint", "size", 0.08)
	LimnerDemo.stroke(scene, [Vector3(-0.05, 1.45, -2.0), Vector3(0.05, 1.45, -2.0)])
	scene.input_hand_absent(0)
	scene.input_commit(1.0 / 90.0)
	# A key at each end of the second: the layer slides from left to right.
	editor.run({"cmd": "set_options", "values": {"recording": true}})
	for key: Array in [[0.0, -0.6], [1.0, 0.6]]:
		editor.run({"cmd": "seek", "time": LimnerEditor.to_flicks(key[0])})
		var moved: Dictionary = scene.layer_details(mover).transform
		moved.translation = [key[1], 0.0, 0.0]
		editor.run({"cmd": "set_layer", "layer": mover, "transform": moved})
	editor.run({"cmd": "set_options", "values": {"recording": false}})
	var camera: Dictionary = editor.run({"cmd": "add_layer", "kind": "camera", "name": "Shot"})
	var placed: Dictionary = scene.layer_details(int(camera.layer)).transform
	placed.translation = [0.0, 1.45, 0.4]
	editor.run({"cmd": "set_layer", "layer": int(camera.layer), "transform": placed})
	editor.run({"cmd": "set_document", "title": "Render test", "range_start": 0, "range_end": LimnerEditor.to_flicks(1.0), "rate": {"num": 12, "den": 1}, "camera_resolution": [320, 180]})
	editor.run({"cmd": "seek", "time": 0})


func _render() -> void:
	_animated_document()
	main.shell.show_details("Export")
	panel.show_section("video")
	for i in 4:
		await process_frame
	var section := panel.video
	section._camera.select(1)
	section._camera.item_selected.emit(1)
	var folder := _out("cap_render")
	for format: int in [0, 1]:
		section._format.select(format)
		section._format.item_selected.emit(format)
		var extension: String = RenderLauncher.FORMATS.keys()[format]
		section._target.set_path(folder.path_join("take." + extension))
		print("settings: ", section.settings())
		var started := Time.get_ticks_msec()
		section._render.pressed.emit()
		var outcome: Array = await section.launcher.finished
		print("finished in %d ms: %s %s; dirty still %s" % [Time.get_ticks_msec() - started, outcome[0], outcome[1], world.scene.status().dirty])
	_describe_avi(folder.path_join("take.avi"))
	var frames := Array(DirAccess.get_files_at(folder.path_join("take"))).filter(func(file: String) -> bool: return file.ends_with(".png"))
	frames.sort()
	print("sequence: %d PNG files, %s to %s, with %s" % [frames.size(), frames[0], frames[-1], "take.wav" if FileAccess.file_exists(folder.path_join("take/take.wav")) else "no sound file"])
	var first := Image.load_from_file(folder.path_join("take").path_join(frames[0]))
	var last := Image.load_from_file(folder.path_join("take").path_join(frames[-1]))
	_save(first, "cap_render_first.png")
	_save(last, "cap_render_last.png")
	for file: String in DirAccess.get_files_at(folder.path_join("take")):
		DirAccess.remove_absolute(folder.path_join("take").path_join(file))
	DirAccess.remove_absolute(folder.path_join("take"))
	DirAccess.remove_absolute(folder.path_join("take.avi"))
	DirAccess.remove_absolute(folder)


## Prints what the header of an AVI file says, and how many frames the file holds.
func _describe_avi(path: String) -> void:
	var data := FileAccess.get_file_as_bytes(path)
	if data.size() < 64 or data.slice(0, 4).get_string_from_ascii() != "RIFF" or data.slice(8, 12).get_string_from_ascii() != "AVI ":
		print("%s is not an AVI file (%d bytes)" % [path, data.size()])
		return
	# The main header: microseconds per frame first, the frame count fifth, then the size.
	var header := _find(data, "avih", 0) + 8
	var codec := _find(data, "strf", 0) + 8
	# The frames themselves: chunks in the "movi" list, pictures tagged 00db and sound 01wb.
	var list := _find(data, "movi", 0)
	var at := list + 4
	var end := list - 4 + data.decode_u32(list - 4)
	var pictures := 0
	var sounds := 0
	while at + 8 <= end:
		var tag := data.slice(at, at + 4).get_string_from_ascii()
		var length := data.decode_u32(at + 4)
		pictures += 1 if tag == "00db" else 0
		sounds += 1 if tag == "01wb" else 0
		at += 8 + length + (length & 1)
	print("AVI header: %d frames of %d x %d, %.3f frames a second, compression %s; the file holds %d pictures and %d pieces of sound in %d bytes" % [
		data.decode_u32(header + 16), data.decode_u32(header + 32), data.decode_u32(header + 36),
		1e6 / data.decode_u32(header), data.slice(codec + 16, codec + 20).get_string_from_ascii(), pictures, sounds, data.size()])


## Where the four letters of `tag` next appear in `data` from `from` on, or -1.
func _find(data: PackedByteArray, tag: String, from: int) -> int:
	var wanted := tag.to_ascii_buffer()
	var at := data.find(wanted[0], from)
	while at >= 0 and at + 4 <= data.size():
		if data.slice(at, at + 4) == wanted:
			return at
		at = data.find(wanted[0], at + 1)
	return -1
