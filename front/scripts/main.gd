extends Node2D

const DEFAULT_SERVER_URL := "ws://127.0.0.1:9001"
const ARENA_SIZE := Vector2(640.0, 360.0)
const PLAYER_RADIUS := 12.0
const MOVE_SPEED := 150.0
const DASH_SPEED := 520.0
const DASH_DURATION := 0.13
const DASH_COOLDOWN := 1.1
const INTERPOLATION_SPEED := 14.0
const CORRECTION_DECAY := 18.0
const CYAN := Color("#27e5ff")
const MAGENTA := Color("#ff38c7")
const DARK := Color("#080a0f")
const PANEL := Color("#121722")
const OBSTACLES := [Rect2(250, 85, 140, 28), Rect2(250, 247, 140, 28)]

var socket := WebSocketPeer.new()
var player_id := 0
var sequence := 0
var players: Array = []
var bullets: Array = []
var phase := "waiting"
var time_left := 60.0
var winner_id = null
var round_winner_id = null
var round_number := 0
var rounds_to_win := 3
var reconnect_grace_left := 0.0
var status := "CONNECTING..."
var reconnect_left := 0.0
var has_joined := false
var server_url := DEFAULT_SERVER_URL
var reconnect_token := ""

# 自分の入力をサーバーの返答より先に画面へ反映するための状態。
var predicted_position := Vector2.ZERO
var predicted_dash_time := 0.0
var predicted_dash_cooldown := 0.0
var predicted_dash_direction := Vector2.ZERO
var prediction_ready := false
var pending_inputs: Array = []
var prediction_visual_offset := Vector2.ZERO

# 他プレイヤーを最新スナップショットへ滑らかに近づけるための描画位置。
var remote_render_positions: Dictionary = {}
var remote_target_positions: Dictionary = {}

# 弾は20Hzの受信位置をそのまま描かず、速度を使って描画フレームごとに外挿する。
var bullet_render_positions: Dictionary = {}
var bullet_velocities: Dictionary = {}


func _ready() -> void:
	get_window().title = "Pixel Shooter PvP"
	var environment_url := OS.get_environment("PIXEL_SHOOTER_SERVER_URL")
	if not environment_url.is_empty():
		server_url = environment_url
	_connect_to_server()


func _connect_to_server() -> void:
	var error := socket.connect_to_url(server_url)
	status = "CONNECTING..." if error == OK else "SERVER OFFLINE"
	queue_redraw()


func _process(delta: float) -> void:
	socket.poll()
	var state := socket.get_ready_state()
	if state == WebSocketPeer.STATE_OPEN:
		if not has_joined:
			has_joined = true
			status = "CONNECTED"
			_send({
				"type": "join",
				"name": "Player",
				"reconnect_token": reconnect_token,
			})
		while socket.get_available_packet_count() > 0:
			_receive(socket.get_packet().get_string_from_utf8())
	elif state == WebSocketPeer.STATE_CLOSED:
		status = "SERVER OFFLINE — RETRYING"
		has_joined = false
		player_id = 0
		prediction_ready = false
		pending_inputs.clear()
		reconnect_left -= delta
		if reconnect_left <= 0.0:
			reconnect_left = 2.0
			socket = WebSocketPeer.new()
			_connect_to_server()

	# 他プレイヤーは約100ms前の状態から最新の受信位置へ滑らかに追従させる。
	var interpolation_weight := 1.0 - exp(-INTERPOLATION_SPEED * delta)
	for id in remote_target_positions:
		var current: Vector2 = remote_render_positions.get(id, remote_target_positions[id])
		remote_render_positions[id] = current.lerp(remote_target_positions[id], interpolation_weight)

	# 次のスナップショットが来るまで、サーバーと同じ速度で弾を進める。
	for id in bullet_render_positions:
		bullet_render_positions[id] += bullet_velocities.get(id, Vector2.ZERO) * delta

	# サーバー補正による見た目の瞬間移動だけを短時間で消す。
	prediction_visual_offset = prediction_visual_offset.lerp(
		Vector2.ZERO,
		1.0 - exp(-CORRECTION_DECAY * delta)
	)
	queue_redraw()


