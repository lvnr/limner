class_name Sounds
extends Node
## The interface's sounds: a click for a button, ticks for undo and redo, a blip for
## picking a colour, a thud for deleting, a chime at a stop, and a quiet hiss while
## paint is being laid down.
##
## Every sound is worked out here from sines and noise when the app starts. There are
## no audio files, and nothing is taken from any other program. Loudness follows the
## `ui_volume` preference; at zero nothing is played at all.

## Samples per second of the sounds made here.
const MIX_RATE := 22050
## The sounds that `play` knows, which include every name the core sends.
const NAMES := ["click", "undo", "redo", "pick", "delete", "stop"]
## How loud the painting hiss is next to the other sounds, as a fraction.
const HISS_LEVEL := 0.22
## Length of the hiss before it repeats, in seconds.
const HISS_SECONDS := 1.0
## Length of the stretch at the end of the hiss that is blended into its start so
## that the repeat cannot be heard, in seconds.
const HISS_BLEND_SECONDS := 0.1
## Tools that lay down paint, and so hiss while they are applied.
const PAINTING_TOOLS := ["paint", "line"]

var _players := {}
var _hiss: AudioStreamPlayer
var _volume := 0.5


func _ready() -> void:
	_add("click", _tone(0.03, 1800.0, 1500.0, 90.0, 0.5, 0.15))
	_add("undo", _tone(0.07, 720.0, 430.0, 45.0, 0.5, 0.0))
	_add("redo", _tone(0.07, 430.0, 720.0, 45.0, 0.5, 0.0))
	_add("pick", _two_tones(0.1, 1175.0, 1568.0, 30.0, 0.45))
	_add("delete", _tone(0.16, 150.0, 55.0, 22.0, 0.8, 0.25))
	_add("stop", _chord(0.6, [880.0, 1320.0, 1760.0], 6.0, 0.4))
	_hiss = AudioStreamPlayer.new()
	_hiss.name = "Hiss"
	_hiss.stream = _hiss_loop()
	add_child(_hiss)
	var preferences := Preferences.shared()
	preferences.changed.connect(_on_preference_changed)
	_set_volume(preferences.number("ui_volume"))


## Plays a sound by name. A name that is not in `NAMES` is ignored, so that a newer
## core can send sounds this interface does not have yet.
func play(sound: String) -> void:
	if _volume <= 0.0 or not _players.has(sound):
		return
	var player: AudioStreamPlayer = _players[sound]
	player.play()


## Starts or stops the hiss of paint going down.
func set_painting(painting: bool) -> void:
	var wanted := painting and _volume > 0.0
	if wanted and not _hiss.playing:
		_hiss.play()
	elif not wanted and _hiss.playing:
		_hiss.stop()


## Keeps the painting hiss in step with what the drawing hand is doing. Call it once
## a frame, after the frame's input has been committed.
func follow_tool(scene: LimnerScene) -> void:
	var cursor := scene.tool_cursor()
	set_painting(bool(cursor.active) and PAINTING_TOOLS.has(cursor.tool))


## True while the painting hiss is sounding.
func is_painting() -> bool:
	return _hiss.playing


func _on_preference_changed(key: String) -> void:
	if key == "ui_volume":
		_set_volume(Preferences.shared().number("ui_volume"))


func _set_volume(volume: float) -> void:
	_volume = volume
	var level := linear_to_db(maxf(volume, 0.0001))
	for sound: String in _players:
		var player: AudioStreamPlayer = _players[sound]
		player.volume_db = level
	_hiss.volume_db = linear_to_db(maxf(volume * HISS_LEVEL, 0.0001))
	if volume <= 0.0:
		_hiss.stop()


func _add(sound: String, stream: AudioStreamWAV) -> void:
	var player := AudioStreamPlayer.new()
	player.name = sound.capitalize()
	player.stream = stream
	add_child(player)
	_players[sound] = player


# ---- making the sounds ----

