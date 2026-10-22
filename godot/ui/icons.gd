class_name Icons
## Limner's icon set, rendered at whatever scale the interface needs. Two kinds, all
## original to this project:
##
## - filled drawings on a 48 grid (`IconArt.ART`, made from `art/icons/*.svg` by
##   `scripts/icons.py`; see `docs/icons.md`), used wherever one exists;
## - line drawings on a 24 grid (`BODIES` below), for everything the filled set does
##   not cover, such as the timeline's frame, clip and key commands.
##
## A name is looked up in the filled set first, then through `ALIASES` (the older
## names the panels use for a drawing the filled set has under another name), then
## in `BODIES`.

const SIZE := 24

## Older icon names and the filled drawing that stands for each.
const ALIASES := {
	"eraser": "erase",
	"paint_layer": "paint",
	"eye": "visible",
	"eye_off": "hidden",
	"lock": "locked",
	"unlock": "unlocked",
	"trash": "delete",
	"flatten": "layers",
	"stack": "layers",
	"pencil": "rename",
	"bbox": "bounds",
	"prev_frame": "previous",
	"next_frame": "next",
	"gear": "settings",
	"folder_open": "open",
	"new_file": "new",
	"chart": "performance",
	"shape_capped_cylinder": "brush_capped_round",
	"shape_capped_ellipse": "brush_capped_oval",
	"shape_rounded_ribbon": "brush_rounded_ribbon",
	"shape_cylinder": "brush_round",
	"shape_ellipse": "brush_oval",
	"shape_ribbon": "brush_ribbon",
	"shape_cube": "brush_square",
	"tl_frame_blank": "blank",
}
## Names the filled set also has, where the line drawing says something it does not:
## the timeline tells an empty key from a filled one.
const KEEP_LINE := ["key", "key_filled"]

