# Quill-style panels: fixed, compact, never scrolling

The headset round of 2026-10-14 made the palette look and feel closer to Quill's (raised keys, round buttons, signs on the hand), but the panels on it are still the desktop panels: long columns of labelled rows, drop-down lists, a layer tree and text boxes that scroll. In a headset that is slow and fiddly: a stick has to scroll a sheet to reach a control, a drop-down opens a list that scrolls again, and the length of a panel changes as the tool or the layer changes.

A friend of the project built a design study of Quill-like panels (a separate Godot project, `limner-ui-design`, with pretend data): four compact panels of fixed size, icons first, the layers and the timeline in one panel, and a small Navigation panel that opens the rest. This page brings that design into Limner, wired to the real editor, for both the headset and the desktop.

Decisions taken with the owner on 2026-10-22:

- **Headset:** the palette on the hand stays (one panel at a time, round buttons to change it, any panel can be pinned in the room). Its panels become the new compact ones.
- **Desktop:** the same new panels, in docks. One set of panels for both.
- **Icons:** the study's 90 original icons (drawn for this project by the friend, released under the MIT licence with Limner) replace ours where they overlap; ours stay for anything they do not cover.
- **Look:** Limner's own colours (warm greys, blue accent, light scheme in the headset by default), with the study's sizes and button states.

## The rules

These are the rules Quill's panels follow, as the study reads them. Every panel keeps them in the headset; tests check the ones that can be checked.

1. **Nothing scrolls.** No panel, list or text box scrolls in the headset. A panel has one fixed size; when a list can be longer than its room (layers, files, fonts, cameras), it shows a page at a time with page keys, and jumps to the page holding the current item.
2. **A panel never changes size.** What changes with the tool or the layer kind is swapped in place, inside an area the panel keeps for it. The largest case decides the size.
3. **No drop-down lists.** A choice among a few is a row or grid of keys, one of which is down. A choice among many is a *cycler*: a key each side of the value, stepping through the choices. Nothing opens a list over the panel.
4. **Icons first, big targets.** Tools, brushes, options and layer columns are icon keys with a tooltip; the tooltip shows in the panel's footer while the tip rests on a key. Keys are at least 38 design pixels (about 30 mm in the headset); the transport keys are 52.
5. **A press acts when it is let go,** over the key it started on; letting go elsewhere cancels it. Sliders change while dragged and make one undo step.
6. **Nothing needs hover, a right click or a keyboard** to be reached. Context menus stay on the desktop as a shortcut only; every entry in them is also a key somewhere.
7. **Few words.** Section captions are a word or two; long explanations are tooltips. Numbers are set with sliders or steppers; the keyboard is for names and text only.
8. **Every panel has a title bar** with its name and a pin key, as now.

## Sizes

Panels are laid out in *design pixels*. In the headset a design pixel is 0.8 mm (`PIXELS_PER_METRE` 1250, up from 1100), so a 500-pixel panel is 40 cm wide and a 38-pixel key is 30 mm. On the desktop the same layout is built at three quarters of the size (`Kit.scale`, 0.75), so text stays sharp instead of being a scaled picture. `Kit.u(pixels)` gives a design size in the current mode.

| Panel | Design size | Contents |
|---|---|---|
| Tools | 500 × 560 | tool keys, tool context, guides, size |
| Color | 500 × 560 | paint and background wells, swatches, colour square and hue strip, hex |
| Layers | 600 × 850, or 1520 × 850 with the timeline open | transport, layer keys, paged layer rows with their column keys, frame and key rows, the timeline lanes |
| Transform, Layer, Document, Export, Monitor, Performance, Settings, Text | 500 × 620 each | two columns of keys, sliders and steppers |

## The panels

### Tools

As the study's Tools panel: a row of the nine tool keys; the tool's name; its context area; a row of guide keys; the size slider at the bottom (the stick changes it too).

- **Paint and Line:** an opacity ramp (a strip showing the colour fading out over a checker, dragged like a slider); seven brush keys; for Paint, three option keys that each step through None, Pressure, Auto and Both (opacity taper, width taper) or toggle (directional), with the setting written small under the key; the animated-brush duration slider.
- **Colorize:** strength and hard-core sliders; the fourteen blend modes as a grid of keys, three to a row.
- **Eraser, Grab, Thicken, Nudge:** strength (and hard core where the tool has it).
- **Selection:** the count selected; Select all, Deselect; two toggles (all visible layers, all frames); Move to new layer, Copy to new layer, Delete.
- **Optimize:** threshold; Optimize layer, Optimize all.
- **Guides row:** floor grid, brush grid, blink the stroke to delete, mirror (a cycler: off, X, Y, Z), and a key that opens the Text panel.

### Color

As the study's: two wells (paint colour, background colour), the chosen one outlined; a row of swatches with an Add key (a press uses a swatch, a hold stores the colour in it, as now); the colour square and the hue strip filling the rest; the hex value and an Add key in a short row under them. The hue, saturation and value sliders go: the square and strip set them, and the hex shows them.

