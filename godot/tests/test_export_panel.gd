extends LimnerTest
## The Export panel: its three sections ask for the right things, and a 3D file of
## every format is written with what its report says.


func _scratch(file_name: String) -> String:
	var dir := OS.get_cache_dir().path_join("limner-tests")
	DirAccess.make_dir_recursive_absolute(dir)
	return dir.path_join("%s-%d-%s" % [Time.get_ticks_usec(), randi(), file_name])


func _remove(path: String) -> void:
	if DirAccess.dir_exists_absolute(path):
		for file in DirAccess.get_files_at(path):
			DirAccess.remove_absolute(path.path_join(file))
		for dir in DirAccess.get_directories_at(path):
			_remove(path.path_join(dir))
	DirAccess.remove_absolute(path)


func _panel() -> ExportPanel:
	var panel := ExportPanel.new(editor)
	mount(panel)
	await frames(2)
	return panel


func _choose(menu: Cycler, index: int) -> void:
	menu.select(index)
	menu.item_selected.emit(index)
	await frames(2)


func _type(field: LineEdit, text: String) -> void:
	field.text = text
	field.text_submitted.emit(text)
	await frames(2)


func _add_camera(layer_name: String) -> int:
	var added: Dictionary = run({"cmd": "add_layer", "kind": "camera", "name": layer_name})
	return int(added.layer)


func test_one_section_is_shown_at_a_time() -> void:
	var panel := await _panel()
	check_eq([panel.image.visible, panel.video.visible, panel.model.visible], [true, false, false], "it opens on the picture")
	check_eq(panel._title.text, "CAPTURE IMAGE", "with its heading")
	for key: String in ["video", "model", "image"]:
		var button: Button = panel._chooser.buttons[key]
		button.pressed.emit()
		await frames(2)
		check_eq([panel.image.visible, panel.video.visible, panel.model.visible], [key == "image", key == "video", key == "model"], "the %s section alone" % key)
		check_fits(panel, key)
	check_eq(panel._title.text, "CAPTURE IMAGE", "and back")
	for icon: String in ["image", "film", "model", "export", "folder_open", "play", "camera", "left", "right"]:
		check(Icons.has(icon), "there is an icon named %s" % icon)


