//! Conversion between the core document and the manifest schema.

use crate::error::FormatError;
use crate::hash::ContentHash;
use crate::json::{self, f32s, nums, Num};
use crate::media::Storage;
use crate::schema::*;
use limner_core::glam::{DQuat, DVec3};
use limner_core::math::Xform;
use limner_core::model::{
    Attenuation, Camera, Document, Drawing, DrawingId, Group, Key, Layer, LayerAnim, LayerId,
    LayerKind, Light, Media, MediaId, MediaKind, Model, Paint, Picture, Sequence, Settings, Sound,
    SoundModifier, Track, Viewpoint,
};
use limner_core::time::FrameRate;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

/// The deepest nesting of layers below the root that the format holds. JSON readers
/// bound their recursion (128 levels is common), and a layer costs two levels.
pub const MAX_LAYER_DEPTH: usize = 48;

pub(crate) const GENERATOR: &str = concat!("limner-format ", env!("CARGO_PKG_VERSION"));

/// Where a media item was written.
pub(crate) struct MediaFile {
    pub storage: Storage,
    pub file: String,
}

// ----- schema defaults: the core's defaults, spelled once -----

impl Default for SettingsDto {
    fn default() -> Self {
        settings_to_dto(&Settings::default())
    }
}

impl XformDto {
    pub(crate) fn identity_rotation() -> [Num; 4] {
        [Num(0.0), Num(0.0), Num(0.0), Num(1.0)]
    }
}

impl Default for XformDto {
    fn default() -> Self {
        xform_to_dto(&Xform::IDENTITY)
    }
}

impl Default for AttenuationDto {
    fn default() -> Self {
        attenuation_to_dto(&Attenuation::default())
    }
}

impl Default for SoundDto {
    fn default() -> Self {
        SoundDto {
            media: None,
            kind: SoundKindDto::default(),
            gain: Num(1.0),
            looped: false,
            attenuation: AttenuationDto::default(),
            modifier: SoundModifierDto::default(),
            source_path: String::new(),
        }
    }
}

impl Default for ViewpointDto {
    fn default() -> Self {
        viewpoint_to_dto(&Viewpoint::default())
    }
}

impl Default for CameraDto {
    fn default() -> Self {
        camera_to_dto(&Camera::default())
    }
}

impl Default for LightDto {
    fn default() -> Self {
        light_to_dto(&Light::default())
    }
}

impl From<Storage> for StorageDto {
    fn from(storage: Storage) -> Self {
        match storage {
            Storage::Encoded => StorageDto::Encoded,
            Storage::RawImage => StorageDto::RawImage,
            Storage::RawAudio => StorageDto::RawAudio,
            Storage::Mesh => StorageDto::Mesh,
        }
    }
}

impl From<StorageDto> for Storage {
    fn from(storage: StorageDto) -> Self {
        match storage {
            StorageDto::Encoded => Storage::Encoded,
            StorageDto::RawImage => Storage::RawImage,
            StorageDto::RawAudio => Storage::RawAudio,
            StorageDto::Mesh => Storage::Mesh,
        }
    }
}

// ----- document to schema -----

fn settings_to_dto(s: &Settings) -> SettingsDto {
    SettingsDto {
        title: s.title.clone(),
        description: s.description.clone(),
        background: nums(s.background),
        rate: [s.rate.num, s.rate.den],
        range_start: s.range_start,
        range_end: s.range_end,
        default_viewpoint: s.default_viewpoint.map(|id| id.0),
        camera_resolution: s.camera_resolution,
        palette: s.palette.iter().map(|c| nums(*c)).collect(),
    }
}

fn xform_to_dto(x: &Xform) -> XformDto {
    XformDto {
        rotation: [
            Num(x.rotation.x),
            Num(x.rotation.y),
            Num(x.rotation.z),
            Num(x.rotation.w),
        ],
        scale: Num(x.scale),
        translation: [
            Num(x.translation.x),
            Num(x.translation.y),
            Num(x.translation.z),
        ],
        flip: x.flip.into(),
    }
}