## Icon bodies on a 24 x 24 grid. `{c}` is replaced by the colour.
const BODIES := {
	# tools
	"paint": '<path d="M20 4 11 13"/><path d="M11 13c-3 0-5 1.500-5 4 0 1.500-1 2.500-2.500 3 4.500 1 9-1 9-5.500z"/>',
	"line": '<path d="M5.500 18.500 18.500 5.500"/><circle cx="5" cy="19" r="1.800" fill="{c}"/><circle cx="19" cy="5" r="1.800" fill="{c}"/>',
	"colorize": '<path d="M12 3c3.500 4.500 6 7.500 6 10.500a6 6 0 0 1-12 0c0-3 2.500-6 6-10.500z"/><path d="M9 14a3 3 0 0 0 3 3"/>',
	"eraser": '<path d="M4 15.500 12.500 7l5.500 5.500-6.500 6.500H8z"/><path d="M8 19h12M9.500 10l5.500 5.500"/>',
	"select": '<circle cx="10.500" cy="10.500" r="7" stroke-dasharray="3 3"/><path d="M13 13l7.500 3-3 1.300-1.300 3z" fill="{c}"/>',
	"grab": '<path d="M8 12.500V6.500a1.500 1.500 0 0 1 3 0V11M11 6V5a1.500 1.500 0 0 1 3 0v6M14 6.500a1.500 1.500 0 0 1 3 0V14c0 4-2.500 7-6 7-3 0-4.500-1.500-6-5l-1.300-3.200c-.500-1.500 1.600-2.400 2.500-1L8 14"/>',
	"thicken": '<path d="M4 12h16" stroke-width="5"/><path d="M12 2.500v3M12 18.500v3M10 4.500l2-2 2 2M10 19.500l2 2 2-2"/>',
	"nudge": '<circle cx="8" cy="12" r="4.500"/><path d="M14.500 12H21M18 9l3 3-3 3"/>',
	"optimize": '<path d="M3 17 9 8l6 6 6-8"/><circle cx="3" cy="17" r="1.700" fill="{c}"/><circle cx="9" cy="8" r="1.700" fill="{c}"/><circle cx="15" cy="14" r="1.700" fill="{c}"/><circle cx="21" cy="6" r="1.700" fill="{c}"/>',
	# layer kinds
	"group": '<path d="M3 7a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z"/>',
	"sequence": '<rect x="3" y="5" width="18" height="14" rx="1.500"/><path d="M7 5v14M17 5v14M3 12h4M17 12h4"/>',
	"paint_layer": '<path d="M6 3h9l4 4v14H6z"/><path d="M9 15c1.500-3 3 2 4.500-1s2 0 2.500-.500"/>',
	"picture": '<rect x="3" y="5" width="18" height="14" rx="1.500"/><circle cx="8.500" cy="10" r="1.500"/><path d="M4 17l5-4.500 3.500 3 3-2.500 4.500 4"/>',
	"model": '<path d="M12 3l8 4.500v9L12 21l-8-4.500v-9z"/><path d="M4 7.500l8 4.500 8-4.500M12 12v9"/>',
	"sound": '<path d="M4 10v4h3.500l4.500 4V6L7.500 10z"/><path d="M15.500 9a4 4 0 0 1 0 6M18 6.500a7.500 7.500 0 0 1 0 11"/>',
	"viewpoint": '<path d="M12 21s-6-6-6-10.500a6 6 0 0 1 12 0C18 15 12 21 12 21z"/><circle cx="12" cy="10.500" r="2"/>',
	"camera": '<rect x="3" y="9" width="12" height="9" rx="1.500"/><path d="M15 12.500l6-3v8l-6-3z"/><circle cx="6.500" cy="6" r="1.800"/><circle cx="11.500" cy="6" r="1.800"/>',
	"light": '<path d="M9.500 17.500h5M10.500 20.500h3M8.500 14.500a5.500 5.500 0 1 1 7 0c-.600.600-1 1.300-1 3h-5c0-1.700-.400-2.400-1-3z"/>',
	# actions
	"eye": '<path d="M2.500 12s3.500-6.500 9.500-6.500S21.500 12 21.500 12 18 18.500 12 18.500 2.500 12 2.500 12z"/><circle cx="12" cy="12" r="2.800"/>',
	"eye_off": '<path d="M4 4l16 16"/><path d="M6.500 7.500C4 9.200 2.500 12 2.500 12s3.500 6.500 9.500 6.500c1.600 0 3-.400 4.200-1M10 5.700c.600-.100 1.300-.200 2-.200 6 0 9.500 6.500 9.500 6.500s-.900 1.700-2.600 3.300"/>',
	"lock": '<rect x="5.500" y="11" width="13" height="9" rx="1.500"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>',
	"unlock": '<rect x="5.500" y="11" width="13" height="9" rx="1.500"/><path d="M8 11V8a4 4 0 0 1 7.500-2"/>',
	"plus": '<path d="M12 5v14M5 12h14"/>',
	"minus": '<path d="M5 12h14"/>',
	"trash": '<path d="M4 7h16M9 7V4h6v3M6.500 7l1 13h9l1-13M10 11v6M14 11v6"/>',
	"duplicate": '<rect x="8" y="8" width="12" height="12" rx="1.500"/><path d="M16 8V5.500A1.500 1.500 0 0 0 14.500 4h-9A1.500 1.500 0 0 0 4 5.500v9A1.500 1.500 0 0 0 5.500 16H8"/>',
	"up": '<path d="M12 19V5M6 11l6-6 6 6"/>',
	"down": '<path d="M12 5v14M6 13l6 6 6-6"/>',
	"left": '<path d="M19 12H5M11 6l-6 6 6 6"/>',
	"right": '<path d="M5 12h14M13 6l6 6-6 6"/>',
	"merge": '<path d="M12 3v9M8 8.500l4 4 4-4"/><path d="M4 17h16M4 20.500h16"/>',
	"flatten": '<path d="M3 8.500l9-5 9 5-9 5z"/><path d="M3 13.500l9 5 9-5"/><path d="M5 21.500h14"/>',
	"folder_plus": '<path d="M3 7a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z"/><path d="M12 11v5M9.500 13.500h5"/>',
	"pencil": '<path d="M4 20l1-4.500L16.500 4l3.500 3.500L8.500 19z"/><path d="M14 6.500l3.500 3.500"/>',
	"move": '<path d="M12 3v18M3 12h18M9 6l3-3 3 3M9 18l3 3 3-3M6 9l-3 3 3 3M18 9l3 3-3 3"/>',
	"bbox": '<rect x="4" y="4" width="16" height="16" stroke-dasharray="3 2.500"/>',
	"undo": '<path d="M8 5L4 9l4 4"/><path d="M4 9h10a6 6 0 0 1 0 12h-3"/>',
	"redo": '<path d="M16 5l4 4-4 4"/><path d="M20 9H10a6 6 0 0 0 0 12h3"/>',
	"play": '<path d="M7 4.500v15l12-7.500z" fill="{c}"/>',
	"pause": '<rect x="6" y="5" width="4" height="14" fill="{c}"/><rect x="14" y="5" width="4" height="14" fill="{c}"/>',
	"record": '<circle cx="12" cy="12" r="6" fill="{c}"/>',
	"prev_frame": '<path d="M7 5v14"/><path d="M18 5.500v13L9.500 12z" fill="{c}"/>',
	"next_frame": '<path d="M17 5v14"/><path d="M6 5.500v13L14.500 12z" fill="{c}"/>',
	"rewind": '<path d="M11.500 6v12L3 12zM21 6v12l-8.500-6z" fill="{c}"/>',
	"forward": '<path d="M12.500 6v12L21 12zM3 6v12l8.500-6z" fill="{c}"/>',
	"loop": '<path d="M17 3l3 3-3 3"/><path d="M4 11V9a3 3 0 0 1 3-3h13"/><path d="M7 21l-3-3 3-3"/><path d="M20 13v2a3 3 0 0 1-3 3H4"/>',
	"mute": '<path d="M4 10v4h3.500l4.500 4V6L7.500 10z"/><path d="M16 9.500l5 5M21 9.500l-5 5"/>',
	"onion": '<circle cx="9" cy="12" r="5.500"/><circle cx="15" cy="12" r="5.500" stroke-dasharray="2.500 2.500"/>',
	"snap": '<path d="M6 4v8a6 6 0 0 0 12 0V4h-4v8a2 2 0 0 1-4 0V4z"/><path d="M6 8h4M14 8h4"/>',
	"key": '<path d="M12 4l6 8-6 8-6-8z"/>',
	"key_filled": '<path d="M12 4l6 8-6 8-6-8z" fill="{c}"/>',
	"gear": '<circle cx="12" cy="12" r="3"/><path d="M12 2.500v3M12 18.500v3M2.500 12h3M18.500 12h3M5.300 5.300l2.100 2.100M16.600 16.600l2.100 2.100M5.300 18.700l2.100-2.100M16.600 7.400l2.100-2.100"/>',
	"folder_open": '<path d="M3 18V6a1 1 0 0 1 1-1h5l2 2h8a1 1 0 0 1 1 1v2"/><path d="M3 18l2.500-7.500h16L19 18z"/>',
	"save": '<path d="M5 4h11l4 4v12H5z"/><path d="M8 4v5h7V4M8 20v-6h8v6"/>',
	"new_file": '<path d="M6 3h9l4 4v14H6z"/><path d="M15 3v4h4M12 11v6M9 14h6"/>',
	"export": '<path d="M12 15V4M8 8l4-4 4 4"/><path d="M5 13v6h14v-6"/>',
	"import": '<path d="M12 4v11M8 11l4 4 4-4"/><path d="M5 13v6h14v-6"/>',
	"grid": '<path d="M4 4h16v16H4zM4 9.300h16M4 14.700h16M9.300 4v16M14.700 4v16"/>',
	"gimbal": '<circle cx="12" cy="12" r="8"/><ellipse cx="12" cy="12" rx="8" ry="3"/><ellipse cx="12" cy="12" rx="3" ry="8"/>',
	"flip": '<path d="M12 3v18" stroke-dasharray="2.500 2.500"/><path d="M9 7L4 17h5z"/><path d="M15 7l5 10h-5z" fill="{c}"/>',
	"home": '<path d="M4 11l8-7 8 7"/><path d="M6 10v9h12v-9"/>',
	# where the work is shown: in a headset, or on the desktop's screen
	"headset": '<path d="M5 8h14a2 2 0 0 1 2 2v4.500a2 2 0 0 1-2 2h-3.500l-1.500-2.500h-4L8.500 16.500H5a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2z"/><path d="M7 8V6.500h10V8"/>',
	"desktop": '<rect x="3" y="4.500" width="18" height="12" rx="1.500"/><path d="M12 16.500V20M8 20h8"/>',
	"close": '<path d="M6 6l12 12M18 6L6 18"/>',
	"check": '<path d="M5 12.500l4.500 4.500L19 7.500"/>',
	"menu": '<path d="M4 7h16M4 12h16M4 17h16"/>',
	"frame_all": '<path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5"/><circle cx="12" cy="12" r="2.500"/>',
	"stop_mark": '<rect x="6" y="6" width="12" height="12" fill="{c}"/>',
	"pivot": '<circle cx="12" cy="12" r="2.500"/><path d="M12 3v5M12 16v5M3 12h5M16 12h5"/>',
	"repeat": '<path d="M4 12a8 8 0 0 1 14-5.300M20 12a8 8 0 0 1-14 5.300"/><path d="M18 3v4h-4M6 21v-4h4"/>',
	"sun": '<circle cx="12" cy="12" r="4"/><path d="M12 2.500v3M12 18.500v3M2.500 12h3M18.500 12h3M5.300 5.300l2.100 2.100M16.600 16.600l2.100 2.100M5.300 18.700l2.100-2.100M16.600 7.400l2.100-2.100"/>',
	"spot": '<path d="M9 3h6l1.500 5h-9z"/><path d="M6 20l3-12M18 20L15 8M6 20h12"/>',
	"image": '<rect x="3" y="5" width="18" height="14" rx="1.500"/><circle cx="12" cy="12" r="3.500"/>',
	"film": '<rect x="3" y="6" width="13" height="12" rx="1.500"/><path d="M16 10.500l5-3v9l-5-3z"/>',
	"info": '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.500v.500"/>',
	"chart": '<path d="M4 20V4M4 20h16M8 16v-5M12 16V8M16 16v-3"/>',
	"brush_cylinder": '<ellipse cx="12" cy="12" rx="4.500" ry="8"/>',
	"dot": '<circle cx="12" cy="12" r="3" fill="{c}"/>',
	# brush shapes: a short stroke seen from the side (round, flat or open ends) above its cross-section
	"shape_capped_cylinder": '<rect x="3" y="3" width="18" height="8" rx="4"/><circle cx="12" cy="17.500" r="3.500" fill="{c}"/>',
	"shape_capped_ellipse": '<rect x="3" y="3" width="18" height="8" rx="4"/><ellipse cx="12" cy="17.500" rx="6" ry="2.200" fill="{c}"/>',
	"shape_rounded_ribbon": '<rect x="3" y="3" width="18" height="8" rx="4"/><path d="M5.500 17.500h13"/>',
	"shape_cylinder": '<path d="M3 3h18v8H3z"/><circle cx="12" cy="17.500" r="3.500" fill="{c}"/>',
	"shape_ellipse": '<path d="M3 3h18v8H3z"/><ellipse cx="12" cy="17.500" rx="6" ry="2.200" fill="{c}"/>',
	"shape_ribbon": '<path d="M3 3h18M3 11h18M3 5.700v.010M3 8.300v.010M21 5.700v.010M21 8.300v.010"/><path d="M5.500 17.500h13"/>',
	"shape_cube": '<path d="M3 3h18v8H3z"/><path d="M8.500 14h7v7h-7z" fill="{c}"/>',
	# the hand-held palette in a headset
	"stack": '<path d="M3 8l9-5 9 5-9 5z"/><path d="M3 12l9 5 9-5M3 16l9 5 9-5"/>',
	"timeline": '<path d="M4 7h10M4 12h16M4 17h7"/><path d="M15 4v16"/>',
	"dock": '<rect x="4" y="13" width="16" height="7" rx="1.500"/><path d="M12 3v7M9 7.500l3 3 3-3"/>',
	"pin": '<path d="M9 3.500h6M10 3.500v5l-3 3.500h10l-3-3.500v-5"/><path d="M12 12v8.500"/>',
	"backspace": '<path d="M9 6h11v12H9l-5-6z"/><path d="M12 9.500l5 5M17 9.500l-5 5"/>',
	"shift": '<path d="M12 4l7 8h-4v7H9v-7H5z"/>',
	"enter": '<path d="M19 5v8H6"/><path d="M10 9l-4 4 4 4"/>',
	# the timeline: view, working range and options
	"tl_zoom_in": '<circle cx="10.500" cy="10.500" r="6.500"/><path d="M15.500 15.500l5 5M7.500 10.500h6M10.500 7.500v6"/>',
	"tl_zoom_out": '<circle cx="10.500" cy="10.500" r="6.500"/><path d="M15.500 15.500l5 5M7.500 10.500h6"/>',
	"tl_fit": '<path d="M4 5v14M20 5v14M7.500 12h9M10 9.500L7.500 12l2.500 2.500M14 9.500l2.500 2.500-2.500 2.500"/>',
	"tl_extras": '<circle cx="7.500" cy="7.500" r="3.500"/><path d="M14 4.500h6v6h-6z"/><path d="M12 13.500l5.500 7h-11z"/>',
	"tl_range_start": '<path d="M10 4H6v16h4"/><path d="M16 10v10"/><path d="M13 4h6v3.500l-3 3-3-3z" fill="{c}"/>',
	"tl_range_end": '<path d="M14 4h4v16h-4"/><path d="M8 10v10"/><path d="M5 4h6v3.500l-3 3-3-3z" fill="{c}"/>',
	"tl_range_all": '<path d="M8 4H4v16h4M16 4h4v16h-4"/><path d="M8 12h8M10.500 9.500L8 12l2.500 2.500M13.500 9.500L16 12l-2.500 2.500"/>',
	"tl_range_clip": '<path d="M8 4H4v16h4M16 4h4v16h-4"/><rect x="8" y="9" width="8" height="6" rx="1" fill="{c}"/>',
	"tl_snap_frames": '<path d="M3 19h18M5 19v-5M9.700 19v-5M14.300 19v-5M19 19v-5"/><path d="M14.300 3v7M11.800 7.500l2.500 2.500 2.500-2.500"/>',
	"tl_layer_at_playhead": '<path d="M6 3v18"/><rect x="6" y="13" width="14" height="5" rx="1" fill="{c}"/><path d="M15 4v6M12 7h6"/>',
	"tl_chevron_down": '<path d="M6 9l6 6 6-6"/>',
	"tl_chevron_up": '<path d="M6 15l6-6 6 6"/>',
	"tl_resize": '<path d="M12 3v18M8.500 6.500L12 3l3.500 3.500M8.500 17.500L12 21l3.500-3.500M5 12h14"/>',
	# the timeline: frames of a paint layer
	"tl_key_frame_prev": '<rect x="12" y="7" width="8" height="10" rx="1" fill="{c}"/><path d="M8 8l-4 4 4 4"/>',
	"tl_key_frame_next": '<rect x="4" y="7" width="8" height="10" rx="1" fill="{c}"/><path d="M16 8l4 4-4 4"/>',
	"tl_hold_before": '<rect x="12" y="9" width="8" height="10" rx="1" fill="{c}"/><path d="M12 14H4M4 10.500v7"/><path d="M6.500 2.500v5M4 5h5"/>',
	"tl_hold_after": '<rect x="4" y="9" width="8" height="10" rx="1" fill="{c}"/><path d="M12 14h8M20 10.500v7"/><path d="M17.500 2.500v5M15 5h5"/>',
	"tl_frame_delete": '<rect x="5" y="6" width="14" height="12" rx="1.500"/><path d="M9 9.500l6 5M15 9.500l-6 5"/>',
	"tl_frame_blank": '<rect x="4" y="9" width="11" height="11" rx="1.500"/><path d="M18.500 3v6M15.500 6h6"/>',
	"tl_frame_duplicate": '<rect x="3" y="10" width="7" height="10" rx="1" fill="{c}"/><rect x="13" y="10" width="7" height="10" rx="1" fill="{c}"/><path d="M5 6.500c3-3.500 8-3.500 11 0M16.500 3v3.500H13"/>',
	"tl_key_frame_remove": '<rect x="3" y="7" width="8" height="10" rx="1" fill="{c}"/><rect x="13" y="7" width="8" height="10" rx="1" stroke-dasharray="2 2"/><path d="M11 12h10"/>',
	"tl_paste_frame": '<rect x="6" y="12.500" width="12" height="8.500" rx="1.500"/><path d="M12 2.500v7M9 7l3 3 3-3"/>',
	"tl_paste_frames_all": '<rect x="2.500" y="14" width="5" height="7" rx="1"/><rect x="9.500" y="14" width="5" height="7" rx="1"/><rect x="16.500" y="14" width="5" height="7" rx="1"/><path d="M12 2.500v4M5 11.500v-2.500h14v2.500M12 9v2.500"/>',
	"tl_frames_clear": '<rect x="3" y="8" width="5" height="9" rx="1"/><rect x="9.500" y="8" width="5" height="9" rx="1"/><rect x="16" y="8" width="5" height="9" rx="1"/><path d="M3 20.500L21 4.500"/>',
	"tl_frames_double": '<rect x="3" y="4" width="6" height="6" rx="1" fill="{c}"/><path d="M12.500 7H20M17.500 4.500L20 7l-2.500 2.500"/><rect x="3" y="14" width="6" height="6" rx="1" fill="{c}"/><rect x="12" y="14" width="6" height="6" rx="1" fill="{c}"/>',
	"tl_reverse": '<path d="M4 8h15M16 5l3 3-3 3M20 16H5M8 13l-3 3 3 3"/>',
	# the timeline: clips
	"tl_clip_toggle": '<rect x="3" y="9" width="10" height="6" rx="1" fill="{c}"/><path d="M16 5h3v14h-3"/>',
	"tl_clip_split": '<rect x="3" y="9" width="6.500" height="6" rx="1" fill="{c}"/><rect x="14.500" y="9" width="6.500" height="6" rx="1" fill="{c}"/><path d="M12 4v16" stroke-dasharray="2.500 2.500"/>',
	"tl_clip_open_end": '<rect x="3" y="9" width="8" height="6" rx="1" fill="{c}"/><path d="M13.500 12H21M18 9l3 3-3 3"/>',
	"tl_clip_delete": '<rect x="3" y="13.500" width="18" height="6" rx="1"/><path d="M9 3.500l6 6M15 3.500l-6 6"/>',
	"tl_clip_duplicate": '<rect x="3" y="6" width="11" height="5" rx="1" fill="{c}"/><rect x="10" y="14" width="11" height="5" rx="1" fill="{c}"/><path d="M19 4.500v6M16 7.500h6"/>',
	# the timeline: keys, loops and chapter marks
	"tl_opacity": '<circle cx="12" cy="12" r="8"/><path d="M12 4a8 8 0 0 1 0 16z" fill="{c}"/>',
	"tl_flag": '<path d="M6 21V4"/><path d="M6 4.500h11l-2.500 4 2.500 4H6z" fill="{c}"/>',
	"tl_flag_remove": '<path d="M6 21V4M6 4.500h11l-2.500 4 2.500 4H6"/><path d="M14.500 16l5 5M19.500 16l-5 5"/>',
	"tl_key_prev": '<path d="M16 6l5 6-5 6-5-6z" fill="{c}"/><path d="M7 8l-4 4 4 4"/>',
	"tl_key_next": '<path d="M8 6l5 6-5 6-5-6z" fill="{c}"/><path d="M17 8l4 4-4 4"/>',
	"tl_key_add": '<path d="M9 8l5 6.500L9 21l-5-6.500z" fill="{c}"/><path d="M18 3v6M15 6h6"/>',
	"tl_key_delete": '<path d="M9 8l5 6.500L9 21l-5-6.500z"/><path d="M15.500 3.500l5 5M20.500 3.500l-5 5"/>',
	"tl_key_children": '<path d="M6 3l3 4-3 4-3-4z" fill="{c}"/><path d="M6 11v8.500h6M6 14h6"/><path d="M15 12l2 2-2 2-2-2zM15 17.500l2 2-2 2-2-2z"/>',
	"tl_key_select_all": '<rect x="3" y="5" width="18" height="14" rx="1.500" stroke-dasharray="3 2.500"/><path d="M8.500 9l2.500 3-2.500 3L6 12z" fill="{c}"/><path d="M15.500 9l2.500 3-2.500 3-2.500-3z" fill="{c}"/>',
	"tl_paste": '<rect x="5" y="5" width="14" height="16" rx="1.500"/><path d="M9 5V3h6v2M9 11h6M9 15h6"/>',
	"tl_loop_point": '<path d="M20 4v16"/><path d="M15.500 12A5.500 5.500 0 1 1 10 6.500h3M10.500 4L13 6.500 10.500 9"/>',
	"tl_bake": '<path d="M5 4h14M5 8h14"/><path d="M12 10.500v5M9.500 13l2.500 2.500 2.500-2.500"/><rect x="5" y="17.500" width="14" height="4" rx="1" fill="{c}"/>',
}

