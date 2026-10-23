# The headset layer

Limner in a headset is the same program as on the desktop with a different thin layer on top. Tools, undo, layers, grabbing the world, what every button and stick does: all of that is in the Rust core and is shared. This page describes the layer that is only there in a headset, how to run it, how to build it for a Quest, and, honestly, what has never run on real hardware.

Everything here was built and tested on a Mac that has no headset and no OpenXR runtime. **No line of the headset layer has been seen through a headset yet.** The last section says exactly what that means and what to do in the first session with one.

## How it is put together

```
godot/xr/
  xr_shell.gd         XRShell: starts the rig, feeds the core every frame, acts on its answers
  rig.gd              XRRig: head and hands in the room, whatever supplies them
  openxr_rig.gd       OpenXRRig: a real headset through OpenXR
  sim_rig.gd          SimRig: mouse and keyboard, or a script, standing in for a headset
  sim_director.gd     places the simulated rig for screenshots (the --sim-… options)
  holding_view.gd     XRHoldingView: what the headset shows while the desktop is in use
  hand_state.gd       what one controller is pressing at one moment
  action_map.tres     the OpenXR actions and their bindings for each kind of controller
  palette.gd          XRPalette: the panels in the other hand, pinning, the keyboard's place
  panel_surface.gd    XRPanelSurface: one panel drawn into a viewport and shown as a sheet
  panel.gdshader      draws a sheet sharply up close and without shimmer far away
  pointer.gd          XRPointer: poking, the ray, picking panels up
  keyboard.gd         XRKeyboard: the keys that type into a panel's text box
  button_map.gd       XRButtonMap: the sheet that lists every control
  controller_model.gd the shapes drawn for the controllers
  controller.gdshader
godot/editor/preferences.gd   app-wide settings in user://settings.json (shared with the desktop)
godot/editor/sounds.gd        interface sounds made in code (shared with the desktop)
godot/editor/mode_switch.gd   ModeSwitch: whether a headset can be changed to, and the buttons' words
godot/ui/settings_panel.gd    the Settings panel
godot/main.gd                 LimnerApp: builds one shell or the other, and changes between them
```

**Viewer space.** Everything Godot draws is in viewer space: the room the user stands in, in metres, Y up. The XR origin sits at the room's origin and never moves, and the world scale stays 1. Navigation is done by the core, which changes where the artwork is in the room. So controller poses go to the core exactly as OpenXR reports them.

**One frame** (`XRShell._process`):

1. The rig reads both controllers into two `XRHandState`s.
2. The `dominant_hand` preference says which is the drawing hand.
3. The palette follows the other hand.
4. The pointer looks at where the drawing hand's tip is relative to the panels. If the hand is over a panel (hovering, pressing, pointing a ray at it or holding it), the hand belongs to the panel.
5. The core is told about both hands and the head (`input_hand`, `input_head`). A hand over a panel is reported as present but pressing nothing, so it cannot paint, grab the world, resize the tool or delete a stroke.
6. `input_commit` runs the tools and returns feedback: haptic pulses go to the right controller (if the `haptics` preference is on), sound names go to `Sounds`, messages go to a line of text above the palette, and "menu" shows or hides the palette.

**Who has the trigger.** A trigger pulled while the hand is over a panel stays with the panels until it is let go, even if the hand slides off; a trigger pulled in open air stays with the tool until it is let go, even if the stroke crosses a panel (and then the panel ignores the hand). The same goes for the grip. This is what keeps a stroke from being cut short by a panel and a panel press from starting a stroke.

**The brush tip** is `XRRig.TIP_OFFSET` (3.5 cm) in front of the controller's OpenXR *aim* pose. It is both where paint appears and what pokes the panels.

**Starting.** `XRShell.is_available()` is true when the OpenXR interface is initialised (Godot does that at start-up when the project has OpenXR switched on) or can be initialised now. It is cheap and, when it fails, undoes the half-made start. `XRShell._ready` then switches the main viewport to XR, turns V-Sync off, and when the session begins asks for the best refresh rate the headset offers out of 120, 90 and 72 Hz. The session's states are followed: without the user's attention (headset off the head, system menu open, session stopping) both hands are reported absent; a recentre is left to the system and does not move the view, because going back to the initial spawn area is something the user asks for in Settings. If starting fails, `started` stays false, `failure` holds the reason, the reason is sent through `editor.message`, and `start_failed` is emitted, so that `main.gd` can fall back to the desktop.

