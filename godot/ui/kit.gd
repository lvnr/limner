class_name Kit
## Building blocks for Limner's panels: one look, one set of sizes, and the theme.
##
## Controls are sized for a finger or a controller tip as much as for a mouse, since
## the same panels are shown on the desktop and on the hand-held palette in a headset.

const ROW := 30
const GAP := 6
const BUTTON := 32

## Sizes of the Quill-style panels, in design pixels (see `u`): a key with an icon,
## the transport's bigger keys, the small keys of a title bar, the room between keys,
## and the margin inside a panel.
const KEY := 38
const KEY_BIG := 52
const KEY_SMALL := 26
const SPACE := 8
const MARGIN := 14
## How much smaller the desktop builds a panel than the headset does. Text keeps its
## own sizes (see `text_size`), so that it stays readable on a screen.
const DESKTOP_SCALE := 0.8

## The colour schemes the interface can be shown in. Each gives every role below a
## colour; `use_scheme` makes one of them current, and from then on `Kit.TEXT` and
## the rest have its values. Nothing in a panel names a colour of its own: it reads
## the role from Kit when it builds or draws, so that a change of scheme reaches it.
##
## - `BG`: panels. `BG_RAISED`: buttons. `BG_HOVER`: a button under the pointer.
##   `BG_SUNKEN`: boxes to type in, lists, the timeline's tracks. `BG_POPUP`: menus
##   and pop-ups, always solid. `BG_DISABLED`: a button that cannot be pressed.
## - `ACCENT`: what is on or chosen. `ACCENT_HOVER`: the same under the pointer.
##   `ACCENT_TEXT`: the accent as text or an icon on a panel.
## - `PRESSED`, `PRESSED_HOVER`, `TEXT_ON_PRESSED`: a button that is down.
## - `SELECTION`, `SELECTION_SOFT`, `TEXT_ON_SELECTION`: the chosen row of a list, and
##   rows that are selected without being the chosen one.
## - `TEXT`, `TEXT_DIM` (labels and notes), `TEXT_DISABLED`, `TEXT_STRONG` (text
##   under the pointer), `TEXT_ON_ACCENT` (on an `ACCENT` fill).
## - `DANGER`, `DANGER_TEXT`: warnings and things that delete or record.
## - `OK`, `WARN`: a measure that is fine, or close to its limit.
## - `BORDER`, `LINE` (separators), `SCROLL` (scroll bar handles), `KNOB` (slider handles).
## - `INK`, `SHADE`: the colours of faint marks drawn over a panel, toward the text and
##   away from it. Used with an alpha of their own.
## - `AXIS_X`, `AXIS_Y`, `AXIS_Z`: labels of the three axes.
## - The timeline: `RULER`, `NAMES` (the column of layer names), `ROW_ALT` (every
##   other track), `ROW_LINE` (between tracks), `RANGE_SHADE` (outside the working
##   range), `PLAYHEAD`, `CHOSEN` (selected keys).
const SCHEMES := {
	"dark": {
		"BG": Color(0.145, 0.153, 0.169, 0.97),
		"BG_RAISED": Color(0.208, 0.220, 0.243),
		"BG_HOVER": Color(0.267, 0.282, 0.310),
		"BG_SUNKEN": Color(0.098, 0.102, 0.114),
		"BG_POPUP": Color(0.171, 0.178, 0.194),
		"BG_DISABLED": Color(0.156, 0.165, 0.182),
		"ACCENT": Color(0.239, 0.545, 0.992),
		"ACCENT_HOVER": Color(0.330, 0.600, 0.993),
		"ACCENT_TEXT": Color(0.429, 0.659, 0.994),
		"PRESSED": Color(0.239, 0.545, 0.992),
		"PRESSED_HOVER": Color(0.330, 0.600, 0.993),
		"TEXT_ON_PRESSED": Color(1.0, 1.0, 1.0),
		"SELECTION": Color(0.179, 0.409, 0.744),
		"SELECTION_SOFT": Color(0.108, 0.245, 0.446),
		"TEXT_ON_SELECTION": Color(1.0, 1.0, 1.0),
		"TEXT": Color(0.894, 0.902, 0.918),
		"TEXT_DIM": Color(0.604, 0.624, 0.659),
		"TEXT_DISABLED": Color(0.423, 0.437, 0.461),
		"TEXT_STRONG": Color(0.980, 0.984, 0.988),
		"TEXT_ON_ACCENT": Color(1.0, 1.0, 1.0),
		"DANGER": Color(0.937, 0.325, 0.314),
		"DANGER_TEXT": Color(0.956, 0.528, 0.520),
		"OK": Color(0.35, 0.75, 0.45),
		"WARN": Color(0.95, 0.72, 0.25),
		"BORDER": Color(0.071, 0.075, 0.082),
		"LINE": Color(1.0, 1.0, 1.0, 0.08),
		"SCROLL": Color(0.267, 0.282, 0.310),
		"KNOB": Color(0.894, 0.902, 0.918),
		"INK": Color(1.0, 1.0, 1.0),
		"SHADE": Color(0.0, 0.0, 0.0),
		"AXIS_X": Color(0.94, 0.45, 0.43),
		"AXIS_Y": Color(0.52, 0.80, 0.44),
		"AXIS_Z": Color(0.45, 0.65, 0.99),
		"RULER": Color(0.146, 0.154, 0.170),
		"NAMES": Color(0.162, 0.172, 0.190),
		"ROW_ALT": Color(1.0, 1.0, 1.0, 0.02),
		"ROW_LINE": Color(0.0, 0.0, 0.0, 0.25),
		"RANGE_SHADE": Color(0.0, 0.0, 0.0, 0.28),
		"PLAYHEAD": Color(1.0, 0.58, 0.12),
		"CHOSEN": Color(1.0, 0.85, 0.32),
	},
	# Warm light greys rather than the dark scheme turned over: panels a little darker
	# than the buttons on them, wells a little darker still, near-black text, and the
	# accent deepened so that white on it, and it on the panels, stay easy to read.
	"light": {
		"BG": Color(0.894, 0.882, 0.863, 0.97),
		"BG_RAISED": Color(0.976, 0.969, 0.957),
		"BG_HOVER": Color(0.996, 0.994, 0.990),
		"BG_SUNKEN": Color(0.816, 0.800, 0.773),
		"BG_POPUP": Color(0.953, 0.945, 0.933),
		"BG_DISABLED": Color(0.867, 0.855, 0.835),
		"ACCENT": Color(0.184, 0.435, 0.839),
		"ACCENT_HOVER": Color(0.243, 0.494, 0.890),
		"ACCENT_TEXT": Color(0.122, 0.341, 0.718),
		"PRESSED": Color(0.769, 0.843, 0.973),
		"PRESSED_HOVER": Color(0.816, 0.878, 0.984),
		"TEXT_ON_PRESSED": Color(0.059, 0.153, 0.341),
		"SELECTION": Color(0.212, 0.463, 0.867),
		"SELECTION_SOFT": Color(0.761, 0.827, 0.937),
		"TEXT_ON_SELECTION": Color(0.996, 0.996, 1.0),
		"TEXT": Color(0.118, 0.114, 0.106),
		"TEXT_DIM": Color(0.365, 0.349, 0.325),
		"TEXT_DISABLED": Color(0.580, 0.561, 0.533),
		"TEXT_STRONG": Color(0.020, 0.020, 0.020),
		"TEXT_ON_ACCENT": Color(1.0, 1.0, 1.0),
		"DANGER": Color(0.788, 0.227, 0.200),
		"DANGER_TEXT": Color(0.698, 0.157, 0.133),
		"OK": Color(0.20, 0.60, 0.31),
		"WARN": Color(0.86, 0.56, 0.06),
		"BORDER": Color(0.702, 0.682, 0.651),
		"LINE": Color(0.0, 0.0, 0.0, 0.12),
		"SCROLL": Color(0.690, 0.671, 0.639),
		"KNOB": Color(0.996, 0.996, 0.996),
		"INK": Color(0.090, 0.082, 0.071),
		"SHADE": Color(1.0, 1.0, 1.0),
		"AXIS_X": Color(0.78, 0.20, 0.18),
		"AXIS_Y": Color(0.20, 0.53, 0.14),
		"AXIS_Z": Color(0.14, 0.38, 0.86),
		"RULER": Color(0.855, 0.843, 0.820),
		"NAMES": Color(0.929, 0.918, 0.902),
		"ROW_ALT": Color(0.0, 0.0, 0.0, 0.025),
		"ROW_LINE": Color(0.30, 0.26, 0.22, 0.16),
		"RANGE_SHADE": Color(0.30, 0.26, 0.22, 0.18),
		"PLAYHEAD": Color(0.94, 0.42, 0.0),
		"CHOSEN": Color(0.98, 0.66, 0.0),
	},
}
## The roles a colour already handed out is recognised as, when the scheme changes
## (see `recolor`). They are told apart by colour, so no two of them, in any scheme,
## share one.
const RECOGNISED := [
	"BG", "BG_RAISED", "BG_HOVER", "BG_SUNKEN", "BG_POPUP", "ACCENT", "ACCENT_TEXT",
	"TEXT", "TEXT_DIM", "TEXT_DISABLED", "DANGER", "DANGER_TEXT", "BORDER",
	"AXIS_X", "AXIS_Y", "AXIS_Z",
]

