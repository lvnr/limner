class_name LimnerPanel
extends VBoxContainer
## The base of the Quill-style panels: a panel of one fixed size, whatever it shows.
##
## A panel declares its size in design pixels (`DESIGN_SIZE`, see `Kit.u`) and is
## built with `Kit.u` sizes. What changes with the tool, the layer's kind or the
## section chosen is swapped in place inside that size; nothing scrolls, nothing opens
## a list over the panel. `fits()` says whether the content keeps to the size, and
## the tests ask it for every state a panel can be in (see `LimnerTest.check_fits`).

## The panel's size in design pixels. Subclasses set their own in `_init`.
var design_size := Vector2(500, 560)

## Controls that scroll or open a list over the panel, which no panel may hold.
const FORBIDDEN := ["ScrollContainer", "OptionButton", "ItemList", "Tree"]


func _init(size_in_design_pixels := Vector2(500, 560)) -> void:
	design_size = size_in_design_pixels
	custom_minimum_size = pixel_size()
	add_theme_constant_override("separation", Kit.u(Kit.SPACE))


## The panel's size in pixels of the interface in use.
func pixel_size() -> Vector2:
	return Vector2(Kit.u(design_size.x), Kit.u(design_size.y))


## Whether the content needs no more room than the panel's size, and holds nothing
## that scrolls or opens a list. An empty list means it fits; otherwise each entry
## says what does not.
func fits() -> Array[String]:
	var problems: Array[String] = []
	var needs := get_minimum_size()
	var has := pixel_size()
	if needs.x > has.x + 0.5 or needs.y > has.y + 0.5:
		problems.append("%s needs %s but has %s" % [name, needs, has])
	for node: Node in find_children("*", "Control", true, false):
		for kind: String in FORBIDDEN:
			if node.is_class(kind) and (node as Control).is_visible_in_tree():
				problems.append("%s holds a %s (%s)" % [name, kind, get_path_to(node)])
	return problems
