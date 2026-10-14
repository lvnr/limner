class_name Kit
## Building blocks for Limner's panels: one look, one set of sizes, and the theme.
##
## Controls are sized for a finger or a controller tip as much as for a mouse, since
## the same panels are shown on the desktop and on the hand-held palette in a headset.

const ROW := 30
const GAP := 6
const BUTTON := 32

const BG := Color(0.145, 0.153, 0.169, 0.97)
const BG_RAISED := Color(0.208, 0.220, 0.243)
const BG_HOVER := Color(0.267, 0.282, 0.310)
const BG_SUNKEN := Color(0.098, 0.102, 0.114)
const ACCENT := Color(0.239, 0.545, 0.992)
const TEXT := Color(0.894, 0.902, 0.918)
const TEXT_DIM := Color(0.604, 0.624, 0.659)
const DANGER := Color(0.937, 0.325, 0.314)
const BORDER := Color(0.071, 0.075, 0.082)


static func _box(color: Color, radius := 5, border := 0, border_color := BORDER, margin := 6) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = color
	box.set_corner_radius_all(radius)
	box.set_border_width_all(border)
	box.border_color = border_color
	box.set_content_margin_all(margin)
	return box


## The theme every Limner panel uses.
static func make_theme() -> Theme:
	var theme := Theme.new()
	theme.default_font_size = 14

	for kind in ["Button", "MenuButton", "OptionButton"]:
		theme.set_stylebox("normal", kind, _box(BG_RAISED))
		theme.set_stylebox("hover", kind, _box(BG_HOVER))
		theme.set_stylebox("pressed", kind, _box(ACCENT))
		theme.set_stylebox("hover_pressed", kind, _box(ACCENT.lightened(0.12)))
		theme.set_stylebox("disabled", kind, _box(BG_RAISED.darkened(0.25)))
		theme.set_stylebox("focus", kind, StyleBoxEmpty.new())
		theme.set_color("font_color", kind, TEXT)
		theme.set_color("font_hover_color", kind, Color.WHITE)
		theme.set_color("font_pressed_color", kind, Color.WHITE)
		theme.set_color("font_hover_pressed_color", kind, Color.WHITE)
		theme.set_color("font_focus_color", kind, TEXT)
		theme.set_color("font_disabled_color", kind, TEXT_DIM.darkened(0.3))
		theme.set_color("icon_disabled_color", kind, Color(1, 1, 1, 0.3))

	theme.set_stylebox("panel", "PanelContainer", _box(BG, 6, 1, BORDER, 8))
	theme.set_stylebox("panel", "Panel", _box(BG, 6, 1))
	theme.set_stylebox("panel", "PopupMenu", _box(BG.lightened(0.03), 6, 1, BORDER, 6))
	theme.set_stylebox("hover", "PopupMenu", _box(ACCENT, 4))
	theme.set_color("font_color", "PopupMenu", TEXT)
	theme.set_color("font_hover_color", "PopupMenu", Color.WHITE)
	theme.set_color("font_disabled_color", "PopupMenu", TEXT_DIM.darkened(0.2))
	theme.set_color("font_separator_color", "PopupMenu", TEXT_DIM)
	theme.set_constant("v_separation", "PopupMenu", 8)
	theme.set_stylebox("panel", "TooltipPanel", _box(BG_SUNKEN, 4, 1, BORDER, 6))
	theme.set_color("font_color", "TooltipLabel", TEXT)

	theme.set_color("font_color", "Label", TEXT)
	theme.set_color("font_color", "CheckBox", TEXT)
	theme.set_color("font_hover_color", "CheckBox", Color.WHITE)
	theme.set_color("font_pressed_color", "CheckBox", TEXT)
	theme.set_color("font_color", "CheckButton", TEXT)
	theme.set_color("font_pressed_color", "CheckButton", TEXT)
	theme.set_icon("checked", "CheckBox", _check_icon(true, TEXT))
	theme.set_icon("unchecked", "CheckBox", _check_icon(false, TEXT))
	theme.set_icon("checked_disabled", "CheckBox", _check_icon(true, TEXT_DIM.darkened(0.3)))
	theme.set_icon("unchecked_disabled", "CheckBox", _check_icon(false, TEXT_DIM.darkened(0.3)))
	theme.set_color("font_disabled_color", "CheckBox", TEXT_DIM.darkened(0.3))
	for kind in ["CheckBox", "CheckButton"]:
		theme.set_stylebox("disabled", kind, StyleBoxEmpty.new())
		theme.set_stylebox("normal", kind, StyleBoxEmpty.new())
		theme.set_stylebox("hover", kind, StyleBoxEmpty.new())
		theme.set_stylebox("pressed", kind, StyleBoxEmpty.new())
		theme.set_stylebox("hover_pressed", kind, StyleBoxEmpty.new())
		theme.set_stylebox("focus", kind, StyleBoxEmpty.new())

	theme.set_stylebox("normal", "LineEdit", _box(BG_SUNKEN, 4, 1))
	theme.set_stylebox("focus", "LineEdit", _box(BG_SUNKEN, 4, 1, ACCENT))
	theme.set_stylebox("read_only", "LineEdit", _box(BG_SUNKEN.lightened(0.04), 4, 1))
	theme.set_color("font_color", "LineEdit", TEXT)
	theme.set_color("font_placeholder_color", "LineEdit", TEXT_DIM)
	theme.set_color("caret_color", "LineEdit", TEXT)
	theme.set_stylebox("normal", "TextEdit", _box(BG_SUNKEN, 4, 1))
	theme.set_stylebox("focus", "TextEdit", _box(BG_SUNKEN, 4, 1, ACCENT))
	theme.set_color("font_color", "TextEdit", TEXT)

	var track := _box(BG_SUNKEN, 4, 0, BORDER, 0)
	track.content_margin_top = 4
	track.content_margin_bottom = 4
	var fill := _box(ACCENT, 4, 0, BORDER, 0)
	theme.set_stylebox("slider", "HSlider", track)
	theme.set_stylebox("grabber_area", "HSlider", fill)
	theme.set_stylebox("grabber_area_highlight", "HSlider", fill)
	theme.set_icon("grabber", "HSlider", _knob(TEXT))
	theme.set_icon("grabber_highlight", "HSlider", _knob(Color.WHITE))
	theme.set_icon("grabber_disabled", "HSlider", _knob(TEXT_DIM))

	theme.set_stylebox("panel", "Tree", _box(BG_SUNKEN, 4, 0, BORDER, 2))
	theme.set_stylebox("focus", "Tree", StyleBoxEmpty.new())
	theme.set_stylebox("selected", "Tree", _box(ACCENT.darkened(0.25), 3, 0, BORDER, 2))
	theme.set_stylebox("selected_focus", "Tree", _box(ACCENT.darkened(0.15), 3, 0, BORDER, 2))
	theme.set_stylebox("hovered", "Tree", _box(BG_HOVER.darkened(0.2), 3, 0, BORDER, 2))
	theme.set_stylebox("button_hover", "Tree", _box(BG_HOVER, 3, 0, BORDER, 2))
	theme.set_color("font_color", "Tree", TEXT)
	theme.set_color("font_selected_color", "Tree", Color.WHITE)
	theme.set_color("guide_color", "Tree", Color(1, 1, 1, 0.04))
	theme.set_color("relationship_line_color", "Tree", Color(1, 1, 1, 0.12))
	theme.set_constant("v_separation", "Tree", 7)
	theme.set_constant("button_margin", "Tree", 6)
	theme.set_constant("draw_relationship_lines", "Tree", 1)
	theme.set_constant("inner_item_margin_left", "Tree", 2)

	theme.set_stylebox("panel", "ScrollContainer", StyleBoxEmpty.new())
	theme.set_stylebox("scroll", "VScrollBar", _box(Color(0, 0, 0, 0), 4, 0, BORDER, 4))
	theme.set_stylebox("grabber", "VScrollBar", _box(BG_HOVER, 4, 0, BORDER, 4))
	theme.set_stylebox("grabber_highlight", "VScrollBar", _box(BG_HOVER.lightened(0.15), 4, 0, BORDER, 4))
	theme.set_stylebox("grabber_pressed", "VScrollBar", _box(ACCENT, 4, 0, BORDER, 4))
	theme.set_stylebox("scroll", "HScrollBar", _box(Color(0, 0, 0, 0), 4, 0, BORDER, 4))
	theme.set_stylebox("grabber", "HScrollBar", _box(BG_HOVER, 4, 0, BORDER, 4))
	theme.set_stylebox("grabber_highlight", "HScrollBar", _box(BG_HOVER.lightened(0.15), 4, 0, BORDER, 4))
	theme.set_stylebox("grabber_pressed", "HScrollBar", _box(ACCENT, 4, 0, BORDER, 4))

	theme.set_stylebox("tab_selected", "TabContainer", _tab(BG, true))
	theme.set_stylebox("tab_unselected", "TabContainer", _tab(BG_SUNKEN, false))
	theme.set_stylebox("tab_hovered", "TabContainer", _tab(BG_RAISED, false))
	theme.set_stylebox("tab_focus", "TabContainer", StyleBoxEmpty.new())
	theme.set_stylebox("panel", "TabContainer", _box(BG, 6, 1, BORDER, 8))
	theme.set_color("font_selected_color", "TabContainer", Color.WHITE)
	theme.set_color("font_unselected_color", "TabContainer", TEXT_DIM)
	theme.set_color("font_hovered_color", "TabContainer", TEXT)
	theme.set_stylebox("tab_selected", "TabBar", _tab(BG, true))
	theme.set_stylebox("tab_unselected", "TabBar", _tab(BG_SUNKEN, false))
	theme.set_stylebox("tab_hovered", "TabBar", _tab(BG_RAISED, false))

	theme.set_stylebox("separator", "HSeparator", _line(false))
	theme.set_stylebox("separator", "VSeparator", _line(true))
	theme.set_constant("separation", "HBoxContainer", GAP)
	theme.set_constant("separation", "VBoxContainer", GAP)
	theme.set_constant("h_separation", "GridContainer", GAP)
	theme.set_constant("v_separation", "GridContainer", GAP)
	theme.set_constant("h_separation", "HFlowContainer", GAP)
	theme.set_constant("v_separation", "HFlowContainer", GAP)

	theme.set_stylebox("embedded_border", "Window", _box(BG, 8, 1, BORDER, 10))
	theme.set_stylebox("embedded_unfocused_border", "Window", _box(BG, 8, 1, BORDER, 10))
	theme.set_stylebox("panel", "AcceptDialog", _box(BG, 8, 1, BORDER, 12))
	theme.set_color("title_color", "Window", TEXT)
	return theme


