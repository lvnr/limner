class_name ModeSwitch
## Changing between the desktop and the headset while the app runs: whether the
## headset can be offered, and the words the buttons that do it use.
##
## The change itself is made by `main.gd` (`switch_to`), which takes one interface
## down and builds the other around the same world and editor, so the document, its
## undo history and the selection carry over. A shell asks for it with its
## `switch_requested` signal; the buttons are in the desktop's top bar and View menu,
## and in the Settings panel of both interfaces.

const TO_HEADSET := "Continue in the headset"
const TO_DESKTOP := "Continue on the desktop"
## Why the headset is not offered, when OpenXR is not running in this process.
const NO_HEADSET := "Start Limner without --xr-mode off, with the headset's software running, to use a headset."
## Why the headset is not offered, once its software has ended the session.
const HEADSET_ENDED := "The headset's software ended its session. Start Limner again to use the headset."

## Lets the buttons know that the headset became available or stopped being so.
class Events extends RefCounted:
	signal changed


static var events := Events.new()
## True once the headset's own software has ended the session (its menu's Quit,
## while the desktop was in use). OpenXR cannot be started again in the same run.
static var headset_ended := false


## Which headset a switch from the desktop would use: "headset" for a real one,
## "sim" for the simulated rig, or "" when there is none to use.
static func headset_target() -> String:
	return target_for(OS.get_name(), OS.get_cmdline_user_args(), _openxr_ready(), headset_ended)


## The rule behind `headset_target`, for the given system, command-line options,
## OpenXR state and ended session. `--sim` asks for the simulated rig; a Mac has no
## OpenXR and gets it too, to try the headset interface with the mouse.
static func target_for(system: String, args: PackedStringArray, openxr_ready: bool, ended: bool) -> String:
	if args.has("--sim"):
		return "sim"
	if ended:
		return ""
	if openxr_ready:
		return "headset"
	if system == "macOS":
		return "sim"
	return ""


## Why the headset cannot be used now, in words for the user; empty when it can.
static func unavailable_reason() -> String:
	if headset_target() != "":
		return ""
	return HEADSET_ENDED if headset_ended else NO_HEADSET


## What a desktop button that changes to the headset says when pointed at.
static func headset_tooltip() -> String:
	match headset_target():
		"headset":
			return "%s: put it on and carry on with the same work there" % TO_HEADSET
		"sim":
			return "%s: the simulated one, worked with the mouse and keyboard (there is no headset here)" % TO_HEADSET
	return unavailable_reason()


## The headset's software ended its session while it was not in use. From now on the
## headset is not offered.
static func mark_headset_ended() -> void:
	if headset_ended:
		return
	headset_ended = true
	events.changed.emit()


## True when OpenXR is running, or has found a headset system it can start on. It
## does not start anything: when the desktop asks, the headset's software stays as
## it is until the user actually changes to it.
static func _openxr_ready() -> bool:
	var openxr := OpenXRRig.find_interface()
	if openxr == null:
		return false
	return openxr.is_initialized() or not openxr.get_system_info().is_empty()