**Quitting and unsaved work.** When the headset's system ends the session (`XRRig.closed`, for instance when the user quits from the system menu) there is no moment left to ask anything, and closing the desktop mirror window quits at once too. In both cases `XRShell.keep_unsaved_work` first copies unsaved work to the recovery file (`Autosave.keep_for_next_time`), and the next start in the headset opens it again, still unsaved; a desktop start offers it. Nothing is asked in the world before quitting, because neither way of quitting leaves time for an answer.

**The mirror.** The desktop window shows one eye, which is what Godot does by itself. The shell sizes the window to 1280 x 720, names it "… — Limner (headset)" and puts one line of text at the bottom saying that the session is in the headset.

**Changing to the desktop and back.** The work can move between the two interfaces while the app runs: "Continue in the headset" is a button in the desktop's top bar (the headset icon), an entry at the bottom of its View menu and a row at the end of the Settings panel; in the headset the same Settings row says "Continue on the desktop". A shell asks with its `switch_requested` signal and `main.gd` (`LimnerApp.switch_to`) makes the change a frame later:

- A stroke or grab in progress is finished or taken back (`input_cancel`).
- The shell in use is taken down with everything it put in the world (`stop()`): the desktop's camera and tool cursor, or the headset's rig with the palette, pointer, cursor and button map under it. Freeing them also drops their connections to the editor and to `Kit`.
- The other shell is built around the same world and editor, so the document, its undo history, the selection, the current layer and the view carry over. It is built as a switch (`switched`): no recovery question or automatic recovery, no start-up preferences applied again (they would reset the gimbal), no button map. One `Autosave` lasts the whole run (`LimnerApp.autosave`), so the recovery copy written during ordinary work is never mistaken for work left by a bad ending.
- If the headset does not start, the desktop is built again and says why, as at start-up.

The headset is offered (`ModeSwitch.headset_target()`) when OpenXR is running or has found a headset system; on a Mac, or with `--sim`, the simulated rig is offered instead. Otherwise the buttons are greyed out and say why: "Start Limner without --xr-mode off, with the headset's software running, to use a headset." OpenXR cannot be started once the app runs without it, and in this build of Godot it cannot be stopped and started again either, so the session stays alive for the rest of the run once it has begun:

- Going to the desktop, `OpenXRRig.stop()` gives the main viewport back (XR off, its drawing size set again, V-Sync and the frame-rate cap from the project settings) and the window takes back the size, place and mode it had before the headset. The headset then shows `XRHoldingView`: a viewport of its own marked for XR, with an empty dim room and a sheet saying "Limner is on the desktop. Pull a trigger to carry on here." Without it the headset would go blank and Godot would complain every frame that no viewport is marked for XR. The trigger only counts once it has been let go since the view appeared, so the press that left the headset does not bring it straight back.
- Coming back, the rig is built again on the live session. The runtime does not announce focus a second time, so `OpenXRRig.start` reads the session's state and starts with the hands working when it is already focused.
- If the headset's own menu ends the session while the desktop is in use, the app does not quit: the holding view goes, the desktop says so, and the headset is no longer offered (`ModeSwitch.headset_ended`).

### Project settings the headset needs

These are in `godot/project.godot`. Without them `XRShell.is_available()` is always false and only the simulated rig runs.

```
[xr]

openxr/enabled=true
openxr/enabled.macos=false
openxr/default_action_map="res://xr/action_map.tres"
openxr/startup_alert=false
shaders/enabled=true
```

- `enabled.macos=false`: there is no OpenXR runtime for macOS, and with OpenXR on Godot prints five lines of errors at every start there. (Checked on the development Mac.)
- `startup_alert=false`: without a headset Godot would otherwise put up a system dialog; Limner falls back to the desktop by itself.
- The reference space is left at Godot's default, Stage: the origin is the middle of the play area, on the floor.

One thing to know before switching this on: with `openxr/enabled=true`, Godot starts the OpenXR runtime every time Limner starts, before any script runs. On a PC where SteamVR is the OpenXR runtime that can launch SteamVR even for a desktop session. Starting with Godot's own `--xr-mode off` (before the `--`) prevents it: `scripts\godot.ps1 --xr-mode off --path godot '--' --desktop`. In PowerShell the `--` has to be in quotes: a bare one is dropped before the script sees it, and the app's own options are then lost.

## The controls, as built

The core decides what the controls do (`crates/limner-core/src/session/gesture.rs` and `grab.rs`); the same list is shown in the headset by the button map (`godot/xr/button_map.gd`), which has to be kept in step by hand.

