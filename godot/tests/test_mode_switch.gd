extends LimnerTest
## Changing between the desktop and the headset while the app runs (`main.gd`,
## `switch_to`): the work comes along, nothing of the interface that was left stays
## behind, and the buttons only offer a headset when there is one. The headset here is
## the simulated one, which is what a Mac switches to; the tests ask for it on other
## systems too (`ModeSwitch.sim_without_openxr`).

## A settings file of the tests' own, so that they neither read nor change the user's.
const SETTINGS := "user://test_mode_switch_settings.json"

var app: LimnerApp


func _exit_tree() -> void:
	ModeSwitch.headset_ended = false
	ModeSwitch.sim_without_openxr = false
	Autosave.clear()
	# The desktop shell asks to be told before the app quits; the test runner must not be.
	get_tree().auto_accept_quit = true
	Kit.set_headset(false)
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(Preferences.DEFAULT_PATH)


## Starts the app as `main.gd` does, on the desktop, with default preferences and no
## work left over from an earlier run.
func _start_app() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(SETTINGS)
	Autosave.clear()
	ModeSwitch.headset_ended = false
	# The simulated headset, as a Mac offers it, on every system.
	ModeSwitch.sim_without_openxr = true
	app = LimnerApp.new()
	add_child(app)
	app.editor.message.connect(func(text: String, is_error: bool) -> void:
		if is_error:
			messages.append(text)
	)
	await frames(3)


## The state of the work that a change of interface must leave as it is.
func _work() -> Dictionary:
	var status := app.world.scene.status()
	return {
		"strokes": int(app.world.scene.info().strokes),
		"dirty": status.dirty,
		"can_undo": status.can_undo,
		"undo_label": status.undo_label,
		"selection": status.selection_count,
		"layer": app.editor.current_layer(),
		"path": app.editor.path,
		"gimbal": app.editor.get_options().get("show_gimbal", false),
	}


## How many connections the things that outlive an interface hold, by name.
func _connections() -> Dictionary:
	var editor_now := app.editor
	var scene := app.world.scene
	return {
		"editor.state_changed": editor_now.state_changed.get_connections().size(),
		"editor.document_changed": editor_now.document_changed.get_connections().size(),
		"editor.document_replaced": editor_now.document_replaced.get_connections().size(),
		"editor.message": editor_now.message.get_connections().size(),
		"scene.playback_event": scene.playback_event.get_connections().size(),
		"scene.state_changed": scene.state_changed.get_connections().size(),
		"Kit.scheme_changed": Kit.events.scheme_changed.get_connections().size(),
		"ModeSwitch.changed": ModeSwitch.events.changed.get_connections().size(),
		"Preferences.changed": Preferences.shared().changed.get_connections().size(),
		"tree.node_added": get_tree().node_added.get_connections().size(),
	}


func _world_children() -> Array[String]:
	var names: Array[String] = []
	for child: Node in app.world.get_children():
		names.append(String(child.name))
	return names


func _settings_in_headset() -> SettingsPanel:
	var surface := app.xr_shell.palette.surfaces.get("Settings") as XRPanelSurface
	check(surface != null, "the palette has a Settings panel")
	return surface.content as SettingsPanel if surface != null else null


func _settings_on_desktop() -> SettingsPanel:
	for node: Node in app.shell.details.find_children("Settings", "", true, false):
		if node is SettingsPanel:
			return node
	failures.append("the desktop has no Settings panel")
	return null


## Runs a command on the app's document and records a failure if it does not work.
func _run(command: Dictionary) -> void:
	var reply: Dictionary = app.world.scene.exec(command)
	if not reply.ok:
		failures.append("%s failed: %s" % [command, reply.error])


