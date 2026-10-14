extends LimnerTest
## Capture: where pictures are taken from, their sizes and names, the options of a
## video render, and, where there is a display to render with, the pictures themselves.


func _scratch(file_name: String) -> String:
	var dir := OS.get_cache_dir().path_join("limner-tests")
	DirAccess.make_dir_recursive_absolute(dir)
	return dir.path_join("%s-%d-%s" % [Time.get_ticks_usec(), randi(), file_name])


## True when pictures can be rendered. Says why not, once per test, when they cannot.
func _can_render(what: String) -> bool:
	if LimnerCapture.can_render():
		return true
	print("     skipped: %s needs a display; run the tests without --headless to include it" % what)
	return false


## True when two screen colours are the same to within what eight bits can hold.
func _same_color(a: Color, b: Color) -> bool:
	return absf(a.r - b.r) < 0.01 and absf(a.g - b.g) < 0.01 and absf(a.b - b.b) < 0.01


func _add_camera(layer_name: String, translation: Array, scale := 1.0) -> int:
	var added: Dictionary = run({"cmd": "add_layer", "kind": "camera", "name": layer_name})
	var id := int(added.layer)
	var placed: Dictionary = world.scene.layer_details(id).transform
	placed.translation = translation
	placed.scale = scale
	run({"cmd": "set_layer", "layer": id, "transform": placed})
	return id


func test_a_camera_layer_gives_its_place_its_direction_and_its_lens() -> void:
	var capture := LimnerCapture.of(editor)
	var id := _add_camera("Shot", [1.0, 2.0, 3.0], 2.0)
	var placed: Dictionary = world.scene.layer_details(id).transform
	# A quarter turn to the left: the camera now looks along -X.
	placed.rotation = [0.0, sin(PI / 4.0), 0.0, cos(PI / 4.0)]
	run({"cmd": "set_layer", "layer": id, "transform": placed})
	run({"cmd": "set_layer_kind", "layer": id, "values": {"fov": 30.0}})
	await frames(2)
	var view := capture.layer_view(id)
	var at: Transform3D = view.transform
	check(at.origin.is_equal_approx(Vector3(1.0, 2.0, 3.0)), "the place: %s" % at.origin)
	check((-at.basis.z).is_equal_approx(Vector3.LEFT), "the direction: %s" % (-at.basis.z))
	check_near(at.basis.x.length(), 1.0, "the layer's scale is not in the view")
	check_near(view.fov, 30.0, "the lens")
	check_near(view.near, 0.02, "the scale moves the near plane")
	check_near(view.far, 2000.0, "and the far plane")
	check_eq(LimnerCapture.of(editor), capture, "one capture per editor")

	# A mirrored camera still gives an ordinary picture.
	run({"cmd": "flip_layer", "layer": id, "axis": "x"})
	await frames(2)
	check(capture.layer_view(id).transform.basis.determinant() > 0.0, "a flipped camera is not mirrored")


func test_a_floor_level_spawn_area_gives_the_height_of_standing_eyes() -> void:
	var capture := LimnerCapture.of(editor)
	var area := int(row_named("Spawn Area").id)
	run({"cmd": "set_layer_kind", "layer": area, "values": {"floor_level": false}})
	var at_eye: Transform3D = capture.layer_view(area).transform
	run({"cmd": "set_layer_kind", "layer": area, "values": {"floor_level": true}})
	var from_floor: Transform3D = capture.layer_view(area).transform
	check_near(from_floor.origin.y - at_eye.origin.y, LimnerCapture.EYE_HEIGHT, "eyes above a floor-level area")
	check_near(capture.layer_view(area).fov, LimnerCapture.DEFAULT_FOV, "a spawn area has no lens of its own")