| Control | Drawing hand | Other hand |
|---|---|---|
| Trigger | Use the tool, with pressure | The modifier (see below) |
| Grip | Take hold of the view, or of what is being transformed (the transform object, or the selection with the Selection tool in hand). While drawing a line: move where it starts | The same; both grips together also scale |
| Grip and stick up / down | Zoom or scale quickly about the hand | The same |
| Stick up / down | Tool size, larger / smaller | New frame / hold frame |
| Stick left / right | Undo / redo, repeating while held. Redo after moving a copy moves another copy as far again (`transform_again`) | Previous / next frame |
| Stick click | Show or hide the gimbal: the transform object's, the current layer's while the view is the transform object, or the selection's. Turned on with nothing to move, it says "Nothing to transform: choose a layer, or select strokes" | Play or pause |
| Lower button (A / X) | Delete the selection or the nearest stroke | Switch between the Selection tool and the last tool |
| Upper button (B / Y) | Pick the colour of the nearest stroke | Show or hide the palette |
| Menu button (a press) | Show or hide the button map (either hand's, where it has one) | |

In painting mode the other hand's stick and its click do nothing.

With the modifier (the other hand's trigger) held:

| Doing | Does |
|---|---|
| Paint | Draws a straight line in the brush's style, at any angle (no snapping) |
| Line | Snaps the ends of the line to the brush grid |
| Selection | A half pull adds to the selection, a full pull takes away |
| Thicken | Thins |
| Drawing hand's stick up / down | The inner sphere, for tools that have one |
| A grip taking hold | Moves a copy: of the selection, or of the layer being transformed |
| Other hand's stick | Left / right: rewind / fast forward. Up / down: copy the frame / delete it |

On the gimbal, with the drawing hand:

| Handle | Grip | Trigger | Trigger with the modifier |
|---|---|---|---|
| Arrow | Move along that axis | The same | Move the pivot along it |
| Square | Move in that plane | The same | Move the pivot in that plane |
| Ring | Turn about that axis | Turn in snapped steps (`rotation_snap_steps`) | Turn a copy in steps |
| Centre | Move and turn freely, as a grab does | Move freely | Move the pivot freely |

Until 0.2.1 the trigger on an arrow, a square or the centre moved the pivot. With the grip not working in a headset (see Bindings), that was all a tester could do to a gimbal, and it looked broken: the gimbal moved and the thing did not.

Over a panel the drawing hand's controls mean something else, and the core sees none of them:

| Control | Over a panel |
|---|---|
| Tip within 6 cm in front | Hover |
| Tip pushed 3 mm into a key, or 3 mm through the flat of the sheet | Press; pulled back 6 mm in front of the key's top: release. A short buzz on the press. The key goes down with the tip |
| Pointing from farther off, up to 2 m | A ray and a dot; the trigger is the button. The grip, the stick's left and right and the upper button still work the artwork |
| Modifier (the other hand's trigger) held | Presses come with Ctrl: a tap adds a key or a row to the selection, as a Ctrl click does |
| Stick up / down | Scroll (or resize a panel that is being held) |
| Lower button | The other mouse button: opens a context menu where there is one |
| Grip, with the tip close | Pick the panel up; it stays where it is let go |

### Bindings

`godot/xr/action_map.tres` has one action set, `limner`, with these actions for both hands: `aim_pose`, `grip_pose`, `trigger`, `squeeze` (the grip), `stick`, `stick_click`, `stick_touch`, `trackpad`, `trackpad_click`, `lower_button`, `upper_button`, `menu_button`, `haptic`.

No action may be called `aim`, `grip` or `default`. Godot names the pose actions so as it makes them (`aim_pose` becomes `aim`, `grip_pose` becomes `grip`) and then refuses any later action with a name already taken, together with every binding to it, with no more than a line in the log. In 0.2.0 the grip action was called `grip`: in the first real headset (a Quest 3 over Link) no grip did anything, so the artwork, a selection, a layer and the panels could not be grabbed. `test_every_action_the_rig_reads_is_made_and_bound` in `test_xr.gd` reads the action map the way Godot does and fails on such a clash.

**Bare hands.** A headset that tracks hands with its cameras, as a Quest does, can hand them to the app as controllers when the controllers are put down: the runtime binds them to `khr/simple_controller` (pinching is its select, our trigger) or to a hand-interaction profile. Unless the `hand_tracking` preference ("Bare hands as controllers" in Settings) is on, `OpenXRRig` treats a hand bound to one of `OpenXRRig.HAND_PROFILES`, or one whose hand tracker says its data comes from the cameras, as not tracked: it does nothing and is not drawn into the core. A controller that only offers the simple profile needs the preference on.

| Controller (OpenXR profile) | Lower | Upper | Menu | Notes |
|---|---|---|---|---|
| Oculus Touch (`oculus/touch_controller`): Rift CV1, Rift S, Quest 1 and 2 | X / A | Y / B | left menu button | |
| Touch Pro (`facebook/touch_controller_pro`), Touch Plus (`meta/touch_controller_plus`) | X / A | Y / B | left menu button | Same paths as Touch |
| Pico 4 (`bytedance/pico4_controller`) | X / A | Y / B | left menu button | |
| Valve Index (`valve/index_controller`) | A | B | none | Grip is the squeeze value |
| HTC Vive wand (`htc/vive_controller`) | none | menu button | none | The trackpad is the stick, but only while it is pressed down; grip is a click |
| Windows Mixed Reality (`microsoft/motion_controller`) | trackpad press | menu button | none | Grip is a click |
| HP Reverb G2 (`hp/mixed_reality_controller`) | X / A | Y / B | menu button | |
| Generic controller (`khr/generic_controller`, OpenXR 1.1) | primary | secondary | none | |
| Khronos simple controller (`khr/simple_controller`) | none | menu | none | Select is the trigger. Enough to paint and open the palette, no more. Runtimes give it to bare hands, so it is ignored unless "Bare hands as controllers" is on |

Where a controller has no lower button, deleting with a button and toggling the Selection tool are not available on it; where it has no menu button, the button map is opened from Settings. The Steam Frame controller has no profile here because its inputs could not be checked offline; SteamVR's own rebinding should cover it.

The action map was written by a script through Godot's own classes and read back by Godot 4.8 dev 7. Whether each runtime accepts each profile can only be seen with that runtime.

## The palette

The palette follows Quill's: raised buttons that go down when pushed, one panel at a time, round buttons to change panels, signs on the hand, and panels of one fixed size on which nothing scrolls. The reasons are in `docs/superpowers/specs/2026-10-14-headset-palette-design.md` (what was measured from a recording of Quill) and `docs/superpowers/specs/2026-10-22-quill-style-panels-design.md` (the panels' own layouts and rules).

- **Sheets.** Every panel is the same `Control` the desktop docks, placed in a `SubViewport` of its own (`XRPanelSurface`) under a grey title bar and above a line for tooltips, on a sheet 8 mm thick. A sheet is as large as its panel, at 1.5 of the panel's design pixels to the millimetre (`PIXELS_PER_METRE`): a key of 38 pixels is 25 mm, the Tools panel 31 cm wide. (Until 0.2.1 it was 1.25, keys of 30 mm as Quill's measured; the first tester in a headset found the panels too large.) Every panel has one fixed size and nothing on a sheet scrolls; a list longer than its room shows a page at a time, and the stick over it steps the pages. In the headset the panels take a look of their own (`Kit.headset`): buttons with a dark outline, a larger text, and a colour scheme of their own, light unless changed (`headset_theme`).
- **Keys.** Every button of a sheet stands 5 mm off it as a key (`XRPanelKeys`): the top shows the button as the panel draws it, the sides take its outline's colour. The tip pushes a key down with it; 3 mm down it presses, and it bottoms out 1 mm above the sheet. A button that is on (the chosen tool) sits half down; a disabled one is flush. Check boxes, sliders, lists and text boxes stay flat, as Quill's do. Keys under an open pop-up stay down. `key.gdshader` draws them with the same three sharpness levels as the sheet.
- **Sharpness.** A panel is drawn at twice its own pixels (`TEXELS_PER_PIXEL`), about 17 to a degree at arm's length, so that text and icons are as sharp as the headset can show. A viewport's picture has no mip-maps, so each sheet keeps two smaller copies of itself in two more viewports, and `panel.gdshader` blends between the three by how far away the sheet is. Text is sharp at arm's length and does not shimmer on a pinned panel across the room.
- **Drawing only when needed.** A sheet is redrawn while the pointer is on it, for 0.4 s after the document or the editor state changed, while a pop-up is open in it, while the piece is playing, and otherwise twice a second. Hidden sheets are never drawn, and the panel on a hidden sheet is hidden as well (`XRPanelSurface` follows its own visibility), so that a panel that works only while it is seen, such as the Monitor's live picture, stops when its sheet is put away or another panel is brought to the front. In its own viewport a panel would otherwise count as shown for ever.
- **Round buttons.** Under the panel, Quill's Navigation: round keys (29 mm across, 9 mm tall) in two staggered rows, below the panels used while painting (Tools, Color, Layers, which holds the timeline, Transform, Monitor), above the rest (Document, Settings, Performance, Export). The Layer and Text panels have none: a layer's kind key in the Layers panel shows the Layer panel, and the Text key of the Tools panel shows Text. The panel in front has its button down; a panel left in the room has its icon in the accent colour. The name of the button under the tip shows in a tag over them. A panel whose script is missing or does not load is left out, so the palette works while panels are being written.
- **Where it is.** Above the hand that holds it, as in Quill: the round buttons centred 10.5 cm above the hand, the panel above them, 3 cm toward the middle of the body, leaning back 20 degrees and turned 18 degrees in toward the eyes (`palette.gd`).
- **Pinning.** The pin at the right of a panel's title bar leaves the panel where it is in the room; on a panel left in the room the same key sends it back, as does its round button. The grip, with the tip close to a panel, picks it up: it follows the hand, the stick resizes it (0.5 to 3 times), and it stays where it is let go. Pointing at a panel from afar never takes the grip. Any number can be pinned. Pinned panels stay when the palette is put away.
- **The hand's signs** (`XRHandDial`). On the hand that holds the palette a see-through disc faces the eyes, with a dark ring and the frame number of the current layer in it (none unless it is a paint layer), and a tag over it with the layer's name. While the thumb rests on that hand's stick (`thumbstick/touch` on Touch and Index controllers) or pushes it, icons around the ring show what the stick does, and what it does with the modifier held; the direction pushed lights up. Painting mode, which takes that stick away, hides them.
- **Pop-ups.** Panels have no drop-downs (a choice among many is a cycler, the value with a key at each side) and no context menus in a headset. The colour pickers of colour wells still open inside the panel's own viewport; a sheet grows while one larger than it is open, and a press beside it closes it.
- **The button map** shows one of five pages (drawing hand, other hand, modifier, gimbal, panels), chosen with keys at its top.
- **Typing.** When a `LineEdit` or `TextEdit` on any panel (or in a pop-up of it) is being typed into, a keyboard sheet appears in front of the user, low, like a laptop's: 42 cm ahead of the eyes and 26 cm below them, facing them (`XRPalette.KEYBOARD_AHEAD`, `KEYBOARD_BELOW`). It stays put in the room until typing ends, so it holds still while the palette hand moves, and its keys are 1.2 times a panel's (about 26 mm). What is typed shows over the keys, with a bar at the caret, so the eyes can stay on the keyboard. Its keys send ordinary key events to the box. It goes when typing ends, or with its own key. (Until 0.2.1 it stood beside the panel and rode on the palette; it was found awkward to reach.)
- **Tooltips.** The tooltip of the control under the pointer appears on the line under the panel after half a second.
- **Files.** The Document panel's File section saves, saves as, starts, opens and imports in one documents folder (`Documents.folder()`: `Documents/Limner`, or `user://documents` on a Quest), with no system dialog: a name is typed on the keyboard sheet, the projects there are listed newest first with their dates, and "save the changes first?" is asked in the panel. See "Files and media" in `docs/interface-api.md`.
- **Messages** from the core and the editor appear for a few seconds on a line above the palette, whether or not the palette is out.

### What in the panels cannot work in a headset

Found by opening every drop-down and picker on every panel through the simulated rig:

- (Fixed.) `godot/ui/save_target.gd` used to ask its own viewport whether a headset was in use to hide its "browse" button. On the palette that viewport is the panel's own `SubViewport`, which never uses XR, so the button stayed and opened a system file dialog nobody could see. It now asks the main viewport (`get_tree().root.use_xr`). Any other panel code that wants to know whether it is in a headset must ask the same way.
- The colour picker (`ColorField`) works, but it is 308 x 610 pixels, taller than most panels, so the sheet grows to hold it. Its eyedropper samples the screen and has nothing to sample in a headset. A smaller picker, or the Color panel's own square, would serve better.
- (Changed.) The layer list's context menu, which a headset opened with the lower button, is gone from the headset: every entry is a key of the Layers, Transform or Layer panel. The desktop keeps it as a right-click shortcut.

## The simulated rig

`-- --xr --sim`, or `-- --xr` on a machine without a headset, runs the same shell with `SimRig`: a camera at head height and two controllers driven by the mouse and keyboard. The controls, the script format and the screenshot options are listed at the top of `godot/xr/sim_rig.gd`.

```bash
scripts/godot.sh --path godot -- --xr --sim --demo                                   # try it by hand
scripts/shot.sh local/out/xr_tools.png --xr --sim --demo --sim-left -0.4,1.45,-0.5 --sim-panel Tools --sim-touch Eraser
scripts/godot.sh --headless --path godot --script res://tests/run.gd -- xr           # the tests
```

`SimRig.play(steps)` performs a list of timed controller states at a fixed 90 steps a second; `godot/tests/test_xr.gd` uses it to poke buttons, drag a slider, pin a panel, type on the keyboard and paint.

## Running on a PC with a Rift or a Quest

1. On the PC: `scripts\fetch-godot.ps1`, then `cargo build -p limner-godot`.
2. Add the `[xr]` settings above to `godot/project.godot` if they are not there.
3. Make one runtime the active OpenXR runtime:
   - **Rift, Rift S, or a Quest over Link / Air Link:** in the Meta Quest Link (Oculus) PC app, Settings > General > "Set Meta Quest Link as active OpenXR runtime".
   - **SteamVR** (Index, Vive, WMR, or a Quest through Steam Link or Virtual Desktop): SteamVR > Settings > OpenXR > "Set SteamVR as OpenXR runtime".
4. Put the headset on standby with its controllers awake, then start: `scripts\godot.ps1 --path godot`. With a headset found the session starts in it; `-- --desktop` keeps it on the desktop; `-- --xr` asks for the headset and falls back to the simulated rig with a note when none is found.
5. To see why a headset was not found, start with `-- --xr --no-sim`: the reason is shown and the app falls back to the desktop.

## A standalone Quest build

Nothing below has been run. It is the list of what is needed, in order, with the commands. The repository's scripts download none of it: `scripts/package.sh` and `scripts\package.ps1` keep only the desktop part of the export templates, so for a Quest take the whole download as in step 1.

1. **Export templates** for the pinned Godot (see `scripts/godot-version.txt`). In the editor: Editor > Manage Export Templates > Download and Install. By hand:
   ```bash
   curl -LO https://github.com/godotengine/godot-builds/releases/download/4.8-dev7/Godot_v4.8-dev7_export_templates.tpz
   # unpack its "templates" folder to:
   #   macOS    ~/Library/Application Support/Godot/export_templates/4.8.dev7/
   #   Windows  %APPDATA%\Godot\export_templates\4.8.dev7\
   #   Linux    ~/.local/share/godot/export_templates/4.8.dev7/
   ```
2. **Android build tools.** A JDK 17 and the Android SDK with the packages Godot's "Exporting for Android" page lists for this version. For Godot 4.5 they were:
   ```bash
   sdkmanager --sdk_root="$ANDROID_HOME" "platform-tools" "build-tools;35.0.0" "platforms;android-35" "cmdline-tools;latest" "cmake;3.10.2.4988404" "ndk;28.1.13356709"
   ```
   Check the page for 4.8 before installing; the numbers move. Then in the editor, Editor Settings > Export > Android: set the Java SDK path and the Android SDK path. A debug keystore, if there is none:
   ```bash
   keytool -keyalg RSA -genkeypair -alias androiddebugkey -keypass android -keystore debug.keystore -storepass android -dname "CN=Android Debug,O=Android,C=US" -validity 9999 -deststoretype pkcs12
   ```
3. **The Rust extension for Android.**
   ```bash
   rustup target add aarch64-linux-android
   cargo install cargo-ndk
   export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.1.13356709"
   cargo ndk --target arm64-v8a --platform 29 build -p limner-godot --release
   cargo ndk --target arm64-v8a --platform 29 build -p limner-godot            # for a debug export
   ```
   This writes `target/aarch64-linux-android/{release,debug}/liblimner_godot.so`, which is where `godot/limner.gdextension` looks (`android.release.arm64`, `android.debug.arm64`).
4. **The Android build template.** In the editor: Project > Install Android Build Template. It creates `godot/android/`, which is git-ignored. The Quest preset uses a Gradle build because the vendors plugin needs one.
5. **The Godot OpenXR Vendors plugin**, which adds what Meta's devices need to the app's manifest and Meta's extensions. Take the release that names this Godot version from https://github.com/GodotVR/godot_openxr_vendors/releases, unpack its `addons/godotopenxrvendors` folder into `godot/addons/`, and enable it under Project > Project Settings > Plugins. Then open Project > Export, choose the "Quest" preset and check, under XR Features, that XR Mode is OpenXR and "Enable Meta Plugin" is ticked. The `meta_xr_features/…` lines in `godot/export_presets.cfg` were written from memory of the plugin's version 3 options, without the plugin installed; expect the editor to correct them.
6. **Project settings for the device.** The `[xr]` settings above, and Project Settings > Rendering > Textures > VRAM Compression > "Import ETC2 ASTC" on.
7. **Export and install.**
   ```bash
   scripts/godot.sh --headless --path godot --export-debug "Quest" ../local/out/builds/quest/limner.apk
   adb install -r local/out/builds/quest/limner.apk
   adb logcat -s godot            # what the app prints
   ```
8. **On the device.** There are no system file dialogs: documents open and save through the Document panel's File section, in `user://documents`, and exports go to their default folders. `user://` is the app's own storage, so documents saved on the Quest are reached from a PC with `adb` (or the device's file sharing), not by browsing its shared storage.

The other presets in `godot/export_presets.cfg` (Windows, Linux, macOS) need only the export templates and a build of the extension for that system. `scripts/package.sh` (`scripts\package.ps1` on Windows) does all of it: it fetches the templates when they are missing, builds the extension, and exports with the preset for the system it runs on. On the Mac this was built on, both the debug and the release package were made that way and the packaged app was started and looked at. Godot's exporter follows the extension's `res://../target/…` paths out of the project folder and puts the library inside the package, so nothing has to be copied by hand. The Windows and Linux packages have not been made.

The macOS preset makes a universal app, but the script builds the extension for the processor it runs on only. For a universal package, build for `aarch64-apple-darwin` and `x86_64-apple-darwin`, join the two with `lipo -create -output target/release/liblimner_godot.dylib <the two files>`, and export. The package is signed ad hoc and not notarised: on another Mac it has to be allowed by hand the first time. The package name `org.limner.app` is as provisional as the name Limner.

## Never run on a headset

Everything in this layer has been exercised only through the simulated rig, on a Mac, in mono. Tests and screenshots show that the logic holds together; they say nothing about how it feels or whether OpenXR agrees. Specifically, none of this has ever run:

- **All of `openxr_rig.gd`:** starting the session, switching the viewport to XR, V-Sync and refresh rate, the session states, reading actions, haptics.
- **`action_map.tres` against any runtime.** Godot loads it; no runtime has been asked to accept its bindings. A profile a runtime rejects is reported in Godot's log at start-up.
- **The `[xr]` project settings** on Windows, Linux or Android. Only the macOS override was checked.
- **Stereo rendering** of anything: the stroke shaders, `panel.gdshader`, `controller.gdshader`, the tool cursor, `Label3D` text.
- **The mirror window's hint.** It is an ordinary 2D label over the main viewport. Whether Godot shows it only in the window, also in the headset, or not at all when the viewport is in XR is not known.
- **Every distance, angle and strength**, which were chosen at a desk: the tip offset, the poke depths, the palette's place in the hand, panel size, haptic strengths, the button map's distance.
- **Performance** in a headset, on a PC or on a Quest. Each visible panel costs three small viewports when it redraws.
- **The Android export**, the Android build of the Rust extension, and the vendors plugin: none was installed or run.
- **Changing between the desktop and the headset with a real one.** The first try, in 0.2.0 on a Quest 3 over Link, left the desktop's 3D view showing old pictures of the headset's panels. The main viewport kept the headset's size and two eyes, since a window lays its viewport out again only when resized; `OpenXRRig.give_back` now makes its pictures anew and has the window lay it out (`test_the_window_gets_its_own_size_back_from_the_headset` checks the size without a headset). Still to be seen with one: `OpenXRRig.stop()`, the holding view and its trigger, re-entry with the session already focused, the window coming back to its old size, and the session ended from the headset's menu while on the desktop. Two things to watch: while the holding view runs, the headset still paces the frames, so the desktop may run at the headset's rate rather than the monitor's; and whether a `SubViewport` marked for XR takes the headset's frames as the main viewport does.
- **Hand dominance on real controllers.** Left-handed use is mirrored by logic; a left-handed Touch user has the menu button on the drawing hand and none on the other.

### The first session, in order

Start with `-- --xr --no-sim` so that a failure is reported instead of hidden. Keep the log window visible.

1. **Does it start?** The headset shows the scene, the window shows one eye and the hint. If not: the reason is on the desktop's message line and in the log. Look for OpenXR errors about the action map.
2. **Head and floor.** The grid is on the real floor and does not swim when the head moves. If the floor is off, the runtime's play area needs setting up (the reference space is Stage).
3. **Controllers.** Both stand-ins follow the hands. The cone's point is where paint will appear. **Tune `XRRig.TIP_OFFSET`** until the tip sits just clear of the controller's front.
4. **Paint.** Trigger paints with pressure; the hiss sounds; stick up / down sizes the brush; left / right undoes and redoes with their ticks; A deletes, B picks. If a stick does the wrong thing, the binding for that controller is wrong, not the core. Bring the hand back to a stroke: it should blink orange before A deletes it. Is the blink a help or a nuisance while painting close to other strokes? It has a switch in the Tools panel, and its reach is `REACH` in `session/gesture.rs`. Switch on "Brush grid" there too: is a 5 cm cell the right size at arm's length, and are the lines too faint or too busy? **Tune `LimnerWorld.BRUSH_GRID_SEEN` and `BRUSH_GRID_CELLS`.**
5. **Grips.** One grip moves the world, two scale it, a grip and the stick zoom.
6. **Palette.** Y (or the other hand's upper button) shows it. Is it where the other hand can hold it and the drawing hand can reach it, and does it face the eyes? **Tune `XRPalette.COINS_ABOVE_HAND`, `TOWARD_MIDDLE`, `FORWARD`, `TILT_DEGREES`, `TURN_IN_DEGREES`.** Is the text sharp at arm's length? The panels are drawn at twice their own pixels (`XRPanelSurface.TEXELS_PER_PIXEL`); if the headset is slow while a panel is touched, try 1. If the panels are too small or large in the hand, change `PIXELS_PER_METRE`. Do the keys go down far enough before they press (`XRPanelKeys.HEIGHT`, `XRPointer.PRESS_DEPTH`)?
7. **Poke.** Push a tool button. It should press as the tip passes through the surface, once, with a small buzz, and not press again until pulled back. **Tune `XRPointer.HOVER_DISTANCE`, `PRESS_DEPTH`, `RELEASE_DISTANCE`** (too shallow presses by accident, too deep feels dead) and **`XRShell.PRESS_PULSE_STRENGTH` / `PRESS_PULSE_SECONDS`**. Drag the Size slider and the colour square. Watch for presses made by accident when the hand passes through a panel from the front on its way somewhere else.
8. **Ray.** From half a metre away, point at a button: a ray and a dot, trigger presses. Then paint near the palette and near a pinned panel: does the ray steal the trigger when it should not? If it does, shorten `XRPointer.RAY_REACH`.
9. **Pin.** Grip a panel off the palette, resize it with the stick, leave it, put the palette away, send it back with its corner button.
10. **Type.** Poke the layer's name in the Layer panel; the keyboard appears low in front of you; type, Enter. Are the keys big enough to hit, and is it where the drawing hand reaches it without stretching? **Tune `XRPalette.KEYBOARD_AHEAD`, `KEYBOARD_BELOW` and `KEYBOARD_MAGNIFICATION`.**
11. **Pop-ups.** Open a drop-down (frame rate in Document) and choose; open the colour picker.
12. **Button map.** Hold the menu button for a second. Is the sheet readable where it stands? **Tune `XRShell.BUTTON_MAP_DISTANCE` and `BUTTON_MAP_MAGNIFICATION`.**
13. **Attention.** Open the system menu, or take the headset off: nothing should paint, and everything should come back after.
14. **Left hand.** Settings > Drawing hand > Left: the palette moves to the right hand and the left trigger paints.
15. **Frame rate.** Open the Performance panel. Note the frame time with the palette out and put away, and with three panels pinned.
16. **Files.** In the Document panel: Save, type a name on the keyboard, Enter. Paint a stroke, press New and answer the question in the panel; then Open, and pick the project from the list. Quit from the headset's system menu with a stroke unsaved and start again: the stroke should be back, unsaved.
17. **Desktop and back.** Paint a stroke, then Settings > "Continue on the desktop". The window should go back to its old size and show the desktop with the stroke, undoable; the headset should show the dim room with the sheet. Pull a trigger: the headset interface comes back with the hands working at once (not waiting for the headset to be taken off and put on). Change to the desktop again and quit from the headset's system menu: the app should stay on the desktop, say the session ended, and grey out "Continue in the headset". Note the desktop's frame rate while the headset shows the sheet.
