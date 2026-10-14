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


func test_the_stroke_the_lower_button_would_delete_blinks_first() -> void:
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1), Vector3(0.2, 1.05, -1)])
	LimnerDemo.stroke(world.scene, [Vector3(0, 1.3, -1), Vector3(0.1, 1.3, -1), Vector3(0.2, 1.35, -1)])
	await frames(2)
	# Away from both strokes nothing is singled out.
	world.scene.input_hand(0, Vector3(0.6, 1.7, -1), Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	await frames(2)
	check(world.scene.stroke_at_hand().is_empty(), "nothing under a hand that is away")
	var blink: MeshInstance3D = world.scene.find_child("AtHand", true, false)
	check(blink == null or not blink.visible, "and nothing blinks")
	# At the first stroke, it is named and lit up.
	world.scene.input_hand(0, Vector3(0.1, 1, -1), Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	await frames(2)
	var at: Dictionary = world.scene.stroke_at_hand()
	check_eq(int(at.get("layer", -1)), int(row_named("Paint").id), "a stroke of the paint layer")
	blink = world.scene.find_child("AtHand", true, false)
	check(blink != null and blink.visible and blink.mesh != null, "it blinks")
	check_eq(blink.layers, LimnerWorld.HELPER_LAYERS, "on the layer that pictures and renders leave out")
	# The lower button takes exactly that stroke.
	world.scene.input_hand(0, Vector3(0.1, 1, -1), Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 1)
	world.scene.input_commit(0.011)
	world.scene.input_hand(0, Vector3(0.1, 1, -1), Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	await frames(2)
	check_eq(row_named("Paint").strokes, 1, "one stroke left")
	check(world.scene.stroke_at_hand().is_empty(), "and none under the hand")
	check(not blink.visible, "the blink has gone")


func test_the_brush_grid_stays_with_the_hand_on_the_worlds_own_lines() -> void:
	var grid: MeshInstance3D = world.find_child("BrushGrid", true, false)
	check(grid != null and not grid.visible, "hidden until it is asked for")
	world.show_brush_grid = true
	world.scene.input_hand(0, Vector3(0.11, 1.21, -0.72), Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	await frames(2)
	check(grid.visible, "shown around a hand that is there")
	check_eq(grid.layers, LimnerWorld.HELPER_LAYERS, "on the layer that pictures and renders leave out")
	check_near(LimnerWorld.brush_grid_spacing(world.scene.view_scale()), 0.05, "five centimetres at the scale of a new document")
	var origin := grid.transform.origin
	check(origin.is_equal_approx(Vector3(0.1, 1.2, -0.7)), "on the nearest crossing of the world's lines: %s" % origin)
	# A move within the cell leaves the lines where they are.
	world.scene.input_hand(0, Vector3(0.12, 1.19, -0.71), Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	await frames(2)
	check(grid.transform.origin.is_equal_approx(origin), "the lines stay put: %s" % grid.transform.origin)
	var material: ShaderMaterial = grid.material_override
	var center: Vector3 = material.get_shader_parameter("center")
	check(center.is_equal_approx(Vector3(0.4, -0.2, -0.2)), "and the fade follows the hand among them: %s" % center)
	# Into the next cell, the lattice comes along by a whole cell.
	world.scene.input_hand(0, Vector3(0.16, 1.2, -0.7), Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	await frames(2)
	check(grid.transform.origin.is_equal_approx(Vector3(0.15, 1.2, -0.7)), "a cell along: %s" % grid.transform.origin)
	world.scene.input_hand_absent(0)
	world.scene.input_commit(0.011)
	await frames(2)
	check(not grid.visible, "gone with the hand")
	# Cells are 1, 2 or 5 times a power of ten, whichever looks nearest to five centimetres.
	check_near(LimnerWorld.brush_grid_spacing(10.0), 0.5, "zoomed out ten times")
	check_near(LimnerWorld.brush_grid_spacing(3.0), 0.2, "zoomed out three times")
	check_near(LimnerWorld.brush_grid_spacing(0.02), 0.001, "zoomed far in")
