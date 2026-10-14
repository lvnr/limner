class_name PerformancePanel
extends VBoxContainer
## The Performance panel: how heavy the scene is, held against what the kind of
## device it is meant for can draw, and the one tool that makes it lighter.
##
## The limits are rules of thumb for a comfortable frame rate, not measurements of
## any one device; they are there so that a scene does not grow past what a headset
## can show without anyone noticing until it stutters.

## What each kind of device can take in one frame. Triangles are counted as drawn
## (both eyes see the same ones); draw calls are what the engine reports. Sounds are
## sound layers playing at once: a standalone headset mixes them on a phone's
## processor, so it gets fewer. Like the rest, a rule of thumb.
const TARGETS := {
	"standalone": {"title": "Standalone", "tooltip": "A headset that runs on its own, such as a Quest", "triangles": 1_000_000, "draw_calls": 300, "sounds": 16, "fps": 72},
	"pc_headset": {"title": "PC headset", "tooltip": "A headset driven by a computer", "triangles": 5_000_000, "draw_calls": 1500, "sounds": 32, "fps": 90},
	"desktop": {"title": "Desktop", "tooltip": "A monitor", "triangles": 15_000_000, "draw_calls": 5000, "sounds": 32, "fps": 60},
}
## Seconds between readings.
const INTERVAL := 0.5

var editor: LimnerEditor
## The kind of device the scene is held against.
var target := "desktop"

var _choice: ChoiceRow
var _frame: PerformanceMeter
var _triangles: PerformanceMeter
var _draw_calls: PerformanceMeter
var _sounds: PerformanceMeter
var _reset: Button
var _counts: Label
var _memory: Label
var _verdict: Label
var _since := 0.0
## Frame time measured here: the frames and seconds of the interval under way, and
## the average of the last whole one (zero until there is one). The engine's own
## frames-per-second reads 1 for its first second, which would leave a peak of a
## whole second behind.
var _frames := 0
var _elapsed := 0.0
var _frame_ms := 0.0
## The first interval after the panel is shown holds whatever made it show (loading,
## building the interface), so it is not measured.
var _warming := true


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Performance"
	add_theme_constant_override("separation", Kit.GAP)

	add_child(Kit.heading("Meant for"))
	_choice = ChoiceRow.new()
	for key: String in TARGETS:
		_choice.add_text(key, TARGETS[key].title, TARGETS[key].tooltip)
	_choice.chosen.connect(func(key: String) -> void:
		target = key
		_choice.set_value(key)
		_read()
	)
	add_child(_choice)

	add_child(Kit.heading("This frame"))
	_frame = PerformanceMeter.new("Frame time", "How long a frame takes, against the time there is at the device's refresh rate")
	_triangles = PerformanceMeter.new("Triangles drawn", "Triangles of everything on screen now, against what the device draws comfortably")
	_draw_calls = PerformanceMeter.new("Draw calls", "Separate pieces the graphics card is asked to draw. Many small layers cost more than a few large ones")
	_sounds = PerformanceMeter.new("Sounds playing", "Sound layers playing now, against how many the device mixes comfortably at once")
	add_child(_frame)
	add_child(_triangles)
	add_child(_draw_calls)
	add_child(_sounds)
	_reset = Kit.button("Reset peaks", "Start the peaks again from now. The mark on each bar and the line under it show the highest reading since the panel was opened or the peaks were reset", reset_peaks)
	_reset.size_flags_horizontal = Control.SIZE_SHRINK_END
	add_child(_reset)
	_verdict = Kit.label("", true)
	_verdict.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	add_child(_verdict)

	add_child(Kit.heading("The document"))
	_counts = Kit.label("", true)
	_counts.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	add_child(_counts)
	_memory = Kit.label("", true)
	_memory.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	add_child(_memory)

	add_child(Kit.heading("Make it lighter"))
	var layer := Kit.button("Optimize this layer", "Removes the points of the paint layer's strokes that do not change their shape, as far as the Optimize tool's threshold allows", func() -> void: _optimize("layer"), "optimize")
	layer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	add_child(layer)
	var all := Kit.button("Optimize every layer", "The same for every unlocked paint layer. One undo step", func() -> void: _optimize("all"), "optimize")
	all.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	add_child(all)
	var hint := Kit.label("Optimizing cannot be told from the original at a distance, but leaves fewer points to recolour or bend later. Do it last, and lock the layers that must stay as they are.", true)
	hint.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	hint.add_theme_font_size_override("font_size", 12)
	add_child(hint)


func _ready() -> void:
	# A headset is what the scene is for when one is in use.
	if OS.has_feature("android"):
		target = "standalone"
	elif get_tree().root.use_xr:
		target = "pc_headset"
	_choice.set_value(target)
	_read()


func _notification(what: int) -> void:
	# Peaks are kept from when the panel is opened.
	if what == NOTIFICATION_VISIBILITY_CHANGED and is_visible_in_tree() and _frame != null:
		_frames = 0
		_elapsed = 0.0
		_since = 0.0
		_warming = true
		reset_peaks()


