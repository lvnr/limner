extends LimnerTest
## The light and dark colour schemes: Kit's colours follow the "theme" preference, and
## a change re-themes panels at once, desktop and headset, icons and drawn widgets too.

## A settings file of the tests' own, so that they neither read nor change the user's.
const SETTINGS := "user://test_color_scheme.json"
const LIGHT: Dictionary = Kit.SCHEMES["light"]
const DARK: Dictionary = Kit.SCHEMES["dark"]


func _exit_tree() -> void:
	Kit.use_scheme("dark")
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(Preferences.DEFAULT_PATH)


## Points the shared preferences at an empty file, so they are at their defaults, and
## has Kit follow them, in the dark scheme.
func _fresh() -> Preferences:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS))
	Preferences.use_file(SETTINGS)
	Kit.follow_preferences()
	return Preferences.shared()


## The colour an icon made by `Icons` was drawn in.
func _icon_color(icon: Texture2D) -> Color:
	var made := Icons.describe(icon)
	return made[1] if not made.is_empty() else Color(0, 0, 0, 0)


## A control that only records what it drew with and how often.
class Swatch extends Control:
	var draws := 0
	var drawn_with := Color()

	func _draw() -> void:
		draws += 1
		drawn_with = Kit.BG_SUNKEN
		draw_rect(Rect2(Vector2.ZERO, size), drawn_with)


func test_every_scheme_names_every_colour_and_roles_are_told_apart() -> void:
	for scheme: String in Kit.SCHEMES:
		var colors: Dictionary = Kit.SCHEMES[scheme]
		check_eq(colors.keys().size(), DARK.keys().size(), "%s gives every role a colour" % scheme)
		for role: String in DARK:
			check(colors.has(role), "%s has %s" % [scheme, role])
	# A colour handed out earlier is recognised by its value, so no two recognised
	# roles share one in any scheme.
	var seen := {}
	for scheme: String in Kit.SCHEMES:
		for role: String in Kit.RECOGNISED:
			var color: Color = Kit.SCHEMES[scheme][role]
			var key := Color(color, 1.0).to_html(false)
			if seen.has(key):
				check_eq(seen[key], role, "%s in %s is told apart from the others" % [role, scheme])
			seen[key] = role
	check_eq(Preferences.CHOICES["theme"], ["dark", "light"], "both schemes can be chosen")
	check_eq(Preferences.DEFAULTS["theme"], "dark", "dark is the default")


func test_kit_takes_its_colours_from_the_preference() -> void:
	var preferences := _fresh()
	check_eq(Kit.scheme, "dark", "dark at the start")
	check_eq(Kit.TEXT, DARK.TEXT, "dark text colour")
	var heard := [0]
	var listen := func() -> void: heard[0] += 1
	Kit.events.scheme_changed.connect(listen)
	preferences.set_value("theme", "light")
	check_eq(Kit.scheme, "light", "the preference switches Kit")
	check_eq(heard[0], 1, "and the change is reported once")
	check_eq(Kit.TEXT, LIGHT.TEXT, "text is the light scheme's")
	check_eq(Kit.BG, LIGHT.BG, "so are panels")
	check_eq(Kit.ACCENT, LIGHT.ACCENT, "and the accent")
	check(Kit.TEXT.get_luminance() < 0.2 and Kit.BG.get_luminance() > 0.8, "dark text on light panels")
	var theme := Kit.make_theme()
	check_eq(theme.get_color("font_color", "Label"), LIGHT.TEXT, "a theme made now is light")
	check_eq((theme.get_stylebox("panel", "PanelContainer") as StyleBoxFlat).bg_color, LIGHT.BG, "with light panels")
	check_eq(Kit.recolor(DARK.TEXT_DIM), LIGHT.TEXT_DIM, "a dark colour is recognised as its role")
	check_eq(Kit.recolor(Color(DARK.ACCENT, 0.3)), Color(LIGHT.ACCENT, 0.3), "keeping its own alpha")
	check_eq(Kit.recolor(Color(0.5, 0.1, 0.9)), Color(0.5, 0.1, 0.9), "any other colour is left alone")
	preferences.set_value("theme", "dark")
	check_eq(Kit.scheme, "dark", "and back")
	check_eq(Kit.TEXT, DARK.TEXT, "with the dark colours again")
	check_eq(heard[0], 2, "reported again")
	preferences.set_value("theme", "purple")
	check_eq(Kit.scheme, "dark", "a scheme that does not exist is not taken")
	Kit.events.scheme_changed.disconnect(listen)


