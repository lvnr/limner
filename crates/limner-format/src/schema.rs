//! The schema of `limn.json`.
//!
//! These structs *are* the file format. They are deliberately separate from the
//! core model so that renaming or reshaping a core type cannot silently change what
//! is written to disk: the conversions in [`crate::manifest`] have to be updated by
//! hand, and adding a variant to a core enum fails to compile here until the format
//! decides how to spell it.
//!
//! Every key and every enumeration value is `snake_case`. Unknown keys are ignored
//! on read. Keys documented with a default may be absent.

use crate::json::Num;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) const FORMAT_NAME: &str = "limn";
pub(crate) const FORMAT_VERSION: u32 = 1;

/// Read before anything else, so a newer file is rejected for its version and not
/// for whatever else changed in it.
#[derive(Deserialize)]
pub(crate) struct Probe {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub version: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ManifestDto {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub generator: String,
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub settings: SettingsDto,
    pub root: LayerDto,
    #[serde(default)]
    pub drawings: Vec<DrawingEntryDto>,
    #[serde(default)]
    pub media: Vec<MediaEntryDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SettingsDto {
    pub title: String,
    pub description: String,
    pub background: [Num; 3],
    pub rate: [u32; 2],
    pub range_start: i64,
    pub range_end: i64,
    pub default_viewpoint: Option<u64>,
    pub camera_resolution: [u32; 2],
    pub palette: Vec<[Num; 3]>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct DrawingEntryDto {
    pub id: u64,
    pub blob: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MediaEntryDto {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    pub kind: MediaKindDto,
    pub storage: StorageDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    pub file: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StorageDto {
    Encoded,
    RawImage,
    RawAudio,
    Mesh,
}

fn yes() -> bool {
    true
}

fn one() -> Num {
    Num(1.0)
}

#[derive(Serialize, Deserialize)]
pub(crate) struct LayerDto {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type")]
    pub kind: LayerTypeDto,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub show_bounds: bool,
    #[serde(default = "one")]
    pub opacity: Num,
    #[serde(default)]
    pub transform: XformDto,
    #[serde(default)]
    pub pivot: XformDto,
    #[serde(default)]
    pub anim: AnimDto,
    // Exactly the member named by `type` is present (`sequence` and `children` for groups).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint: Option<PaintDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<PictureDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<SoundDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewpoint: Option<ViewpointDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<LightDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<SequenceDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<LayerDto>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LayerTypeDto {
    Group,
    Paint,
    Picture,
    Model,
    Sound,
    Viewpoint,
    Camera,
    Light,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct XformDto {
    /// Unit quaternion, `[x, y, z, w]`.
    pub rotation: [Num; 4],
    pub scale: Num,
    pub translation: [Num; 3],
    pub flip: FlipDto,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AnimDto {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub visibility: Vec<KeyDto<bool>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub offset: Vec<KeyDto<i64>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub opacity: Vec<KeyDto<Num>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub transform: Vec<XformKeyDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<KeyDto<ActionDto>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub channels: BTreeMap<String, Vec<KeyDto<Num>>>,
}

fn is_step(interp: &InterpDto) -> bool {
    *interp == InterpDto::Step
}

#[derive(Serialize, Deserialize)]
pub(crate) struct KeyDto<V> {
    pub time: i64,
    #[serde(default, skip_serializing_if = "is_step")]
    pub interp: InterpDto,
    pub value: V,
}

/// A transform key: the key's members and the transform's, side by side.
#[derive(Serialize, Deserialize)]
pub(crate) struct XformKeyDto {
    pub time: i64,
    #[serde(default, skip_serializing_if = "is_step")]
    pub interp: InterpDto,
    #[serde(default = "XformDto::identity_rotation")]
    pub rotation: [Num; 4],
    #[serde(default = "one")]
    pub scale: Num,
    #[serde(default)]
    pub translation: [Num; 3],
    #[serde(default)]
    pub flip: FlipDto,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SequenceDto {
    pub loop_duration: i64,
    pub max_repeat: u32,
}

fn default_rate() -> [u32; 2] {
    [24, 1]
}

#[derive(Serialize, Deserialize)]
pub(crate) struct PaintDto {
    #[serde(default = "default_rate")]
    pub rate: [u32; 2],
    pub drawings: Vec<u64>,
    pub frames: Vec<u32>,
    #[serde(default)]
    pub max_repeat: u32,
    #[serde(default)]
    pub shading: ShadingDto,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PictureDto {
    pub media: Option<u64>,
    pub projection: ProjectionDto,
    pub viewer_locked: bool,
    pub source_path: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ModelDto {
    pub media: Option<u64>,
    pub shading: ShadingDto,
    pub wireframe: bool,
    pub source_path: String,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SoundDto {
    pub media: Option<u64>,
    pub kind: SoundKindDto,
    pub gain: Num,
    pub looped: bool,
    pub attenuation: AttenuationDto,
    pub modifier: SoundModifierDto,
    pub source_path: String,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AttenuationDto {
    pub mode: AttenuationModeDto,
    pub min: Num,
    pub max: Num,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum SoundModifierDto {
    #[default]
    None,
    Cone {
        inner: Num,
        band: Num,
        outside: Num,
    },
    Frustum {
        inner_h: Num,
        inner_v: Num,
        band: Num,
        outside: Num,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ViewpointDto {
    pub floor_level: bool,
    pub color: [Num; 3],
    pub volume: [Num; 4],
    pub allow_translation: [bool; 3],
    pub exporting: bool,
    pub show_volume: bool,
    pub thumbnail: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CameraDto {
    pub fov: Num,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LightDto {
    pub kind: LightKindDto,
    pub color: [Num; 3],
    pub energy: Num,
    pub range: Num,
    pub spot_angle: Num,
    pub spot_blend: Num,
    pub shadows: bool,
}

/// Declares a schema enumeration, its spelling in JSON, and its mapping to and from
/// the core enumeration it stands for. The matches are exhaustive in both directions.
macro_rules! string_enum {
    ($dto:ident, $core:ty, default $default:ident, { $($variant:ident = $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub(crate) enum $dto {
            $(#[serde(rename = $text)] $variant,)+
        }

        impl Default for $dto {
            fn default() -> Self {
                $dto::$default
            }
        }

        impl From<$core> for $dto {
            fn from(value: $core) -> Self {
                type Core = $core;
                match value {
                    $(Core::$variant => $dto::$variant,)+
                }
            }
        }

        impl From<$dto> for $core {
            fn from(value: $dto) -> Self {
                type Core = $core;
                match value {
                    $($dto::$variant => Core::$variant,)+
                }
            }
        }
    };
}

string_enum!(FlipDto, limner_core::math::Flip, default None, {
    None = "none", X = "x", Y = "y", Z = "z",
});
string_enum!(InterpDto, limner_core::model::Interp, default Step, {
    Step = "step", Linear = "linear", EaseIn = "ease_in", EaseOut = "ease_out",
    EaseInOut = "ease_in_out", Smooth = "smooth",
});
string_enum!(ActionDto, limner_core::model::Action, default Stop, {
    Stop = "stop", Play = "play", Loop = "loop",
});
string_enum!(ShadingDto, limner_core::model::Shading, default Unlit, {
    Unlit = "unlit", Lit = "lit",
});
string_enum!(ProjectionDto, limner_core::model::Projection, default Flat, {
    Flat = "flat", Equirect = "equirect", EquirectStereo = "equirect_stereo",
    CubemapCross = "cubemap_cross", CubemapStrip = "cubemap_strip",
});
string_enum!(SoundKindDto, limner_core::model::SoundKind, default Flat, {
    Flat = "flat", Positional = "positional", Ambisonic = "ambisonic",
});
string_enum!(AttenuationModeDto, limner_core::model::AttenuationMode, default None, {
    None = "none", Linear = "linear", Logarithmic = "logarithmic",
});
string_enum!(LightKindDto, limner_core::model::LightKind, default Point, {
    Point = "point", Spot = "spot", Directional = "directional",
});
string_enum!(MediaKindDto, limner_core::model::MediaKind, default Image, {
    Image = "image", Audio = "audio", Mesh = "mesh",
});
