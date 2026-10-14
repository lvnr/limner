class_name TextStrokes
extends RefCounted
## Turns a line of text into strokes.
##
## Limner paints with strokes, so text is strokes too: the outline of every letter as
## thin tubes and, when the text is to be solid, rows of flat ribbons that fill it.
## Once added they are ordinary strokes that can be moved, recoloured, erased and
## exported like any others.
##
## Shapes come from the font's own outlines, so any font the engine can load and any
## script it can shape will do.

## The size the font is shaped at. Outlines scale, so this only has to be large
## enough for the engine's fixed-point coordinates not to show.
const SHAPING_SIZE := 96
## Straight pieces each curve of an outline is cut into.
const CURVE_STEPS := 5
## Rows of ribbon that fill one letter height.
const ROWS := 40
## Half the thickness of the outline, as a share of the letter height.
const OUTLINE := 0.012
## Where the baseline sits below the middle of the block, as a share of the letter height.
const BASELINE := 0.3

const ON_CURVE := 1
const OFF_CUBIC := 2


## The strokes of `text` set in `font`, `height` metres tall, placed by `frame`:
## the text runs along the frame's X axis, centred on its origin, standing along its
## Y axis and facing along its Z axis. `solid` fills the letters; otherwise only
## their outlines are drawn. Returns stroke descriptions for the `add_strokes`
## command, in the space `frame` is in.
static func build(text: String, font: Font, height: float, frame: Transform3D, solid := true) -> Array[Dictionary]:
	var strokes: Array[Dictionary] = []
	var server := TextServerManager.get_primary_interface()
	if text.strip_edges().is_empty() or font == null or server == null:
		return strokes
	var shaped := server.create_shaped_text()
	server.shaped_text_add_string(shaped, text, font.get_rids(), SHAPING_SIZE)
	server.shaped_text_shape(shaped)
	var glyphs: Array[Dictionary] = server.shaped_text_get_glyphs(shaped)
	var total: float = server.shaped_text_get_size(shaped).x
	server.free_rid(shaped)

	var scale := height / float(SHAPING_SIZE)
	var facing := [frame.basis.z.x, frame.basis.z.y, frame.basis.z.z]
	# From the font's own units, with y running down, to where the letter stands.
	var place := func(point: Vector2) -> Array:
		var local := Vector3((point.x - total * 0.5) * scale, -point.y * scale - height * BASELINE, 0.0)
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
