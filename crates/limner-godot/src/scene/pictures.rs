//! Picture layers on screen: a quad for a flat picture, the inside of a sphere for
//! one that surrounds the viewer.

use super::nodes::{release, Materials, MediaCache};
use super::strokes::layer_seed;
use crate::convert::transform3d;
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::image::{AlphaMode, Format};
use godot::classes::{Image, ImageTexture, MeshInstance3D, QuadMesh, ShaderMaterial, SphereMesh};
use godot::global::Error;
use godot::prelude::*;
use limner::core::eval::LayerState;
use limner::core::glam::{DAffine3, DVec3};
use limner::core::model::{Media, MediaData, Picture, Projection};
use std::sync::Arc;

/// The radius, in layer units, of the sphere a surrounding picture is drawn on:
/// large enough to stand in at the layer's own scale.
pub(super) const SPHERE_RADIUS: f64 = 10.0;

/// A picture as the graphics card holds it.
pub(super) struct Texture {
    texture: Gd<ImageTexture>,
    pub(super) width: i32,
    pub(super) height: i32,
    has_alpha: bool,
}

/// Decodes a picture, whatever form the document holds it in.
pub(super) fn decode(media: &Media) -> Option<Gd<Image>> {
    match &media.data {
        MediaData::RawImage {
            width,
            height,
            channels,
            pixels,
        } => {
            let format = match channels {
                1 => Format::L8,
                2 => Format::LA8,
                3 => Format::RGB8,
                4 => Format::RGBA8,
                _ => return None,
            };
            let expected = *width as usize * *height as usize * *channels as usize;
            if expected == 0 || pixels.len() != expected {
                return None;
            }
            Image::create_from_data(
                *width as i32,
                *height as i32,
                false,
                format,
                &PackedByteArray::from(&pixels[..]),
            )
        }
        MediaData::Encoded { format, bytes } => {
            let mut image = Image::new_gd();
            let data = PackedByteArray::from(&bytes[..]);
            let result = match format.as_str() {
                "png" => image.load_png_from_buffer(&data),
                "jpg" => image.load_jpg_from_buffer(&data),
                _ => return None,
            };
            (result == Error::OK && !image.is_empty()).then_some(image)
        }
        _ => None,
    }
}

pub(super) fn build_texture(media: &Media) -> Option<Texture> {
    let mut image = decode(media)?;
    let has_alpha = image.detect_alpha() != AlphaMode::NONE;
    let (width, height) = (image.get_width(), image.get_height());
    // Flat pictures are seen small as well as large; without these they would shimmer.
    image.generate_mipmaps();
    Some(Texture {
        texture: ImageTexture::create_from_image(&image)?,
        width,
        height,
        has_alpha,
    })
}

/// The meshes every picture shares: a unit quad and a unit sphere.
#[derive(Default)]
pub(super) struct PictureMeshes {
    quad: Option<Gd<QuadMesh>>,
    sphere: Option<Gd<SphereMesh>>,
}

impl PictureMeshes {
    fn quad(&mut self) -> &Gd<QuadMesh> {
        self.quad.get_or_insert_with(QuadMesh::new_gd)
    }

    fn sphere(&mut self) -> &Gd<SphereMesh> {
        self.sphere.get_or_insert_with(|| {
            let mut sphere = SphereMesh::new_gd();
            sphere.set_radius(1.0);
            sphere.set_height(2.0);
            sphere.set_radial_segments(64);
            sphere.set_rings(32);
            sphere
        })
    }
}

/// The number the sphere shader knows a way of wrapping by. A cube map's layout is
/// told from the picture's shape: a cross is four faces by three, lying or standing,
/// and a strip is six faces in a column or a row.
fn projection_code(projection: Projection, width: i32, height: i32) -> Option<i32> {
    match projection {
        Projection::Flat => None,
        Projection::Equirect => Some(0),
        Projection::EquirectStereo => Some(1),
        Projection::CubemapCross => Some(if width >= height { 2 } else { 3 }),
        Projection::CubemapStrip => Some(if height >= width { 4 } else { 5 }),
    }
}

/// What a picture's material was made for; it is made again when any of it changes.
#[derive(Clone, Copy, PartialEq)]
struct Look {
    media: usize,
    projection: Option<i32>,
    see_through: bool,
    materials: u32,
}

pub(super) struct PictureNode {
    node: Gd<MeshInstance3D>,
    material: Option<Gd<ShaderMaterial>>,
    look: Option<Look>,
    opacity: f32,
    dither_frame: i32,
    transform: Option<Transform3D>,
    visible: bool,
}

