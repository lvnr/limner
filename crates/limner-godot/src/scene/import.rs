//! Bringing picture, sound and model files into the document.
//!
//! The facade reads the file and decides what it becomes. What is added here is the
//! part only the engine can do: checking that a picture or a sound really decodes,
//! and reading the model formats the engine has importers for (glTF, FBX).

use super::{pictures, sounds};
use crate::convert::linear_from_color;
use godot::classes::base_material_3d::TextureParam;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::{
    ArrayMesh, BaseMaterial3D, FbxDocument, FbxState, GltfDocument, GltfState,
    ImporterMeshInstance3D, Material, MeshInstance3D,
};
use godot::global::Error;
use godot::prelude::*;
use limner::core::model::{LayerId, Media, MediaId, MediaKind, MeshData};
use limner::core::session::media::MediaLayerKind;
use limner::core::session::Session;
use limner::media::{self, FileKind, Import, MAX_PICTURE_SIDE};
use std::path::Path;

/// Imports the file at `path` as a new layer. Returns the layer and what was left
/// out or repaired on the way.
pub(super) fn import(session: &mut Session, path: &str) -> Result<(LayerId, Vec<String>), String> {
    let path = Path::new(path);
    // Another project (a Limner one, packed or not, or a Quill folder) comes in whole,
    // as a group.
    let is_project = path.is_dir()
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("limnz"));
    if is_project {
        let opened = limner::open(path).map_err(|e| e.to_string())?;
        let name = if opened.document.settings.title.trim().is_empty() {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Imported")
                .to_owned()
        } else {
            opened.document.settings.title.clone()
        };
        let group = session
            .import_document(&opened.document, &name)
            .map_err(|e| e.to_string())?;
        return Ok((group, opened.warnings));
    }
    let kind = media::kind_of(path).map_err(|e| e.to_string())?;
    let import = if kind.needs_engine() {
        let (mesh, warnings) = engine_mesh(path, kind)?;
        media::model_from_mesh(path, mesh, warnings)
    } else {
        media::read(path)
    }
    .map_err(|e| e.to_string())?;
    check_decodes(&import)?;
    let layer = session
        .add_media_layer(import.media)
        .map_err(|e| e.to_string())?;
    Ok((layer, import.warnings))
}

/// Makes sure the engine can show or play what is about to be embedded, so that a
/// damaged file is refused now and not left in the document as a blank layer.
fn check_decodes(import: &Import) -> Result<(), String> {
    let media = Media {
        id: MediaId(0),
        name: String::new(),
        kind: MediaKind::Image,
        data: import.media.data.clone(),
    };
    match import.media.kind {
        MediaLayerKind::Picture => {
            let image = pictures::decode(&media)
                .ok_or("the picture could not be decoded; the file may be damaged")?;
            let (width, height) = (image.get_width(), image.get_height());
            if width.max(height) > MAX_PICTURE_SIDE as i32 {
                return Err(format!(
                    "the picture is {width} by {height} pixels; the most that can be shown is {MAX_PICTURE_SIDE} on a side"
                ));
            }
        }
        MediaLayerKind::Sound { .. } => {
            sounds::build_stream(&media)
                .ok_or("the sound could not be decoded; the file may be damaged or in an encoding that cannot be played")?;
        }
        MediaLayerKind::Model => {}
    }
    Ok(())
}

/// Geometry gathered from every mesh of a scene, with the transforms of the nodes
/// that hold them applied.
#[derive(Default)]
struct Gathered {
    mesh: MeshData,
    /// One per position once any surface brought a colour; the surfaces before it are white.
    colored: bool,
    has_textures: bool,
    skipped_surfaces: usize,
}

