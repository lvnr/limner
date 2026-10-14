# Capture: pictures and video from Limner

Limner makes four kinds of picture of the artwork:

| What | Where | Result |
|---|---|---|
| **Still** | Export panel, "Capture image" | A PNG of the current view or of a camera layer, at any size up to 8192 x 8192 |
| **360 still** | Export panel, "Capture image" | A PNG of everything around a place: equirectangular, stereo equirectangular, or cube faces in a row or unfolded |
| **Camera render** | Export panel, "Render camera"; or the command line | A video of a range of the timeline through a camera layer, flat or 360, rendered by a second copy of the application while work goes on |
| **Current frame** | Monitor panel, "Export current frame" | A PNG of what the chosen camera sees now, at the document's camera resolution |

All four are rendered the same way, off screen, and none of them shows anything but artwork. 3D files (glTF, USD, FBX, OBJ) are a separate matter; see `docs/export.md`. They share the Export panel, whose third section, "Export 3D file", is described at the end of this page.

The code is in `godot/capture/` (rendering, the video job and its launcher), `godot/shaders/capture_*.gdshader`, and `godot/ui/` (`export_panel.gd` with one file per section, `monitor_panel.gd`, `viewfinder.gd`).

## How a picture is rendered

A picture is never a copy of the window. It is rendered into targets of its own (`CaptureRig`), at the size asked for, whatever size the window is.

**Passes.** See-through strokes are drawn by keeping or discarding whole pixels against a noise pattern, so that strokes never have to be sorted (design, section 8). One rendering of that is stippled. On screen the pattern moves every frame and the eye averages it; a picture has to do the averaging itself. So a picture is rendered several times ("passes"), each with the pattern in a new place, and the passes are averaged in linear light with float precision. The average converges on the smooth transparency the stipple stands for.

**Supersampling.** At Good and Best the picture is rendered at twice the size each way, with 4x MSAA, and each pixel of the result is the mean of the four rendered for it. Together that is sixteen coverage samples per pixel in every pass.

**Tiles.** One shot is at most 2048 rendered pixels a side. A larger picture is rendered tile by tile, each through the part of the camera's frustum that belongs to it, and the tiles are put together. Tile edges fall on multiples of the 64-pixel dither pattern, so even a one-pass picture has no seam. This is what lets an 8192 x 8192 picture at Best (16384 x 16384 rendered pixels, 32 times over) fit in ordinary graphics memory.

**Within one frame.** `CaptureRig.shoot` asks the engine to draw once per pass (`RenderingServer.force_draw`) and returns with the result. Nothing waits for the next frame and nothing is shown on screen in between, so the main view does not flicker. The window does not respond while a picture renders.

**Artwork only.** While a picture is taken (`LimnerCapture.begin` to `end`) these are hidden, and put back before anything reaches the screen:

- the floor grid (`world.show_grid`);
- gizmos of cameras, lights and spawn areas (the editor option `show_extras`), and onion skins (`onion_skin`);
- every other node that shares the room with the artwork, which today is the tool cursor.

Panels are never in a picture: they are drawn in the window, not in the 3D world the picture is rendered from. The capture camera also sees render layer 1 only (`CaptureRig.ARTWORK_LAYERS`), so anything put on another layer is left out without needing to be hidden.

**Transparent background.** With "Transparent background" the background colour is left out and the PNG has an alpha channel: how much of each pixel the artwork covers, counting see-through strokes by their opacity. Alpha is straight (the colour is not multiplied by it), which is what PNG defines. Internally the passes are averaged with colour multiplied by coverage, the only form that can be averaged and filtered without fringes, and divided out at the end.

## Quality and what it costs

| Quality | Passes | Rendered size | See-through strokes |
|---|---|---|---|
| Draft | 1 | the final size | stippled, as in a single frame on screen |
| Good | 8 | twice the size each way | smooth, a little grain left |
| Best | 32 | twice the size each way | clean |

In pixels drawn, Good is 32 times the work of Draft and Best 128 times. How much of that shows up as time depends on the graphics card. Measured on the development Mac (Apple M1 Max) with a sample of 2.4 million triangles:

