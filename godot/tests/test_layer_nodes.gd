extends LimnerTest
## What the scene shows and plays for layers that are not paint: lights, pictures,
## models, sounds, and the gizmos that mark where things are.

const PLACE := {"rotation": [0, 0, 0, 1], "scale": 1.0, "translation": [0, 0, 0], "flip": "none"}

var _dir := ""


func _file(file_name: String) -> String:
	if _dir == "":
		_dir = LayerTestKit.scratch_dir()
	return _dir.path_join(file_name)


## The tests play tones; nobody running them wants to hear that.
func _enter_tree() -> void:
	AudioServer.set_bus_mute(0, true)


func _exit_tree() -> void:
	AudioServer.set_bus_mute(0, false)
	if _dir != "":
		LayerTestKit.remove(_dir)


func _add(kind: String) -> int:
	var result: Dictionary = run({"cmd": "add_layer", "kind": kind})
	return int(result.layer)


func _place(layer: int, translation: Vector3, scale := 1.0) -> void:
	var transform := PLACE.duplicate()
	transform.translation = [translation.x, translation.y, translation.z]
	transform.scale = scale
	run({"cmd": "set_layer", "layer": layer, "transform": transform})


func _props(layer: int, values: Dictionary) -> void:
	run({"cmd": "set_layer_kind", "layer": layer, "values": values})


func _import(path: String) -> int:
	var result := world.scene.import_file(path)
	check(result.ok, "%s imports: %s" % [path.get_file(), result.error])
	return int(result.layer)


func test_a_light_layer_is_a_light_in_the_scene() -> void:
	var id := _add("light")
	_place(id, Vector3(1, 2, -3))
	_props(id, {"color": [1.0, 0.0, 0.0], "energy": 2.0, "range": 4.0})
	await frames(2)
	check_eq(LayerTestKit.count(world.scene, "Light3D"), 1, "one light")
	var light: Light3D = LayerTestKit.node(world.scene, "Light", id)
	check(light is OmniLight3D, "a point light shines all round")
	check(light.visible, "it is on")
	check_near(light.light_energy, 2.0, "energy")
	check(light.light_color.is_equal_approx(Color.RED), "colour: %s" % light.light_color)
	check_near((light as OmniLight3D).omni_range, 4.0, "range")
	check(light.global_position.is_equal_approx(Vector3(1, 2, -3)), "where it is: %s" % light.global_position)
	check_eq(light.shadow_enabled, false, "no shadows until asked")

	# Fading the layer dims the light; hiding it puts the light out.
	run({"cmd": "set_layer", "layer": id, "opacity": 0.25})
	_props(id, {"shadows": true})
	await frames(2)
	check_near(light.light_energy, 0.5, "energy at a quarter opacity")
	check_eq(light.shadow_enabled, true, "shadows")
	run({"cmd": "set_layer", "layer": id, "visible": false})
	await frames(2)
	check_eq(light.visible, false, "a hidden layer gives no light")
	check_eq(LayerTestKit.gizmo(world.scene, id).size(), 0, "and has no gizmo")
	run({"cmd": "set_layer", "layer": id, "visible": true})

	# Its reach is in layer units: a larger layer and a closer view both lengthen it.
	_place(id, Vector3(1, 2, -3), 10.0)
	await frames(2)
	check_near((light as OmniLight3D).omni_range, 40.0, "range of a layer ten times larger", 1e-3)
	world.scene.view_zoom(Vector3.ZERO, 2.0)
	await frames(2)
	check_near((light as OmniLight3D).omni_range, 80.0, "range at twice the zoom", 1e-3)
	check(light.global_transform.basis.is_equal_approx(Basis.IDENTITY), "the light itself is never scaled")


