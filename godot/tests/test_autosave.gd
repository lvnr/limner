extends LimnerTest
## Unsaved work is copied aside and comes back after a bad ending.

const SETTINGS := "user://test_autosave_settings.json"


func _autosave() -> Autosave:
	Preferences.use_file(SETTINGS)
	Autosave.clear()
	var autosave := Autosave.new()
	autosave.editor = editor
	add_child(autosave)
	await frames(1)
	return autosave


func _paint() -> void:
	run({"cmd": "add_strokes", "strokes": [{"points": [[0, 1, -1], [0.2, 1, -1]], "width": 0.01}]})
	await frames(2)


func test_unsaved_work_is_copied_aside_and_cleared_when_it_is_safe() -> void:
	var autosave := await _autosave()
	check(Autosave.pending().is_empty(), "nothing to recover at first")
	await _paint()
	check(editor.status.dirty, "there is unsaved work")
	check(autosave.save_now(), "the copy is written")
	var note := Autosave.pending()
	check_eq(note.get("title"), "Untitled", "it says what it is a copy of")
	check(editor.status.dirty, "writing the copy does not count as saving")

	# Once the work is saved the copy is pointless.
	var saved := OS.get_user_data_dir().path_join("test_autosave.limnz")
	check(editor.save_document(saved), "saving")
	await frames(2)
	check(Autosave.pending().is_empty(), "cleared once nothing is unsaved")
	DirAccess.remove_absolute(saved)
	Autosave.clear()
	Preferences.use_file(Preferences.DEFAULT_PATH)


func test_the_timer_waits_for_its_minutes_and_for_a_quiet_moment() -> void:
	var autosave := await _autosave()
	Preferences.shared().set_value("autosave_minutes", 1)
	await _paint()
	autosave._process(59.0)
	check(Autosave.pending().is_empty(), "not before the time is up")
	run({"cmd": "play"})
	await frames(2)
	autosave._process(2.0)
	check(Autosave.pending().is_empty(), "not while the piece plays")
	run({"cmd": "pause"})
	await frames(2)
	autosave._process(0.1)
	check(not Autosave.pending().is_empty(), "then at the first quiet moment")
	Autosave.clear()
	Preferences.shared().set_value("autosave_minutes", 0)
	autosave._process(100000.0)
	check(Autosave.pending().is_empty(), "never when switched off")
	Autosave.clear()
	Preferences.use_file(Preferences.DEFAULT_PATH)


func test_work_left_from_a_bad_ending_comes_back_as_unsaved() -> void:
	var first := await _autosave()
	await _paint()
	check(first.save_now(), "a copy from the session that ends badly")
	first.free()

	# The next session: a fresh document, and a copy waiting.
	editor.new_document()
	await frames(2)
	var autosave := Autosave.new()
	autosave.editor = editor
	add_child(autosave)
	await frames(2)
	check(not Autosave.pending().is_empty(), "a clean new document does not clear a copy that is waiting")
	check_eq(int(world.scene.info().strokes), 0, "nothing recovered until asked")
	check(autosave.recover(), "recovering")
	await frames(2)
	check_eq(int(world.scene.info().strokes), 1, "the stroke is back")
	check(editor.status.dirty, "and still counts as unsaved")
	check_eq(editor.path, "", "it is not a document of its own")

	# Set aside instead, it is kept under another name and no longer offered.
	editor.new_document()
	await frames(2)
	await _paint()
	autosave.save_now()
	var waiting := Autosave.new()
	waiting.editor = editor
	add_child(waiting)
	var kept := waiting.set_aside()
	check(kept.ends_with(".limnz") and FileAccess.file_exists(kept), "put aside: %s" % kept)
	check(Autosave.pending().is_empty(), "and not offered again")
	DirAccess.remove_absolute(kept)
	Autosave.clear()
	Preferences.use_file(Preferences.DEFAULT_PATH)


## A session that ends badly leaves a copy; the next one starts on a fresh document
## with the copy waiting, as at a real start.
func _next_session_with_a_copy() -> Autosave:
	var first := await _autosave()
	await _paint()
	check(first.save_now(), "a copy from the session that ends badly")
	first.free()
	editor.new_document()
	await frames(2)
	var autosave := Autosave.new()
	autosave.editor = editor
	add_child(autosave)
	await frames(2)
	return autosave


func test_recovered_work_keeps_its_copy_until_it_is_saved() -> void:
	var autosave := await _next_session_with_a_copy()
	check(not autosave.is_settled(), "the copy waits for an answer")
	check(autosave.recover(), "recovering")
	check(autosave.is_settled(), "which answers it")
	check(not Autosave.pending().is_empty(), "the copy is still there right after recovering")
	await frames(3)
	check(not Autosave.pending().is_empty(), "and frames later, when the change signals have come")
	check(editor.status.dirty, "the work is unsaved")

	# A second bad ending before the next autosave: the work is still there.
	autosave.free()
	editor.new_document()
	await frames(2)
	var again := Autosave.new()
	again.editor = editor
	add_child(again)
	await frames(2)
	check(again.recover(), "recovered a second time")
	await frames(2)
	check_eq(int(world.scene.info().strokes), 1, "with the stroke")

	var saved := OS.get_user_data_dir().path_join("test_autosave_recovered.limnz")
	check(editor.save_document(saved), "saving the recovered work")
	await frames(2)
	check(Autosave.pending().is_empty(), "is what clears the copy")
	DirAccess.remove_absolute(saved)
	Autosave.clear()
	Preferences.use_file(Preferences.DEFAULT_PATH)


func test_quitting_before_the_offer_is_answered_keeps_the_copy() -> void:
	var autosave := await _next_session_with_a_copy()
	check(not editor.status.dirty, "the document of the new session is clean")
	autosave.quitting()
	check(not Autosave.pending().is_empty(), "quitting with the question unanswered keeps the copy for next time")
	autosave.discard()
	check(autosave.is_settled() and Autosave.pending().is_empty(), "discarding is an answer, and removes it")

	# Answered, a clean quit leaves nothing behind, and work that would be lost
	# when the headset's system ends the session is copied first.
	await _paint()
	check(autosave.keep_for_next_time(), "unsaved work is kept when the app closes without asking")
	check(not Autosave.pending().is_empty(), "as a copy")
	autosave.quitting()
	check(Autosave.pending().is_empty(), "a deliberate quit removes the copy")
	var saved := OS.get_user_data_dir().path_join("test_autosave_quit.limnz")
	editor.save_document(saved)
	await frames(2)
	check(not autosave.keep_for_next_time(), "with nothing unsaved there is nothing to keep")
	check(Autosave.pending().is_empty(), "and no copy is written")
	DirAccess.remove_absolute(saved)
	Autosave.clear()
	Preferences.use_file(Preferences.DEFAULT_PATH)