func _physics_process(delta: float) -> void:
	if player_id == 0 or socket.get_ready_state() != WebSocketPeer.STATE_OPEN:
		return

	sequence += 1
	var movement := Input.get_vector("move_left", "move_right", "move_up", "move_down")
	var origin := predicted_position if prediction_ready else ARENA_SIZE * 0.5
	var aim := (get_local_mouse_position() - origin).normalized()
	if aim == Vector2.ZERO:
		aim = Vector2.RIGHT
	var reload_pressed := Input.is_action_just_pressed("reload")
	var dash_pressed := Input.is_action_just_pressed("dash")

	var input_record := {
		"sequence": sequence,
		"delta": delta,
		"movement": movement,
		"dash_pressed": dash_pressed,
	}
	if _is_playing_phase():
		pending_inputs.append(input_record)
		_simulate_predicted_input(input_record)
	else:
		pending_inputs.clear()

	_send({
		"type": "input",
		"sequence": sequence,
		"move_x": movement.x,
		"move_y": movement.y,
		"aim_x": aim.x,
		"aim_y": aim.y,
		"shooting": Input.is_mouse_button_pressed(MOUSE_BUTTON_LEFT),
		"reload_pressed": reload_pressed,
		"dash_pressed": dash_pressed,
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
			reconnect_token = str(message.get("reconnect_token", ""))
			status = "RECONNECTED" if bool(message.get("reconnected", false)) else "CONNECTED"
		"rejected":
			status = str(message.get("reason", "Connection rejected"))
		"snapshot":
			players = message.get("players", [])
			bullets = message.get("bullets", [])
			phase = message.get("phase", "waiting")
			time_left = float(message.get("time_left", 0.0))
			round_number = int(message.get("round_number", 0))
			rounds_to_win = int(message.get("rounds_to_win", 3))
			round_winner_id = message.get("round_winner_id")
			winner_id = message.get("winner_id")
			reconnect_grace_left = float(message.get("reconnect_grace_left", 0.0))
			_update_positions_from_snapshot()
			_update_bullets_from_snapshot()


func _update_positions_from_snapshot() -> void:
	for player in players:
		var id := int(player.get("id", 0))
		var server_position := _to_vector(player.get("position", {}))
		if id == player_id:
			_reconcile_local_player(player, server_position)
		else:
			remote_target_positions[id] = server_position
			if not remote_render_positions.has(id):
				remote_render_positions[id] = server_position


func _update_bullets_from_snapshot() -> void:
	var active_ids: Dictionary = {}
	for bullet in bullets:
		var id := int(bullet.get("id", 0))
		var server_position := _to_vector(bullet.get("position", {}))
		var velocity := _to_vector(bullet.get("velocity", {}))
		active_ids[id] = true
		bullet_velocities[id] = velocity
		if bullet_render_positions.has(id):
			var current: Vector2 = bullet_render_positions[id]
			var error := current.distance_to(server_position)
			# 小さな誤差は半分だけ補正し、大きくずれた場合だけ確定位置へ戻す。
			bullet_render_positions[id] = (
				server_position if error > 32.0 else current.lerp(server_position, 0.5)
			)
		else:
			bullet_render_positions[id] = server_position

	# 最新スナップショットに存在しない弾は、命中・壁衝突・寿命切れで削除済み。
	for id in bullet_render_positions.keys():
		if not active_ids.has(id):
			bullet_render_positions.erase(id)
			bullet_velocities.erase(id)


# サーバーの確定位置へ戻し、まだサーバーが処理していない入力だけを再適用する。
func _reconcile_local_player(player: Dictionary, server_position: Vector2) -> void:
	var old_visual_position := predicted_position + prediction_visual_offset
	var acknowledged_sequence := int(player.get("last_input_sequence", 0))
	var remaining_inputs: Array = []
	for input_record in pending_inputs:
		if int(input_record.get("sequence", 0)) > acknowledged_sequence:
			remaining_inputs.append(input_record)
	pending_inputs = remaining_inputs

	predicted_position = server_position
	predicted_dash_time = float(player.get("dash_time_left", 0.0))
	predicted_dash_cooldown = float(player.get("dash_cooldown_left", 0.0))
	prediction_ready = true
	for input_record in pending_inputs:
		_simulate_predicted_input(input_record)

	# 計算上は即座に補正しつつ、表示だけは以前の位置から滑らかに移動させる。
	prediction_visual_offset = old_visual_position - predicted_position


func _simulate_predicted_input(input_record: Dictionary) -> void:
	if not prediction_ready:
		return
	var delta := float(input_record.get("delta", 0.0))
	var movement: Vector2 = input_record.get("movement", Vector2.ZERO)
	predicted_dash_cooldown = maxf(predicted_dash_cooldown - delta, 0.0)
	if (
		bool(input_record.get("dash_pressed", false))
		and predicted_dash_cooldown <= 0.0
		and movement.length_squared() > 0.001
	):
		predicted_dash_direction = movement.normalized()
		predicted_dash_time = DASH_DURATION
		predicted_dash_cooldown = DASH_COOLDOWN

	var direction := movement
	var speed := MOVE_SPEED
	if predicted_dash_time > 0.0:
		predicted_dash_time = maxf(predicted_dash_time - delta, 0.0)
		direction = predicted_dash_direction
		speed = DASH_SPEED
	_move_predicted_with_collision(direction * speed * delta)


func _move_predicted_with_collision(delta: Vector2) -> void:
	var next := predicted_position
	next.x += delta.x
	if _valid_player_position(next):
		predicted_position.x = next.x
	next = predicted_position
	next.y += delta.y
	if _valid_player_position(next):
		predicted_position.y = next.y


func _valid_player_position(position: Vector2) -> bool:
	if (
		position.x < PLAYER_RADIUS
		or position.x > ARENA_SIZE.x - PLAYER_RADIUS
		or position.y < PLAYER_RADIUS
		or position.y > ARENA_SIZE.y - PLAYER_RADIUS
	):
		return false
	for obstacle in OBSTACLES:
		if obstacle.grow(PLAYER_RADIUS).has_point(position):
			return false
	return true


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, Vector2(640, 400)), DARK)
	draw_rect(Rect2(Vector2.ZERO, ARENA_SIZE), PANEL)
	draw_rect(Rect2(Vector2(1, 1), ARENA_SIZE - Vector2(2, 2)), Color("#dce5ef"), false, 2.0)
	for obstacle in OBSTACLES:
		draw_rect(obstacle, Color("#dce5ef"))
		draw_rect(obstacle.grow(-4), Color("#252b36"))

	for bullet in bullets:
		var bullet_color := _player_color(int(bullet.get("owner_id", 0)))
		var bullet_id := int(bullet.get("id", 0))
		var position: Vector2 = bullet_render_positions.get(
			bullet_id,
			_to_vector(bullet.get("position", {}))
		)
		draw_circle(position, 4.0, bullet_color)

	for player in players:
		_draw_player(player)

	_draw_hud()