## Lets whoever shows panels know that the scheme changed.
class Events extends RefCounted:
	## Every colour in Kit has its new value. Whoever owns a themed root re-applies
	## the theme with `Kit.apply_theme`.
	signal scheme_changed

	func _on_preference_changed(key: String) -> void:
		if key == Kit.scheme_preference():
			Kit.use_scheme(Preferences.shared().text(key))


## The name of the scheme in use, a key of `SCHEMES`.
static var scheme := "dark"
## True while the panels are shown in a headset. Its theme gives buttons an outline
## and a larger text, and its colours follow a preference of their own (see
## `set_headset`).
static var headset := false
static var events := Events.new()
static var _following := false

static var BG: Color = SCHEMES.dark.BG
static var BG_RAISED: Color = SCHEMES.dark.BG_RAISED
static var BG_HOVER: Color = SCHEMES.dark.BG_HOVER
static var BG_SUNKEN: Color = SCHEMES.dark.BG_SUNKEN
static var BG_POPUP: Color = SCHEMES.dark.BG_POPUP
static var BG_DISABLED: Color = SCHEMES.dark.BG_DISABLED
static var ACCENT: Color = SCHEMES.dark.ACCENT
static var ACCENT_HOVER: Color = SCHEMES.dark.ACCENT_HOVER
static var ACCENT_TEXT: Color = SCHEMES.dark.ACCENT_TEXT
static var PRESSED: Color = SCHEMES.dark.PRESSED
static var PRESSED_HOVER: Color = SCHEMES.dark.PRESSED_HOVER
static var TEXT_ON_PRESSED: Color = SCHEMES.dark.TEXT_ON_PRESSED
static var SELECTION: Color = SCHEMES.dark.SELECTION
static var SELECTION_SOFT: Color = SCHEMES.dark.SELECTION_SOFT
static var TEXT_ON_SELECTION: Color = SCHEMES.dark.TEXT_ON_SELECTION
static var TEXT: Color = SCHEMES.dark.TEXT
static var TEXT_DIM: Color = SCHEMES.dark.TEXT_DIM
static var TEXT_DISABLED: Color = SCHEMES.dark.TEXT_DISABLED
static var TEXT_STRONG: Color = SCHEMES.dark.TEXT_STRONG
static var TEXT_ON_ACCENT: Color = SCHEMES.dark.TEXT_ON_ACCENT
static var DANGER: Color = SCHEMES.dark.DANGER
static var DANGER_TEXT: Color = SCHEMES.dark.DANGER_TEXT
static var OK: Color = SCHEMES.dark.OK
static var WARN: Color = SCHEMES.dark.WARN
static var BORDER: Color = SCHEMES.dark.BORDER
static var LINE: Color = SCHEMES.dark.LINE
static var SCROLL: Color = SCHEMES.dark.SCROLL
static var KNOB: Color = SCHEMES.dark.KNOB
static var INK: Color = SCHEMES.dark.INK
static var SHADE: Color = SCHEMES.dark.SHADE
static var AXIS_X: Color = SCHEMES.dark.AXIS_X
static var AXIS_Y: Color = SCHEMES.dark.AXIS_Y
static var AXIS_Z: Color = SCHEMES.dark.AXIS_Z
static var RULER: Color = SCHEMES.dark.RULER
static var NAMES: Color = SCHEMES.dark.NAMES
static var ROW_ALT: Color = SCHEMES.dark.ROW_ALT
static var ROW_LINE: Color = SCHEMES.dark.ROW_LINE
static var RANGE_SHADE: Color = SCHEMES.dark.RANGE_SHADE
static var PLAYHEAD: Color = SCHEMES.dark.PLAYHEAD
static var CHOSEN: Color = SCHEMES.dark.CHOSEN


