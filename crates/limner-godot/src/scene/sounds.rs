//! Sound layers: one audio player each, kept in step with the timeline.
//!
//! How loud a sound is at the listener (its fade with distance and its directional
//! cone or frustum) is worked out here and set as the player's volume, because the
//! engine's own distance models have no radius of full volume and no fall-off band.
//! The engine places a positional sound between the ears.

use super::nodes::{release, MediaCache, Placement};
use crate::convert::vector3;
use godot::classes::audio_stream_player_3d::AttenuationModel;
use godot::classes::audio_stream_wav::{Format, LoopMode};
use godot::classes::{
    AudioStream, AudioStreamMp3, AudioStreamOggVorbis, AudioStreamPlayer, AudioStreamPlayer3D,
    AudioStreamWav,
};
use godot::prelude::*;
use limner::core::eval::LayerState;
use limner::core::glam::DVec3;
use limner::core::model::{
    Attenuation, AttenuationMode, Media, MediaData, Sound, SoundKind, SoundModifier,
};
use limner::core::time::{self, Flicks};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How far a player may be from the timeline before it is moved to match.
const MAX_DRIFT: f64 = 0.05;
/// How much of a sound is heard for each move of the playhead while scrubbing.
const SNIPPET: Duration = Duration::from_millis(120);
/// How long a player is left alone after it was started or moved. The position it
/// reports only catches up once the audio thread has mixed from the new place.
const SETTLE: Duration = Duration::from_millis(300);

// ----- pure decisions -----

/// Samples of any supported layout as 16-bit PCM, which is what the engine plays.
/// Returns the bytes and whether they are stereo. Sound with more than two channels
/// keeps only its first, which for ambisonic sound is the channel that carries the
/// whole field.
pub(super) fn pcm16(
    channels: u16,
    bits: u16,
    float: bool,
    samples: &[u8],
) -> Option<(Vec<u8>, bool)> {
    let width = match (bits, float) {
        (8 | 16 | 24 | 32, false) | (32 | 64, true) => bits as usize / 8,
        _ => return None,
    };
    if channels == 0 {
        return None;
    }
    let from_float = |v: f64| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
    let sample = |bytes: &[u8]| -> i16 {
        match (bits, float) {
            (8, _) => (i16::from(bytes[0]) - 128) << 8,
            (16, _) => i16::from_le_bytes([bytes[0], bytes[1]]),
            (24, _) => i16::from_le_bytes([bytes[1], bytes[2]]),
            (32, false) => i16::from_le_bytes([bytes[2], bytes[3]]),
            (32, true) => from_float(f64::from(f32::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
            ]))),
            _ => from_float(f64::from_le_bytes(
                bytes[..8].try_into().expect("eight bytes wide"),
            )),
        }
    };
    let kept = if channels == 2 { 2 } else { 1 };
    let frame = width * channels as usize;
    let mut out = Vec::with_capacity(samples.len() / frame * kept * 2);
    for frame in samples.chunks_exact(frame) {
        for channel in frame.chunks_exact(width).take(kept) {
            out.extend(sample(channel).to_le_bytes());
        }
    }
    Some((out, kept == 2))
}

/// How loud a sound is at `distance` (viewer-space metres) from its source. The radii
/// are in layer units; `scale` is metres per layer unit, from the layer and the view.
/// Inside the near radius the sound is at full volume, beyond the far one silent.
/// "Linear" falls evenly between them; "logarithmic" falls as one over the distance,
/// shifted so that it too reaches silence at the far radius.
pub(super) fn distance_gain(attenuation: &Attenuation, scale: f64, distance: f64) -> f32 {
    let near = f64::from(attenuation.min).max(0.0) * scale;
    let far = (f64::from(attenuation.max) * scale).max(near);
    let gain = match attenuation.mode {
        AttenuationMode::None => 1.0,
        _ if distance <= near => 1.0,
        _ if distance >= far => 0.0,
        AttenuationMode::Linear => (far - distance) / (far - near),
        AttenuationMode::Logarithmic => {
            // A source with no near radius would be silent everywhere but at its centre.
            let near = near.max(far * 1e-3);
            ((near / distance - near / far) / (1.0 - near / far)).clamp(0.0, 1.0)
        }
    };
    gain as f32
}

