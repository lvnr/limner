extends LimnerTest
## The Performance panel holds the scene against a kind of device.

var _dir := ""


## The tests play tones; nobody running them wants to hear that.
func _enter_tree() -> void:
	AudioServer.set_bus_mute(0, true)


func _exit_tree() -> void:
	AudioServer.set_bus_mute(0, false)
	if _dir != "":
		LayerTestKit.remove(_dir)


## A sound layer playing a tone of `seconds`.
func _sound(seconds: float) -> int:
	if _dir == "":
		_dir = LayerTestKit.scratch_dir()
	var path := LayerTestKit.wav(_dir.path_join("tone_%d.wav" % randi()), seconds, 1)
	var result := world.scene.import_file(path)
	check(result.ok, "the tone imports: %s" % result.error)
	return int(result.layer)


func _panel() -> PerformancePanel:
	var panel := PerformancePanel.new(editor)
	mount(panel)
	await frames(2)
	return panel


func test_it_counts_what_is_in_the_document() -> void:
	var panel := await _panel()
	run({"cmd": "add_strokes", "strokes": [
		{"brush": "cylinder", "points": [[0, 1, -1], [0.1, 1, -1], [0.2, 1, -1]], "width": 0.01},
		{"points": [[0, 1.2, -1], [0.2, 1.2, -1]], "width": 0.01},
	]})
	await frames(3)
	var now := panel.readings()
	check_eq(now.strokes, 2, "strokes")
	check(now.points >= 5, "points: %d" % now.points)
	check(now.triangles > 0, "triangles on screen: %d" % now.triangles)
	check_eq(now.layers_shown, 1, "paint layers showing")
	panel._read()
	check(panel._counts.text.contains("2 strokes"), "the count is written out: %s" % panel._counts.text)


func test_frame_time_is_measured_after_the_panel_settles() -> void:
	var panel := await _panel()
	check_eq(panel.readings().frame_ms, 0.0, "nothing measured at first")
	check_eq(panel._frame._reading.text, "measuring", "and the reading says so")
	panel._process(0.25)
	panel._process(0.25)
	check_eq(panel.readings().frame_ms, 0.0, "the first half second is not measured")
	for step in 25:
		panel._process(0.02)
	check_near(panel.readings().frame_ms, 20.0, "the average of the next", 0.01)
	check(panel._frame._reading.text.begins_with("20.0 ms of"), panel._frame._reading.text)
	check(panel._frame.peak < 100.0, "no peak from the start: %s" % panel._frame.peak)


func test_the_verdict_follows_the_target() -> void:
	var panel := await _panel()
	check_eq(panel.target, "desktop", "a monitor unless a headset is in use")
	check(panel.verdict().begins_with("Well within"), panel.verdict())
	# The same scene against a limit it breaks.
	panel._triangles.show_reading("", 1_200_000, 1_000_000)
	check_eq(panel._triangles.level(), 2, "past the limit")
	check(panel.verdict().begins_with("More triangles"), panel.verdict())
	panel._triangles.show_reading("", 900_000, 1_000_000)
	check_eq(panel._triangles.level(), 1, "near the limit")
	check(panel.verdict().begins_with("Close to"), panel.verdict())
	panel._choice.chosen.emit("standalone")
	check_eq(panel.target, "standalone", "choosing a target")
	check(panel._triangles._reading.text.ends_with("of 1 M"), panel._triangles._reading.text)


func test_optimizing_from_the_panel_is_one_undo_step() -> void:
	var panel := await _panel()
	var points: Array = []
	for i in 200:
		points.append([i * 0.001, 1.0, -1.0])
	run({"cmd": "add_strokes", "strokes": [{"points": points, "width": 0.01}]})
	await frames(2)
	var before := int(world.scene.info().points)
	panel._optimize("layer")
	await frames(2)
	var after := int(world.scene.info().points)
	check(after < before / 10, "a straight stroke needs few points: %d of %d" % [after, before])
	check_eq(editor.status.undo_label, "Optimize strokes", "one step")
	check_eq(PerformancePanel._count(1_234_567), "1.2 M", "counts as people write them")
	check_eq(PerformancePanel._count(12_400), "12.4 k", "thousands")
	check_eq(PerformancePanel._count(950), "950", "small numbers")