func _process(delta: float) -> void:
	if not is_visible_in_tree():
		return
	# A sound heard while scrubbing lasts a fraction of a second, less than the time
	# between readings: the peaks of what the scene reports are looked at every frame.
	var stats := editor.scene.stats()
	_sounds.peak = maxf(_sounds.peak, float(stats.get("sounds_playing", 0)))
	_triangles.peak = maxf(_triangles.peak, float(stats.get("triangles", 0)))
	_frames += 1
	_elapsed += delta
	_since += delta
	if _since >= INTERVAL:
		_since = 0.0
		if not _warming and _frames > 0:
			_frame_ms = _elapsed / _frames * 1000.0
		_warming = false
		_frames = 0
		_elapsed = 0.0
		_read()


func _optimize(scope: String) -> void:
	var before := int(editor.scene.info().points)
	if editor.run({"cmd": "optimize", "scope": scope}) == null:
		return
	var after := int(editor.scene.info().points)
	editor.message.emit("Optimized: %s points fewer" % _count(before - after), false)
	_read()


## What the panel shows, as numbers: for tests and for anything else that wants them.
func readings() -> Dictionary:
	var stats := editor.scene.stats()
	var info := editor.scene.info()
	return {
		# The average over the last half second; zero until one has been measured.
		"frame_ms": _frame_ms,
		"triangles": int(stats.triangles),
		# Of the scene alone: the panels are drawn too, but are not what is being judged.
		"draw_calls": get_tree().root.get_render_info(Viewport.RENDER_INFO_TYPE_VISIBLE, Viewport.RENDER_INFO_DRAW_CALLS_IN_FRAME) if is_inside_tree() else 0,
		"sounds": int(stats.get("sounds_playing", 0)),
		"layers_shown": int(stats.visible_layers),
		"layers": int(info.layers),
		"strokes": int(info.strokes),
		"points": int(info.points),
		"video_memory": int(Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED)),
		"memory": int(Performance.get_monitor(Performance.MEMORY_STATIC)),
	}


func _read() -> void:
	if _frame == null:
		return
	var limits: Dictionary = TARGETS[target]
	var now := readings()
	var budget_ms := 1000.0 / float(limits.fps)
	var frame_text := "%.1f ms of %.1f" % [now.frame_ms, budget_ms] if now.frame_ms > 0.0 else "measuring"
	_frame.show_reading(frame_text, now.frame_ms, budget_ms)
	_triangles.show_reading("%s of %s" % [_count(now.triangles), _count(limits.triangles)], now.triangles, limits.triangles)
	_draw_calls.show_reading("%s of %s" % [_count(now.draw_calls), _count(limits.draw_calls)], now.draw_calls, limits.draw_calls)
	_sounds.show_reading("%d of %d" % [now.sounds, limits.sounds], now.sounds, limits.sounds)
	_frame.show_peak("peak %.1f ms" % _frame.peak if _frame.peak > 0.0 else "")
	_triangles.show_peak("peak %s" % _count(roundi(_triangles.peak)))
	_draw_calls.show_peak("peak %s" % _count(roundi(_draw_calls.peak)))
	_sounds.show_peak("peak %d" % roundi(_sounds.peak))
	_counts.text = "%s strokes with %s points in %s layers, %s of them showing now." % [_count(now.strokes), _count(now.points), _count(now.layers), _count(now.layers_shown)]
	_memory.text = "Memory: %s in use, %s on the graphics card." % [String.humanize_size(now.memory), String.humanize_size(now.video_memory)]
	_verdict.text = verdict()


## Starts every meter's peak again from the next reading.
func reset_peaks() -> void:
	for meter: PerformanceMeter in meters().values():
		meter.reset_peak()
	_read()


## The meters, by what they measure.
func meters() -> Dictionary:
	return {"frame": _frame, "triangles": _triangles, "draw_calls": _draw_calls, "sounds": _sounds}


## One sentence on how the scene stands against the target.
func verdict() -> String:
	var worst := maxi(maxi(_triangles.level(), _draw_calls.level()), _sounds.level())
	var device: String = TARGETS[target].title.to_lower()
	match worst:
		2:
			if _triangles.level() < 2 and _draw_calls.level() < 2:
				return "More sounds playing at once than a %s mixes comfortably. Shorten the clips of sounds that overlap, or mix some of them into one." % device
			var what := "triangles" if _triangles.level() == 2 else "separate pieces"
			return "More %s on screen than a %s draws comfortably. Optimize, hide what is not needed here, or merge small layers." % [what, device]
		1:
			return "Close to what a %s draws comfortably." % device
	return "Well within what a %s draws comfortably." % device


## A count as people write it: 950, 12.4 k, 1.2 M.
static func _count(value: int) -> String:
	var size := absi(value)
	if size >= 1_000_000:
		return "%s M" % ("%.1f" % (value / 1_000_000.0)).trim_suffix(".0")
	if size >= 10_000:
		return "%s k" % ("%.1f" % (value / 1000.0)).trim_suffix(".0")
	return str(value)
