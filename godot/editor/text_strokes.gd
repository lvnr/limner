class_name TextStrokes
extends RefCounted
## Turns text into strokes.
##
## Limner paints with strokes, so text is strokes too: the outline of every letter as
## thin tubes and, when the text is to be solid, rows of flat ribbons that fill it.
## Once added they are ordinary strokes that can be moved, recoloured, erased and
## exported like any others.
##
## Shapes come from the font's own outlines, so any font the engine can load and any
## script it can shape will do: the built-in font or one installed on the system.

## The size the font is shaped at. Outlines scale, so this only has to be large
## enough for the engine's fixed-point coordinates not to show.
const SHAPING_SIZE := 96
## Straight pieces each curve of an outline is cut into.
const CURVE_STEPS := 5
## Rows of ribbon that fill one letter height.
const ROWS := 40
## Half the thickness of the outline, as a share of the letter height.
const OUTLINE := 0.012
## Where the baseline of a single line sits below the middle of the block, as a share
## of the letter height.
const BASELINE := 0.3
## The least and the most distance from one line's baseline to the next, as a share of
## the letter height. In between, the font's own line height is used.
const LINE_SPACING_MIN := 1.2
const LINE_SPACING_MAX := 1.6
## How much a font without a bold face of its own is thickened for Bold, and how far a
## font without an italic face leans for Italic (sideways per unit of height).
const EMBOLDEN := 0.7
const SLANT := 0.2

const ON_CURVE := 1
const OFF_CUBIC := 2

static var _system_fonts := PackedStringArray()


## The strokes of `text` set in `font`, `height` metres tall, placed by `frame`:
## the text runs along the frame's X axis, standing along its Y axis and facing along
## its Z axis. Each line break starts a new line under the last; the block of lines is
## centred on the frame's origin, and `align` lines its rows up on the left, in the
## middle or on the right of the block. `solid` fills the letters; otherwise only
## their outlines are drawn. Returns stroke descriptions for the `add_strokes`
## command, in the space `frame` is in.
static func build(text: String, font: Font, height: float, frame: Transform3D, solid := true, align := HORIZONTAL_ALIGNMENT_CENTER) -> Array[Dictionary]:
	var strokes: Array[Dictionary] = []
	var server := TextServerManager.get_primary_interface()
	if text.strip_edges().is_empty() or font == null or server == null:
		return strokes
	var lines := split_lines(text)
	var glyphs_of_lines: Array[Array] = []
	var widths := PackedFloat32Array()
	for line in lines:
		var glyphs: Array[Dictionary] = []
		var width := 0.0
		if not line.is_empty():
			var shaped := server.create_shaped_text()
			server.shaped_text_add_string(shaped, line, font.get_rids(), SHAPING_SIZE)
			server.shaped_text_shape(shaped)
			glyphs = server.shaped_text_get_glyphs(shaped)
			width = server.shaped_text_get_size(shaped).x
			server.free_rid(shaped)
		glyphs_of_lines.append(glyphs)
		widths.append(width)
	var block := 0.0
	for width in widths:
		block = maxf(block, width)

	var scale := height / float(SHAPING_SIZE)
	var pitch := line_spacing(font) * height
	var count := lines.size()
	for index in count:
		var left := -widths[index] * 0.5
		if align == HORIZONTAL_ALIGNMENT_LEFT:
			left = -block * 0.5
		elif align == HORIZONTAL_ALIGNMENT_RIGHT:
			left = block * 0.5 - widths[index]
		# The middle line, or the gap between the two middle ones, is at the origin.
		var baseline := -height * BASELINE + ((count - 1) * 0.5 - index) * pitch
		var glyphs: Array[Dictionary] = []
		glyphs.assign(glyphs_of_lines[index])
		strokes.append_array(_line_strokes(server, glyphs, left, baseline, scale, height, frame, solid))
	return strokes


