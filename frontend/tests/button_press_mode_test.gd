extends SceneTree


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var main_scene: PackedScene = load("res://scenes/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame

	var buttons: Array[BaseButton] = []
	_collect_buttons(main, buttons)
	if buttons.is_empty():
		push_error("button press mode: no buttons were found")
		quit(1)
		return

	for button in buttons:
		if button.action_mode != BaseButton.ACTION_MODE_BUTTON_PRESS:
			push_error(
				"button press mode: %s still reacts on release" % button.get_path()
			)
			quit(1)
			return

	print("button press mode: all %d buttons react on press" % buttons.size())
	main.queue_free()
	await process_frame
	quit(0)


func _collect_buttons(node: Node, buttons: Array[BaseButton]) -> void:
	if node is BaseButton:
		buttons.append(node)
	for child in node.get_children():
		_collect_buttons(child, buttons)