func test_a_light_changes_kind_and_points_along_the_layer() -> void:
	var id := _add("light")
	_props(id, {"kind": "spot", "spot_angle": 20.0, "spot_blend": 0.5, "range": 3.0})
	var facing := PLACE.duplicate()
	facing.rotation = [0.0, sin(PI / 4.0), 0.0, cos(PI / 4.0)]
	run({"cmd": "set_layer", "layer": id, "transform": facing})
	await frames(2)
	var light: Light3D = LayerTestKit.node(world.scene, "Light", id)
	check(light is SpotLight3D, "a spot light")
	check_eq(LayerTestKit.count(world.scene, "Light3D"), 1, "the point light it was is gone")
	var spot := light as SpotLight3D
	check_near(spot.spot_angle, 20.0, "cone angle")
	check_near(spot.spot_range, 3.0, "range")
	check_near(spot.spot_angle_attenuation, 2.0, "a soft edge")
	# Turned a quarter turn about Y, the layer's -Z points along -X.
	check((-light.global_transform.basis.z).is_equal_approx(Vector3(-1, 0, 0)), "it shines where the layer faces")

	_props(id, {"kind": "directional"})
	await frames(2)
	light = LayerTestKit.node(world.scene, "Light", id)
	check(light is DirectionalLight3D, "a directional light")
	check_eq(LayerTestKit.count(world.scene, "Light3D"), 1, "still one light")

	run({"cmd": "delete_layers", "layers": [id]})
	await frames(2)
	check_eq(LayerTestKit.count(world.scene, "Light3D"), 0, "deleting the layer puts it out")
	editor.undo()
	await frames(2)
	check_eq(LayerTestKit.count(world.scene, "Light3D"), 1, "undo brings it back")
	editor.new_document()
	await frames(2)
	check_eq(LayerTestKit.count(world.scene, "Light3D"), 0, "a new document has none")


func test_a_lit_paint_layer_takes_the_lit_materials() -> void:
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.2, 1, -1)])
	await frames(2)
	var chunk: MeshInstance3D = null
	for child in world.scene.get_node("Strokes").get_children():
		if child is MeshInstance3D:
			chunk = child
	var material: ShaderMaterial = chunk.get_surface_override_material(0)
	check(material.shader.resource_path.ends_with("stroke_tube_opaque.gdshader"), "painted strokes are unlit: %s" % material.shader.resource_path)
	var paint := int(row_named("Paint").id)
	_props(paint, {"shading": "lit"})
	await frames(3)
	material = chunk.get_surface_override_material(0)
	check(material.shader.resource_path.ends_with("stroke_tube_opaque_lit.gdshader"), "a lit layer is shaded: %s" % material.shader.resource_path)
	run({"cmd": "set_layer", "layer": paint, "opacity": 0.5})
	await frames(2)
	material = chunk.get_surface_override_material(0)
	check(material.shader.resource_path.ends_with("stroke_tube_see_through_lit.gdshader"), "and stays shaded when faded: %s" % material.shader.resource_path)
	editor.undo()
	editor.undo()
	await frames(3)
	material = chunk.get_surface_override_material(0)
	check(material.shader.resource_path.ends_with("stroke_tube_opaque.gdshader"), "undo makes it unlit again: %s" % material.shader.resource_path)


func test_adding_a_light_puts_it_in_sight_and_makes_the_paint_take_it() -> void:
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.2, 1, -1)])
	await frames(2)
	var chunk: MeshInstance3D = null
	for child in world.scene.get_node("Strokes").get_children():
		if child is MeshInstance3D:
			chunk = child
	var material: ShaderMaterial = chunk.get_surface_override_material(0)
	check(material.shader.resource_path.ends_with("stroke_tube_opaque.gdshader"), "paint starts unlit: %s" % material.shader.resource_path)
	var id := _add("light")
	await frames(3)
	material = chunk.get_surface_override_material(0)
	check(material.shader.resource_path.ends_with("stroke_tube_opaque_lit.gdshader"), "the first light makes the paint lit: %s" % material.shader.resource_path)
	var light: Light3D = LayerTestKit.node(world.scene, "Light", id)
	# Ahead of the eyes (1.6 m up when nothing says otherwise), toward the hand that
	# drew, and above them: not on the floor.
	var at := light.global_position
	check_near(at.y, 1.9, "above the eyes")
	check_near(Vector2(at.x, at.z).length(), 0.8, "0.8 m ahead")
	check(at.z < -0.7 and at.x > 0.0, "toward the hand: %s" % at)
	check_near((light as OmniLight3D).omni_range, 3.0, "a reach that suits the room")
	editor.undo()
	await frames(3)
	material = chunk.get_surface_override_material(0)
	check(material.shader.resource_path.ends_with("stroke_tube_opaque.gdshader"), "one undo takes the light and the shading back: %s" % material.shader.resource_path)
	check_eq(LayerTestKit.count(world.scene, "Light3D"), 0, "no light left")