| Picture | Draft | Good | Best |
|---|---|---|---|
| 1920 x 1080 still | 60 ms | 112 ms | 184 ms |
| 8192 x 8192 still | | | 2.8 s |
| 8192 x 4096 equirectangular | | | 3.2 s |
| 4096 x 4096 stereo equirectangular | | 1.0 s | |

A slower card, and above all a headset's, will be nearer the ratio of the pixel counts. Opaque strokes look the same at every quality apart from their edges; if a piece has no see-through strokes, Good is enough.

## Stills

"Capture image" in the Export panel.

| Control | What it does |
|---|---|
| Picture | Flat picture, or one of the four 360 layouts (below) |
| From | "Current view" (the desktop view, or the head in a headset) or a camera layer by name. For 360 pictures spawn areas are listed too |
| Size | The document's camera resolution, a size from the Document panel's list, or a typed width and height, 16 to 8192 each. 360 pictures offer a list of sizes in the shape of their layout |
| Eyes apart | Stereo only: the distance between the eyes in metres of the room the viewer stands in. Default 0.064 |
| Quality | Draft, Good or Best, with a line saying what the choice trades |
| Transparent background | Leaves the background out |
| File name, Folder | Where the PNG goes |
| Capture | Renders and saves, then reports the path through `editor.message` |

**Where a picture is taken from.**

- *Current view*: the window's own camera.
- *A camera layer*: its place and direction at the playhead (`scene.layer_transform`; a camera looks along its -Z) and its vertical field of view (`props.fov`). The layer's scale is taken out of the transform; as in Quill it changes nothing a lens does and only moves the clipping planes, which are 0.01 and 1000 of the layer's own metres. A mirrored camera layer gives an ordinary, unmirrored picture.
- *A spawn area* (360 only): its place and the direction it faces. A floor-level spawn area gives a point 1.6 of its own metres above it, where a standing viewer's eyes are.

**Where the file goes.** The default folder is `Limner` in the user's Pictures folder, made when first needed, and the default name is `<title>_<date>_<time>.png`, for example `Night garden_20261014_153012.png`. Both can be used without any dialog, which is what a headset needs. On the desktop the folder button opens the system's save dialog to choose both; a name typed into the box is used as it is. Videos default to `Movies/Limner` and 3D files to `Documents/Limner`.

## 360 stills

A 360 picture is made from six square pictures taken from one point, each a quarter turn wide: ahead, right, behind, left, up and down. They are rendered like any other picture, with the same passes and supersampling.

| Layout | Shape | What it is |
|---|---|---|
| Equirectangular | 2 : 1 | Longitude across, latitude down, the direction the capture faces in the middle. What video sites and viewers call "360" |
| Stereo, over and under | 1 : 1 | Two equirectangular pictures, the left eye's above the right eye's |
| Cube faces in a row | 6 : 1 | The faces side by side: right, left, up, down, front, back |
| Cube faces unfolded | 4 : 3 | A cross: up above front, down below it, and left, front, right, back in the middle row |

For the equirectangular layouts the faces are kept in linear light and unrolled by a shader (`capture_panorama.gdshader`), which takes four samples per pixel at Good and Best. Faces are rendered slightly wider than a quarter turn so that filtering at an edge finds real picture, and are a third of the picture's width, a little finer than the picture is where it is finest. Cube layouts need no resampling; the faces are placed as they are. Up and down are turned so that they join the front face.

A picture from "Current view" is levelled first: it keeps its place and compass direction but not the tilt of the head. Pictures from a camera layer or a spawn area use the layer's rotation as it is.

**Stereo is the simple kind.** The two eyes are two cube maps taken from points `eye separation` apart, side by side across the direction the capture faces. That gives correct depth ahead and reasonable depth to about 60 degrees either side. It is not omni-directional stereo: to the left and right the eyes are one behind the other, so there is no depth there, and behind the viewer the eyes are swapped, so depth is inverted, with a visible seam where the left and right edges of the picture meet. It suits pieces that are looked at mostly forwards. True omni-directional stereo needs a different eye position for every column of the picture and is future work.

