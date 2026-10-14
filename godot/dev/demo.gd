class_name LimnerDemo
## Paints a sample sheet through the same input path a controller uses: every brush
## as a freehand stroke, a straight line and a single click. Used for screenshots
## (`-- --demo`) and as a quick visual check after changing brushes or tessellation.

const BRUSHES := ["capped_cylinder", "capped_ellipse", "rounded_ribbon", "cylinder", "ellipse", "ribbon", "cube"]
const COLORS := [Color("e4572e"), Color("f3a712"), Color("a8c686"), Color("669bbc"), Color("29335c"), Color("8d5a97"), Color("3a3335")]


## Presses the trigger at the first point, moves through the rest and lets go.
static func stroke(scene: LimnerScene, points: Array, pressures: Array = []) -> void:
	var facing := Quaternion.IDENTITY
	for i in points.size():
		var pressure: float = pressures[i] if i < pressures.size() else 1.0
		scene.input_hand(0, points[i], facing, pressure, 0.0, Vector2.ZERO, 0)
		scene.input_commit(1.0 / 90.0)
	scene.input_hand(0, points[-1], facing, 0.0, 0.0, Vector2.ZERO, 0)
	scene.input_commit(1.0 / 90.0)


static func run(editor: LimnerEditor) -> void:
	var scene := editor.scene
	scene.input_head(Vector3(0.0, 1.4, 0.0))
	for row in BRUSHES.size():
		var y := 1.75 - row * 0.1
		editor.run({"cmd": "set_color", "color": LimnerEditor.linear_rgb(COLORS[row])})
		editor.run({"cmd": "set_tool", "tool": "paint"})
		editor.set_tool_setting("paint", "brush", BRUSHES[row])
		editor.set_tool_setting("paint", "size", 0.012)
		# A wave, pressed harder toward its middle.
		var points := []
		var pressures := []
		for i in 61:
			var t := i / 60.0
			points.append(Vector3(-0.55 + t * 0.4, y + sin(t * TAU * 1.5) * 0.02, -1.5))
			pressures.append(0.25 + 0.75 * sin(t * PI))
		stroke(scene, points, pressures)
		# A single click.
		stroke(scene, [Vector3(-0.08, y, -1.5)])
		# A straight line that thickens along its length.
		editor.run({"cmd": "set_tool", "tool": "line"})
		editor.set_tool_setting("line", "brush", BRUSHES[row])
		editor.set_tool_setting("line", "size", 0.004)
		scene.input_hand(0, Vector3(0.0, y - 0.02, -1.5), Quaternion.IDENTITY, 1.0, 0.0, Vector2.ZERO, 0)
		scene.input_commit(1.0 / 90.0)
		editor.set_tool_setting("line", "size", 0.014)
		stroke(scene, [Vector3(0.3, y + 0.02, -1.5)])
		# A see-through stroke crossing the row.
		editor.run({"cmd": "set_tool", "tool": "paint"})
		editor.set_tool_setting("paint", "opacity", 0.45)
		stroke(scene, [Vector3(0.36, y - 0.03, -1.5), Vector3(0.45, y + 0.03, -1.5), Vector3(0.54, y - 0.03, -1.5)])
		editor.set_tool_setting("paint", "opacity", 1.0)
	editor.run({"cmd": "set_tool", "tool": "paint"})
	scene.input_hand_absent(0)
	scene.input_commit(1.0 / 90.0)
