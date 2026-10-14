extends LimnerTest
## Text written in strokes.

## A settings file of the tests' own, so that the font chosen here is not the user's.
const SETTINGS := "user://test_text_settings.json"
## Families of which at least one is installed on most systems.
const COMMON_FONTS := ["Helvetica", "Arial", "Times New Roman", "DejaVu Sans", "Liberation Sans", "Noto Sans"]


func _exit_tree() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(Preferences.DEFAULT_PATH)


## Points the shared preferences at an empty file, so they are at their defaults.
func _fresh_preferences() -> Preferences:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(SETTINGS)
	return Preferences.shared()


## The lowest and highest point of a stroke, as [left, low, right, high].
func _bounds(strokes: Array) -> Array[float]:
	var box: Array[float] = [INF, INF, -INF, -INF]
	for stroke: Dictionary in strokes:
		for point: Array in stroke.points:
			box[0] = minf(box[0], point[0])
			box[1] = minf(box[1], point[1])
			box[2] = maxf(box[2], point[0])
			box[3] = maxf(box[3], point[1])
	return box


func _outlines(strokes: Array[Dictionary]) -> Array:
	return strokes.filter(func(stroke: Dictionary) -> bool: return stroke.brush == "cylinder")


## An installed font family that has outlines, or "" when there is none.
func _some_system_font() -> String:
	var installed := TextStrokes.system_fonts()
	var candidates: Array = COMMON_FONTS.filter(func(family: String) -> bool: return installed.has(family))
	candidates.append_array(Array(installed).slice(0, 20))
	for family: String in candidates:
		var font := TextStrokes.font_named(family)
		if font != null and not TextStrokes.build("H", font, 0.1, Transform3D.IDENTITY, false).is_empty():
			return family
	return ""


func test_a_letter_becomes_its_outline_and_its_filling() -> void:
	var frame := Transform3D(Basis.IDENTITY, Vector3(0.0, 1.0, -1.0))
	var outline := TextStrokes.build("L", ThemeDB.fallback_font, 0.2, frame, false)
	check_eq(outline.size(), 1, "an L is one outline")
	var points: Array = outline[0].points
	check(points.size() >= 7, "with a corner at each turn, and closed")
	check_eq(points[0], points[points.size() - 1], "the outline ends where it began")
	var low := INF
	var high := -INF
	var left := INF
	var right := -INF
	for point: Array in points:
		check_near(point[2], -1.0, "it stands in the plane of the frame")
		left = minf(left, point[0])
		right = maxf(right, point[0])
		low = minf(low, point[1])
		high = maxf(high, point[1])
	check(high - low > 0.1 and high - low < 0.2, "a capital is most of the letter height: %s" % (high - low))
	check(right - left < high - low, "an L is taller than it is wide")
	check_near((left + right) * 0.5, 0.0, "centred on the frame", 0.01)

	var solid := TextStrokes.build("L", ThemeDB.fallback_font, 0.2, frame, true)
	var rows := solid.filter(func(stroke: Dictionary) -> bool: return stroke.brush == "ribbon")
	check(rows.size() > 20, "filled row by row: %d rows" % rows.size())
	# The stem of the L is narrow; its foot is as wide as the letter.
	var widest := 0.0
	var narrowest := INF
	for row: Dictionary in rows:
		var span: float = row.points[1][0] - row.points[0][0]
		widest = maxf(widest, span)
		narrowest = minf(narrowest, span)
	check(widest > narrowest * 2.5, "the foot is wider than the stem")
	check(widest <= right - left + 1e-4, "and no row leaves the outline")


func test_letters_with_holes_are_not_filled_in() -> void:
	var frame := Transform3D.IDENTITY
	var strokes := TextStrokes.build("O", ThemeDB.fallback_font, 1.0, frame, true)
	var outlines := strokes.filter(func(stroke: Dictionary) -> bool: return stroke.brush == "cylinder")
	check_eq(outlines.size(), 2, "an O has an outside and an inside")
	# Across its middle an O is two runs of paint with a gap between.
	var rows := strokes.filter(func(stroke: Dictionary) -> bool:
		return stroke.brush == "ribbon" and absf(stroke.points[0][1] - 0.05) < 0.03
	)
	var level: float = rows[0].points[0][1]
	var middle := rows.filter(func(stroke: Dictionary) -> bool: return is_equal_approx(stroke.points[0][1], level))
	check_eq(middle.size(), 2, "two spans across the middle of the O")


