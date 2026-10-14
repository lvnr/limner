//! Writing the open document to other files: the interchange formats, and a copy of
//! the project that leaves the document's own place and unsaved state alone.

use super::LimnerScene;
use crate::convert::json_from_variant;
use godot::prelude::*;
use limner::core::model::Document;
use limner::core::time::{Flicks, FrameRate};
use limner::export::{
    write_fbx, write_glb, write_obj, write_usda, write_usdz, ExportError, ExportOptions,
    ExportReport, FbxColorSpace, FbxOptions, FbxUnit, GltfOptions, UsdOptions,
};
use serde_json::Value;
use std::path::Path;

/// Everything an export can be told, with the defaults already filled in.
struct Settings {
    scene: ExportOptions,
    gltf: GltfOptions,
    fbx: FbxOptions,
}

impl Settings {
    /// Reads the options an interface sent. Options left out keep their defaults:
    /// a still at the start of the working range, or with `animation` the whole
    /// working range at the document's rate. An option this does not know is an
    /// error that names it, so that a misspelt one is never silently ignored.
    fn read(doc: &Document, options: &Value) -> Result<Self, String> {
        let mut scene = ExportOptions::working_range(doc);
        scene.animation = false;
        let mut gltf = GltfOptions::default();
        let mut fbx = FbxOptions::default();
        let members = options
            .as_object()
            .ok_or_else(|| "the export options must be a dictionary".to_owned())?;
        for (name, value) in members {
            match name.as_str() {
                "animation" => scene.animation = flag(name, value)?,
                "start" => scene.start = time(name, value)?,
                "end" => scene.end = time(name, value)?,
                "rate" => scene.rate = rate(value)?,
                "include_hidden" => scene.include_hidden = flag(name, value)?,
                "bake_transforms" => scene.bake_transforms = flag(name, value)?,
                "lights" => scene.lights = flag(name, value)?,
                "cameras" => scene.cameras = flag(name, value)?,
                "fbx_unit" => {
                    fbx.unit = match value.as_str() {
                        Some("metres") => FbxUnit::Metres,
                        Some("centimetres") => FbxUnit::Centimetres,
                        _ => return Err(format!(
                            "the option \"fbx_unit\" must be \"metres\" or \"centimetres\", not {value}"
                        )),
                    }
                }
                "fbx_colors" => {
                    fbx.color_space = match value.as_str() {
                        Some("srgb") => FbxColorSpace::Srgb,
                        Some("linear") => FbxColorSpace::Linear,
                        _ => {
                            return Err(format!(
                            "the option \"fbx_colors\" must be \"srgb\" or \"linear\", not {value}"
                        ))
                        }
                    }
                }
                "gltf_node_visibility" => gltf.node_visibility = flag(name, value)?,
                "gltf_animate_opacity" => gltf.animate_opacity = flag(name, value)?,
                _ => return Err(format!("unknown export option \"{name}\"")),
            }
        }
        if !scene.animation {
            scene.end = scene.start;
        }
        Ok(Self { scene, gltf, fbx })
    }
}

fn flag(name: &str, value: &Value) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| format!("the option \"{name}\" must be true or false, not {value}"))
}

fn time(name: &str, value: &Value) -> Result<Flicks, String> {
    value
        .as_i64()
        .filter(|flicks| *flicks >= 0)
        .ok_or_else(|| format!("the option \"{name}\" must be a time in flicks, not {value}"))
}

/// A sampling rate: frames per second as a number, or `{num, den}`.
fn rate(value: &Value) -> Result<FrameRate, String> {
    let rate = match value {
        Value::Object(parts) => {
            let part = |key: &str| parts.get(key).and_then(Value::as_u64).unwrap_or(0) as u32;
            FrameRate::new(part("num"), part("den"))
        }
        _ => match value.as_f64() {
            Some(fps) if fps > 0.0 => FrameRate::from_f64(fps),
            _ => FrameRate::new(0, 0),
        },
    };
    if rate.is_valid() {
        Ok(rate)
    } else {
        Err(format!(
            "the option \"rate\" must be frames per second or {{num, den}}, not {value}"
        ))
    }
}

/// Writes `doc` to `path` in the format its extension names.
fn write(doc: &Document, path: &Path, settings: &Settings) -> Result<ExportReport, String> {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let scene = &settings.scene;
    let result: Result<ExportReport, ExportError> = match extension.as_str() {
        "glb" => write_glb(doc, path, scene, &settings.gltf),
        "obj" => write_obj(doc, path, scene.start, scene.include_hidden),
        "usda" => write_usda(doc, path, scene, &UsdOptions::default()),
        "usdz" => write_usdz(doc, path, scene, &UsdOptions::default()),
        "fbx" => write_fbx(doc, path, scene, &settings.fbx),
        _ => {
            return Err(format!(
                "{} does not end in .glb, .obj, .usda, .usdz or .fbx",
                path.display()
            ))
        }
    };
    result.map_err(|error| error.to_string())
}

#[godot_api(secondary)]
impl LimnerScene {
    /// Exports the document to `path`; the extension chooses the format: `.glb`,
    /// `.obj`, `.usda`, `.usdz` or `.fbx`.
    ///
    /// `options` may hold `animation` (false: a still at `start`), `start` and `end`
    /// (flicks; the working range when left out), `rate` (frames per second or
    /// `{num, den}`; the document's when left out), `include_hidden`,
    /// `bake_transforms`, `lights`, `cameras`, and for single formats `fbx_unit`
    /// ("metres" or "centimetres"), `fbx_colors` ("srgb" or "linear"),
    /// `gltf_node_visibility` and `gltf_animate_opacity`.
    ///
    /// Returns `{ok, error, nodes, meshes, vertices, triangles, animation_frames,
    /// bytes, warnings}`; `warnings` says in plain words what the format could not
    /// hold, such as sounds. The call returns when the file is written, which takes
    /// seconds for a large document.
    #[func]
    fn export_file(&self, path: GString, options: VarDictionary) -> VarDictionary {
        let doc = self.session.doc();
        let path = path.to_string();
        let written = Settings::read(doc, &json_from_variant(&options.to_variant()))
            .and_then(|settings| write(doc, Path::new(&path), &settings));
        let (report, error) = match written {
            Ok(report) => (report, String::new()),
            Err(error) => (ExportReport::default(), error),
        };
        let warnings: PackedStringArray = report.warnings.iter().map(GString::from).collect();
        vdict! {
            "ok" => error.is_empty(),
            "error" => error,
            "nodes" => report.nodes as i64,
            "meshes" => report.meshes as i64,
            "vertices" => report.vertices as i64,
            "triangles" => report.triangles as i64,
            "animation_frames" => report.animation_frames as i64,
            "bytes" => report.bytes as i64,
            "warnings" => &warnings,
        }
    }

    /// Saves a copy of the document and the editor's working state at `path` (a
    /// `.limn` folder or a `.limnz` file) without making that the document's place
    /// and without counting its changes as saved. A render in another process works
    /// from such a copy. Returns `{ok, error, bytes_written}`.
    #[func]
    fn save_copy(&self, path: GString) -> VarDictionary {
        let state = self.session.editor_state();
        match limner::save_with_state(
            self.session.doc(),
            Path::new(&path.to_string()),
            &state,
            None,
        ) {
            Ok(report) => {
                vdict! { "ok" => true, "error" => "", "bytes_written" => report.bytes_written as i64 }
            }
            Err(error) => {
                vdict! { "ok" => false, "error" => error.to_string(), "bytes_written" => 0 }
            }
        }
    }
}
