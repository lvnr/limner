class_name XRHandState
extends RefCounted
## What one controller reports at one moment, apart from where it is: how far its
## trigger and grip are pressed, where its stick is, and which buttons are down.
##
## A real controller and a simulated one both fill in one of these, and nothing after
## that point can tell them apart.

## False when the controller is switched off, out of sight of the tracking, or absent.
var tracked := false
## Index trigger, 0 to 1.
var trigger := 0.0
## Grip (the squeeze under the middle finger), 0 to 1.
var grip := 0.0
## Thumbstick: x to the right, y forward.
var stick := Vector2.ZERO
## Lower face button: A on a right Touch controller, X on a left one.
var lower := false
## Upper face button: B on a right Touch controller, Y on a left one.
var upper := false
## The stick pushed in.
var stick_click := false
## The small menu button, where the controller has one.
var menu := false


## The face buttons and the stick click packed the way `LimnerScene.input_hand` takes them.
func buttons() -> int:
	return (1 if lower else 0) | (2 if upper else 0) | (4 if stick_click else 0)


## Puts everything back to "nothing pressed", keeping whether the controller is tracked.
func release_all() -> void:
	trigger = 0.0
	grip = 0.0
	stick = Vector2.ZERO
	lower = false
	upper = false
	stick_click = false
	menu = false