/// How loud a sound is in the direction `to_listener`, given in the sound's own frame
/// (it faces -Z). Angles are measured from the axis the sound faces along: at full
/// volume inside the inner angle, fading over the band to the outside level.
pub(super) fn direction_gain(modifier: &SoundModifier, to_listener: DVec3) -> f32 {
    let Some(direction) = to_listener.try_normalize() else {
        return 1.0;
    };
    let (excess, band, outside) = match *modifier {
        SoundModifier::None => return 1.0,
        SoundModifier::Cone {
            inner,
            band,
            outside,
        } => {
            let angle = (-direction.z).clamp(-1.0, 1.0).acos();
            (angle - f64::from(inner), band, outside)
        }
        SoundModifier::Frustum {
            inner_h,
            inner_v,
            band,
            outside,
        } => {
            let across = direction.x.atan2(-direction.z).abs();
            let up = direction.y.atan2(direction.x.hypot(direction.z)).abs();
            (
                (across - f64::from(inner_h)).max(up - f64::from(inner_v)),
                band,
                outside,
            )
        }
    };
    let outside = outside.clamp(0.0, 1.0);
    let t = if excess <= 0.0 {
        0.0
    } else if band <= 0.0 {
        1.0
    } else {
        (excess / f64::from(band)).min(1.0) as f32
    };
    1.0 + (outside - 1.0) * t
}

/// Where in its audio a sound is, `local` seconds into its clip. `None` when it is
/// before its start or, unless it loops, past its end.
pub(super) fn stream_position(local: f64, length: f64, looped: bool) -> Option<f64> {
    if !(length > 0.0 && local >= 0.0) {
        None
    } else if looped {
        Some(local % length)
    } else {
        (local < length).then_some(local)
    }
}

/// How far apart two places in a sound are. In a loop the end is next to the start.
pub(super) fn drift(position: f64, target: f64, length: f64, looped: bool) -> f64 {
    let apart = (position - target).abs();
    if looped && length > 0.0 {
        apart.min(length - apart).abs()
    } else {
        apart
    }
}

/// What a sound's player should do this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Step {
    Keep,
    Stop,
    /// Start playing from here, in seconds.
    Play(f64),
    /// Start a short snippet from here: the playhead was moved by hand.
    Scrub(f64),
    /// Move the playing sound here.
    Seek(f64),
}

/// What a sound's player is doing and where its layer is in time.
#[derive(Clone, Copy, Debug)]
pub(super) struct Moment {
    /// The document is playing.
    pub playing: bool,
    /// The layer's own time changed since the last frame.
    pub moved: bool,
    /// Where the sound should be, when it should be heard at all.
    pub target: Option<f64>,
    pub length: f64,
    pub looped: bool,
    pub player_playing: bool,
    pub player_position: f64,
    /// The player was started or moved a moment ago and has not caught up yet.
    pub settling: bool,
    /// A scrubbing snippet is still sounding.
    pub in_snippet: bool,
}

pub(super) fn step(moment: &Moment) -> Step {
    let Some(target) = moment.target else {
        return if moment.player_playing {
            Step::Stop
        } else {
            Step::Keep
        };
    };
    match (moment.playing, moment.moved) {
        (true, true) if !moment.player_playing => Step::Play(target),
        (true, true) => {
            let apart = drift(moment.player_position, target, moment.length, moment.looped);
            if !moment.settling && apart > MAX_DRIFT {
                Step::Seek(target)
            } else {
                Step::Keep
            }
        }
        // Scrubbing: each move of the playhead is heard for a moment.
        (false, true) => Step::Scrub(target),
        // Time stands still for this layer: playback is paused or held at a Stop.
        (_, false) if moment.player_playing && !moment.in_snippet => Step::Stop,
        (_, false) => Step::Keep,
    }
}

// ----- streams -----

/// A sound as the engine plays it.
pub(super) struct Stream {
    stream: Gd<AudioStream>,
    /// In seconds.
    pub(super) length: f64,
}

