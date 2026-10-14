class_name LayerTestKit
## Helpers for the tests of pictures, models, sounds, lights and gizmos: small files
## written at test time, so that importing can be tried without keeping binary files
## in the repository, and ways to find the nodes the scene makes for a layer.


## The node that holds everything the scene shows for layers that are not paint.
static func holder(scene: LimnerScene) -> Node:
	return scene.get_node_or_null("Strokes/Layers")


## The node named `kind` ("Picture", "Model", "Sound", "Light") for a layer, or null.
static func node(scene: LimnerScene, kind: String, layer: int) -> Node:
	return scene.get_node_or_null("Strokes/Layers/%s_%d" % [kind, layer])


## The gizmo parts on screen for a layer.
static func gizmo(scene: LimnerScene, layer: int) -> Array[MeshInstance3D]:
	var parts: Array[MeshInstance3D] = []
	var from := holder(scene)
	if from == null:
		return parts
	for child in from.get_children():
		if child is MeshInstance3D and child.name.begins_with("Gizmo_%d_" % layer) and child.visible and not child.is_queued_for_deletion():
			parts.append(child)
	return parts


## How many nodes of a class the scene holds for layers, not counting those being freed.
static func count(scene: LimnerScene, type: String) -> int:
	var from := holder(scene)
	if from == null:
		return 0
	var total := 0
	for child in from.get_children():
		if child.is_class(type) and not child.is_queued_for_deletion() and not child.name.begins_with("Gizmo_"):
			total += 1
	return total


## A fresh folder for one test's files.
static func scratch_dir() -> String:
	var dir := OS.get_cache_dir().path_join("limner-tests").path_join("media-%d-%d" % [Time.get_ticks_usec(), randi()])
	DirAccess.make_dir_recursive_absolute(dir)
	return dir


static func remove(path: String) -> void:
	if DirAccess.dir_exists_absolute(path):
		for file in DirAccess.get_files_at(path):
			DirAccess.remove_absolute(path.path_join(file))
		for dir in DirAccess.get_directories_at(path):
			remove(path.path_join(dir))
	DirAccess.remove_absolute(path)


## A picture whose left half is red and right half blue. With `alpha`, the bottom
## half is see-through.
static func image(width: int, height: int, alpha := false) -> Image:
	var picture := Image.create(width, height, false, Image.FORMAT_RGBA8 if alpha else Image.FORMAT_RGB8)
	for y in height:
		for x in width:
			var color := Color.RED if x < width / 2.0 else Color.BLUE
			if alpha and y >= height / 2.0:
				color.a = 0.0
			picture.set_pixel(x, y, color)
	return picture


static func png(path: String, width: int, height: int, alpha := false) -> String:
	image(width, height, alpha).save_png(path)
	return path


static func jpg(path: String, width: int, height: int) -> String:
	image(width, height).save_jpg(path)
	return path


## The bytes of a 16-bit WAV file holding a 440 Hz tone.
static func wav_bytes(seconds: float, channels: int, rate := 22050) -> PackedByteArray:
	var frames := int(seconds * rate)
	var data := StreamPeerBuffer.new()
	data.big_endian = false
	var size := frames * channels * 2
	data.put_data("RIFF".to_ascii_buffer())
	data.put_u32(36 + size)
	data.put_data("WAVEfmt ".to_ascii_buffer())
	data.put_u32(16)
	data.put_u16(1)
	data.put_u16(channels)
	data.put_u32(rate)
	data.put_u32(rate * channels * 2)
	data.put_u16(channels * 2)
	data.put_u16(16)
	data.put_data("data".to_ascii_buffer())
	data.put_u32(size)
	for i in frames:
		var sample := int(sin(float(i) / rate * 440.0 * TAU) * 8000.0)
		for channel in channels:
			data.put_16(sample)
	return data.data_array


static func wav(path: String, seconds: float, channels: int, rate := 22050) -> String:
	return write(path, wav_bytes(seconds, channels, rate))


## About `seconds` of silence as an MP3 file: MPEG-1 layer III frames, 128 kbit/s,
## 44.1 kHz, mono, with nothing in them.
static func mp3(path: String, seconds: float) -> String:
	var data := PackedByteArray()
	for i in int(seconds * 44100.0 / 1152.0):
		var frame := PackedByteArray([0xFF, 0xFB, 0x90, 0xC4])
		frame.resize(417)
		data.append_array(frame)
	return write(path, data)


## A square made of one four-cornered face, with a colour on every corner, then a
## triangle standing on it that has normals.
static func obj(path: String, extra := "") -> String:
	var text := "\n".join([
		"# test model",
		"v -1 0 -1 1 0 0",
		"v 1 0 -1 0 1 0",
		"v 1 0 1 0 0 1",
		"v -1 0 1 1 1 1",
		"v 0 2 0 1 1 0",
		"vn 0 0 1",
		"f 1 2 3 4",
		"f 1//1 2//1 5//1",
		extra,
		"",
	])
	return write(path, text.to_utf8_buffer())


## A glTF binary holding a box 2 by 1 by 1, on a node moved 5 along X and doubled in size.
static func glb(path: String) -> String:
	var root := Node3D.new()
	var box := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	mesh.size = Vector3(2.0, 1.0, 1.0)
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(1.0, 0.0, 0.0)
	mesh.material = material
	box.mesh = mesh
	box.transform = Transform3D(Basis.from_scale(Vector3(2.0, 2.0, 2.0)), Vector3(5.0, 0.0, 0.0))
	root.add_child(box)
	var document := GLTFDocument.new()
	var state := GLTFState.new()
	document.append_from_scene(root, state)
	document.write_to_filesystem(state, path)
	root.free()
	return path


static func write(path: String, bytes: PackedByteArray) -> String:
	var file := FileAccess.open(path, FileAccess.WRITE)
	file.store_buffer(bytes)
	file.close()
	return path
