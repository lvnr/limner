extends LimnerTest
## The building blocks of the Quill-style panels: keys, key grids, cyclers, pagers,
## step keys, the opacity ramp, and the fixed-size panel with its `fits` check.


func test_keys_are_sized_for_the_interface_in_use() -> void:
	var desktop := Kit.key("paint", "Paint", Callable())
	check_eq(desktop.custom_minimum_size, Vector2(Kit.u(Kit.KEY), Kit.u(Kit.KEY)), "a key is KEY design pixels")
	check_eq(int(desktop.custom_minimum_size.x), roundi(Kit.KEY * Kit.DESKTOP_SCALE), "smaller on the desktop")
	check_eq(desktop.theme_type_variation, "Key", "with the theme's key look")
	Kit.set_headset(true)
	var headset := Kit.key("paint", "Paint", Callable())
	check_eq(int(headset.custom_minimum_size.x), Kit.KEY, "and full size in a headset")
	check(Kit.text_size("body") > 14, "where text is larger too")
	Kit.set_headset(false)
	desktop.free()
	headset.free()


func test_a_key_grid_keeps_one_key_down() -> void:
	var grid := KeyGrid.new(3)
	mount(grid)
	var picked: Array[String] = []
	grid.chosen.connect(func(key: String) -> void: picked.append(key))
	for mode: String in ["normal", "darken", "multiply", "overlay"]:
		grid.add_text(mode, mode.capitalize())
	check_eq(grid.columns, 3, "three to a row")
	grid.set_value("darken")
	check_eq(picked, [] as Array[String], "setting a value is not reported")
	(grid.buttons["overlay"] as Button).pressed.emit()
	check_eq(grid.get_value(), "overlay", "a press moves the choice")
	check(not (grid.buttons["darken"] as Button).button_pressed, "and lets the other key up")
	(grid.buttons["overlay"] as Button).pressed.emit()
	check(grid.buttons["overlay"].button_pressed, "pressing the key that is down leaves it down")
	check_eq(picked, ["overlay", "overlay"] as Array[String], "every press is reported")


func test_a_cycler_steps_through_its_choices() -> void:
	var cycler := Cycler.new("Frame rate", [["24", "24 fps"], ["25", "25 fps"], ["30", "30 fps"]])
	mount(cycler)
	var picked: Array[String] = []
	cycler.chosen.connect(func(key: String) -> void: picked.append(key))
	cycler.set_value("24")
	check(cycler._before.disabled, "nothing before the first")
	cycler._after.pressed.emit()
	cycler._after.pressed.emit()
	check_eq(cycler.get_value(), "30", "two steps on")
	check(cycler._after.disabled, "nothing after the last")
	cycler._after.pressed.emit()
	check_eq(picked, ["25", "30"] as Array[String], "each step reported once; none past the end")
	cycler.set_value("48", "48 fps (custom)")
	check_eq(cycler.get_value(), "", "a value that is not a choice")
	check_eq(cycler._value.text, "48 fps (custom)", "shows the words it was given")
	cycler._after.pressed.emit()
	check_eq(cycler.get_value(), "24", "and the next step starts from the first choice")
	cycler.wraps = true
	cycler.set_value("30")
	cycler._after.pressed.emit()
	check_eq(cycler.get_value(), "24", "a cycler that wraps goes round")
	cycler.set_choices([["25", "25 fps"], ["24", "24 fps"]])
	check_eq(cycler.get_value(), "24", "new choices keep the current one")


func test_a_pager_shows_a_page_at_a_time() -> void:
	var pager := Pager.new(10)
	mount(pager)
	var pages: Array[int] = []
	pager.page_changed.connect(func(first: int) -> void: pages.append(first))
	pager.set_count(34)
	check_eq(pager.first, 0, "starts at the first page")
	check(pager._before.disabled and not pager._after.disabled, "only the page after can be reached")
	check_eq(pager._place.text, "1–10 of 34", "says where it is")
	pager.step(1)
	pager.step(1)
	pager.step(1)
	check_eq(pager.first, 30, "the last page")
	check_eq(pager._place.text, "31–34 of 34", "which is short")
	check(pager._after.disabled, "and has nothing after it")
	pager.step(1)
	check_eq(pages, [10, 20, 30] as Array[int], "each page reported once")
	pager.show_item(12)
	check_eq(pager.first, 10, "showing an item goes to its page")
	pager.set_count(8)
	check_eq(pager.first, 0, "a shorter list moves the page back")
	check_eq(pager._place.text, "8", "and one page says only how many")
	check_eq(pager.pages(), 1, "one page")
	pager.set_per_page(4)
	check_eq(pager.pages(), 2, "fewer to a page, more pages")


func test_a_step_key_goes_round_its_settings() -> void:
	var taper := StepKey.new("taper", "Width taper", [["none", "Off"], ["pressure", "Press"], ["auto", "Auto"], ["both", "Both"]])
	mount(taper)
	var picked: Array[String] = []
	taper.chosen.connect(func(key: String) -> void: picked.append(key))
	taper.set_value("none")
	check(not taper.key_button.button_pressed, "off is up")
	check_eq(taper._word.text, "Off", "and says so under the key")
	taper.key_button.pressed.emit()
	check_eq(taper.get_value(), "pressure", "a press steps on")
	check(taper.key_button.button_pressed, "any other setting is down")
	check_eq(taper._word.text, "Press", "with its word")
	for i in 3:
		taper.key_button.pressed.emit()
	check_eq(taper.get_value(), "none", "and round to the start")
	check_eq(picked, ["pressure", "auto", "both", "none"] as Array[String], "every step reported")


func test_the_opacity_ramp_reports_where_it_is_dragged() -> void:
	var ramp := OpacityRamp.new()
	var host := mount(ramp)
	host.size = Vector2(300, 60)
	await frames(1)
	var events: Array[String] = []
	var values: Array[float] = []
	ramp.drag_started.connect(func() -> void: events.append("start"))
	ramp.changed.connect(func(value: float) -> void: values.append(value))
	ramp.drag_ended.connect(func() -> void: events.append("end"))
	var press := InputEventMouseButton.new()
	press.button_index = MOUSE_BUTTON_LEFT
	press.pressed = true
	press.position = Vector2(ramp.size.x * 0.25, 5)
	ramp._gui_input(press)
	var move := InputEventMouseMotion.new()
	move.position = Vector2(ramp.size.x * 2.0, 5)
	ramp._gui_input(move)
	var release := press.duplicate() as InputEventMouseButton
	release.pressed = false
	ramp._gui_input(release)
	check_eq(events, ["start", "end"] as Array[String], "one drag")
	check_near(values[0], 0.25, "a quarter of the way along")
	check_near(values[1], 1.0, "past the end is solid")
	ramp.set_value(0.5)
	check_near(ramp.get_value(), 0.5, "set from outside")


func test_a_panel_reports_what_does_not_fit() -> void:
	var panel := LimnerPanel.new(Vector2(200, 100))
	mount(panel)
	check_eq(panel.pixel_size(), Vector2(Kit.u(200), Kit.u(100)), "its size in pixels of the interface in use")
	check(panel.fits().is_empty(), "an empty panel fits: %s" % [panel.fits()])
	var tall := Control.new()
	tall.custom_minimum_size = Vector2(10, Kit.u(400))
	panel.add_child(tall)
	check_eq(panel.fits().size(), 1, "content taller than the panel is reported")
	tall.queue_free()
	await frames(1)
	panel.add_child(OptionButton.new())
	var problems := panel.fits()
	check(problems.size() == 1 and problems[0].contains("OptionButton"), "a drop-down is reported: %s" % [problems])
