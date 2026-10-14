extends LimnerTest
## The app's preferences, the Settings panel that edits them and the interface
## sounds that follow one of them.

## A settings file of the tests' own, so that they neither read nor change the user's.
const SETTINGS := "user://test_preferences.json"


func _exit_tree() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(Preferences.DEFAULT_PATH)


## Points the shared preferences at an empty file, so they are at their defaults.
func _fresh() -> Preferences:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(SETTINGS)
	return Preferences.shared()


func _write(text: String) -> void:
	var file := FileAccess.open(SETTINGS, FileAccess.WRITE)
	file.store_string(text)
	file.close()


func test_defaults_and_saving() -> void:
	var preferences := _fresh()
	check_eq(preferences.text("dominant_hand"), "right", "right-handed by default")
	check_eq(preferences.flag("haptics"), true, "haptics on")
	check_near(preferences.number("ui_volume"), 0.5, "volume half-way")
	check_eq(preferences.flag("show_button_map_at_start"), true, "button map at start")
	check_eq(preferences.flag("show_gimbal_by_default"), false, "gimbal on demand")
	check_eq(preferences.text("theme"), "dark", "the dark scheme")
	check_eq(preferences.value("autosave_minutes"), 5, "autosave every five minutes")
	var heard: Array[String] = []
	var listen := func(key: String) -> void: heard.append(key)
	preferences.changed.connect(listen)
	preferences.set_value("dominant_hand", "left")
	preferences.set_value("ui_volume", 0.8)
	preferences.set_value("ui_volume", 0.8)
	check_eq(heard, ["dominant_hand", "ui_volume"] as Array[String], "each change is reported once")
	var stored: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(SETTINGS))
	check_eq(stored.get("dominant_hand"), "left", "a change is on disk at once")
	check_near(stored.get("ui_volume"), 0.8, "with the others")
	# Values that make no sense are brought in line or left out.
	preferences.set_value("ui_volume", 7.0)
	check_near(preferences.number("ui_volume"), 1.0, "a volume above full is full")
	preferences.set_value("dominant_hand", "both")
	check_eq(preferences.text("dominant_hand"), "left", "a hand that is neither is ignored")
	preferences.set_value("haptics", "yes")
	check_eq(preferences.flag("haptics"), true, "a word is not a switch")
	preferences.set_value("autosave_minutes", 2.6)
	check_eq(preferences.value("autosave_minutes"), 3, "minutes are whole")
	preferences.changed.disconnect(listen)


func test_a_damaged_or_foreign_file_is_safe_to_load() -> void:
	var preferences := _fresh()
	_write("this is not json {")
	preferences.load_from_disk()
	check_eq(preferences.text("dominant_hand"), "right", "a damaged file gives the defaults")
	_write('{"dominant_hand": 3, "haptics": false, "ui_volume": "loud", "autosave_minutes": 900, "from_a_newer_version": [1, 2]}')
	preferences.load_from_disk()
	check_eq(preferences.text("dominant_hand"), "right", "a value of the wrong kind falls back to the default")
	check_eq(preferences.flag("haptics"), false, "a good value beside it is kept")
	check_near(preferences.number("ui_volume"), 0.5, "another wrong one falls back too")
	check_eq(preferences.value("autosave_minutes"), 120, "a number out of range is brought into it")
	preferences.set_value("theme", "dark")
	preferences.set_value("haptics", true)
	var stored: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(SETTINGS))
	check_eq(stored.get("from_a_newer_version"), [1.0, 2.0], "a setting this version does not know survives a save")
	_write("[1, 2, 3]")
	preferences.load_from_disk()
	check_eq(preferences.flag("haptics"), true, "a file that is not a list of settings gives the defaults")


