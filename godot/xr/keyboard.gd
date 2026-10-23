class_name XRKeyboard
extends VBoxContainer
## A keyboard to poke at in a headset: letters, digits, a few symbols, space,
## backspace, the two arrows that move the caret, and Enter.
##
## It types by sending key presses to `target`, the viewport that holds the text box
## being typed into, exactly as a real keyboard would. The text box cannot tell the
## difference, so whatever it does on Enter or on a change it does here too.

## The user pressed the key that puts the keyboard away.
signal done

## Height of a key, in pixels. A little more than a row of a panel: keys are hit in
## quick succession and a miss is a typo.
const KEY_HEIGHT := 36
## The rows of keys that type what is written on them.
const ROWS := ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm.,"]
## The symbols on the bottom row, beside the space bar.
const SYMBOLS := ["#", "-", "_", "/", "'"]
## How many characters of a long line show before the caret and after it.
const SHOWN_BEFORE := 30
const SHOWN_AFTER := 10

## The viewport whose text box gets the keys.
var target: Viewport

## What has been typed, over the keys, so that the eyes can stay on the keyboard.
var _typed: Label
var _shift: Button
var _letters: Array[Button] = []


func _init() -> void:
	name = "Keyboard"
	add_theme_constant_override("separation", 3)
	_typed = Kit.label("")
	_typed.add_theme_stylebox_override("normal", Kit._box(Kit.BG_SUNKEN, 4, 1, Kit.BORDER, 8))
	_typed.custom_minimum_size = Vector2(0, KEY_HEIGHT)
	_typed.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	_typed.clip_text = true
	add_child(_typed)
	for index in ROWS.size():
		var row := _row()
		var keys: String = ROWS[index]
		if index == 3:
			_shift = _key("", "Capital letter next", Callable(), "shift")
			_shift.toggle_mode = true
			_shift.toggled.connect(func(_on: bool) -> void: _show_case())
			row.add_child(_shift)
		for character in keys:
			var key := _key(character, "", _type.bind(character))
			if character.to_upper() != character:
				_letters.append(key)
			row.add_child(key)
		if index == 2:
			row.add_child(_key("", "Delete the character before the caret", _press.bind(KEY_BACKSPACE), "backspace"))
		add_child(row)
	var bottom := _row()
	for symbol: String in SYMBOLS:
		bottom.add_child(_key(symbol, "", _type.bind(symbol)))
	var space := _key("space", "", _type.bind(" "))
	space.size_flags_stretch_ratio = 2.6
	bottom.add_child(space)
	bottom.add_child(_key("", "Move the caret left", _press.bind(KEY_LEFT), "left"))
	bottom.add_child(_key("", "Move the caret right", _press.bind(KEY_RIGHT), "right"))
	bottom.add_child(_key("", "Enter", _press.bind(KEY_ENTER), "enter"))
	bottom.add_child(_key("", "Put the keyboard away", func() -> void: done.emit(), "check"))
	add_child(bottom)


## Shows the text of `box` over the keys, with a bar where the caret is: of a text
## area, the line the caret is on; of a long line, the part about the caret.
func show_text(box: Control) -> void:
	var row := ""
	var caret := 0
	var line := box as LineEdit
	var area := box as TextEdit
	if line != null:
		row = line.text
		caret = line.caret_column
	elif area != null:
		row = area.get_line(area.get_caret_line())
		caret = area.get_caret_column()
	var before := row.substr(0, caret)
	var after := row.substr(caret)
	if before.length() > SHOWN_BEFORE:
		before = "…" + before.right(SHOWN_BEFORE)
	if after.length() > SHOWN_AFTER:
		after = after.left(SHOWN_AFTER) + "…"
	var text := before + "|" + after
	if _typed.text != text:
		_typed.text = text


func _row() -> HBoxContainer:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 3)
	return row


func _key(text: String, tooltip: String, on_pressed: Callable, icon := "") -> Button:
	var key := Kit.button(text, tooltip, on_pressed, icon)
	key.custom_minimum_size = Vector2(0, KEY_HEIGHT)
	key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	key.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	key.clip_text = true
	Kit.narrow(key, 2)
	return key


## Types a character, as a capital when Shift is down, which lets Shift up again.
func _type(character: String) -> void:
	var typed := character.to_upper() if _shift.button_pressed else character
	# A key's code is the code of the capital letter, or of the symbol itself.
	_send(character.to_upper().unicode_at(0) as Key, typed.unicode_at(0), _shift.button_pressed)
	if _shift.button_pressed:
		_shift.button_pressed = false


## Presses a key that types nothing: backspace, an arrow, Enter.
func _press(key: Key) -> void:
	_send(key, 0, false)


## Sends a key going down and coming up to the text box.
func _send(key: Key, unicode: int, shifted: bool) -> void:
	if target == null or not is_instance_valid(target):
		return
	for pressed: bool in [true, false]:
		var event := InputEventKey.new()
		event.keycode = key
		event.physical_keycode = key
		event.key_label = key
		event.unicode = unicode if pressed else 0
		event.shift_pressed = shifted
		event.pressed = pressed
		target.push_input(event)


## Shows the letters as they will be typed.
func _show_case() -> void:
	for key in _letters:
		key.text = key.text.to_upper() if _shift.button_pressed else key.text.to_lower()