fn keys_to_dto<T: Copy, V>(track: &Track<T>, value: impl Fn(T) -> V) -> Vec<KeyDto<V>> {
    track
        .keys()
        .iter()
        .map(|key| KeyDto {
            time: key.time,
            interp: key.interp.into(),
            value: value(key.value),
        })
        .collect()
}

fn anim_to_dto(anim: &LayerAnim) -> AnimDto {
    AnimDto {
        visibility: keys_to_dto(&anim.visibility, |v| v),
        offset: keys_to_dto(&anim.offset, |v| v),
        opacity: keys_to_dto(&anim.opacity, Num::from_f32),
        transform: anim
            .transform
            .keys()
            .iter()
            .map(|key| {
                let value = xform_to_dto(&key.value);
                XformKeyDto {
                    time: key.time,
                    interp: key.interp.into(),
                    rotation: value.rotation,
                    scale: value.scale,
                    translation: value.translation,
                    flip: value.flip,
                }
            })
            .collect(),
        actions: keys_to_dto(&anim.actions, ActionDto::from),
        channels: anim
            .channels
            .iter()
            .map(|(name, track)| (name.clone(), keys_to_dto(track, Num)))
            .collect(),
    }
}

fn attenuation_to_dto(a: &Attenuation) -> AttenuationDto {
    AttenuationDto {
        mode: a.mode.into(),
        min: a.min.into(),
        max: a.max.into(),
    }
}

fn viewpoint_to_dto(v: &Viewpoint) -> ViewpointDto {
    ViewpointDto {
        floor_level: v.floor_level,
        color: nums(v.color),
        volume: nums(v.volume),
        allow_translation: v.allow_translation,
        exporting: v.exporting,
        show_volume: v.show_volume,
        thumbnail: v.thumbnail.map(|id| id.0),
    }
}

fn camera_to_dto(c: &Camera) -> CameraDto {
    CameraDto { fov: c.fov.into() }
}

fn light_to_dto(l: &Light) -> LightDto {
    LightDto {
        kind: l.kind.into(),
        color: nums(l.color),
        energy: l.energy.into(),
        range: l.range.into(),
        spot_angle: l.spot_angle.into(),
        spot_blend: l.spot_blend.into(),
        shadows: l.shadows,
    }
}