## The lines of `text`, without what trails at the end of each (it would move a line
## that is not aligned left) and without empty lines before the first or after the last.
static func split_lines(text: String) -> PackedStringArray:
	var lines := PackedStringArray()
	for line in text.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
		lines.append(line.strip_edges(false, true))
	while not lines.is_empty() and lines[0].is_empty():
		lines.remove_at(0)
	while not lines.is_empty() and lines[lines.size() - 1].is_empty():
		lines.remove_at(lines.size() - 1)
	return lines


## The distance from one baseline to the next, as a share of the letter height.
static func line_spacing(font: Font) -> float:
	return clampf(font.get_height(SHAPING_SIZE) / float(SHAPING_SIZE), LINE_SPACING_MIN, LINE_SPACING_MAX)


# ---- fonts ----

## The families of the fonts installed on the system, sorted. Asked of the system
## once; it takes a moment when there are many.
static func system_fonts() -> PackedStringArray:
	if _system_fonts.is_empty():
		var found := OS.get_system_fonts()
		var unique := {}
		for family in found:
			unique[family] = true
		var sorted: Array = unique.keys()
		sorted.sort_custom(func(a: String, b: String) -> bool: return a.naturalnocasecmp_to(b) < 0)
		_system_fonts = PackedStringArray(sorted)
	return _system_fonts


## Whether a font family is installed. The empty name is the built-in font, always there.
static func is_installed(family: String) -> bool:
	return family.is_empty() or not OS.get_system_font_path(family).is_empty()


## The font of a family, in bold or italic or both; the empty name is the built-in
## font. A family's own bold and italic faces are used where it has them; otherwise
## the letters are thickened or slanted. Returns `null` when the family is not
## installed.
static func font_named(family: String, bold := false, italic := false) -> Font:
	var font: Font = ThemeDB.fallback_font
	if not family.is_empty():
		if not is_installed(family):
			return null
		var system := SystemFont.new()
		system.font_names = PackedStringArray([family])
		system.font_weight = 700 if bold else 400
		system.font_italic = italic
		font = system
	var style := font.get_font_style()
	var thicken := bold and (style & TextServer.FONT_BOLD) == 0
	var slant := italic and (style & TextServer.FONT_ITALIC) == 0
	if not thicken and not slant:
		return font
	var variation := FontVariation.new()
	variation.base_font = font
	if thicken:
		variation.variation_embolden = EMBOLDEN
	if slant:
		# The engine takes this as a shear of x by height: the tops of letters go right.
		variation.variation_transform = Transform2D(Vector2(1.0, SLANT), Vector2(0.0, 1.0), Vector2.ZERO)
	return variation


# ---- shapes ----

## The strokes of one shaped line whose pen starts `left` font units from the middle
## of the block and whose baseline is `baseline` metres above the frame's origin.
static func _line_strokes(server: TextServer, glyphs: Array[Dictionary], left: float, baseline: float, scale: float, height: float, frame: Transform3D, solid: bool) -> Array[Dictionary]:
	var strokes: Array[Dictionary] = []
	var facing := [frame.basis.z.x, frame.basis.z.y, frame.basis.z.z]
	# From the font's own units, with y running down, to where the letter stands.
	var place := func(point: Vector2) -> Array:
		var local := Vector3((left + point.x) * scale, baseline - point.y * scale, 0.0)
		var placed := frame * local
		return [placed.x, placed.y, placed.z]

	var pen := 0.0
	for glyph in glyphs:
		var advance: float = glyph.get("advance", 0.0)
		var offset: Vector2 = glyph.get("offset", Vector2.ZERO)
		var outlines := _outlines(server, glyph.font_rid, int(glyph.index))
		var origin := Vector2(pen + offset.x, offset.y)
		for outline: PackedVector2Array in outlines:
			var points: Array = []
			for point in outline:
				points.append(place.call(origin + point))
			strokes.append({"brush": "cylinder", "points": points, "width": height * OUTLINE})
		if solid:
			var row := float(SHAPING_SIZE) / float(ROWS)
			for span: Array in _fill(outlines, row):
				var from: Vector2 = span[0]
				var to: Vector2 = span[1]
				strokes.append({
					"brush": "ribbon",
					"points": [place.call(origin + from), place.call(origin + to)],
					# A hair over half a row, so that neighbouring rows leave no gap.
					"width": row * scale * 0.52,
					"normal": facing,
				})
		pen += advance * float(glyph.get("repeat", 1))
	return strokes


