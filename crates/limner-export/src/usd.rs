//! USD writers: a `.usda` text layer, and a `.usdz` package that holds it.
//!
//! USD's defaults can be set to exactly Limner's space (metres, Y up, right-handed),
//! so nothing is converted. The mapping of each Limner concept is described in
//! `docs/export.md`; the short version:
//!
//! * one prim per layer under a single default prim, with `translate`, `orient` and
//!   `scale` ops; a mirrored layer has a negative scale on its flip axis;
//! * one `Mesh` prim per kind of surface of a drawing, with `points`, `normals`,
//!   linear `displayColor` and `displayOpacity`;
//! * animation as time samples, one time code per sampled frame; clips and
//!   frame-by-frame drawings as `visibility`, which USD has natively, so nothing is
//!   scaled to zero here;
//! * lights as UsdLux prims, cameras as `Camera` prims, and one small
//!   `UsdPreviewSurface` material per look that reads `displayColor`;
//! * sounds as UsdMedia `SpatialAudio` prims under their layer's prim, one per
//!   stretch of the range in which the sound plays, referring to the audio file
//!   (written beside a `.usda`, packed inside a `.usdz`).
//!
//! **Winding.** The scene's triangles are counter-clockwise seen from outside, in a
//! right-handed space, so every mesh says `orientation = "rightHanded"`: that is USD's
//! name for "the front is the side from which the vertices run counter-clockwise".
//! A mirrored layer keeps this value. USD flips the facing of anything under a
//! transform with a negative determinant by itself, and a baked scene arrives here
//! with its winding already reversed.

use crate::common::{
    camera_aspect, camera_fov, light_color_and_intensity, printable, transforms, visibility,
    GENERATOR,
};
use crate::gltf::{kept_frames, CAMERA_ZFAR, CAMERA_ZNEAR};
use crate::scene::{
    ExportAudio, ExportCamera, ExportLight, ExportMedia, ExportNode, ExportPaint, ExportScene,
    ExportSound, NodeContent, Sampled,
};
use crate::{ExportError, ExportReport};
use limner_core::model::{
    AttenuationMode, LightKind, Shading, SoundKind, SoundModifier, OPAQUE_THRESHOLD,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::sync::Arc;

/// Height of the camera's film back, in USD's camera units (tenths of a scene unit).
/// Only its ratio to the focal length matters; 24 is the height of a full-frame sensor
/// in millimetres, so the numbers look familiar in tools that show them.
pub const CAMERA_VERTICAL_APERTURE: f32 = 24.0;

/// Radius written for point and spot lights. They are marked `treatAsPoint`; the
/// radius is for renderers that ignore the mark, and `normalize` keeps their
/// brightness independent of it.
pub const LIGHT_RADIUS: f32 = 0.01;

/// File data in a USDZ package starts on a multiple of this many bytes.
pub const USDZ_ALIGNMENT: usize = 64;

/// The audio encodings the USDZ specification allows inside a package, as file
/// extensions. Limner's sounds arrive as WAV, MP3 or Ogg Vorbis; Ogg is not allowed.
pub const USDZ_AUDIO_FORMATS: [&str; 3] = ["m4a", "mp3", "wav"];

/// Where a layer's audio files go, relative to the layer.
const AUDIO_FOLDER: &str = "audio";

/// A file a USD layer refers to: written beside a `.usda`, packed inside a `.usdz`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsdAsset {
    /// The path relative to the layer, with forward slashes, as the layer names it.
    pub path: String,
    pub bytes: Arc<[u8]>,
}

/// A USD text layer and the files it refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsdaOutput {
    pub text: String,
    /// The audio of sound layers, each file once however many layers play it.
    pub assets: Vec<UsdAsset>,
    pub report: ExportReport,
}

/// Choices that only concern USD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsdOptions {
    /// Write `normals`. Without them a viewer computes its own, which facets the
    /// seven-sided tubes.
    pub normals: bool,
    /// Write the `UsdPreviewSurface` materials and bind them. Without them a viewer
    /// falls back to `displayColor` and `displayOpacity`, which is all some need.
    pub materials: bool,
}

impl Default for UsdOptions {
    fn default() -> Self {
        Self {
            normals: true,
            materials: true,
        }
    }
}

