class_name XRButtonMap
extends LimnerPanel
## The sheet that says what every control on the two controllers does, a page at a
## time: keys at its top choose the drawing hand, the other hand, the modifier, the
## gimbal or the panels. Nothing on it scrolls.
##
## What it lists is what this build does: the rows follow the controller handling in
## the core (`gesture.rs` and `grab.rs` under `crates/limner-core/src/session/`) and
## in the headset layer (`godot/xr/pointer.gd`, `godot/xr/xr_shell.gd`). Change
## those and change this.
## Buttons are named for the hand that holds them, so the sheet rewrites itself when
## the drawing hand changes.

## The sheet's size in design pixels (see `LimnerPanel`).
const DESIGN_SIZE := Vector2(660, 600)
## Width of the column that names the control, in pixels.
const CONTROL_WIDTH := 176
## The least width the column of explanations wraps to, in pixels.
const MEANING_WIDTH := 320

## What the drawing hand's controls do. `{lower}` and `{upper}` become the names of
## the face buttons on that hand. Where Limner does what Quill does, the words follow
## Quill's own button sheet.
const DRAWING_HAND := [
	["Trigger", "Use the tool. How far it is pulled is the pressure."],
	["Grip", "Take hold of the view, or of what is being transformed. While drawing a line: move where it starts."],
	["Both grips", "Scale as well, by moving the hands apart or together."],
	["Grip and stick up / down", "Zoom or scale quickly, about the hand."],
	["Stick up / down", "Tool size, larger / smaller."],
	["Stick left / right", "Undo / redo. Hold to keep going."],
	["Stick click", "Show or hide the gimbal: for the transform object, the current layer, or the selection."],
	["{lower} (lower button)", "Delete the selection, or else the stroke nearest the tip."],
	["{upper} (upper button)", "Pick the colour of the stroke nearest the tip."],
]
## What the other hand's controls do.
const OTHER_HAND := [
	["Trigger", "The modifier: see below."],
	["Grip", "Take hold, as with the drawing hand."],
	["Stick left / right", "Previous / next frame."],
	["Stick up / down", "New frame / hold this frame longer."],
	["Stick click", "Play or pause."],
	["Stick, painting mode", "Does nothing, nor does its click: no frame is made by accident."],
	["{lower} (lower button)", "Switch between the Selection tool and the tool used before it."],
	["{upper} (upper button)", "Show or hide the palette."],
	["Menu button", "Show or hide this sheet."],
]
## What the modifier (the other hand's trigger) changes.
const MODIFIER := [
	["Paint", "Draws a straight line, at any angle."],
	["Line", "Snaps the ends of the line to the brush grid."],
	["Selection", "A half pull adds to the selection, a full pull takes away."],
	["Thicken", "Thins."],
	["Stick up / down", "Drawing hand: the inner sphere, for tools that have one."],
	["Grip", "Moves a copy: of the selection, or of the layer being transformed."],
	["Redo after a copy", "Moves another copy as far again."],
	["Stick, other hand", "Left / right: rewind / fast forward. Up / down: copy the frame / delete it."],
]
## What the drawing hand does on the gimbal.
const GIMBAL := [
	["Grip or trigger: arrow / square", "Move along that axis / in that plane."],
	["Grip: ring", "Turn about that axis."],
	["Trigger: ring", "Turn in snapped steps (Rotation snap steps, in Transform)."],
	["Grip or trigger: centre", "Move freely."],
	["Modifier + trigger", "On an arrow, a square or the centre: move the pivot; the thing stays."],
]
## How the panels are worked. These belong to the drawing hand.
const PANELS := [
	["Tip near a panel", "Hover. Push a key or the sheet in to press; pull back to let go."],
	["Pointing from afar", "A ray shows where. The trigger is the button; the grip still takes hold of the artwork."],
	["Modifier held", "A press adds a key or a row to the selection."],
	["Stick up / down", "Step the page of a list under the pointer: the layers, the files."],
	["Grip, tip close", "Pick the panel up and leave it anywhere in the room. The stick resizes it while held."],
]
## Names of the lower and upper face buttons on a left and on a right Touch controller.
const FACE_BUTTONS := {"left": ["X", "Y"], "right": ["A", "B"]}

## The pages, by key: the words on the key that shows it.
const PAGES := {"drawing": "Drawing hand", "other": "Other hand", "modifier": "Modifier", "gimbal": "Gimbal", "panels": "Panels"}

var _preferences: Preferences
var _pages: KeyGrid
## The table of each page, by key: only the chosen one is shown.
var _tables := {}
var _shown := "drawing"


func _init() -> void:
	super(DESIGN_SIZE)
	name = "ButtonMap"
	_preferences = Preferences.shared()
	_pages = KeyGrid.new(PAGES.size())
	for key: String in PAGES:
		var page := _pages.add_text(key, PAGES[key], "Show what the controls do: %s" % String(PAGES[key]).to_lower())
		page.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_pages.set_value(_shown)
	_pages.chosen.connect(show_page)
	add_child(_pages)
	for key: String in PAGES:
		var table := GridContainer.new()
		table.columns = 2
		table.add_theme_constant_override("h_separation", 10)
		table.add_theme_constant_override("v_separation", 4)
		table.visible = key == _shown
		_tables[key] = table
		add_child(table)
	add_child(Form.note("Buttons are named as on Touch controllers. Other controllers have them in the same places where they have them at all."))


## Shows the page `key` (a key of `PAGES`).
func show_page(key: String) -> void:
	_shown = key
	_pages.set_value(key)
	for page: String in _tables:
		(_tables[page] as Control).visible = page == key


func _ready() -> void:
	_preferences.changed.connect(_on_preference_changed)
	refresh()


## Writes the sheet for the hand that draws now.
func refresh() -> void:
	for table: GridContainer in _tables.values():
		for child in table.get_children():
			table.remove_child(child)
			child.queue_free()
	var drawing := _preferences.text("dominant_hand")
	var other := "left" if drawing == "right" else "right"
	_section(_tables.drawing, "Drawing hand (%s)" % drawing, DRAWING_HAND, drawing)
	_section(_tables.other, "Other hand (%s)" % other, OTHER_HAND, other)
	_section(_tables.modifier, "With the modifier held", MODIFIER, other)
	_section(_tables.gimbal, "Gimbal, with the drawing hand", GIMBAL, drawing)
	_section(_tables.panels, "Panels, with the drawing hand", PANELS, drawing)


func _on_preference_changed(key: String) -> void:
	if key == "dominant_hand":
		refresh()


func _section(table: GridContainer, title: String, rows: Array, hand: String) -> void:
	var heading := Kit.heading(title)
	heading.custom_minimum_size = Vector2(0, Kit.ROW)
	heading.vertical_alignment = VERTICAL_ALIGNMENT_BOTTOM
	table.add_child(heading)
	table.add_child(Control.new())
	var faces: Array = FACE_BUTTONS[hand]
	for row: Array in rows:
		var control_name: String = row[0]
		var meaning: String = row[1]
		var control := Kit.label(control_name.format({"lower": faces[0], "upper": faces[1]}))
		control.custom_minimum_size = Vector2(CONTROL_WIDTH, 0)
		control.vertical_alignment = VERTICAL_ALIGNMENT_TOP
		control.size_flags_vertical = Control.SIZE_FILL
		table.add_child(control)
		var text := Kit.label(meaning, true)
		text.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		text.custom_minimum_size = Vector2(MEANING_WIDTH, 0)
		text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		text.vertical_alignment = VERTICAL_ALIGNMENT_TOP
		table.add_child(text)