func test_a_view_is_levelled_for_a_360_picture() -> void:
	var tilted := Basis.from_euler(Vector3(deg_to_rad(-35.0), deg_to_rad(70.0), deg_to_rad(20.0)))
	var view := {"transform": Transform3D(tilted, Vector3(4.0, 5.0, 6.0)), "fov": 50.0, "near": 0.01, "far": 100.0}
	var upright: Transform3D = LimnerCapture.level(view).transform
	check(upright.origin.is_equal_approx(Vector3(4.0, 5.0, 6.0)), "the place stays")
	check(upright.basis.y.is_equal_approx(Vector3.UP), "up is up: %s" % upright.basis.y)
	var ahead := -tilted.z
	ahead.y = 0.0
	check((-upright.basis.z).is_equal_approx(ahead.normalized()), "the compass direction stays")
	check((view.transform as Transform3D).basis.is_equal_approx(tilted), "the view given is left alone")
	# With no camera in the window, the current view is the desktop's own.
	var current := LimnerCapture.of(editor).current_view()
	check((current.transform as Transform3D).origin.is_equal_approx(DesktopShell.HEAD), "the default view is from the desktop head")


func test_360_pictures_have_the_shape_of_their_layout() -> void:
	check_eq(LimnerCapture.panorama_size("equirect", 4096), Vector2i(4096, 2048), "equirectangular is two to one")
	check_eq(LimnerCapture.panorama_size("equirect_stereo", 4096), Vector2i(4096, 4096), "stereo stacks two")
	check_eq(LimnerCapture.panorama_size("cube_strip", 1536), Vector2i(1536, 256), "six faces in a row")
	check_eq(LimnerCapture.panorama_size("cube_cross", 1024), Vector2i(1024, 768), "a cross is four faces by three")
	for layout: String in LimnerCapture.LAYOUTS:
		var size := LimnerCapture.panorama_size(layout, LimnerCapture.MAX_SIZE)
		check(size.x <= LimnerCapture.MAX_SIZE and size.y <= LimnerCapture.MAX_SIZE, "%s stays within the largest picture" % layout)
	for face: String in CaptureRig.FACES:
		var axes: Basis = LimnerCapture.FACE_AXES[face]
		check(axes.is_equal_approx(axes.orthonormalized()) and axes.determinant() > 0.0, "the %s face has proper axes" % face)
		check(LimnerCapture.STRIP_CELLS.has(face) and LimnerCapture.CROSS_CELLS.has(face), "the %s face has its place in both sheets" % face)
	check((-(LimnerCapture.FACE_AXES["right"] as Basis).z).is_equal_approx(Vector3.RIGHT), "the right face looks right")
	check((-(LimnerCapture.FACE_AXES["up"] as Basis).z).is_equal_approx(Vector3.UP), "the up face looks up")


func test_files_are_named_after_the_document_and_the_time() -> void:
	var pattern := RegEx.create_from_string("^Night garden_\\d{8}_\\d{6}\\.png$")
	check(pattern.search(LimnerCapture.stamped_name("Night garden", "png")) != null, "title, date and time: %s" % LimnerCapture.stamped_name("Night garden", "png"))
	check(LimnerCapture.stamped_name("", "png").begins_with("Untitled_"), "a document without a title")
	check(not LimnerCapture.stamped_name("a/b:c?", "png").contains("/"), "nothing a file name cannot hold")
	check_eq(LimnerCapture.default_folder().get_file(), "Limner", "pictures go to a folder named Limner")
	for quality: String in LimnerCapture.QUALITIES:
		check(LimnerCapture.QUALITY_NOTES.has(quality), "%s says what it trades" % quality)
	check_eq(int(LimnerCapture.QUALITIES.draft.passes), 1, "Draft is one pass")
	check_eq(int(LimnerCapture.QUALITIES.good.passes), 8, "Good is eight")
	check_eq(int(LimnerCapture.QUALITIES.best.passes), 32, "Best is thirty-two")