func test_the_settings_panel_edits_the_preferences() -> void:
	var preferences := _fresh()
	var panel := SettingsPanel.new(editor)
	mount(panel, Vector2(280, 700))
	await frames(2)
	check(panel.get_combined_minimum_size().x <= 264.0, "fits a panel 280 wide, needs %s" % panel.get_combined_minimum_size().x)
	check_eq(panel._hand.get_value(), "right", "shows the drawing hand")
	(panel._hand.buttons["left"] as Button).pressed.emit()
	check_eq(preferences.text("dominant_hand"), "left", "the Left button changes it")
	panel._haptics.button_pressed = false
	check_eq(preferences.flag("haptics"), false, "the vibration switch")
	panel._volume.slider.value = 0.25
	check_near(preferences.number("ui_volume"), 0.25, "the volume slider")
	panel._button_map_at_start.button_pressed = false
	check_eq(preferences.flag("show_button_map_at_start"), false, "the button map switch")
	panel._gimbal.button_pressed = true
	check_eq(preferences.flag("show_gimbal_by_default"), true, "the gimbal switch")
	panel._autosave.slider.value = 0.0
	check_eq(preferences.value("autosave_minutes"), 0, "autosave can be switched off")
	check_eq(panel._autosave._value.text, "off", "and says so")
	panel._autosave.slider.value = 12.0
	check_eq(panel._autosave._value.text, "12 min", "or set in minutes")
	check_eq(panel._theme.item_count, 2, "the dark and the light scheme are offered")
	# A change made elsewhere shows in the panel.
	preferences.set_value("dominant_hand", "right")
	preferences.set_value("ui_volume", 0.9)
	await frames(1)
	check_eq(panel._hand.get_value(), "right", "the panel follows a change made elsewhere")
	check_near(panel._volume.get_value(), 0.9, "for every setting")
	var asked := [0]
	panel.button_map_requested.connect(func() -> void: asked[0] += 1)
	for node: Node in panel.find_children("*", "Button", true, false):
		var button: Button = node
		if button.text == "Show button map":
			button.pressed.emit()
		elif button.text.begins_with("Recentre"):
			check_eq(button.text, "Recentre view on the initial spawn area", "the recentre button")
			button.pressed.emit()
	check_eq(asked[0], 1, "Show button map asks for the sheet")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_gimbal_preference_sets_the_starting_option() -> void:
	var preferences := _fresh()
	check_eq(editor.get_options().show_gimbal, false, "the gimbal starts hidden")
	preferences.set_value("show_gimbal_by_default", true)
	preferences.apply_at_start(editor)
	await frames(2)
	check_eq(editor.get_options().show_gimbal, true, "with the preference on, a session starts with it showing")