pub(super) fn build_stream(media: &Media) -> Option<Stream> {
    let stream: Gd<AudioStream> = match &media.data {
        MediaData::RawAudio {
            channels,
            sample_rate,
            bits_per_sample,
            float,
            samples,
        } => {
            let (data, stereo) = pcm16(*channels, *bits_per_sample, *float, samples)?;
            let mut wav = AudioStreamWav::new_gd();
            wav.set_format(Format::FORMAT_16_BITS);
            wav.set_mix_rate(*sample_rate as i32);
            wav.set_stereo(stereo);
            wav.set_data(&PackedByteArray::from(data));
            wav.upcast()
        }
        MediaData::Encoded { format, bytes } => {
            let data = PackedByteArray::from(&bytes[..]);
            match format.as_str() {
                "wav" => AudioStreamWav::load_from_buffer(&data)?.upcast(),
                "ogg" => AudioStreamOggVorbis::load_from_buffer(&data)?.upcast(),
                "mp3" => AudioStreamMp3::load_from_buffer(&data)?.upcast(),
                _ => return None,
            }
        }
        _ => return None,
    };
    let length = stream.get_length();
    (length > 0.0).then_some(Stream { stream, length })
}

/// A copy of a stream that goes round. Looping is a setting of the stream itself, and
/// two layers may play the same sound, one looping and one not.
fn looping(stream: &Stream) -> Option<Gd<AudioStream>> {
    let copy = stream.stream.duplicate_resource();
    let copy = match copy.try_cast::<AudioStreamWav>() {
        Ok(mut wav) => {
            let frames = (stream.length * f64::from(wav.get_mix_rate())).round() as i32;
            wav.set_loop_mode(LoopMode::FORWARD);
            wav.set_loop_begin(0);
            wav.set_loop_end(frames);
            return Some(wav.upcast());
        }
        Err(other) => other,
    };
    let copy = match copy.try_cast::<AudioStreamOggVorbis>() {
        Ok(mut ogg) => {
            ogg.set_loop(true);
            return Some(ogg.upcast());
        }
        Err(other) => other,
    };
    let mut mp3 = copy.try_cast::<AudioStreamMp3>().ok()?;
    mp3.set_loop(true);
    Some(mp3.upcast())
}

// ----- players -----

enum Player {
    Flat(Gd<AudioStreamPlayer>),
    Positional(Gd<AudioStreamPlayer3D>),
}

/// Runs the same call on whichever kind of player this is.
macro_rules! on_player {
    ($player:expr, $p:ident => $call:expr) => {
        match $player {
            Player::Flat($p) => $call,
            Player::Positional($p) => $call,
        }
    };
}

/// What a player's stream was made for; it is made again when any of it changes.
#[derive(Clone, Copy, PartialEq)]
struct Loaded {
    media: usize,
    looped: bool,
}

pub(super) struct SoundNode {
    player: Player,
    loaded: Option<Loaded>,
    length: f64,
    volume: f32,
    last_local: Option<Flicks>,
    snippet_until: Option<Instant>,
    settle_until: Option<Instant>,
}

/// What every sound needs to know about the moment.
pub(super) struct Hearing {
    /// The document is playing.
    pub playing: bool,
    /// Every sound is silenced.
    pub mute: bool,
    /// Where the listener's ears are, in viewer space.
    pub listener: DVec3,
}

/// True when a sound of this kind is a point in space.
pub(super) fn is_positional(kind: SoundKind) -> bool {
    kind == SoundKind::Positional
}

impl SoundNode {
    pub(super) fn new(holder: &mut Gd<Node3D>, state: &LayerState, positional: bool) -> Self {
        let name = format!("Sound_{}", state.id.0);
        let player = if positional {
            let mut player = AudioStreamPlayer3D::new_alloc();
            player.set_name(&name);
            // The fade with distance is worked out here; see the top of this file.
            player.set_attenuation_model(AttenuationModel::DISABLED);
            player.set_max_distance(0.0);
            player.set_attenuation_filter_cutoff_hz(20500.0);
            holder.add_child(&player);
            Player::Positional(player)
        } else {
            let mut player = AudioStreamPlayer::new_alloc();
            player.set_name(&name);
            holder.add_child(&player);
            Player::Flat(player)
        };
        Self {
            player,
            loaded: None,
            length: 0.0,
            volume: -1.0,
            last_local: None,
            snippet_until: None,
            settle_until: None,
        }
    }

