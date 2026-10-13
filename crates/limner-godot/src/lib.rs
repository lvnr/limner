//! The Godot extension: a thin bridge between the Limner core and Godot's scene tree.

mod convert;
mod meshes;
mod scene;

use godot::prelude::*;

struct LimnerExtension;

#[gdextension]
unsafe impl ExtensionLibrary for LimnerExtension {}
