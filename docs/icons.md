# Icons and the interface font

## The filled icons

The 90 filled icons in `art/icons/` were drawn for Limner by a contributor to the project, in a design study of Quill-like panels (October 2026), and are released with Limner under its MIT licence. They are original drawings: nothing in them is copied or traced from Quill or any other program.

Each is an SVG on a 48 × 48 grid, drawn in black, with a few greys for the tonal faces of the brush shapes. `scripts/icons.py` turns them into `godot/ui/icon_art.gd`, where black becomes the colour the icon is drawn in and each grey that colour at a matching opacity. After changing or adding a drawing:

```bash
python3 scripts/icons.py
```

`Icons.texture(name, colour, pixels)` draws the filled icon called `name` when there is one. The panels written before these icons use some older names; `Icons.ALIASES` sends each to its filled drawing:

| Older name | Filled icon |
|---|---|
| `eraser` | `erase` |
| `paint_layer` | `paint` |
| `eye`, `eye_off` | `visible`, `hidden` |
| `lock`, `unlock` | `locked`, `unlocked` |
| `trash` | `delete` |
| `flatten`, `stack` | `layers` |
| `pencil` | `rename` |
| `bbox` | `bounds` |
| `prev_frame`, `next_frame` | `previous`, `next` |
| `gear` | `settings` |
| `folder_open` | `open` |
| `new_file` | `new` |
| `chart` | `performance` |
| `shape_capped_cylinder` … `shape_cube` | `brush_capped_round`, `brush_capped_oval`, `brush_rounded_ribbon`, `brush_round`, `brush_oval`, `brush_ribbon`, `brush_square` |
| `tl_frame_blank` | `blank` |

`key` and `key_filled` keep their line drawings, which tell an empty key from a set one.

## The line icons

Everything the filled set does not cover (the timeline's frame, clip, key and range commands, the layer kinds `light` and the like, the keyboard's keys) is a line drawing on a 24 grid in `Icons.BODIES`, also original to Limner. A new icon should be a filled drawing in `art/icons/` when it can be; a line drawing is for the small, specific commands.

## The font

The interface is set in Noto Sans (from the Noto project, Google Fonts commit `8b0a1d0f5983c89bc2b93f1b5fb55f9e252744b5`), under the SIL Open Font Licence 1.1, which is in `godot/ui/fonts/OFL.txt` and must travel with the font.
