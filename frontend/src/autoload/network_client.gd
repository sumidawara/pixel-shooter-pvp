extends Node

signal status_changed(text: String)
signal welcome_received(player_id: int, reconnected: bool)
signal map_definition_received(map_definition: Dictionary)
signal rejected(reason: String)
signal snapshot_received(snapshot: Dictionary)

const START_RETRY_SECONDS := 0.35
const START_MAX_ATTEMPTS := 3

var socket := WebSocketPeer.new()
var server_url := NetworkConfig.DEFAULT_GAME_SERVER_URL
var player_name := "Player"
var player_id := 0
var reconnect_token := ""
var join_ticket := ""
var connection_requested := false
var has_joined := false
var reconnect_left := 0.0
var start_request_pending := false
var start_retry_left := 0.0
var start_attempts := 0
var matchmaking_request: HTTPRequest
var connection_generation := 0


func connect_to_server(url: String, requested_name: String) -> void:
	_begin_connection_attempt()
	_prepare_player_name(requested_name)
	join_ticket = ""
	_connect_websocket(url)


func connect_via_matchmaker(url: String, requested_name: String) -> void:
	_begin_connection_attempt()
	var generation := connection_generation
	_prepare_player_name(requested_name)
	var matchmaker_url := url.strip_edges().trim_suffix("/")
	if matchmaker_url.is_empty():
		matchmaker_url = NetworkConfig.DEFAULT_MATCHMAKER_URL
	if not matchmaker_url.begins_with("http://") and not matchmaker_url.begins_with("https://"):
		matchmaker_url = "http://" + matchmaker_url
	status_changed.emit("FINDING ROOM...")
	matchmaking_request = HTTPRequest.new()
	add_child(matchmaking_request)
	var headers := PackedStringArray(["Content-Type: application/json"])
	var error := matchmaking_request.request(
		matchmaker_url + "/v1/matchmake",
		headers,
		HTTPClient.METHOD_POST,
		JSON.stringify({"player_name": player_name})
	)
	if error != OK:
		matchmaking_request.queue_free()
		matchmaking_request = null
		status_changed.emit("MATCHMAKER REQUEST COULD NOT START")
		rejected.emit("Matchmaker request could not start")
		return
	var request := matchmaking_request
	var result: Array = await request.request_completed
	if generation != connection_generation:
		if is_instance_valid(request):
			request.queue_free()
		return
	request.queue_free()
	matchmaking_request = null
	var response_code := int(result[1])
	var body: PackedByteArray = result[3]
	var response = JSON.parse_string(body.get_string_from_utf8())
	if response_code < 200 or response_code >= 300 or typeof(response) != TYPE_DICTIONARY:
		var reason := "No game server is available"
		if typeof(response) == TYPE_DICTIONARY:
			reason = str(response.get("error", reason))
		status_changed.emit(reason)
		rejected.emit(reason)
		return
	join_ticket = str(response.get("join_ticket", ""))
	_connect_websocket(str(response.get("game_url", "")))


func _prepare_player_name(requested_name: String) -> void:
	player_name = requested_name.strip_edges()
	if player_name.is_empty():
		player_name = "Player"


func _connect_websocket(url: String) -> void:
	server_url = url.strip_edges()
	if server_url.is_empty():
		server_url = NetworkConfig.DEFAULT_GAME_SERVER_URL
	if not server_url.begins_with("ws://") and not server_url.begins_with("wss://"):
		server_url = "ws://" + server_url
	connection_requested = true
	reconnect_left = 0.0
	_open_socket()


func disconnect_from_server() -> void:
	connection_generation += 1
	connection_requested = false
	has_joined = false
	player_id = 0
	join_ticket = ""
	start_request_pending = false
	start_attempts = 0
	if is_instance_valid(matchmaking_request):
		matchmaking_request.cancel_request()
		matchmaking_request.queue_free()
	matchmaking_request = null
	if socket.get_ready_state() == WebSocketPeer.STATE_OPEN:
		socket.close()
	socket = WebSocketPeer.new()
	status_changed.emit("READY")


func _begin_connection_attempt() -> void:
	connection_generation += 1
	if socket.get_ready_state() == WebSocketPeer.STATE_OPEN:
		socket.close()
	socket = WebSocketPeer.new()
	if is_instance_valid(matchmaking_request):
		matchmaking_request.cancel_request()
		matchmaking_request.queue_free()
	matchmaking_request = null


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
				"join_ticket": join_ticket,
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
		"map_definition":
			var map_definition = message.get("map", {})
			if typeof(map_definition) != TYPE_DICTIONARY:
				status_changed.emit("INVALID MAP DEFINITION")
				return
			map_definition_received.emit(map_definition)
		"snapshot":
			if start_request_pending and str(message.get("phase", "waiting")) != "waiting":
				start_request_pending = false
				status_changed.emit("MATCH STARTING")
			snapshot_received.emit(message)
