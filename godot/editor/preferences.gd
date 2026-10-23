class_name Preferences
extends RefCounted
## Settings that belong to the person and the machine, not to a document: which hand
## draws, whether controllers vibrate, how loud the interface is.
##
## There is one set for the whole app, reached with `Preferences.shared()`. It is
## kept in `user://settings.json`. A missing or damaged file, or a value of the wrong
## kind in it, falls back to the default without complaint, and settings written by a
## newer version that this one does not know are kept as they are.

## A setting changed, by the user or by loading. `key` is its name in `DEFAULTS`.
signal changed(key: String)

## Every setting with the value it has until the user changes it.
##
## - `dominant_hand`: the hand that draws, "right" or "left".
## - `haptics`: whether controllers vibrate.
## - `hand_tracking`: whether a bare hand that the headset sees, with no controller
##   in it, works as one (pinching for the trigger). Off: a controller put down
##   leaves its hand doing nothing.
## - `ui_volume`: loudness of interface sounds, 0 (silent) to 1.
## - `show_button_map_at_start`: whether the sheet of controls is shown when a headset session starts.
## - `show_gimbal_by_default`: whether a new session starts with the gimbal showing.
## - `theme`: the desktop interface's colour scheme, "dark" or "light". Kit follows it
##   at once (see `Kit.follow_preferences`).
## - `headset_theme`: the same for the panels in a headset, light unless changed, as
##   Quill's are.
## - `autosave_minutes`: minutes between automatic saves, 0 for never.
## - `palette`: the swatches a new document starts with, as linear `[r, g, b]`
##   lists: the ones last set in the Colour panel. Empty for the built-in ones.
## - `text_font`: the family of the font text is written in, empty for the built-in one.
## - `recent_fonts`: the families last chosen for text, the latest first, at most
##   `RECENT_FONTS` of them.
const DEFAULTS := {
	"dominant_hand": "right",
	"haptics": true,
	"hand_tracking": false,
	"ui_volume": 0.5,
	"show_button_map_at_start": true,
	"show_gimbal_by_default": false,
	"theme": "dark",
	"headset_theme": "light",
	"autosave_minutes": 5,
	"palette": [],
	"text_font": "",
	"recent_fonts": [],
}
## The values a setting may take, for those that are a choice.
const CHOICES := {
	"dominant_hand": ["right", "left"],
	"theme": ["dark", "light"],
	"headset_theme": ["dark", "light"],
}
## The lowest and highest value, for those that are a number.
const RANGES := {
	"ui_volume": [0.0, 1.0],
	"autosave_minutes": [0, 120],
}

## The most font families `recent_fonts` keeps.
const RECENT_FONTS := 6

## Where the settings are kept unless a test says otherwise.
const DEFAULT_PATH := "user://settings.json"

## Where the settings are kept. Tests point this at a file of their own with `use_file`.
static var storage_path := DEFAULT_PATH
static var _shared: Preferences

var _values := {}
## Entries of the file that this version has no setting for.
var _unknown := {}


## The app's one set of preferences, read from disk the first time it is asked for.
static func shared() -> Preferences:
	if _shared == null:
		_shared = Preferences.new()
		_shared.load_from_disk()
	return _shared


## Keeps the settings in another file from now on and reads what is there. The shared
## set stays the same object, so whoever listens to it keeps hearing changes.
static func use_file(path: String) -> void:
	storage_path = path
	shared().load_from_disk()


func _init() -> void:
	_values = DEFAULTS.duplicate()


## The value of a setting, or `null` for a name that is not one.
func value(key: String) -> Variant:
	return _values.get(key)


## A setting that is a word, such as `dominant_hand`.
func text(key: String) -> String:
	return str(_values.get(key, ""))


## A setting that is on or off.
func flag(key: String) -> bool:
	return bool(_values.get(key, false))


## A setting that is a number.
func number(key: String) -> float:
	return float(_values.get(key, 0.0))


## Changes a setting and saves. A value of the wrong kind or outside what the setting
## allows is brought in line first; one that cannot be is ignored.
func set_value(key: String, new_value: Variant) -> void:
	if not DEFAULTS.has(key):
		push_warning("There is no preference named %s" % key)
		return
	var cleaned: Variant = _clean(key, new_value)
	if cleaned == null or cleaned == _values[key]:
		return
	_values[key] = cleaned
	save()
	changed.emit(key)