impl PictureNode {
    pub(super) fn new(holder: &mut Gd<Node3D>, state: &LayerState) -> Self {
        let mut node = MeshInstance3D::new_alloc();
        node.set_name(&format!("Picture_{}", state.id.0));
        node.set_visible(false);
        // A picture is not a thing in the scene: it throws no shadow on lit layers.
        node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        node.set_instance_shader_parameter("layer_seed", &layer_seed(state.id).to_variant());
        holder.add_child(&node);
        Self {
            node,
            material: None,
            look: None,
            opacity: -1.0,
            dither_frame: 0,
            transform: None,
            visible: false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn sync(
        &mut self,
        picture: &Picture,
        media: Option<&Arc<Media>>,
        state: &LayerState,
        viewer_from_world: &DAffine3,
        textures: &mut MediaCache<Texture>,
        meshes: &mut PictureMeshes,
        materials: &Materials,
    ) {
        let texture = media
            .filter(|_| state.visible && state.opacity > 0.0)
            .and_then(|media| Some((media, textures.get(media, build_texture)?)));
        let Some((media, texture)) = texture else {
            self.set_visible(false);
            return;
        };
        let projection = projection_code(picture.projection, texture.width, texture.height);
        let look = Look {
            media: Arc::as_ptr(media) as usize,
            projection,
            see_through: texture.has_alpha || state.opacity < 0.999,
            materials: materials.version,
        };
        if self.look != Some(look) {
            let template = &materials.picture
                [usize::from(projection.is_some()) * 2 + usize::from(look.see_through)];
            let Some(mut material) = template.as_ref().map(|t| t.duplicate_resource()) else {
                self.set_visible(false);
                return;
            };
            material.set_shader_parameter("picture", &texture.texture.to_variant());
            match projection {
                Some(code) => {
                    material.set_shader_parameter("projection", &code.to_variant());
                    self.node.set_mesh(meshes.sphere());
                }
                None => self.node.set_mesh(meshes.quad()),
            }
            self.node.set_material_override(&material);
            self.dither_frame = materials.dither_frame;
            self.material = Some(material);
            self.look = Some(look);
            self.transform = None;
        }
        if look.see_through && self.dither_frame != materials.dither_frame {
            // Each picture has a material of its own, so the moving dither pattern
            // has to be passed on to it.
            if let Some(material) = &mut self.material {
                material.set_shader_parameter("frame_index", &materials.dither_frame.to_variant());
            }
            self.dither_frame = materials.dither_frame;
        }
        if self.opacity != state.opacity {
            self.node
                .set_instance_shader_parameter("layer_opacity", &state.opacity.to_variant());
            self.opacity = state.opacity;
        }
        // A picture locked to the viewer keeps its place in the room: navigation,
        // which moves the world, does not move it.
        let place = if picture.viewer_locked {
            state.world
        } else {
            *viewer_from_world * state.world
        };
        let size = match projection {
            Some(_) => DVec3::splat(SPHERE_RADIUS),
            // One layer unit tall, and as wide as the picture's shape makes it.
            None => DVec3::new(texture.width as f64 / texture.height as f64, 1.0, 1.0),
        };
        let transform = transform3d(&(place * DAffine3::from_scale(size)));
        if self.transform != Some(transform) {
            self.node.set_transform(transform);
            self.transform = Some(transform);
        }
        self.set_visible(true);
    }

    fn set_visible(&mut self, visible: bool) {
        if self.visible != visible {
            self.node.set_visible(visible);
            self.visible = visible;
        }
    }

    pub(super) fn free(self) {
        release(self.node.upcast());
    }
}

/// The corners of a picture's content in its layer's space.
pub(super) fn bounds(picture: &Picture, texture: &Texture) -> (DVec3, DVec3) {
    let half = match picture.projection {
        Projection::Flat => {
            DVec3::new(texture.width as f64 / texture.height as f64 * 0.5, 0.5, 0.0)
        }
        _ => DVec3::splat(SPHERE_RADIUS),
    };
    (-half, half)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cube_map_layout_is_told_from_the_shape_of_the_picture() {
        assert_eq!(projection_code(Projection::Flat, 4, 3), None);
        assert_eq!(projection_code(Projection::Equirect, 4, 2), Some(0));
        assert_eq!(projection_code(Projection::EquirectStereo, 4, 4), Some(1));
        assert_eq!(projection_code(Projection::CubemapCross, 400, 300), Some(2));
        assert_eq!(projection_code(Projection::CubemapCross, 300, 400), Some(3));
        assert_eq!(projection_code(Projection::CubemapStrip, 100, 600), Some(4));
        assert_eq!(projection_code(Projection::CubemapStrip, 600, 100), Some(5));
    }
}