func test_gizmos_mark_spawn_areas_cameras_lights_and_sounds() -> void:
	await frames(2)
	var spawn := int(row_named("Spawn Area").id)
	# The ring, the mark of the spawn area a viewer starts at, and the eyes.
	check_eq(LayerTestKit.gizmo(world.scene, spawn).size(), 3, "the default spawn area")
	_props(spawn, {"floor_level": true})
	await frames(2)
	var parts := LayerTestKit.gizmo(world.scene, spawn)
	check_eq(parts.size(), 4, "a floor-level one has a pole up to the eyes")
	check_near(parts[3].global_position.y, 1.6, "the eyes are at standing height")
	var blue := LimnerEditor.screen_color(world.scene.layer_details(spawn).props.color)
	var color: Color = parts[0].get_instance_shader_parameter("line_color")
	check(color.is_equal_approx(blue), "in the spawn area's own colour: %s" % color)
	check(parts[0].material_override is ShaderMaterial, "drawn with the line material")
	check_eq(parts[0].cast_shadow, GeometryInstance3D.SHADOW_CASTING_SETTING_OFF, "and never throwing a shadow")

	var second := _add("viewpoint")
	var camera := _add("camera")
	var light := _add("light")
	_place(camera, Vector3(1, 1, 0))
	_props(light, {"color": [0.0, 1.0, 0.0]})
	await frames(2)
	check_eq(LayerTestKit.gizmo(world.scene, second).size(), 2, "another spawn area is not marked as the first")
	var frustum := LayerTestKit.gizmo(world.scene, camera)
	check_eq(frustum.size(), 1, "a camera")
	# 1920 by 1080: the gizmo is as wide as the picture, and never larger than a gizmo.
	var size := frustum[0].global_transform.basis.get_scale()
	check_near(size.x / size.y, 16.0 / 9.0, "the camera's picture shape", 1e-3)
	check_near(maxf(size.x, maxf(size.y, size.z)), 0.08, "gizmo size", 1e-4)
	var sun := LayerTestKit.gizmo(world.scene, light)
	check_eq(sun.size(), 3, "a light: its mark, an outline, and a solid heart")
	color = sun[0].get_instance_shader_parameter("line_color")
	check(color.is_equal_approx(Color.GREEN), "in the light's colour: %s" % color)
	var outline: Color = sun[1].get_instance_shader_parameter("line_color")
	check(outline.get_luminance() < 0.1, "a bright light is outlined in black: %s" % outline)
	color = sun[2].get_instance_shader_parameter("line_color")
	check(color.is_equal_approx(Color.GREEN), "its heart in its colour: %s" % color)
	check_near(sun[0].global_transform.basis.get_scale().x, 0.08, "as large as other gizmos", 1e-4)

	# Gizmos keep their size however far the view is zoomed.
	world.scene.view_zoom(Vector3.ZERO, 50.0)
	await frames(2)
	size = LayerTestKit.gizmo(world.scene, camera)[0].global_transform.basis.get_scale()
	check_near(maxf(size.x, maxf(size.y, size.z)), 0.08, "gizmo size after zooming", 1e-4)
	check(LayerTestKit.gizmo(world.scene, camera)[0].global_position.is_equal_approx(Vector3(50, 50, 0)), "but stays at its layer")

	var voice := _import(LayerTestKit.wav(_file("voice.wav"), 0.2, 1))
	await frames(2)
	check_eq(LayerTestKit.gizmo(world.scene, voice).size(), 1, "a sound")
	_props(voice, {"attenuation": {"mode": "linear", "min": 0.5, "max": 2.0}})
	await frames(2)
	parts = LayerTestKit.gizmo(world.scene, voice)
	check_eq(parts.size(), 3, "a sound that fades with distance shows how far it reaches")
	# The view is zoomed fifty times, and the reach is measured in the layer's units.
	check_near(parts[1].global_transform.basis.x.length(), 25.0, "full volume inside", 1e-2)
	check_near(parts[2].global_transform.basis.x.length(), 100.0, "silent beyond", 1e-2)

	editor.set_option("show_extras", false)
	await frames(2)
	for id: int in [spawn, second, camera, light, voice]:
		check_eq(LayerTestKit.gizmo(world.scene, id).size(), 0, "no gizmos when extras are off (layer %d)" % id)
	check(LayerTestKit.node(world.scene, "Light", light).visible, "the light itself still shines")


