extends LimnerTest
## The Performance panel holds the scene against a kind of device.


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
