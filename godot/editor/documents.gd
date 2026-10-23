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
## The folder last browsed to in the Document panel, for this run; empty for `folder()`.
static var last_folder := ""


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
	last_folder = ""


## The folder the Document panel shows first: the one last browsed to, while it is
## still there, or else `folder()`.
static func browsing() -> String:
	if last_folder != "" and DirAccess.dir_exists_absolute(last_folder):
		return last_folder
	return folder()


## The usual places to start looking, as [path, name] pairs: the documents folder,
## the user's own folders that exist, and on Windows each drive. On a standalone
## headset, only the documents folder.
static func places() -> Array[Array]:
	var found: Array[Array] = [[folder(), "Limner documents"]]
	if OS.has_feature("android") or _folder_override != "":
		return found
	var own := [
		[OS.get_system_dir(OS.SYSTEM_DIR_DOCUMENTS), "Documents"],
		[OS.get_system_dir(OS.SYSTEM_DIR_DESKTOP), "Desktop"],
		[OS.get_system_dir(OS.SYSTEM_DIR_DOWNLOADS), "Downloads"],
		[OS.get_system_dir(OS.SYSTEM_DIR_PICTURES), "Pictures"],
		[OS.get_system_dir(OS.SYSTEM_DIR_MOVIES), "Videos"],
		[OS.get_environment("USERPROFILE") if OS.get_name() == "Windows" else OS.get_environment("HOME"), "Home"],
	]
	for place: Array in own:
		var path: String = place[0]
		if path != "" and DirAccess.dir_exists_absolute(path):
			found.append([path.simplify_path(), place[1]])
	if OS.get_name() == "Windows":
		for index in DirAccess.get_drive_count():
			var drive := DirAccess.get_drive_name(index)
			found.append([drive + "/", "Drive %s" % drive])
	return found


## The folder above `path`, or `path` itself at the top of a drive.
static func parent_of(path: String) -> String:
	var up := path.simplify_path().get_base_dir()
	if up == "" or up == path:
		return path
	# "C:" is the drive's current folder to Windows, not its top.
	if up.ends_with(":"):
		up += "/"
	return up


## The folders in `in_folder` that can be gone into: not hidden, and not projects
## (which `projects` lists), by name, as `{path, name, time, kind}` with `kind`
## "folder".
static func folders(in_folder: String) -> Array[Dictionary]:
	var found: Array[Dictionary] = []
	if not DirAccess.dir_exists_absolute(in_folder):
		return found
	for sub: String in DirAccess.get_directories_at(in_folder):
		if sub.begins_with(".") or sub.begins_with("$"):
			continue
		var path := in_folder.path_join(sub)
		if FileAccess.file_exists(path.path_join("limn.json")) or FileAccess.file_exists(path.path_join("Quill.json")):
			continue
		found.append({"path": path, "name": sub, "time": 0, "kind": "folder"})
	found.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return str(a.name).naturalnocasecmp_to(str(b.name)) < 0)
	return found


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
