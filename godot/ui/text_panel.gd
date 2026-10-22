class_name TextPanel
extends LimnerPanel
## Words written in strokes: what to write, how the lines line up, the font, and the
## size. Opened from the Text key of the Tools panel.
##
## Nothing in it scrolls: the text box holds `MAX_LINES` lines of up to `MAX_LINE`
## characters, which always show whole, and the fonts (often hundreds) show eight to
## a page, with page keys and a box to find one by name.

## How the lines of a text line up, by key: the word on its key, its tooltip and the
## alignment it stands for.
const TEXT_ALIGNMENTS := {
	"left": ["Left", "Line the rows of text up on the left", HORIZONTAL_ALIGNMENT_LEFT],
	"centre": ["Centre", "Centre each row of text", HORIZONTAL_ALIGNMENT_CENTER],
	"right": ["Right", "Line the rows of text up on the right", HORIZONTAL_ALIGNMENT_RIGHT],
}
## What the font keys call the font that comes with Limner.
const BUILT_IN_FONT := "Built-in font"
const FONT_NOTE := "The font the text is written in: the built-in one or one installed on this computer. The ones chosen last come first."
const MAX_LINES := 4
const MAX_LINE := 48
const FONTS_PER_PAGE := 8
const DESIGN_SIZE := Vector2(470, 640)

var editor: LimnerEditor

var _text: TextEdit
var _text_align: KeyGrid
var _font_search: LineEdit
## The keys of the fonts on the page shown. Each key's metadata is its family, empty
## for the built-in font.
var _fonts: KeyGrid
var _font_pager: Pager
var _text_bold: Button
var _text_italic: Button
var _text_height: SliderRow
var _text_solid: Button
## Every font that can be chosen, in the order the pages show them: [shown, family].
var _font_list: Array = []
var _fonts_queued := false
var _last_good_text := ""


func _init(limner_editor: LimnerEditor) -> void:
	super(DESIGN_SIZE)
	editor = limner_editor
	name = "Text"

	_text = TextEdit.new()
	_text.placeholder_text = "Words to write in strokes\nEnter starts a new line"
	_text.custom_minimum_size = Vector2(0, Kit.u(26) * MAX_LINES + Kit.u(12))
	_text.wrap_mode = TextEdit.LINE_WRAPPING_NONE
	_text.scroll_fit_content_height = false
	_text.tooltip_text = "Text is written in strokes of the paint colour on the paint layer, in front of you, and selected so that it can be moved into place. Enter starts a new line; Ctrl+Enter (Cmd+Enter on a Mac) adds the text. Up to %d lines of %d letters." % [MAX_LINES, MAX_LINE]
	_text.gui_input.connect(_on_text_input)
	_text.text_changed.connect(_keep_to_the_box)
	add_child(_text)

	_text_align = KeyGrid.new(3)
	for key: String in TEXT_ALIGNMENTS:
		var words: Array = TEXT_ALIGNMENTS[key]
		_text_align.add_text(key, words[0], words[1])
	_text_align.set_value("centre")
	add_child(_text_align)

	add_child(Kit.caption("Font"))
	_font_search = LineEdit.new()
	_font_search.placeholder_text = "Find a font"
	_font_search.tooltip_text = "Show only the fonts whose name holds these letters. Enter takes the first of them."
	_font_search.custom_minimum_size = Vector2(0, Kit.u(Kit.KEY))
	_font_search.clear_button_enabled = true
	_font_search.text_changed.connect(func(_typed: String) -> void: _fill_fonts())
	_font_search.text_submitted.connect(func(_typed: String) -> void: _take_first_font())
	add_child(_font_search)
	_fonts = KeyGrid.new(2)
	for index in FONTS_PER_PAGE:
		var key := _fonts.add_text(str(index), "", FONT_NOTE)
		key.custom_minimum_size.x = 0
	_fonts.chosen.connect(_on_font_chosen)
	add_child(_fonts)
	_font_pager = Pager.new(FONTS_PER_PAGE, false)
	_font_pager.page_changed.connect(func(_first: int) -> void: _show_font_page())
	add_child(_font_pager)

	_text_bold = Kit.text_key("Bold", "Heavier letters: the font's own bold, or the letters thickened when it has none", Callable(), "", true)
	_text_italic = Kit.text_key("Italic", "Slanted letters: the font's own italic, or the letters leant when it has none", Callable(), "", true)
	_text_solid = Kit.text_key("Filled", "Fill the letters with paint; otherwise only their outlines are drawn", Callable(), "", true)
	_text_solid.button_pressed = true
	for key: Button in [_text_bold, _text_italic, _text_solid]:
		key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	add_child(Kit.key_row([_text_bold, _text_italic, _text_solid]))

	_text_height = SliderRow.new("Height", 0.01, 1.0, 0.001, "How tall the letters are", ToolsPanel._millimetres, true)
	_text_height.set_value(0.1)
	add_child(_text_height)
	var add := Kit.text_key("Add text", "Write the text in strokes", _add_text, "plus")
	add.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	add_child(add)
	_fill_fonts()