static func _tab(color: Color, selected: bool) -> StyleBoxFlat:
	var box := _box(color, 5, 0, BORDER, 8)
	box.corner_radius_bottom_left = 0
	box.corner_radius_bottom_right = 0
	box.content_margin_top = 6
	box.content_margin_bottom = 6
	if selected:
		box.border_width_top = 2
		box.border_color = ACCENT
	return box


static func _line(vertical: bool) -> StyleBoxLine:
	var line := StyleBoxLine.new()
	line.color = Color(1, 1, 1, 0.08)
	line.vertical = vertical
	line.thickness = 1
	return line


## The box of a check box: an outline, filled with the accent colour and ticked when checked.
static func _check_icon(checked: bool, outline: Color) -> Texture2D:
	var inside := ""
	if checked:
		inside = '<rect x="2" y="2" width="14" height="14" rx="3" fill="#%s"/><path d="M5.500 9.200l2.500 2.500 4.500-5" fill="none" stroke="#ffffff" stroke-width="1.800" stroke-linecap="round" stroke-linejoin="round"/>' % ACCENT.to_html(false)
	else:
		inside = '<rect x="2.500" y="2.500" width="13" height="13" rx="3" fill="#%s" stroke="#%s" stroke-width="1.200"/>' % [BG_SUNKEN.to_html(false), outline.darkened(0.35).to_html(false)]
	var svg := '<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 18 18">%s</svg>' % inside
	return DPITexture.create_from_string(svg)