## Makes `name` the scheme in use and reports `events.scheme_changed`. An unknown name,
## or the one already in use, changes nothing.
static func use_scheme(name: String) -> void:
	if not SCHEMES.has(name) or name == scheme:
		return
	scheme = name
	var colors: Dictionary = SCHEMES[name]
	BG = colors.BG
	BG_RAISED = colors.BG_RAISED
	BG_HOVER = colors.BG_HOVER
	BG_SUNKEN = colors.BG_SUNKEN
	BG_POPUP = colors.BG_POPUP
	BG_DISABLED = colors.BG_DISABLED
	ACCENT = colors.ACCENT
	ACCENT_HOVER = colors.ACCENT_HOVER
	ACCENT_TEXT = colors.ACCENT_TEXT
	PRESSED = colors.PRESSED
	PRESSED_HOVER = colors.PRESSED_HOVER
	TEXT_ON_PRESSED = colors.TEXT_ON_PRESSED
	SELECTION = colors.SELECTION
	SELECTION_SOFT = colors.SELECTION_SOFT
	TEXT_ON_SELECTION = colors.TEXT_ON_SELECTION
	TEXT = colors.TEXT
	TEXT_DIM = colors.TEXT_DIM
	TEXT_DISABLED = colors.TEXT_DISABLED
	TEXT_STRONG = colors.TEXT_STRONG
	TEXT_ON_ACCENT = colors.TEXT_ON_ACCENT
	DANGER = colors.DANGER
	DANGER_TEXT = colors.DANGER_TEXT
	OK = colors.OK
	WARN = colors.WARN
	BORDER = colors.BORDER
	LINE = colors.LINE
	SCROLL = colors.SCROLL
	KNOB = colors.KNOB
	INK = colors.INK
	SHADE = colors.SHADE
	AXIS_X = colors.AXIS_X
	AXIS_Y = colors.AXIS_Y
	AXIS_Z = colors.AXIS_Z
	RULER = colors.RULER
	NAMES = colors.NAMES
	ROW_ALT = colors.ROW_ALT
	ROW_LINE = colors.ROW_LINE
	RANGE_SHADE = colors.RANGE_SHADE
	PLAYHEAD = colors.PLAYHEAD
	CHOSEN = colors.CHOSEN
	events.scheme_changed.emit()


## Takes the scheme from the preference that applies (see `scheme_preference`) now
## and whenever it changes. Each shell calls this as it starts; calling it again does
## no harm. `--scheme <name>` on the command line (after `--`) wins over the
## preference for that run, without changing it, for screenshots of either scheme.
static func follow_preferences() -> void:
	var args := OS.get_cmdline_user_args()
	var at := args.find("--scheme")
	if at >= 0 and at + 1 < args.size():
		use_scheme(args[at + 1])
	else:
		use_scheme(Preferences.shared().text(scheme_preference()))
	if not _following:
		_following = true
		Preferences.shared().changed.connect(events._on_preference_changed)