fn layer_to_dto(doc: &Document, id: LayerId, depth: usize) -> Result<LayerDto, FormatError> {
    if depth > MAX_LAYER_DEPTH {
        return Err(FormatError::Unsaveable(format!(
            "layers are nested more than {MAX_LAYER_DEPTH} levels deep (at \"{}\")",
            doc.path_of(id)
        )));
    }
    let layer = doc
        .layer(id)
        .ok_or_else(|| FormatError::Unsaveable(format!("layer {id} is missing")))?;
    let mut dto = LayerDto {
        id: id.0,
        name: layer.name.clone(),
        kind: LayerTypeDto::Group,
        visible: layer.visible,
        locked: layer.locked,
        collapsed: layer.collapsed,
        show_bounds: layer.show_bounds,
        opacity: layer.opacity.into(),
        transform: xform_to_dto(&layer.transform),
        pivot: xform_to_dto(&layer.pivot),
        anim: anim_to_dto(&layer.anim),
        paint: None,
        picture: None,
        model: None,
        sound: None,
        viewpoint: None,
        camera: None,
        light: None,
        sequence: None,
        children: None,
    };
    match &layer.kind {
        LayerKind::Group(group) => {
            dto.kind = LayerTypeDto::Group;
            dto.sequence = group.sequence.map(|s| SequenceDto {
                loop_duration: s.loop_duration,
                max_repeat: s.max_repeat,
            });
            dto.children = Some(
                group
                    .children
                    .iter()
                    .map(|child| layer_to_dto(doc, *child, depth + 1))
                    .collect::<Result<_, _>>()?,
            );
        }
        LayerKind::Paint(paint) => {
            dto.kind = LayerTypeDto::Paint;
            dto.paint = Some(PaintDto {
                rate: [paint.rate.num, paint.rate.den],
                drawings: paint.drawings.iter().map(|d| d.0).collect(),
                frames: paint.frames.clone(),
                max_repeat: paint.max_repeat,
                shading: paint.shading.into(),
            });
        }
        LayerKind::Picture(picture) => {
            dto.kind = LayerTypeDto::Picture;
            dto.picture = Some(PictureDto {
                media: picture.media.map(|m| m.0),
                projection: picture.projection.into(),
                viewer_locked: picture.viewer_locked,
                source_path: picture.source_path.clone(),
            });
        }
        LayerKind::Model(model) => {
            dto.kind = LayerTypeDto::Model;
            dto.model = Some(ModelDto {
                media: model.media.map(|m| m.0),
                shading: model.shading.into(),
                wireframe: model.wireframe,
                source_path: model.source_path.clone(),
            });
        }
        LayerKind::Sound(sound) => {
            dto.kind = LayerTypeDto::Sound;
            dto.sound = Some(SoundDto {
                media: sound.media.map(|m| m.0),
                kind: sound.kind.into(),
                gain: sound.gain.into(),
                looped: sound.looped,
                attenuation: attenuation_to_dto(&sound.attenuation),
                modifier: match sound.modifier {
                    SoundModifier::None => SoundModifierDto::None,
                    SoundModifier::Cone {
                        inner,
                        band,
                        outside,
                    } => SoundModifierDto::Cone {
                        inner: inner.into(),
                        band: band.into(),
                        outside: outside.into(),
                    },
                    SoundModifier::Frustum {
                        inner_h,
                        inner_v,
                        band,
                        outside,
                    } => SoundModifierDto::Frustum {
                        inner_h: inner_h.into(),
                        inner_v: inner_v.into(),
                        band: band.into(),
                        outside: outside.into(),
                    },
                },
                source_path: sound.source_path.clone(),
            });
        }
        LayerKind::Viewpoint(viewpoint) => {
            dto.kind = LayerTypeDto::Viewpoint;
            dto.viewpoint = Some(viewpoint_to_dto(viewpoint));
        }
        LayerKind::Camera(camera) => {
            dto.kind = LayerTypeDto::Camera;
            dto.camera = Some(camera_to_dto(camera));
        }
        LayerKind::Light(light) => {
            dto.kind = LayerTypeDto::Light;
            dto.light = Some(light_to_dto(light));
        }
    }
    Ok(dto)
}

/// Describes `doc` as a manifest, given where its drawings and media were written.
pub(crate) fn from_document(
    doc: &Document,
    drawings: &BTreeMap<DrawingId, ContentHash>,
    media: &BTreeMap<MediaId, MediaFile>,
) -> Result<ManifestDto, FormatError> {
    Ok(ManifestDto {
        format: FORMAT_NAME.to_owned(),
        version: FORMAT_VERSION,
        generator: GENERATOR.to_owned(),
        next_id: doc.next_id(),
        settings: settings_to_dto(&doc.settings),
        root: layer_to_dto(doc, doc.root(), 0)?,
        drawings: drawings
            .iter()
            .map(|(id, hash)| DrawingEntryDto {
                id: id.0,
                blob: hash.to_hex(),
            })
            .collect(),
        media: doc
            .media_items()
            .filter_map(|item| {
                let written = media.get(&item.id)?;
                Some(MediaEntryDto {
                    id: item.id.0,
                    name: item.name.clone(),
                    kind: item.kind.into(),
                    storage: written.storage.into(),
                    format: match &item.data {
                        limner_core::model::MediaData::Encoded { format, .. } => {
                            Some(format.clone())
                        }
                        _ => None,
                    },
                    file: written.file.clone(),
                })
            })
            .collect(),
    })
}

/// The exact bytes of `limn.json` for a manifest.
pub(crate) fn to_bytes(manifest: &ManifestDto) -> Result<Vec<u8>, FormatError> {
    json::to_pretty_string(manifest)
        .map(String::into_bytes)
        .map_err(|e| FormatError::Unsaveable(format!("the manifest cannot be written: {e}")))
}

