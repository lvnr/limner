//! Wavefront OBJ writer: a still, in world space.
//!
//! One object per paint layer. Vertex colours use the `v x y z r g b` extension,
//! which Blender, MeshLab, ZBrush and others read; they expect the colours to be
//! sRGB-encoded in `0..=1`, so that is what is written (Limner stores linear).

use crate::scene::{ExportScene, NodeContent};
use crate::ExportReport;
use limner_core::glam::{DAffine3, Vec3};
use std::collections::HashSet;
use std::fmt::Write;

/// Encodes a linear colour component with the sRGB transfer curve.
pub fn linear_to_srgb(linear: f32) -> f32 {
    let c = if linear.is_finite() {
        linear.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// An object name OBJ readers agree on: no whitespace, and unique in the file.
fn object_name(name: &str, layer: u64, taken: &mut HashSet<String>) -> String {
    let mut clean: String = name
        .chars()
        .map(|c| {
            if c.is_whitespace() || c.is_control() || c == '#' {
                '_'
            } else {
                c
            }
        })
        .collect();
    if clean.is_empty() {
        clean = "layer".to_owned();
    }
    if taken.contains(&clean) {
        clean = format!("{clean}_{layer}");
    }
    taken.insert(clean.clone());
    clean
}

/// Writes the first frame of a scene as OBJ text.
///
/// Every paint node that is visible on the first frame becomes one `o`/`g` with
/// world-space vertices (`v x y z r g b`, colour sRGB-encoded), normals (`vn`) and
/// triangles (`f a//a b//b c//c`). Nodes that still carry a transform are moved into
/// world space here, with the winding reversed under a mirror. OBJ has no opacity,
/// no sidedness, no hierarchy and no animation, so those are not written.
pub fn scene_to_obj(scene: &ExportScene) -> (String, ExportReport) {
    let mut out = String::new();
    let mut report = ExportReport::default();
    let mut taken = HashSet::new();
    let mut base = 1usize; // OBJ indices start at one and run through the whole file.

    out.push_str("# Limner OBJ export\n");
    if !scene.name.is_empty() {
        let title: String = scene.name.chars().filter(|c| !c.is_control()).collect();
        let _ = writeln!(out, "# {title}");
    }
    out.push_str("# metres, +Y up, right-handed, triangles counter-clockwise from outside\n");
    out.push_str("# vertex colours: v x y z r g b, sRGB-encoded 0..1\n");

    for (index, node) in scene.nodes.iter().enumerate() {
        let NodeContent::Paint(paint) = &node.content else {
            continue;
        };
        if !scene.is_visible(index, 0) {
            continue;
        }
        let meshes: Vec<usize> = paint
            .drawings
            .iter()
            .filter(|d| *d.shown.first())
            .flat_map(|d| d.meshes.iter().copied())
            .collect();
        if meshes.is_empty() {
            continue;
        }
        let world = scene.world_transform(index, 0);
        let placed = world != DAffine3::IDENTITY;
        let name = object_name(&node.name, node.layer.0, &mut taken);
        let _ = writeln!(out, "o {name}\ng {name}");
        report.nodes += 1;

        for mesh_index in meshes {
            let mut moved;
            let mesh = if placed {
                moved = scene.meshes[mesh_index].clone();
                moved.transform(&world);
                &moved
            } else {
                &scene.meshes[mesh_index]
            };
            for (p, c) in mesh.positions.iter().zip(&mesh.colors) {
                let _ = writeln!(
                    out,
                    "v {} {} {} {:.4} {:.4} {:.4}",
                    p[0],
                    p[1],
                    p[2],
                    linear_to_srgb(c[0]),
                    linear_to_srgb(c[1]),
                    linear_to_srgb(c[2]),
                );
            }
            for n in &mesh.normals {
                let n = Vec3::from_array(*n);
                let _ = writeln!(out, "vn {:.5} {:.5} {:.5}", n.x, n.y, n.z);
            }
            for t in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = t.map(|i| base + i as usize);
                let _ = writeln!(out, "f {a}//{a} {b}//{b} {c}//{c}");
            }
            base += mesh.vertex_count();
            report.meshes += 1;
            report.vertices += mesh.vertex_count();
            report.triangles += mesh.triangle_count();
        }
    }
    report.bytes = out.len() as u64;
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_encoding_matches_the_core_tables() {
        assert_eq!(linear_to_srgb(0.0), 0.0);
        assert!((linear_to_srgb(1.0) - 1.0).abs() < 1e-6);
        assert!((linear_to_srgb(0.5) - 0.735_357).abs() < 1e-5);
        assert!((linear_to_srgb(0.214_041_14) - 0.5).abs() < 1e-3);
        assert!((linear_to_srgb(9.0) - 1.0).abs() < 1e-6, "clamped");
        assert_eq!(linear_to_srgb(f32::NAN), 0.0);
        // The same curve the renderer uses for its 8-bit colours.
        for i in 0..=20 {
            let linear = i as f32 / 20.0;
            let byte = limner_core::tess::linear_to_srgb8(linear);
            assert_eq!((linear_to_srgb(linear) * 255.0 + 0.5) as u8, byte);
        }
    }

    #[test]
    fn object_names_are_clean_and_unique() {
        let mut taken = HashSet::new();
        assert_eq!(object_name("Left arm", 4, &mut taken), "Left_arm");
        assert_eq!(object_name("Left arm", 9, &mut taken), "Left_arm_9");
        assert_eq!(object_name("", 2, &mut taken), "layer");
    }
}