## A short tone that slides from one pitch to another and dies away. `decay` is how
## fast it dies (larger is shorter), `level` its loudness from 0 to 1, and `noise`
## the share of it that is hiss rather than tone, which gives a click its edge.
func _tone(seconds: float, from_hz: float, to_hz: float, decay: float, level: float, noise: float) -> AudioStreamWAV:
	var count := int(seconds * MIX_RATE)
	var samples := PackedFloat32Array()
	samples.resize(count)
	var phase := 0.0
	for i in count:
		var t := float(i) / MIX_RATE
		phase += TAU * lerpf(from_hz, to_hz, float(i) / count) / MIX_RATE
		var body := sin(phase) * (1.0 - noise) + randf_range(-1.0, 1.0) * noise
		samples[i] = body * exp(-decay * t) * level
	return _stream(_faded(samples))


## Two pitches one after the other, the second starting half-way: a "got it" blip.
func _two_tones(seconds: float, first_hz: float, second_hz: float, decay: float, level: float) -> AudioStreamWAV:
	var count := int(seconds * MIX_RATE)
	var samples := PackedFloat32Array()
	samples.resize(count)
	var half := count >> 1
	var phase := 0.0
	for i in count:
		var second := i >= half
		var t := float(i - (half if second else 0)) / MIX_RATE
		phase += TAU * (second_hz if second else first_hz) / MIX_RATE
		samples[i] = sin(phase) * exp(-decay * t) * level
	return _stream(_faded(samples))


## Several pitches sounding together and ringing out, each quieter than the one below.
func _chord(seconds: float, pitches: Array, decay: float, level: float) -> AudioStreamWAV:
	var count := int(seconds * MIX_RATE)
	var samples := PackedFloat32Array()
	samples.resize(count)
	for i in count:
		var t := float(i) / MIX_RATE
		var sum := 0.0
		for index in pitches.size():
			var pitch: float = pitches[index]
			sum += sin(TAU * pitch * t) / (index + 1.0)
		samples[i] = sum * exp(-decay * t) * level * 0.6
	return _stream(_faded(samples))


## A second of soft noise whose end runs into its start, to be played round and round.
func _hiss_loop() -> AudioStreamWAV:
	var count := int(HISS_SECONDS * MIX_RATE)
	var blend := int(HISS_BLEND_SECONDS * MIX_RATE)
	var raw := PackedFloat32Array()
	raw.resize(count + blend)
	# Noise with the harshest part of its top taken off, and its rumble too.
	var smooth := 0.0
	var slow := 0.0
	for i in raw.size():
		var white := randf_range(-1.0, 1.0)
		smooth += (white - smooth) * 0.35
		slow += (smooth - slow) * 0.02
		raw[i] = (smooth - slow) * 0.6
	var samples := PackedFloat32Array()
	samples.resize(count)
	for i in count:
		samples[i] = raw[i]
	# The tail beyond the loop is faded in under the start, so the join is seamless.
	for i in blend:
		var share := float(i) / blend
		samples[i] = raw[i] * share + raw[count + i] * (1.0 - share)
	var stream := _stream(samples)
	stream.loop_mode = AudioStreamWAV.LOOP_FORWARD
	stream.loop_begin = 0
	stream.loop_end = count
	return stream


## Brings the first and last few samples to silence so that a sound cannot start or
## end with a click of its own.
func _faded(samples: PackedFloat32Array) -> PackedFloat32Array:
	var edge := mini(48, samples.size() >> 1)
	for i in edge:
		var share := float(i) / edge
		samples[i] *= share
		samples[samples.size() - 1 - i] *= share
	return samples


## Samples from -1 to 1 as a 16-bit mono sound.
func _stream(samples: PackedFloat32Array) -> AudioStreamWAV:
	var data := PackedByteArray()
	data.resize(samples.size() * 2)
	for i in samples.size():
		data.encode_s16(i * 2, roundi(clampf(samples[i], -1.0, 1.0) * 32767.0))
	var stream := AudioStreamWAV.new()
	stream.format = AudioStreamWAV.FORMAT_16_BITS
	stream.mix_rate = MIX_RATE
	stream.stereo = false
	stream.data = data
	return stream
