class_name TimelineScale
extends RefCounted
## How time is laid out along the tracks of the timeline: the zoom, what is scrolled
## into view, and the document's frame grid.
##
## Times are flicks on the root timeline, positions are pixels inside the view.

const FLICKS := LimnerEditor.FLICKS_PER_SECOND
## Pixels per second: an hour across a small panel, to one frame across a hand.
const MIN_ZOOM := 0.15
const MAX_ZOOM := 6000.0

## Pixels per second.
var zoom := 96.0
## The time, in seconds, drawn at `left`.
var start := 0.0
## The x at which the time `start` is drawn.
var left := 0.0
## The document's frame rate, as a fraction.
var rate_num := 24
var rate_den := 1


func x_of(time: int) -> float:
	return left + (float(time) / FLICKS - start) * zoom


func time_at(x: float) -> int:
	return roundi(((x - left) / zoom + start) * FLICKS)


## How long `pixels` are, in flicks.
func duration_of(pixels: float) -> int:
	return roundi(pixels / zoom * FLICKS)


## Width of one document frame in pixels.
func frame_width() -> float:
	return zoom * rate_den / rate_num


## Changes the zoom by `factor`, keeping the time under `x` where it is.
func zoom_about(x: float, factor: float) -> void:
	var anchor := (x - left) / zoom + start
	zoom = clampf(zoom * factor, MIN_ZOOM, MAX_ZOOM)
	start = maxf(anchor - (x - left) / zoom, 0.0)


## Shows the span from zero to `end` across `width` pixels.
func fit(end: int, width: float) -> void:
	var seconds := maxf(float(end) / FLICKS, 0.5)
	zoom = clampf(maxf(width, 40.0) / seconds, MIN_ZOOM, MAX_ZOOM)
	start = 0.0


# ---- the frame grid ----

## Index of the document frame that contains `time`.
func frame_at(time: int) -> int:
	var n := time * rate_num
	var d := FLICKS * rate_den
	@warning_ignore("integer_division")
	var frame := n / d
	if n < 0 and n % d != 0:
		frame -= 1
	return frame


## The first instant of document frame `frame`.
func frame_start(frame: int) -> int:
	var n := frame * FLICKS * rate_den
	@warning_ignore("integer_division")
	var time := n / rate_num
	if n > 0 and n % rate_num != 0:
		time += 1
	return time


## The frame boundary nearest to `time`.
func snap(time: int) -> int:
	var frame := frame_at(time)
	var lower := frame_start(frame)
	var upper := frame_start(frame + 1)
	return lower if time - lower <= upper - time else upper


## `time` moved back to the start of the frame it is in.
func snap_floor(time: int) -> int:
	return frame_start(frame_at(time))