func test_sounds_playing_are_held_against_the_target() -> void:
	var panel := await _panel()
	_sound(4.0)
	_sound(4.0)
	await frames(2)
	check_eq(panel.readings().sounds, 0, "nothing plays while the document is stopped")
	run({"cmd": "play"})
	await frames(3)
	check_eq(panel.readings().sounds, 2, "both sound layers play")
	panel._read()
	check_eq(panel._sounds._reading.text, "2 of 32", "against the desktop's limit")
	panel._choice.chosen.emit("standalone")
	check_eq(panel._sounds._reading.text, "2 of 16", "a standalone headset mixes fewer")
	run({"cmd": "pause"})
	await frames(2)
	check_eq(panel.readings().sounds, 0, "and none once it stops")
	panel._read()
	check_eq(panel._sounds._reading.text, "0 of 16", "the reading follows")
	check_eq(panel._sounds._peak_label.text, "peak 2", "the peak stays")
	panel._sounds.show_reading("", 20, 16)
	check(panel.verdict().begins_with("More sounds"), panel.verdict())
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_every_meter_keeps_its_peak_until_reset() -> void:
	var meter := PerformanceMeter.new("Test", "")
	mount(meter)
	meter.show_reading("", 5.0, 10.0)
	meter.show_reading("", 3.0, 10.0)
	check_eq(meter.peak, 5.0, "the highest reading is kept")
	meter.show_reading("", 12.0, 10.0)
	meter.show_reading("", 2.0, 10.0)
	check_eq([meter.level(), meter.peak_level()], [0, 2], "within the limit now, past it at the peak")
	meter.reset_peak()
	check_eq(meter.peak, 0.0, "reset")
	meter.show_reading("", 4.0, 10.0)
	check_eq(meter.peak, 4.0, "and set again by the next reading")

	var panel := await _panel()
	check_eq(panel.meters().keys(), ["frame", "triangles", "draw_calls", "sounds"], "a peak on every meter")
	for key: String in panel.meters():
		(panel.meters()[key] as PerformanceMeter).show_reading("", 1e9, 1.0)
	panel._read()
	for key: String in panel.meters():
		check_eq((panel.meters()[key] as PerformanceMeter).peak, 1e9, "%s keeps its peak" % key)
	check(panel._triangles._peak_label.text.begins_with("peak 1000"), "the peak is written out: %s" % panel._triangles._peak_label.text)
	panel._reset.pressed.emit()
	for key: String in panel.meters():
		check((panel.meters()[key] as PerformanceMeter).peak < 1e9, "%s peak reset" % key)
	check_eq(panel._sounds.peak, 0.0, "nothing has played since")
	# Opening the panel again starts the peaks again.
	panel._sounds.show_reading("", 7.0, 16.0)
	panel.visible = false
	panel.visible = true
	check_eq(panel._sounds.peak, 0.0, "peaks start again when the panel is shown")


func test_a_sound_heard_between_readings_still_reaches_the_peak() -> void:
	var panel := await _panel()
	panel.reset_peaks()
	_sound(4.0)
	await frames(2)
	run({"cmd": "seek", "time": LimnerEditor.to_flicks(1.0)})
	# Scrubbing plays a snippet a fraction of a second long.
	await frames(3)
	check_eq(panel._sounds.peak, 1.0, "the snippet is in the peak")
	# Let the snippet end, so that nothing is left playing when the scene goes.
	await get_tree().create_timer(0.3).timeout
	await frames(2)
	check_eq(panel.readings().sounds, 0, "the snippet is over")
	check_eq(panel._sounds.peak, 1.0, "and the peak remembers it")