/// Parses `limn.json`, rejecting other formats and newer versions before anything else.
pub(crate) fn parse(bytes: &[u8], path: &Path) -> Result<ManifestDto, FormatError> {
    let json_error = |source| FormatError::Json {
        path: path.to_path_buf(),
        source,
    };
    let probe: Probe = serde_json::from_slice(bytes).map_err(json_error)?;
    match probe.format.as_deref() {
        Some(FORMAT_NAME) => {}
        Some(other) => {
            return Err(FormatError::manifest(
                path,
                format!("its format is \"{other}\", not \"{FORMAT_NAME}\""),
            ))
        }
        None => {
            return Err(FormatError::manifest(
                path,
                "it has no \"format\" member, so it is not a Limner manifest",
            ))
        }
    }
    match probe.version {
        Some(version) if version > FORMAT_VERSION as u64 => {
            return Err(FormatError::UnsupportedVersion {
                path: path.to_path_buf(),
                found: version,
                supported: FORMAT_VERSION,
            })
        }
        Some(0) | None => {
            return Err(FormatError::manifest(
                path,
                "it has no usable \"version\" member",
            ))
        }
        Some(_) => {}
    }
    serde_json::from_slice(bytes).map_err(json_error)
}

// ----- schema to document -----

fn xform_from_dto(x: &XformDto) -> Xform {
    Xform {
        rotation: DQuat::from_xyzw(
            x.rotation[0].0,
            x.rotation[1].0,
            x.rotation[2].0,
            x.rotation[3].0,
        ),
        scale: x.scale.0,
        translation: DVec3::new(x.translation[0].0, x.translation[1].0, x.translation[2].0),
        flip: x.flip.into(),
    }
}

fn track_from_dto<T, V>(keys: Vec<KeyDto<V>>, value: impl Fn(V) -> T) -> Track<T> {
    Track::from_keys(
        keys.into_iter()
            .map(|key| Key::new(key.time, value(key.value), key.interp.into())),
    )
}

fn anim_from_dto(anim: AnimDto) -> LayerAnim {
    LayerAnim {
        visibility: track_from_dto(anim.visibility, |v| v),
        offset: track_from_dto(anim.offset, |v| v),
        opacity: track_from_dto(anim.opacity, Num::to_f32),
        transform: Track::from_keys(anim.transform.into_iter().map(|key| {
            let value = xform_from_dto(&XformDto {
                rotation: key.rotation,
                scale: key.scale,
                translation: key.translation,
                flip: key.flip,
            });
            Key::new(key.time, value, key.interp.into())
        })),
        actions: track_from_dto(anim.actions, Into::into),
        channels: anim
            .channels
            .into_iter()
            .map(|(name, keys)| (name, track_from_dto(keys, |v| v.0)))
            .collect(),
    }
}

fn rate_from_dto(rate: [u32; 2]) -> FrameRate {
    FrameRate::new(rate[0], rate[1])
}

struct Builder<'a> {
    doc: Document,
    path: &'a Path,
    seen: BTreeSet<u64>,
}