func _draw_player(player: Dictionary) -> void:
	var id := int(player.get("id", 0))
	var position := _render_position(player)
	var color := _player_color(id)
	if not bool(player.get("connected", true)):
		draw_string(
			ThemeDB.fallback_font,
			position + Vector2(-38, -20),
			"DISCONNECTED",
			HORIZONTAL_ALIGNMENT_CENTER,
			76,
			10,
			color
		)
	var alive := bool(player.get("alive", false))
	if not alive:
		var respawn := float(player.get("respawn_left", 0.0))
		draw_string(ThemeDB.fallback_font, position + Vector2(-28, 4), "RESPAWN %.1f" % respawn, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, color)
		return

	var aim := _to_vector(player.get("aim", {}))
	var invulnerable := float(player.get("invulnerable_left", 0.0)) > 0.0
	var visible_now := not invulnerable or Time.get_ticks_msec() % 120 < 65
	if not visible_now:
		return
	if bool(player.get("dashing", false)):
		draw_line(position - aim * 8.0, position - aim * 25.0, Color(color, 0.35), 8.0)
	draw_circle(position, PLAYER_RADIUS + 2.0, Color("#050609"))
	draw_circle(position, PLAYER_RADIUS, Color("#e6edf5"))
	draw_arc(position, PLAYER_RADIUS - 3.0, 0, TAU, 24, color, 3.0)
	draw_line(position, position + aim * 22.0, color, 5.0)
	var name_text := str(player.get("name", "Player"))
	draw_string(ThemeDB.fallback_font, position + Vector2(-24, -19), name_text, HORIZONTAL_ALIGNMENT_CENTER, 48, 10, color)