This goes beyond Quill, which had no stereo 360 capture.

## Camera renders

"Render camera" in the Export panel renders a range of the timeline through a camera to a video. As in Quill 2.7, the render runs in a process of its own so the artist keeps working.

### How it works

1. **Render** saves the document as it is now, unsaved changes included, as a copy in `user://renders/` that is removed when the render ends. The user's own file is never written: saving it again would replace its picture. The copy is written by `LimnerScene.save_copy`, which neither moves the document to the copy nor counts its changes as saved. The view is saved with it, which is what "Current view" renders.
2. `RenderLauncher` starts a second copy of the application (`OS.get_executable_path()`, with `--path` when it is not an exported build) with Godot's Movie Maker options and Limner's `--render` options.
3. In that process `main.gd` hands over to `RenderJob`. On every frame the job places the playhead (`start + frame / fps`), renders the camera's view off screen with all the passes of the chosen quality, and shows the result full-window. Godot's Movie Maker records the window once per frame and writes the file.
4. The job reports progress in a small JSON file (`{state, frame, frames, error}`) that the launcher reads four times a second; the panel shows a progress bar and **Cancel**. Cancelling asks the job to stop, so that it closes its file properly and what was rendered so far stays playable; a job that does not answer within five seconds is stopped.

Edits made after Render is pressed are not in the video. The panel says so.

**Quality in video.** Movie Maker takes one picture per frame, so the passes have to happen inside the frame: the job renders them all before the frame is recorded, exactly as for a still. A frame at Good therefore costs what a still at Good costs. On the development Mac a 1280 x 720 render at Best ran at about 12 frames a second, and 24 frames of the 2.4-million-triangle sample at 1920 x 1080 and Good took about 5 seconds including starting the process and loading the project.

**The window of the render process** is only a progress display. Its content is drawn at the size of the video whatever size the window is (content scaling), so a video can be larger than the screen: 3840 x 2160 was rendered on a display 3456 pixels wide. The job draws each finished frame explicitly instead of waiting for the engine to do it, so rendering continues when the window is minimized, which is how the panel starts it. A minimized and a visible render of the test document gave identical frames.

**The frame size is fixed when the process starts.** Godot decides the size of a movie from the project's window size before any script runs; it cannot be given on the command line. The launcher therefore writes a three-line settings file that the new process picks up, and the new process removes it as soon as it has started (the launcher does too). The file is the one named by the project setting `application/config/project_settings_override` when that is set, and `override.cfg` in the project folder otherwise, which Godot always reads. The second lies inside the project for about a second per render; naming a file in `user://` in the project settings avoids that and also works for an installed application whose own folder cannot be written to.

### Panel controls

| Control | What it does |
|---|---|
| Camera | A camera layer by name, or "Current view" |
| Picture | 2D, 360, or 360 stereo (over and under). 360 uses the camera's position and its forward direction as the centre |
| Range | The document's working range, or a start and an end typed in seconds or in frames |
| Frame rate | The document's, or one from the Document panel's list |
| Size | The document's camera resolution, a size from the list, or typed. For 360 the width counts and the shape follows |
| Format | AVI, PNG sequence or OGV (below) |
| Quality | Draft, Good or Best, per frame |
| With sound | Records the document's sound |
| File name, Folder | Where the video goes; default `Movies/Limner` |
| Preview | Plays the range in a loop, with the main view kept behind the camera and given its lens (`CameraView.preview`). Pausing ends it |
| Render | Starts the render |

### Formats

These are what Godot's Movie Maker writes; the file ending chooses.

| Format | Contents | Notes |
|---|---|---|
| AVI (`.avi`) | Motion JPEG video and uncompressed sound | Opens almost anywhere and cuts cleanly, since every frame is a whole picture. Large. JPEG quality is the project setting `editor/movie_writer/video_quality`, 0.75 unless changed |
| PNG sequence (`.png`) | One lossless PNG per frame and a WAV | Goes into a folder of its own named like the file: `take.png` gives `take/take00000000.png`, `take/take00000001.png`, ... and `take/take.wav`. The best source for editing and encoding |
| OGV (`.ogv`) | Theora video and Vorbis sound | Small, open formats, visibly compressed. For previews |