/// A number as the shortest decimal that reads back as the same `f32`.
fn f(x: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

fn tuple3(v: [f32; 3]) -> String {
    format!("({}, {}, {})", f(v[0]), f(v[1]), f(v[2]))
}

/// A quoted USD string.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Turns a layer name into a USD prim name: ASCII letters, digits and underscores,
/// not starting with a digit. `fallback` is used when nothing usable is left.
fn identifier(name: &str, fallback: &str) -> String {
    let mut out: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.chars().all(|c| c == '_') {
        out = fallback.to_owned();
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// A prim name that no sibling has yet: the wanted one, or failing that the wanted
/// one with `tag` appended, then with a counter.
fn unique(wanted: String, tag: u64, taken: &mut HashSet<String>) -> String {
    let mut name = wanted.clone();
    if taken.contains(&name) {
        name = format!("{wanted}_{tag}");
    }
    let mut counter = 2;
    while taken.contains(&name) {
        name = format!("{wanted}_{tag}_{counter}");
        counter += 1;
    }
    taken.insert(name.clone());
    name
}

/// The name of the default prim, which is also the root layer's file name in a package.
fn default_prim_name(scene: &ExportScene) -> String {
    identifier(&scene.name, "Limner_scene")
}

/// One of the four looks a mesh can have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Look {
    lit: bool,
    see_through: bool,
}

impl Look {
    fn name(self) -> &'static str {
        match (self.lit, self.see_through) {
            (false, false) => "unlit",
            (false, true) => "unlit_see_through",
            (true, false) => "lit",
            (true, true) => "lit_see_through",
        }
    }
}

struct Writer<'a> {
    scene: &'a ExportScene,
    options: &'a UsdOptions,
    /// Writing the layer of a `.usdz`, which may hold only some audio encodings.
    package: bool,
    out: String,
    root: String,
    looks: BTreeSet<Look>,
    report: ExportReport,
    assets: Vec<UsdAsset>,
    /// The asset path of each media item already written, by media id.
    asset_paths: HashMap<u64, String>,
    asset_names: HashSet<String>,
}

/// The name of a sound's file in the audio folder: the media item's file name made
/// safe for every file system and asset resolver (ASCII letters, digits, `-` and
/// `_`), with the extension of the encoding. Unique among the files written.
fn audio_file_name(name: &str, id: u64, format: &str, taken: &mut HashSet<String>) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = match base.rfind('.') {
        Some(dot) if dot > 0 => &base[..dot],
        _ => base,
    };
    let mut stem: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.chars().all(|c| c == '_') {
        stem = "sound".to_owned();
    }
    let mut file = format!("{stem}.{format}");
    if taken.contains(&file) {
        file = format!("{stem}_{id}.{format}");
    }
    let mut counter = 2;
    while taken.contains(&file) {
        file = format!("{stem}_{id}_{counter}.{format}");
        counter += 1;
    }
    taken.insert(file.clone());
    file
}

/// UsdMedia's name for how a sound is heard: at its place, or around the listener.
fn aural_mode(kind: SoundKind) -> &'static str {
    match kind {
        SoundKind::Positional => "spatial",
        // An ambisonic recording surrounds the listener; it has no single place.
        SoundKind::Flat | SoundKind::Ambisonic => "nonSpatial",
    }
}

impl<'a> Writer<'a> {
    fn pad(&mut self, depth: usize) {
        for _ in 0..depth {
            self.out.push_str("    ");
        }
    }