func test_desktop_to_headset_and_back_keeps_the_work_and_leaves_nothing_behind() -> void:
	await _start_app()
	check_eq(app.mode, "desktop", "the app starts on the desktop here")
	check(not Kit.headset, "with the desktop's look")
	var desktop_settings := _settings_on_desktop()
	if desktop_settings != null:
		check_eq(desktop_settings._switch.text, ModeSwitch.TO_HEADSET, "the desktop's Settings offer the headset")
		check(not desktop_settings._switch.disabled, "and can be used on this machine (the simulated headset)")

	# Some work: a layer, a stroke on it, the stroke selected.
	_run({"cmd": "add_layer", "kind": "paint"})
	LimnerDemo.stroke(app.world.scene, [Vector3(0, 1.2, -1), Vector3(0.1, 1.25, -1), Vector3(0.2, 1.2, -1)])
	_run({"cmd": "select_all"})
	await frames(2)
	var before := _work()
	check(before.strokes > 0 and before.selection > 0 and before.can_undo, "there is work to carry over: %s" % [before])
	# The recovery copy is written during ordinary work. Changing interface must not
	# take it for work left over from a run that ended badly.
	check(app.autosave.save_now(), "a recovery copy is written")
	# A start-up preference that a change of interface must not apply again.
	Preferences.shared().set_value("show_gimbal_by_default", not bool(before.gimbal))
	var connections := _connections()
	var children := _world_children()
	var replaced := [0]
	app.editor.document_replaced.connect(func() -> void: replaced[0] += 1)
	connections["editor.document_replaced"] += 1

	app.world.scene.exec({"cmd": "set_tool", "tool": "paint"})
	app.switch_to("headset")
	await frames(4)
	check_eq(app.mode, "headset", "the app is in the headset")
	check(app.shell == null and app.xr_shell != null and app.xr_shell.started, "with the headset shell alone")
	if app.xr_shell == null or not app.xr_shell.started:
		return
	check(app.xr_shell.rig is SimRig, "on the simulated rig, as a Mac has no headset")
	check(Kit.headset, "the panels take the headset's look")
	var work_in_headset := _work()
	work_in_headset.erase("undo_label")
	var expected := before.duplicate()
	expected.erase("undo_label")
	check_eq(work_in_headset, expected, "the work is as it was")
	check_eq(app.world.scene.status().undo_label, before.undo_label, "and so is the undo history")
	check_eq(replaced[0], 0, "the recovery copy was not opened over the work")
	check(not app.xr_shell.button_map.visible, "the button map does not show as it does at start")
	check(not app.world.has_node("Head") and not app.world.has_node("ToolCursor"), "the desktop's camera and cursor are gone from the world")
	check_eq(app.get_viewport().get_camera_3d(), app.xr_shell.rig.head, "the view is the headset's")
	check(app.get_tree().auto_accept_quit, "closing the mirror window quits, as in a headset started alone")
	var headset_settings := _settings_in_headset()
	if headset_settings == null:
		return
	check_eq(headset_settings._switch.text, ModeSwitch.TO_DESKTOP, "the headset's Settings offer the desktop")
	check(not headset_settings._switch.disabled, "always")

	# Back, through the Settings panel's button, as the user would.
	headset_settings._switch.pressed.emit()
	await frames(4)
	check_eq(app.mode, "desktop", "the app is on the desktop again")
	check(app.xr_shell == null and app.shell != null, "with the desktop shell alone")
	if app.shell == null:
		return
	check(not Kit.headset, "the panels take the desktop's look again")
	var after := _work()
	check_eq(after, before, "the work is still as it was")
	check_eq(replaced[0], 0, "the recovery copy was not offered or opened")
	check_eq(_world_children(), children, "the world holds what it held before, and no more")
	check_eq(_connections(), connections, "nothing that was taken down is still connected")
	check_eq(app.get_viewport().get_camera_3d(), app.shell.camera, "the view is the desktop camera's")
	check(not app.get_tree().auto_accept_quit, "the desktop asks before quitting again")
	check(not Autosave.pending().is_empty(), "the recovery copy is still kept for a bad ending")
	check(app.holding_view == null, "no holding view without a real headset")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_without_a_headset_the_buttons_say_why_and_do_nothing() -> void:
	await _start_app()
	check(not app.shell._headset_button.disabled, "the top bar offers the (simulated) headset")
	ModeSwitch.mark_headset_ended()
	await frames(1)
	check(app.shell._headset_button.disabled, "the top bar's button is disabled once there is none")
	check_eq(app.shell._headset_button.tooltip_text, ModeSwitch.HEADSET_ENDED, "and says why")
	var menu := app.shell._view_menu
	check(menu.is_item_disabled(menu.get_item_index(DesktopShell.ViewMenu.HEADSET)), "so is the View menu's entry")
	var settings := _settings_on_desktop()
	if settings != null:
		check(settings._switch.disabled, "and the Settings panel's row")
		check(settings._switch_note.visible and settings._switch_note.text == ModeSwitch.HEADSET_ENDED, "which says why under it")
	app.switch_to("headset")
	await frames(2)
	check_eq(app.mode, "desktop", "asking anyway changes nothing")
	check(app.shell != null and app.xr_shell == null, "the desktop stays as it is")
	check_eq(messages, [ModeSwitch.HEADSET_ENDED] as Array[String], "the user is told why")
	messages.clear()
	ModeSwitch.headset_ended = false


func test_the_headset_is_offered_where_there_is_one_or_a_simulated_one() -> void:
	var none := PackedStringArray()
	check_eq(ModeSwitch.target_for("Windows", none, true, false), "headset", "OpenXR running: the headset")
	check_eq(ModeSwitch.target_for("Windows", none, false, false), "", "OpenXR off (--xr-mode off, or no runtime): nothing")
	check_eq(ModeSwitch.target_for("Windows", none, true, true), "", "the headset's software ended the session: nothing")
	check_eq(ModeSwitch.target_for("macOS", none, false, false), "sim", "a Mac: the simulated headset")
	check_eq(ModeSwitch.target_for("Windows", PackedStringArray(["--xr", "--sim"]), false, false), "sim", "--sim: the simulated headset")
	check_eq(ModeSwitch.NO_HEADSET, "Start Limner without --xr-mode off, with the headset's software running, to use a headset.", "the reason given without OpenXR")
