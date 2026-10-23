# Where Limner differs from Quill

Limner follows Quill 2.9's features and habits. It does not copy its pixels or its file layout, and where a better design was available it was taken. This page lists every place a Quill user will notice a difference, and why. It also lists the places where Quill's exact behaviour is not known and Limner made a choice that can be tuned once it has been measured on a real install.

## Additions

| What | Notes |
|---|---|
| **Lights** | A light layer (sun, point, spot) shades the paint layers and models whose shading is set to Lit. Everything else is drawn as painted, as in Quill. Lit strokes use a soft wrapped shading with no highlights, so thin strokes do not fall into black. Paint starts unlit, so adding the first light to a document whose paint is all unlit also lights every paint layer (one undo step with the light, and a message saying so); a paint layer added after that is lit too. A light's settings say how many paint layers take light and have a "Light all paint layers" button. A new light is put in sight, 0.8 m ahead of and 0.3 m above the eyes, pointing 45 degrees down at the work, reaching 3 m of the room; its mark is a gizmo-sized sun, cone or arrow with a solid heart, outlined in black or white so it reads on a light floor or a dark sky. |
| **Baking light into paint** | "Bake lighting into paint" writes the light a lit layer gets into its strokes' colours, so the look survives without the lights and in every export. One colour per stroke point, so a tube gets the mean of the light around it. Lights with shadows on throw them in the bake too: a point is lit by such a light only where no visible stroke at that moment stands between them (each frame of an animated layer is shaded by its own strokes). |
| **Ambient light** | A document setting for how bright lit paint is where no light reaches. |
| **Mirrored working** | With a mirror on X, Y or Z through the pivot of the layer being painted (turned on for a layer whose pivot is not set, it sets the pivot in front of the viewer, square to the way they face, so that X is left and right: through the layer's origin it was in the middle of the room's floor in a headset, and a tester's twins appeared somewhere unexpected), Paint and Line lay a twin of every stroke, and Colorize, Eraser, Thicken, Nudge, Optimize, Grab and Selection work on both sides. With Grab, strokes near the hand's reflection follow the reflection of the hand's motion; each point moves once, with the sphere on its side of the mirror, and a point on the mirror takes the average of the two motions so it stays on the mirror. Quill's way (copy, flip, merge) still works. |
| **Text** | Text written in strokes, from the font's own outlines: outlined or filled, one or several lines aligned left, centre or right, in the built-in font or any font installed on the computer, with bold and italic. The font chosen is remembered, the last few are at the top of the list, and the list can be searched. |
| **Performance panel** | The scene held against what a standalone headset, a PC headset or a desktop draws comfortably. |
| **Autosave and recovery** | Unsaved work is copied aside every few minutes and offered back after a crash. |
| **A desktop mode** | The whole editor runs with a mouse or a pen: the same tools, panels and commands. A mouse cannot give depth, so the hand works on a plane in front of the camera (Ctrl + wheel moves the plane). The work moves between the desktop and the headset while the app runs, undo history and all ("Continue in the headset", "Continue on the desktop"). |
| **Smooth keys** | A sixth interpolation mode that curves through neighbouring keys. Quill's five are kept. |
| **An open file format** | `.limn` folders and packed `.limnz` files, documented in `docs/format.md`. Quill projects open directly; saving always writes the native format. |
| **More export formats** | glTF, USD/USDZ, FBX and OBJ with hierarchy, animation, lights and cameras, and sound layers in USD/USDZ (`docs/export.md`). |
| **A command line** | `limner inspect`, `convert`, `export`. |
| **Escape** | Takes back a gesture that is half done. |
| **Importing a project into another** | A `.limn` or `.limnz` project, or a Quill project, arrives as one folded group. |
| **Swatches that stay** | The swatches set in the Colour panel are the ones the next new document starts with. |

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
| Gimbal with the camera as the transform object | The gimbal shows only for a layer that is the transform object, or for a selection; the transform object starts as the camera, so a click of the stick usually shows nothing. | While the view is the transform object, the gimbal is the current layer's (unless that is the root or locked); the grip away from the gimbal still moves the view. A click that turns it on with nothing to move says "Nothing to transform: choose a layer, or select strokes". Giving the transform object back to the view, or Deselect while the gimbal is the selection's, puts the gimbal away. | A gimbal that does not appear when asked for looks broken; one that stays after its layer is let go looks stuck. |
| Where a layer's gimbal sits | At the layer's pivot, which for a new or painted layer is its origin: often the floor under the viewer, far from the strokes. | At the pivot once it has been set; until then, at the middle of the layer's content (as Reset Pivot puts it). Using the gimbal on such a layer sets the pivot there, in the same undo step, so the layer and its animation turn about the point the gimbal showed. A pivot at the origin and unturned counts as not set, including after Make Pivot Origin. | The gimbal appears over what was painted, and turns it about its middle. |
| Models | Single-sided. | Drawn from both sides. | A cave seen from inside works without flipping faces. |
| Sound cones | Not documented whether angles are full or half angles. | Half angles from the layer's forward (-Z) axis. | To be checked against Quill by ear. |
| Tool gestures | | Every trigger press, grab and slider drag is exactly one undo step. | Stated as a rule so that tools added later follow it. |
| Selecting keys in the timeline | | Keys of several layers can be selected and moved together. Pressing a key of another kind (position, opacity, chapter mark) switches the key type to it. | Fewer trips to the key-type switch. |
| Interface colours | A light and a dark scheme, chosen as a preference. | The same two (Settings > Colours), in Limner's own colours: warm light greys with near-black text, and the blue accent deepened, rather than the dark scheme turned over. The desktop and the headset each keep a scheme of their own: the desktop starts dark, the headset light, as Quill's panels are. A change shows at once on every panel without a restart. The floor grid, gimbal, tool cursor and gizmos keep their colours in both. | Quill's pixels are not the goal; each scheme is drawn to read well on its own. The 3D helpers sit in the scene, not on a panel. |
| The palette's buttons | Raised buttons with a dark outline and a visible thickness, pushed with the tip; the chosen one cyan. | Every button stands 5 mm off its sheet as a key that goes down with the tip, presses 3 mm down and bottoms out 1 mm above the sheet; a button that is on rests half down; a disabled one is flush. Outlined as in Quill, in Limner's colours. Check boxes, sliders and lists stay flat. The depths are guesses to tune in a headset. | A press has to be felt and seen without anything to touch. |
| The palette's panels | Tools, Color, Transform, Monitor and Timeline (with the layers) in the lower row of round buttons; Documents (with settings), Performance and Export above, and the Theater. Compact panels of fixed size, icons first; nothing scrolls. | The same: Tools, Color, Layers (with the timeline), Transform and Monitor below; Document, Settings, Performance and Export above. Every panel has one fixed size and nothing on it scrolls: long lists (layers, files, fonts) show a page at a time with page keys, and the stick over them steps the pages; choices among many are a value with a key at each side instead of a drop-down. A layer's own settings (the Layer panel) and Text are panels shown from keys of other panels. No Theater. | Settings are a panel of their own, and Text is something Limner adds. |
| Pointing at a panel from afar | Panels are poked; a laser pointer may exist but is not documented. | A ray from the drawing hand reaches panels up to 2 m away; it takes only the trigger and the stick's up and down, so the grip and undo still work the artwork while a panel is in view. The grip picks a panel up only with the tip close to it. | Pinned panels across the room stay usable without walking to them. |
| Painting with a group current | A paint layer must be current to paint. | A paint layer current: as in Quill. A group, a sequence or the root current: Paint and Line make a new paint layer inside it and paint there, one undo step for both. A camera, light, spawn area, picture, model or sound current: the trigger does nothing, the tool at the hand turns into a hollow grey ring, and one message says why. The tools that change strokes work on the selection, or else only on a current paint layer. | Choosing a group and starting to paint is the natural way to begin a new part of a scene; painting on with a camera chosen was a trap. |
| Brush with the modifier | Draws a straight line. | Draws a free straight line in the brush's own style, size and opacity; the stick resizes it while it is pulled, as the cursor shows. | The line looks like the brush it came from. |
| Line tool with the modifier | Snaps the line to the grid. | Snaps both ends to the nearest crossings of the brush grid, in world space (the grid the brush grid shows, which also appears while snapping). Quill's grid spacing is not known; see the brush grid below. | |
| The trigger while the same hand's grip holds something | | Does not start the tool, so a finger resting on the trigger while gripping cannot paint. A stroke already under way carries on. | On Touch controllers squeezing the grip easily brushes the trigger. |
| Dragging across a clip in the timeline | | Moves the clip. To select the keys under it instead, hold Shift or turn on "select a range". | Clips cover nearly every track, so a plain drag has to mean one thing. |

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
| The blink on the stroke at the hand | Shows what the lower button would delete: the nearest stroke of the current paint layer, within 1 cm of the tool's sphere. Not shown while a tool or a grip is at work, nor after it until the hand has left every stroke, nor while strokes are selected. Can be switched off in the Tools panel | `session/gesture.rs` `update_at_hand`, `REACH` |
| The brush grid | Lines one cell apart on all three axes, on the world's own grid, reaching three cells from the hand and fading on the way. A cell is 1, 2 or 5 times a power of ten in world units, whichever looks nearest to 5 cm at the current zoom; the Line tool with the modifier snaps to the same crossings. What Quill's looks like, and the spacing its line snapping uses, were not found in any source | `session/view.rs` `brush_grid_spacing`, `BRUSH_GRID_SEEN`; `godot/world/world.gd` `BRUSH_GRID_CELLS` |
| The trigger on the gimbal | Moves the thing along an arrow, in a square's plane or by the centre, as the grip does; turns a ring in snapped steps. With the modifier held it moves the pivot instead (on a ring, turns a copy). A tester found a trigger that moved the pivot, as it did in 0.2.0, confusing | `session/gimbal.rs` `HandleGesture` |
| Pulling the start of a clip out past the beginning of its content | The start stops there; the content never moves. Move the clip to start it earlier. | `ops/clips.rs` `trim_clip` |

## Not carried over

| What | Why |
|---|---|
| Publishing to Oculus Media Studio / Quill Theater, IMM export | The services are gone or closed. Export to open formats instead. |
| Facebook Spaces sharing | Removed from Quill itself in 2.1. |
| Reading Quill's `State.json` tool settings | Only the paint layer and the grid switch are carried over when a Quill project is opened. |
| Saving back to Quill's format | The importer is one-way by design. |