    pub(super) fn is_positional(&self) -> bool {
        matches!(self.player, Player::Positional(_))
    }

    /// True while the player is sounding: during playback, or for the snippet a
    /// scrub plays. A muted sound still counts, since it is mixed all the same.
    pub(super) fn is_playing(&self) -> bool {
        on_player!(&self.player, p => p.is_playing())
    }

    /// Brings the player in line with its layer. `audible` says that the layer and
    /// everything above it are inside their clips; the eye toggle is not asked, since
    /// hiding a sound layer hides its gizmo and nothing else.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sync(
        &mut self,
        sound: &Sound,
        media: Option<&Arc<Media>>,
        state: &LayerState,
        audible: bool,
        place: &Placement,
        hearing: &Hearing,
        streams: &mut MediaCache<Stream>,
    ) {
        let stream = media.and_then(|media| Some((media, streams.get(media, build_stream)?)));
        let loaded = stream.as_ref().map(|(media, _)| Loaded {
            media: Arc::as_ptr(media) as usize,
            looped: sound.looped,
        });
        if self.loaded != loaded {
            on_player!(&mut self.player, p => p.stop());
            let playable = stream.as_ref().and_then(|(_, stream)| {
                if sound.looped {
                    looping(stream)
                } else {
                    Some(stream.stream.clone())
                }
            });
            self.length = match (&playable, &stream) {
                (Some(_), Some((_, stream))) => stream.length,
                _ => 0.0,
            };
            match &playable {
                Some(stream) => on_player!(&mut self.player, p => p.set_stream(stream)),
                None => on_player!(&mut self.player, p => p.set_stream(Gd::null_arg())),
            }
            self.loaded = loaded;
            self.snippet_until = None;
            self.settle_until = None;
        }

        let mut volume = sound.gain * state.opacity;
        if let Player::Positional(player) = &mut self.player {
            let position = place.rigid.translation;
            player.set_position(vector3(position));
            let to_listener = hearing.listener - position;
            volume *= distance_gain(&sound.attenuation, place.scale, to_listener.length());
            let in_own_frame = place.rigid.matrix3.transpose() * to_listener;
            volume *= direction_gain(&sound.modifier, in_own_frame);
        }
        let volume = if hearing.mute {
            0.0
        } else {
            volume.clamp(0.0, 4.0)
        };
        if self.volume != volume {
            on_player!(&mut self.player, p => p.set_volume_linear(volume));
            self.volume = volume;
        }

        let now = Instant::now();
        let local = time::to_seconds(state.local_time);
        let moved = self.last_local.is_some_and(|last| last != state.local_time);
        self.last_local = Some(state.local_time);
        let player_playing = on_player!(&self.player, p => p.is_playing());
        let moment = Moment {
            playing: hearing.playing,
            moved,
            target: stream_position(local, self.length, sound.looped).filter(|_| audible),
            length: self.length,
            looped: sound.looped,
            player_playing,
            player_position: if player_playing {
                f64::from(on_player!(&self.player, p => p.get_playback_position()))
            } else {
                0.0
            },
            settling: self.settle_until.is_some_and(|until| now < until),
            in_snippet: self.snippet_until.is_some_and(|until| now < until),
        };
        match step(&moment) {
            Step::Keep => {}
            Step::Stop => {
                on_player!(&mut self.player, p => p.stop());
                self.snippet_until = None;
            }
            Step::Play(from) => {
                on_player!(&mut self.player, p => p.play_ex().from_position(from as f32).done());
                self.snippet_until = None;
                self.settle_until = Some(now + SETTLE);
            }
            Step::Scrub(from) => {
                on_player!(&mut self.player, p => p.play_ex().from_position(from as f32).done());
                self.snippet_until = Some(now + SNIPPET);
            }
            Step::Seek(to) => {
                on_player!(&mut self.player, p => p.seek(to as f32));
                self.settle_until = Some(now + SETTLE);
            }
        }
    }

    pub(super) fn free(self) {
        on_player!(self.player, p => {
            let mut p = p;
            p.stop();
            release(p.upcast());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn samples_of_every_layout_become_16_bit() {
        let (data, stereo) = pcm16(1, 16, false, &[0x34, 0x12, 0xFF, 0x7F]).unwrap();
        assert_eq!((data, stereo), (vec![0x34, 0x12, 0xFF, 0x7F], false));
        // Eight-bit samples are unsigned: 128 is silence.
        let (data, _) = pcm16(1, 8, false, &[128, 255, 0]).unwrap();
        assert_eq!(data, [0, 0, 0, 0x7F, 0, 0x80]);
        let (data, stereo) = pcm16(2, 24, false, &[9, 0x34, 0x12, 9, 0x78, 0x56]).unwrap();
        assert_eq!((data, stereo), (vec![0x34, 0x12, 0x78, 0x56], true));
        let floats: Vec<u8> = [0.5f32, -1.0, 2.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let (data, _) = pcm16(1, 32, true, &floats).unwrap();
        let values: Vec<i16> = data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
            .collect();
        assert_eq!(values, [16384, -32767, 32767]);
    }

    #[test]
    fn sound_wider_than_stereo_keeps_its_first_channel() {
        let frames: Vec<u8> = (0..8u8).flat_map(|i| [i, 0]).collect();
        let (data, stereo) = pcm16(4, 16, false, &frames).unwrap();
        assert_eq!((data, stereo), (vec![0, 0, 4, 0], false));
        assert!(pcm16(0, 16, false, &frames).is_none());
        assert!(pcm16(1, 12, false, &frames).is_none());
        assert!(pcm16(1, 16, true, &frames).is_none());
    }

    #[test]
    fn a_sound_fades_between_its_two_radii_at_the_scale_of_its_layer() {
        let linear = Attenuation {
            mode: AttenuationMode::Linear,
            min: 1.0,
            max: 3.0,
        };
        assert_eq!(distance_gain(&linear, 1.0, 0.5), 1.0);
        assert_eq!(distance_gain(&linear, 1.0, 2.0), 0.5);
        assert_eq!(distance_gain(&linear, 1.0, 3.0), 0.0);
        assert_eq!(distance_gain(&linear, 1.0, 30.0), 0.0);
        // A layer ten times larger is heard ten times further away.
        assert_eq!(distance_gain(&linear, 10.0, 20.0), 0.5);
        assert_eq!(distance_gain(&linear, 0.001, 0.002), 0.5);

        let log = Attenuation {
            mode: AttenuationMode::Logarithmic,
            ..linear
        };
        assert_eq!(distance_gain(&log, 1.0, 1.0), 1.0);
        assert_eq!(distance_gain(&log, 1.0, 3.0), 0.0);
        let halfway = distance_gain(&log, 1.0, 2.0);
        assert!((halfway - 0.25).abs() < 1e-6, "{halfway}");
        assert!(
            halfway < distance_gain(&linear, 1.0, 2.0),
            "falls faster near the source"
        );

        let none = Attenuation {
            mode: AttenuationMode::None,
            ..linear
        };
        assert_eq!(distance_gain(&none, 1.0, 1e9), 1.0);
        let point = Attenuation {
            mode: AttenuationMode::Linear,
            min: 2.0,
            max: 2.0,
        };
        assert_eq!(distance_gain(&point, 1.0, 1.9), 1.0);
        assert_eq!(distance_gain(&point, 1.0, 2.1), 0.0);
    }

    #[test]
    fn a_cone_is_loud_ahead_and_fades_over_its_band() {
        let cone = SoundModifier::Cone {
            inner: 0.5,
            band: 0.5,
            outside: 0.2,
        };
        let at = |angle: f64| DVec3::new(angle.sin(), 0.0, -angle.cos());
        assert_eq!(direction_gain(&cone, at(0.0)), 1.0);
        assert_eq!(direction_gain(&cone, at(0.49) * 7.0), 1.0);
        assert!((direction_gain(&cone, at(0.75)) - 0.6).abs() < 1e-5);
        assert!((direction_gain(&cone, at(1.2)) - 0.2).abs() < 1e-6);
        assert!((direction_gain(&cone, DVec3::Z) - 0.2).abs() < 1e-6);
        assert_eq!(direction_gain(&cone, DVec3::ZERO), 1.0);
        assert_eq!(direction_gain(&SoundModifier::None, DVec3::Z), 1.0);
        let hard = SoundModifier::Cone {
            inner: 0.5,
            band: 0.0,
            outside: 0.0,
        };
        assert_eq!(direction_gain(&hard, at(0.51)), 0.0);
    }

    #[test]
    fn a_frustum_has_its_own_width_and_height() {
        let frustum = SoundModifier::Frustum {
            inner_h: 1.0,
            inner_v: 0.2,
            band: 0.2,
            outside: 0.0,
        };
        let across = |angle: f64| DVec3::new(angle.sin(), 0.0, -angle.cos());
        let up = |angle: f64| DVec3::new(0.0, angle.sin(), -angle.cos());
        assert_eq!(direction_gain(&frustum, across(0.9)), 1.0);
        assert_eq!(direction_gain(&frustum, up(0.19)), 1.0);
        assert!((direction_gain(&frustum, up(0.3)) - 0.5).abs() < 1e-5);
        assert_eq!(direction_gain(&frustum, up(f64::from(FRAC_PI_2))), 0.0);
        assert_eq!(direction_gain(&frustum, across(1.3)), 0.0);
    }

    #[test]
    fn a_sound_is_somewhere_in_its_audio_only_while_it_lasts() {
        assert_eq!(stream_position(0.5, 2.0, false), Some(0.5));
        assert_eq!(stream_position(2.0, 2.0, false), None);
        assert_eq!(stream_position(-0.1, 2.0, true), None);
        assert_eq!(stream_position(4.5, 2.0, true), Some(0.5));
        assert_eq!(stream_position(1.0, 0.0, true), None);
        assert_eq!(drift(0.2, 0.5, 2.0, false), 0.3);
        assert!((drift(1.99, 0.01, 2.0, true) - 0.02).abs() < 1e-9);
        assert!((drift(1.99, 0.01, 2.0, false) - 1.98).abs() < 1e-9);
    }

    fn moment() -> Moment {
        Moment {
            playing: true,
            moved: true,
            target: Some(1.0),
            length: 4.0,
            looped: false,
            player_playing: true,
            player_position: 1.0,
            settling: false,
            in_snippet: false,
        }
    }

    #[test]
    fn playback_starts_follows_and_stops_with_the_timeline() {
        let start = Moment {
            player_playing: false,
            ..moment()
        };
        assert_eq!(step(&start), Step::Play(1.0));
        assert_eq!(step(&moment()), Step::Keep);
        let slightly_off = Moment {
            player_position: 1.04,
            ..moment()
        };
        assert_eq!(step(&slightly_off), Step::Keep);
        let after_a_seek = Moment {
            player_position: 3.0,
            ..moment()
        };
        assert_eq!(step(&after_a_seek), Step::Seek(1.0));
        let just_started = Moment {
            settling: true,
            ..after_a_seek
        };
        assert_eq!(step(&just_started), Step::Keep);
        let clip_over = Moment {
            target: None,
            ..moment()
        };
        assert_eq!(step(&clip_over), Step::Stop);
        let paused = Moment {
            playing: false,
            moved: false,
            ..moment()
        };
        assert_eq!(step(&paused), Step::Stop);
        let held_at_a_stop = Moment {
            moved: false,
            ..moment()
        };
        assert_eq!(step(&held_at_a_stop), Step::Stop);
        let silent = Moment {
            target: None,
            player_playing: false,
            ..moment()
        };
        assert_eq!(step(&silent), Step::Keep);
    }

    #[test]
    fn scrubbing_plays_a_snippet_from_each_new_position() {
        let scrub = Moment {
            playing: false,
            player_playing: false,
            ..moment()
        };
        assert_eq!(step(&scrub), Step::Scrub(1.0));
        let sounding = Moment {
            playing: false,
            moved: false,
            in_snippet: true,
            ..moment()
        };
        assert_eq!(step(&sounding), Step::Keep);
        let over = Moment {
            in_snippet: false,
            ..sounding
        };
        assert_eq!(step(&over), Step::Stop);
        let moved_again = Moment {
            moved: true,
            target: Some(2.5),
            ..sounding
        };
        assert_eq!(step(&moved_again), Step::Scrub(2.5));
    }
}
