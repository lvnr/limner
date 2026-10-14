extends LimnerTest
## Bringing picture, sound and model files into the document.

var _dir := ""


func _file(file_name: String) -> String:
	if _dir == "":
		_dir = LayerTestKit.scratch_dir()
	return _dir.path_join(file_name)


func _exit_tree() -> void:
	if _dir != "":
		LayerTestKit.remove(_dir)


## Imports a file that should import, and returns the new layer's id.
func _import(path: String) -> int:
	var result := world.scene.import_file(path)
	check(result.ok, "%s imports: %s" % [path.get_file(), result.error])
	return int(result.layer)


## The reason a file that should not import was refused.
func _refusal(path: String) -> String:
	var result := world.scene.import_file(path)
	check(not result.ok, "%s is refused" % path.get_file())
	check_eq(int(result.layer), -1, "no layer for %s" % path.get_file())
	return result.error


func test_a_png_becomes_a_picture_layer_in_one_undo_step() -> void:
	var id := _import(LayerTestKit.png(_file("Tree.png"), 8, 4))
	await frames(2)
	var row := editor.row(id)
	check_eq(row.get("kind"), "picture", "kind")
	check_eq(row.get("name"), "Tree", "named after the file")
	check_eq(row.get("current"), true, "selected")
	check_eq(editor.status.undo_label, "Import picture", "what undo will take back")
	check_eq(editor.status.dirty, true, "something to save")
	var details := world.scene.layer_details(id)
	check_eq(details.props.projection, "flat", "a flat picture")
	var node: MeshInstance3D = LayerTestKit.node(world.scene, "Picture", id)
	check(node != null and node.visible, "the picture is on screen")
	var material: ShaderMaterial = node.material_override
	var texture: Texture2D = material.get_shader_parameter("picture")
	check_eq(texture.get_size(), Vector2(8, 4), "texture size")
	check(node.mesh is QuadMesh, "drawn on a quad")
	check_near(node.global_transform.basis.x.length(), 2.0, "twice as wide as tall")
	check_near(node.global_transform.basis.y.length(), 1.0, "one unit tall")

	editor.undo()
	await frames(2)
	check(editor.row(id).is_empty(), "undo takes the layer away")
	check_eq(LayerTestKit.count(world.scene, "MeshInstance3D"), 0, "and its node")
	editor.redo()
	await frames(2)
	check(LayerTestKit.node(world.scene, "Picture", id) != null, "redo brings both back")


func test_a_jpeg_and_a_see_through_png_import() -> void:
	var photo := _import(LayerTestKit.jpg(_file("photo.jpeg"), 16, 16))
	var cutout := _import(LayerTestKit.png(_file("cutout.png"), 4, 4, true))
	await frames(2)
	var photo_node: MeshInstance3D = LayerTestKit.node(world.scene, "Picture", photo)
	var cutout_node: MeshInstance3D = LayerTestKit.node(world.scene, "Picture", cutout)
	var opaque: ShaderMaterial = photo_node.material_override
	var see_through: ShaderMaterial = cutout_node.material_override
	check(opaque.shader.resource_path.ends_with("picture_flat_opaque.gdshader"), "a photograph is drawn without holes")
	check(see_through.shader.resource_path.ends_with("picture_flat_see_through.gdshader"), "a cut-out keeps its holes")
	check(see_through.get_shader_parameter("blue_noise") != null, "and has the noise to dither them with")
	var rows := editor.get_rows()
	check_eq(rows[rows.size() - 1].name, "cutout", "each import lands after the one before")
	check_eq(rows[rows.size() - 2].name, "photo", "in the order they came")