func test_text_is_added_as_selected_strokes_in_one_undo_step() -> void:
	run({"cmd": "set_color", "color": [1.0, 0.0, 0.0]})
	var count := editor.add_text("Hi", 0.1)
	await frames(2)
	check(count > 20, "strokes for two letters: %d" % count)
	check_eq(int(world.scene.info().strokes), count, "all in the document")
	check_eq(int(editor.status.selection_count), count, "and selected")
	check_eq(editor.status.tool, "select", "with the Selection tool in hand to move them")
	check_eq(editor.status.undo_label, "Add strokes", "as one step")
	editor.undo()
	await frames(2)
	check_eq(int(world.scene.info().strokes), 0, "taken back at once")
	check_eq(editor.add_text("   ", 0.1), 0, "nothing to write")
	check_eq(messages.size(), 1, "and it says so")


func test_the_tools_panel_writes_what_is_typed() -> void:
	_fresh_preferences()
	var panel := ToolsPanel.new(editor)
	mount(panel)
	await frames(2)
	panel._text.text = "A"
	panel._text_height.set_value(0.05)
	panel._text_solid.button_pressed = false
	panel._add_text()
	await frames(2)
	check_eq(int(world.scene.info().strokes), 2, "an A in outline: outside and inside")


func test_two_lines_make_two_rows_one_above_the_other() -> void:
	var font := ThemeDB.fallback_font
	var one := _bounds(TextStrokes.build("I", font, 0.2, Transform3D.IDENTITY, false))
	var two := _outlines(TextStrokes.build("I\nI", font, 0.2, Transform3D.IDENTITY, false))
	check_eq(two.size(), 2, "two letters")
	if two.size() != 2:
		return
	var upper := _bounds([two[0]])
	var lower := _bounds([two[1]])
	check(upper[1] > lower[3], "the first line is above the second: %s over %s" % [upper, lower])
	check_near(upper[0], lower[0], "one straight above the other")
	var pitch := TextStrokes.line_spacing(font) * 0.2
	check_near(upper[1] - lower[1], pitch, "a line apart")
	check(pitch >= 0.2 * TextStrokes.LINE_SPACING_MIN - 1e-6 and pitch <= 0.2 * TextStrokes.LINE_SPACING_MAX + 1e-6, "a sensible line spacing: %s" % pitch)
	check_near((upper[1] + lower[1]) * 0.5, one[1], "the block stays centred where a single line would be")
	# Line breaks of any kind; empty lines in between count, at the ends they do not.
	check_eq(TextStrokes.split_lines("\nA\r\n\nB  \rC\n\n"), PackedStringArray(["A", "", "B", "C"]), "lines")
	var gap := _outlines(TextStrokes.build("I\n\nI", font, 0.2, Transform3D.IDENTITY, false))
	check_near(_bounds([gap[0]])[1] - _bounds([gap[1]])[1], pitch * 2.0, "an empty line leaves a gap")