impl Gathered {
    /// Adds one surface: Godot mesh arrays, the material drawn with, and where the
    /// node holding it sits in the scene.
    fn add_surface(
        &mut self,
        primitive: PrimitiveType,
        arrays: &VarArray,
        material: Option<Gd<Material>>,
        transform: Transform3D,
    ) {
        let array = |kind: ArrayType| arrays.get(kind.ord() as usize);
        let positions: Option<PackedVector3Array> =
            array(ArrayType::VERTEX).and_then(|v| v.try_to().ok());
        let Some(positions) =
            positions.filter(|p| primitive == PrimitiveType::TRIANGLES && !p.is_empty())
        else {
            self.skipped_surfaces += 1;
            return;
        };
        let count = positions.len();
        let normals: Option<PackedVector3Array> = array(ArrayType::NORMAL)
            .and_then(|v| v.try_to().ok())
            .filter(|n: &PackedVector3Array| n.len() == count);
        let colors: Option<PackedColorArray> = array(ArrayType::COLOR)
            .and_then(|v| v.try_to().ok())
            .filter(|c: &PackedColorArray| c.len() == count);
        let indices: Option<PackedInt32Array> =
            array(ArrayType::INDEX).and_then(|v| v.try_to().ok());

        // A material gives its plain colour; what it holds in textures is left out.
        let material = material.and_then(|m| m.try_cast::<BaseMaterial3D>().ok());
        let tint = material.as_ref().map(|m| linear_from_color(m.get_albedo()));
        if material.is_some_and(|m| m.get_texture(TextureParam::ALBEDO).is_some()) {
            self.has_textures = true;
        }

        let base = self.mesh.positions.len() as u32;
        let had_normals = self.mesh.normals.len() == base as usize;
        self.mesh.positions.extend(
            positions
                .as_slice()
                .iter()
                .map(|p| (transform * *p).to_array()),
        );
        match normals.filter(|_| had_normals) {
            Some(normals) => {
                // Normals turn with the inverse transpose, which keeps them square to
                // the surface when a node is stretched.
                let basis = transform.basis.inverse().transposed();
                self.mesh.normals.extend(
                    normals
                        .as_slice()
                        .iter()
                        .map(|n| (basis * *n).normalized_or_zero().to_array()),
                );
            }
            // Normals are kept only when every surface has them.
            None => self.mesh.normals.clear(),
        }
        if colors.is_some() || tint.is_some() {
            if !self.colored {
                self.mesh.colors = vec![[1.0; 3]; base as usize];
                self.colored = true;
            }
            let tint = tint.unwrap_or([1.0; 3]);
            self.mesh.colors.extend((0..count).map(|i| {
                // Vertex colours in these formats are linear already.
                let c = colors.as_ref().map_or(Color::WHITE, |c| c.as_slice()[i]);
                [c.r * tint[0], c.g * tint[1], c.b * tint[2]]
            }));
        } else if self.colored {
            self.mesh
                .colors
                .extend(std::iter::repeat_n([1.0; 3], count));
        }

        let corners: Vec<u32> = match &indices {
            Some(indices) if !indices.is_empty() => {
                indices.as_slice().iter().map(|i| *i as u32).collect()
            }
            _ => (0..count as u32).collect(),
        };
        // Godot's front faces wind clockwise and the document's counter-clockwise; a
        // mirroring node swaps them once more.
        let swap = transform.basis.determinant() >= 0.0;
        for triangle in corners.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|i| base + i);
            self.mesh
                .indices
                .extend(if swap { [a, c, b] } else { [a, b, c] });
        }
    }

    /// Adds every mesh at or under `node`, which sits at `parent` in the scene.
    fn add_node(&mut self, node: &Gd<Node>, parent: Transform3D) {
        let transform = match node.clone().try_cast::<Node3D>() {
            Ok(spatial) => parent * spatial.get_transform(),
            Err(_) => parent,
        };
        if let Ok(instance) = node.clone().try_cast::<MeshInstance3D>() {
            if let Some(mesh) = instance.get_mesh() {
                for surface in 0..mesh.get_surface_count() {
                    let primitive = mesh
                        .clone()
                        .try_cast::<ArrayMesh>()
                        .map_or(PrimitiveType::TRIANGLES, |m| {
                            m.surface_get_primitive_type(surface)
                        });
                    let material = instance
                        .get_surface_override_material(surface)
                        .or_else(|| mesh.surface_get_material(surface));
                    self.add_surface(
                        primitive,
                        &mesh.surface_get_arrays(surface),
                        material,
                        transform,
                    );
                }
            }
        } else if let Ok(instance) = node.clone().try_cast::<ImporterMeshInstance3D>() {
            if let Some(mesh) = instance.get_mesh() {
                for surface in 0..mesh.get_surface_count() {
                    self.add_surface(
                        mesh.get_surface_primitive_type(surface),
                        &mesh.get_surface_arrays(surface),
                        mesh.get_surface_material(surface),
                        transform,
                    );
                }
            }
        }
        for child in node.get_children().iter_shared() {
            self.add_node(&child, transform);
        }
    }
}

/// Reads a glTF or FBX file with the engine's importer and gathers its meshes into one.
fn engine_mesh(path: &Path, kind: FileKind) -> Result<(MeshData, Vec<String>), String> {
    let (mut document, state): (Gd<GltfDocument>, Gd<GltfState>) = match kind {
        FileKind::Fbx => (FbxDocument::new_gd().upcast(), FbxState::new_gd().upcast()),
        _ => (GltfDocument::new_gd(), GltfState::new_gd()),
    };
    let file = path.to_string_lossy().into_owned();
    let result = document.append_from_file(file.as_str(), &state);
    if result != Error::OK {
        return Err(format!(
            "the model could not be read ({})",
            format!("{result:?}").to_lowercase().replace('_', " ")
        ));
    }
    let scene = document
        .generate_scene(&state)
        .ok_or("the model could not be read: it holds no scene")?;
    let mut gathered = Gathered::default();
    gathered.add_node(&scene, Transform3D::IDENTITY);
    scene.free();

    let mut warnings = Vec::new();
    if gathered.has_textures || !state.get_images().is_empty() {
        warnings
            .push("the model's textures are not imported; it keeps its plain colours".to_owned());
    }
    if !state.get_animations().is_empty() {
        warnings.push("the model's animation is not imported".to_owned());
    }
    if !state.get_skins().is_empty() {
        warnings
            .push("the model's skeleton is not imported; it stands in its rest pose".to_owned());
    }
    if gathered.skipped_surfaces > 0 {
        warnings.push(format!(
            "{} part(s) of the model are lines or points and were left out",
            gathered.skipped_surfaces
        ));
    }
    Ok((gathered.mesh, warnings))
}