func test_the_picture_section_asks_for_what_its_controls_say() -> void:
	var panel := await _panel()
	var section := panel.image
	var camera := _add_camera("Wide")
	await frames(2)
	var asked := section.request()
	check_eq([asked.kind, asked.layer, asked.size, asked.quality, asked.transparent], ["flat", -1, Vector2i(1920, 1080), "good", false], "a flat picture of the current view at the document's size")
	check_eq([section._size_row.visible, section._width_row.visible, section._eyes_row.visible], [true, false, false], "with a width and a height")
	check_eq(section._from.item_count, 2, "from the view or the camera")
	check_eq(section._from.get_item_text(1), "Wide", "which is listed by name")

	await _choose(section._from, 1)
	await _choose(section._size._menu, 3)
	var draft: Button = section._quality.buttons["draft"]
	draft.pressed.emit()
	section._transparent.button_pressed = true
	asked = section.request()
	check_eq([asked.layer, asked.size, asked.quality, asked.transparent], [camera, Vector2i(1024, 1024), "draft", true], "the camera, a listed size, Draft, transparent")
	check_eq(section._quality_note.text, LimnerCapture.QUALITY_NOTES.draft, "the note says what Draft trades")

	await _choose(section._size._menu, section._size._menu.item_count - 1)
	await _type(section._size._width, "99999")
	await _type(section._size._height, "5")
	check_eq(section.request().size, Vector2i(8192, 16), "a typed size is kept within 16 and 8192")

	await _choose(section._kind, 2)
	asked = section.request()
	check_eq([asked.kind, asked.size], ["equirect_stereo", Vector2i(2048, 2048)], "a stereo 360 picture, two to one twice")
	check_near(asked.eye_separation, 0.064, "with eyes 64 mm apart")
	check_eq([section._size_row.visible, section._width_row.visible, section._eyes_row.visible], [false, true, true], "it has one size and an eye distance")
	check_eq(section._from.item_count, 3, "and can be taken from the spawn area as well")
	check_eq(asked.layer, camera, "the camera is still chosen")
	await _type(section._eyes, "0.07")
	check_near(section.request().eye_separation, 0.07, "the eye distance typed")
	await _choose(section._kind, 3)
	check_eq(section.request().size, Vector2i(3072, 512), "a row of six faces")
	await _choose(section._width, 0)
	await _choose(section._kind, 4)
	check_eq(section.request().size, Vector2i(1024, 768), "an unfolded cube, at the size chosen first in the list")
	await _choose(section._kind, 1)
	check_eq(section.request().size, Vector2i(1024, 512), "an equirectangular picture")

	run({"cmd": "delete_layers", "layers": [camera]})
	await frames(2)
	check_eq(section.request().layer, -1, "when the camera goes, the current view is what is left")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_capture_writes_a_png_of_the_size_asked_for() -> void:
	var panel := await _panel()
	var notes: Array[String] = []
	editor.message.connect(func(text: String, _is_error: bool) -> void: notes.append(text))
	if not LimnerCapture.can_render():
		print("     skipped: capturing needs a display; run the tests without --headless to include it")
		check_eq(await panel.image.capture(), "", "without a display nothing is written")
		check_eq(notes, ["Pictures cannot be rendered without a display"] as Array[String], "and the user is told why")
		return
	LimnerDemo.run(editor)
	var folder := _scratch("pictures")
	panel.image._target.set_path(folder.path_join("sheet.png"))
	panel.image._size.set_custom(Vector2i(300, 200))
	var draft: Button = panel.image._quality.buttons["draft"]
	draft.pressed.emit()
	var path: String = await panel.image.capture()
	check_eq(path, folder.path_join("sheet.png"), "the file named")
	check_eq(Image.load_from_file(path).get_size(), Vector2i(300, 200), "is a PNG of the size asked for")
	check_eq(notes, ["Saved %s" % path] as Array[String], "and the user is told where it is")
	check(panel.image._result.text.contains("300 × 200"), "the panel says what was made: %s" % panel.image._result.text)
	check(not panel.image._capture.disabled, "the button is ready again")

	await _choose(panel.image._kind, 1)
	await _choose(panel.image._width, 0)
	panel.image._target.set_path(folder.path_join("around.png"))
	path = await panel.image.capture()
	check_eq(Image.load_from_file(path).get_size(), Vector2i(1024, 512), "a 360 picture of its size")
	_remove(folder)


