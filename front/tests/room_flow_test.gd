extends SceneTree


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var main_scene: PackedScene = load("res://scenes/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame

	var menu = main.get_node("MenuScreen")
	var game = main.get_node("GameScreen")
	var network = root.get_node_or_null("NetworkClient")
	menu.player_name_input.text = "RoomFlowTest"
	menu.port_input.value = 9017
	menu._request_create_room()
	for _attempt in range(20):
		await create_timer(0.2).timeout
		if network != null and network.is_open():
			break

	if network == null or not network.is_open():
		push_error("room flow: local server connection did not open")
		main._leave_room()
		quit(1)
		return

	var guest := WebSocketPeer.new()
	if guest.connect_to_url("ws://127.0.0.1:9017") != OK:
		push_error("room flow: second player connection could not start")
		main._leave_room()
		quit(1)
		return
	var guest_joined := false
	for _attempt in range(40):
		guest.poll()
		if guest.get_ready_state() == WebSocketPeer.STATE_OPEN and not guest_joined:
			guest_joined = true
			guest.send_text(JSON.stringify({
				"type": "join",
				"name": "RoomFlowGuest",
				"reconnect_token": "",
			}))
		await create_timer(0.05).timeout
		if menu.room_waiting_label.text.contains("2/4"):
			break

	if not menu.room_waiting_label.text.contains("2/4"):
		push_error("room flow: second human player did not enter the lobby")
		guest.close()
		main._leave_room()
		quit(1)
		return

	if menu.start_button.disabled:
		push_error("room flow: host could not start with two human players")
		guest.close()
		main._leave_room()
		quit(1)
		return

	menu.start_button.button_down.emit()
	for _attempt in range(20):
		guest.poll()
		await create_timer(0.05).timeout
		if game.visible and game.phase in ["countdown", "running"]:
			break
	if not game.visible or game.phase not in ["countdown", "running"]:
		push_error(
			"room flow: two-human game did not leave the lobby "
			+ "(visible=%s, phase=%s)" % [game.visible, game.phase]
		)
		guest.close()
		main._leave_room()
		quit(1)
		return

	print("room flow: host started a match with a second human player")
	guest.close()
	guest.poll()
	main._leave_room()
	main.queue_free()
	await process_frame
	quit(0)