**There is no H.264 or H.265.** Godot ships no encoder for them, and Limner installs nothing. To get an MP4, render a PNG sequence or an AVI and encode it yourself. With ffmpeg, which is not part of Limner and which nothing here depends on:

```bash
# from a PNG sequence rendered at 24 fps
ffmpeg -framerate 24 -i take/take%08d.png -i take/take.wav \
       -c:v libx264 -crf 18 -pix_fmt yuv420p -c:a aac -shortest take.mp4
# from an AVI
ffmpeg -i take.avi -c:v libx264 -crf 18 -pix_fmt yuv420p -c:a aac take.mp4
```

A 360 video also needs to be marked as one before a video site shows it as a sphere (Google's Spatial Media Metadata Injector does that); the encoded file itself is an ordinary video.

### The command line

A render can be scripted without opening the application. One command is enough:

```bash
scripts/godot.sh --headless --path godot -- --render --open piece.limn --out piece.avi
```

| Option | Meaning | Default |
|---|---|---|
| `--open <project>` | A `.limn` folder, a `.limnz` file or a Quill project | required |
| `--out <file>` | The video: `.avi`, `.png` (a sequence) or `.ogv` | required in this form |
| `--camera <id\|path\|view>` | A camera layer by id or by path (`Root/Shots/Wide`), or `view` | the first camera layer; without one, the view the document was saved with |
| `--range <start_s> <end_s>` | Seconds; the end is not included, so 0 to 1 at 12 fps is 12 frames | the working range |
| `--fps <n>` | Frames per second | the document's rate |
| `--size <w>x<h>` | 16 to 8192 each; for 360 the width counts | the document's camera resolution |
| `--quality draft\|good\|best` | | `good` |
| `--projection flat\|equirect\|equirect_stereo` | | `flat` |
| `--no-sound` | Record silence | sound on |
| `--minimized` | Keep the render window out of the way | off |
| `--status <file>` | Where to write progress as JSON | none |

Started like this the process does not render. It works out the size, starts the rendering process with the right engine options, passes on its progress and ends with its result: exit code 0 when the video is written. Because it draws nothing, `--headless` is fine for it; the rendering process it starts does need a display. Progress is printed as `render: frame N of M`.

What it starts is the other form, which can also be used directly:

```bash
scripts/godot.sh --path godot --write-movie /abs/piece.avi --fixed-fps 24 --disable-vsync \
    -- --render --open piece.limn
```

Here Godot's own options choose the file and the frame rate, the folder of the file must exist, and the frames have the size the process started with: the project's window size, 1440 x 900, unless a settings file says otherwise (above). The job says so when `--size` or the document's resolution asks for something else. `--fixed-fps` takes whole numbers only, and Movie Maker records one frame's worth of sound at that rate for every frame. For a rate such as 23.976 the launcher gives Godot 24 and the job the exact rate, so every frame shows the right instant and every drawn frame appears once; the job plays the sound faster by the same proportion (`AudioServer.playback_speed_scale`, here 1.001, 1.7 cents higher), so picture and sound stay together in a file labelled 24 that runs 0.1% short. An editor that conforms the file to 23.976 gets the exact timing back.

## The Monitor

The Monitor panel is a viewfinder for camera layers, as in Quill 2.7.

- **The picture** is live: a small viewport that shares the 3D world with the main view (`SubViewport.world_3d`), rendered only while the panel is on screen. It has the shape of the document's camera resolution, as large as fits, centred, with black bars where the shape leaves room. It shows the room as the main view does, so the floor grid and gizmos are in it; saved pictures are without them.
- **Camera chooser**, with previous and next. Without a camera layer the panel says so and offers to add one.
- **Framing**: lines dividing the picture 3 x 3 or 4 x 4.
- **Resolution**: the document's camera resolution, from the Document panel's list or typed. It writes `set_document {camera_resolution}`, so it is the same setting, undoable like any other.
- **Use this camera for transforms** makes the camera layer the transform target (`set_transform_target`), so grabbing moves it.
- **Look through camera** keeps the main view behind the camera, with its lens, until switched off (`CameraView.follow`). The window's camera never moves in Limner; navigation moves the artwork. So looking through a camera means moving the artwork until the layer sits where the head is. While it is on this is repeated every frame, which follows an animated camera and means navigating by hand has no lasting effect. In a headset the head cannot be given a place or a lens; there the viewer is stood where the camera is, once.
- **Export current frame** saves what the camera sees now, at the document's camera resolution and Good quality, to the default pictures folder.

## Export 3D file

The third section of the Export panel writes the artwork through `LimnerScene.export_file(path, options)`; what each format holds is in `docs/export.md`.

| Control | Applies to | Option sent |
|---|---|---|
| Format, with a line on what it is good for | | the file ending: `.glb`, `.usda`, `.usdz`, `.fbx`, `.obj` |
| Still or Animated | all but OBJ | `animation`; a still also sends `start`, the playhead. Animated covers the working range at the document's rate |
| Hidden layers too | all | `include_hidden` |
| Bake transforms, Lights, Cameras | all but OBJ | `bake_transforms`, `lights`, `cameras` |
| Unit, Colours | FBX | `fbx_unit` (`metres`, `centimetres`), `fbx_colors` (`srgb`, `linear`) |
| Visibility extension, Animate opacity | glTF | `gltf_node_visibility`, `gltf_animate_opacity` |

`export_file` also takes `end` and `rate` (frames per second, or `{num, den}`). An option it does not know is an error that names it. It returns `{ok, error, nodes, meshes, vertices, triangles, animation_frames, bytes}`, which the panel shows.

The export runs in the window's own thread. The bridge is not built for calls from other threads, so the panel shows "Writing…" for a frame and then blocks until the file is written; when that took more than half a second the report says the window did not respond meanwhile.

From the command line:

```bash
cargo run -p limner-cli -- export <input> <output> [--animated] [--start <seconds>] [--end <seconds>]
    [--fps <n>] [--include-hidden] [--bake] [--no-lights] [--no-cameras]
    [--fbx-unit metres|centimetres] [--fbx-colors srgb|linear]
```

Without `--animated` it writes a still at `--start` (default: the start of the working range); with it, the working range or the range given, at the document's rate or `--fps`. It prints the same counts.

## Compared with Quill

| | Quill 2.9 | Limner |
|---|---|---|
| Still size | Up to 8k x 8k | Up to 8192 x 8192, rendered in tiles |
| See-through strokes in captures | As on screen | Averaged over 8 or 32 passes, supersampled |
| Transparent background | No | Yes, straight alpha |
| 360 stills | Equirectangular, cube map, vertical strip | Equirectangular, cube faces in a row, cube faces unfolded |
| Stereo 360 | No (a long-standing request) | Stills and video, over and under, by the simple method |
| Camera render | Separate process; H.264, VPX, PNG sequences | Separate process; Motion JPEG AVI, Theora OGV, PNG sequences. No H.264 |
| Scripted render | No | One command |
| Hand-held recording ("Capture Animation") with smoothing and filters | Yes | No; cameras are layers and are animated with keys |
| Monitor | Viewfinder, grids, resolution, export frame | The same, plus "Look through camera" in the main view |

## Known limits

- **No H.264.** See "Formats" for how to encode one yourself.
- **Motion JPEG is large**, and its quality is a project setting, not a control in the panel.
- **OGV is visibly compressed** at Godot's default quality.
- **Whole frame rates only in the file's label.** Fractional rates are sampled exactly but labelled with the nearest whole rate.
- **Simple stereo.** No depth to the sides, inverted depth and a seam behind the viewer.
- **Cube faces have their own dither pattern each**, so a Draft 360 picture can show the stipple changing at face edges. Good and Best average it away.
- **The window does not respond while a still renders**, and playback is not paused for it. Stills are fast on a desktop card (table above) but will take noticeably longer on slower ones.
- **The selection highlight is not hidden.** If strokes are selected when a picture is taken, they are highlighted in it. Deselect first.
- **The Monitor shows the grid and gizmos**, like the main view, because it shares its world. Putting the grid, the tool cursor and gizmos on a render layer other than 1 would remove them from the Monitor and make hiding them for captures unnecessary; the capture and Monitor cameras already look at layer 1 only.
- **Edits during a preview or while looking through a camera** are possible, but navigation is held.
- **A cancelled render** may hold one frame more than reported: the frame on screen when it stops is recorded once more.
- **One render at a time** per window.
- **Sound** is whatever the application's audio bus plays while the document plays. The job sets the document playing for that reason, while still placing the playhead itself on every frame.

## What has and has not been verified

Verified on the development Mac (macOS, Apple M1 Max, Godot 4.8 dev 7):

- **Tests.** `godot/tests/test_capture.gd`, `test_export_panel.gd` and `test_monitor_panel.gd`: 26 tests. Headless, the ones that need to render say they are skipped; run without `--headless` they all run and pass: a still has exactly the size asked for and is written as a PNG of that size; the grid, the dither setting, gizmos and onion skins are as they were afterwards; a transparent still has no alpha where there is no artwork, full alpha on opaque strokes and partial alpha on see-through ones; a picture in two tiles has no seam; Good is less than half as rough as Draft and Best smoother still; every 360 layout has its size and what is ahead is in the middle; the panels send the commands and options their controls say; a 3D file of the demo sheet in every format has non-zero counts and the reported size.
- **By eye** (`godot/tests/shots/capture_shots.gd` writes these to `local/out/`): the demo sheet at Draft (see-through chevrons stippled) and at Best (smooth), with a transparent background; a 360 picture of a sample project in all four layouts, with the equirectangular picture and the unfolded cube agreeing on what is where and no seams between faces; the stereo picture with the left eye's view shifted the right way; the Export panel in its three sections and the Monitor with a camera.
- **Video.** A one-second document built by commands (two paint layers, one with a transform key at each end, and a camera), rendered at 12 fps and 320 x 180 from the Export panel as a user would: the AVI's header says 12 frames of 320 x 180 at 12 fps, compression MJPG, and the file holds 12 pictures and 12 pieces of sound; the PNG sequence is 12 files of 320 x 180 and a WAV. The first and last frame show the keyed layer on the left and on the right, the see-through stroke smooth, and no grid, gizmo or panel. The document was still unsaved afterwards and the copy made for the render was gone.
- Also rendered through the command line: OGV (12 frames, Theora and Vorbis, read back with ffprobe), 3840 x 2160 on a smaller display, a stereo 360 frame, a minimized render identical to a visible one, a cancelled render (playable, 64 frames after "63 of 18000"), and the refusals for a wrong format, camera or quality.
- `cargo test -p limner-cli`: argument handling of `limner export` and a round trip of a synthetic document through every format.

Not verified:

- **Anything in a headset.** Stills are rendered by asking the engine to draw several times within a frame; whether that disturbs an OpenXR session's frame timing has to be tried on real hardware. "Look through camera" and Preview fall back to standing the viewer at the camera there, which has only been read, not run.
- **Sound in videos.** Sound layers cannot be imported in this build yet. The sound stream is written (it is silence in every test file), but that a sound layer is heard in a render, and in time with the picture, has not been tested.
- **Windows and Linux.** The launcher uses only portable engine calls, but it has only been run on macOS. In an installed application whose folder cannot be written to, the settings file needs the `user://` location described under "How it works".
- **Rendering a saved, unchanged project in place.** The launcher then saves the project again and renders from it; the tests and checks above all went through the temporary copy.
- **Speed in the background on macOS.** In one of about ten runs a minimized render of 12 small frames took 25 seconds instead of one; the machine was busy with other work and it did not happen again. macOS slows processes it considers idle, and whether that applies to a minimized render has not been established.
- **Players and editors.** The files were read back with ffprobe and ffmpeg and by parsing the AVI header; they were not opened in video editors.
- **360 pictures in a viewer.** Orientation and layout were checked by eye on the flat pictures, not in a 360 viewer or a headset.