static func _knob(color: Color) -> Texture2D:
	var svg := '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 14 14"><circle cx="7" cy="7" r="6" fill="#%s" stroke="#121316" stroke-width="1"/></svg>' % color.to_html(false)
	return DPITexture.create_from_string(svg)


# ---- widgets ----

static func label(text: String, dim := false) -> Label:
	var node := Label.new()
	node.text = text
	node.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	if dim:
		node.add_theme_color_override("font_color", TEXT_DIM)
	return node


## A small heading that separates groups of controls inside a panel.
static func heading(text: String) -> Label:
	var node := label(text.to_upper(), true)
	node.add_theme_font_size_override("font_size", 11)
	return node


static func button(text: String, tooltip: String, on_pressed: Callable, icon := "") -> Button:
	var node := Button.new()
	node.text = text
	node.tooltip_text = tooltip
	node.custom_minimum_size = Vector2(0, BUTTON)
	node.focus_mode = Control.FOCUS_NONE
	if icon != "":
		node.icon = Icons.texture(icon, TEXT, 18)
	if on_pressed.is_valid():
		node.pressed.connect(on_pressed)
	return node


## A square button showing only an icon. `toggle` makes it stay down when pressed.
static func icon_button(icon: String, tooltip: String, on_pressed: Callable, toggle := false) -> Button:
	var node := Button.new()
	node.icon = Icons.texture(icon, TEXT, 20)
	node.tooltip_text = tooltip
	node.toggle_mode = toggle
	node.custom_minimum_size = Vector2(BUTTON, BUTTON)
	node.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	node.focus_mode = Control.FOCUS_NONE
	if on_pressed.is_valid():
		node.pressed.connect(on_pressed)
	return node