func test_render_options_are_read_and_checked() -> void:
	var read := RenderJob.parse(PackedStringArray(["--render", "--open", "piece.limn", "--camera", "Root/Shot", "--range", "1.5", "4", "--fps", "12", "--size", "640x360", "--quality", "best", "--projection", "equirect", "--no-sound", "--out", "piece.avi", "--status", "s.json", "--minimized"]))
	check_eq(read.error, "", "a full command line")
	check_eq([read.open, read.camera, read.range, read.fps, read.size], ["piece.limn", "Root/Shot", [1.5, 4.0], 12.0, Vector2i(640, 360)], "what was asked for")
	check_eq([read.quality, read.projection, read.sound, read.out, read.status, read.minimized], ["best", "equirect", false, "piece.avi", "s.json", true], "and how")

	var plain := RenderJob.parse(PackedStringArray(["--render", "--open", "piece.limn"]))
	check_eq([plain.error, plain.quality, plain.projection, plain.sound], ["", "good", "flat", true], "defaults")
	check(not plain.has("camera") and not plain.has("range") and not plain.has("fps") and not plain.has("size"), "what is left out is decided from the document")

	for bad: Array in [
		[["--render"], "--open"],
		[["--render", "--open"], "needs a value"],
		[["--render", "--open", "p", "--quality", "fine"], "draft, good or best"],
		[["--render", "--open", "p", "--projection", "fisheye"], "flat, equirect or equirect_stereo"],
		[["--render", "--open", "p", "--range", "2"], "a start and an end"],
		[["--render", "--open", "p", "--range", "2", "1"], "an end after it"],
		[["--render", "--open", "p", "--range", "a", "b"], "an end after it"],
		[["--render", "--open", "p", "--fps", "0"], "between 1 and 480"],
		[["--render", "--open", "p", "--size", "640"], "<width>x<height>"],
		[["--render", "--open", "p", "--size", "9000x100"], "16 to 8192"],
		[["--render", "--open", "p", "--loud"], "unknown render option --loud"],
	]:
		var error: String = RenderJob.parse(PackedStringArray(bad[0])).error
		check(error.contains(bad[1]), "%s is refused: %s" % [bad[0], error])


func test_the_launcher_passes_engine_and_render_options() -> void:
	var settings := {"output": "/films/take.avi", "size": Vector2i(1920, 1080), "fps": 12.0, "quality": "best", "projection": "flat", "sound": false, "camera": "7", "range": [0.5, 2.0], "minimized": true}
	var line := RenderLauncher.command_line("/work/piece.limnz", settings, "/tmp/status.json")
	var split := line.find("--")
	var engine := line.slice(0, split)
	var render := line.slice(split + 1)
	check_eq(engine[engine.find("--write-movie") + 1], "/films/take.avi", "Godot is told the file")
	check_eq(engine[engine.find("--fixed-fps") + 1], "12", "and the frame rate")
	check(engine.has("--disable-vsync"), "and not to wait for the screen")
	check(engine.has("--path"), "a development build is told where the project is")
	check_eq(engine[engine.find("--resolution") + 1], "480x270", "the window is small; the picture is not")
	check_eq(render[0], "--render", "then Limner's own options")
	var read := RenderJob.parse(render)
	check_eq(read.error, "", "which the job understands")
	check_eq([read.open, read.camera, read.range, read.fps, read.size, read.quality, read.projection, read.sound, read.status, read.minimized], ["/work/piece.limnz", "7", [0.5, 2.0], 12.0, Vector2i(1920, 1080), "best", "flat", false, "/tmp/status.json", true], "all of them")

	check_eq(RenderLauncher.movie_path("/films/take.avi"), "/films/take.avi", "a video is one file")
	check_eq(RenderLauncher.movie_path("/films/take.png"), "/films/take/take.png", "a sequence gets a folder of its own")
	var sequence := RenderLauncher.command_line("/work/piece.limnz", {"output": "/films/take.png", "size": Vector2i(320, 180), "fps": 23.976}, "/tmp/s.json")
	check_eq(sequence[sequence.find("--write-movie") + 1], "/films/take/take.png", "which Godot is given")
	check_eq(sequence[sequence.find("--fixed-fps") + 1], "24", "Godot takes whole frame rates")
	check_eq(sequence[sequence.find("--fps") + 1], "23.976", "the job takes the exact one")
	check(not sequence.has("--camera") and not sequence.has("--range") and not sequence.has("--no-sound"), "what is not set is not passed")

	check(RenderLauncher.override_text(Vector2i(320, 180)).begins_with(RenderLauncher.OVERRIDE_MARK), "the settings file is marked as ours")
	check(RenderLauncher.override_text(Vector2i(320, 180)).contains("window/size/window_width_override=320\nwindow/size/window_height_override=180"), "and sets the size of the movie")
	check_eq(RenderLauncher.movie_size(), Vector2i(1440, 900), "without it, movies have the size of the project's window")
	var launcher := RenderLauncher.new()
	add_child(launcher)
	check_eq(launcher.launch("/work/piece.limnz", {"output": "/films/take.mp4", "size": Vector2i(320, 180), "fps": 12.0}), "The file name must end in .avi, .png or .ogv", "other formats are refused before anything starts")
	check(not launcher.running, "and nothing runs")


