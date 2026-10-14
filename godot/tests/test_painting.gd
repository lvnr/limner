extends LimnerTest
## Strokes painted through the controller input path end up in the document.


func test_a_trigger_press_paints_a_stroke_and_undo_removes_it() -> void:
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1), Vector3(0.2, 1.05, -1)])
	await frames(2)
	check_eq(row_named("Paint").strokes, 1, "strokes after painting")
	check_eq(editor.status.undo_label, "Paint stroke", "what undo will take back")
	check_eq(editor.status.dirty, true, "the document has something to save")
	editor.undo()
	await frames(2)
	check_eq(row_named("Paint").strokes, 0, "strokes after undo")


func test_the_demo_sheet_paints_every_brush() -> void:
	LimnerDemo.run(editor)
	await frames(2)
	check_eq(row_named("Paint").strokes, 28, "four strokes per brush")
	check(world.scene.stats().triangles > 2000, "the strokes are on screen: %s" % [world.scene.stats()])
	check(messages.is_empty(), "no complaints: %s" % [messages])


func test_painting_on_a_locked_layer_reports_why() -> void:
	run({"cmd": "set_layer", "layer": int(row_named("Paint").id), "locked": true})
	world.scene.input_hand(0, Vector3(0, 1, -1), Quaternion.IDENTITY, 1.0, 0.0, Vector2.ZERO, 0)
	var feedback: Array = world.scene.input_commit(0.011)
	check_eq(feedback.size(), 1, "one piece of feedback")
	check_eq(feedback[0].get("type"), "message", "it is a message")
	check_eq(feedback[0].get("text"), "Layer \"Paint\" is locked", "and says why")


func test_the_cursor_follows_the_hand_and_the_tool() -> void:
	run({"cmd": "set_tool", "tool": "colorize"})
	run({"cmd": "set_tool_settings", "values": {"colorize": {"size": 0.05, "inner": 0.5}}})
	world.scene.input_hand(0, Vector3(0.1, 1.2, -0.7), Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	var cursor := world.scene.tool_cursor()
	check_eq(cursor.tracked, true, "the hand is there")
	check_eq(cursor.tool, "colorize", "tool")
	check_near(cursor.radius, 0.05, "radius")
	check_near(cursor.inner, 0.025, "inner radius")
	check(cursor.position.is_equal_approx(Vector3(0.1, 1.2, -0.7)), "position %s" % cursor.position)
	world.scene.input_hand_absent(0)
	world.scene.input_commit(0.011)
	check_eq(world.scene.tool_cursor().tracked, false, "the hand has gone")