func test_bounds_are_drawn_around_a_layer_that_asks_for_them() -> void:
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.4, 1.2, -1)])
	var paint := int(row_named("Paint").id)
	await frames(2)
	check_eq(LayerTestKit.gizmo(world.scene, paint).size(), 0, "none until asked")
	run({"cmd": "set_layer", "layer": paint, "show_bounds": true})
	await frames(2)
	var parts := LayerTestKit.gizmo(world.scene, paint)
	check_eq(parts.size(), 1, "a box")
	var size := parts[0].global_transform.basis.get_scale()
	check(size.x > 0.4 and size.x < 0.5 and size.y > 0.2 and size.y < 0.3, "around the stroke: %s" % size)
	check_near(parts[0].global_position.x, 0.2, "centred on it", 1e-3)

	var picture := _import(LayerTestKit.png(_file("wide.png"), 8, 2))
	run({"cmd": "set_layer", "layer": picture, "show_bounds": true})
	await frames(2)
	parts = LayerTestKit.gizmo(world.scene, picture)
	check_eq(parts.size(), 1, "a box round a picture")
	size = parts[0].global_transform.basis.get_scale()
	check_near(size.x, 4.0, "as wide as the picture")
	check_near(size.y, 1.0, "and as tall")

	run({"cmd": "set_layer", "layer": paint, "show_bounds": false})
	await frames(2)
	check_eq(LayerTestKit.gizmo(world.scene, paint).size(), 0, "and gone when no longer asked for")


func test_a_picture_follows_its_settings() -> void:
	var id := _import(LayerTestKit.png(_file("sky.png"), 16, 8))
	_place(id, Vector3(0, 1, -2))
	await frames(2)
	var node: MeshInstance3D = LayerTestKit.node(world.scene, "Picture", id)
	check(node.global_position.is_equal_approx(Vector3(0, 1, -2)), "where the layer is")
	check_eq(node.cast_shadow, GeometryInstance3D.SHADOW_CASTING_SETTING_OFF, "a picture throws no shadow")

	_props(id, {"projection": "equirect"})
	await frames(2)
	check(node.mesh is SphereMesh, "a picture that surrounds the viewer is drawn on a sphere")
	var material: ShaderMaterial = node.material_override
	check(material.shader.resource_path.ends_with("picture_sphere_opaque.gdshader"), "from the inside: %s" % material.shader.resource_path)
	check_eq(material.get_shader_parameter("projection"), 0, "equirectangular")
	check_near(node.global_transform.basis.x.length(), 10.0, "ten units in radius")
	_props(id, {"projection": "equirect_stereo"})
	await frames(2)
	material = node.material_override
	check_eq(material.get_shader_parameter("projection"), 1, "one eye above the other")
	_props(id, {"projection": "cubemap_strip"})
	await frames(2)
	material = node.material_override
	check_eq(material.get_shader_parameter("projection"), 5, "a strip lying on its side, as the picture is wider than tall")
	_props(id, {"projection": "flat"})
	await frames(2)
	check(node.mesh is QuadMesh, "and flat again")

	# Navigation moves the world, and a picture with it, unless it is locked to the viewer.
	world.scene.view_pan(Vector3(1, 0, 0))
	await frames(2)
	check(node.global_position.is_equal_approx(Vector3(1, 1, -2)), "it moves with the world: %s" % node.global_position)
	_props(id, {"viewer_locked": true})
	await frames(2)
	check(node.global_position.is_equal_approx(Vector3(0, 1, -2)), "locked, it stays in the room: %s" % node.global_position)
	world.scene.view_zoom(Vector3.ZERO, 3.0)
	await frames(2)
	check(node.global_position.is_equal_approx(Vector3(0, 1, -2)), "whatever the view does: %s" % node.global_position)
	check_near(node.global_transform.basis.y.length(), 1.0, "and keeps its size")

	run({"cmd": "set_layer", "layer": id, "opacity": 0.5})
	await frames(2)
	material = node.material_override
	check(material.shader.resource_path.ends_with("picture_flat_see_through.gdshader"), "a faded picture is dithered: %s" % material.shader.resource_path)
	check_near(node.get_instance_shader_parameter("layer_opacity"), 0.5, "by the layer's opacity")
	run({"cmd": "set_layer", "layer": id, "visible": false})
	await frames(2)
	check_eq(node.visible, false, "hidden with its layer")


