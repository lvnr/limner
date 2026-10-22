extends LimnerTest
## Every panel keeps to its size in every state it can be in, on the desktop and in a
## headset, and holds nothing that scrolls or opens a list (see `LimnerPanel.fits`).
## The other panel tests check what the controls do; this one checks only that they
## fit, so that a panel never has to scroll on the palette.

const DOCUMENTS := "user://test_panels_fit_documents"


func _exit_tree() -> void:
	Documents.use_folder("")
	Kit.set_headset(false)


func _mount(panel: LimnerPanel) -> LimnerPanel:
	mount(panel, Vector2(1600, 1400))
	await frames(3)
	return panel


func _both(check_one: Callable) -> void:
	for headset: bool in [false, true]:
		Kit.set_headset(headset)
		await check_one.call("headset" if headset else "desktop")
	Kit.set_headset(false)


func test_transform_performance_and_settings_fit() -> void:
	await _both(func(where: String) -> void:
		for panel: LimnerPanel in [TransformPanel.new(editor), PerformancePanel.new(editor), SettingsPanel.new(editor)]:
			await _mount(panel)
			check_fits(panel, where)
			panel.get_parent().queue_free()
	)


func test_the_button_map_fits_every_page() -> void:
	await _both(func(where: String) -> void:
		var sheet: XRButtonMap = await _mount(XRButtonMap.new())
		for page: String in XRButtonMap.PAGES:
			sheet.show_page(page)
			await frames(1)
			check_fits(sheet, "%s, %s" % [page, where])
		sheet.get_parent().queue_free()
	)


func test_the_layer_panel_fits_every_kind() -> void:
	# One layer of every kind that can be made without a file, and a sound's longest
	# settings: a frustum, with distance falloff.
	var layers := {"paint": _id("Paint")}
	for kind: String in ["group", "sequence", "viewpoint", "camera", "light"]:
		layers[kind] = int(run({"cmd": "add_layer", "kind": kind}).layer)
	await _both(func(where: String) -> void:
		var panel: PropertiesPanel = await _mount(PropertiesPanel.new(editor))
		check_fits(panel, "no layer, %s" % where)
		for kind: String in layers:
			run({"cmd": "select_layers", "layers": [layers[kind]]})
			await frames(2)
			check_fits(panel, "%s, %s" % [kind, where])
		for light_kind: String in ["point", "spot", "directional"]:
			run({"cmd": "set_layer_kind", "layer": layers.light, "values": {"kind": light_kind}})
			run({"cmd": "select_layers", "layers": [layers.light]})
			await frames(2)
			check_fits(panel, "%s light, %s" % [light_kind, where])
		# A sound section is checked as it would be for a sound layer, whatever the layer.
		for section: String in panel._sections:
			for other: String in panel._sections:
				(panel._sections[other] as Control).visible = other == section
			await frames(1)
			check_fits(panel, "every control of %s shown, %s" % [section, where])
		panel.get_parent().queue_free()
	)


func test_the_document_panel_fits_every_file_part() -> void:
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(DOCUMENTS))
	Documents.use_folder(ProjectSettings.globalize_path(DOCUMENTS))
	# More projects than a page shows.
	for number in 14:
		var path := ProjectSettings.globalize_path(DOCUMENTS + "/Project %02d.limnz" % number)
		editor.save_document(path)
	await _both(func(where: String) -> void:
		var panel: DocumentPanel = await _mount(DocumentPanel.new(editor))
		for mode: String in ["", "save_as", "confirm", "open", "import"]:
			panel.show_file_part(mode)
			await frames(2)
			check_fits(panel, "file part \"%s\", %s" % [mode, where])
		panel.get_parent().queue_free()
	)
	for number in 14:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(DOCUMENTS + "/Project %02d.limnz" % number))


func test_the_export_panel_fits_every_section_and_choice() -> void:
	await _both(func(where: String) -> void:
		var panel: ExportPanel = await _mount(ExportPanel.new(editor))
		for index in ExportImageSection.KINDS.size():
			panel.show_section("image")
			panel.image._kind.select(index)
			panel.image.refresh()
			await frames(1)
			check_fits(panel, "picture %s, %s" % [ExportImageSection.KINDS[index][0], where])
		for index in ExportVideoSection.PROJECTIONS.size():
			panel.show_section("video")
			panel.video._projection.select(index)
			panel.video._range.set_value("custom")
			panel.video.refresh()
			await frames(1)
			check_fits(panel, "video %s, %s" % [ExportVideoSection.PROJECTIONS[index][0], where])
		for index in ExportModelSection.FORMATS.size():
			panel.show_section("model")
			panel.model._format.select(index)
			panel.model.refresh()
			await frames(1)
			check_fits(panel, "3D file %s, %s" % [ExportModelSection.FORMATS[index][0], where])
		panel.get_parent().queue_free()
	)


func test_the_monitor_fits_with_and_without_a_camera() -> void:
	await _both(func(where: String) -> void:
		var panel: MonitorPanel = await _mount(MonitorPanel.new(editor))
		check_fits(panel, "no camera, %s" % where)
		panel.get_parent().queue_free()
	)
	run({"cmd": "add_layer", "kind": "camera"})
	await _both(func(where: String) -> void:
		var panel: MonitorPanel = await _mount(MonitorPanel.new(editor))
		check_fits(panel, "a camera, %s" % where)
		panel.get_parent().queue_free()
	)


func _id(layer_name: String) -> int:
	return int(row_named(layer_name).id)
