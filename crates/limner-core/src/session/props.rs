//! The settings each kind of layer has beyond the common ones, as flat records: what
//! a property panel shows and what the `set_layer_kind` command changes.

use super::playback::MAX_TIME;
use crate::json::{merge, patched};
use crate::model::{
    Attenuation, Layer, LayerKind, Light, Projection, Shading, SoundKind, SoundModifier,
};
use crate::time::Flicks;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaintProps {
    pub shading: Shading,
    /// How many times the frames play before holding on the last one. Zero loops forever.
    pub max_repeat: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SequenceProps {
    /// Where the sequence loops, in its own time. Zero means it does not loop.
    pub loop_duration: Flicks,
    pub max_repeat: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraProps {
    /// Vertical field of view in degrees.
    pub fov: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewpointProps {
    pub floor_level: bool,
    pub color: [f32; 3],
    pub volume: [f32; 4],
    pub allow_translation: [bool; 3],
    pub exporting: bool,
    pub show_volume: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundProps {
    pub kind: SoundKind,
    pub gain: f32,
    pub looped: bool,
    pub attenuation: Attenuation,
    pub modifier: SoundModifier,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PictureProps {
    pub projection: Projection,
    pub viewer_locked: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelProps {
    pub shading: Shading,
    pub wireframe: bool,
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

/// What a sound's cone or frustum starts as when a sound first takes that shape:
/// Quill's own starting angles, in radians. The Layer panel starts from the same.
fn starting_shape(name: &str) -> Option<Value> {
    match name {
        "cone" => Some(json!({"inner": 0.5, "band": 0.1, "outside": 0.0})),
        "frustum" => Some(json!({"inner_h": 0.6, "inner_v": 0.4, "band": 0.1, "outside": 0.0})),
        _ => None,
    }
}

/// A sound's shape with `change` applied. `change` is `"none"`, or an object naming
/// one shape and some or all of its values: `{"cone": {"inner": 0.5}}`. Values it
/// leaves out keep what the sound has, or, when the sound takes a new shape, the
/// fade and outside volume it had and the new shape's starting angles.
fn patched_modifier(current: SoundModifier, change: &Value) -> Result<SoundModifier, String> {
    let Value::Object(named) = change else {
        return serde_json::from_value(change.clone()).map_err(|e| e.to_string());
    };
    let mut entries = named.iter();
    let (Some((shape, values)), None) = (entries.next(), entries.next()) else {
        return Err("name exactly one shape: none, cone or frustum".into());
    };
    let Some(mut base) = starting_shape(shape) else {
        return Err(format!("there is no shape named \"{shape}\""));
    };
    let Value::Object(values) = values else {
        return Err(format!("the values of a {shape} go in an object"));
    };
    if let Some(unknown) = values.keys().find(|key| base.get(key.as_str()).is_none()) {
        return Err(format!("a {shape} has no setting named \"{unknown}\""));
    }
    // What the sound has now: all of it when it keeps its shape, and only the fade
    // and outside volume when it changes to the other one.
    if let Ok(Value::Object(now)) = serde_json::to_value(current) {
        if let Some(Value::Object(fields)) = now.values().next() {
            let same_shape = now.contains_key(shape.as_str());
            for (key, value) in fields {
                if same_shape || key == "band" || key == "outside" {
                    base[key.as_str()] = value.clone();
                }
            }
        }
    }
    merge(&mut base, &Value::Object(values.clone()));
    let mut whole = Map::new();
    whole.insert(shape.clone(), base);
    serde_json::from_value(Value::Object(whole)).map_err(|e| e.to_string())
}

/// The kind-specific settings of a layer as a JSON object.
pub fn kind_props(layer: &Layer) -> Value {
    let value = match &layer.kind {
        LayerKind::Group(group) => match &group.sequence {
            Some(sequence) => serde_json::to_value(SequenceProps {
                loop_duration: sequence.loop_duration,
                max_repeat: sequence.max_repeat,
            }),
            None => Ok(json!({})),
        },
        LayerKind::Paint(paint) => serde_json::to_value(PaintProps {
            shading: paint.shading,
            max_repeat: paint.max_repeat,
        }),
        LayerKind::Picture(picture) => serde_json::to_value(PictureProps {
            projection: picture.projection,
            viewer_locked: picture.viewer_locked,
        }),
        LayerKind::Model(model) => serde_json::to_value(ModelProps {
            shading: model.shading,
            wireframe: model.wireframe,
        }),
        LayerKind::Sound(sound) => serde_json::to_value(SoundProps {
            kind: sound.kind,
            gain: sound.gain,
            looped: sound.looped,
            attenuation: sound.attenuation,
            modifier: sound.modifier,
        }),
        LayerKind::Viewpoint(v) => serde_json::to_value(ViewpointProps {
            floor_level: v.floor_level,
            color: v.color,
            volume: v.volume,
            allow_translation: v.allow_translation,
            exporting: v.exporting,
            show_volume: v.show_volume,
        }),
        LayerKind::Camera(camera) => serde_json::to_value(CameraProps { fov: camera.fov }),
        LayerKind::Light(light) => serde_json::to_value(light),
    };
    value.unwrap_or(Value::Null)
}

/// Changes the kind-specific settings named in `patch`. Values are brought into range.
pub fn apply_kind_patch(layer: &mut Layer, patch: &Map<String, Value>) -> Result<(), String> {
    match &mut layer.kind {
        LayerKind::Group(group) => {
            let Some(sequence) = &mut group.sequence else {
                return Err("a group has no settings of its own".into());
            };
            let props = patched(
                &SequenceProps {
                    loop_duration: sequence.loop_duration,
                    max_repeat: sequence.max_repeat,
                },
                patch,
            )?;
            sequence.loop_duration = props.loop_duration.clamp(0, MAX_TIME);
            sequence.max_repeat = props.max_repeat;
        }
        LayerKind::Paint(paint) => {
            let props = patched(
                &PaintProps {
                    shading: paint.shading,
                    max_repeat: paint.max_repeat,
                },
                patch,
            )?;
            paint.shading = props.shading;
            paint.max_repeat = props.max_repeat;
        }
        LayerKind::Picture(picture) => {
            let props = patched(
                &PictureProps {
                    projection: picture.projection,
                    viewer_locked: picture.viewer_locked,
                },
                patch,
            )?;
            picture.projection = props.projection;
            picture.viewer_locked = props.viewer_locked;
        }
        LayerKind::Model(model) => {
            let props = patched(
                &ModelProps {
                    shading: model.shading,
                    wireframe: model.wireframe,
                },
                patch,
            )?;
            model.shading = props.shading;
            model.wireframe = props.wireframe;
        }
        LayerKind::Sound(sound) => {
            // The modifier is one of several shapes, so it is patched on its own.
            let mut patch = patch.clone();
            let modifier = match patch.remove("modifier") {
                Some(change) => patched_modifier(sound.modifier, &change)
                    .map_err(|e| format!("modifier: {e}"))?,
                None => sound.modifier,
            };
            let patch = &patch;
            let props = patched(
                &SoundProps {
                    kind: sound.kind,
                    gain: sound.gain,
                    looped: sound.looped,
                    attenuation: sound.attenuation,
                    modifier,
                },
                patch,
            )?;
            sound.kind = props.kind;
            sound.gain = finite(props.gain, 1.0).clamp(0.0, 4.0);
            sound.looped = props.looped;
            let min = finite(props.attenuation.min, 0.1).max(0.0);
            sound.attenuation = Attenuation {
                mode: props.attenuation.mode,
                min,
                max: finite(props.attenuation.max, 0.5).max(min),
            };
            sound.modifier = props.modifier;
        }
        LayerKind::Viewpoint(v) => {
            let props = patched(
                &ViewpointProps {
                    floor_level: v.floor_level,
                    color: v.color,
                    volume: v.volume,
                    allow_translation: v.allow_translation,
                    exporting: v.exporting,
                    show_volume: v.show_volume,
                },
                patch,
            )?;
            v.floor_level = props.floor_level;
            v.color = props.color.map(|c| finite(c, 0.0).clamp(0.0, 1.0));
            v.volume = [
                finite(props.volume[0], 0.0),
                finite(props.volume[1], 1.0),
                finite(props.volume[2], 0.0),
                finite(props.volume[3], 2.0).max(0.0),
            ];
            v.allow_translation = props.allow_translation;
            v.exporting = props.exporting;
            v.show_volume = props.show_volume;
        }
        LayerKind::Camera(camera) => {
            let props = patched(&CameraProps { fov: camera.fov }, patch)?;
            camera.fov = finite(props.fov, 45.0).clamp(1.0, 170.0);
        }
        LayerKind::Light(light) => {
            let props: Light = patched(light, patch)?;
            *light = Light {
                kind: props.kind,
                color: props.color.map(|c| finite(c, 1.0).max(0.0)),
                energy: finite(props.energy, 1.0).clamp(0.0, 1000.0),
                range: finite(props.range, 5.0).max(1e-6),
                spot_angle: finite(props.spot_angle, 35.0).clamp(0.5, 89.0),
                spot_blend: finite(props.spot_blend, 0.15).clamp(0.0, 1.0),
                shadows: props.shadows,
            };
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Camera, Group, LayerId, LightKind};

    fn object(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn a_light_takes_a_partial_patch_and_keeps_values_in_range() {
        let mut layer = Layer::new(LayerId(1), "Sun", LayerKind::Light(Light::default()));
        apply_kind_patch(
            &mut layer,
            &object(json!({"kind": "spot", "energy": -3.0, "spot_angle": 400.0})),
        )
        .unwrap();
        let LayerKind::Light(light) = &layer.kind else {
            unreachable!()
        };
        assert_eq!(light.kind, LightKind::Spot);
        assert_eq!(light.energy, 0.0);
        assert_eq!(light.spot_angle, 89.0);
        assert_eq!(light.range, 5.0, "untouched fields stay");
        assert_eq!(kind_props(&layer)["kind"], "spot");
    }

    #[test]
    fn a_sound_modifier_is_replaced_whole() {
        use crate::model::{Attenuation, Sound, SoundKind, SoundModifier};
        let sound = Sound {
            media: None,
            kind: SoundKind::Positional,
            gain: 1.0,
            looped: false,
            attenuation: Attenuation::default(),
            modifier: SoundModifier::Cone {
                inner: 0.5,
                band: 0.1,
                outside: 0.0,
            },
            source_path: String::new(),
        };
        let mut layer = Layer::new(LayerId(1), "Voice", LayerKind::Sound(sound));
        apply_kind_patch(&mut layer, &object(json!({"gain": 0.5, "modifier": {"frustum": {"inner_h": 0.6, "inner_v": 0.4, "band": 0.1, "outside": 0.2}}}))).unwrap();
        let LayerKind::Sound(sound) = &layer.kind else {
            unreachable!()
        };
        assert_eq!(
            sound.modifier,
            SoundModifier::Frustum {
                inner_h: 0.6,
                inner_v: 0.4,
                band: 0.1,
                outside: 0.2
            }
        );
        assert_eq!(sound.gain, 0.5);
        apply_kind_patch(&mut layer, &object(json!({"modifier": "none"}))).unwrap();
        let LayerKind::Sound(sound) = &layer.kind else {
            unreachable!()
        };
        assert_eq!(sound.modifier, SoundModifier::None);
        assert!(
            apply_kind_patch(&mut layer, &object(json!({"modifier": {"sphere": {}}}))).is_err()
        );
    }

    fn voice() -> Layer {
        use crate::model::{Attenuation, Sound, SoundKind, SoundModifier};
        let sound = Sound {
            media: None,
            kind: SoundKind::Positional,
            gain: 1.0,
            looped: false,
            attenuation: Attenuation::default(),
            modifier: SoundModifier::Cone {
                inner: 0.5,
                band: 0.1,
                outside: 0.2,
            },
            source_path: String::new(),
        };
        Layer::new(LayerId(1), "Voice", LayerKind::Sound(sound))
    }

    fn modifier(layer: &Layer) -> SoundModifier {
        let LayerKind::Sound(sound) = &layer.kind else {
            unreachable!()
        };
        sound.modifier
    }

    #[test]
    fn a_partial_sound_shape_changes_only_what_it_names() {
        let mut layer = voice();
        // One slider of the cone.
        apply_kind_patch(
            &mut layer,
            &object(json!({"modifier": {"cone": {"inner": 0.9}}})),
        )
        .unwrap();
        assert_eq!(
            modifier(&layer),
            SoundModifier::Cone {
                inner: 0.9,
                band: 0.1,
                outside: 0.2
            }
        );
        // The whole cone still works.
        apply_kind_patch(
            &mut layer,
            &object(json!({"modifier": {"cone": {"inner": 0.4, "band": 0.3, "outside": 0.0}}})),
        )
        .unwrap();
        assert_eq!(
            modifier(&layer),
            SoundModifier::Cone {
                inner: 0.4,
                band: 0.3,
                outside: 0.0
            }
        );
        // Switching to a frustum with only some of its values keeps the fade and the
        // outside volume and starts the rest where a new frustum starts.
        apply_kind_patch(
            &mut layer,
            &object(json!({"modifier": {"frustum": {"inner_v": 0.7}}})),
        )
        .unwrap();
        assert_eq!(
            modifier(&layer),
            SoundModifier::Frustum {
                inner_h: 0.6,
                inner_v: 0.7,
                band: 0.3,
                outside: 0.0
            }
        );
        apply_kind_patch(&mut layer, &object(json!({"modifier": "none"}))).unwrap();
        assert_eq!(modifier(&layer), SoundModifier::None);
        apply_kind_patch(
            &mut layer,
            &object(json!({"modifier": {"cone": {"band": 0.2}}})),
        )
        .unwrap();
        assert_eq!(
            modifier(&layer),
            SoundModifier::Cone {
                inner: 0.5,
                band: 0.2,
                outside: 0.0
            }
        );
        // Names that do not belong are refused, and leave the shape as it was.
        for bad in [
            json!({"modifier": {"cone": {"sise": 0.2}}}),
            json!({"modifier": {"cone": {"inner": "wide"}}}),
            json!({"modifier": {"sphere": {}}}),
            json!({"modifier": {"cone": {}, "frustum": {}}}),
            json!({"modifier": 3}),
        ] {
            assert!(
                apply_kind_patch(&mut layer, &object(bad.clone())).is_err(),
                "{bad}"
            );
        }
        assert_eq!(
            modifier(&layer),
            SoundModifier::Cone {
                inner: 0.5,
                band: 0.2,
                outside: 0.0
            }
        );
    }

    #[test]
    fn a_partial_attenuation_keeps_its_other_values() {
        let mut layer = voice();
        apply_kind_patch(&mut layer, &object(json!({"attenuation": {"max": 3.0}}))).unwrap();
        let LayerKind::Sound(sound) = &layer.kind else {
            unreachable!()
        };
        assert_eq!(sound.attenuation.max, 3.0);
        assert_eq!(sound.attenuation.min, 0.1, "untouched");
    }

    #[test]
    fn a_sequence_loop_is_kept_to_a_usable_length() {
        let mut group = Layer::new(
            LayerId(2),
            "Walk",
            LayerKind::Group(Group {
                sequence: Some(Default::default()),
                ..Group::default()
            }),
        );
        apply_kind_patch(&mut group, &object(json!({"loop_duration": i64::MAX}))).unwrap();
        let LayerKind::Group(g) = &group.kind else {
            unreachable!()
        };
        assert_eq!(g.sequence.unwrap().loop_duration, MAX_TIME);
    }

    #[test]
    fn settings_that_do_not_exist_for_the_kind_are_refused() {
        let mut camera = Layer::new(LayerId(1), "Cam", LayerKind::Camera(Camera::default()));
        assert!(apply_kind_patch(&mut camera, &object(json!({"energy": 2.0}))).is_err());
        apply_kind_patch(&mut camera, &object(json!({"fov": 30}))).unwrap();
        assert_eq!(kind_props(&camera), json!({"fov": 30.0}));
        let mut group = Layer::new(LayerId(2), "G", LayerKind::Group(Group::default()));
        assert!(apply_kind_patch(&mut group, &object(json!({"loop_duration": 5}))).is_err());
    }
}