## Reads the file again, reporting every setting that ends up different.
func load_from_disk() -> void:
	var loaded := DEFAULTS.duplicate()
	_unknown = {}
	var stored := _read_file()
	for key: String in stored:
		if not DEFAULTS.has(key):
			_unknown[key] = stored[key]
			continue
		var cleaned: Variant = _clean(key, stored[key])
		if cleaned != null:
			loaded[key] = cleaned
	for key: String in DEFAULTS:
		if loaded[key] != _values[key]:
			_values[key] = loaded[key]
			changed.emit(key)


## Writes the settings. The file is written beside the old one and then swapped in,
## so that a crash half-way cannot leave a damaged file.
func save() -> void:
	var contents := _unknown.duplicate()
	contents.merge(_values, true)
	var temporary := storage_path + ".tmp"
	var file := FileAccess.open(temporary, FileAccess.WRITE)
	if file == null:
		push_warning("Could not save preferences to %s: %s" % [storage_path, error_string(FileAccess.get_open_error())])
		return
	file.store_string(JSON.stringify(contents, "\t") + "\n")
	file.close()
	var error := DirAccess.rename_absolute(temporary, storage_path)
	if error != OK:
		push_warning("Could not save preferences to %s: %s" % [storage_path, error_string(error)])


## A list of colours as plain `[r, g, b]` lists of numbers, or `null` when `raw` is
## not one. At most 64 are kept.
func _clean_colors(raw: Variant) -> Variant:
	if not raw is Array:
		return null
	var colors: Array = []
	for entry: Variant in raw as Array:
		if not (entry is Array and (entry as Array).size() == 3):
			return null
		var color: Array = []
		for channel: Variant in entry as Array:
			if not (channel is float or channel is int) or not is_finite(float(channel)) or float(channel) < 0.0:
				return null
			color.append(float(channel))
		colors.append(color)
	return colors.slice(0, 64)


## A list of words, without empty or repeated ones, or `null` when `raw` is not a list
## of words. At most `RECENT_FONTS` are kept.
func _clean_words(raw: Variant) -> Variant:
	if not raw is Array:
		return null
	var words: Array = []
	for entry: Variant in raw as Array:
		if not entry is String:
			return null
		if not (entry as String).is_empty() and not words.has(entry):
			words.append(entry)
	return words.slice(0, RECENT_FONTS)


## Makes `family` the font text is written in and puts it first among the recent ones.
## The empty name is the built-in font, which is not counted as a recent one.
func choose_font(family: String) -> void:
	if not family.is_empty():
		var recent: Array = [family]
		recent.append_array(_values.get("recent_fonts", []))
		set_value("recent_fonts", recent)
	set_value("text_font", family)


## The swatches a new document starts with; empty for the built-in ones.
func palette() -> Array:
	return _values.get("palette", [])


## Sets the editor options that a preference gives the starting value of. Each shell
## calls this once when it starts.
func apply_at_start(editor: LimnerEditor) -> void:
	# The untouched document the app starts with takes the user's own swatches.
	if not palette().is_empty() and editor.path == "" and not editor.scene.status().dirty and int(editor.scene.info().strokes) == 0:
		editor.new_document()
	if flag("show_gimbal_by_default") != bool(editor.get_options().get("show_gimbal", false)):
		editor.set_option("show_gimbal", flag("show_gimbal_by_default"))


## The file's contents as a dictionary; empty when there is no file or it is not one.
func _read_file() -> Dictionary:
	if not FileAccess.file_exists(storage_path):
		return {}
	var reader := JSON.new()
	if reader.parse(FileAccess.get_file_as_string(storage_path)) != OK or not reader.data is Dictionary:
		return {}
	return reader.data


## `raw` as a valid value for `key`, or `null` when it cannot be one.
func _clean(key: String, raw: Variant) -> Variant:
	var default: Variant = DEFAULTS[key]
	if default is Array:
		return _clean_colors(raw) if key == "palette" else _clean_words(raw)
	if default is bool:
		return raw if raw is bool else null
	if default is String:
		var choices: Array = CHOICES.get(key, [])
		return raw if raw is String and (choices.is_empty() or choices.has(raw)) else null
	if not (raw is float or raw is int):
		return null
	var limits: Array = RANGES.get(key, [-INF, INF])
	if default is int:
		return clampi(roundi(float(raw)), int(limits[0]), int(limits[1]))
	return clampf(float(raw), float(limits[0]), float(limits[1]))
