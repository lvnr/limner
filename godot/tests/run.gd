extends SceneTree
## Runs the interface tests headless:
##
##   scripts/godot.sh --headless --path godot --script res://tests/run.gd
##   scripts/godot.sh --headless --path godot --script res://tests/run.gd -- layers   (only files matching "layers")
##
## Every `test_*.gd` file in this folder extends `LimnerTest`; each of its `test_*`
## methods runs against a fresh, empty document. The exit code is 1 when a test fails.


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var only := Array(OS.get_cmdline_user_args())
	var files := Array(DirAccess.get_files_at("res://tests")).filter(
		func(file: String) -> bool: return file.begins_with("test_") and file.ends_with(".gd")
	)
	files.sort()
	var count := 0
	var failed := 0
	for file: String in files:
		if not only.is_empty() and not only.any(func(word: String) -> bool: return file.contains(word)):
			continue
		var script: GDScript = load("res://tests/" + file)
		if script == null or not script.can_instantiate():
			print("FAIL %s does not load" % file)
			failed += 1
			continue
		for method: Dictionary in script.get_script_method_list():
			var method_name: String = method.name
			if not method_name.begins_with("test_"):
				continue
			var case: LimnerTest = script.new()
			root.add_child(case)
			await case.setup()
			await case.call(method_name)
			count += 1
			if case.failures.is_empty():
				print("ok   %s :: %s" % [file, method_name])
			else:
				failed += 1
				print("FAIL %s :: %s" % [file, method_name])
				for failure in case.failures:
					print("       ", failure)
			case.queue_free()
			await process_frame
	print("%d tests, %d failed" % [count, failed])
	quit(1 if failed > 0 else 0)
