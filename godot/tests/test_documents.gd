extends LimnerTest
## The File section of the Document panel: saving, saving as, starting, opening and
## importing in the documents folder, with the questions asked in the panel. This is
## what a headset has instead of system file dialogs.

var folder := ""


func _exit_tree() -> void:
	Documents.use_folder("")
	if folder != "":
		LayerTestKit.remove(folder.get_base_dir())


func _panel() -> DocumentPanel:
	folder = LayerTestKit.scratch_dir().path_join("Limner")
	Documents.use_folder(folder)
	var panel := DocumentPanel.new(editor)
	mount(panel, Vector2(300, 1400))
	await frames(2)
	return panel


func _paint() -> void:
	run({"cmd": "add_strokes", "strokes": [{"points": [[0, 1, -1], [0.2, 1, -1]], "width": 0.01}]})
	await frames(2)


## Presses a button the way a click or a poke does, and lets the panel settle.
func _press(button: Button) -> void:
	check(button.is_visible_in_tree() and not button.disabled, "\"%s\" can be pressed" % button.text)
	button.pressed.emit()
	await frames(2)


## The buttons now in the part under the File buttons, by their text.
func _drawer_buttons(panel: DocumentPanel) -> Dictionary:
	var found := {}
	for node: Node in panel._drawer.find_children("*", "Button", true, false):
		var button: Button = node
		if button.is_visible_in_tree():
			found[button.text] = button
	return found


## Gives a file a modification time `minutes_ago`, where the system has `touch`.
func _age(path: String, minutes_ago: int) -> void:
	if OS.get_name() == "Windows":
		return
	var when := Time.get_datetime_dict_from_unix_time(int(Time.get_unix_time_from_system()) - minutes_ago * 60 + int(Time.get_time_zone_from_system().bias) * 60)
	var stamp := "%04d%02d%02d%02d%02d" % [when.year, when.month, when.day, when.hour, when.minute]
	OS.execute("touch", ["-t", stamp, path])


