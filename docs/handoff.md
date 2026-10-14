# Hand-over: from the Mac to the PC and the headset

Limner was built end to end on a Mac that has no headset. Everything that can be checked without one has been: by tests, and by looking at screenshots. Nothing has run in a headset, on Windows, or on a Quest. This page says what is in place, how to carry on from a Windows PC, what to try first with the headset on, and which parts are still unproven.

## Where things stand

| Plan step | What is there | Checked here | Not yet checked |
|---|---|---|---|
| 1. Viewer | Opens Quill projects and native ones, draws every stroke type, floor grid, orbit/pan/zoom | Tests, screenshots of the sample project and of public Quill files | Other graphics cards |
| 2. Player | Playback with stops, loops, chapters and the working range; clips, sequences, frames; pictures, 360 pictures, models, sounds, spawn areas, cameras | Tests; pictures and models by screenshot; sound by its state only | Sound by ear |
| 3. Headset | OpenXR rig, hand-held palette with every panel, pinning, poking and far pointing, keyboard, button map, haptics, sounds; a simulated rig for working without a headset | 22 tests and screenshots on the simulated rig | Everything on a real headset (see below) |
| 4. Lights | Sun, point and spot light layers; lit paint layers and models; shadows per light; ambient light; baking light into paint | Tests; live and baked lighting compared by screenshot | Stereo rendering, speed in a headset |
| 5. Own format and export | `.limn` folders and `.limnz` files, crash-safe saving, autosave and recovery; export to glTF, USD/USDZ, FBX, OBJ from the app and the command line | Round-trip and crash tests; exports read back by strict readers written for the tests, and by outside ones: Godot's glTF and FBX importers, Apple's USD tools | Opening the exports in Blender, Unity and Unreal |
| 6. Painting and editing | Nine tools, seven brushes, tapers, 14 colour modes, selection, layers, undo; grabbing the world, a layer or the selection; two-hand scale; the gimbal; duplicate-and-repeat; the blink on the stroke about to be deleted; mirrored working; text | Core tests driving the tools as controllers would; desktop screenshots | How any of it feels in the hand |
| 7. Animating | Frames, holds, onion skin, clips, keys with six easings, recording, marks, loop points, baking a sequence, painting during playback, the Timeline panel, painting mode | Core and panel tests | Timing by feel, long recordings |
| 8. Capture | Stills, 360 stills, stereo, transparent background, camera renders to AVI, OGV or PNG sequence with sound, the Monitor | Windowed tests and screenshots; rendered files inspected | Sound in a render, Windows and Linux |
| 9. Beyond Quill | Smooth keys, mirrored working, text, Performance panel, persistent swatches, autosave, importing another project as a group | Tests | |

Numbers from the last full run on the Mac are at the end of this page.

## Moving to the Windows PC

The repository has no remote; nothing has been pushed anywhere. Two ways to move it:

