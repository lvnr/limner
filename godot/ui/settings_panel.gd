class_name SettingsPanel
extends VBoxContainer
## The Settings panel: the preferences that belong to the person rather than to a
## document, and two things to ask for by hand: the sheet of controls, and standing
## at the initial spawn area again.
##
## The preferences are the app's shared `Preferences`; a change here is saved at once
## and every part of the interface that listens follows it.

## The user asked to see the sheet that lists what every control does.
signal button_map_requested

## The longest time between automatic saves that the slider offers, in minutes.
const AUTOSAVE_SLIDER_MAX := 30.0

var editor: LimnerEditor
var preferences: Preferences

var _hand: ChoiceRow
var _haptics: CheckBox
var _volume: SliderRow
var _button_map_at_start: CheckBox
var _gimbal: CheckBox
var _theme: OptionButton
var _autosave: SliderRow


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	preferences = Preferences.shared()
	name = "Settings"

	add_child(Kit.heading("Controllers"))
	_hand = ChoiceRow.new()
	_hand.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_hand.add_text("left", "Left", "Draw with the left hand; the right hand holds the palette")
	_hand.add_text("right", "Right", "Draw with the right hand; the left hand holds the palette")
	_hand.chosen.connect(func(side: String) -> void: preferences.set_value("dominant_hand", side))
	add_child(Form.row("Drawing hand", [_hand], "The hand that paints. The other one holds the palette and the modifier."))
	_haptics = Kit.toggle("Controller vibration", "Whether the controllers give a small buzz on strokes and button presses", func(on: bool) -> void: preferences.set_value("haptics", on))
	add_child(_haptics)

	add_child(Kit.heading("Sound"))
	_volume = SliderRow.new("Volume", 0.0, 1.0, 0.01, "How loud the interface's clicks, ticks and painting hiss are. All the way down is silent.")
	_volume.changed.connect(func(value: float) -> void: preferences.set_value("ui_volume", value))
	add_child(_volume)

	add_child(Kit.heading("Interface"))
	_button_map_at_start = Kit.toggle("Button map at start", "Whether a headset session opens with the sheet that says what every control does", func(on: bool) -> void: preferences.set_value("show_button_map_at_start", on))
	add_child(_button_map_at_start)
	_gimbal = Kit.toggle("Gimbal shown by default", "Whether a session starts with the gimbal showing, instead of only when asked for", func(on: bool) -> void: preferences.set_value("show_gimbal_by_default", on))
	add_child(_gimbal)
	_theme = Kit.option(["Dark"], "The interface's colours. Only the dark scheme exists so far.", func(index: int) -> void: preferences.set_value("theme", Preferences.CHOICES["theme"][index]))
	add_child(Form.row("Colours", [_theme], "The interface's colours"))
	add_child(Form.note("Only the dark scheme exists so far."))

	add_child(Kit.heading("Saving"))
	_autosave = SliderRow.new("Autosave", 0.0, AUTOSAVE_SLIDER_MAX, 1.0, "How often the open project is saved by itself. All the way down switches it off.", _autosave_text)
	_autosave.changed.connect(func(value: float) -> void: preferences.set_value("autosave_minutes", roundi(value)))
	add_child(_autosave)

	add_child(Kit.heading("Help and view"))
	add_child(_wide(Kit.button("Show button map", "Shows or hides the sheet that says what every control does", func() -> void: button_map_requested.emit(), "info")))
	add_child(_wide(Kit.button("Recentre view on the initial spawn area", "Puts you back where the piece starts, facing the way it starts", _recentre, "home")))


func _ready() -> void:
	preferences.changed.connect(_on_preference_changed)
	refresh()


## Brings every control in line with the preferences.
func refresh() -> void:
	_hand.set_value(preferences.text("dominant_hand"))
	Kit.set_quiet(_haptics, preferences.flag("haptics"))
	_volume.set_value(preferences.number("ui_volume"))
	Kit.set_quiet(_button_map_at_start, preferences.flag("show_button_map_at_start"))
	Kit.set_quiet(_gimbal, preferences.flag("show_gimbal_by_default"))
	var themes: Array = Preferences.CHOICES["theme"]
	Kit.set_quiet(_theme, maxi(0, themes.find(preferences.text("theme"))))
	_autosave.set_value(preferences.number("autosave_minutes"))


func _on_preference_changed(_key: String) -> void:
	refresh()


func _recentre() -> void:
	editor.scene.view_go_to_default_viewpoint()
	editor.message.emit("Back at the initial spawn area", false)


func _autosave_text(minutes: float) -> String:
	return "off" if minutes < 0.5 else "%d min" % roundi(minutes)


## Lets a button's caption wrap, so that a long one does not make the panel wider.
func _wide(button: Button) -> Button:
	button.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	button.alignment = HORIZONTAL_ALIGNMENT_LEFT
	return button