func test_save_names_a_new_document_and_then_saves_in_place() -> void:
	var panel := await _panel()
	check_eq(panel._file_note.text, "A new document, not saved yet.", "what the panel says about a new document")
	await _paint()
	await _press(panel._file_buttons.save)
	check_eq(panel.file_mode, "save_as", "a document without a file of its own is given a name first")
	check_eq(panel._save_name.text, "Untitled", "starting from its title")
	check(not DirAccess.dir_exists_absolute(folder), "the documents folder is not made before it is needed")

	panel._save_name.text = "Night garden"
	panel._save_name.text_changed.emit("Night garden")
	check(panel._save_note.text.contains("Night garden.limnz"), "the panel says what file that makes: %s" % panel._save_note.text)
	await _press(_drawer_buttons(panel)["Save"])
	var path := folder.path_join("Night garden.limnz")
	check(FileAccess.file_exists(path), "saved as a packed project in the documents folder, which was made")
	check_eq(editor.path, path, "which is now the document's file")
	check_eq(panel.file_mode, "", "and the name box is put away")
	await frames(2)
	check(not editor.status.dirty, "nothing is unsaved")
	check_eq(panel._file_note.text, "Kept in Limner/Night garden.limnz.", "the panel says where the document is kept")

	await _paint()
	check(panel._file_note.text.ends_with("with unsaved changes."), "and when it has changes: %s" % panel._file_note.text)
	await _press(panel._file_buttons.save)
	check_eq(panel.file_mode, "", "Save then saves in place, without asking")
	await frames(2)
	check(not editor.status.dirty, "and the changes are saved")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_save_as_asks_before_replacing_a_file() -> void:
	var panel := await _panel()
	await _paint()
	check(panel.save_as("Sketch"), "a first project")
	editor.new_document()
	await frames(2)
	await _paint()
	await _press(panel._file_buttons.save_as)
	panel._save_name.text = "Sketch"
	panel._save_name.text_changed.emit("Sketch")
	await _press(_drawer_buttons(panel)["Save"])
	check_eq(panel.file_mode, "save_as", "a name that is taken is not saved over at once")
	check(panel._save_note.text.contains("already a project called Sketch"), "the panel says so: %s" % panel._save_note.text)
	check_eq(editor.path, "", "the new document is not saved yet")
	await _press(_drawer_buttons(panel)["Replace"])
	check_eq(editor.path, folder.path_join("Sketch.limnz"), "pressed again, it replaces the file")
	check(not panel.save_as("   "), "a name of nothing is refused")
	check_eq(Documents.path_for("a/b:c"), folder.path_join("a_b_c.limnz"), "what a file name cannot hold is left out")
	check_eq(Documents.path_for("Tree.limnz"), folder.path_join("Tree.limnz"), "the ending is not doubled")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_open_lists_the_projects_newest_first_and_asks_about_changes() -> void:
	var panel := await _panel()
	await _paint()
	panel.save_as("Older")
	await _paint()
	await _paint()
	panel.save_as("Newer")
	editor.save_document(folder.path_join("Folder project.limn"))
	DirAccess.make_dir_recursive_absolute(folder.path_join("A Quill piece"))
	LayerTestKit.write(folder.path_join("A Quill piece/Quill.json"), "{}".to_utf8_buffer())
	LayerTestKit.write(folder.path_join("notes.txt"), "not a project".to_utf8_buffer())
	_age(folder.path_join("Older.limnz"), 60)
	_age(folder.path_join("Folder project.limn/limn.json"), 30)
	_age(folder.path_join("Folder project.limn/state.json"), 30)
	_age(folder.path_join("A Quill piece/Quill.json"), 90)

	var listed := Documents.projects()
	var names := listed.map(func(entry: Dictionary) -> String: return entry.name)
	if OS.get_name() != "Windows":
		check_eq(names, ["Newer", "Folder project", "Older", "A Quill piece"], "every kind of project, newest first")
	check_eq(listed.map(func(entry: Dictionary) -> String: return entry.kind).count("quill"), 1, "a Quill project folder is among them")

	editor.new_document()
	await frames(2)
	await _paint()
	await _press(panel._file_buttons.open)
	check_eq(panel.file_mode, "open", "the list is shown")
	var entries := _drawer_buttons(panel)
	check(entries.has("Older") and entries.has("Newer") and entries.has("A Quill piece (Quill)"), "with the projects' names: %s" % [entries.keys()])
	check(not entries.has("notes"), "and nothing else")

	await _press(entries["Older"])
	check_eq(panel.file_mode, "confirm", "unsaved changes are asked about first")
	check_eq(editor.path, "", "and nothing is opened yet")
	await _press(_drawer_buttons(panel)["Cancel"])
	check_eq(panel.file_mode, "", "Cancel stays")
	check(editor.status.dirty, "with the changes")

	await _press(panel._file_buttons.open)
	await _press(_drawer_buttons(panel)["Older"])
	await _press(_drawer_buttons(panel)["Don't save"])
	check_eq(editor.path, folder.path_join("Older.limnz"), "Don't save opens the project")
	check_eq(int(world.scene.info().strokes), 1, "as it was saved")
	check_eq(panel.file_mode, "", "and the list goes")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_saving_first_then_going_on() -> void:
	var panel := await _panel()
	var opened: Array[bool] = []
	panel.document_opened.connect(func(native: bool) -> void: opened.append(native))
	await _paint()
	panel.save_as("Kept")
	editor.new_document()
	await frames(2)

	# A document with no file of its own: Save asks for a name, then goes on.
	await _paint()
	await _press(panel._file_buttons.open)
	await _press(_drawer_buttons(panel)["Kept"])
	await _press(_drawer_buttons(panel)["Save"])
	check_eq(panel.file_mode, "save_as", "a new document is given a name before it is saved")
	panel._save_name.text = "Second"
	await _press(_drawer_buttons(panel)["Save"])
	check(FileAccess.file_exists(folder.path_join("Second.limnz")), "it is saved")
	check_eq(editor.path, folder.path_join("Kept.limnz"), "and then the project chosen is opened")
	check_eq(opened, [true] as Array[bool], "the panel says a Limner project was opened")

	# With a file of its own, Save saves there and goes on to a new document.
	await _paint()
	await _press(panel._file_buttons.new)
	check_eq(panel.file_mode, "confirm", "New asks about unsaved changes")
	await _press(_drawer_buttons(panel)["Save"])
	check_eq(editor.path, "", "a new document")
	check_eq(int(world.scene.info().strokes), 0, "an empty one")
	editor.open_document(folder.path_join("Kept.limnz"))
	await frames(2)
	check_eq(int(world.scene.info().strokes), 2, "the change was saved before")

	# Without changes there is nothing to ask.
	await _press(panel._file_buttons.new)
	check_eq(panel.file_mode, "", "New on a saved document does not ask")
	check_eq(editor.path, "", "it starts a new one")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_import_brings_a_file_from_the_folder_in_as_a_layer() -> void:
	var panel := await _panel()
	await _press(panel._file_buttons.import)
	check(panel._drawer.find_children("*", "Label", true, false).any(func(label: Label) -> bool: return label.is_visible_in_tree() and label.text.begins_with("Nothing to import")), "an empty folder says what could go there")
	DirAccess.make_dir_recursive_absolute(folder)
	LayerTestKit.wav(folder.path_join("tone.wav"), 0.2, 1)
	LayerTestKit.png(folder.path_join("sky.png"), 8, 4)
	LayerTestKit.write(folder.path_join("readme.txt"), "no".to_utf8_buffer())
	await _press(panel._file_buttons.import)
	check_eq(panel.file_mode, "", "pressing Import again puts the list away")
	await _press(panel._file_buttons.import)
	var entries := _drawer_buttons(panel)
	check(entries.has("tone.wav") and entries.has("sky.png"), "the sound and the picture are listed: %s" % [entries.keys()])
	check(not entries.has("readme.txt"), "what cannot be imported is not")
	var layers := int(editor.get_info().layers)
	await _press(entries["tone.wav"])
	check_eq(int(editor.get_info().layers), layers + 1, "the sound is a new layer")
	check_eq(row_named("tone").kind, "sound", "of its kind")
	check_eq(panel.file_mode, "", "and the list goes")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_dates_are_shown_as_the_clock_shows_them() -> void:
	check_eq(Documents.local_time(0), "", "no date for no time")
	var bias := int(Time.get_time_zone_from_system().bias)
	check_eq(Documents.local_time(1_000_000_000 - bias * 60), "2001-09-09 01:46", "a time in the user's own zone")
	Documents.use_folder("")
	check(Documents.folder().ends_with("Limner") or Documents.folder().ends_with("documents"), "the documents folder: %s" % Documents.folder())