impl Builder<'_> {
    fn bad(&self, reason: String) -> FormatError {
        FormatError::manifest(self.path, reason)
    }

    /// Splits a layer record into its common part, its kind and its children.
    fn kind(&mut self, dto: &mut LayerDto) -> Result<(LayerKind, Vec<LayerDto>), FormatError> {
        let id = dto.id;
        if !self.seen.insert(id) {
            return Err(self.bad(format!("layer id {id} is used by more than one layer")));
        }
        let missing = |what: &str| {
            FormatError::manifest(
                self.path,
                format!("layer {id} has type \"{what}\" but no \"{what}\" member"),
            )
        };
        let children = dto.children.take().unwrap_or_default();
        if dto.kind != LayerTypeDto::Group && !children.is_empty() {
            return Err(self.bad(format!("layer {id} is not a group but has children")));
        }
        let kind = match dto.kind {
            LayerTypeDto::Group => LayerKind::Group(Group {
                children: Vec::new(),
                sequence: dto.sequence.map(|s| Sequence {
                    loop_duration: s.loop_duration,
                    max_repeat: s.max_repeat,
                }),
            }),
            LayerTypeDto::Paint => {
                let paint = dto.paint.take().ok_or_else(|| missing("paint"))?;
                LayerKind::Paint(Paint {
                    rate: rate_from_dto(paint.rate),
                    drawings: paint.drawings.into_iter().map(DrawingId).collect(),
                    frames: paint.frames,
                    max_repeat: paint.max_repeat,
                    shading: paint.shading.into(),
                })
            }
            LayerTypeDto::Picture => {
                let picture = dto.picture.take().ok_or_else(|| missing("picture"))?;
                LayerKind::Picture(Picture {
                    media: picture.media.map(MediaId),
                    projection: picture.projection.into(),
                    viewer_locked: picture.viewer_locked,
                    source_path: picture.source_path,
                })
            }
            LayerTypeDto::Model => {
                let model = dto.model.take().ok_or_else(|| missing("model"))?;
                LayerKind::Model(Model {
                    media: model.media.map(MediaId),
                    shading: model.shading.into(),
                    wireframe: model.wireframe,
                    source_path: model.source_path,
                })
            }
            LayerTypeDto::Sound => {
                let sound = dto.sound.take().ok_or_else(|| missing("sound"))?;
                LayerKind::Sound(Sound {
                    media: sound.media.map(MediaId),
                    kind: sound.kind.into(),
                    gain: sound.gain.to_f32(),
                    looped: sound.looped,
                    attenuation: Attenuation {
                        mode: sound.attenuation.mode.into(),
                        min: sound.attenuation.min.to_f32(),
                        max: sound.attenuation.max.to_f32(),
                    },
                    modifier: match sound.modifier {
                        SoundModifierDto::None => SoundModifier::None,
                        SoundModifierDto::Cone {
                            inner,
                            band,
                            outside,
                        } => SoundModifier::Cone {
                            inner: inner.to_f32(),
                            band: band.to_f32(),
                            outside: outside.to_f32(),
                        },
                        SoundModifierDto::Frustum {
                            inner_h,
                            inner_v,
                            band,
                            outside,
                        } => SoundModifier::Frustum {
                            inner_h: inner_h.to_f32(),
                            inner_v: inner_v.to_f32(),
                            band: band.to_f32(),
                            outside: outside.to_f32(),
                        },
                    },
                    source_path: sound.source_path,
                })
            }
            LayerTypeDto::Viewpoint => {
                let v = dto.viewpoint.take().ok_or_else(|| missing("viewpoint"))?;
                LayerKind::Viewpoint(Viewpoint {
                    floor_level: v.floor_level,
                    color: f32s(v.color),
                    volume: f32s(v.volume),
                    allow_translation: v.allow_translation,
                    exporting: v.exporting,
                    show_volume: v.show_volume,
                    thumbnail: v.thumbnail.map(MediaId),
                })
            }
            LayerTypeDto::Camera => {
                let camera = dto.camera.take().ok_or_else(|| missing("camera"))?;
                LayerKind::Camera(Camera {
                    fov: camera.fov.to_f32(),
                })
            }
            LayerTypeDto::Light => {
                let light = dto.light.take().ok_or_else(|| missing("light"))?;
                LayerKind::Light(Light {
                    kind: light.kind.into(),
                    color: f32s(light.color),
                    energy: light.energy.to_f32(),
                    range: light.range.to_f32(),
                    spot_angle: light.spot_angle.to_f32(),
                    spot_blend: light.spot_blend.to_f32(),
                    shadows: light.shadows,
                })
            }
        };
        Ok((kind, children))
    }

    fn apply_common(layer: &mut Layer, dto: LayerDto) {
        layer.name = dto.name;
        layer.visible = dto.visible;
        layer.locked = dto.locked;
        layer.collapsed = dto.collapsed;
        layer.show_bounds = dto.show_bounds;
        layer.opacity = dto.opacity.to_f32();
        layer.transform = xform_from_dto(&dto.transform);
        layer.pivot = xform_from_dto(&dto.pivot);
        layer.anim = anim_from_dto(dto.anim);
    }

    fn add(&mut self, mut dto: LayerDto, parent: LayerId, depth: usize) -> Result<(), FormatError> {
        let id = LayerId(dto.id);
        if depth > MAX_LAYER_DEPTH {
            return Err(self.bad(format!(
                "layers are nested more than {MAX_LAYER_DEPTH} levels deep (at layer {id})"
            )));
        }
        let (kind, children) = self.kind(&mut dto)?;
        let mut layer = Layer::new(id, "", kind);
        Self::apply_common(&mut layer, dto);
        self.doc
            .insert_layer(layer, parent, usize::MAX)
            .map_err(|e| self.bad(format!("layer {id} cannot be placed: {e}")))?;
        for child in children {
            self.add(child, id, depth + 1)?;
        }
        Ok(())
    }
}