## The preference that names the scheme: "headset_theme" while the panels are shown
## in a headset, "theme" on the desktop.
static func scheme_preference() -> String:
	return "headset_theme" if headset else "theme"


## Shows the panels the headset's way, or the desktop's. The shell that starts calls
## it before `follow_preferences`. Reports `events.scheme_changed` when it changes
## anything, so that every themed root takes the new theme.
static func set_headset(on: bool) -> void:
	if on == headset:
		return
	headset = on
	events.scheme_changed.emit()


## A size in design pixels as pixels of the interface in use: as it is in a headset,
## where a design pixel is 0.8 mm, and `DESKTOP_SCALE` of it on the desktop.
static func u(pixels: float) -> int:
	return roundi(pixels * (1.0 if headset else DESKTOP_SCALE))


## The size of text in a role, for the interface in use: "title" (a panel's own
## heading, such as the tool's name), "body", "caption" (the word or two over a group
## of keys), "small" (the setting written under a key), "big" (the timecode).
static func text_size(role: String) -> int:
	match role:
		"title":
			return 18 if headset else 15
		"caption":
			return 13 if headset else 12
		"small":
			return 11 if headset else 10
		"big":
			return 22 if headset else 17
	return 15 if headset else 14


## `color` as the same role in the scheme in use: a colour that one of the `RECOGNISED`
## roles had in any scheme comes back as that role's colour now, keeping its own alpha.
## Any other colour comes back as it is.
static func recolor(color: Color) -> Color:
	for scheme_name: String in SCHEMES:
		var colors: Dictionary = SCHEMES[scheme_name]
		for role: String in RECOGNISED:
			var known: Color = colors[role]
			if is_equal_approx(known.r, color.r) and is_equal_approx(known.g, color.g) and is_equal_approx(known.b, color.b):
				var now: Color = SCHEMES[scheme][role]
				return Color(now, color.a)
	return color


## Gives `root` a freshly made theme (or `theme`), hands it to the windows and controls
## under it that had the old one, and brings what was built with the old colours in
## line (see `restyle`).
static func apply_theme(root: Control, theme: Theme = null) -> void:
	var old := root.theme
	var fresh := theme if theme != null else make_theme()
	root.theme = fresh
	var nodes: Array[Node] = []
	_gather(root, nodes)
	if old != null and old != fresh:
		for node: Node in nodes:
			if (node is Control or node is Window) and node.get("theme") == old:
				node.set("theme", fresh)
	restyle(root)


## Brings everything under `node` (and `node`) in line with the scheme in use, for what
## a theme does not reach: colour and style box overrides set from Kit's colours, icons
## made by `Icons` in them, and custom drawing. A node can do the rest itself in a
## method `apply_scheme()`, which is called last.
static func restyle(node: Node) -> void:
	var nodes: Array[Node] = []
	_gather(node, nodes)
	for each: Node in nodes:
		_restyle_one(each)
	for each: Node in nodes:
		if each.has_method("apply_scheme"):
			each.call("apply_scheme")


## `node` and everything under it, including the parts controls make for themselves
## (the list of an option button, say), which are internal children.
static func _gather(node: Node, into: Array[Node]) -> void:
	into.append(node)
	for child: Node in node.get_children(true):
		_gather(child, into)


const _COLOR_NAMES := ["font_color", "font_hover_color", "font_pressed_color", "font_disabled_color", "font_outline_color", "font_placeholder_color"]
const _BOX_NAMES := ["normal", "hover", "pressed", "hover_pressed", "disabled", "focus", "panel", "background", "fill", "read_only"]


static func _restyle_one(node: Node) -> void:
	if node is Control:
		var control: Control = node
		for color_name: String in _COLOR_NAMES:
			if control.has_theme_color_override(color_name):
				control.add_theme_color_override(color_name, recolor(control.get_theme_color(color_name)))
		for box_name: String in _BOX_NAMES:
			if control.has_theme_stylebox_override(box_name):
				var box := control.get_theme_stylebox(box_name)
				if box is StyleBoxFlat:
					var flat: StyleBoxFlat = (box as StyleBoxFlat).duplicate()
					flat.bg_color = recolor(flat.bg_color)
					flat.border_color = recolor(flat.border_color)
					control.add_theme_stylebox_override(box_name, flat)
		control.queue_redraw()
	if node is Button:
		var button: Button = node
		button.icon = Icons.retint(button.icon)
	if node is OptionButton:
		var option_button: OptionButton = node
		for index in option_button.item_count:
			option_button.set_item_icon(index, Icons.retint(option_button.get_item_icon(index)))
	elif node is PopupMenu:
		var menu: PopupMenu = node
		for index in menu.item_count:
			menu.set_item_icon(index, Icons.retint(menu.get_item_icon(index)))
	elif node is TabContainer:
		var tabs: TabContainer = node
		for index in tabs.get_tab_count():
			tabs.set_tab_icon(index, Icons.retint(tabs.get_tab_icon(index)))
	elif node is TabBar:
		var bar: TabBar = node
		for index in bar.tab_count:
			bar.set_tab_icon(index, Icons.retint(bar.get_tab_icon(index)))
	elif node is TextureRect:
		var picture: TextureRect = node
		picture.texture = Icons.retint(picture.texture)
	elif node is Tree:
		var tree: Tree = node
		var item := tree.get_root()
		while item != null:
			for column in tree.columns:
				item.set_icon(column, Icons.retint(item.get_icon(column)))
				for index in item.get_button_count(column):
					item.set_button(column, index, Icons.retint(item.get_button(column, index)))
			item = item.get_next_in_tree()


