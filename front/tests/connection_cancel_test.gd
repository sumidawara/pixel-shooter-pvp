extends SceneTree


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var main_scene: PackedScene = load("res://scenes/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame

	var menu = main.get_node("MenuScreen")
	var network = root.get_node("NetworkClient")
	if not main._is_matchmaker_url("ws://127.0.0.1:8080"):
		push_error("connection cancel: default Matchmaker port was not recognized")
		quit(1)
		return
	if main._as_http_url("ws://127.0.0.1:8080") != "http://127.0.0.1:8080":
		push_error("connection cancel: Matchmaker URL was not normalized to HTTP")
		quit(1)
		return
	if not main._is_matchmaker_url("match.example.test:8080"):
		push_error("connection cancel: scheme-less Matchmaker URL was not recognized")
		quit(1)
		return

	network.connection_requested = true
	menu.set_connecting(true)
	if menu.join_button.text != "CANCEL" or menu.join_button.disabled:
		push_error("connection cancel: cancel button was not enabled")
		quit(1)
		return
	menu.join_button.pressed.emit()
	await process_frame
	if network.connection_requested or menu.is_connecting:
		push_error("connection cancel: pending connection was not stopped")
		quit(1)
		return

	network.connection_requested = true
	menu.set_connecting(true)
	menu._leave_join_page()
	await process_frame
	if network.connection_requested or menu.is_connecting:
		push_error("connection cancel: BACK did not stop the pending connection")
		quit(1)
		return

	main._on_rejected("test rejection")
	if not menu.join_page.visible or menu.join_button.text != "JOIN ROOM":
		push_error("connection cancel: rejection did not return to a retryable Join screen")
		quit(1)
		return

	print("connection cancel: URL normalization, CANCEL, and BACK passed")
	main.queue_free()
	await process_frame
	quit(0)
