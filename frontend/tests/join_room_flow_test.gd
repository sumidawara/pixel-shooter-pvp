extends SceneTree

const SERVER_URL := "ws://127.0.0.1:9019"


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
	menu.server_input.text = SERVER_URL
	menu.player_name_input.text = "JoinFlowHost"
	menu.request_connection()

	for _attempt in range(40):
		await create_timer(0.05).timeout
		if network != null and network.is_open():
			break
	if network == null or not network.is_open():
		push_error("join flow: first client could not join the manual server")
		main._leave_room()
		quit(1)
		return

	var guest := WebSocketPeer.new()
	if guest.connect_to_url(SERVER_URL) != OK:
		push_error("join flow: second client connection could not start")
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
				"name": "JoinFlowGuest",
				"reconnect_token": "",
			}))
		await create_timer(0.05).timeout
		if menu.room_waiting_label.text.contains("2/4"):
			break

	if not menu.is_room_host or menu.start_button.disabled:
		push_error(
			"join flow: first JOIN ROOM client was not an enabled host "
			+ "(host=%s, disabled=%s)" % [menu.is_room_host, menu.start_button.disabled]
		)
		guest.close()
		main._leave_room()
		quit(1)
		return

	menu.start_button.pressed.emit()
	for _attempt in range(40):
		guest.poll()
		await create_timer(0.05).timeout
		if game.visible and game.phase in ["countdown", "running"]:
			break
	if not game.visible or game.phase not in ["countdown", "running"]:
		push_error(
			"join flow: START GAME did not leave the lobby "
			+ "(visible=%s, phase=%s, status=%s)"
			% [game.visible, game.phase, menu.status_label.text]
		)
		guest.close()
		main._leave_room()
		quit(1)
		return

	print("join flow: first JOIN ROOM client started the two-player match")
	guest.close()
	guest.poll()
	main._leave_room()
	main.queue_free()
	await process_frame
	quit(0)