- **Copy the folder.** Leave out `target/` (build output, several GB), `tools/` (the Godot download) and `godot/.godot/` (the engine's cache); they are rebuilt. `local/` holds reference sources and scratch output and is not needed.
- **Use a remote of your own.** `git remote add origin <url>` and push. `3d models&design/` (material from the Quill install) is git-ignored and must stay out of any repository. `reports/` and `research_notes/` are tracked: they quote a few numbers measured from your own sample project (key times, a camera's field of view), so read them before making the repository public.

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

## The first session with the headset

`docs/xr.md` has the full list: how to make Meta's PC app the active OpenXR runtime, a 15-step first session in order, and the names of the constants to tune at each step (where the brush tip sits, how deep a poke has to go, where the palette rides in the hand). In short:

1. Start with `scripts\godot.ps1 --path godot -- --xr --no-sim`, so that a failure is reported instead of hidden behind the simulated rig.
2. If the scene does not appear in the headset, the reason is on the desktop window's message line and in the log. OpenXR complaints about the action map (`godot/xr/action_map.tres`) are the first thing to look for: no runtime has seen it yet.
3. Work down the list in `docs/xr.md`: floor and head, controllers and the brush tip, painting, grips, the palette, poking, the ray, pinning, typing, pop-ups, the button map, taking the headset off, the left hand, frame rate.

The original Rift is a PC headset, so the "PC headset" target in the Performance panel is the one to watch. A standalone Quest build needs downloads that were deliberately not made here (export templates, the Android SDK and NDK, the OpenXR vendors plugin); the steps are in `docs/xr.md`.

## What has never been checked

- **Anything in a headset.** Session start, controller bindings, stereo rendering of every shader, haptics, the palette's place and size, poke depths, performance.
- **Anything on Windows or Linux.** The Rust code is portable and has no platform-specific parts, but it has only been built and run on macOS (Apple silicon).
- **A Quest.** No Android build of the extension, no export, no vendors plugin.
- **Packaged builds on Windows, Linux and Quest.** `scripts/package.sh` made a macOS package here, debug and release, and the packaged app was started and looked at. `scripts\package.ps1` is its untested twin for Windows. The macOS package holds the extension for Apple silicon only and is not notarised.
- **The exports in Blender, Unity and Unreal.** glTF and FBX files load in Godot's own importers and USD files pass Apple's USD tools, but nothing has been opened in the programs people will take them to. `docs/export.md` lists what was checked and says what each file should look like there.
- **Sound by ear.** Players start, stop, loop, seek and fade as they should according to their state. Nobody has listened.
- **FBX and Ogg import with real files**, ambisonic sound (only the first channels are played), MP3 beyond generated silence.
- **Quill's exact numbers.** How fast its tools act, its selection hit test, its sound fall-off. Limner's choices are constants listed in `docs/quill-differences.md`; the table of what to measure on a PC with Quill is at the end of `reports/Quill feature map and tech stack.md` and in `docs/quill-import.md`.

## Known gaps

Things a Quill user may look for and not find, or that are simpler than they could be:

- The Grab tool and the Selection tool do not work across the mirror; the other seven tools do.
- Moving a very large selection re-draws its strokes every frame and will be slow in the tens of thousands of strokes.
- Text is one line, in the built-in font, and has no font chooser.
- The stroke the lower button would delete blinks orange first, as in Quill. The upper button picks a colour from the nearest stroke of any visible layer, and that one blinks only when it is on the layer being painted.
- No saved "home" views (there is "Go to initial spawn area") and no brush grid helper.
- Baked lighting has no shadows, and a tube gets one brightness all the way round.
- Sounds are not exported to 3D formats; ambisonic sound is reduced to its first channels.
- The Timeline does not scroll by itself while a drag nears its edge, Escape does not cancel a drag on its tracks, and pinch and two-finger panning have never met a real trackpad ("The Timeline panel" in `docs/interface-api.md`).
- What the Quill importer leaves out is listed in `docs/quill-import.md`; what each exporter loses is in `docs/export.md`; the limits of capture are in `docs/capture.md`.

## Where to read

| For | Read |
|---|---|
| Rules, layout and commands for the next coding session | `CLAUDE.md` |
| The design and its reasons | `docs/superpowers/specs/2026-10-14-limner-design.md` |
| Milestones and what is ticked | `docs/superpowers/plans/2026-10-14-limner-master-plan.md` |
| What Quill does, with sources | `reports/Quill feature map and tech stack.md` |
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

- The sample project (53 layers showing, 948,000 triangles) with the whole desktop interface: 3.5 ms a frame with V-Sync off, playing or still.
- Turning one full chunk of strokes (60,000 vertices) into triangles again, which a tool pays for each chunk it changes in a frame: 1.0 to 1.7 ms (`cargo run -p limner-core --example tess_cost`).
- A 1080p still: 60, 112 and 184 ms at the three qualities; an 8192 x 8192 still at the best: 2.8 s.
- The same sample in the packaged release build: 3.2 ms a frame. Drawing is what takes the time, not the Rust code, so the development build is a fair guide to speed.
- The macOS package is 74 MB zipped.

Test counts at hand-over are in the last section of the plan file.
