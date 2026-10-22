# Plan: Quill-style panels

Design: `docs/superpowers/specs/2026-10-22-quill-style-panels-design.md`. Each step ends with the tests passing, a look at screenshots of what changed, and a commit.

**Done, 2026-10-22**, all sixteen steps, on Windows with the simulated headset. Two choices made along the way: on the desktop, panel heights are not held to their design size (a dock gives a panel its height, and the older widgets keep their pixel sizes there), and the desktop's left dock shows Tools, Color and Text one at a time, since stacked they do not fit a window above the Layers panel. Not done: the step's release (the package and a GitHub release are left to the owner's word).

## 1. Foundations

1. **Icons and font.** Copy the study's 90 SVGs into `godot/ui/icons/`, Noto Sans and its licence into `godot/ui/fonts/`. `Icons.texture()` loads a file icon when one exists (tinted like the stroke icons; cached). A table in `docs/icons.md` maps our icon names to the new files; names with no new file keep their stroke icon. Kit uses Noto Sans. Credit and licence in `docs/icons.md` and the README.
2. **Kit sizes and states.** `Kit.scale` (1.0 headset, 0.75 desktop) and `Kit.u()`; key sizes `KEY` 38, `KEY_BIG` 52; buttons act on release; the study's button states (normal, hover, down, disabled) in our colours.
3. **New widgets.** `KeyGrid` (keys in rows of n, one down), `Cycler` (◀ value ▶), `Pager` (a page of rows with page keys, following a chosen row), `IconOption` (a key that steps through a few settings and writes the setting under itself), `OpacityRamp`.
4. **Panel frame.** `LimnerPanel` base: `design_size()`, built at `Kit.u()`; a test helper `assert_fits(panel)` that checks the minimum size and forbids `ScrollContainer`, `OptionButton`, `ItemList` and `Tree` under the panel.

## 2. Tools and Color

5. **Tools** rebuilt to the study's layout with the real tool settings; Text moved out to a new **Text** panel. Tests: every tool's context fits; every key sends what the old control sent.
6. **Color** rebuilt; HSV sliders removed; hex kept. Tests updated.

## 3. Layers and the timeline

7. **LayerRows**: paged rows of keys (fold, name, visible, kind, transform target, lock, bounds), following the current layer; selection with the modifier; rename through the name key and the keyboard.
8. **TimelineView** drawn for a given list of rows (no names column, no scroll bars, no vertical scrolling); time moved by zoom, fit and page keys; follow the playhead while playing.
9. **LayersPanel** combining the transport row, the layer keys, LayerRows and (when open) the timeline with its context rows. Replaces the old LayersPanel and TimelinePanel. Tests moved over.

## 4. The other panels

10. **Layer** (properties): kinds swapped in place, drop-downs to keys and cyclers.
11. **Document**: paged file lists, fixed description box, cyclers.
12. **Export**: three sections that fit; cyclers for cameras and sizes.
13. **Transform, Monitor, Performance, Settings**: fixed sizes, no drop-downs.

## 5. Shells

14. **Headset**: new coin set; sheets without scroll containers at the panels' sizes; the stick pages; `PIXELS_PER_METRE` 1250; button map in three pages; Layer and Text panels shown from their keys.
15. **Desktop**: docks without scrolling; Tools and Color left; tabs and panels right; the Layers panel in the bottom dock with rows following its height.

## 6. Finish

16. Docs: `interface-api.md` (rules and widgets), `xr.md` (palette, sizes), `quill-differences.md`, `handoff.md`; screenshots of every panel in both schemes; full test run on Windows; package and release.