    fn line(&mut self, depth: usize, text: &str) {
        self.pad(depth);
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// Opens `def <kind> "<name>"`, with a metadata block when there is any.
    fn open(&mut self, depth: usize, kind: &str, name: &str, metadata: &[String]) {
        if !self.out.ends_with("{\n") {
            self.out.push('\n');
        }
        self.pad(depth);
        let _ = write!(self.out, "def {kind} \"{name}\"");
        if metadata.is_empty() {
            self.out.push('\n');
        } else {
            self.out.push_str(" (\n");
            for entry in metadata {
                for row in entry.lines() {
                    self.line(depth + 1, row);
                }
            }
            self.line(depth, ")");
        }
        self.line(depth, "{");
    }

    fn close(&mut self, depth: usize) {
        self.line(depth, "}");
    }

    /// Per-frame values as time samples. The time code of a sampled frame is its
    /// index, so codes run from zero at the start of the export range. Runs of equal
    /// values are merged the way the glTF writer merges them: both ends of a hold for
    /// values USD interpolates, one sample per change for `held` ones (tokens).
    fn time_samples<T: PartialEq>(
        &mut self,
        depth: usize,
        declaration: &str,
        values: &[T],
        held: bool,
        format: impl Fn(&T) -> String,
    ) {
        self.pad(depth);
        let _ = writeln!(self.out, "{declaration}.timeSamples = {{");
        for frame in kept_frames(values, held) {
            self.pad(depth + 1);
            let _ = writeln!(self.out, "{frame}: {},", format(&values[frame as usize]));
        }
        self.line(depth, "}");
        self.report.animation_channels += 1;
    }

    /// A property with its value on the first frame and, when it changes, its samples.
    fn sampled<T: PartialEq>(
        &mut self,
        depth: usize,
        declaration: &str,
        values: &[T],
        held: bool,
        format: impl Fn(&T) -> String,
    ) {
        self.pad(depth);
        let _ = writeln!(self.out, "{declaration} = {}", format(&values[0]));
        if values.iter().any(|v| *v != values[0]) {
            self.time_samples(depth, declaration, values, held, format);
        }
    }

    /// `token visibility`: authored only for prims that are hidden at some point.
    fn visibility(&mut self, depth: usize, shown: &[bool]) {
        if shown.iter().all(|s| *s) {
            return;
        }
        let token = |s: &bool| if *s { "\"inherited\"" } else { "\"invisible\"" }.to_owned();
        self.sampled(depth, "token visibility", shown, true, token);
    }

    /// Translate, orient and scale ops for a layer transform. Ops that would do
    /// nothing are left out, and the rest keep USD's usual order.
    fn xform_ops(&mut self, depth: usize, node: &ExportNode) {
        let samples = transforms(self.scene, node);
        let translations: Vec<[f64; 3]> =
            samples.iter().map(|x| x.translation.to_array()).collect();
        let mut rotations: Vec<[f32; 4]> = Vec::with_capacity(samples.len());
        for x in &samples {
            let q = x.rotation;
            let mut r = [q.w as f32, q.x as f32, q.y as f32, q.z as f32];
            // q and -q are the same rotation, but interpolating between them goes the
            // long way round: stay on the previous sample's hemisphere.
            if let Some(previous) = rotations.last() {
                let dot: f32 = previous.iter().zip(&r).map(|(a, b)| a * b).sum();
                if dot < 0.0 {
                    r = r.map(|c| -c);
                }
            }
            rotations.push(r);
        }
        let scales: Vec<[f32; 3]> = samples
            .iter()
            .map(|x| (x.flip.signs() * x.scale).as_vec3().to_array())
            .collect();

        let mut order = Vec::new();
        if translations.iter().any(|t| *t != [0.0; 3]) {
            self.sampled(
                depth,
                "double3 xformOp:translate",
                &translations,
                false,
                |t| format!("({}, {}, {})", t[0], t[1], t[2]),
            );
            order.push("\"xformOp:translate\"");
        }
        if rotations.iter().any(|r| *r != [1.0, 0.0, 0.0, 0.0]) {
            // USD writes quaternions real part first.
            self.sampled(depth, "quatf xformOp:orient", &rotations, false, |r| {
                format!("({}, {}, {}, {})", r[0], r[1], r[2], r[3])
            });
            order.push("\"xformOp:orient\"");
        }
        if scales.iter().any(|s| *s != [1.0; 3]) {
            self.sampled(depth, "float3 xformOp:scale", &scales, false, |s| {
                tuple3(*s)
            });
            order.push("\"xformOp:scale\"");
        }
        if !order.is_empty() {
            let line = format!("uniform token[] xformOpOrder = [{}]", order.join(", "));
            self.line(depth, &line);
        }
    }

    /// One scene mesh as a `Mesh` prim. `opacity` is the layer's, which USD has no
    /// place for other than the vertices.
    fn mesh(
        &mut self,
        depth: usize,
        name: &str,
        mesh_index: usize,
        shading: Shading,
        opacity: f32,
    ) {
        let scene = self.scene;
        let mesh = &scene.meshes[mesh_index];
        let look = Look {
            lit: shading == Shading::Lit,
            // Blending is only for what is really see-through.
            see_through: mesh.has_transparency || opacity < OPAQUE_THRESHOLD,
        };
        let mut metadata = Vec::new();
        if self.options.materials {
            metadata.push("prepend apiSchemas = [\"MaterialBindingAPI\"]".to_owned());
            self.looks.insert(look);
        }
        self.open(depth, "Mesh", name, &metadata);
        let d = depth + 1;

        self.line(
            d,
            &format!("uniform bool doubleSided = {}", u8::from(mesh.double_sided)),
        );
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in &mesh.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        self.line(
            d,
            &format!("float3[] extent = [{}, {}]", tuple3(min), tuple3(max)),
        );

        let out = &mut self.out;
        let pad = "    ".repeat(d);
        let _ = write!(out, "{pad}int[] faceVertexCounts = [");
        for i in 0..mesh.triangle_count() {
            out.push_str(if i == 0 { "3" } else { ", 3" });
        }
        let _ = write!(out, "]\n{pad}int[] faceVertexIndices = [");
        for (i, index) in mesh.indices.iter().enumerate() {
            let _ = write!(out, "{}{index}", if i == 0 { "" } else { ", " });
        }
        out.push_str("]\n");
        if self.options.materials {
            let _ = writeln!(
                out,
                "{pad}rel material:binding = </{}/Materials/{}>",
                self.root,
                look.name()
            );
        }
        let triples = |out: &mut String, values: &mut dyn Iterator<Item = [f32; 3]>| {
            for (i, v) in values.enumerate() {
                let _ = write!(
                    out,
                    "{}({}, {}, {})",
                    if i == 0 { "" } else { ", " },
                    f(v[0]),
                    f(v[1]),
                    f(v[2])
                );
            }
        };
        if self.options.normals {
            let _ = write!(out, "{pad}normal3f[] normals = [");
            triples(out, &mut mesh.normals.iter().copied());
            out.push_str("] (interpolation = \"vertex\")\n");
        }
        let _ = writeln!(out, "{pad}uniform token orientation = \"rightHanded\"");
        let _ = write!(out, "{pad}point3f[] points = [");
        triples(out, &mut mesh.positions.iter().copied());
        // displayColor is linear by definition, like the scene's colours, so the
        // numbers go out as they are; negative values are the only thing removed.
        let _ = write!(out, "]\n{pad}color3f[] primvars:displayColor = [");
        triples(
            out,
            &mut mesh
                .colors
                .iter()
                .map(|c| [f(c[0]).max(0.0), f(c[1]).max(0.0), f(c[2]).max(0.0)]),
        );
        let _ = write!(
            out,
            "] (interpolation = \"vertex\")\n{pad}float[] primvars:displayOpacity = ["
        );
        for (i, c) in mesh.colors.iter().enumerate() {
            let alpha = (f(c[3]) * opacity).clamp(0.0, 1.0);
            let _ = write!(out, "{}{alpha}", if i == 0 { "" } else { ", " });
        }
        out.push_str("] (interpolation = \"vertex\")\n");
        let _ = writeln!(out, "{pad}uniform token subdivisionScheme = \"none\"");
        self.close(depth);

        self.report.meshes += 1;
        self.report.vertices += mesh.vertex_count();
        self.report.triangles += mesh.triangle_count();
    }

    /// The meshes of one drawing as children of the prim at `depth - 1`.
    fn drawing_meshes(&mut self, depth: usize, meshes: &[usize], shading: Shading, opacity: f32) {
        let mut taken = HashSet::new();
        for mesh_index in meshes {
            let mesh = &self.scene.meshes[*mesh_index];
            let kind = match (mesh.double_sided, mesh.has_transparency) {
                (false, false) => "Tubes",
                (true, false) => "Flat",
                (false, true) => "TubesSeeThrough",
                (true, true) => "FlatSeeThrough",
            };
            let name = unique(kind.to_owned(), *mesh_index as u64, &mut taken);
            self.mesh(depth, &name, *mesh_index, shading, opacity);
        }
    }

    fn paint(&mut self, depth: usize, node: &ExportNode, prim_name: &str, paint: &ExportPaint) {
        let opacity = f(crate::common::peak_opacity(node)).clamp(0.0, 1.0);
        // The usual case, a layer showing one drawing throughout, needs no extra prim.
        if let [only] = paint.drawings.as_slice() {
            if only.shown == Sampled::Constant(true) {
                self.drawing_meshes(depth, &only.meshes, paint.shading, opacity);
                return;
            }
        }
        // Frame-by-frame: one child per drawing, switched by its visibility.
        let frames = self.scene.frame_count();
        for drawing in &paint.drawings {
            let name = format!("{prim_name}_{:03}", drawing.ordinal);
            let metadata = vec![format!(
                "customData = {{\n    dictionary limner = {{\n        int drawing = {}\n        int64 drawingId = {}\n        int64 layerId = {}\n    }}\n}}",
                drawing.ordinal, drawing.drawing.0, node.layer.0
            )];
            self.open(depth, "Xform", &name, &metadata);
            let shown: Vec<bool> = (0..frames).map(|k| *drawing.shown.at(k)).collect();
            self.visibility(depth + 1, &shown);
            self.drawing_meshes(depth + 1, &drawing.meshes, paint.shading, opacity);
            self.close(depth);
            self.report.nodes += 1;
        }
    }

    fn light(&mut self, depth: usize, light: &ExportLight) {
        let (color, intensity) = light_color_and_intensity(light);
        self.line(depth, &format!("color3f inputs:color = {}", tuple3(color)));
        self.line(depth, &format!("float inputs:intensity = {intensity}"));
        if light.kind != LightKind::Directional {
            self.line(depth, "bool inputs:normalize = 1");
            self.line(depth, &format!("float inputs:radius = {LIGHT_RADIUS}"));
            self.line(depth, "bool treatAsPoint = 1");
        }
        if light.kind == LightKind::Spot {
            // UsdLux measures the cone like Limner does: degrees off the axis.
            let angle = f(light.spot_angle).clamp(0.0, 90.0);
            let softness = f(light.spot_blend).clamp(0.0, 1.0);
            self.line(depth, &format!("float inputs:shaping:cone:angle = {angle}"));
            self.line(
                depth,
                &format!("float inputs:shaping:cone:softness = {softness}"),
            );
        }
        self.line(
            depth,
            &format!("bool inputs:shadow:enable = {}", u8::from(light.shadows)),
        );
    }

    fn camera(&mut self, depth: usize, camera: &ExportCamera) {
        let half = (camera_fov(camera).to_radians() * 0.5).tan();
        let focal = CAMERA_VERTICAL_APERTURE * 0.5 / half;
        let horizontal = CAMERA_VERTICAL_APERTURE * camera_aspect(camera);
        self.line(
            depth,
            &format!("float2 clippingRange = ({CAMERA_ZNEAR}, {CAMERA_ZFAR})"),
        );
        self.line(depth, &format!("float focalLength = {focal}"));
        self.line(depth, &format!("float horizontalAperture = {horizontal}"));
        self.line(depth, "token projection = \"perspective\"");
        self.line(
            depth,
            &format!("float verticalAperture = {CAMERA_VERTICAL_APERTURE}"),
        );
    }

    /// The path the layer gives a sound's audio, writing the file the first time.
    fn audio_asset(&mut self, sound: &ExportSound, audio: &ExportAudio) -> String {
        let id = sound.media.media.map_or(0, |m| m.0);
        if let Some(path) = self.asset_paths.get(&id) {
            return path.clone();
        }
        let file = audio_file_name(&sound.media.name, id, &audio.format, &mut self.asset_names);
        let path = format!("{AUDIO_FOLDER}/{file}");
        self.assets.push(UsdAsset {
            path: path.clone(),
            bytes: audio.bytes.clone(),
        });
        self.asset_paths.insert(id, path.clone());
        path
    }

    /// One `SpatialAudio` prim per stretch of the range in which the sound plays.
    ///
    /// A play that lasts to the end of the range is open-ended (`onceFromStart` or
    /// `loopFromStart`): the sound goes on as it would in Limner. One that stops
    /// before (its clip ends, its sequence loops back, its audio runs out) has an
    /// `endTime` on the first frame it is no longer heard (`onceFromStartToEnd` or
    /// `loopFromStartToEnd`). `gain` is Limner's gain times the layer's effective
    /// opacity, with time samples when the opacity changes, as Limner's player does.
    fn sound(&mut self, depth: usize, node: &ExportNode, sound: &ExportSound) {
        if sound.plays.is_empty() {
            return;
        }
        let Some(audio) = &sound.audio else {
            if sound.media.media.is_some() {
                self.report.warnings.push(format!(
                    "the sound layer \"{}\" was left out: its audio cannot be written as a file",
                    printable(&node.name)
                ));
            }
            return;
        };
        if self.package && !USDZ_AUDIO_FORMATS.contains(&audio.format.as_str()) {
            self.report.warnings.push(format!(
                "the sound layer \"{}\" was left out: a .usdz package may hold only M4A, MP3 \
                 and WAV audio, and this sound is {}; export a .usda to keep it, or convert it",
                printable(&node.name),
                audio.format.to_ascii_uppercase()
            ));
            return;
        }
        let path = self.audio_asset(sound, audio);
        let frames = self.scene.frame_count();
        // In single precision, which is what Limner holds, so 0.8 is written as 0.8.
        let gains: Vec<f32> = (0..frames)
            .map(|k| f(sound.gain * *node.opacity.at(k)).max(0.0))
            .collect();
        for (i, play) in sound.plays.iter().enumerate() {
            // A sound layer has no children, so these names are free.
            let name = if sound.plays.len() == 1 {
                "Audio".to_owned()
            } else {
                format!("Audio_{i:03}")
            };
            self.open(depth, "SpatialAudio", &name, &[]);
            let d = depth + 1;
            let open_ended = play.last + 1 >= frames;
            self.line(
                d,
                &format!("uniform token auralMode = \"{}\"", aural_mode(sound.kind)),
            );
            if !open_ended {
                self.line(d, &format!("uniform timecode endTime = {}", play.last + 1));
            }
            self.line(d, &format!("uniform asset filePath = @{path}@"));
            self.sampled(d, "double gain", &gains, false, |g| g.to_string());
            let offset = if play.offset.is_finite() {
                play.offset.max(0.0)
            } else {
                0.0
            };
            self.line(d, &format!("uniform double mediaOffset = {offset}"));
            let mode = match (sound.looped, open_ended) {
                (false, true) => "onceFromStart",
                (false, false) => "onceFromStartToEnd",
                (true, true) => "loopFromStart",
                (true, false) => "loopFromStartToEnd",
            };
            self.line(d, &format!("uniform token playbackMode = \"{mode}\""));
            self.line(d, &format!("uniform timecode startTime = {}", play.first));
            self.close(depth);
            self.report.sounds += 1;
        }
    }

    fn node(&mut self, depth: usize, index: usize, taken: &mut HashSet<String>) {
        let scene = self.scene;
        let node = &scene.nodes[index];
        let name = unique(
            identifier(&node.name, node.kind.as_str()),
            node.layer.0,
            taken,
        );

        let mut custom = format!(
            "customData = {{\n    dictionary limner = {{\n        string kind = {}\n        int64 layerId = {}\n",
            quoted(node.kind.as_str()),
            node.layer.0
        );
        let mut field = |declaration: &str, value: String| {
            let _ = writeln!(custom, "        {declaration} = {value}");
        };
        match &node.content {
            NodeContent::Media(ExportMedia {
                media,
                name,
                source_path,
            }) => {
                field("string media", quoted(name));
                if let Some(id) = media {
                    field("int64 mediaId", id.0.to_string());
                }
                field("string name", quoted(&node.name));
                field("string sourcePath", quoted(source_path));
            }
            NodeContent::Light(light) => {
                field("float energy", f(light.energy).to_string());
                field("string name", quoted(&node.name));
                field("float range", f(light.range).to_string());
                field("float worldRange", f(light.world_range).to_string());
            }
            NodeContent::Sound(sound) => {
                let media = &sound.media;
                field("float gain", f(sound.gain).to_string());
                field("bool looped", u8::from(sound.looped).to_string());
                field("string media", quoted(&media.name));
                if let Some(id) = media.media {
                    field("int64 mediaId", id.0.to_string());
                }
                field("string name", quoted(&node.name));
                let attenuation = &sound.attenuation;
                let mode = match attenuation.mode {
                    AttenuationMode::None => "none",
                    AttenuationMode::Linear => "linear",
                    AttenuationMode::Logarithmic => "logarithmic",
                };
                field("string soundAttenuation", quoted(mode));
                field("float soundAttenuationMax", f(attenuation.max).to_string());
                field("float soundAttenuationMin", f(attenuation.min).to_string());
                let kind = match sound.kind {
                    SoundKind::Flat => "flat",
                    SoundKind::Positional => "positional",
                    SoundKind::Ambisonic => "ambisonic",
                };
                field("string soundKind", quoted(kind));
                match sound.modifier {
                    SoundModifier::None => field("string soundModifier", quoted("none")),
                    SoundModifier::Cone {
                        inner,
                        band,
                        outside,
                    } => {
                        field("string soundModifier", quoted("cone"));
                        field("float soundModifierBand", f(band).to_string());
                        field("float soundModifierInner", f(inner).to_string());
                        field("float soundModifierOutside", f(outside).to_string());
                    }
                    SoundModifier::Frustum {
                        inner_h,
                        inner_v,
                        band,
                        outside,
                    } => {
                        field("string soundModifier", quoted("frustum"));
                        field("float soundModifierBand", f(band).to_string());
                        field("float soundModifierInnerH", f(inner_h).to_string());
                        field("float soundModifierInnerV", f(inner_v).to_string());
                        field("float soundModifierOutside", f(outside).to_string());
                    }
                }
                field("string sourcePath", quoted(&media.source_path));
            }
            _ => field("string name", quoted(&node.name)),
        }
        custom.push_str("    }\n}");

        let mut metadata = Vec::new();
        let kind = match &node.content {
            NodeContent::Light(light) => {
                metadata.push(
                    if light.kind == LightKind::Spot {
                        "prepend apiSchemas = [\"ShapingAPI\", \"ShadowAPI\"]"
                    } else {
                        "prepend apiSchemas = [\"ShadowAPI\"]"
                    }
                    .to_owned(),
                );
                if light.kind == LightKind::Directional {
                    "DistantLight"
                } else {
                    "SphereLight"
                }
            }
            NodeContent::Camera(_) => "Camera",
            _ => "Xform",
        };
        metadata.push(custom);
        self.open(depth, kind, &name, &metadata);
        let d = depth + 1;

        match &node.content {
            NodeContent::Light(light) => self.light(d, light),
            NodeContent::Camera(camera) => self.camera(d, camera),
            _ => {}
        }
        self.visibility(d, &visibility(scene, node));
        self.xform_ops(d, node);

        let mut child_names = HashSet::new();
        match &node.content {
            NodeContent::Paint(paint) => self.paint(d, node, &name, paint),
            NodeContent::Sound(sound) => self.sound(d, node, sound),
            _ => {}
        }
        for child in &node.children {
            self.node(d, *child, &mut child_names);
        }
        self.close(depth);
        self.report.nodes += 1;
    }

    /// The materials that were bound, as a scope beside the layers.
    fn materials(&mut self, depth: usize) {
        if self.looks.is_empty() {
            return;
        }
        self.open(depth, "Scope", "Materials", &[]);
        let looks: Vec<Look> = self.looks.iter().copied().collect();
        for look in looks {
            let path = format!("/{}/Materials/{}", self.root, look.name());
            self.open(depth + 1, "Material", look.name(), &[]);
            let d = depth + 2;
            self.line(
                d,
                &format!("token outputs:surface.connect = <{path}/Surface.outputs:surface>"),
            );

            self.open(d, "Shader", "Surface", &[]);
            let s = d + 1;
            self.line(s, "uniform token info:id = \"UsdPreviewSurface\"");
            let color = format!("<{path}/Color.outputs:result>");
            if look.lit {
                self.line(s, &format!("color3f inputs:diffuseColor.connect = {color}"));
            } else {
                // Unlit: the paint is the light. Black diffuse and no specular leave
                // nothing for scene lights to add; the colour is emitted instead.
                self.line(s, "color3f inputs:diffuseColor = (0, 0, 0)");
                self.line(
                    s,
                    &format!("color3f inputs:emissiveColor.connect = {color}"),
                );
                self.line(s, "color3f inputs:specularColor = (0, 0, 0)");
                self.line(s, "int inputs:useSpecularWorkflow = 1");
            }
            self.line(s, "float inputs:metallic = 0");
            if look.see_through {
                self.line(
                    s,
                    &format!("float inputs:opacity.connect = <{path}/Opacity.outputs:result>"),
                );
            }
            self.line(s, "float inputs:roughness = 1");
            self.line(s, "token outputs:surface");
            self.close(d);

            self.open(d, "Shader", "Color", &[]);
            self.line(s, "uniform token info:id = \"UsdPrimvarReader_float3\"");
            self.line(s, "float3 inputs:fallback = (1, 1, 1)");
            self.line(s, "string inputs:varname = \"displayColor\"");
            self.line(s, "float3 outputs:result");
            self.close(d);

            if look.see_through {
                self.open(d, "Shader", "Opacity", &[]);
                self.line(s, "uniform token info:id = \"UsdPrimvarReader_float\"");
                self.line(s, "float inputs:fallback = 1");
                self.line(s, "string inputs:varname = \"displayOpacity\"");
                self.line(s, "float outputs:result");
                self.close(d);
            }
            self.close(depth + 1);
        }
        self.close(depth);
    }

    fn finish(mut self) -> UsdaOutput {
        let scene = self.scene;
        let frames = scene.frame_count();
        let rate = if scene.rate.is_valid() {
            scene.rate.as_f64()
        } else {
            24.0
        };

        self.out.push_str("#usda 1.0\n(\n");
        let _ = writeln!(self.out, "    defaultPrim = \"{}\"", self.root);
        let _ = writeln!(self.out, "    doc = {}", quoted(GENERATOR));
        if scene.is_animated() {
            let _ = writeln!(self.out, "    endTimeCode = {}", frames - 1);
        }
        let _ = writeln!(self.out, "    framesPerSecond = {rate}");
        self.out.push_str("    metersPerUnit = 1\n");
        if scene.is_animated() {
            self.out.push_str("    startTimeCode = 0\n");
        }
        let _ = writeln!(self.out, "    timeCodesPerSecond = {rate}");
        self.out.push_str("    upAxis = \"Y\"\n");
        let _ = write!(
            self.out,
            "    customLayerData = {{\n        dictionary limner = {{\n            float3 background = {}\n            bool baked = {}\n            int2 frameRate = ({}, {})\n            int frames = {}\n            double startSeconds = {}\n            string title = {}\n        }}\n    }}\n",
            tuple3(scene.background),
            u8::from(scene.baked),
            scene.rate.num,
            scene.rate.den,
            frames,
            limner_core::time::to_seconds(scene.times[0]),
            quoted(&printable(&scene.name)),
        );
        self.out.push_str(")\n");

        let root = self.root.clone();
        self.open(0, "Xform", &root, &[]);
        // "Materials" is taken before any layer can claim it.
        let mut taken = HashSet::from(["Materials".to_owned()]);
        for index in &scene.roots {
            self.node(1, *index, &mut taken);
        }
        self.materials(1);
        self.close(0);

        self.report.animation_frames = if scene.is_animated() { frames } else { 0 };
        self.report.bytes = self.out.len() as u64;
        UsdaOutput {
            text: self.out,
            assets: self.assets,
            report: self.report,
        }
    }
}

fn write_layer(scene: &ExportScene, options: &UsdOptions, package: bool) -> UsdaOutput {
    Writer {
        scene,
        options,
        package,
        out: String::new(),
        root: default_prim_name(scene),
        looks: BTreeSet::new(),
        report: ExportReport::default(),
        assets: Vec::new(),
        asset_paths: HashMap::new(),
        asset_names: HashSet::new(),
    }
    .finish()
}

/// Writes an export scene as a USD text layer (`.usda`).
///
/// Everything sits under one default prim named after the document. Animation is
/// written as time samples at one time code per sampled frame, starting at zero.
/// Sound layers refer to their audio as files under `audio/` beside the layer; use
/// [`scene_to_usda_with_assets`] to have those files too.
pub fn scene_to_usda(
    scene: &ExportScene,
    options: &UsdOptions,
) -> Result<(String, ExportReport), ExportError> {
    scene_to_usda_with_assets(scene, options).map(|output| (output.text, output.report))
}

/// Writes an export scene as a USD text layer together with the audio files its
/// sound layers refer to, which belong beside it at the paths given.
pub fn scene_to_usda_with_assets(
    scene: &ExportScene,
    options: &UsdOptions,
) -> Result<UsdaOutput, ExportError> {
    Ok(write_layer(scene, options, false))
}

/// Writes an export scene as a USDZ package: an uncompressed zip whose first file is
/// the layer [`scene_to_usda`] writes, named after the default prim, followed by the
/// audio files of its sounds. Sounds whose audio a package may not hold (Ogg Vorbis)
/// are left out with a warning; everything else is the same as in the `.usda`.
///
/// The root layer is text. The USDZ specification allows that, but some consumers
/// accept only binary (`usdc`) root layers; see `docs/export.md`.
pub fn scene_to_usdz(
    scene: &ExportScene,
    options: &UsdOptions,
) -> Result<(Vec<u8>, ExportReport), ExportError> {
    let UsdaOutput {
        text,
        assets,
        mut report,
    } = write_layer(scene, options, true);
    let name = format!("{}.usda", default_prim_name(scene));
    let mut files: Vec<(&str, &[u8])> = vec![(name.as_str(), text.as_bytes())];
    files.extend(assets.iter().map(|a| (a.path.as_str(), &a.bytes[..])));
    let bytes = usdz_package(&files)?;
    report.bytes = bytes.len() as u64;
    Ok((bytes, report))
}

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

/// CRC-32 as zip uses it.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc = CRC_TABLE[((crc ^ u32::from(*byte)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Packs files into a USDZ archive, in the order given; the first one is the root
/// layer. Names are paths inside the package, with forward slashes.
///
/// The USDZ rules are all followed here: a plain zip without compression or
/// encryption, and every file's data starting on a 64-byte boundary. The padding
/// that achieves this is an extra field in the file's local header, as OpenUSD's own
/// writer does it. Dates are fixed, so the same input always gives the same bytes.
pub fn usdz_package(files: &[(&str, &[u8])]) -> Result<Vec<u8>, ExportError> {
    const LOCAL_HEADER: usize = 30;
    // 1 January 1980, the earliest date zip can express.
    const DOS_TIME: u16 = 0;
    const DOS_DATE: u16 = (1 << 5) | 1;
    // Id of the padding record, the one OpenUSD's writer uses.
    const PADDING_ID: u16 = 0x1986;
    // Bit 11: names are UTF-8.
    let flags = |name: &str| if name.is_ascii() { 0u16 } else { 1 << 11 };
    let too_large = |bytes: usize| ExportError::FileTooLarge {
        format: "USDZ",
        bytes,
    };

    let payload: usize = files
        .iter()
        .map(|(name, data)| LOCAL_HEADER + 46 + 2 * name.len() + 2 * USDZ_ALIGNMENT + data.len())
        .sum();
    // Without zip64, offsets and sizes are 32 bits and counts and names 16.
    if payload + 22 > u32::MAX as usize
        || files.len() > usize::from(u16::MAX)
        || files
            .iter()
            .any(|(name, _)| name.len() > usize::from(u16::MAX))
    {
        return Err(too_large(payload + 22));
    }

    let mut out = Vec::with_capacity(payload + 22);
    let mut directory = Vec::new();
    for (name, data) in files {
        let offset = out.len();
        let crc = crc32(data);
        let unpadded = offset + LOCAL_HEADER + name.len();
        let mut padding = (USDZ_ALIGNMENT - unpadded % USDZ_ALIGNMENT) % USDZ_ALIGNMENT;
        // An extra record needs four bytes for its own header.
        if padding != 0 && padding < 4 {
            padding += USDZ_ALIGNMENT;
        }

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        let mut shared = Vec::with_capacity(26);
        shared.extend_from_slice(&10u16.to_le_bytes()); // version needed: plain stored files
        shared.extend_from_slice(&flags(name).to_le_bytes());
        shared.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        shared.extend_from_slice(&DOS_TIME.to_le_bytes());
        shared.extend_from_slice(&DOS_DATE.to_le_bytes());
        shared.extend_from_slice(&crc.to_le_bytes());
        shared.extend_from_slice(&(data.len() as u32).to_le_bytes()); // compressed size
        shared.extend_from_slice(&(data.len() as u32).to_le_bytes()); // real size
        shared.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&shared);
        out.extend_from_slice(&(padding as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        if padding != 0 {
            out.extend_from_slice(&PADDING_ID.to_le_bytes());
            out.extend_from_slice(&((padding - 4) as u16).to_le_bytes());
            out.resize(out.len() + padding - 4, 0);
        }
        debug_assert_eq!(out.len() % USDZ_ALIGNMENT, 0);
        out.extend_from_slice(data);

        directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes()); // version made by
        directory.extend_from_slice(&shared);
        directory.extend_from_slice(&0u16.to_le_bytes()); // extra length
        directory.extend_from_slice(&0u16.to_le_bytes()); // comment length
        directory.extend_from_slice(&0u16.to_le_bytes()); // disk number
        directory.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        directory.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        directory.extend_from_slice(&(offset as u32).to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }

    let directory_offset = out.len();
    out.extend_from_slice(&directory);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // this disk
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with the directory
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    out.extend_from_slice(&(directory_offset as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    if out.len() > u32::MAX as usize {
        return Err(too_large(out.len()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_identifiers() {
        assert_eq!(identifier("Left arm", "Group"), "Left_arm");
        assert_eq!(identifier("3rd take", "Group"), "_3rd_take");
        assert_eq!(identifier("été", "Group"), "_t_");
        assert_eq!(identifier("", "Paint"), "Paint");
        assert_eq!(identifier("  ", "Paint"), "Paint");
        assert_eq!(identifier("---", "Paint"), "Paint");
        assert_eq!(identifier("a.b/c", "Group"), "a_b_c");
    }

    #[test]
    fn sibling_names_are_made_unique() {
        let mut taken = HashSet::new();
        assert_eq!(unique("Arm".to_owned(), 4, &mut taken), "Arm");
        assert_eq!(unique("Arm".to_owned(), 9, &mut taken), "Arm_9");
        assert_eq!(unique("Arm".to_owned(), 9, &mut taken), "Arm_9_2");
        assert_eq!(unique("Arm_9".to_owned(), 1, &mut taken), "Arm_9_1");
    }

    #[test]
    fn audio_files_get_safe_unique_names() {
        let mut taken = HashSet::new();
        assert_eq!(
            audio_file_name("Rain.WAV", 7, "wav", &mut taken),
            "Rain.wav"
        );
        assert_eq!(
            audio_file_name("Rain.wav", 8, "wav", &mut taken),
            "Rain_8.wav"
        );
        assert_eq!(
            audio_file_name("Rain", 8, "wav", &mut taken),
            "Rain_8_2.wav"
        );
        assert_eq!(
            audio_file_name("C:\\sounds\\bird song (2).mp3", 1, "mp3", &mut taken),
            "bird_song__2_.mp3"
        );
        assert_eq!(audio_file_name("été.ogg", 2, "ogg", &mut taken), "_t_.ogg");
        assert_eq!(audio_file_name("", 3, "wav", &mut taken), "sound.wav");
        assert_eq!(
            audio_file_name(".hidden", 4, "wav", &mut taken),
            "_hidden.wav"
        );
    }

    #[test]
    fn strings_are_escaped() {
        assert_eq!(quoted("a \"b\" \\ c\n"), "\"a \\\"b\\\" \\\\ c\"");
    }

    #[test]
    fn crc_matches_the_published_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }
}
