# Where Limner differs from Quill

Limner follows Quill 2.9's features and habits. It does not copy its pixels or its file layout, and where a better design was available it was taken. This page lists every place a Quill user will notice a difference, and why. It also lists the places where Quill's exact behaviour is not known and Limner made a choice that can be tuned once it has been measured on a real install (see the table of unknowns at the end of `reports/Quill feature map and tech stack.md`).

## Additions

| What | Notes |
|---|---|
| **Lights** | A light layer (sun, point, spot) shades the paint layers and models whose shading is set to Lit. Everything else is drawn as painted, as in Quill. Lit strokes use a soft wrapped shading with no highlights, so thin strokes do not fall into black. |
| **Baking light into paint** | "Bake lighting into paint" writes the light a lit layer gets into its strokes' colours, so the look survives without the lights and in every export. One colour per stroke point, so a tube gets the mean of the light around it; shadows are not baked. |
| **Ambient light** | A document setting for how bright lit paint is where no light reaches. |
| **Mirrored working** | With a mirror on X, Y or Z through the pivot of the layer being painted, Paint and Line lay a twin of every stroke and Colorize, Eraser, Thicken, Nudge and Optimize work on both sides. Quill's way (copy, flip, merge) still works. |
| **Text** | A line of text written in strokes, from the font's own outlines: outlined or filled. |
| **Smooth keys** | See below. |
| **Performance panel** | The scene held against what a standalone headset, a PC headset or a desktop draws comfortably. |
| **Autosave and recovery** | Unsaved work is copied aside every few minutes and offered back after a crash. |
| **A desktop mode** | The whole editor runs with a mouse or a pen: the same tools, panels and commands. A mouse cannot give depth, so the hand works on a plane in front of the camera (Ctrl + wheel moves the plane). |
| **Smooth keys** | A sixth interpolation mode that curves through neighbouring keys. Quill's five are kept. |
| **An open file format** | `.limn` folders and packed `.limnz` files, documented in `docs/format.md`. Quill projects open directly; saving always writes the native format. |
| **More export formats** | glTF, USD/USDZ, FBX and OBJ with hierarchy, animation, lights and cameras (`docs/export.md`). |
| **A command line** | `limner inspect`, `convert`, `export`. |
| **Escape** | Takes back a gesture that is half done. |

## Changed on purpose

| Area | Quill | Limner | Why |
|---|---|---|---|
| See-through strokes | Blended over what is behind them, sorted by the order they were painted. | Kept or dropped pixel by pixel against a noise pattern that changes every frame; never sorted. | It works with any engine renderer, costs the same on a phone-class headset as opaque paint, and overlapping see-through strokes cannot flicker or pop when layers reorder. Up close a see-through stroke looks finely grained instead of glassy. |
| Pictures with transparency, faded layers | Blended. | The same dither as strokes. | One rule for everything that is seen through. |
| New keys | Stepped (hold) by default. | Linear by default; the default is an option (`default_interp`). | A new user's first two keys move instead of jumping. |
| Undo of a recorded take | One undo step per frame of the take. | One undo step for the whole take. | Taking back a bad take is one action. |
| Undo of edits that change nothing | Not known. | An edit that leaves the document exactly as it was makes no undo step. Folding a group in the layer list is saved but is not an undo step. | Undo never appears to do nothing. |
| Redo as "transform again" | Redo repeats the last duplicate-and-move. | The same, but only while nothing else has been done since the copy was made. | Redo cannot surprise you with a copy long after the fact. |
| Baking a looping sequence | Keeps the original. | Keeps the original and hides it, so the baked layer is not drawn twice. | |
| Gimbal size | A fixed size in the room. | Scales with its distance from the eyes, so it is the same size to look at wherever it is. | It has to work from a desktop camera too. |
| Gimbal arrows | Reach past the rings. | End short of the rings. | A hand is never at an arrow and a ring at once. |
| Models | Single-sided. | Drawn from both sides. | A cave seen from inside works without flipping faces. |
| Sound cones | Not documented whether angles are full or half angles. | Half angles from the layer's forward (-Z) axis. | To be checked against Quill by ear. |
| Tool gestures | | Every trigger press, grab and slider drag is exactly one undo step. | Stated as a rule so that tools added later follow it. |

## Choices where Quill's behaviour is not known

These were chosen to feel right and are constants in the code, easy to change after measuring.

| What | Limner's choice | Where |
|---|---|---|
| Rapid zoom with the stick while gripping | A factor of e every 0.45 s at full push | `session/grab.rs` `ZOOM_SECONDS` |
| Steadying with a light grip | Motion eased over up to 0.2 s, none from 90% grip | `session/grab.rs` |
| Grip press and release levels | 0.35 and 0.2 | `session/grab.rs` |
| Tool size with the stick | A factor of e every 0.6 s | `session/gesture.rs` |
| Stick repeat (undo, redo, frames) | After 0.4 s, then every 0.09 s | `session/gesture.rs` |
| Thicken rate, Nudge, Grab falloff, Eraser core | See `session/tools/sphere.rs` | |
| Line density | One point per 5 mm of viewer-space length | `session/tools/paint.rs` |
| Paint density | One point per millimetre of hand travel at any zoom | `session/tools/paint.rs` |
| Selection hit test | A stroke is touched when the sphere reaches its surface at any stored point | `session/tools/select.rs` |
| Duplicating a selection with the modifier | Only while the Selection tool is in hand (as Quill 1.8 says; one later tutorial shows otherwise) | `session/grab.rs` |
| Sound fade with distance | Linear: even between the two radii. Logarithmic: `inner / distance`, shifted to reach silence at the outer radius | `scene/sounds.rs` |
| Light range | The layer's range × its scale | `scene/lights.rs` |
| Undo depth | 200 steps, not kept across save and open | `session/history.rs` |

## Not carried over

| What | Why |
|---|---|
| Publishing to Oculus Media Studio / Quill Theater, IMM export | The services are gone or closed. Export to open formats instead. |
| Facebook Spaces sharing | Removed from Quill itself in 2.1. |
| Reading Quill's `State.json` tool settings | Only the paint layer and the grid switch are carried over when a Quill project is opened. |
| Saving back to Quill's format | The importer is one-way by design. |
