extends LimnerTest
## Carrying selected strokes: they are shown moved without being made again, and
## land where they were shown when the grip lets go.


func _hand(position: Vector3, trigger: float, grip: float) -> void:
	world.scene.input_hand(0, position, Quaternion.IDENTITY, trigger, grip, Vector2.ZERO, 0)
	world.scene.input_commit(1.0 / 90.0)


## The highlight over the selected stroke. A highlight made again replaces the old
## one at the end of the frame, so a name may have a number added.
func _highlight() -> MeshInstance3D:
	for node: Node in world.scene.find_children("Selected*", "MeshInstance3D", true, false):
		if not node.is_queued_for_deletion():
			return node
	return null


func test_a_carried_selection_moves_without_making_meshes_again() -> void:
	var lower := Vector3(0.1, 1.0, -1.0)
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), lower, Vector3(0.2, 1.0, -1)])
	LimnerDemo.stroke(world.scene, [Vector3(0, 1.3, -1), Vector3(0.1, 1.3, -1), Vector3(0.2, 1.3, -1)])
	run({"cmd": "set_tool", "tool": "select"})
	run({"cmd": "set_tool_settings", "values": {"select": {"size": 0.03}}})
	_hand(lower, 0.0, 0.0)
	_hand(lower, 1.0, 0.0)
	_hand(lower, 0.0, 0.0)
	await frames(2)
	check_eq(int(world.scene.status().selection_count), 1, "one stroke selected")
	var highlight := _highlight()
	check(highlight != null, "the selection is lit up")
	if highlight == null:
		return
	var lit_at := highlight.global_transform.origin
	var revision: int = world.scene.status().doc_revision

	_hand(lower, 0.0, 1.0)
	await frames(1)
	var by := Vector3(0.0, 0.1, 0.05)
	for i in range(1, 5):
		_hand(lower + by * (i / 4.0), 0.0, 1.0)
		await frames(1)
		check_eq(world.scene.stats().meshes_built, 0, "nothing is made again on frame %d" % i)
	check_eq(world.scene.status().doc_revision, revision, "the document waits for the grip to let go")
	check(
		_highlight().global_transform.origin.is_equal_approx(lit_at + by),
		"the highlight moves with the strokes: %s" % _highlight().global_transform.origin
	)

	_hand(lower + by, 0.0, 0.0)
	await frames(2)
	check_eq(world.scene.status().undo_label, "Move strokes", "one undo step")
	check(world.scene.stats().triangles > 0, "the strokes are on screen")
	highlight = _highlight()
	check(highlight != null, "still lit up")
	if highlight != null:
		check(
			highlight.global_transform.origin.distance_to(lit_at + by) < 1e-3,
			"the highlight is made again where it was carried to: %s" % highlight.global_transform.origin
		)
	check(messages.is_empty(), "no complaints: %s" % [messages])


func test_a_layer_hidden_and_shown_again_draws_its_strokes() -> void:
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1), Vector3(0.2, 1.05, -1)])
	await frames(2)
	var triangles: int = world.scene.stats().triangles
	check(triangles > 0, "the stroke is on screen")
	var layer := int(row_named("Paint").id)
	run({"cmd": "set_layer", "layer": layer, "visible": false})
	await frames(2)
	check_eq(world.scene.stats().triangles, 0, "hidden, it lets go of its mesh")
	run({"cmd": "set_layer", "layer": layer, "visible": true})
	await frames(2)
	check_eq(world.scene.stats().triangles, triangles, "shown again")
