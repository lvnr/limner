# The headset palette, closer to Quill

The first session in a real headset (Windows, a Meta PC headset) showed that the palette worked but did not feel right: dense desktop panels on a flat sheet, small targets, a thin strip of tabs. Quill's palette is the opposite: big buttons that stand off the panel and go down when pushed, one panel at a time, a cluster of round buttons to change panels, and small signs riding on the hand. This page says what changes and why. Quill's habits are the target; its exact pixels are not (see the design spec's settled decisions).

## What Quill does (from a recording of Quill 2.9 in a headset, light scheme)

- **One panel at a time** on a light sheet: a grey title bar with the panel's name at the left and a pin at the right; near-white body; black text, large.
- **Raised buttons.** Square icon buttons and text buttons with dark outlines and a visible thickness; the chosen one is filled with cyan. Check boxes are flat outlined squares. Sliders are flat bars.
- **A cluster of round buttons** under the panel, in two staggered rows: the panels used while painting below (Tools, Color, Transform, Monitor, Timeline), the rest above (Documents, Settings, Performance, Export). Thick coins with dark rims; the panel showing is cyan.
- **On the hand that holds the palette:** a grey disc with a thick black ring and the frame number in it; above it a black tag with the selected layer's name. With the thumb on the stick, icons around the ring say what the stick does (up, down, left, right), and they change while the modifier is held (duplicate frame, delete frame, rewind, fast forward).
- The palette rides **above** that hand: the hand at the bottom, the round buttons just above it, the panel above them.

## What Limner does now

The palette is held beside the hand like a clipboard, 25 cm wide, with a strip of small square tabs over the panel. Every panel is the desktop panel drawn into a viewport and shown on a flat quad, at 1.4 pixels per millimetre: a 32-pixel button is 23 mm. Pressing is a poke 3 mm through the flat sheet. The look follows the desktop's colour scheme (dark by default).

## The design

### 1. Raised keys that go down

Every button on a sheet stands off it as a key: a block 5 mm tall whose top shows the button as the panel draws it and whose sides are a darker shade of the button. What counts as a key: `Button`, `OptionButton`, `MenuButton` and `ColorPickerButton`, including each choice of a `ChoiceRow`. Check boxes, sliders, lists and text boxes stay flat, as Quill's are.

- The top of the key is what the tip touches. Pushed, the key goes down with the tip; 3 mm down it clicks (the press, the buzz and the click sound) and it stops 1 mm above the sheet. It comes back up with the tip. Pointing from afar, a pressed key drops to the bottom.
- A button that is on (the chosen tool, a toggled option) sits half down, latched. A disabled button is flush with the sheet.
- Keys under an open pop-up are hidden, so that a drop-down's list is never pierced by the keys below it.
- Built with one `MultiMeshInstance3D` per sheet for square keys and one for round ones; a shader samples the sheet's picture for the tops (with the same three sharpness levels as the sheet) and shades the sides. The sheet works out where its keys are after each layout and whenever it redraws.

### 2. Thick sheets with a title bar and a pin

Each sheet has a body 8 mm thick behind its face. The title bar is a grey band with the name at the left and a **pin key** at the right, always there on palette panels: pressed, the panel stays where it is in the room; on a panel left in the room the same key sends it back. Picking a panel up with the grip still works.

### 3. Bigger, at Quill's scale

The sheets are drawn at 1.1 pixels per millimetre instead of 1.4, so a 32-pixel button is 29 mm across and text is a third larger, and the palette's panels are 384 pixels (35 cm) wide. The tallest a panel gets before it scrolls stays near half a metre.

### 4. The round buttons

The strip of tabs is replaced by round keys, 40 mm across and 10 mm tall, in two staggered rows under the panel. Below: Tools, Color, Layers, Timeline, Transform, Layer. Above: Document, Export, Monitor, Performance, Settings. The panel showing is filled with the accent; a panel left in the room has a small pin mark, and pressing its key brings it back. The name of the button under the tip shows in a tag over the cluster.

### 5. Where the palette rides

Above the hand that holds it, as in Quill: the round buttons centred 7 cm above the hand, the panel above them, leaning back 20 degrees toward the eyes, shifted a little toward the body's middle. The constants stay named in `palette.gd` for tuning in the headset.

### 6. Signs on the hand that holds the palette

A translucent grey disc, 6 cm across, rides on that hand facing the eyes, with a dark ring and the frame number of the selected layer in it (blank when the layer has no frames), and above it a tag with the selected layer's name.

While the thumb rests on the stick (OpenXR's `thumbstick/touch`, where the controller has it) or pushes it, icons around the ring show what the stick does: up a new frame, down hold the frame, left and right the frame before and after, a click play or stop. With the modifier held they become copy frame, delete frame, back to the start and to the end. The direction being pushed lights up. While the piece plays the centre shows a play mark.

### 7. The headset's own look

The headset takes Quill's light look by default, whatever the desktop uses: a preference of its own, `headset_theme`, light unless changed; the Colours row of the Settings panel sets whichever applies to the interface in use. In the headset, buttons get a dark 2-pixel outline, rounded corners and the accent as the fill of a button that is on, with text and icons on it in its own text colour; the panel text is a size larger. Limner keeps its own colours (warm greys, its blue accent) rather than Quill's cyan.

## What stays

The panels are the desktop panels, unchanged: everything here is how the headset presents them. Poking, the far ray, picking panels up, the keyboard, pop-ups and tooltips work as before. The keys change where a press starts (the top of a key rather than the sheet) and add a picture of the key going down.

## Tests

- Panel surface: keys are found for buttons and not for check boxes; a disabled key is flush; keys under an open pop-up are hidden; the key under a pressed pointer is at the bottom; a toggled button's key is latched half down.
- Pointer: pressing a key starts at its top (a tip 2 mm above the sheet over a key presses; over plain sheet it does not).
- Palette: the round buttons choose panels; the pin key pins and sends back; the layout puts the cluster above the hand and the panel above the cluster.
- Hand signs: the frame number and layer name follow the session; the stick icons show while the stick is touched and change with the modifier.
- Screenshots through the simulated headset, light and dark, looked at.

## Not checked on this Mac

How any of it feels, the sizes and distances, and `thumbstick/touch` on each controller: the first session back in the headset decides.
