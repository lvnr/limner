class_name Documents
extends RefCounted
## The folder where documents are kept when there is no system file dialog to ask
## where (in a headset), and what is in it.
##
## The Document panel saves, opens and imports here, on the desktop as in a headset.
## The folder is one named Limner in the user's Documents folder, made when it is
## first needed; on Android (a standalone headset) it is in the app's own storage.

## What can be brought into a document as a new layer, by file ending.
const PICTURES := ["png", "jpg", "jpeg"]
const SOUNDS := ["wav", "ogg", "mp3"]
const MODELS := ["obj", "glb", "gltf", "fbx"]

## Set by tests, so that they neither read nor fill the user's own folder.
static var _folder_override := ""


## Where documents are kept. Not made until something is saved there.
static func folder() -> String:
	if _folder_override != "":
		return _folder_override
	var documents := OS.get_system_dir(OS.SYSTEM_DIR_DOCUMENTS)
	if documents == "" or OS.has_feature("android"):
		return ProjectSettings.globalize_path("user://documents")
	return documents.path_join("Limner")


## Uses another folder from now on; an empty string goes back to the usual one.
static func use_folder(path: String) -> void:
	_folder_override = path


## The file a document named `title` is saved as in `in_folder`: a packed `.limnz`,
## named after the title with what a file name cannot hold left out. Empty when
## nothing of the title is left.
static func path_for(title: String, in_folder := folder()) -> String:
	var clean := title.strip_edges()
	if clean.get_extension().to_lower() == "limnz":
		clean = clean.get_basename().strip_edges()
	clean = clean.validate_filename().strip_edges()
	if clean == "" or clean.trim_prefix(".") == "":
		return ""
	return in_folder.path_join(clean + ".limnz")


## The projects in `in_folder`, newest first: `{path, name, time, kind}` with `time`
## in seconds since 1970 and `kind` one of "limnz" (a packed file), "limn" (a project
## folder) and "quill" (a Quill project folder, which opens as a copy to save anew).
static func projects(in_folder := folder()) -> Array[Dictionary]:
	var found: Array[Dictionary] = []
	if not DirAccess.dir_exists_absolute(in_folder):
		return found
	for file: String in DirAccess.get_files_at(in_folder):
		if file.get_extension().to_lower() == "limnz":
			var path := in_folder.path_join(file)
			found.append({"path": path, "name": file.get_basename(), "time": FileAccess.get_modified_time(path), "kind": "limnz"})
	for sub: String in DirAccess.get_directories_at(in_folder):
		var path := in_folder.path_join(sub)
		var kind := ""
		var times: Array[int] = []
		if FileAccess.file_exists(path.path_join("limn.json")):
			kind = "limn"
			for inside: String in ["limn.json", "state.json", "thumbnail.png"]:
				if FileAccess.file_exists(path.path_join(inside)):
					times.append(FileAccess.get_modified_time(path.path_join(inside)))
		elif FileAccess.file_exists(path.path_join("Quill.json")):
			kind = "quill"
			for inside: String in ["Quill.json", "Quill.qbin", "State.json"]:
				if FileAccess.file_exists(path.path_join(inside)):
					times.append(FileAccess.get_modified_time(path.path_join(inside)))
		if kind == "":
			continue
		var name := sub.get_basename() if kind == "limn" else sub
		found.append({"path": path, "name": name, "time": times.max() if not times.is_empty() else 0, "kind": kind})
	found.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return int(a.time) > int(b.time) if int(a.time) != int(b.time) else str(a.name).naturalnocasecmp_to(str(b.name)) < 0
	)
	return found


## What in `in_folder` can be imported as a layer, newest first: pictures, sounds,
## models and other projects, as `{path, name, time, kind}` with `kind` one of
## "picture", "sound", "model" and the kinds `projects` gives.
static func importable(in_folder := folder()) -> Array[Dictionary]:
	var found: Array[Dictionary] = []
	if not DirAccess.dir_exists_absolute(in_folder):
		return found
	for file: String in DirAccess.get_files_at(in_folder):
		var ending := file.get_extension().to_lower()
		var kind := "picture" if PICTURES.has(ending) else ("sound" if SOUNDS.has(ending) else ("model" if MODELS.has(ending) else ""))
		if kind != "":
			var path := in_folder.path_join(file)
			found.append({"path": path, "name": file, "time": FileAccess.get_modified_time(path), "kind": kind})
	found.append_array(projects(in_folder))
	found.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return int(a.time) > int(b.time) if int(a.time) != int(b.time) else str(a.name).naturalnocasecmp_to(str(b.name)) < 0
	)
	return found


## A time in seconds since 1970 as the user's clock shows it: "2026-10-14 15:30".
static func local_time(unix: int) -> String:
	if unix <= 0:
		return ""
	var bias := int(Time.get_time_zone_from_system().get("bias", 0))
	var when := Time.get_datetime_dict_from_unix_time(unix + bias * 60)
	return "%04d-%02d-%02d %02d:%02d" % [when.year, when.month, when.day, when.hour, when.minute]


## A small picture of the artwork as the viewer sees it now, as PNG data, for a
## saved project to show. Empty where nothing can be rendered (a run without a display).
static func thumbnail(editor: LimnerEditor) -> PackedByteArray:
	if not LimnerCapture.can_render():
		return PackedByteArray()
	var capture := LimnerCapture.of(editor)
	var image := capture.still(capture.current_view(), Vector2i(512, 288), "draft")
	if image == null or image.is_empty():
		return PackedByteArray()
	return image.save_png_to_buffer()