func test_switching_retheme_a_mounted_panel_at_once() -> void:
	var preferences := _fresh()
	var settings := SettingsPanel.new(editor)
	mount(settings, Vector2(280, 900))
	var host: Control = settings.get_parent()
	var layers := TimelinePanel.new(editor)
	host.remove_child(settings)
	var column := Kit.column([settings, layers])
	host.add_child(column)
	var swatch := Swatch.new()
	swatch.custom_minimum_size = Vector2(40, 10)
	column.add_child(swatch)
	# The shells re-apply the theme when the scheme changes; so does this host.
	var reapply := func() -> void: Kit.apply_theme(host)
	Kit.events.scheme_changed.connect(reapply)
	await frames(3)
	check_eq(settings._theme.item_count, 2, "dark and light are offered")
	check_eq(settings._theme.get_item_text(1), "Light", "the second is Light")
	check_eq(settings._theme.selected, 0, "dark is chosen")
	var heading: Label = settings.get_child(0)
	var button: Button = null
	for node: Node in settings.find_children("*", "Button", true, false):
		if (node as Button).text == "Show button map":
			button = node
	check(button != null, "the panel has a button with an icon")
	if button == null:
		Kit.events.scheme_changed.disconnect(reapply)
		return
	var dark_theme := host.theme
	check_eq(heading.get_theme_color("font_color"), DARK.TEXT_DIM, "a heading is dim text")
	check_eq(_icon_color(button.icon), DARK.TEXT, "an icon in the dark scheme's text colour")
	var draws_before := swatch.draws
	check_eq(swatch.drawn_with, DARK.BG_SUNKEN, "the drawn widget drew in dark")

	# Choosing Light in the Settings panel.
	settings._theme.select(1)
	settings._theme.item_selected.emit(1)
	await frames(2)
	check_eq(preferences.text("theme"), "light", "the panel sets the preference")
	check_eq(Kit.scheme, "light", "which Kit follows")
	check(host.theme != dark_theme, "the host has a new theme")
	check_eq(settings.get_theme_color("font_color", "Label"), LIGHT.TEXT, "labels read light")
	var name_key: Button = layers.layer_rows._lines[0].name
	check_eq((name_key.get_theme_stylebox("normal") as StyleBoxFlat).bg_color, LIGHT.BG_RAISED, "a layer row's key is light")
	check_eq(heading.get_theme_color("font_color"), LIGHT.TEXT_DIM, "a dim label set from Kit follows")
	check_eq(_icon_color(button.icon), LIGHT.TEXT, "the icon is made again in the light text colour")
	check_eq(name_key.get_theme_color("font_color"), LIGHT.TEXT, "its words are light-scheme text")
	check_eq(name_key.get_theme_color("icon_normal_color"), LIGHT.TEXT, "and its icon is tinted the same")
	check(swatch.draws > draws_before, "the drawn widget drew again")
	check_eq(swatch.drawn_with, LIGHT.BG_SUNKEN, "in the light colours")
	check_eq(settings._theme.selected, 1, "the panel shows Light")

	# And back.
	draws_before = swatch.draws
	preferences.set_value("theme", "dark")
	await frames(2)
	check_eq(Kit.scheme, "dark", "back to dark")
	check_eq(settings.get_theme_color("font_color", "Label"), DARK.TEXT, "labels read dark again")
	check_eq(heading.get_theme_color("font_color"), DARK.TEXT_DIM, "dim labels too")
	check_eq(_icon_color(button.icon), DARK.TEXT, "icons too")
	check(swatch.draws > draws_before, "the drawn widget drew again")
	check_eq(swatch.drawn_with, DARK.BG_SUNKEN, "in the dark colours")
	check_eq(settings._theme.selected, 0, "the panel shows Dark")
	Kit.events.scheme_changed.disconnect(reapply)
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_timeline_draws_again_in_the_new_colours() -> void:
	var preferences := _fresh()
	var timeline := TimelinePanel.new(editor)
	mount(timeline, Vector2(900, 320))
	var host: Control = timeline.get_parent()
	var reapply := func() -> void: Kit.apply_theme(host)
	Kit.events.scheme_changed.connect(reapply)
	await frames(3)
	var draws := [0]
	timeline.view.draw.connect(func() -> void: draws[0] += 1)
	var play: Button = timeline.controls.play
	check_eq(play.get_theme_color("icon_normal_color"), DARK.TEXT, "the play key's icon is tinted dark-scheme text")
	preferences.set_value("theme", "light")
	await frames(2)
	check(draws[0] > 0, "the tracks are drawn again")
	check_eq(play.get_theme_color("icon_normal_color"), LIGHT.TEXT, "the play key's icon takes the light text colour")
	preferences.set_value("theme", "dark")
	await frames(2)
	check_eq(play.get_theme_color("icon_normal_color"), DARK.TEXT, "and again on the way back")
	Kit.events.scheme_changed.disconnect(reapply)
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_headset_panels_follow_the_scheme() -> void:
	var preferences := _fresh()
	preferences.set_value("show_button_map_at_start", false)
	var shell := XRShell.new()
	shell.mode = "sim"
	shell.world = world
	shell.editor = editor
	add_child(shell)
	if not shell.started:
		failures.append("the headset shell did not start on the simulated rig: %s" % shell.failure)
		return
	(shell.rig as SimRig).listen_to_devices = false
	await frames(4)
	check_eq(Kit.headset, true, "the panels take the headset's look")
	check_eq(Kit.scheme_preference(), "headset_theme", "and follow the headset's own scheme")
	var surface: XRPanelSurface = shell.palette.surfaces["Tools"]
	var root: PanelContainer = surface._root
	var sheet := root.get_theme_stylebox("panel") as StyleBoxFlat
	check_eq(Color(sheet.bg_color, 1.0), Color(LIGHT.BG, 1.0), "a sheet is light, as Quill's are, whatever the desktop uses")
	preferences.set_value("theme", "dark")
	await frames(2)
	sheet = root.get_theme_stylebox("panel") as StyleBoxFlat
	check_eq(Color(sheet.bg_color, 1.0), Color(LIGHT.BG, 1.0), "the desktop's scheme does not reach the headset")
	preferences.set_value("headset_theme", "dark")
	await frames(2)
	sheet = root.get_theme_stylebox("panel") as StyleBoxFlat
	check_eq(Color(sheet.bg_color, 1.0), Color(DARK.BG, 1.0), "the sheet turns dark")
	check_eq(surface.content.get_theme_color("font_color", "Label"), DARK.TEXT, "its labels read dark")
	var coin: Button = shell.palette.coins["Tools"]
	check_eq(_icon_color(coin.icon), DARK.TEXT, "the palette's round buttons are made again")
	check_eq(shell._mirror_hint.get_theme_color("font_color"), DARK.TEXT, "the line on the desktop window follows")
	preferences.set_value("headset_theme", "light")
	await frames(2)
	sheet = root.get_theme_stylebox("panel") as StyleBoxFlat
	check_eq(Color(sheet.bg_color, 1.0), Color(LIGHT.BG, 1.0), "and back")
	shell.queue_free()
	Kit.set_headset(false)
	check(messages.is_empty(), "no errors: %s" % [messages])
