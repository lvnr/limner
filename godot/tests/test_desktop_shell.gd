extends LimnerTest
## What the desktop shell does itself, beyond the panels it docks.

## A settings file of the tests' own, so that they neither read nor change the user's.
const SETTINGS := "user://test_desktop_shell_settings.json"

var shell: DesktopShell


func _exit_tree() -> void:
	# The shell asks to be told before the app quits; the test runner must not be.
	get_tree().auto_accept_quit = true
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(Preferences.DEFAULT_PATH)


func _shell() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(SETTINGS)
	Autosave.clear()
	shell = DesktopShell.new()
	shell.world = world
	shell.editor = editor
	add_child(shell)
	await frames(2)


func test_wheel_steps_in_one_frame_each_count() -> void:
	await _shell()
	run({"cmd": "set_tool_settings", "values": {"paint": {"size": 0.01}}})
	run({"cmd": "set_tool", "tool": "paint"})
	await frames(2)
	# Three notches of Shift and the wheel arriving together, before any signal.
	for i in 3:
		shell._scale_tool_size(2.0)
	check_near(float(world.scene.tool_settings().paint.size), 0.08, "each step starts from the size the last one left")
	shell._scale_tool_size(0.5)
	check_near(float(world.scene.tool_settings().paint.size), 0.04, "and smaller again")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_two_saves_in_one_frame_leave_the_panels_shown() -> void:
	await _shell()
	check(shell.ui.visible, "the panels are shown")
	# Two pictures for two saves, asked for before either is taken.
	shell._thumbnail()
	shell._thumbnail()
	check(not shell.ui.visible, "the panels are hidden for the pictures")
	RenderingServer.frame_post_draw.emit()
	await frames(1)
	check(shell.ui.visible, "and shown again once both are taken")

	# Hidden by the user to begin with, they stay hidden.
	shell.ui.visible = false
	shell._thumbnail()
	shell._thumbnail()
	RenderingServer.frame_post_draw.emit()
	await frames(1)
	check(not shell.ui.visible, "panels the user hid stay hidden")
	shell.ui.visible = true