func test_a_rate_that_is_not_whole_keeps_picture_and_sound_together() -> void:
	# Movie Maker writes whole frame rates only, one frame's worth of sound per frame.
	check_eq(RenderJob.movie_fps(PackedStringArray(["--write-movie", "/films/take.avi", "--fixed-fps", "24", "--disable-vsync"])), 24, "the rate Godot was given")
	check_eq(RenderJob.movie_fps(PackedStringArray(["--write-movie", "/films/take.avi"])), int(ProjectSettings.get_setting("editor/movie_writer/fps", 60)), "or the project's movie rate")
	var line := RenderLauncher.command_line("/work/piece.limnz", {"output": "/films/take.avi", "size": Vector2i(320, 180), "fps": 24000.0 / 1001.0}, "/tmp/s.json")
	var movie := RenderJob.movie_fps(line)
	check_eq(movie, 24, "23.976 is written as 24")
	var fps: float = RenderJob.parse(line.slice(line.find("--") + 1)).fps
	var speed := RenderJob.sound_speed(movie, fps)
	check_near(speed, 1.001, "so the sound plays 0.1% faster", 1e-4)
	# Over ten minutes of frames, the sound heard and the playhead stay together.
	var frames := roundi(600.0 * fps)
	var picture_seconds := frames / fps
	var sound_seconds := frames * (1.0 / movie) * speed
	check_near(sound_seconds, picture_seconds, "picture and sound agree after ten minutes", 1e-3)
	check_near(RenderJob.sound_speed(12, 12.0), 1.0, "a whole rate plays sound as it is")


func test_rendering_leaves_the_users_file_alone() -> void:
	run({"cmd": "add_strokes", "strokes": [{"points": [[0, 1, -1], [0.2, 1, -1]], "width": 0.01}]})
	var path := _scratch("kept.limnz")
	var picture := Image.create(8, 4, false, Image.FORMAT_RGB8)
	picture.fill(Color(0.2, 0.4, 0.6))
	check(editor.save_document(path, picture.save_png_to_buffer()), "a saved document with a picture")
	await frames(2)
	var before := FileAccess.get_file_as_bytes(path)
	var renders := ProjectSettings.globalize_path("user://renders")
	var copies_before := DirAccess.get_files_at(renders).size() if DirAccess.dir_exists_absolute(renders) else 0
	var launcher := RenderLauncher.new()
	add_child(launcher)
	# A file name that is refused stops the render after the document is put aside
	# for it and before any process starts.
	var problem := launcher.render_document(editor, {"output": _scratch("take.mp4"), "size": Vector2i(320, 180), "fps": 12.0})
	check(problem.contains(".avi"), "the render is refused: %s" % problem)
	check_eq(FileAccess.get_file_as_bytes(path), before, "the user's file is untouched, picture and all")
	check_eq(DirAccess.get_files_at(renders).size(), copies_before, "and the copy made for the render is gone")
	check_eq(editor.path, path, "the document is still that file")
	DirAccess.remove_absolute(path)


