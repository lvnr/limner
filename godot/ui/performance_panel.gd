class_name PerformancePanel
extends VBoxContainer
## The Performance panel: how heavy the scene is, held against what the kind of
## device it is meant for can draw, and the one tool that makes it lighter.
##
## The limits are rules of thumb for a comfortable frame rate, not measurements of
## any one device; they are there so that a scene does not grow past what a headset
## can show without anyone noticing until it stutters.

## What each kind of device can take in one frame. Triangles are counted as drawn
## (both eyes see the same ones); draw calls are what the engine reports.
const TARGETS := {
	"standalone": {"title": "Standalone", "tooltip": "A headset that runs on its own, such as a Quest", "triangles": 1_000_000, "draw_calls": 300, "fps": 72},
	"pc_headset": {"title": "PC headset", "tooltip": "A headset driven by a computer", "triangles": 5_000_000, "draw_calls": 1500, "fps": 90},
	"desktop": {"title": "Desktop", "tooltip": "A monitor", "triangles": 15_000_000, "draw_calls": 5000, "fps": 60},
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
var _counts: Label
var _memory: Label
var _verdict: Label
var _since := INTERVAL


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
	add_child(_frame)
	add_child(_triangles)
	add_child(_draw_calls)
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


func _process(delta: float) -> void:
	if not is_visible_in_tree():
		return
	_since += delta
	if _since >= INTERVAL:
		_since = 0.0
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
		"frame_ms": 1000.0 / maxf(Performance.get_monitor(Performance.TIME_FPS), 1.0),
		"triangles": int(stats.triangles),
		# Of the scene alone: the panels are drawn too, but are not what is being judged.
		"draw_calls": get_tree().root.get_render_info(Viewport.RENDER_INFO_TYPE_VISIBLE, Viewport.RENDER_INFO_DRAW_CALLS_IN_FRAME) if is_inside_tree() else 0,
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
	_frame.show_reading("%.1f ms of %.1f" % [now.frame_ms, budget_ms], now.frame_ms, budget_ms)
	_triangles.show_reading("%s of %s" % [_count(now.triangles), _count(limits.triangles)], now.triangles, limits.triangles)
	_draw_calls.show_reading("%s of %s" % [_count(now.draw_calls), _count(limits.draw_calls)], now.draw_calls, limits.draw_calls)
	_counts.text = "%s strokes with %s points in %s layers, %s of them showing now." % [_count(now.strokes), _count(now.points), _count(now.layers), _count(now.layers_shown)]
	_memory.text = "Memory: %s in use, %s on the graphics card." % [String.humanize_size(now.memory), String.humanize_size(now.video_memory)]
	_verdict.text = verdict()


## One sentence on how the scene stands against the target.
func verdict() -> String:
	var worst := maxi(_triangles.level(), _draw_calls.level())
	var device: String = TARGETS[target].title.to_lower()
	match worst:
		2:
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
