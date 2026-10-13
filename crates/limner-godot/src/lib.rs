use godot::prelude::*;

struct LimnerExtension;

#[gdextension]
unsafe impl ExtensionLibrary for LimnerExtension {}

/// Spike: confirms the extension loads and can be called from GDScript.
#[derive(GodotClass)]
#[class(base=Node3D)]
struct LimnerProbe {
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for LimnerProbe {
    fn init(base: Base<Node3D>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl LimnerProbe {
    #[func]
    fn hello(&self) -> GString {
        GString::from("limner-godot loaded")
    }
}
