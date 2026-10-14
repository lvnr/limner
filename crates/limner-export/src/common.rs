//! Small decisions the USD and FBX writers share, so the two cannot drift apart.

use crate::scene::{ExportCamera, ExportLight, ExportNode, ExportScene, NodeContent, Sampled};
use limner_core::glam::DQuat;
use limner_core::math::Xform;

/// The scene's name in a file, for formats that want one.
pub(crate) fn title(scene: &ExportScene) -> &str {
    if scene.name.is_empty() {
        "Limner scene"
    } else {
        scene.name.as_str()
    }
}

/// What every file says wrote it.
pub(crate) const GENERATOR: &str =
    concat!("Limner ", env!("CARGO_PKG_VERSION"), " (limner-export)");

/// A transform that can be written: non-finite numbers fall back to no transform,
/// and the rotation is a unit quaternion.
pub(crate) fn clean(x: &Xform) -> Xform {
    if !x.is_finite() {
        return Xform::IDENTITY;
    }
    let rotation = x.rotation.normalize();
    Xform {
        rotation: if rotation.is_finite() {
            rotation
        } else {
            DQuat::IDENTITY
        },
        ..*x
    }
}

/// The node's transform on every sampled frame, cleaned.
pub(crate) fn transforms(scene: &ExportScene, node: &ExportNode) -> Vec<Xform> {
    (0..scene.frame_count())
        .map(|frame| clean(node.transform.at(frame)))
        .collect()
}

/// The one opacity a format without animated opacity gives a layer: the largest it
/// has in the range. The first frame's would make a layer that fades in invisible for
/// the whole export; the largest shows it as it looks at its fullest. For a layer
/// whose opacity does not change, this is simply its opacity.
pub(crate) fn peak_opacity(node: &ExportNode) -> f32 {
    match &node.opacity {
        Sampled::Constant(opacity) => *opacity,
        Sampled::PerFrame(samples) => samples.iter().copied().fold(f32::NAN, f32::max),
    }
}

/// The node's own visibility on every sampled frame.
pub(crate) fn visibility(scene: &ExportScene, node: &ExportNode) -> Vec<bool> {
    (0..scene.frame_count())
        .map(|frame| *node.visible.at(frame))
        .collect()
}

/// A light's colour with every component in `0..=1`, and the intensity that makes up
/// for it: colours brighter than 1 move their excess into the intensity. glTF needs
/// this; USD and FBX do the same so that all three files carry the same numbers.
pub(crate) fn light_color_and_intensity(light: &ExportLight) -> ([f32; 3], f32) {
    let color = light
        .color
        .map(|c| if c.is_finite() { c.max(0.0) } else { 0.0 });
    let peak = color.iter().copied().fold(1.0f32, f32::max);
    let energy = if light.energy.is_finite() {
        light.energy
    } else {
        0.0
    };
    (color.map(|c| c / peak), (energy * peak).max(0.0))
}

/// Width over height of the camera's picture.
pub(crate) fn camera_aspect(camera: &ExportCamera) -> f32 {
    let [width, height] = camera.resolution;
    if width > 0 && height > 0 {
        width as f32 / height as f32
    } else {
        16.0 / 9.0
    }
}

/// The camera's vertical field of view in degrees, kept inside what a perspective
/// camera can have.
pub(crate) fn camera_fov(camera: &ExportCamera) -> f32 {
    if camera.fov.is_finite() {
        camera.fov.clamp(1.0, 179.0)
    } else {
        45.0
    }
}

/// A name without control characters, which no format wants inside a string.
pub(crate) fn printable(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).collect()
}

/// The warning for a format that holds no sound, naming the sound layers that would
/// have been heard in the range; `None` when there are none. `why` finishes the
/// sentence.
pub(crate) fn sounds_left_out(scene: &ExportScene, why: &str) -> Option<String> {
    let names: Vec<String> = scene
        .nodes
        .iter()
        .filter(|node| match &node.content {
            NodeContent::Sound(sound) => sound.audio.is_some() && !sound.plays.is_empty(),
            _ => false,
        })
        .map(|node| format!("\"{}\"", printable(&node.name)))
        .collect();
    match names.as_slice() {
        [] => None,
        [one] => Some(format!("the sound layer {one} was left out: {why}")),
        _ => Some(format!(
            "{} sound layers were left out ({}): {why}",
            names.len(),
            names.join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limner_core::model::LightKind;

    #[test]
    fn bright_light_colours_move_into_the_intensity() {
        let light = ExportLight {
            kind: LightKind::Point,
            color: [2.0, 1.0, 0.5],
            energy: 1.5,
            range: 1.0,
            world_range: 1.0,
            spot_angle: 30.0,
            spot_blend: 0.0,
            shadows: false,
        };
        assert_eq!(light_color_and_intensity(&light), ([1.0, 0.5, 0.25], 3.0));
        let dim = ExportLight {
            color: [0.5, f32::NAN, -1.0],
            ..light
        };
        assert_eq!(light_color_and_intensity(&dim), ([0.5, 0.0, 0.0], 1.5));
    }

    #[test]
    fn broken_transforms_are_replaced() {
        let mut broken = Xform::IDENTITY;
        broken.translation.x = f64::NAN;
        assert_eq!(clean(&broken), Xform::IDENTITY);
        let mut long = Xform::IDENTITY;
        long.rotation = DQuat::from_xyzw(0.0, 0.0, 0.0, 2.0);
        assert_eq!(clean(&long).rotation, DQuat::IDENTITY);
    }
}