static func _box(color: Color, radius := 5, border := 0, border_color := BORDER, margin := 6) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = color
	box.set_corner_radius_all(radius)
	box.set_border_width_all(border)
	box.border_color = border_color
	box.set_content_margin_all(margin)
	return box


static var _font: Font


## The interface's typeface: Noto Sans (SIL Open Font Licence, `ui/fonts/OFL.txt`).
static func font() -> Font:
	if _font == null:
		_font = load("res://ui/fonts/NotoSans.ttf") as Font
	return _font


## The theme every Limner panel uses.
static func make_theme() -> Theme:
	var theme := Theme.new()
	theme.default_font = font()
	theme.default_font_size = 15 if headset else 14

	for kind in ["Button", "MenuButton", "OptionButton"]:
		theme.set_stylebox("normal", kind, _key_box(BG_RAISED))
		theme.set_stylebox("hover", kind, _key_box(BG_HOVER))
		theme.set_stylebox("pressed", kind, _key_box(PRESSED, true))
		theme.set_stylebox("hover_pressed", kind, _key_box(PRESSED_HOVER, true))
		theme.set_stylebox("disabled", kind, _key_box(BG_DISABLED, false, true))
		theme.set_stylebox("focus", kind, StyleBoxEmpty.new())
		theme.set_color("font_color", kind, TEXT)
		theme.set_color("font_hover_color", kind, TEXT_STRONG)
		theme.set_color("font_pressed_color", kind, TEXT_ON_PRESSED)
		theme.set_color("font_hover_pressed_color", kind, TEXT_ON_PRESSED)
		theme.set_color("font_focus_color", kind, TEXT)
		theme.set_color("font_disabled_color", kind, TEXT_DISABLED)
		theme.set_color("icon_disabled_color", kind, Color(1, 1, 1, 0.3))
	# Keys (see `key`): buttons whose icon is drawn white and tinted here for each
	# state, so that a change of scheme reaches them with the theme, and with little
	# room around the icon.
	# Text keys are the same with room for a word.
	for variation: String in ["Key", "TextKey"]:
		theme.set_type_variation(variation, "Button")
		for state: String in ["normal", "hover", "pressed", "hover_pressed", "disabled"]:
			var key_box: StyleBoxFlat = (theme.get_stylebox(state, "Button") as StyleBoxFlat).duplicate()
			key_box.set_content_margin_all(2)
			if variation == "TextKey":
				key_box.content_margin_left = u(8)
				key_box.content_margin_right = u(8)
			theme.set_stylebox(state, variation, key_box)
		theme.set_color("icon_normal_color", variation, TEXT)
		theme.set_color("icon_hover_color", variation, TEXT_STRONG)
		theme.set_color("icon_focus_color", variation, TEXT)
		theme.set_color("icon_pressed_color", variation, TEXT_ON_PRESSED)
		theme.set_color("icon_hover_pressed_color", variation, TEXT_ON_PRESSED)
		theme.set_color("icon_disabled_color", variation, TEXT_DISABLED)
	# Narrow buttons (see `narrow`) are buttons with less room at their sides.
	for margin in [1, 2, 3]:
		var narrow_kind := "NarrowButton%d" % margin
		theme.set_type_variation(narrow_kind, "Button")
		for state: String in ["normal", "hover", "pressed", "hover_pressed", "disabled"]:
			var box: StyleBoxFlat = (theme.get_stylebox(state, "Button") as StyleBoxFlat).duplicate()
			box.content_margin_left = margin
			box.content_margin_right = margin
			theme.set_stylebox(state, narrow_kind, box)

	theme.set_stylebox("panel", "PanelContainer", _box(BG, 6, 1, BORDER, 8))
	# A sunken box that shows a value (the value of a `Cycler`, a page count).
	theme.set_type_variation("Well", "PanelContainer")
	theme.set_stylebox("panel", "Well", _box(BG_SUNKEN, 5, 1, BORDER, 4))
	theme.set_stylebox("panel", "Panel", _box(BG, 6, 1))
	theme.set_stylebox("panel", "PopupMenu", _box(BG_POPUP, 6, 1, BORDER, 6))
	theme.set_stylebox("hover", "PopupMenu", _box(PRESSED, 4))
	theme.set_color("font_color", "PopupMenu", TEXT)
	theme.set_color("font_hover_color", "PopupMenu", TEXT_ON_PRESSED)
	theme.set_color("font_disabled_color", "PopupMenu", TEXT_DISABLED)
	theme.set_color("font_separator_color", "PopupMenu", TEXT_DIM)
	theme.set_color("font_accelerator_color", "PopupMenu", TEXT_DIM)
	theme.set_icon("checked", "PopupMenu", _check_icon(true, TEXT_DIM, 16))
	theme.set_icon("unchecked", "PopupMenu", _check_icon(false, TEXT_DIM, 16))
	theme.set_icon("radio_checked", "PopupMenu", _radio_icon(true, TEXT_DIM))
	theme.set_icon("radio_unchecked", "PopupMenu", _radio_icon(false, TEXT_DIM))
	theme.set_icon("submenu", "PopupMenu", _chevron(false, TEXT_DIM))
	theme.set_constant("v_separation", "PopupMenu", 8)
	theme.set_stylebox("panel", "TooltipPanel", _box(BG_SUNKEN, 4, 1, BORDER, 6))
	theme.set_color("font_color", "TooltipLabel", TEXT)

	theme.set_color("font_color", "Label", TEXT)
	theme.set_color("font_color", "CheckBox", TEXT)
	theme.set_color("font_hover_color", "CheckBox", TEXT_STRONG)
	theme.set_color("font_pressed_color", "CheckBox", TEXT)
	theme.set_color("font_color", "CheckButton", TEXT)
	theme.set_color("font_pressed_color", "CheckButton", TEXT)
	theme.set_icon("checked", "CheckBox", _check_icon(true, TEXT_DIM))
	theme.set_icon("unchecked", "CheckBox", _check_icon(false, TEXT_DIM))
	theme.set_icon("checked_disabled", "CheckBox", _check_icon(true, TEXT_DISABLED))
	theme.set_icon("unchecked_disabled", "CheckBox", _check_icon(false, TEXT_DISABLED))
	theme.set_color("font_disabled_color", "CheckBox", TEXT_DISABLED)

	var bar_back := _box(BG_SUNKEN, 4, 0, BORDER, 0)
	var bar_fill := _box(ACCENT, 4, 0, BORDER, 0)
	theme.set_stylebox("background", "ProgressBar", bar_back)
	theme.set_stylebox("fill", "ProgressBar", bar_fill)
	theme.set_color("font_color", "ProgressBar", TEXT)
	for kind in ["CheckBox", "CheckButton"]:
		theme.set_stylebox("disabled", kind, StyleBoxEmpty.new())
		theme.set_stylebox("normal", kind, StyleBoxEmpty.new())
		theme.set_stylebox("hover", kind, StyleBoxEmpty.new())
		theme.set_stylebox("pressed", kind, StyleBoxEmpty.new())
		theme.set_stylebox("hover_pressed", kind, StyleBoxEmpty.new())
		theme.set_stylebox("focus", kind, StyleBoxEmpty.new())

	theme.set_stylebox("normal", "LineEdit", _box(BG_SUNKEN, 4, 1))
	theme.set_stylebox("focus", "LineEdit", _box(BG_SUNKEN, 4, 1, ACCENT))
	theme.set_stylebox("read_only", "LineEdit", _box(BG_SUNKEN.lerp(BG, 0.3), 4, 1))
	theme.set_stylebox("normal", "TextEdit", _box(BG_SUNKEN, 4, 1))
	theme.set_stylebox("focus", "TextEdit", _box(BG_SUNKEN, 4, 1, ACCENT))
	theme.set_stylebox("read_only", "TextEdit", _box(BG_SUNKEN.lerp(BG, 0.3), 4, 1))
	for kind in ["LineEdit", "TextEdit"]:
		theme.set_color("font_color", kind, TEXT)
		theme.set_color("font_placeholder_color", kind, TEXT_DIM)
		theme.set_color("font_uneditable_color", kind, TEXT_DIM)
		theme.set_color("font_readonly_color", kind, TEXT_DIM)
		theme.set_color("font_selected_color", kind, TEXT_STRONG)
		theme.set_color("selection_color", kind, Color(ACCENT, 0.35))
		theme.set_color("caret_color", kind, TEXT)
		theme.set_color("clear_button_color", kind, TEXT_DIM)
	theme.set_color("current_line_color", "TextEdit", Color(INK, 0.04))

	# The menus along the top of the desktop window.
	theme.set_color("font_color", "MenuBar", TEXT)
	theme.set_color("font_hover_color", "MenuBar", TEXT_STRONG)
	theme.set_color("font_pressed_color", "MenuBar", TEXT_STRONG)
	theme.set_color("font_hover_pressed_color", "MenuBar", TEXT_STRONG)
	theme.set_color("font_focus_color", "MenuBar", TEXT)
	theme.set_color("font_disabled_color", "MenuBar", TEXT_DISABLED)

	var track := _box(BG_SUNKEN, 4, 0, BORDER, 0)
	track.content_margin_top = 4
	track.content_margin_bottom = 4
	var fill := _box(ACCENT, 4, 0, BORDER, 0)
	theme.set_stylebox("slider", "HSlider", track)
	theme.set_stylebox("grabber_area", "HSlider", fill)
	theme.set_stylebox("grabber_area_highlight", "HSlider", fill)
	theme.set_icon("grabber", "HSlider", _knob(KNOB))
	theme.set_icon("grabber_highlight", "HSlider", _knob(KNOB.lerp(ACCENT, 0.2)))
	theme.set_icon("grabber_disabled", "HSlider", _knob(TEXT_DISABLED))

	theme.set_stylebox("panel", "Tree", _box(BG_SUNKEN, 4, 0, BORDER, 2))
	theme.set_stylebox("focus", "Tree", StyleBoxEmpty.new())
	theme.set_stylebox("selected", "Tree", _box(SELECTION, 3, 0, BORDER, 2))
	theme.set_stylebox("selected_focus", "Tree", _box(SELECTION.lerp(ACCENT, 0.4), 3, 0, BORDER, 2))
	theme.set_stylebox("hovered", "Tree", _box(BG_RAISED, 3, 0, BORDER, 2))
	theme.set_stylebox("button_hover", "Tree", _box(BG_HOVER, 3, 0, BORDER, 2))
	theme.set_color("font_color", "Tree", TEXT)
	theme.set_color("font_selected_color", "Tree", TEXT_ON_SELECTION)
	theme.set_color("guide_color", "Tree", Color(INK, 0.04))
	theme.set_color("relationship_line_color", "Tree", Color(INK, 0.14))
	theme.set_color("parent_hl_line_color", "Tree", Color(INK, 0.24))
	theme.set_color("children_hl_line_color", "Tree", Color(INK, 0.24))
	# The outline of the row the keyboard is on, drawn over the chosen row's fill.
	var cursor := _box(Color(0, 0, 0, 0), 3, 1, Color(TEXT_ON_SELECTION, 0.85), 2)
	theme.set_stylebox("cursor", "Tree", cursor)
	theme.set_stylebox("cursor_unfocused", "Tree", cursor)
	theme.set_icon("arrow", "Tree", _chevron(true, TEXT_DIM))
	theme.set_icon("arrow_collapsed", "Tree", _chevron(false, TEXT_DIM))
	theme.set_icon("arrow_collapsed_mirrored", "Tree", _chevron(false, TEXT_DIM, true))
	theme.set_constant("v_separation", "Tree", 7)
	theme.set_constant("button_margin", "Tree", 6)
	theme.set_constant("draw_relationship_lines", "Tree", 1)
	theme.set_constant("inner_item_margin_left", "Tree", 2)

	theme.set_stylebox("panel", "ScrollContainer", StyleBoxEmpty.new())
	theme.set_stylebox("scroll", "VScrollBar", _box(Color(0, 0, 0, 0), 4, 0, BORDER, 4))
	theme.set_stylebox("grabber", "VScrollBar", _box(SCROLL, 4, 0, BORDER, 4))
	theme.set_stylebox("grabber_highlight", "VScrollBar", _box(SCROLL.lerp(TEXT, 0.15), 4, 0, BORDER, 4))
	theme.set_stylebox("grabber_pressed", "VScrollBar", _box(ACCENT, 4, 0, BORDER, 4))
	theme.set_stylebox("scroll", "HScrollBar", _box(Color(0, 0, 0, 0), 4, 0, BORDER, 4))
	theme.set_stylebox("grabber", "HScrollBar", _box(SCROLL, 4, 0, BORDER, 4))
	theme.set_stylebox("grabber_highlight", "HScrollBar", _box(SCROLL.lerp(TEXT, 0.15), 4, 0, BORDER, 4))
	theme.set_stylebox("grabber_pressed", "HScrollBar", _box(ACCENT, 4, 0, BORDER, 4))

	theme.set_stylebox("tab_selected", "TabContainer", _tab(BG, true))
	theme.set_stylebox("tab_unselected", "TabContainer", _tab(BG_SUNKEN, false))
	theme.set_stylebox("tab_hovered", "TabContainer", _tab(BG_RAISED, false))
	theme.set_stylebox("tab_focus", "TabContainer", StyleBoxEmpty.new())
	theme.set_stylebox("panel", "TabContainer", _box(BG, 6, 1, BORDER, 8))
	theme.set_color("font_selected_color", "TabContainer", TEXT_STRONG)
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
	theme.set_icon("close", "Window", Icons.texture("close", TEXT_DIM, 16))
	theme.set_icon("close_pressed", "Window", Icons.texture("close", TEXT, 16))
	# Pop-ups (the colour picker, drop-downs) are solid: something see-through over a
	# panel is hard to read, and in a headset it is impossible.
	theme.set_stylebox("panel", "PopupPanel", _box(BG_POPUP, 6, 1, BORDER, 8))
	return theme


