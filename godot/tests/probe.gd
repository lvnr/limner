extends SceneTree

func _init() -> void:
	var ok := ClassDB.class_exists("LimnerProbe")
	print("class_exists: ", ok)
	if ok:
		var p = ClassDB.instantiate("LimnerProbe")
		print("hello: ", p.hello())
		p.free()
	print("engine: ", Engine.get_version_info().string)
	quit(0 if ok else 1)