func test_alignment_moves_the_short_lines() -> void:
	var font := ThemeDB.fallback_font
	var long_line := _bounds(_outlines(TextStrokes.build("WWWW", font, 0.1, Transform3D.IDENTITY, false)))
	var places := {}
	for align: HorizontalAlignment in [HORIZONTAL_ALIGNMENT_LEFT, HORIZONTAL_ALIGNMENT_CENTER, HORIZONTAL_ALIGNMENT_RIGHT]:
		var outlines := _outlines(TextStrokes.build("WWWW\nI", font, 0.1, Transform3D.IDENTITY, false, align))
		check_eq(outlines.size(), 5, "four Ws and an I")
		var top := _bounds(outlines.slice(0, 4))
		check_near(top[0], long_line[0], "the long line stays where it is (%d)" % align)
		places[align] = _bounds([outlines[4]])
	var left: Array[float] = places[HORIZONTAL_ALIGNMENT_LEFT]
	var centre: Array[float] = places[HORIZONTAL_ALIGNMENT_CENTER]
	var right: Array[float] = places[HORIZONTAL_ALIGNMENT_RIGHT]
	check(left[0] - long_line[0] < 0.02, "on the left, the I starts where the Ws do: %s and %s" % [left[0], long_line[0]])
	check_near((centre[0] + centre[2]) * 0.5, 0.0, "centred, it is in the middle", 0.005)
	check(long_line[2] - right[2] < 0.02, "on the right, it ends where they do: %s and %s" % [right[2], long_line[2]])
	check(left[0] < centre[0] and centre[0] < right[0], "left, centre, right")


func test_a_system_font_makes_strokes_of_its_own() -> void:
	var family := _some_system_font()
	if family == "":
		print("       (skipped: no system font with outlines here)")
		return
	check(TextStrokes.is_installed(family), "%s is installed" % family)
	var font := TextStrokes.font_named(family)
	check(font is SystemFont, "%s is a system font" % family)
	var own := TextStrokes.build("Ag", font, 0.1, Transform3D.IDENTITY, true)
	var built_in := TextStrokes.build("Ag", ThemeDB.fallback_font, 0.1, Transform3D.IDENTITY, true)
	check(own.size() > 10, "%s writes strokes: %d" % [family, own.size()])
	check(_bounds(own) != _bounds(built_in), "and they are its letters, not the built-in font's")
	var fonts := TextStrokes.system_fonts()
	check(fonts.size() > 0, "fonts are listed")
	var sorted := Array(fonts)
	sorted.sort_custom(func(a: String, b: String) -> bool: return a.naturalnocasecmp_to(b) < 0)
	check_eq(Array(fonts), sorted, "in order")


func test_bold_and_italic() -> void:
	var font := ThemeDB.fallback_font
	var plain := _bounds(_outlines(TextStrokes.build("l", font, 0.2, Transform3D.IDENTITY, false)))
	var bold := TextStrokes.font_named("", true, false)
	check(bold is FontVariation, "the built-in font has no bold face, so it is thickened")
	var heavy := _bounds(_outlines(TextStrokes.build("l", bold, 0.2, Transform3D.IDENTITY, false)))
	check(heavy[2] - heavy[0] > (plain[2] - plain[0]) * 1.1, "bold letters are wider: %s against %s" % [heavy[2] - heavy[0], plain[2] - plain[0]])
	var leaning := _outlines(TextStrokes.build("l", TextStrokes.font_named("", false, true), 0.2, Transform3D.IDENTITY, false))
	# The top of a slanted l is further right than its foot.
	var box := _bounds(leaning)
	var top_x := INF
	var foot_x := INF
	for point: Array in leaning[0].points:
		if point[1] > box[3] - 0.005:
			top_x = minf(top_x, point[0])
		if point[1] < box[1] + 0.005:
			foot_x = minf(foot_x, point[0])
	var lean := (top_x - foot_x) / (box[3] - box[1])
	check(lean > TextStrokes.SLANT * 0.5, "italic letters lean forward: %s sideways per unit of height" % lean)
	check(TextStrokes.font_named("", false, false) == font, "neither is the plain built-in font")