## The look of a button. On the desktop a plain rounded box. In a headset, where a
## button stands off its panel as a key, a box with a dark outline like Quill's, and
## a button that is down is filled with a stronger shade of the accent.
static func _key_box(color: Color, down := false, disabled := false) -> StyleBoxFlat:
	if not headset:
		return _box(color)
	var face := PRESSED.lerp(ACCENT, 0.35) if down else color
	var outline := Color(TEXT, 0.35 if disabled else 0.85)
	return _box(face, 6, 2, outline, 6)


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
	line.color = LINE
	line.vertical = vertical
	line.thickness = 1
	return line


## The box of a check box: an outline, filled with the accent colour and ticked when checked.
static func _check_icon(checked: bool, outline: Color, pixels := 18) -> Texture2D:
	var inside := ""
	if checked:
		inside = '<rect x="2" y="2" width="14" height="14" rx="3" fill="#%s"/><path d="M5.500 9.200l2.500 2.500 4.500-5" fill="none" stroke="#ffffff" stroke-width="1.800" stroke-linecap="round" stroke-linejoin="round"/>' % ACCENT.to_html(false)
	else:
		inside = '<rect x="2.500" y="2.500" width="13" height="13" rx="3" fill="#%s" stroke="#%s" stroke-width="1.200"/>' % [BG_SUNKEN.to_html(false), outline.to_html(false)]
	var svg := '<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d" viewBox="0 0 18 18">%s</svg>' % [pixels, pixels, inside]
	return DPITexture.create_from_string(svg)