func _ready() -> void:
	Preferences.shared().changed.connect(_on_preference_changed)


## Keeps the text within `MAX_LINES` lines of `MAX_LINE` characters, so that the box
## never has to scroll: what goes past is taken back.
func _keep_to_the_box() -> void:
	var lines := _text.text.split("\n")
	var fits := lines.size() <= MAX_LINES
	for line in lines:
		fits = fits and line.length() <= MAX_LINE
	if fits:
		_last_good_text = _text.text
		return
	var caret := Vector2i(_text.get_caret_column(), _text.get_caret_line())
	_text.text = _last_good_text
	_text.set_caret_line(mini(caret.y, _text.get_line_count() - 1))
	_text.set_caret_column(mini(caret.x, _text.get_line(_text.get_caret_line()).length()))


# ---- fonts ----

## Lists the fonts: the built-in font, then the one in use and the others chosen
## last, then every font installed, or only those whose name holds what is in the
## search box; then shows the page that holds the font in use.
func _fill_fonts() -> void:
	var preferences := Preferences.shared()
	var current := preferences.text("text_font")
	var search := _font_search.text.strip_edges()
	_font_list = [[BUILT_IN_FONT, ""]]
	var listed := {"": true}
	var recent: Array = [current]
	# While searching, only the font in use stays first, so the matches come next.
	if search.is_empty():
		recent.append_array(preferences.value("recent_fonts"))
	for family: String in recent:
		if listed.has(family):
			continue
		listed[family] = true
		_font_list.append([family if TextStrokes.is_installed(family) else "%s (not installed)" % family, family])
	for family in TextStrokes.system_fonts():
		if listed.has(family) or (not search.is_empty() and not family.containsn(search)):
			continue
		listed[family] = true
		_font_list.append([family, family])
	_font_pager.set_count(_font_list.size())
	_font_pager.show_item(0 if not search.is_empty() else _font_index(current))
	_show_font_page()


## Puts the fonts of the page shown on the font keys.
func _show_font_page() -> void:
	var current := Preferences.shared().text("text_font")
	for index in FONTS_PER_PAGE:
		var key: Button = _fonts.buttons[str(index)]
		var at := _font_pager.first + index
		key.visible = at < _font_list.size()
		if not key.visible:
			continue
		var entry: Array = _font_list[at]
		key.text = entry[0]
		key.tooltip_text = "%s. %s" % [entry[0], FONT_NOTE]
		key.set_meta("family", entry[1])
		key.set_pressed_no_signal(entry[1] == current)


## Where `family` is in the list of fonts; 0, the built-in font, when it is not there.
func _font_index(family: String) -> int:
	for index in _font_list.size():
		if _font_list[index][1] == family:
			return index
	return 0


func _on_font_chosen(key: String) -> void:
	var button: Button = _fonts.buttons[key]
	Preferences.shared().choose_font(str(button.get_meta("family", "")))


## Chooses the first font the search found.
func _take_first_font() -> void:
	if _font_list.size() > 1:
		Preferences.shared().choose_font(str(_font_list[1][1]))


func _on_preference_changed(key: String) -> void:
	# Listed again after the key has finished reporting the choice that changed it.
	if (key == "text_font" or key == "recent_fonts") and not _fonts_queued:
		_fonts_queued = true
		_refill_fonts.call_deferred()


func _refill_fonts() -> void:
	_fonts_queued = false
	_fill_fonts()


# ---- writing ----

## Ctrl+Enter (Cmd+Enter on a Mac) in the text box adds the text; Enter alone starts a
## new line, as it does from the headset's keyboard.
func _on_text_input(event: InputEvent) -> void:
	var key := event as InputEventKey
	if key != null and key.pressed and (key.keycode == KEY_ENTER or key.keycode == KEY_KP_ENTER) and key.is_command_or_control_pressed():
		_text.accept_event()
		_add_text()


## The font chosen for text, in bold or italic as the keys say. When it is not
## installed here, the built-in font stands in and `fell_back` is set.
func _text_font_now(fell_back: Array[bool]) -> Font:
	var family := Preferences.shared().text("text_font")
	var font := TextStrokes.font_named(family, _text_bold.button_pressed, _text_italic.button_pressed)
	if font == null:
		fell_back[0] = true
		font = TextStrokes.font_named("", _text_bold.button_pressed, _text_italic.button_pressed)
	return font


func _add_text() -> void:
	var fell_back: Array[bool] = [false]
	var font := _text_font_now(fell_back)
	var align: HorizontalAlignment = TEXT_ALIGNMENTS.get(_text_align.get_value(), TEXT_ALIGNMENTS["centre"])[2]
	var count := editor.add_text(_text.text, _text_height.get_value(), _text_solid.button_pressed, font, align)
	if count == 0:
		return
	if fell_back[0]:
		editor.message.emit("\"%s\" is not installed here, so the text is in the built-in font: %d strokes, selected" % [Preferences.shared().text("text_font"), count], false)
	else:
		editor.message.emit("Added the text as %d strokes; it is selected, ready to move" % count, false)
