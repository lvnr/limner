extends LimnerTest
## The preview at the hand, and its twin at the mirror image when working mirrored.


func _cursor() -> ToolCursor:
	var cursor := ToolCursor.new()
	cursor.scene = world.scene
	world.add_child(cursor)
	return cursor


func _hand_at(position: Vector3) -> void:
	world.scene.input_hand(0, position, Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)


func test_a_twin_shows_where_mirrored_tools_also_act() -> void:
	var cursor := _cursor()
	_hand_at(Vector3(0.2, 1.2, -0.7))
	await frames(2)
	check(cursor.visible, "the preview follows a hand that is there")
	check_eq(cursor.mirror_position(), null, "no twin without a mirror")
	# The paint layer's pivot is at the origin, so a mirror on X is the plane x = 0.
	run({"cmd": "set_tool_settings", "values": {"symmetry": "x"}})
	await frames(2)
	var twin: Variant = cursor.mirror_position()
	check(twin != null and (twin as Vector3).is_equal_approx(Vector3(-0.2, 1.2, -0.7)), "the twin at the mirror image: %s" % [twin])
	run({"cmd": "set_tool", "tool": "eraser"})
	_hand_at(Vector3(0.3, 1.1, -0.6))
	await frames(2)
	twin = cursor.mirror_position()
	check(twin != null and (twin as Vector3).is_equal_approx(Vector3(-0.3, 1.1, -0.6)), "for the other tools too, and it follows the hand: %s" % [twin])
	run({"cmd": "set_tool_settings", "values": {"symmetry": "off"}})
	await frames(2)
	check_eq(cursor.mirror_position(), null, "gone when the mirror is off")
	world.scene.input_hand_absent(0)
	world.scene.input_commit(0.011)
	await frames(2)
	check(not cursor.visible, "and no preview without a hand")
	cursor.free()