## The ring of a choice among several (an entry of a drop-down), with the accent
## colour in it when it is the one chosen. 16 pixels, the size of the engine's own.
static func _radio_icon(checked: bool, outline: Color) -> Texture2D:
	var inside := '<circle cx="9" cy="9" r="6.500" fill="#%s" stroke="#%s" stroke-width="1.200"/>' % [BG_SUNKEN.to_html(false), outline.to_html(false)]
	if checked:
		inside = '<circle cx="9" cy="9" r="7" fill="#%s"/><circle cx="9" cy="9" r="3" fill="#ffffff"/>' % ACCENT.to_html(false)
	var svg := '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 18 18">%s</svg>' % inside
	return DPITexture.create_from_string(svg)


## A small arrowhead pointing down (`open`), or right (left when `mirrored`): the fold
## arrows of lists and the arrow of a menu entry that opens another.
static func _chevron(open: bool, color: Color, mirrored := false) -> Texture2D:
	var path := "M3.500 6 7 9.500 10.500 6" if open else ("M8 3.500 4.500 7 8 10.500" if mirrored else "M6 3.500 9.500 7 6 10.500")
	var svg := '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 14 14"><path d="%s" fill="none" stroke="#%s" stroke-width="1.600" stroke-linecap="round" stroke-linejoin="round"/></svg>' % [path, color.to_html(false)]
	return DPITexture.create_from_string(svg)


