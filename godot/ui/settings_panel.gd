class_name SettingsPanel
extends LimnerPanel
## The Settings panel: the preferences that belong to the person rather than to a
## document, and three things to ask for by hand: the sheet of controls, standing at
## the initial spawn area again, and carrying on in the other interface (the headset
## from the desktop, the desktop from the headset).
##
## The preferences are the app's shared `Preferences`; a change here is saved at once
## and every part of the interface that listens follows it.

## The panel's size in design pixels (see `LimnerPanel`).
const DESIGN_SIZE := Vector2(470, 640)

## The user asked to see the sheet that lists what every control does.
signal button_map_requested
## The user asked to carry on in the other interface: "headset" or "desktop".
signal switch_requested(to: String)

## The longest time between automatic saves that the slider offers, in minutes.
const AUTOSAVE_SLIDER_MAX := 30.0

var editor: LimnerEditor
var preferences: Preferences

var _hand: ChoiceRow
var _haptics: CheckBox
var _hands: CheckBox
var _volume: SliderRow
var _button_map_at_start: CheckBox
var _gimbal: CheckBox
var _theme: Cycler
var _autosave: SliderRow
## "Continue in the headset" on the desktop, "Continue on the desktop" in a headset.
var _switch: Button
## Why the headset cannot be used, under the button, when it cannot.
var _switch_note: Label


func _init(limner_editor: LimnerEditor) -> void:
	super(DESIGN_SIZE)
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
	_hands = Kit.toggle("Bare hands as controllers", "Whether a hand the headset sees without a controller in it works as one, pinching for the trigger. Off, putting the controllers down leaves the hands doing nothing.", func(on: bool) -> void: preferences.set_value("hand_tracking", on))
	add_child(_hands)

	add_child(Kit.heading("Sound"))
	_volume = SliderRow.new("Volume", 0.0, 1.0, 0.01, "How loud the interface's clicks, ticks and painting hiss are. All the way down is silent.")
	_volume.changed.connect(func(value: float) -> void: preferences.set_value("ui_volume", value))
	add_child(_volume)

	add_child(Kit.heading("Interface"))
	_button_map_at_start = Kit.toggle("Button map at start", "Whether a headset session opens with the sheet that says what every control does", func(on: bool) -> void: preferences.set_value("show_button_map_at_start", on))
	add_child(_button_map_at_start)
	_gimbal = Kit.toggle("Gimbal shown by default", "Whether a session starts with the gimbal showing, instead of only when asked for", func(on: bool) -> void: preferences.set_value("show_gimbal_by_default", on))
	add_child(_gimbal)
	# The desktop and the headset each keep a scheme of their own; the row sets the
	# one of the interface in use.
	_theme = Kit.choice(["Dark", "Light"], "The interface's colours: dark panels with light text, or light panels with dark text. The desktop and the headset each keep their own. A change shows at once.", func(index: int) -> void: preferences.set_value(Kit.scheme_preference(), Preferences.CHOICES["theme"][index]))
	add_child(Form.row("Colours", [_theme], "The colours of this interface"))

	add_child(Kit.heading("Saving"))
	_autosave = SliderRow.new("Autosave", 0.0, AUTOSAVE_SLIDER_MAX, 1.0, "How often the open project is saved by itself. All the way down switches it off.", _autosave_text)
	_autosave.changed.connect(func(value: float) -> void: preferences.set_value("autosave_minutes", roundi(value)))
	add_child(_autosave)

	add_child(Kit.heading("Help and view"))
	add_child(_wide(Kit.button("Show button map", "Shows or hides the sheet that says what every control does", func() -> void: button_map_requested.emit(), "info")))
	add_child(_wide(Kit.button("Recentre view on the initial spawn area", "Puts you back where the piece starts, facing the way it starts", _recentre, "home")))

	add_child(Kit.heading("Desktop and headset"))
	_switch = _wide(Kit.button(ModeSwitch.TO_HEADSET, "", _on_switch_pressed, "headset"))
	add_child(_switch)
	_switch_note = Kit.label("", true)
	_switch_note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_switch_note.custom_minimum_size = Vector2(120, 0)
	add_child(_switch_note)


func _ready() -> void:
	preferences.changed.connect(_on_preference_changed)
	# The row's words follow the interface the panel is shown in, and whether a
	# headset is there to change to.
	Kit.events.scheme_changed.connect(_refresh_switch)
	ModeSwitch.events.changed.connect(_refresh_switch)
	refresh()


## Brings every control in line with the preferences.
func refresh() -> void:
	_hand.set_value(preferences.text("dominant_hand"))
	Kit.set_quiet(_haptics, preferences.flag("haptics"))
	Kit.set_quiet(_hands, preferences.flag("hand_tracking"))
	_volume.set_value(preferences.number("ui_volume"))
	Kit.set_quiet(_button_map_at_start, preferences.flag("show_button_map_at_start"))
	Kit.set_quiet(_gimbal, preferences.flag("show_gimbal_by_default"))
	var themes: Array = Preferences.CHOICES["theme"]
	Kit.set_quiet(_theme, maxi(0, themes.find(preferences.text(Kit.scheme_preference()))))
	_autosave.set_value(preferences.number("autosave_minutes"))
	_refresh_switch()


## The change of interface the row offers here: to the desktop in a headset, to the
## headset on the desktop.
func switch_target() -> String:
	return "desktop" if Kit.headset else "headset"


func _refresh_switch() -> void:
	var to_desktop := switch_target() == "desktop"
	var reason := "" if to_desktop else ModeSwitch.unavailable_reason()
	_switch.text = ModeSwitch.TO_DESKTOP if to_desktop else ModeSwitch.TO_HEADSET
	_switch.icon = Icons.texture("desktop" if to_desktop else "headset", Kit.TEXT, 18)
	_switch.tooltip_text = "Takes off the headset's interface and carries on with the same work on the desktop" if to_desktop else ModeSwitch.headset_tooltip()
	_switch.disabled = reason != ""
	_switch_note.text = reason
	_switch_note.visible = reason != ""


func _on_switch_pressed() -> void:
	switch_requested.emit(switch_target())


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
