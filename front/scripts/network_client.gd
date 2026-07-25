extends Node

signal status_changed(text: String)
signal welcome_received(player_id: int, reconnected: bool)
signal rejected(reason: String)
signal snapshot_received(snapshot: Dictionary)

const DEFAULT_SERVER_URL := "ws://127.0.0.1:9001"
const START_RETRY_SECONDS := 0.35
const START_MAX_ATTEMPTS := 3

var socket := WebSocketPeer.new()
var server_url := DEFAULT_SERVER_URL
var player_name := "Player"
var player_id := 0
var reconnect_token := ""
var connection_requested := false
var has_joined := false
var reconnect_left := 0.0
var start_request_pending := false
var start_retry_left := 0.0
var start_attempts := 0


func connect_to_server(url: String, requested_name: String) -> void:
	server_url = url.strip_edges()
	if server_url.is_empty():
		server_url = DEFAULT_SERVER_URL
	if not server_url.begins_with("ws://") and not server_url.begins_with("wss://"):
		server_url = "ws://" + server_url
	player_name = requested_name.strip_edges()
	if player_name.is_empty():
		player_name = "Player"
	connection_requested = true
	reconnect_left = 0.0
	_open_socket()


func disconnect_from_server() -> void:
	connection_requested = false
	has_joined = false
	player_id = 0
	start_request_pending = false
	start_attempts = 0
	if socket.get_ready_state() == WebSocketPeer.STATE_OPEN:
		socket.close()
	socket = WebSocketPeer.new()
	status_changed.emit("READY")


func send_input(input_message: Dictionary) -> void:
	send_message(input_message)


func send_message(message: Dictionary) -> void:
	if socket.get_ready_state() == WebSocketPeer.STATE_OPEN:
		socket.send_text(JSON.stringify(message))


func add_cpu() -> void:
	send_message({"type": "add_cpu"})


func remove_cpu(id: int) -> void:
	send_message({"type": "remove_cpu", "player_id": id})


func update_room_settings(settings: Dictionary) -> void:
	send_message({"type": "update_room_settings", "settings": settings})


func start_match() -> void:
	if socket.get_ready_state() != WebSocketPeer.STATE_OPEN:
		status_changed.emit("START FAILED — NOT CONNECTED")
		return
	start_request_pending = true
	start_attempts = 0
	_send_start_request()


func is_open() -> bool:
	return socket.get_ready_state() == WebSocketPeer.STATE_OPEN


func _process(delta: float) -> void:
	if not connection_requested:
		return
	socket.poll()
	var state := socket.get_ready_state()
	if state == WebSocketPeer.STATE_OPEN:
		if not has_joined:
			has_joined = true
			status_changed.emit("CONNECTED")
			socket.send_text(JSON.stringify({
				"type": "join",
				"name": player_name,
				"reconnect_token": reconnect_token,
			}))
		while socket.get_available_packet_count() > 0:
			_receive(socket.get_packet().get_string_from_utf8())
		_update_start_request(delta)
	elif state == WebSocketPeer.STATE_CLOSED:
		has_joined = false
		player_id = 0
		status_changed.emit("SERVER OFFLINE — RETRYING")
		reconnect_left -= delta
		if reconnect_left <= 0.0:
			reconnect_left = 2.0
			_open_socket()


func _open_socket() -> void:
	socket = WebSocketPeer.new()
	var error := socket.connect_to_url(server_url)
	if error == OK:
		status_changed.emit("CONNECTING...")
	else:
		status_changed.emit("CONNECTION COULD NOT START")


func _send_start_request() -> void:
	start_attempts += 1
	start_retry_left = START_RETRY_SECONDS
	socket.send_text(JSON.stringify({"type": "start_match"}))
	status_changed.emit("STARTING MATCH...")


func _update_start_request(delta: float) -> void:
	if not start_request_pending:
		return
	start_retry_left -= delta
	if start_retry_left > 0.0:
		return
	if start_attempts >= START_MAX_ATTEMPTS:
		start_request_pending = false
		status_changed.emit("START REQUEST WAS NOT ACCEPTED")
		return
	_send_start_request()


func _receive(text: String) -> void:
	var message = JSON.parse_string(text)
	if typeof(message) != TYPE_DICTIONARY:
		return
	match message.get("type", ""):
		"welcome":
			player_id = int(message.get("player_id", 0))
			reconnect_token = str(message.get("reconnect_token", ""))
			var reconnected := bool(message.get("reconnected", false))
			status_changed.emit("RECONNECTED" if reconnected else "CONNECTED")
			welcome_received.emit(player_id, reconnected)
		"rejected":
			var reason := str(message.get("reason", "Connection rejected"))
			connection_requested = false
			socket.close()
			status_changed.emit(reason)
			rejected.emit(reason)
		"snapshot":
			if start_request_pending and str(message.get("phase", "waiting")) != "waiting":
				start_request_pending = false
				status_changed.emit("MATCH STARTING")
			snapshot_received.emit(message)
