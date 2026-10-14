extends LimnerTest
## Text written in strokes.


func test_a_letter_becomes_its_outline_and_its_filling() -> void:
	var frame := Transform3D(Basis.IDENTITY, Vector3(0.0, 1.0, -1.0))
	var outline := TextStrokes.build("L", ThemeDB.fallback_font, 0.2, frame, false)
	check_eq(outline.size(), 1, "an L is one outline")
	var points: Array = outline[0].points
	check(points.size() >= 7, "with a corner at each turn, and closed")
	check_eq(points[0], points[points.size() - 1], "the outline ends where it began")
	var low := INF
	var high := -INF
	var left := INF
	var right := -INF
	for point: Array in points:
		check_near(point[2], -1.0, "it stands in the plane of the frame")
		left = minf(left, point[0])
		right = maxf(right, point[0])
		low = minf(low, point[1])
		high = maxf(high, point[1])
	check(high - low > 0.1 and high - low < 0.2, "a capital is most of the letter height: %s" % (high - low))
	check(right - left < high - low, "an L is taller than it is wide")
	check_near((left + right) * 0.5, 0.0, "centred on the frame", 0.01)

	var solid := TextStrokes.build("L", ThemeDB.fallback_font, 0.2, frame, true)
	var rows := solid.filter(func(stroke: Dictionary) -> bool: return stroke.brush == "ribbon")
	check(rows.size() > 20, "filled row by row: %d rows" % rows.size())
	# The stem of the L is narrow; its foot is as wide as the letter.
	var widest := 0.0
	var narrowest := INF
	for row: Dictionary in rows:
		var span: float = row.points[1][0] - row.points[0][0]
		widest = maxf(widest, span)
		narrowest = minf(narrowest, span)
	check(widest > narrowest * 2.5, "the foot is wider than the stem")
	check(widest <= right - left + 1e-4, "and no row leaves the outline")


func test_letters_with_holes_are_not_filled_in() -> void:
	var frame := Transform3D.IDENTITY
	var strokes := TextStrokes.build("O", ThemeDB.fallback_font, 1.0, frame, true)
	var outlines := strokes.filter(func(stroke: Dictionary) -> bool: return stroke.brush == "cylinder")
	check_eq(outlines.size(), 2, "an O has an outside and an inside")
	# Across its middle an O is two runs of paint with a gap between.
	var rows := strokes.filter(func(stroke: Dictionary) -> bool:
		return stroke.brush == "ribbon" and absf(stroke.points[0][1] - 0.05) < 0.03
	)
	var level: float = rows[0].points[0][1]
	var middle := rows.filter(func(stroke: Dictionary) -> bool: return is_equal_approx(stroke.points[0][1], level))
	check_eq(middle.size(), 2, "two spans across the middle of the O")


func test_text_is_added_as_selected_strokes_in_one_undo_step() -> void:
	run({"cmd": "set_color", "color": [1.0, 0.0, 0.0]})
	var count := editor.add_text("Hi", 0.1)
	await frames(2)
	check(count > 20, "strokes for two letters: %d" % count)
	check_eq(int(world.scene.info().strokes), count, "all in the document")
	check_eq(int(editor.status.selection_count), count, "and selected")
	check_eq(editor.status.tool, "select", "with the Selection tool in hand to move them")
	check_eq(editor.status.undo_label, "Add strokes", "as one step")
	editor.undo()
	await frames(2)
	check_eq(int(world.scene.info().strokes), 0, "taken back at once")
	check_eq(editor.add_text("   ", 0.1), 0, "nothing to write")
	check_eq(messages.size(), 1, "and it says so")


func test_the_tools_panel_writes_what_is_typed() -> void:
	var panel := ToolsPanel.new(editor)
	mount(panel)
	await frames(2)
	panel._text.text = "A"
	panel._text_height.set_value(0.05)
	panel._text_solid.button_pressed = false
	panel._add_text()
	await frames(2)
	check_eq(int(world.scene.info().strokes), 2, "an A in outline: outside and inside")