## The closed outlines of one glyph as runs of straight pieces, in font units.
static func _outlines(server: TextServer, font: RID, index: int) -> Array[PackedVector2Array]:
	var outlines: Array[PackedVector2Array] = []
	var contours := server.font_get_glyph_contours(font, SHAPING_SIZE, index)
	if contours.is_empty():
		return outlines
	var points: PackedVector3Array = contours.points
	var ends: PackedInt32Array = contours.contours
	var start := 0
	for end in ends:
		var contour: Array[Vector3] = []
		for i in range(start, end + 1):
			contour.append(points[i])
		start = end + 1
		var outline := _flatten(contour)
		if outline.size() >= 3:
			outlines.append(outline)
	return outlines


## One contour, its curves cut into straight pieces. The last point repeats the first.
static func _flatten(contour: Array[Vector3]) -> PackedVector2Array:
	var out := PackedVector2Array()
	# Between two neighbouring control points of the simple kind lies a point on the
	# curve that the font leaves unsaid; say it.
	var full: Array[Vector3] = []
	for i in contour.size():
		var a := contour[i]
		var b := contour[(i + 1) % contour.size()]
		full.append(a)
		if int(a.z) != ON_CURVE and int(a.z) != OFF_CUBIC and int(b.z) != ON_CURVE and int(b.z) != OFF_CUBIC:
			full.append(Vector3((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, ON_CURVE))
	var first := -1
	for i in full.size():
		if int(full[i].z) == ON_CURVE:
			first = i
			break
	if first < 0:
		return out
	var count := full.size()
	var at := func(i: int) -> Vector3: return full[(first + i) % count]
	var current := Vector2(at.call(0).x, at.call(0).y)
	out.append(current)
	var i := 1
	while i <= count:
		var point: Vector3 = at.call(i)
		var here := Vector2(point.x, point.y)
		if int(point.z) == ON_CURVE:
			out.append(here)
			current = here
			i += 1
		elif int(point.z) == OFF_CUBIC:
			var second: Vector3 = at.call(i + 1)
			var last: Vector3 = at.call(i + 2)
			var control := Vector2(second.x, second.y)
			var end := Vector2(last.x, last.y)
			for step in range(1, CURVE_STEPS + 1):
				out.append(current.bezier_interpolate(here, control, end, float(step) / CURVE_STEPS))
			current = end
			i += 3
		else:
			var last: Vector3 = at.call(i + 1)
			var end := Vector2(last.x, last.y)
			for step in range(1, CURVE_STEPS + 1):
				var t := float(step) / CURVE_STEPS
				out.append(current.lerp(here, t).lerp(here.lerp(end, t), t))
			current = end
			i += 2
	return out


## The inside of a glyph as level spans, one row of `row` font units after another:
## pairs of points to run a ribbon between. A point is inside when a level line from
## it crosses the outlines an odd number of times.
static func _fill(outlines: Array[PackedVector2Array], row: float) -> Array[Array]:
	var spans: Array[Array] = []
	var top := INF
	var bottom := -INF
	for outline in outlines:
		for point in outline:
			top = minf(top, point.y)
			bottom = maxf(bottom, point.y)
	if top >= bottom:
		return spans
	var y := (floorf(top / row) + 0.5) * row
	while y < bottom:
		var crossings := PackedFloat32Array()
		for outline in outlines:
			for i in outline.size() - 1:
				var a := outline[i]
				var b := outline[i + 1]
				# Each piece owns its upper end and not its lower, so a corner counts once.
				if (a.y <= y) != (b.y <= y):
					crossings.append(a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y))
		crossings.sort()
		for i in range(0, crossings.size() - 1, 2):
			if crossings[i + 1] - crossings[i] > row * 0.05:
				spans.append([Vector2(crossings[i], y), Vector2(crossings[i + 1], y)])
		y += row
	return spans
