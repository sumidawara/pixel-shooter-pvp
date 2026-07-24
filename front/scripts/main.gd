extends Node2D

const SERVER_URL := "ws://127.0.0.1:9001"
const ARENA_SIZE := Vector2(640.0, 360.0)
const PLAYER_RADIUS := 12.0
const CYAN := Color("#27e5ff")
const MAGENTA := Color("#ff38c7")
const DARK := Color("#080a0f")
const PANEL := Color("#121722")

var socket := WebSocketPeer.new()
var player_id := 0
var sequence := 0
var players: Array = []
var bullets: Array = []
var phase := "waiting"
var time_left := 60.0
var winner_id = null
var status := "CONNECTING..."
var reconnect_left := 0.0
var has_joined := false


func _ready() -> void:
	get_window().title = "Pixel Shooter PvP"
	_connect_to_server()


func _connect_to_server() -> void:
	var error := socket.connect_to_url(SERVER_URL)
	status = "CONNECTING..." if error == OK else "SERVER OFFLINE"
	queue_redraw()


func _process(delta: float) -> void:
	socket.poll()
	var state := socket.get_ready_state()
	if state == WebSocketPeer.STATE_OPEN:
		if not has_joined:
			has_joined = true
			status = "CONNECTED"
			_send({"type": "join", "name": "Player"})
		while socket.get_available_packet_count() > 0:
			_receive(socket.get_packet().get_string_from_utf8())
		_send_input()
	elif state == WebSocketPeer.STATE_CLOSED:
		status = "SERVER OFFLINE — RETRYING"
		has_joined = false
		player_id = 0
		reconnect_left -= delta
		if reconnect_left <= 0.0:
			reconnect_left = 2.0
			socket = WebSocketPeer.new()
			_connect_to_server()
	queue_redraw()


func _send_input() -> void:
	if player_id == 0:
		return
	sequence += 1
	var movement := Input.get_vector("move_left", "move_right", "move_up", "move_down")
	var me := _find_player(player_id)
	var origin := Vector2(ARENA_SIZE.x * 0.5, ARENA_SIZE.y * 0.5)
	if not me.is_empty():
		origin = _to_vector(me.get("position", {}))
	var aim := (get_local_mouse_position() - origin).normalized()
	_send({
		"type": "input",
		"sequence": sequence,
		"move_x": movement.x,
		"move_y": movement.y,
		"aim_x": aim.x,
		"aim_y": aim.y,
		"shooting": Input.is_mouse_button_pressed(MOUSE_BUTTON_LEFT),
	})


func _send(message: Dictionary) -> void:
	if socket.get_ready_state() == WebSocketPeer.STATE_OPEN:
		socket.send_text(JSON.stringify(message))


func _receive(text: String) -> void:
	var message = JSON.parse_string(text)
	if typeof(message) != TYPE_DICTIONARY:
		return
	match message.get("type", ""):
		"welcome":
			player_id = int(message.get("player_id", 0))
		"rejected":
			status = str(message.get("reason", "Connection rejected"))
		"snapshot":
			players = message.get("players", [])
			bullets = message.get("bullets", [])
			phase = message.get("phase", "waiting")
			time_left = float(message.get("time_left", 0.0))
			winner_id = message.get("winner_id")


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, Vector2(640, 400)), DARK)
	draw_rect(Rect2(Vector2.ZERO, ARENA_SIZE), PANEL)
	draw_rect(Rect2(Vector2(1, 1), ARENA_SIZE - Vector2(2, 2)), Color("#dce5ef"), false, 2.0)
	for obstacle in [Rect2(250, 85, 140, 28), Rect2(250, 247, 140, 28)]:
		draw_rect(obstacle, Color("#dce5ef"))
		draw_rect(obstacle.grow(-4), Color("#252b36"))

	for bullet in bullets:
		var bullet_color := _player_color(int(bullet.get("owner_id", 0)))
		draw_circle(_to_vector(bullet.get("position", {})), 4.0, bullet_color)

	for player in players:
		_draw_player(player)

	_draw_hud()


func _draw_player(player: Dictionary) -> void:
	var id := int(player.get("id", 0))
	var position := _to_vector(player.get("position", {}))
	var color := _player_color(id)
	var alive := bool(player.get("alive", false))
	if not alive:
		var respawn := float(player.get("respawn_left", 0.0))
		draw_string(ThemeDB.fallback_font, position + Vector2(-28, 4), "RESPAWN %.1f" % respawn, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, color)
		return
	var aim := _to_vector(player.get("aim", {}))
	draw_circle(position, PLAYER_RADIUS + 2.0, Color("#050609"))
	draw_circle(position, PLAYER_RADIUS, Color("#e6edf5"))
	draw_arc(position, PLAYER_RADIUS - 3.0, 0, TAU, 24, color, 3.0)
	draw_line(position, position + aim * 22.0, color, 5.0)
	var name_text := str(player.get("name", "Player"))
	draw_string(ThemeDB.fallback_font, position + Vector2(-24, -19), name_text, HORIZONTAL_ALIGNMENT_CENTER, 48, 10, color)


func _draw_hud() -> void:
	draw_rect(Rect2(0, 360, 640, 40), Color("#05070a"))
	var sorted := players.duplicate()
	sorted.sort_custom(func(a, b): return int(a.get("id", 0)) < int(b.get("id", 0)))
	for i in range(sorted.size()):
		var player: Dictionary = sorted[i]
		var x := 14.0 if i == 0 else 430.0
		var color := _player_color(int(player.get("id", 0)))
		var hp := int(player.get("hp", 0))
		var score := int(player.get("score", 0))
		draw_string(ThemeDB.fallback_font, Vector2(x, 377), "%s  K:%d" % [str(player.get("name", "P")), score], HORIZONTAL_ALIGNMENT_LEFT, 180, 14, color)
		for heart in range(5):
			var heart_color := color if heart < hp else Color("#303642")
			draw_rect(Rect2(x + heart * 22.0, 383, 17, 7), heart_color)

	var center_text := "%02d" % int(ceil(time_left))
	if phase == "waiting":
		center_text = "WAITING FOR 2 PLAYERS"
	elif phase == "finished":
		center_text = "DRAW"
		if winner_id != null:
			center_text = "PLAYER %d WINS" % int(winner_id)
	draw_string(ThemeDB.fallback_font, Vector2(220, 385), center_text, HORIZONTAL_ALIGNMENT_CENTER, 200, 18, Color.WHITE)
	draw_string(ThemeDB.fallback_font, Vector2(8, 18), status, HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color("#758195"))
	draw_string(ThemeDB.fallback_font, Vector2(490, 18), "WASD + MOUSE", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color("#758195"))


func _find_player(id: int) -> Dictionary:
	for player in players:
		if int(player.get("id", 0)) == id:
			return player
	return {}


func _player_color(id: int) -> Color:
	var sorted_ids: Array[int] = []
	for player in players:
		sorted_ids.append(int(player.get("id", 0)))
	sorted_ids.sort()
	return CYAN if sorted_ids.is_empty() or id == sorted_ids[0] else MAGENTA


func _to_vector(value: Dictionary) -> Vector2:
	return Vector2(float(value.get("x", 0.0)), float(value.get("y", 0.0)))