/// Rebuilds the document a manifest describes, with every id as it was saved.
/// `drawings` and `media` are the already loaded contents of its two tables.
pub(crate) fn into_document(
    manifest: ManifestDto,
    drawings: Vec<(DrawingId, Arc<Drawing>)>,
    media: Vec<Media>,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Result<Document, FormatError> {
    let mut builder = Builder {
        doc: Document::empty(),
        path,
        seen: BTreeSet::new(),
    };
    let mut drawing_ids = BTreeSet::new();
    for (id, drawing) in drawings {
        if !drawing_ids.insert(id.0) {
            return Err(builder.bad(format!("drawing id {id} is used by more than one drawing")));
        }
        builder.doc.insert_drawing(id, drawing);
    }
    let mut media_ids = BTreeSet::new();
    for item in media {
        if !media_ids.insert(item.id.0) {
            return Err(builder.bad(format!(
                "media id {} is used by more than one media item",
                item.id
            )));
        }
        builder.doc.insert_media(item);
    }

    let mut root = manifest.root;
    let root_id = builder.doc.root();
    if root.id != root_id.0 {
        return Err(builder.bad(format!(
            "the root layer has id {}, but this version of Limner needs it to be {root_id}",
            root.id
        )));
    }
    if root.kind != LayerTypeDto::Group {
        return Err(builder.bad("the root layer is not a group".to_owned()));
    }
    let (kind, children) = builder.kind(&mut root)?;
    {
        let layer = builder
            .doc
            .layer_mut(root_id)
            .expect("a new document has a root");
        if let (Some(group), LayerKind::Group(loaded)) = (layer.as_group_mut(), kind) {
            group.sequence = loaded.sequence;
        }
        Builder::apply_common(layer, root);
    }
    for child in children {
        builder.add(child, root_id, 1)?;
    }

    let Builder { mut doc, seen, .. } = builder;
    // Ids come from one counter, so no two items of any kind should share one. The
    // model keeps the three kinds apart, so this is reported rather than refused.
    let shared = seen
        .intersection(&drawing_ids)
        .chain(seen.intersection(&media_ids))
        .chain(drawing_ids.intersection(&media_ids))
        .next();
    if let Some(shared) = shared {
        warnings.push(format!(
            "id {shared} is used by more than one kind of item (layer, drawing, media)"
        ));
    }

    let s = manifest.settings;
    doc.settings = Settings {
        title: s.title,
        description: s.description,
        background: f32s(s.background),
        rate: rate_from_dto(s.rate),
        range_start: s.range_start,
        range_end: s.range_end,
        default_viewpoint: s.default_viewpoint.map(LayerId),
        camera_resolution: s.camera_resolution,
        palette: s.palette.into_iter().map(f32s).collect(),
    };
    if s.default_viewpoint
        .is_some_and(|id| doc.layer(LayerId(id)).is_none())
    {
        warnings.push("the default viewpoint refers to a layer that does not exist".to_owned());
    }

    if doc.next_id() > manifest.next_id {
        warnings.push(format!(
            "the id counter in limn.json ({}) was behind the ids in use; it was raised to {}",
            manifest.next_id,
            doc.next_id()
        ));
    }
    doc.reserve_ids(manifest.next_id);
    Ok(doc)
}

/// The media kind a table entry declares.
pub(crate) fn media_kind(entry: &MediaEntryDto) -> MediaKind {
    entry.kind.into()
}
