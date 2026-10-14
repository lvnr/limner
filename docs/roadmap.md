# What could come next

Limner does what the plan set out to do, on the machine it was built on. This page lists what is worth doing next, most useful first. Nothing here is promised.

## First: prove it on real hardware

1. **Run it on the Windows PC.** Build, run the tests, open a project. Fix what the scripts or paths get wrong there. (`docs/handoff.md`)
2. **Put the headset on.** Work through the first-session list in `docs/xr.md` and tune the constants it names. Expect the controller bindings and the palette's place in the hand to need a second pass.
3. **Measure Quill.** With Quill on the same PC, measure the things `docs/quill-differences.md` lists as choices: tool rates, zoom speed, selection hit test, sound fall-off. Change the constants to match where matching matters.
4. **Open the exports** in Blender, Unity and Unreal and compare them with `docs/export.md`.
5. **Listen** to a project with sound.

## Then: make it shippable

- **Packaged builds to hand to other people.** `scripts/package.sh` and `scripts\package.ps1` make a package for the system they run on; the macOS one has been made and run. Still to do: the Windows package on a real PC, a universal and notarised macOS package, an installer or a zip with a licence file, and an icon.
- **A Quest build.** The steps are in `docs/xr.md`. On a Quest, watch the Performance panel: the limits there are rules of thumb until measured.
- **A name.** "Limner" is a working title; the package name `org.limner.app` goes with it.
- **A licence check of the fonts and sounds that ship.** Interface sounds are made in code and icons are original drawings; the interface font is the engine's own.

## Features close to hand

Small steps from what exists:

- Mirror the Grab and Selection tools too.
- Text: several lines, a font chooser, a single-line stroke font for small labels.
- Saved views ("home" positions) beside the spawn areas.
- A light colour scheme beside the dark one. The colours are constants in `godot/ui/kit.gd` and in the widgets that draw themselves; they would have to become something a preference can swap.
- A count of sounds playing, and peaks, in the Performance panel.
- Shadows in baked lighting; baking per ring vertex instead of per point.
- Sounds in glTF and USD exports.
- Curved turning for smooth keys (they curve position and size now; turning takes the shortest way).

## Larger pieces

- **Speed for very large scenes.** Strokes are already cut into chunks that are culled and cached one by one. Next would be coarser stand-ins for distant chunks, moving a selection on the graphics card instead of re-making its strokes each frame, and tessellating on a worker thread.
- **A Blender add-on** that reads `.limn` directly (the format is documented in `docs/format.md` for exactly this).
- **Painting together.** Every edit is already a small command that can be sent over a wire; what is missing is a shared session and conflict rules.
- **Hand tracking** as an alternative to controllers.
- **A viewer build** for sharing finished pieces: playback only, no tools.