func test_a_missing_font_falls_back_to_the_built_in_one() -> void:
	check_eq(TextStrokes.is_installed("No Such Font Limner 1"), false, "not installed")
	check(TextStrokes.font_named("No Such Font Limner 1") == null, "so there is no font of that name")
	var preferences := _fresh_preferences()
	preferences.choose_font("No Such Font Limner 1")
	var notes: Array[String] = []
	editor.message.connect(func(text: String, _is_error: bool) -> void: notes.append(text))
	var panel := ToolsPanel.new(editor)
	mount(panel)
	await frames(2)
	check_eq(panel._text_font.get_item_text(panel._text_font.selected), "No Such Font Limner 1 (not installed)", "the list says the font is missing")
	panel._text.text = "A"
	panel._text_solid.button_pressed = false
	panel._add_text()
	await frames(2)
	check_eq(int(world.scene.info().strokes), 2, "the text is written all the same, in the built-in font")
	check(notes.size() > 0 and notes.back().contains("built-in font") and notes.back().contains("No Such Font Limner 1"), "and it says so: %s" % [notes])
	check(messages.is_empty(), "not as an error: %s" % [messages])
	check_eq(preferences.text("text_font"), "No Such Font Limner 1", "the choice is kept, for when the font is back")


func test_the_panel_chooses_and_remembers_a_font() -> void:
	var preferences := _fresh_preferences()
	var panel := ToolsPanel.new(editor)
	mount(panel)
	await frames(2)
	check(panel.get_combined_minimum_size().x <= 300.0, "the panel fits a dock 300 wide, needs %s" % panel.get_combined_minimum_size().x)
	check_eq(panel._text_font.get_item_text(panel._text_font.selected), ToolsPanel.BUILT_IN_FONT, "the built-in font to start with")
	check_eq(panel._text_font.item_count, TextStrokes.system_fonts().size() + 2, "then every installed font, after a separator")
	var family := _some_system_font()
	if family == "":
		print("       (skipped the rest: no system font with outlines here)")
		return
	# Searching narrows the list; Enter takes the first match.
	panel._font_search.text = family
	panel._font_search.text_changed.emit(family)
	check(panel._text_font.item_count < 20, "a search leaves a short list: %d" % panel._text_font.item_count)
	panel._font_search.text_submitted.emit(family)
	await frames(2)
	var chosen := preferences.text("text_font")
	check(chosen.containsn(family), "the first match is chosen: %s" % chosen)
	check_eq(preferences.value("recent_fonts"), [chosen], "and is the most recent")
	check_eq(str(panel._text_font.get_item_metadata(panel._text_font.selected)), chosen, "the list shows it")
	# Picking from the list works the same way.
	panel._font_search.text = ""
	panel._font_search.text_changed.emit("")
	panel._text_font.select(0)
	panel._text_font.item_selected.emit(0)
	await frames(2)
	check_eq(preferences.text("text_font"), "", "back to the built-in font")
	check_eq(panel._text_font.get_item_text(1), chosen, "the recent font stays at the top of the list")
	# The text is written in the font chosen.
	preferences.choose_font(family)
	await frames(1)
	panel._text.text = "Hi"
	panel._text_align.set_value("left")
	panel._add_text()
	await frames(2)
	var own := TextStrokes.build("Hi", TextStrokes.font_named(family), 0.1, Transform3D.IDENTITY, true)
	check_eq(int(world.scene.info().strokes), own.size(), "the text is in %s" % family)
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_several_lines_from_the_panel_and_the_headset_keyboard() -> void:
	_fresh_preferences()
	var panel := ToolsPanel.new(editor)
	mount(panel)
	await frames(2)
	# The headset's keyboard types into the box like a real one: Enter starts a line.
	var view := SubViewport.new()
	view.size = Vector2i(300, 200)
	view.gui_embed_subwindows = true
	add_child(view)
	var box := panel._text
	panel._text.get_parent().remove_child(box)
	view.add_child(box)
	await frames(1)
	box.grab_focus()
	var keyboard := XRKeyboard.new()
	add_child(keyboard)
	keyboard.target = view
	keyboard._type("i")
	keyboard._press(KEY_ENTER)
	keyboard._type("i")
	keyboard._type("i")
	check_eq(box.text, "i\nii", "Enter makes a new line in the text box")
	panel._text_solid.button_pressed = false
	panel._text_align.set_value("right")
	panel._add_text()
	await frames(2)
	check_eq(int(world.scene.info().strokes), 6, "three letters on two lines, each a stem and a dot")
	check(messages.is_empty(), "no errors: %s" % [messages])
