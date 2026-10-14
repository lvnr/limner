# Hand-over: from the Mac to the PC and the headset

Limner was built end to end on a Mac that has no headset. Everything that can be checked without one has been: by tests, and by looking at screenshots. Nothing has run in a headset, on Windows, or on a Quest. This page says what is in place, how to carry on from a Windows PC, what to try first with the headset on, and which parts are still unproven.

## Where things stand

| Plan step | What is there | Checked here | Not yet checked |
|---|---|---|---|
| 1. Viewer | Opens Quill projects and native ones, draws every stroke type, floor grid, orbit/pan/zoom | Tests, screenshots of the sample project and of public Quill files | Other graphics cards |
| 2. Player | Playback with stops, loops, chapters and the working range; clips, sequences, frames; pictures, 360 pictures, models, sounds, spawn areas, cameras | Tests; pictures and models by screenshot; sound by its state only | Sound by ear |
| 3. Headset | OpenXR rig, hand-held palette with every panel, pinning, poking and far pointing, keyboard, button map, haptics, sounds; a simulated rig for working without a headset | 17 tests of the headset layer on the simulated rig, and screenshots of it | Everything on a real headset (see below) |
| 4. Lights | Sun, point and spot light layers; lit paint layers and models; shadows per light; ambient light; baking light into paint | Tests; live and baked lighting compared by screenshot | Stereo rendering, speed in a headset |
| 5. Own format and export | `.limn` folders and `.limnz` files, crash-safe saving, autosave and recovery; export to glTF, USD/USDZ, FBX, OBJ from the app and the command line | Round-trip and crash tests; exports read back by strict readers written for the tests, and by outside ones: Godot's glTF and FBX importers, Apple's USD tools | Opening the exports in Blender, Unity and Unreal |
| 6. Painting and editing | Nine tools, seven brushes, tapers, 14 colour modes, selection, layers, undo; grabbing the world, a layer or the selection; two-hand scale; the gimbal; duplicate-and-repeat; the blink on the stroke about to be deleted; the brush grid; mirrored working; text | Core tests driving the tools as controllers would; desktop screenshots | How any of it feels in the hand |
| 7. Animating | Frames, holds, onion skin, clips, keys with six easings, recording, marks, loop points, baking a sequence, painting during playback, the Timeline panel, painting mode | Core and panel tests | Timing by feel, long recordings |
| 8. Capture | Stills, 360 stills, stereo, transparent background, camera renders to AVI, OGV or PNG sequence with sound, the Monitor | Windowed tests and screenshots; rendered files inspected | Sound in a render, Windows and Linux |
| 9. Beyond Quill | Smooth keys, mirrored working, text, Performance panel, persistent swatches, autosave, importing another project as a group | Tests | |

Numbers from the last full run on the Mac are at the end of this page.

## Moving to the Windows PC