static var _cache := {}
## What each texture made here shows: texture -> [name, colour, pixels].
static var _made := {}


## The icon `name` as a texture in `color`, drawn `pixels` wide at normal interface scale.
## Textures are kept and handed out again for the same name, colour and size.
static func texture(name: String, color: Color = Color(0.9, 0.9, 0.9), pixels: int = 20) -> Texture2D:
	var key := "%s/%s/%d" % [name, color.to_html(), pixels]
	if _cache.has(key):
		return _cache[key]
	var svg := ""
	var art := filled(name)
	if art != "":
		svg = art.replace("{px}", str(pixels))
	else:
		var body: String = BODIES.get(name, BODIES["dot"])
		svg = (
			'<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d" viewBox="0 0 24 24" fill="none" stroke="{c}" stroke-width="1.800" stroke-linecap="round" stroke-linejoin="round">%s</svg>'
			% [pixels, pixels, body]
		)
	svg = svg.replace("{c}", "#" + color.to_html(false))
	var icon: Texture2D = DPITexture.create_from_string(svg)
	_cache[key] = icon
	_made[icon] = [name, color, pixels]
	return icon


## `icon` drawn again in the colour scheme in use: an icon made here in a colour of
## one of Kit's roles comes back in that role's colour now (see `Kit.recolor`). Any
## other texture, or none, comes back as it is.
static func retint(icon: Texture2D) -> Texture2D:
	if icon == null or not _made.has(icon):
		return icon
	var made: Array = _made[icon]
	var color := Kit.recolor(made[1])
	if color == made[1]:
		return icon
	return texture(made[0], color, made[2])


## The name and colour of an icon made here, as [name, colour, pixels], or an empty
## list for any other texture.
static func describe(icon: Texture2D) -> Array:
	return _made.get(icon, []) if icon != null else []


static func has(name: String) -> bool:
	return filled(name) != "" or BODIES.has(name)


## The filled drawing that `name` stands for, with `{c}` and `{px}` still in it, or
## an empty string when the name is drawn as a line icon.
static func filled(name: String) -> String:
	if name in KEEP_LINE:
		return ""
	var art_name: String = ALIASES.get(name, name)
	return IconArt.ART.get(art_name, "")