### Layers (and the timeline)

The study's combined panel. The left part is always there; the timeline lanes open to its right with a key, making the panel wide.

- **Transport row:** rewind, previous frame, play, record, next frame, forward; mute; the timecode (a press switches it between time and frame); the frame rate as a cycler; onion skin and its frame count; zoom in, zoom out, fit.
- **Layer keys:** a key per kind to add a layer (group, sequence, paint, picture, model, sound, spawn area, camera, light), then rename, duplicate, delete, move up, move down, group the selected, merge, flatten, and the opacity slider of the current layer.
- **Layer rows:** ten rows a page. Each row: a fold key for groups, the name key (indented by depth; a press makes it current, a press with the modifier adds it to the selection), then the column keys: visible, kind (opens the Layer panel for it), transform target, lock, bounds. Page keys above and below step a page; the page follows the current layer. Folding a group shortens the list.
- **Timeline (when open):** the ruler and the lanes for the same ten rows, drawn as now (clips, frames, holds, keys); the frame, clip, key and chapter rows of keys above them, swapped by the current layer's kind as now. Time is moved with zoom, fit, and keys that step the view a page earlier or later; the view follows the playhead while playing. The range ends and the playhead are dragged on the ruler.

The layer tree and the separate Timeline panel go: this panel replaces both.

### Layer

The current layer's own settings, opened from the kind key of its row (and from its coin when it is the panel showing). One area per kind, swapped in place; every drop-down becomes keys or a cycler (light type, picture projection, sound attenuation and direction, spawn-area height). A sound's cone and frustum angles are sliders in two columns, so the longest kind still fits.

### Document

File keys (Save, Save as, New, Open, Import) at the top. Open and Import show the documents folder in pages of eight rows, with page keys and Close. The title, the description (a box of three lines, which no longer scrolls: it holds 280 characters), background and ambient light wells, the frame rate cycler, the working range and the camera resolution cycler.

### Export

Three keys choose Image, Video or 3D file; each section fits the panel. The camera to use is a cycler; format, quality and kind are rows of keys. The report after an export is cut to three lines, with the rest in the log.

### Monitor, Performance, Settings, Transform

Already short; their drop-downs become keys or cyclers and they get fixed sizes. Settings' button map key opens the button map, which becomes three pages (drawing hand, other hand, with the modifier) with page keys instead of one long scrolling sheet.

### Text

Moved out of Tools into a panel of its own: the text box (three lines, no scrolling), alignment keys, the font as a cycler with a page of eight fonts to pick from, bold, italic, filled, height, Add.

## The headset

- The palette's round buttons become the study's Navigation: Tools, Color, Layers, Transform, Monitor below; Document, Settings, Performance, Export above. The Layer and Text panels have no coin: they are opened from the Layers and Tools panels and show on the palette like any other.
- The sheet loses its scroll container; its body is the panel's fixed size. The stick no longer scrolls: over a panel with pages it steps a page.
- Pop-ups are no longer used by panels; the code that grows a sheet around an open pop-up stays for the desktop-only context menu.

## The desktop

- Left dock: Tools above Color. Right dock: the Navigation keys as tabs (Transform, Layer, Document, Export, Monitor, Performance, Settings, Text) over the panel chosen. Bottom dock: the Layers panel with its timeline open, as wide as the window; on the desktop the number of rows a page follows the dock's height.
- The docks no longer scroll. The menus, the top bar and the keyboard shortcuts stay.

## Icons and font

The study's 90 icons are SVG files on a 48 grid, drawn filled (solid shapes) rather than as strokes. They go in `godot/ui/icons/` and `Icons.texture(name, colour, pixels)` loads a file of that name when there is one, before falling back to the stroke icons in `Icons.BODIES`. `docs/icons.md` lists which name maps to which icon and credits the friend. The study's font, Noto Sans (SIL Open Font Licence), becomes the interface font, with its licence beside it.

## Interface code rules that change

`docs/interface-api.md`'s rule 8 ("lay out correctly from 280 px wide") becomes: a panel declares its design size; it is built with `Kit.u()` sizes; it never uses a `ScrollContainer`, `OptionButton`, `ItemList` or `Tree`; long lists use `Pager`; few choices use `ChoiceRow` or `KeyGrid`, many use `Cycler`.

## Tests

- **Fits:** for every panel and every state that swaps content (each tool, each layer kind, each export section, each document file mode), the panel's minimum size is within its design size, and no `ScrollContainer`, `OptionButton`, `ItemList` or `Tree` is found under it.
- The existing panel tests move to the new controls; what each control sends to the editor does not change.
- Headset: no sheet has a scroll container; the stick over the Layers panel steps a page.
- Screenshots of every panel in both schemes, desktop and headset, looked at.

## Not in this round

- The headset-to-desktop switch drawing leftovers in the 3D view (seen on a friend's PC, 2026-10-15) is a separate fix that needs a headset to reproduce.
- The study's free-floating panel mode (all panels in the room at once) is not built: the palette is kept, and panels are pinned one by one.