The project is public at [github.com/lvnr/limner](https://github.com/lvnr/limner). Clone it on the PC:

```powershell
git clone https://github.com/lvnr/limner.git
```

The research behind the project (the Quill feature report and its notes, `reports/` and `research_notes/`) is kept out of the repository on purpose: it describes Quill's install files and quotes numbers from a private sample. It stays on the Mac; copy those two folders across by hand if the PC session should have them, and keep them out of git (they are listed in `.gitignore`).

On the PC:

1. Install [Rust](https://rustup.rs) (stable), Git, and the Visual Studio Build Tools with the "Desktop development with C++" workload (Rust needs its linker).
2. In the repository folder:
   ```powershell
   scripts\fetch-godot.ps1                 # downloads the pinned Godot into tools\
   cargo build -p limner-godot             # builds the extension the Godot project loads
   scripts\test.ps1                        # Rust tests, then the interface tests
   scripts\godot.ps1 --xr-mode off --path godot -- --desktop     # the app, on the desktop
   scripts\package.ps1                     # optional: a packaged Limner.exe in local\out\builds\windows\
   ```
3. If something fails at step 2, that is new information: none of it has run on Windows. The likely places are the PowerShell scripts (written without a Windows machine) and paths with backslashes. `CLAUDE.md` is written for the next Claude Code session and has the layout, the rules and the commands.

The project is set to start OpenXR whenever the app starts on Windows. That is what makes a headset work without a switch, and it means the headset's software can wake up for a desktop session. `--xr-mode off` (before the `--`) prevents that; the test and screenshot scripts already pass it.

### Windows, first run

On a Windows 10 PC (Core i9-9900K, GeForce RTX 4060, Windows PowerShell 5.1, Rust 1.99, Visual Studio 2022 Build Tools), with Godot 4.8 dev 7:

- `cargo build -p limner-godot` built without changes, and every Rust test passed.
- The headless interface tests: 221 run, 0 failed (the capture tests need a window and were left out).
- The app ran on the desktop with Vulkan on the GeForce and drew the demo sheet as on the Mac.
- `scripts\package.ps1` made a release package (`Limner.exe`, `Limner.pck`, `limner_godot.dll`, 40 MB zipped) that ran from a fresh folder.

Three things in the PowerShell scripts were wrong and are fixed: Windows PowerShell 5.1 turns a program's stderr into errors when the output is redirected, so a harmless Godot warning stopped `test.ps1` and `package.ps1` at the import (`godot.ps1` now sets `$ErrorActionPreference = "Continue"` and relies on Godot's exit code); `shot.ps1` passed `--position 40,40` unquoted, which PowerShell splits into two arguments; and the package's extension needed the Visual C++ runtime (`VCRUNTIME140.dll`), which `package.ps1` now links in statically so the package runs on a PC without it.

## The first session with the headset

`docs/xr.md` has the full list: how to make Meta's PC app the active OpenXR runtime, a 15-step first session in order, and the names of the constants to tune at each step (where the brush tip sits, how deep a poke has to go, where the palette rides in the hand). In short:

1. Start with `scripts\godot.ps1 --path godot -- --xr --no-sim`, so that a failure is reported instead of hidden behind the simulated rig.
2. If the scene does not appear in the headset, the reason is on the desktop window's message line and in the log. OpenXR complaints about the action map (`godot/xr/action_map.tres`) are the first thing to look for: no runtime has seen it yet.
3. Work down the list in `docs/xr.md`: floor and head, controllers and the brush tip, painting, grips, the palette, poking, the ray, pinning, typing, pop-ups, the button map, taking the headset off, the left hand, frame rate.

The original Rift is a PC headset, so the "PC headset" target in the Performance panel is the one to watch. A standalone Quest build needs downloads that were deliberately not made here (the Android part of Godot's export templates, the Android SDK and NDK, the OpenXR vendors plugin); the steps are in `docs/xr.md`.

## What has never been checked

- **Anything in a headset.** Session start, controller bindings, stereo rendering of every shader, haptics, the palette's place and size, poke depths, performance.
- **Linux.** The Rust code is portable and has no platform-specific parts, but it has not been built or run there. Windows is now checked on the desktop (see "Windows, first run" below); Windows with a headset is not.
- **A Quest.** No Android build of the extension, no export, no vendors plugin.
- **Packaged builds on Linux and Quest.** `scripts/package.sh` made a macOS package here, debug and release, and the packaged app was started and looked at. `scripts\package.ps1` has made a Windows release package (see below). The macOS package holds the extension for Apple silicon only and is not notarised; the Windows one is not signed, so Windows SmartScreen warns before the first start.
- **The exports in Blender, Unity and Unreal.** glTF and FBX files load in Godot's own importers and USD files pass Apple's USD tools, but nothing has been opened in the programs people will take them to. `docs/export.md` lists what was checked and says what each file should look like there.
- **Sound by ear.** Players start, stop, loop, seek and fade as they should according to their state. Nobody has listened.
- **FBX and Ogg import with real files**, ambisonic sound (only the first channels are played), MP3 beyond generated silence.
- **Quill's exact numbers.** How fast its tools act, its selection hit test, its sound fall-off. Limner's choices are constants listed in `docs/quill-differences.md`; the table of what to measure on a PC with Quill is at the end of the private research report (see above) and in `docs/quill-import.md`.

## Known gaps

Things a Quill user may look for and not find, or that are simpler than they could be:

- Moving a very large selection is smooth while it is carried (only the motion changes each frame), but taking hold and letting go each make the triangles of the touched chunks again once: about 70 ms for 20,000 selected strokes mixed through 40,000 in the development build, a visible hitch at both ends. `cargo run -p limner-core --example selection_move` measures it. Onion skins of other frames whose strokes are carried show them unmoved until the grip lets go.
- Text has no single-line stroke font for small labels: letters are always outlines (and fillings), which get heavy below a centimetre or so. A font chosen on one computer and missing on another falls back to the built-in one with a note.
- The upper button picks a colour from the nearest stroke of any visible layer, but the blink at the hand shows what the lower button would delete, which is always a stroke of the layer being painted. A stroke on another layer is picked from without blinking first.
- The Performance panel's limit on sounds playing at once (16 standalone, 32 PC headset and desktop) is a rule of thumb, like its other limits. The frame-time peak is the highest of its half-second averages, so a single long frame does not show.
- Baked lighting gives a tube one brightness all the way round. Baked shadows treat every stroke segment as a round capsule (ribbons too, so their shadows are a little rounder than the live ones), are hard-edged where the live view filters its shadow maps, leave out model layers (the core does not hold their shapes), and ignore how directional strokes fade; a point inside another stroke is not shaded by it.
- Sounds are exported only to USD and USDZ, as UsdMedia `SpatialAudio` prims with the audio beside the `.usda` or inside the `.usdz` (Ogg Vorbis cannot go in a `.usdz` and is left out with a warning). Apple's `usdchecker` accepts the files, but no player has played them. glTF (no ratified audio extension yet), FBX and OBJ leave sounds out and say so in the export's warnings. Ambisonic sound is reduced to its first channels.
- The Timeline's pinch and two-finger panning have only met synthetic events, written to match what Godot sends on macOS, never a real trackpad; the pan speed may want adjusting by feel ("The Timeline panel" in `docs/interface-api.md`).
- The Document panel's File section (the headset's only way to save and open) works in one folder, `Documents/Limner` (`user://documents` on a Quest): no subfolders, no renaming or deleting, and its lists show the 30 newest entries. On a Quest that folder is the app's own storage, reached from a PC only with `adb`.
- The headset never asks before quitting: when its system ends the session, or the mirror window is closed, unsaved work is copied to the recovery file and opened again at the next start. Whether OpenXR's "instance exiting" leaves enough time to write a large document has not been seen on hardware.
- Videos at a rate that is not a whole number (23.976) are written at the next whole rate with the sound played 0.1% faster to stay with the picture (`docs/capture.md`). Only the arithmetic is tested; no such render with sound has been listened to.
- What the Quill importer leaves out is listed in `docs/quill-import.md`; what each exporter loses is in `docs/export.md`; the limits of capture are in `docs/capture.md`.

## Where to read

| For | Read |
|---|---|
| Rules, layout and commands for the next coding session | `CLAUDE.md` |
| The design and its reasons | `docs/superpowers/specs/2026-10-14-limner-design.md` |
| Milestones and what is ticked | `docs/superpowers/plans/2026-10-14-limner-master-plan.md` |
| What Quill does, with sources | The private research report, if copied across (see "Moving to the Windows PC") |
| Where Limner differs from Quill | `docs/quill-differences.md` |
| How panels talk to the document; every command | `docs/interface-api.md` |
| The file format | `docs/format.md` |
| Opening Quill projects | `docs/quill-import.md` |
| Export to other programs | `docs/export.md` |
| Stills, video, the Monitor | `docs/capture.md` |
| The headset layer and its first session | `docs/xr.md` |
| What could come next | `docs/roadmap.md` |

## Numbers from the Mac

Measured on an M1 Max, development build, Godot 4.8 dev 7.

- The sample project (53 layers showing, 948,000 triangles) with the whole desktop interface: 3.3 to 3.5 ms a frame with V-Sync off, playing or still.
- Turning one full chunk of strokes (60,000 vertices) into triangles again, which a tool pays for each chunk it changes in a frame: 1.0 to 1.7 ms (`cargo run -p limner-core --example tess_cost`).
- A 1080p still: 60, 112 and 184 ms at the three qualities; an 8192 x 8192 still at the best: 2.8 s.
- The same sample in the packaged release build: 3.2 ms a frame. Drawing is what takes the time, not the Rust code, so the development build is a fair guide to speed.
- The macOS package is 74 MB zipped.

Test counts at hand-over are in the last section of the plan file.
