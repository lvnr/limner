class_name Pager
extends HBoxContainer
## The page keys of a list that shows a page at a time instead of scrolling: a key
## for the page before, the place in the list ("11–20 of 34"), a key for the page
## after. The list itself is the owner's: it asks `first` and `per_page` which items
## to show, and redraws on `page_changed`.
##
## Pages start at multiples of `per_page`, so an item is always on the same page.

signal page_changed(first: int)

## How many items a page shows.
var per_page := 10
## How many items there are.
var count := 0
## The index of the first item on the page shown.
var first := 0

var _before: Button
var _after: Button
var _place: Label


## `vertical` gives the keys arrows up and down, for a list of rows; otherwise they
## point left and right.
func _init(items_per_page := 10, vertical := true) -> void:
	per_page = maxi(items_per_page, 1)
	add_theme_constant_override("separation", Kit.u(4))
	_before = Kit.key("up" if vertical else "previous", "Page before", step.bind(-1))
	_after = Kit.key("down" if vertical else "next", "Page after", step.bind(1))
	_place = Kit.caption("")
	_place.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_place.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	add_child(_before)
	add_child(_place)
	add_child(_after)
	_show()


## Sets how many items there are; the page shown stays, or moves back if the list
## got shorter than it.
func set_count(items: int) -> void:
	count = maxi(items, 0)
	_go(first)


## Changes how many items a page shows (the desktop fits it to the room it has).
func set_per_page(items: int) -> void:
	per_page = maxi(items, 1)
	_go(first)


## Shows the page that holds item `index`.
func show_item(index: int) -> void:
	if index >= 0:
		_go(index)


## Steps `by` pages (negative for earlier ones).
func step(by: int) -> void:
	_go(first + by * per_page)


## The number of pages, at least one.
func pages() -> int:
	return maxi(ceili(float(count) / per_page), 1)


func _go(index: int) -> void:
	var last_page := (pages() - 1) * per_page
	var page := clampi((index / per_page) * per_page, 0, last_page)
	var moved := page != first
	first = page
	_show()
	if moved:
		page_changed.emit(first)


func _show() -> void:
	_before.disabled = first <= 0
	_after.disabled = first + per_page >= count
	if count == 0:
		_place.text = ""
	elif count <= per_page:
		_place.text = "%d" % count
	else:
		_place.text = "%d–%d of %d" % [first + 1, mini(first + per_page, count), count]