func test_files_that_are_not_what_they_say_are_refused_with_a_reason() -> void:
	var before := editor.get_rows().size()
	# A header that claims 20000 pixels across.
	var header := PackedByteArray([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0, 0, 0x4E, 0x20, 0, 0, 0, 16, 8, 2, 0, 0, 0])
	var reason := _refusal(LayerTestKit.write(_file("huge.png"), header))
	check(reason.contains("20000 by 16") and reason.contains("16384"), "too large: %s" % reason)
	reason = _refusal(LayerTestKit.write(_file("notes.png"), "hello".to_utf8_buffer()))
	check(reason.contains("not a PNG picture"), "not a picture: %s" % reason)
	# The header of a small picture with nothing behind it.
	header[18] = 0
	header[19] = 16
	reason = _refusal(LayerTestKit.write(_file("cut.png"), header))
	check(reason.contains("could not be decoded"), "damaged: %s" % reason)
	reason = _refusal(LayerTestKit.write(_file("noise.wav"), "RIFF0000WAVEfmt ".to_utf8_buffer()))
	check(reason.contains("not a WAV sound"), "not a sound: %s" % reason)
	reason = _refusal(LayerTestKit.write(_file("letter.txt"), "dear".to_utf8_buffer()))
	check(reason.begins_with(".txt files cannot be imported"), "unknown kind: %s" % reason)
	reason = _refusal(_file("missing.obj"))
	check(reason.contains("could not be read"), "missing: %s" % reason)
	reason = _refusal(LayerTestKit.write(_file("points.obj"), "v 0 0 0\nv 1 0 0\n".to_utf8_buffer()))
	check(reason.contains("no triangles"), "empty model: %s" % reason)
	reason = _refusal(LayerTestKit.write(_file("broken.glb"), "glTF but not really".to_utf8_buffer()))
	check(reason.contains("could not be read"), "broken glTF: %s" % reason)
	await frames(2)
	check_eq(editor.get_rows().size(), before, "nothing was added")
	check_eq(editor.status.dirty, false, "and nothing changed")
	check_eq(editor.import_file(_file("letter.txt")), -1, "the editor reports it")
	check_eq(messages.size(), 1, "as one message")
	check(messages[0].begins_with("Could not import letter.txt"), "that says which file: %s" % [messages])


func test_a_mono_sound_is_a_point_in_space_and_a_stereo_one_is_everywhere() -> void:
	var voice := _import(LayerTestKit.wav(_file("voice.wav"), 0.5, 1))
	var music := _import(LayerTestKit.wav(_file("music.wav"), 0.25, 2))
	await frames(2)
	check_eq(editor.row(voice).get("kind"), "sound", "kind")
	check_eq(editor.status.undo_label, "Import sound", "what undo will take back")
	check_eq(world.scene.layer_details(voice).props.kind, "positional", "one channel")
	check_eq(world.scene.layer_details(music).props.kind, "flat", "two channels")
	var voice_player := LayerTestKit.node(world.scene, "Sound", voice)
	var music_player := LayerTestKit.node(world.scene, "Sound", music)
	check(voice_player is AudioStreamPlayer3D, "a player in space")
	check(music_player is AudioStreamPlayer, "a player that is everywhere")
	var stream: AudioStream = voice_player.get("stream")
	check_near(stream.get_length(), 0.5, "the stream is as long as the file", 0.01)
	stream = music_player.get("stream")
	check_near(stream.get_length(), 0.25, "the second stream too", 0.01)
	check(stream is AudioStreamWAV and (stream as AudioStreamWAV).stereo, "in stereo")


func test_an_mp3_imports() -> void:
	var id := _import(LayerTestKit.mp3(_file("hum.mp3"), 1.0))
	await frames(2)
	check_eq(world.scene.layer_details(id).props.kind, "positional", "one channel")
	var player := LayerTestKit.node(world.scene, "Sound", id)
	var stream: AudioStream = player.get("stream")
	check(stream is AudioStreamMP3, "an MP3 stream")
	check_near(stream.get_length(), 1.0, "about a second long", 0.1)