func test_a_still_has_the_size_asked_for_and_leaves_the_editor_as_it_was() -> void:
	if not _can_render("a still"):
		return
	LimnerDemo.run(editor)
	run({"cmd": "set_options", "values": {"show_extras": true, "onion_skin": true}})
	world.show_grid = true
	world.animate_dither = false
	await frames(2)
	var capture := LimnerCapture.of(editor)
	var view := {"transform": Transform3D(Basis.IDENTITY, Vector3(0.0, 1.45, 0.0)), "fov": 30.0, "near": 0.01, "far": 2000.0}

	var draft := capture.still(view, Vector2i(333, 217), "draft")
	check_eq(draft.get_size(), Vector2i(333, 217), "exactly the size asked for")
	check_eq(draft.get_format(), Image.FORMAT_RGB8, "no alpha without transparency")
	check_eq([world.show_grid, world.animate_dither], [true, false], "the grid and the dither are as they were")
	check_eq([world.scene.options().show_extras, world.scene.options().onion_skin], [true, true], "so are gizmos and onion skins")
	check_eq(capture.get_child_count(), 0, "the render targets are let go")
	var background := LimnerEditor.screen_color(editor.get_info().background)
	check(_same_color(draft.get_pixel(3, 3), background), "the corner shows the background and no grid: %s" % draft.get_pixel(3, 3))

	var path := _scratch("still.png")
	check_eq(LimnerCapture.save_png(draft, path), OK, "it is written")
	check_eq(Image.load_from_file(path).get_size(), Vector2i(333, 217), "as a PNG of that size")
	DirAccess.remove_absolute(path)

	var clear := capture.still(view, Vector2i(320, 180), "good", true)
	check_eq(clear.get_format(), Image.FORMAT_RGBA8, "a transparent picture has alpha")
	check_near(clear.get_pixel(3, 3).a, 0.0, "nothing where there is no artwork")
	var covered := 0
	var partly := 0
	for y in clear.get_height():
		for x in clear.get_width():
			var alpha := clear.get_pixel(x, y).a
			covered += 1 if alpha > 0.99 else 0
			partly += 1 if alpha > 0.2 and alpha < 0.8 else 0
	check(covered > 500, "opaque strokes are fully covered (%d pixels)" % covered)
	check(partly > 200, "see-through strokes are partly covered (%d pixels)" % partly)

	# A picture larger than one shot is put together from tiles.
	var wide := capture.still(view, Vector2i(2600, 120), "draft")
	check_eq(wide.get_size(), Vector2i(2600, 120), "a picture in two tiles")
	check(wide.get_pixel(2047, 2).is_equal_approx(wide.get_pixel(2048, 2)), "without a seam in the background")


func test_more_passes_make_see_through_strokes_smooth() -> void:
	if not _can_render("comparing qualities"):
		return
	LimnerDemo.run(editor)
	await frames(2)
	var capture := LimnerCapture.of(editor)
	# Close on the see-through chevrons at the right of the sheet.
	var view := {"transform": Transform3D(Basis.IDENTITY, Vector3(0.45, 1.45, -0.6)), "fov": 30.0, "near": 0.01, "far": 2000.0}
	var roughness := {}
	for quality: String in LimnerCapture.QUALITIES:
		var image := capture.still(view, Vector2i(240, 240), quality)
		# How much neighbouring pixels differ, summed over the picture.
		var total := 0.0
		for y in image.get_height():
			for x in image.get_width() - 1:
				total += absf(image.get_pixel(x, y).get_luminance() - image.get_pixel(x + 1, y).get_luminance())
		roughness[quality] = total
	check(roughness.good < roughness.draft * 0.5, "Good is much smoother than Draft: %s" % roughness)
	check(roughness.best < roughness.good, "and Best smoother still: %s" % roughness)


func test_360_pictures_are_rendered_in_every_layout() -> void:
	if not _can_render("a 360 picture"):
		return
	LimnerDemo.run(editor)
	await frames(2)
	var capture := LimnerCapture.of(editor)
	var view := {"transform": Transform3D(Basis.IDENTITY, Vector3(0.0, 1.45, 0.0)), "fov": 60.0, "near": 0.01, "far": 2000.0}
	var background := LimnerEditor.screen_color(editor.get_info().background)
	for layout: String in LimnerCapture.LAYOUTS:
		var width := 768 if layout == "cube_strip" else 512
		var image := capture.panorama(view, layout, width, "draft")
		check_eq(image.get_size(), LimnerCapture.panorama_size(layout, width), "the size of %s" % layout)
	# The sheet is ahead of the viewer, so it is in the middle of an equirectangular picture.
	var around := capture.panorama(view, "equirect", 512, "draft")
	var painted := 0
	for y in range(108, 148):
		for x in range(216, 296):
			if not _same_color(around.get_pixel(x, y), background):
				painted += 1
	check(painted > 50, "what is ahead is in the middle (%d painted pixels)" % painted)
	check(_same_color(around.get_pixel(5, 128), background), "and not behind")
	check_eq(world.show_grid, true, "the grid is back")