func test_a_model_follows_its_settings() -> void:
	var id := _import(LayerTestKit.obj(_file("rock.obj")))
	await frames(2)
	var node: MeshInstance3D = LayerTestKit.node(world.scene, "Model", id)
	var wire: MeshInstance3D = node.get_node("Wire")
	var material: ShaderMaterial = node.material_override
	check(material.shader.resource_path.ends_with("stroke_flat_opaque.gdshader"), "unlit: %s" % material.shader.resource_path)
	check_eq(wire.visible, false, "no wireframe until asked")

	_props(id, {"shading": "lit", "wireframe": true})
	await frames(2)
	material = node.material_override
	check(material.shader.resource_path.ends_with("stroke_flat_opaque_lit.gdshader"), "lit: %s" % material.shader.resource_path)
	check_eq(wire.visible, true, "the wireframe is drawn over it")
	check_eq(wire.mesh.surface_get_primitive_type(0), Mesh.PRIMITIVE_LINES, "as lines")
	# Three triangles that share no vertices: nine edges.
	var edges: PackedInt32Array = wire.mesh.surface_get_arrays(0)[Mesh.ARRAY_INDEX]
	check_eq(edges.size(), 18, "one line per edge")

	run({"cmd": "set_layer", "layer": id, "opacity": 0.4})
	await frames(2)
	material = node.material_override
	check(material.shader.resource_path.ends_with("stroke_flat_see_through_lit.gdshader"), "faded, it is dithered: %s" % material.shader.resource_path)
	check_near(node.get_instance_shader_parameter("layer_opacity"), 0.4, "by the layer's opacity")

	_place(id, Vector3(3, 0, 0), 0.5)
	await frames(2)
	check(node.global_position.is_equal_approx(Vector3(3, 0.5, 0)), "it moves with its layer: %s" % node.global_position)
	check_near(node.global_transform.basis.x.length(), 0.5, "and takes its scale in the transform, not in the mesh")
	check_eq(node.mesh.get_aabb().size, Vector3(2, 2, 2), "the mesh is as it was")

	var copy: Dictionary = run({"cmd": "duplicate_layers", "layers": [id]})
	await frames(2)
	var twin: MeshInstance3D = LayerTestKit.node(world.scene, "Model", int(copy.layers[0]))
	check(twin != null and twin.mesh == node.mesh, "a copy of the layer shares the mesh")

	run({"cmd": "delete_layers", "layers": [id, int(copy.layers[0])]})
	await frames(2)
	check_eq(LayerTestKit.count(world.scene, "MeshInstance3D"), 0, "deleted with its layer")


func test_a_sound_plays_with_the_timeline() -> void:
	var id := _import(LayerTestKit.wav(_file("tone.wav"), 2.0, 2))
	await frames(2)
	var player: AudioStreamPlayer = LayerTestKit.node(world.scene, "Sound", id)
	check_eq(player.playing, false, "silent while the document is not playing")
	check_near(player.volume_linear, 1.0, "at full volume")

	run({"cmd": "play"})
	await frames(3)
	check_eq(player.playing, true, "it plays when the document plays")
	# The eye hides a sound's gizmo and nothing else.
	run({"cmd": "set_layer", "layer": id, "visible": false})
	await frames(2)
	check_eq(player.playing, true, "hiding the layer does not silence it")
	check_eq(LayerTestKit.gizmo(world.scene, id).size(), 0, "only its gizmo goes")

	run({"cmd": "set_layer", "layer": id, "opacity": 0.5})
	_props(id, {"gain": 0.5})
	await frames(2)
	check_near(player.volume_linear, 0.25, "volume is gain times opacity")
	editor.set_option("mute", true)
	await frames(2)
	check_near(player.volume_linear, 0.0, "muted")
	check_eq(player.playing, true, "but still in step")
	editor.set_option("mute", false)

	run({"cmd": "pause"})
	await frames(2)
	check_eq(player.playing, false, "it stops when the document pauses")

	# Past its end a sound is silent, unless it loops.
	run({"cmd": "seek", "time": LimnerEditor.to_flicks(3.0)})
	run({"cmd": "play"})
	await frames(3)
	check_eq(player.playing, false, "nothing left to play after two seconds")
	_props(id, {"looped": true})
	await frames(3)
	check_eq(player.playing, true, "a looping sound goes round")
	var stream: AudioStreamWAV = player.stream
	check_eq(stream.loop_mode, AudioStreamWAV.LOOP_FORWARD, "the stream loops")
	run({"cmd": "pause"})

	run({"cmd": "delete_layers", "layers": [id]})
	await frames(2)
	check_eq(LayerTestKit.count(world.scene, "AudioStreamPlayer"), 0, "the player goes with the layer")