func _render_position(player: Dictionary) -> Vector2:
	var id := int(player.get("id", 0))
	if id == player_id and prediction_ready:
		return predicted_position + prediction_visual_offset
	return remote_render_positions.get(id, _to_vector(player.get("position", {})))


func _draw_hud() -> void:
	draw_rect(Rect2(0, 360, 640, 40), Color("#05070a"))
	var sorted := players.duplicate()
	sorted.sort_custom(func(a, b): return int(a.get("id", 0)) < int(b.get("id", 0)))
	for i in range(sorted.size()):
		var player: Dictionary = sorted[i]
		var x := 12.0 if i == 0 else 442.0
		var color := _player_color(int(player.get("id", 0)))
		var hp := int(player.get("hp", 0))
		var score := int(player.get("score", 0))
		var round_wins := int(player.get("round_wins", 0))
		var ammo := int(player.get("ammo", 0))
		var max_ammo := int(player.get("max_ammo", 6))
		var reloading := bool(player.get("reloading", false))
		var label := "%s R:%d K:%d %d/%d" % [
			str(player.get("name", "P")),
			round_wins,
			score,
			ammo,
			max_ammo
		]
		if reloading:
			label = "%s  R:%.1f" % [label, float(player.get("reload_left", 0.0))]
		draw_string(ThemeDB.fallback_font, Vector2(x, 374), label, HORIZONTAL_ALIGNMENT_LEFT, 190, 11, color)
		for heart in range(5):
			var heart_color := color if heart < hp else Color("#303642")
			draw_rect(Rect2(x + heart * 19.0, 380, 14, 6), heart_color)
		var dash_ratio: float = 1.0 - clampf(float(player.get("dash_cooldown_left", 0.0)) / DASH_COOLDOWN, 0.0, 1.0)
		draw_rect(Rect2(x, 390, 95, 3), Color("#303642"))
		draw_rect(Rect2(x, 390, 95 * dash_ratio, 3), color)

	var center_text := "%02d" % int(ceil(time_left))
	if phase == "waiting":
		center_text = "WAITING FOR 2 PLAYERS"
	elif phase == "countdown":
		center_text = "ROUND %d  %d" % [round_number, int(ceil(time_left))]
	elif phase == "overtime":
		center_text = "OVERTIME  %02d" % int(ceil(time_left))
	elif phase == "round_end":
		center_text = "PLAYER %d TAKES ROUND" % int(round_winner_id)
	elif phase == "paused":
		center_text = "RECONNECTING... %.1f" % reconnect_grace_left
	elif phase == "match_finished":
		center_text = "DRAW"
		if winner_id != null:
			center_text = "PLAYER %d WINS MATCH" % int(winner_id)
	draw_string(ThemeDB.fallback_font, Vector2(220, 385), center_text, HORIZONTAL_ALIGNMENT_CENTER, 200, 18, Color.WHITE)
	draw_string(ThemeDB.fallback_font, Vector2(8, 18), status, HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color("#758195"))
	draw_string(ThemeDB.fallback_font, Vector2(425, 18), "WASD / LMB / R / SPACE", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color("#758195"))


func _player_color(id: int) -> Color:
	var sorted_ids: Array[int] = []
	for player in players:
		sorted_ids.append(int(player.get("id", 0)))
	sorted_ids.sort()
	return CYAN if sorted_ids.is_empty() or id == sorted_ids[0] else MAGENTA


func _to_vector(value: Dictionary) -> Vector2:
	return Vector2(float(value.get("x", 0.0)), float(value.get("y", 0.0)))


func _is_playing_phase() -> bool:
	return phase == "running" or phase == "overtime"