func test_an_obj_becomes_a_model() -> void:
	var result := world.scene.import_file(LayerTestKit.obj(_file("Rock.obj"), "mtllib rock.mtl"))
	check(result.ok, "it imports: %s" % result.error)
	var warnings: PackedStringArray = result.warnings
	check_eq(warnings.size(), 2, "what was left out is said: %s" % [warnings])
	check(warnings[0].contains("materials and textures"), "materials: %s" % [warnings])
	check(warnings[1].contains("normals"), "normals on some faces only: %s" % [warnings])
	var id := int(result.layer)
	await frames(2)
	check_eq(editor.row(id).get("kind"), "model", "kind")
	check_eq(editor.row(id).get("name"), "Rock", "named after the file")
	check_eq(editor.status.undo_label, "Import model", "what undo will take back")
	check_eq(world.scene.layer_details(id).props.shading, "unlit", "drawn as it is until asked otherwise")
	var node: MeshInstance3D = LayerTestKit.node(world.scene, "Model", id)
	check(node != null and node.visible, "the model is on screen")
	var arrays := node.mesh.surface_get_arrays(0)
	var positions: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
	# Three triangles; with face normals no two of them share a corner.
	check_eq(indices.size(), 9, "the four-cornered face became two triangles")
	check_eq(positions.size(), 9, "vertices")
	var colors: PackedColorArray = arrays[Mesh.ARRAY_COLOR]
	check(colors[0].is_equal_approx(Color.RED), "vertex colours came along: %s" % colors[0])
	check_eq(node.mesh.get_aabb().size, Vector3(2, 2, 2), "its size")
	# The mesh is centred on itself and the node carries the centre.
	check(node.global_position.is_equal_approx(Vector3(0, 1, 0)), "where it sits: %s" % node.global_position)


func test_a_gltf_file_becomes_one_model_with_its_nodes_in_place() -> void:
	var result := world.scene.import_file(LayerTestKit.glb(_file("crate.glb")))
	check(result.ok, "it imports: %s" % result.error)
	var id := int(result.layer)
	await frames(2)
	check_eq(editor.row(id).get("kind"), "model", "kind")
	var node: MeshInstance3D = LayerTestKit.node(world.scene, "Model", id)
	check(node != null and node.visible, "the model is on screen")
	var arrays := node.mesh.surface_get_arrays(0)
	var positions: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
	check_eq(positions.size(), 24, "a box has four corners on each of six sides")
	check_eq(indices.size(), 36, "and twelve triangles")
	# The node holding the box was moved and doubled; that is part of the model now.
	check(node.mesh.get_aabb().size.is_equal_approx(Vector3(4, 2, 2)), "its size: %s" % node.mesh.get_aabb().size)
	check(node.global_position.is_equal_approx(Vector3(5, 0, 0)), "where it sits: %s" % node.global_position)
	var colors: PackedColorArray = arrays[Mesh.ARRAY_COLOR]
	check(colors[0].is_equal_approx(Color.RED), "the material's colour came along: %s" % colors[0])
	var normals: PackedVector3Array = arrays[Mesh.ARRAY_NORMAL]
	check_near(normals[0].length(), 1.0, "normals keep their length under the node's scale")


func test_an_imported_model_survives_saving_and_opening() -> void:
	var id := _import(LayerTestKit.obj(_file("Rock.obj")))
	_import(LayerTestKit.png(_file("Tree.png"), 8, 4))
	_import(LayerTestKit.wav(_file("voice.wav"), 0.5, 1))
	run({"cmd": "set_layer_kind", "layer": id, "values": {"shading": "lit", "wireframe": true}})
	var path := _file("media.limn")
	check(editor.save_document(path), "the save works")
	editor.new_document()
	await frames(2)
	check_eq(LayerTestKit.count(world.scene, "Node"), 0, "a new document shows none of it")
	check(editor.open_document(path), "the project opens")
	await frames(2)
	check_eq(row_named("Rock").get("kind"), "model", "the model")
	check_eq(row_named("Tree").get("kind"), "picture", "the picture")
	check_eq(row_named("voice").get("kind"), "sound", "the sound")
	check_eq(LayerTestKit.count(world.scene, "MeshInstance3D"), 2, "the model and the picture are on screen")
	check_eq(LayerTestKit.count(world.scene, "AudioStreamPlayer3D"), 1, "and the sound has its player")
	var model: MeshInstance3D = LayerTestKit.node(world.scene, "Model", int(row_named("Rock").id))
	check(model.get_node("Wire").visible, "the wireframe setting came back")