static func toggle(text: String, tooltip: String, on_toggled: Callable) -> CheckBox:
	var node := CheckBox.new()
	node.text = text
	node.tooltip_text = tooltip
	node.focus_mode = Control.FOCUS_NONE
	node.custom_minimum_size = Vector2(0, ROW)
	if on_toggled.is_valid():
		node.toggled.connect(on_toggled)
	return node


static func row(children: Array = []) -> HBoxContainer:
	var node := HBoxContainer.new()
	for child: Control in children:
		node.add_child(child)
	return node


static func column(children: Array = []) -> VBoxContainer:
	var node := VBoxContainer.new()
	for child: Control in children:
		node.add_child(child)
	return node


static func spacer() -> Control:
	var node := Control.new()
	node.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return node


static func separator() -> HSeparator:
	return HSeparator.new()


static func option(items: Array, tooltip: String, on_selected: Callable) -> OptionButton:
	var node := OptionButton.new()
	node.tooltip_text = tooltip
	node.focus_mode = Control.FOCUS_NONE
	node.custom_minimum_size = Vector2(0, BUTTON)
	node.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for item: String in items:
		node.add_item(item)
	if on_selected.is_valid():
		node.item_selected.connect(on_selected)
	return node


## Sets a control's value without it reporting the change as if the user had made it.
static func set_quiet(control: Control, value: Variant) -> void:
	if control is Range:
		control.set_value_no_signal(value)
	elif control is OptionButton:
		control.select(value)
	elif control is BaseButton:
		control.set_pressed_no_signal(value)
	elif control is LineEdit:
		if not control.has_focus() and control.text != str(value):
			control.text = str(value)


## Gives a button `margin` pixels of padding at its sides instead of the usual six,
## for strips that have to fit many buttons across a narrow panel.
static func narrow(button: Button, margin := 3) -> void:
	var looks := {
		"normal": BG_RAISED,
		"hover": BG_HOVER,
		"pressed": ACCENT,
		"hover_pressed": ACCENT.lightened(0.12),
		"disabled": BG_RAISED.darkened(0.25),
	}
	for state: String in looks:
		var box := _box(looks[state])
		box.content_margin_left = margin
		box.content_margin_right = margin
		button.add_theme_stylebox_override(state, box)


## A drop-down that takes the width its row has left and cuts long entries short,
## where `option` makes one as wide as its longest entry. For menus whose entries
## are descriptions that would push a panel wider than a narrow dock.
static func fitted_option(items: Array, tooltip: String, on_selected: Callable) -> OptionButton:
	var node := option(items, tooltip, on_selected)
	node.fit_to_longest_item = false
	node.clip_text = true
	return node