func test_sounds_are_made_in_code_and_follow_the_volume() -> void:
	var preferences := _fresh()
	var sounds := Sounds.new()
	add_child(sounds)
	for sound: String in Sounds.NAMES:
		var player: AudioStreamPlayer = sounds._players.get(sound)
		check(player != null, "there is a sound called %s" % sound)
		if player == null:
			continue
		var stream: AudioStreamWAV = player.stream
		check(stream.data.size() > 400, "%s has something to play" % sound)
		check(stream.get_length() < 1.0, "%s is short: %s s" % [sound, stream.get_length()])
		check_near(player.volume_db, linear_to_db(0.5), "%s plays at the volume preference" % sound, 0.01)
	for sound: String in ["undo", "redo", "pick", "delete", "stop"]:
		check(Sounds.NAMES.has(sound), "the core's \"%s\" has a sound" % sound)
	sounds.play("no such sound")
	var hiss: AudioStreamWAV = sounds._hiss.stream
	check_eq(hiss.loop_mode, AudioStreamWAV.LOOP_FORWARD, "the painting hiss loops")
	sounds.set_painting(true)
	check_eq(sounds.is_painting(), true, "the hiss sounds while painting")
	sounds.set_painting(false)
	check_eq(sounds.is_painting(), false, "and stops with it")
	preferences.set_value("ui_volume", 1.0)
	check_near((sounds._players["click"] as AudioStreamPlayer).volume_db, 0.0, "louder when the preference says so", 0.01)
	preferences.set_value("ui_volume", 0.0)
	sounds.set_painting(true)
	check_eq(sounds.is_painting(), false, "silent at zero")
	# The hiss follows the tool: it sounds while paint goes down.
	preferences.set_value("ui_volume", 0.5)
	world.scene.input_hand(0, Vector3(0, 1, -1), Quaternion.IDENTITY, 1.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	sounds.follow_tool(world.scene)
	check_eq(sounds.is_painting(), true, "painting starts the hiss")
	world.scene.input_hand(0, Vector3(0, 1, -1), Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	sounds.follow_tool(world.scene)
	check_eq(sounds.is_painting(), false, "letting go stops it")
	# A trigger on a handle of the gimbal moves the layer: that is not paint going down.
	var paint: int = editor.status.paint_layer
	run({"cmd": "set_layer", "layer": paint, "transform": {"rotation": [0.0, 0.0, 0.0, 1.0], "scale": 1.0, "translation": [0.0, 1.2, -0.6]}})
	run({"cmd": "set_options", "values": {"show_gimbal": true}})
	run({"cmd": "set_transform_target", "layer": paint})
	run({"cmd": "set_tool", "tool": "paint"})
	var gimbal: Dictionary = world.scene.gimbal()
	check(not gimbal.is_empty(), "a gimbal to try")
	var on_arrow: Vector3 = (gimbal.position as Vector3) + Vector3.RIGHT * 0.6 * float(gimbal.size)
	world.scene.input_hand(0, on_arrow, Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	check(world.scene.tool_cursor().at_handle, "the hand is at an arrow of the gimbal")
	for step in 4:
		world.scene.input_hand(0, on_arrow + Vector3.RIGHT * 0.01 * step, Quaternion.IDENTITY, 1.0, 0.0, Vector2.ZERO, 0)
		world.scene.input_commit(0.011)
		sounds.follow_tool(world.scene)
	check(world.scene.tool_cursor().active, "the trigger works the handle")
	check_eq(sounds.is_painting(), false, "and the hiss stays off with Paint in hand")
	world.scene.input_hand(0, on_arrow, Quaternion.IDENTITY, 0.0, 0.0, Vector2.ZERO, 0)
	world.scene.input_commit(0.011)
	sounds.follow_tool(world.scene)


func test_swatches_set_in_the_colour_panel_come_back_in_new_documents() -> void:
	Preferences.use_file("user://test_palette_settings.json")
	Preferences.shared().set_value("palette", [])
	var panel := ColorPanel.new(editor)
	mount(panel)
	await frames(2)
	run({"cmd": "set_color", "color": [1.0, 0.0, 0.0]})
	await frames(2)
	panel._store_swatch(2)
	await frames(2)
	var stored := Preferences.shared().palette()
	check_eq(stored.size(), 16, "the whole palette is remembered")
	check_eq(stored[2], [1.0, 0.0, 0.0], "with the new swatch")
	editor.new_document()
	await frames(2)
	check_eq(editor.get_info().palette[2], [1.0, 0.0, 0.0], "a new document starts with it")
	check(not editor.status.dirty, "and counts as untouched")
	# Something that is not a palette is not taken for one.
	Preferences.shared().set_value("palette", [[1.0, 2.0], "red"])
	check_eq(Preferences.shared().palette().size(), 16, "nonsense is refused")
	Preferences.shared().set_value("palette", [])
	DirAccess.remove_absolute(ProjectSettings.globalize_path("user://test_palette_settings.json"))
	Preferences.use_file(Preferences.DEFAULT_PATH)


func test_the_text_font_and_the_recent_ones_are_remembered() -> void:
	var preferences := _fresh()
	check_eq(preferences.text("text_font"), "", "the built-in font by default")
	check_eq(preferences.value("recent_fonts"), [], "and nothing chosen before")
	preferences.choose_font("Alpha")
	preferences.choose_font("Beta")
	preferences.choose_font("Alpha")
	check_eq(preferences.text("text_font"), "Alpha", "the font chosen last is the one in use")
	check_eq(preferences.value("recent_fonts"), ["Alpha", "Beta"], "the latest first, each once")
	preferences.choose_font("")
	check_eq(preferences.text("text_font"), "", "the built-in font can be chosen again")
	check_eq(preferences.value("recent_fonts"), ["Alpha", "Beta"], "and is not a recent one")
	for family: String in ["C", "D", "E", "F", "G", "H"]:
		preferences.choose_font(family)
	check_eq(preferences.value("recent_fonts"), ["H", "G", "F", "E", "D", "C"], "only the last few are kept")
	var stored: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(SETTINGS))
	check_eq(stored.get("text_font"), "H", "on disk")
	_write('{"text_font": 12, "recent_fonts": ["A", 3]}')
	preferences.load_from_disk()
	check_eq(preferences.text("text_font"), "", "a font that is not a name falls back to the built-in one")
	check_eq(preferences.value("recent_fonts"), [], "a list that is not of names is refused")