static func _knob(color: Color) -> Texture2D:
	var svg := '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 14 14"><circle cx="7" cy="7" r="6" fill="#%s" stroke="#%s" stroke-width="1"/></svg>' % [color.to_html(false), BORDER.to_html(false)]
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


## A key: a square button showing an icon, the building block of the Quill-style
## panels. `size` is in design pixels (see `u`); `toggle` makes it stay down when
## pressed. Its icon is drawn white and tinted by the theme for each state.
static func key(icon: String, tooltip: String, on_pressed: Callable, toggle := false, size := KEY) -> Button:
	var node := Button.new()
	node.theme_type_variation = "Key"
	node.icon = Icons.texture(icon, Color.WHITE, u(size * 0.58))
	node.tooltip_text = tooltip
	node.toggle_mode = toggle
	node.custom_minimum_size = Vector2(u(size), u(size))
	node.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	node.vertical_icon_alignment = VERTICAL_ALIGNMENT_CENTER
	node.focus_mode = Control.FOCUS_NONE
	if on_pressed.is_valid():
		node.pressed.connect(on_pressed)
	return node


## Gives `node` (a key) the icon `icon`, drawn the way `key` draws them.
static func set_key_icon(node: Button, icon: String, size := KEY) -> void:
	node.icon = Icons.texture(icon, Color.WHITE, u(size * 0.58))


## A key with a word on it, and an icon before the word when `icon` is given. As tall
## as a key; a word too long for it is cut short, with the whole of it in the tooltip.
static func text_key(text: String, tooltip: String, on_pressed: Callable, icon := "", toggle := false) -> Button:
	var node := Button.new()
	node.theme_type_variation = "TextKey"
	node.text = text
	node.tooltip_text = tooltip if tooltip != "" else text
	node.toggle_mode = toggle
	node.custom_minimum_size = Vector2(u(KEY), u(KEY))
	node.clip_text = true
	node.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	node.focus_mode = Control.FOCUS_NONE
	if icon != "":
		node.icon = Icons.texture(icon, Color.WHITE, u(KEY * 0.5))
	if on_pressed.is_valid():
		node.pressed.connect(on_pressed)
	return node


## The word or two over a group of keys.
static func caption(text: String) -> Label:
	var node := label(text, true)
	node.add_theme_font_size_override("font_size", text_size("caption"))
	return node


## A panel's heading inside it, such as the name of the tool in use.
static func title(text: String) -> Label:
	var node := label(text)
	node.add_theme_font_size_override("font_size", text_size("title"))
	return node


## A row of keys, `SPACE` apart.
static func key_row(children: Array = []) -> HBoxContainer:
	var node := row(children)
	node.add_theme_constant_override("separation", u(SPACE))
	return node


## A column of groups, `SPACE` apart.
static func key_column(children: Array = []) -> VBoxContainer:
	var node := column(children)
	node.add_theme_constant_override("separation", u(SPACE))
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
	if margin >= 1 and margin <= 3:
		# A kind of button the theme defines, so that it follows a change of scheme.
		button.theme_type_variation = "NarrowButton%d" % margin
		return
	var looks := {
		"normal": _key_box(BG_RAISED),
		"hover": _key_box(BG_HOVER),
		"pressed": _key_box(PRESSED, true),
		"hover_pressed": _key_box(PRESSED_HOVER, true),
		"disabled": _key_box(BG_DISABLED, false, true),
	}
	for state: String in looks:
		var box: StyleBoxFlat = looks[state]
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
