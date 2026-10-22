extends LimnerTest
## The Monitor panel: it shows the chosen camera in the shape of the document's
## resolution, steps between cameras, and sends the commands of its controls.


## An editor that remembers the commands it was asked to run.
class Recorder extends LimnerEditor:
	var sent: Array[Dictionary] = []

	func run(command: Dictionary) -> Variant:
		sent.append(command)
		return super(command)


var recorder: Recorder
## Stands in for the desktop's camera.
var head: Camera3D


func _panel() -> MonitorPanel:
	recorder = Recorder.new()
	add_child(recorder)
	recorder.setup(world.scene)
	recorder.message.connect(func(text: String, is_error: bool) -> void:
		if is_error:
			messages.append(text)
	)
	head = Camera3D.new()
	head.position = DesktopShell.HEAD
	head.fov = 60.0
	world.add_child(head)
	var panel := MonitorPanel.new(recorder)
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


func _add_camera(layer_name: String, translation := [0.0, 0.0, 0.0]) -> int:
	var added: Dictionary = run({"cmd": "add_layer", "kind": "camera", "name": layer_name})
	var id := int(added.layer)
	var placed: Dictionary = world.scene.layer_details(id).transform
	placed.translation = translation
	run({"cmd": "set_layer", "layer": id, "transform": placed})
	return id


## Checks that the picture has the shape `aspect`, is as large as fits in the
## viewfinder, and sits in its middle.
func _check_fits(finder: Viewfinder, aspect: float, what: String) -> void:
	var picture := finder.picture_rect()
	check_near(picture.size.x / picture.size.y, aspect, "%s: the shape" % what, 1e-3)
	check(is_equal_approx(picture.size.x, finder.size.x) or is_equal_approx(picture.size.y, finder.size.y), "%s: as large as fits, %s in %s" % [what, picture.size, finder.size])
	check(picture.size.x <= finder.size.x + 0.01 and picture.size.y <= finder.size.y + 0.01, "%s: inside the viewfinder" % what)
	check(picture.get_center().is_equal_approx(finder.size * 0.5), "%s: in the middle, with equal bars" % what)


func test_without_a_camera_it_offers_to_add_one() -> void:
	var panel := await _panel()
	check_eq([panel._with_camera.visible, panel._without_camera.visible], [false, true], "a document without cameras")
	check_eq(panel.camera_chosen(), -1, "no camera is chosen")
	check(not panel.viewfinder.is_live(), "nothing is rendered")
	var add: Button = panel._without_camera.get_child(1)
	add.pressed.emit()
	await frames(2)
	check_eq(recorder.sent, [{"cmd": "add_layer", "kind": "camera"}] as Array[Dictionary], "the button adds a camera layer")
	check_eq([panel._with_camera.visible, panel._without_camera.visible], [true, false], "which the Monitor then shows")
	check_eq(recorder.row(panel.camera_chosen()).kind, "camera", "it is chosen")
	check(panel.viewfinder.is_live(), "and its picture is live")
	check_fits(panel)
	recorder.undo()
	await frames(2)
	check_eq([panel._with_camera.visible, panel.viewfinder.is_live()], [false, false], "undoing takes it away again")


func test_cameras_are_listed_and_stepped_through() -> void:
	var ids := [_add_camera("Wide"), _add_camera("Close"), _add_camera("Top")]
	run({"cmd": "add_layer", "kind": "light"})
	var panel := await _panel()
	check_eq(panel._camera.item_count, 3, "the three cameras and nothing else")
	var listed: Array[int] = panel._camera.layers()
	var names := listed.map(func(id: int) -> String: return recorder.row(id).name)
	check_eq(panel.camera_chosen(), listed[0], "the first is shown")
	check_eq(panel._camera.get_item_text(0), names[0], "by name")
	check(not panel._next.disabled and not panel._previous.disabled, "there are others to go to")

	panel._next.pressed.emit()
	check_eq(panel.camera_chosen(), listed[1], "next")
	panel._next.pressed.emit()
	panel._next.pressed.emit()
	check_eq(panel.camera_chosen(), listed[0], "next from the last is the first")
	panel._previous.pressed.emit()
	check_eq(panel.camera_chosen(), listed[2], "previous from the first is the last")
	await _choose(panel._camera, 1)
	check_eq(panel.camera_chosen(), listed[1], "or chosen from the list")

	run({"cmd": "delete_layers", "layers": [listed[1]]})
	await frames(2)
	check_eq(panel._camera.item_count, 2, "a deleted camera leaves the list")
	check_eq(panel.camera_chosen(), listed[0], "and the first is shown instead")
	run({"cmd": "delete_layers", "layers": [listed[2]]})
	await frames(2)
	check(panel._next.disabled and panel._previous.disabled, "one camera has no next")
	check_eq(ids.size(), 3, "three were made")
	check(recorder.sent.is_empty(), "looking at cameras changes nothing: %s" % [recorder.sent])


