class_name CameraView
extends Node
## Puts the view of the window behind a camera layer.
##
## The window's own camera never moves: navigation moves the artwork. Looking
## through a camera layer therefore means moving the artwork until that layer sits
## where the head is, facing the way the head faces, and giving the head the
## layer's lens. While a camera is followed this is repeated every frame, so the
## view stays with an animated camera, and navigating by hand has no lasting effect.
##
## In a headset the head cannot be given another place or lens. There,
## `look_through` stands the viewer where the camera is, and nothing is followed.

## The view started or stopped following a camera.
signal changed

var editor: LimnerEditor
## The camera layer the view is kept behind, or -1.
var following := -1
## True while a range is being played in a loop for a preview.
var previewing := false

## The lens the window had before it was given a camera's, in degrees; 0 when it has its own.
var _own_lens := 0.0
var _preview_start := 0.0
var _preview_end := 0.0


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "CameraView"


## The camera view that goes with `editor`, made the first time it is asked for.
static func of(limner_editor: LimnerEditor) -> CameraView:
	var view: CameraView = limner_editor.get_node_or_null("CameraView")
	if view == null:
		view = CameraView.new(limner_editor)
		limner_editor.add_child(view)
	return view


## Moves the view behind camera layer `layer`, once. Navigation is free afterwards;
## the lens stays the camera's until `stop`.
func look_through(layer: int) -> void:
	var head := get_viewport().get_camera_3d()
	if head == null:
		return
	if get_viewport().use_xr:
		editor.scene.view_go_to_viewpoint(layer)
		return
	var view := LimnerCapture.of(editor).layer_view(layer)
	var placed: Transform3D = view.transform
	var turn := (head.global_transform.basis * placed.basis.inverse()).get_rotation_quaternion()
	if turn.get_angle() > 1e-6:
		editor.scene.view_rotate(placed.origin, turn.get_axis(), turn.get_angle())
	editor.scene.view_pan(head.global_position - placed.origin)
	if _own_lens == 0.0:
		_own_lens = head.fov
	head.fov = view.fov


## Keeps the view behind camera layer `layer` until `stop`.
func follow(layer: int) -> void:
	following = layer
	look_through(layer)
	changed.emit()


## Plays the timeline from `start` to `end` (seconds) over and over, seen through
## camera layer `layer`, or through the view as it is when `layer` is -1.
func preview(layer: int, start: float, end: float) -> void:
	previewing = true
	_preview_start = start
	_preview_end = end
	following = layer
	editor.run({"cmd": "seek", "time": LimnerEditor.to_flicks(start)})
	editor.run({"cmd": "play"})
	if layer >= 0:
		look_through(layer)
	changed.emit()


## Lets go of the camera: the view stays where it is and gets its own lens back.
## A preview stops playing.
func stop() -> void:
	if following < 0 and not previewing and _own_lens == 0.0:
		return
	if previewing:
		editor.run({"cmd": "pause"})
	previewing = false
	following = -1
	var head := get_viewport().get_camera_3d()
	if head != null and _own_lens != 0.0:
		head.fov = _own_lens
	_own_lens = 0.0
	changed.emit()


func _process(_delta: float) -> void:
	if previewing:
		var seconds: float = editor.status.seconds
		if seconds >= _preview_end or seconds < _preview_start or not editor.status.playing:
			if editor.status.playing:
				editor.run({"cmd": "seek", "time": LimnerEditor.to_flicks(_preview_start)})
			else:
				# Paused from elsewhere: the preview is over.
				stop()
				return
	if following < 0 or get_viewport().use_xr:
		return
	if editor.row(following).is_empty():
		stop()
		return
	look_through(following)