func test_the_video_section_asks_for_what_its_controls_say() -> void:
	var panel := await _panel()
	var section := panel.video
	var asked := section.settings()
	check_eq([asked.size, asked.fps, asked.quality, asked.projection, asked.sound, asked.camera, asked.range], [Vector2i(1920, 1080), 24.0, "good", "flat", true, "view", [0.0, 10.0]], "the current view over the working range, as the document is set")
	check(String(asked.output).get_base_dir().ends_with("Movies/Limner") or String(asked.output).get_base_dir().get_file() == "Limner", "into a folder named Limner: %s" % asked.output)
	check_eq(String(asked.output).get_extension(), "avi", "as an AVI")
	check(not section._times.visible, "no boxes for the range of the document")
	check_eq(section._rate.get_item_text(0), "Document (24 fps)", "the document's rate is named")

	var camera := _add_camera("Shot")
	run({"cmd": "set_document", "rate": {"num": 12, "den": 1}, "range_end": LimnerEditor.to_flicks(4.0), "camera_resolution": [640, 360]})
	await frames(2)
	await _choose(section._camera, 1)
	asked = section.settings()
	check_eq([asked.camera, asked.fps, asked.range, asked.size], [str(camera), 12.0, [0.0, 4.0], Vector2i(640, 360)], "it follows the document")

	var custom: Button = section._range.buttons["custom"]
	custom.pressed.emit()
	await frames(2)
	check(section._times.visible, "a custom range has boxes")
	await _type(section._start_seconds, "1")
	await _type(section._end_frames, "30")
	check_eq(section.settings().range, [1.0, 2.5], "typed in seconds and in frames")
	check_eq([section._start_frames.text, section._end_seconds.text], ["12", "2.5"], "each shown both ways")
	await _type(section._end_seconds, "0.5")
	check_eq(section.settings().range, [1.0, 2.5], "an end before the start is refused")
	check_eq(messages, ["The range must end after it starts"] as Array[String], "with the reason")
	messages.clear()

	await _choose(section._rate, DocumentPanel.RATES.find(30) + 1)
	check_eq(section.settings().fps, 30.0, "another frame rate")
	check_eq(section._end_frames.text, "75", "frames are counted at the video's rate")
	await _choose(section._projection, 1)
	check_eq(section.settings().size, Vector2i(640, 320), "a 360 video takes its width and is two to one")
	check_eq(section.settings().projection, "equirect", "equirectangular")
	await _choose(section._format, 1)
	check_eq(String(section.settings().output).get_extension(), "png", "a PNG sequence")
	check_eq(section._format_note.text, RenderLauncher.FORMAT_NOTES.png, "with what it is good for")
	await _choose(section._format, 2)
	check_eq(String(section.settings().output).get_extension(), "ogv", "an OGV")
	section._sound.button_pressed = false
	var best: Button = section._quality.buttons["best"]
	best.pressed.emit()
	await _type(section._target._name, "take one")
	asked = section.settings()
	check_eq([asked.sound, asked.quality, String(asked.output).get_file(), asked.minimized], [false, "best", "take one.ogv", true], "silent, Best, named")

	var line := RenderLauncher.command_line("/work/piece.limnz", asked, "/tmp/status.json")
	var read := RenderJob.parse(line.slice(line.find("--") + 1))
	check_eq([read.error, read.camera, read.range, read.fps, read.size, read.projection, read.sound], ["", str(camera), [1.0, 2.5], 30.0, Vector2i(640, 320), "equirect", false], "the render process is told the same")
	check(section._result.text.contains("later changes are not in it"), "the panel says that later edits are not rendered")
	check(not section._progress_row.visible, "no progress is shown before a render")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_preview_plays_the_range_through_the_camera() -> void:
	var head := Camera3D.new()
	head.position = DesktopShell.HEAD
	world.add_child(head)
	var panel := await _panel()
	var section := panel.video
	var camera := _add_camera("Shot")
	var placed: Dictionary = world.scene.layer_details(camera).transform
	placed.translation = [2.0, 1.0, -3.0]
	run({"cmd": "set_layer", "layer": camera, "transform": placed})
	run({"cmd": "set_document", "range_start": LimnerEditor.to_flicks(1.0), "range_end": LimnerEditor.to_flicks(1.5)})
	await frames(2)
	await _choose(section._camera, 1)

	section._preview.button_pressed = true
	await frames(2)
	check_eq(editor.status.playing, true, "the preview plays")
	check(float(editor.status.seconds) >= 1.0 and float(editor.status.seconds) < 1.5, "inside the range: %s" % editor.status.seconds)
	check(world.scene.layer_transform(camera).origin.is_equal_approx(DesktopShell.HEAD), "seen from the camera: %s" % world.scene.layer_transform(camera).origin)
	check_near(head.fov, 45.0, "through its lens")
	# Half a second at 24 frames a second is over in a moment; it starts again.
	run({"cmd": "seek", "time": LimnerEditor.to_flicks(1.49)})
	await frames(4)
	check_eq(editor.status.playing, true, "it goes round")
	check(float(editor.status.seconds) < 1.4, "from the start: %s" % editor.status.seconds)

	section._preview.button_pressed = false
	await frames(2)
	check_eq(editor.status.playing, false, "switching it off stops playing")
	check_near(head.fov, 75.0, "and gives the view its own lens back")
	section._preview.button_pressed = true
	await frames(2)
	run({"cmd": "pause"})
	await frames(3)
	check_eq(section._preview.button_pressed, false, "pausing elsewhere ends the preview")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_3d_section_sends_only_the_options_that_apply() -> void:
	var panel := await _panel()
	var section := panel.model
	run({"cmd": "seek", "time": LimnerEditor.to_flicks(2.0)})
	await frames(2)
	check_eq(section.format_chosen(), "glb", "glTF comes first")
	check_eq(section.options(), {"animation": false, "include_hidden": false, "start": LimnerEditor.to_flicks(2.0), "bake_transforms": false, "lights": true, "cameras": true, "gltf_node_visibility": false, "gltf_animate_opacity": false}, "a still at the playhead")
	check_eq([section._gltf_rows.visible, section._fbx_rows.visible, section._gltf_opacity.visible], [true, false, false], "with glTF's own options")

	var animated: Button = section._moving.buttons["animated"]
	animated.pressed.emit()
	section._gltf_opacity.button_pressed = true
	section._gltf_visibility.button_pressed = true
	section._hidden.button_pressed = true
	section._bake.button_pressed = true
	section._lights.button_pressed = false
	await frames(2)
	check_eq(section.options(), {"animation": true, "include_hidden": true, "bake_transforms": true, "lights": false, "cameras": true, "gltf_node_visibility": true, "gltf_animate_opacity": true}, "animated: the working range, so no time is sent")
	check(section._gltf_opacity.visible, "opacity can be animated")

	await _choose(section._format, 3)
	check_eq(section.format_chosen(), "fbx", "FBX")
	check_eq([section._gltf_rows.visible, section._fbx_rows.visible], [false, true], "has a unit and a colour encoding instead")
	var centimetres: Button = section._fbx_unit.buttons["centimetres"]
	centimetres.pressed.emit()
	var linear: Button = section._fbx_colors.buttons["linear"]
	linear.pressed.emit()
	var sent := section.options()
	check_eq([sent.fbx_unit, sent.fbx_colors, sent.has("gltf_node_visibility")], ["centimetres", "linear", false], "which are sent, and glTF's are not")

	await _choose(section._format, 4)
	check_eq(section.format_chosen(), "obj", "OBJ")
	check_eq(section.options(), {"animation": false, "include_hidden": true, "start": LimnerEditor.to_flicks(2.0)}, "is a still whatever was chosen")
	check_eq([section._moving_row.visible, section._bake.visible, section._lights.visible, section._cameras.visible, section._fbx_rows.visible], [false, false, false, false, false], "and shows nothing it cannot hold")
	for index in ExportModelSection.FORMATS.size():
		await _choose(section._format, index)
		check_eq(section._format_note.text, ExportModelSection.FORMATS[index][2], "what %s is good for" % section.format_chosen())