func test_the_picture_has_the_shape_of_the_resolution() -> void:
	_add_camera("Shot")
	var panel := await _panel()
	var finder := panel.viewfinder
	check(finder.size.x > 200.0 and finder.size.y >= 168.0, "the viewfinder has room: %s" % finder.size)
	check_eq(panel._resolution.get_item_text(panel._resolution.selected), "1920 × 1080", "the document's resolution")
	_check_fits(finder, 16.0 / 9.0, "sixteen to nine")

	await _choose(panel._resolution, 1)
	check_eq(recorder.sent, [{"cmd": "set_document", "camera_resolution": [1080, 1920]}] as Array[Dictionary], "a size from the list is written to the document")
	_check_fits(finder, 9.0 / 16.0, "upright")
	check_near(finder.picture_rect().size.y, finder.size.y, "an upright picture is as tall as the viewfinder, with bars at its sides", 0.01)
	check_eq(finder._viewport.size, Vector2i(360, 640), "the picture rendered has that shape too")

	await _choose(panel._resolution, DocumentPanel.RESOLUTIONS.size())
	check(panel._resolution_custom.visible, "Custom shows its boxes")
	check_eq(recorder.sent.size(), 1, "and changes nothing yet")
	await _type(panel._width, "3000")
	await _type(panel._height, "1000")
	check_eq(recorder.get_info().camera_resolution, [3000, 1000], "a typed size")
	_check_fits(finder, 3.0, "a banner")
	check_near(finder.picture_rect().size.x, finder.size.x, "a banner is as wide as the viewfinder, with bars above and below", 0.01)
	recorder.undo()
	recorder.undo()
	recorder.undo()
	await frames(2)
	_check_fits(finder, 16.0 / 9.0, "the picture follows undo")
	check_eq(recorder.status.can_undo, true, "the camera itself is still there to undo")

	check_eq(finder.divisions, 0, "no framing lines to begin with")
	var thirds: Button = panel._grid.buttons["3"]
	thirds.pressed.emit()
	check_eq(finder.divisions, 3, "thirds")
	var quarters: Button = panel._grid.buttons["4"]
	quarters.pressed.emit()
	check_eq(finder.divisions, 4, "quarters")
	var none: Button = panel._grid.buttons["0"]
	none.pressed.emit()
	check_eq(finder.divisions, 0, "and none again")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_picture_is_taken_from_the_camera_and_only_while_shown() -> void:
	var id := _add_camera("Shot", [1.0, 2.0, 3.0])
	run({"cmd": "set_layer_kind", "layer": id, "values": {"fov": 33.0}})
	var panel := await _panel()
	var eye := panel.viewfinder.camera
	check(eye.global_position.is_equal_approx(Vector3(1.0, 2.0, 3.0)), "the viewfinder's camera is at the layer: %s" % eye.global_position)
	check_near(eye.fov, 33.0, "with its lens")
	check_eq(eye.cull_mask, CaptureRig.ARTWORK_LAYERS, "and sees artwork only")
	check_eq(panel.viewfinder._viewport.world_3d, get_viewport().world_3d, "in the same world as the main view")
	# Navigating moves the artwork, and the camera layer with it.
	world.scene.view_pan(Vector3(0.5, 0.0, 0.0))
	await frames(2)
	check(eye.global_position.is_equal_approx(Vector3(1.5, 2.0, 3.0)), "it follows navigation: %s" % eye.global_position)
	check(panel.viewfinder.is_live(), "the picture is live while the panel is shown")
	panel.get_parent().visible = false
	await frames(2)
	check(not panel.viewfinder.is_live(), "and not rendered while it is hidden")
	world.scene.view_pan(Vector3(0.5, 0.0, 0.0))
	await frames(2)
	check(eye.global_position.is_equal_approx(Vector3(1.5, 2.0, 3.0)), "nor followed")
	panel.get_parent().visible = true
	await frames(2)
	check(panel.viewfinder.is_live() and eye.global_position.is_equal_approx(Vector3(2.0, 2.0, 3.0)), "until it is shown again")


func test_the_camera_can_be_what_grabbing_moves() -> void:
	var id := _add_camera("Shot")
	var other := _add_camera("Other")
	var panel := await _panel()
	var listed: Array[int] = panel._camera.layers()
	panel._camera.set_layer(id)
	panel._transform_target.button_pressed = true
	await frames(2)
	check_eq(recorder.sent, [{"cmd": "set_transform_target", "layer": id}] as Array[Dictionary], "ticking sends the camera as the transform target")
	check_eq(recorder.status.transform_target, id, "which it then is")
	panel.step_camera(1 if listed[0] == id else -1)
	await frames(2)
	check_eq(panel.camera_chosen(), other, "another camera")
	check_eq(panel._transform_target.button_pressed, false, "is not the target")
	panel.step_camera(1)
	await frames(2)
	check_eq(panel._transform_target.button_pressed, true, "the first still is")
	panel._transform_target.button_pressed = false
	await frames(2)
	check_eq(recorder.sent.back(), {"cmd": "set_transform_target", "layer": null}, "unticking gives grabbing back to the view")
	check_eq(recorder.status.transform_target, null, "which it then moves")
	run({"cmd": "set_transform_target", "layer": id})
	await frames(2)
	check_eq(panel._transform_target.button_pressed, true, "the box follows a target set elsewhere")


