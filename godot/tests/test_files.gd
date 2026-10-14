extends LimnerTest
## Saving a project and opening it again.


func _scratch(file_name: String) -> String:
	var dir := OS.get_cache_dir().path_join("limner-tests")
	DirAccess.make_dir_recursive_absolute(dir)
	return dir.path_join("%s-%d-%s" % [Time.get_ticks_usec(), randi(), file_name])


func _remove(path: String) -> void:
	if DirAccess.dir_exists_absolute(path):
		for file in DirAccess.get_files_at(path):
			DirAccess.remove_absolute(path.path_join(file))
		for dir in DirAccess.get_directories_at(path):
			_remove(path.path_join(dir))
	DirAccess.remove_absolute(path)


func test_a_saved_project_reopens_with_its_strokes_and_working_state() -> void:
	LimnerDemo.run(editor)
	run({"cmd": "set_tool", "tool": "nudge"})
	run({"cmd": "set_color", "color": [0.25, 0.5, 0.75]})
	run({"cmd": "seek", "time": 705600000})
	world.scene.view_pan(Vector3(0.5, 0.0, 0.0))
	var placement := world.scene.viewer_from_world()
	await frames(2)
	check_eq(editor.status.dirty, true, "something to save")

	var path := _scratch("sheet.limn")
	check(editor.save_document(path), "the save works")
	await frames(2)
	check_eq(editor.status.dirty, false, "nothing left to save")
	check(FileAccess.file_exists(path.path_join("limn.json")), "the project folder has its manifest")
	check(FileAccess.file_exists(path.path_join("state.json")), "and the editor's state")

	editor.new_document()
	await frames(2)
	check_eq(row_named("Paint").strokes, 0, "a new document is empty")
	check(editor.open_document(path), "the project opens")
	await frames(2)
	check_eq(row_named("Paint").strokes, 28, "strokes after reopening")
	check_eq(editor.path_is_native, true, "it can be saved back in place")
	check_eq(editor.get_tools().tool, "nudge", "the tool in hand")
	check_near(editor.get_tools().color[1], 0.5, "the paint colour")
	check_eq(editor.status.time, 705600000, "the playhead")
	check(world.scene.viewer_from_world().is_equal_approx(placement), "the view")
	_remove(path)


func test_a_packed_project_is_one_file() -> void:
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.2, 1, -1)])
	var path := _scratch("one.limnz")
	check(editor.save_document(path), "the save works")
	check(FileAccess.file_exists(path), "one file")
	editor.new_document()
	check(editor.open_document(path), "it opens")
	await frames(2)
	check_eq(row_named("Paint").strokes, 1, "strokes after reopening")
	_remove(path)


func test_saving_where_it_must_not_reports_why() -> void:
	var folder := _scratch("notes")
	DirAccess.make_dir_recursive_absolute(folder)
	var file := FileAccess.open(folder.path_join("letter.txt"), FileAccess.WRITE)
	file.store_string("dear")
	file.close()
	check(not editor.save_document(folder), "a folder holding other things is refused")
	check_eq(messages.size(), 1, "with a message")
	check(not FileAccess.file_exists(folder.path_join("limn.json")), "and nothing is written into it")
	check(not editor.open_document(folder.path_join("missing.limn")), "a missing project does not open")
	_remove(folder)