func test_every_format_is_written_with_what_its_report_says() -> void:
	var panel := await _panel()
	var section := panel.model
	LimnerDemo.run(editor)
	_add_camera("Shot")
	var notes: Array[String] = []
	editor.message.connect(func(text: String, _is_error: bool) -> void: notes.append(text))
	var folder := _scratch("models")
	for index in ExportModelSection.FORMATS.size():
		await _choose(section._format, index)
		var format := section.format_chosen()
		section._target.set_path(folder.path_join("sheet." + format))
		var reply: Dictionary = await section.export_file()
		check_eq([reply.ok, reply.error], [true, ""], "%s is written" % format)
		check_eq(reply.path, folder.path_join("sheet." + format), "where it was asked for")
		check(FileAccess.file_exists(reply.path), "the %s file is there" % format)
		for count: String in ["nodes", "meshes", "vertices", "triangles", "bytes"]:
			check(int(reply[count]) > 0, "%s reports its %s: %s" % [format, count, reply[count]])
		check_eq(int(reply.animation_frames), 0, "a still has no animation")
		check_eq(int(reply.bytes), FileAccess.get_file_as_bytes(reply.path).size(), "the size reported is the size of the %s file" % format)
		check(section._report.text.contains("vertices") and section._report.text.contains(reply.path), "the panel shows the report: %s" % section._report.text)
		check_eq(notes.back(), "Exported %s" % reply.path, "and tells the user")
	check_eq(int(world.scene.export_file(folder.path_join("sheet.obj"), {}).vertices), 5492, "every vertex of the sheet")

	await _choose(section._format, 0)
	var animated: Button = section._moving.buttons["animated"]
	animated.pressed.emit()
	run({"cmd": "set_document", "range_end": LimnerEditor.to_flicks(0.5)})
	section._target.set_path(folder.path_join("moving.glb"))
	var moving: Dictionary = await section.export_file()
	check_eq([moving.ok, int(moving.animation_frames)], [true, 13], "half a second at 24 frames a second, both ends included")
	check(section._report.text.contains("13 frames of animation"), "which the report says: %s" % section._report.text)
	_remove(folder)
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_what_a_format_leaves_out_is_shown() -> void:
	var panel := await _panel()
	var section := panel.model
	LimnerDemo.run(editor)
	var folder := _scratch("sound")
	DirAccess.make_dir_recursive_absolute(folder)
	editor.import_file(LayerTestKit.wav(folder.path_join("tone.wav"), 0.3, 1))
	await _choose(section._format, 0)
	check_eq(section.format_chosen(), "glb", "glTF first")
	section._target.set_path(folder.path_join("with_sound.glb"))
	var reply: Dictionary = await section.export_file()
	check(reply.ok, "written: %s" % reply.error)
	var warnings: PackedStringArray = reply.get("warnings", PackedStringArray())
	check_eq(warnings.size(), 1, "glTF holds no sound, and says so: %s" % [warnings])
	if warnings.size() == 1:
		check(section._report.text.contains(warnings[0]), "the panel shows it: %s" % section._report.text)
	_remove(folder)
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_an_export_that_cannot_be_made_says_why() -> void:
	LimnerDemo.run(editor)
	var folder := _scratch("refused")
	DirAccess.make_dir_recursive_absolute(folder)
	var scene := world.scene
	var unknown := scene.export_file(folder.path_join("a.glb"), {"sparkle": true})
	check_eq([unknown.ok, unknown.error], [false, "unknown export option \"sparkle\""], "an unknown option is named")
	check_eq([int(unknown.nodes), int(unknown.bytes)], [0, 0], "and nothing is counted")
	check(not FileAccess.file_exists(folder.path_join("a.glb")), "or written")
	check(scene.export_file(folder.path_join("a.stl"), {}).error.contains(".glb, .obj, .usda, .usdz or .fbx"), "an unknown format lists the known ones")
	check(scene.export_file(folder.path_join("a.fbx"), {"fbx_unit": "inches"}).error.contains("\"metres\" or \"centimetres\""), "a wrong unit lists the right ones")
	check(scene.export_file(folder.path_join("a.fbx"), {"fbx_colors": 3}).error.contains("\"srgb\" or \"linear\""), "a wrong colour encoding too")
	check(scene.export_file(folder.path_join("a.glb"), {"animation": "yes"}).error.contains("\"animation\" must be true or false"), "a wrong kind of value is named")
	check(scene.export_file(folder.path_join("a.glb"), {"animation": true, "start": 100, "end": 50}).error.contains("before it starts"), "a range that ends before it starts")
	check(scene.export_file(folder.path_join("a.glb"), {"rate": 0}).error.contains("\"rate\""), "a rate of nothing")
	check(scene.export_file(folder.path_join("missing").path_join("a.glb"), {}).error.contains("could not write"), "a folder that is not there")
	var sampled := scene.export_file(folder.path_join("b.glb"), {"animation": true, "start": 0, "end": LimnerEditor.to_flicks(1.0), "rate": {"num": 12, "den": 1}})
	check_eq([sampled.ok, int(sampled.animation_frames)], [true, 13], "a range and a rate given outright")
	check_eq(int(scene.export_file(folder.path_join("c.glb"), {"animation": true, "end": LimnerEditor.to_flicks(1.0), "rate": 6}).animation_frames), 7, "a rate as a plain number")
	_remove(folder)


func test_a_copy_for_rendering_leaves_the_document_unsaved() -> void:
	LimnerDemo.run(editor)
	await frames(2)
	check_eq(editor.status.dirty, true, "something unsaved")
	var path := _scratch("copy.limnz")
	var saved := world.scene.save_copy(path)
	check_eq([saved.ok, saved.error], [true, ""], "the copy is written")
	check(int(saved.bytes_written) > 0 and FileAccess.file_exists(path), "as a file")
	await frames(2)
	check_eq(world.scene.status().dirty, true, "the document still counts as unsaved")
	check_eq(world.scene.info().path, "", "and has not moved to the copy")
	var refused := world.scene.save_copy(path.path_join("inside-a-file.limnz"))
	check(not refused.ok and refused.error != "", "a copy that cannot be written says why: %s" % refused.error)
	check(editor.open_document(path), "the copy opens")
	await frames(2)
	check_eq(row_named("Paint").strokes, 28, "with every stroke")
	_remove(path)
