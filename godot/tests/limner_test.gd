class_name LimnerTest
extends Node
## Base class for interface tests: a world, an editor and an empty document, plus
## small helpers for checking results.

var world: LimnerWorld
var editor: LimnerEditor
var failures: Array[String] = []


func setup() -> void:
	# Kit's look is the app's, kept between tests: each starts on the desktop's, dark.
	Kit.set_headset(false)
	Kit.use_scheme("dark")
	world = LimnerWorld.new()
	add_child(world)
	editor = LimnerEditor.new()
	add_child(editor)
	editor.setup(world.scene)
	editor.message.connect(func(text: String, is_error: bool) -> void:
		if is_error:
			messages.append(text)
	)
	await frames(2)


## Error messages the editor reported during the test.
var messages: Array[String] = []


## Lets `count` frames pass, so deferred calls run and change signals arrive.
func frames(count := 1) -> void:
	for i in count:
		await get_tree().process_frame


## Puts a panel on screen the way a dock would, 300 px wide.
func mount(panel: Control, size := Vector2(300, 700)) -> Control:
	var host := PanelContainer.new()
	host.theme = Kit.make_theme()
	host.size = size
	add_child(host)
	host.add_child(panel)
	return panel


func check(condition: bool, what: String) -> void:
	if not condition:
		failures.append(what)


func check_eq(actual: Variant, expected: Variant, what: String) -> void:
	if actual != expected:
		failures.append("%s: got %s, expected %s" % [what, actual, expected])


func check_near(actual: float, expected: float, what: String, tolerance := 1e-4) -> void:
	if absf(actual - expected) > tolerance:
		failures.append("%s: got %s, expected %s" % [what, actual, expected])


## Runs a command and records a failure if it does not work.
func run(command: Dictionary) -> Variant:
	var reply: Dictionary = world.scene.exec(command)
	if not reply.ok:
		failures.append("%s failed: %s" % [command, reply.error])
		return null
	return reply.result


## The row of the first layer with this name.
func row_named(layer_name: String) -> Dictionary:
	for row: Dictionary in editor.get_rows():
		if row.name == layer_name:
			return row
	failures.append("there is no layer named %s" % layer_name)
	return {}