func test_looking_through_the_camera_moves_the_main_view_behind_it() -> void:
	var id := _add_camera("Shot", [2.0, 0.5, -4.0])
	var placed: Dictionary = world.scene.layer_details(id).transform
	var turned := Quaternion.from_euler(Vector3(deg_to_rad(-20.0), deg_to_rad(130.0), deg_to_rad(10.0)))
	placed.rotation = [turned.x, turned.y, turned.z, turned.w]
	placed.scale = 0.5
	run({"cmd": "set_layer", "layer": id, "transform": placed})
	run({"cmd": "set_layer_kind", "layer": id, "values": {"fov": 37.0}})
	var panel := await _panel()
	panel._look_through.button_pressed = true
	await frames(2)
	var layer := world.scene.layer_transform(id)
	check(layer.origin.is_equal_approx(DesktopShell.HEAD), "the camera layer is where the head is: %s" % layer.origin)
	check(layer.basis.orthonormalized().is_equal_approx(Basis.IDENTITY), "facing the way the head faces: %s" % layer.basis.orthonormalized())
	check_near(head.fov, 37.0, "and the head has the camera's lens")
	check_near(layer.basis.get_scale().x, 0.5, "the size of things is left alone")
	check_eq(CameraView.of(recorder).following, id, "the view follows the camera")

	# Navigating has no lasting effect while it does.
	world.scene.view_pan(Vector3(0.0, 3.0, 0.0))
	await frames(2)
	check(world.scene.layer_transform(id).origin.is_equal_approx(DesktopShell.HEAD), "the view stays behind the camera")
	placed.translation = [9.0, 9.0, 9.0]
	run({"cmd": "set_layer", "layer": id, "transform": placed})
	await frames(2)
	check(world.scene.layer_transform(id).origin.is_equal_approx(DesktopShell.HEAD), "also when the camera moves")

	panel._look_through.button_pressed = false
	await frames(2)
	check_near(head.fov, 60.0, "switching it off gives the head its own lens back")
	check_eq(CameraView.of(recorder).following, -1, "and lets go of the view")
	world.scene.view_pan(Vector3(0.0, 3.0, 0.0))
	await frames(2)
	check(not world.scene.layer_transform(id).origin.is_equal_approx(DesktopShell.HEAD), "navigation is free again")

	panel._look_through.button_pressed = true
	await frames(2)
	run({"cmd": "delete_layers", "layers": [id]})
	await frames(3)
	check_eq(CameraView.of(recorder).following, -1, "a deleted camera is let go of")
	check_near(head.fov, 60.0, "with the lens put back")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_another_document_ends_looking_through_the_camera() -> void:
	var id := _add_camera("Shot", [2.0, 0.5, -4.0])
	run({"cmd": "set_layer_kind", "layer": id, "values": {"fov": 37.0}})
	var panel := await _panel()
	panel._look_through.button_pressed = true
	await frames(2)
	check_eq(CameraView.of(recorder).following, id, "looking through the camera")
	check_near(head.fov, 37.0, "with its lens")

	# The new document's layers are numbered from the start again, so an id kept
	# from the old one could name one of them.
	recorder.new_document()
	await frames(2)
	check_eq(CameraView.of(recorder).following, -1, "a new document lets go of the camera")
	check_near(head.fov, 60.0, "and gives the head its own lens back")
	check(not panel._look_through.button_pressed, "the box follows")
	var other := _add_camera("Another", [0.0, 3.0, 0.0])
	await frames(2)
	check(not world.scene.layer_transform(other).origin.is_equal_approx(DesktopShell.HEAD), "a camera of the new document is not followed")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_current_frame_is_saved_at_the_resolution() -> void:
	_add_camera("Shot", [0.0, 1.45, 1.0])
	run({"cmd": "set_document", "title": "Sheet", "camera_resolution": [320, 200]})
	var panel := await _panel()
	LimnerDemo.run(recorder)
	var notes: Array[String] = []
	recorder.message.connect(func(text: String, _is_error: bool) -> void: notes.append(text))
	if not LimnerCapture.can_render():
		print("     skipped: saving a frame needs a display; run the tests without --headless to include it")
		check_eq(await panel.export_frame(), "", "without a display nothing is written")
		check_eq(notes, ["Pictures cannot be rendered without a display"] as Array[String], "and the user is told why")
		messages.clear()
		return
	panel.folder = OS.get_cache_dir().path_join("limner-tests").path_join("frames-%d" % Time.get_ticks_usec())
	var path: String = await panel.export_frame()
	check(path.begins_with(panel.folder.path_join("Sheet_")) and path.ends_with(".png"), "named after the document and the time: %s" % path)
	check_eq(Image.load_from_file(path).get_size(), Vector2i(320, 200), "at the document's camera resolution")
	check_eq(notes, ["Saved %s" % path] as Array[String], "and the user is told where it is")
	DirAccess.remove_absolute(path)
	DirAccess.remove_absolute(panel.folder)
