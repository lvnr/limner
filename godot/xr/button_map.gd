class_name XRButtonMap
extends VBoxContainer
## The sheet that says what every control on the two controllers does.
##
## What it lists is what this build does: the rows follow the controller handling in
## the core (`gesture.rs` and `grab.rs` under `crates/limner-core/src/session/`) and
## in the headset layer (`godot/xr/pointer.gd`, `godot/xr/xr_shell.gd`). Change
## those and change this.
## Buttons are named for the hand that holds them, so the sheet rewrites itself when
## the drawing hand changes.

## Width of the column that names the control, in pixels.
const CONTROL_WIDTH := 176
## The least width the column of explanations wraps to, in pixels.
const MEANING_WIDTH := 320

## What the drawing hand's controls do. `{lower}` and `{upper}` become the names of
## the face buttons on that hand.
const DRAWING_HAND := [
	["Trigger", "Use the tool. How far it is pulled is the pressure. On a handle of the gimbal it moves the handle."],
	["Grip", "Take hold of the view, or of the layer or selection being transformed. With the modifier held as it takes hold: move a copy."],
	["Both grips", "Scale as well, by moving the hands apart or together."],
	["Grip and stick up / down", "Zoom or scale quickly, about the hand."],
	["Stick up / down", "Tool size. With the modifier: the inner sphere, for tools that have one."],
	["Stick left / right", "Undo / redo. Hold to keep going. After moving a copy, redo makes the next copy."],
	["Stick click", "Show or hide the gimbal."],
	["{lower} (lower button)", "Delete the selection, or else the stroke nearest the tip."],
	["{upper} (upper button)", "Take the paint colour from the stroke nearest the tip."],
]
## What the other hand's controls do.
const OTHER_HAND := [
	["Trigger", "The modifier. Paint draws a straight line; Thicken thins; Selection adds on a half pull and removes on a full one."],
	["Grip", "Take hold, as with the drawing hand."],
	["Stick left / right", "Previous / next frame. With the modifier: rewind / fast forward."],
	["Stick up / down", "New frame / hold this frame longer. With the modifier: copy the frame / delete it."],
	["Stick click", "Play or pause."],
	["{lower} (lower button)", "Switch between the Selection tool and the tool used before it."],
	["{upper} (upper button)", "Show or hide the palette."],
	["Menu button, held", "Show or hide this sheet."],
]
## How the panels are worked. These belong to the drawing hand.
const PANELS := [
	["Tip near a panel", "Hover. Push through the surface to press; pull back to let go."],
	["Pointing from afar", "A ray shows where. The trigger is the button."],
	["Stick up / down", "Scroll what is under the pointer."],
	["{lower} (lower button)", "Open the menu of what is under the pointer, as a right click does: a layer's, in the layer list."],
	["Grip over a panel", "Pick the panel up and leave it anywhere in the room. The stick resizes it while held; the button in its corner sends it back."],
]
## Names of the lower and upper face buttons on a left and on a right Touch controller.
const FACE_BUTTONS := {"left": ["X", "Y"], "right": ["A", "B"]}

var _preferences: Preferences
var _table: GridContainer


func _init() -> void:
	name = "ButtonMap"
	_preferences = Preferences.shared()
	_table = GridContainer.new()
	_table.columns = 2
	_table.add_theme_constant_override("h_separation", 10)
	_table.add_theme_constant_override("v_separation", 4)
	add_child(_table)
	add_child(Form.note("Buttons are named as on Touch controllers. Other controllers have them in the same places where they have them at all."))


func _ready() -> void:
	_preferences.changed.connect(_on_preference_changed)
	refresh()


## Writes the sheet for the hand that draws now.
func refresh() -> void:
	for child in _table.get_children():
		_table.remove_child(child)
		child.queue_free()
	var drawing := _preferences.text("dominant_hand")
	var other := "left" if drawing == "right" else "right"
	_section("Drawing hand (%s)" % drawing, DRAWING_HAND, drawing)
	_section("Other hand (%s)" % other, OTHER_HAND, other)
	_section("Panels, with the drawing hand", PANELS, drawing)


func _on_preference_changed(key: String) -> void:
	if key == "dominant_hand":
		refresh()


func _section(title: String, rows: Array, hand: String) -> void:
	var heading := Kit.heading(title)
	heading.custom_minimum_size = Vector2(0, Kit.ROW)
	heading.vertical_alignment = VERTICAL_ALIGNMENT_BOTTOM
	_table.add_child(heading)
	_table.add_child(Control.new())
	var faces: Array = FACE_BUTTONS[hand]
	for row: Array in rows:
		var control_name: String = row[0]
		var meaning: String = row[1]
		var control := Kit.label(control_name.format({"lower": faces[0], "upper": faces[1]}))
		control.custom_minimum_size = Vector2(CONTROL_WIDTH, 0)
		control.vertical_alignment = VERTICAL_ALIGNMENT_TOP
		control.size_flags_vertical = Control.SIZE_FILL
		_table.add_child(control)
		var text := Kit.label(meaning, true)
		text.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		text.custom_minimum_size = Vector2(MEANING_WIDTH, 0)
		text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		text.vertical_alignment = VERTICAL_ALIGNMENT_TOP
		_table.add_child(text)