func test_scrubbing_plays_a_snippet() -> void:
	var id := _import(LayerTestKit.wav(_file("tone.wav"), 2.0, 2))
	await frames(2)
	var player: AudioStreamPlayer = LayerTestKit.node(world.scene, "Sound", id)
	run({"cmd": "seek", "time": LimnerEditor.to_flicks(0.5)})
	await frames(1)
	check_eq(player.playing, true, "moving the playhead is heard")
	await get_tree().create_timer(0.3).timeout
	await frames(2)
	check_eq(player.playing, false, "for a moment only")


func test_a_positional_sound_is_placed_and_fades_with_distance() -> void:
	var id := _import(LayerTestKit.wav(_file("voice.wav"), 1.0, 1))
	_place(id, Vector3(0, 0, -4))
	_props(id, {"attenuation": {"mode": "linear", "min": 1.0, "max": 3.0}})
	await frames(2)
	var player: AudioStreamPlayer3D = LayerTestKit.node(world.scene, "Sound", id)
	check(player.global_position.is_equal_approx(Vector3(0, 0, -4)), "where the layer is")
	check_eq(player.attenuation_model, AudioStreamPlayer3D.ATTENUATION_DISABLED, "the fade is the document's, not the engine's")
	# With no camera in the test the listener is at the origin, four metres away.
	check_near(player.volume_linear, 0.0, "silent beyond the far radius")
	_place(id, Vector3(0, 0, -2))
	await frames(2)
	check_near(player.volume_linear, 0.5, "half way between the radii")
	_place(id, Vector3(0, 0, -2), 2.0)
	await frames(2)
	check_near(player.volume_linear, 1.0, "a layer twice the size is heard twice as far")

	# Facing away from the listener with a narrow cone, only the outside level is left.
	_props(id, {"attenuation": {"mode": "none"}, "modifier": {"cone": {"inner": 0.5, "band": 0.1, "outside": 0.2}}})
	await frames(2)
	check_near(player.volume_linear, 0.2, "behind the cone")
	var facing := PLACE.duplicate()
	facing.rotation = [0.0, 1.0, 0.0, 0.0]
	facing.translation = [0, 0, -2]
	run({"cmd": "set_layer", "layer": id, "transform": facing})
	await frames(2)
	check_near(player.volume_linear, 1.0, "turned to face the listener")

	_props(id, {"kind": "flat"})
	await frames(2)
	check(LayerTestKit.node(world.scene, "Sound", id) is AudioStreamPlayer, "made flat, it gets the other kind of player")
	check_eq(LayerTestKit.count(world.scene, "AudioStreamPlayer3D"), 0, "and the first is gone")


func test_a_playing_sound_keeps_to_the_timeline() -> void:
	if DisplayServer.get_name() == "headless":
		# A headless run has no audio device to keep time by. Run this one in a window:
		# scripts/godot.sh --path godot --script res://tests/run.gd -- layer_nodes
		return
	var id := _import(LayerTestKit.wav(_file("tone.wav"), 4.0, 1))
	_props(id, {"kind": "flat"})
	await frames(2)
	var player: AudioStreamPlayer = LayerTestKit.node(world.scene, "Sound", id)
	run({"cmd": "play"})
	await get_tree().create_timer(1.0).timeout
	check_near(player.get_playback_position(), world.scene.get_time(), "a second in", 0.08)
	run({"cmd": "seek", "time": LimnerEditor.to_flicks(2.5)})
	await get_tree().create_timer(0.6).timeout
	check_near(player.get_playback_position(), world.scene.get_time(), "after jumping ahead", 0.08)
	run({"cmd": "pause"})


func test_another_document_leaves_nothing_behind() -> void:
	_add("light")
	_add("camera")
	_import(LayerTestKit.png(_file("a.png"), 4, 4))
	_import(LayerTestKit.obj(_file("a.obj")))
	_import(LayerTestKit.wav(_file("a.wav"), 0.2, 1))
	await frames(2)
	var holder := LayerTestKit.holder(world.scene)
	check(holder.get_child_count() >= 8, "nodes for every layer: %d" % holder.get_child_count())
	editor.new_document()
	await frames(2)
	# What is left is the gizmo of the new document's own spawn area.
	for child in holder.get_children():
		check(child.name.begins_with("Gizmo_"), "%s was left behind" % child.name)
	check_eq(holder.get_child_count(), 3, "three parts: ring, first-spawn mark, eyes")
