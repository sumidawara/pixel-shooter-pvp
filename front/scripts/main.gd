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
const DARK := Color("#050609")
const PANEL := Color("#0d1119")
const WHITE := Color("#e9f1f7")
const MUTED := Color("#788594")
const OBSTACLES := [Rect2(250, 85, 140, 28), Rect2(250, 247, 140, 28)]

const PLAYER_STAND: Texture2D = preload("res://assets/art/player_stand.png")
const PLAYER_RUN: Texture2D = preload("res://assets/art/player_run.png")
const CURSOR_TEXTURE: Texture2D = preload("res://assets/art/cursor.png")
const SPARKLE_TEXTURE: Texture2D = preload("res://assets/art/sparkle.png")
const TILEMAP_TEXTURE: Texture2D = preload("res://assets/art/tilemap.png")
const TITLE_TEXTURE: Texture2D = preload("res://assets/art/title.png")
const GAMEOVER_TEXTURE: Texture2D = preload("res://assets/art/gameover.png")
const PIXEL_FONT: Font = preload("res://assets/fonts/PixelMplus12-Regular.ttf")
const PIXEL_FONT_BOLD: Font = preload("res://assets/fonts/PixelMplus12-Bold.ttf")
const SFX_SHOT: AudioStream = preload("res://assets/audio/shot.wav")
const SFX_HIT: AudioStream = preload("res://assets/audio/hit.wav")
const SFX_DASH: AudioStream = preload("res://assets/audio/dash.wav")
const SFX_RELOAD: AudioStream = preload("res://assets/audio/reload.wav")
const SFX_COUNTDOWN: AudioStream = preload("res://assets/audio/countdown.wav")
const SFX_ROUND_START: AudioStream = preload("res://assets/audio/round_start.wav")
const SFX_ROUND_END: AudioStream = preload("res://assets/audio/round_end.wav")

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
var server_move_speed := MOVE_SPEED
var server_dash_speed := DASH_SPEED
var server_dash_duration := DASH_DURATION
var server_dash_cooldown := DASH_COOLDOWN
var status := "READY"
var reconnect_left := 0.0
var has_joined := false
var connection_requested := false
var server_url := DEFAULT_SERVER_URL
var player_name := "Player"
var reconnect_token := ""

var menu_root: Control
var server_input: LineEdit
var name_input: LineEdit
var connect_button: Button
var menu_status: Label
var menu_open := true

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

# スナップショットの差分から作る、クライアントだけの見た目と音の演出。
var particles: Array = []
var sparkles: Array = []
var screen_flash := 0.0
var screen_shake := 0.0
var countdown_second := -1


func _ready() -> void:
	get_window().title = "Pixel Shooter PvP"
	Input.set_custom_mouse_cursor(CURSOR_TEXTURE, Input.CURSOR_ARROW, Vector2(12, 12))
	var environment_url := OS.get_environment("PIXEL_SHOOTER_SERVER_URL")
	if not environment_url.is_empty():
		server_url = environment_url
	_build_menu()
	if OS.get_environment("PIXEL_SHOOTER_AUTOCONNECT") == "1":
		_on_connect_pressed()
	queue_redraw()


func _build_menu() -> void:
	menu_root = Control.new()
	menu_root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	add_child(menu_root)

	var panel := PanelContainer.new()
	panel.position = Vector2(30, 54)
	panel.size = Vector2(272, 278)
	panel.add_theme_stylebox_override("panel", _panel_style())
	menu_root.add_child(panel)

	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 18)
	margin.add_theme_constant_override("margin_right", 18)
	margin.add_theme_constant_override("margin_top", 15)
	margin.add_theme_constant_override("margin_bottom", 15)
	panel.add_child(margin)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 8)
	margin.add_child(column)

	var title := _label("PIXEL SHOOTER", 25, WHITE, true)
	column.add_child(title)
	var subtitle := _label("PVP // AUTHORITATIVE SERVER", 10, CYAN)
	column.add_child(subtitle)
	column.add_child(HSeparator.new())
	column.add_child(_label("PLAYER NAME", 11, MUTED))

	name_input = LineEdit.new()
	name_input.text = "Player-%03d" % (OS.get_process_id() % 1000)
	name_input.max_length = 16
	_style_line_edit(name_input)
	column.add_child(name_input)

	column.add_child(_label("SERVER URL", 11, MUTED))
	server_input = LineEdit.new()
	server_input.text = server_url
	server_input.placeholder_text = DEFAULT_SERVER_URL
	_style_line_edit(server_input)
	server_input.text_submitted.connect(func(_value: String): _on_connect_pressed())
	column.add_child(server_input)

	connect_button = Button.new()
	connect_button.text = "CONNECT"
	connect_button.custom_minimum_size = Vector2(0, 36)
	_style_button(connect_button)
	connect_button.pressed.connect(_on_connect_pressed)
	column.add_child(connect_button)

	menu_status = _label("READY", 11, MUTED)
	menu_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	menu_status.custom_minimum_size = Vector2(0, 28)
	column.add_child(menu_status)
	column.add_child(_label("WASD MOVE  /  LMB FIRE\nR RELOAD  /  SPACE DASH", 10, Color("#aab4bf")))


func _panel_style() -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = Color("#090c12e8")
	style.border_color = Color("#dce5ef")
	style.set_border_width_all(2)
	style.corner_radius_top_left = 2
	style.corner_radius_top_right = 2
	style.corner_radius_bottom_left = 2
	style.corner_radius_bottom_right = 2
	return style


func _label(text: String, size: int, color: Color, bold := false) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_override("font", PIXEL_FONT_BOLD if bold else PIXEL_FONT)
	label.add_theme_font_size_override("font_size", size)
	label.add_theme_color_override("font_color", color)
	return label


func _style_line_edit(line_edit: LineEdit) -> void:
	line_edit.add_theme_font_override("font", PIXEL_FONT)
	line_edit.add_theme_font_size_override("font_size", 12)
	line_edit.add_theme_color_override("font_color", WHITE)
	line_edit.add_theme_color_override("caret_color", CYAN)
	var style := StyleBoxFlat.new()
	style.bg_color = Color("#111722")
	style.border_color = Color("#3b4654")
	style.set_border_width_all(1)
	style.set_content_margin_all(7)
	line_edit.add_theme_stylebox_override("normal", style)
	line_edit.add_theme_stylebox_override("focus", style)


func _style_button(button: Button) -> void:
	button.add_theme_font_override("font", PIXEL_FONT_BOLD)
	button.add_theme_font_size_override("font_size", 14)
	button.add_theme_color_override("font_color", DARK)
	button.add_theme_color_override("font_hover_color", DARK)
	for state_name in ["normal", "hover", "pressed"]:
		var style := StyleBoxFlat.new()
		style.bg_color = CYAN if state_name != "pressed" else MAGENTA
		style.set_content_margin_all(7)
		button.add_theme_stylebox_override(state_name, style)


func _on_connect_pressed() -> void:
	server_url = server_input.text.strip_edges()
	if server_url.is_empty():
		server_url = DEFAULT_SERVER_URL
	if not server_url.begins_with("ws://") and not server_url.begins_with("wss://"):
		server_url = "ws://" + server_url
	server_input.text = server_url
	player_name = name_input.text.strip_edges()
	if player_name.is_empty():
		player_name = "Player"
	connection_requested = true
	connect_button.disabled = true
	_connect_to_server()


func _connect_to_server() -> void:
	socket = WebSocketPeer.new()
	var error := socket.connect_to_url(server_url)
	status = "CONNECTING..."
	menu_status.text = "CONNECTING TO %s" % server_url
	if error != OK:
		status = "SERVER OFFLINE"
		menu_status.text = "CONNECTION COULD NOT START"
		connect_button.disabled = false
	queue_redraw()


func _go_to_menu() -> void:
	connection_requested = false
	if socket.get_ready_state() == WebSocketPeer.STATE_OPEN:
		socket.close()
	socket = WebSocketPeer.new()
	has_joined = false
	player_id = 0
	prediction_ready = false
	pending_inputs.clear()
	players.clear()
	bullets.clear()
	bullet_render_positions.clear()
	bullet_velocities.clear()
	status = "READY"
	menu_status.text = "READY"
	connect_button.disabled = false
	menu_open = true
	menu_root.visible = true
	queue_redraw()


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel") and not menu_open:
		_go_to_menu()


func _process(delta: float) -> void:
	if connection_requested:
		socket.poll()
		var state := socket.get_ready_state()
		if state == WebSocketPeer.STATE_OPEN:
			if not has_joined:
				has_joined = true
				status = "CONNECTED"
				_send({
					"type": "join",
					"name": player_name,
					"reconnect_token": reconnect_token,
				})
			while socket.get_available_packet_count() > 0:
				_receive(socket.get_packet().get_string_from_utf8())
		elif state == WebSocketPeer.STATE_CLOSED:
			status = "SERVER OFFLINE — RETRYING"
			menu_status.text = "SERVER OFFLINE — RETRYING..."
			has_joined = false
			player_id = 0
			prediction_ready = false
			pending_inputs.clear()
			reconnect_left -= delta
			if reconnect_left <= 0.0:
				reconnect_left = 2.0
				_connect_to_server()

	# 他プレイヤーは約100ms前の状態から最新の受信位置へ滑らかに追従させる。
	var interpolation_weight := 1.0 - exp(-INTERPOLATION_SPEED * delta)
	for id in remote_target_positions:
		var current: Vector2 = remote_render_positions.get(id, remote_target_positions[id])
		remote_render_positions[id] = current.lerp(remote_target_positions[id], interpolation_weight)

	# 次のスナップショットが来るまで、サーバーと同じ速度で弾を進める。
	for id in bullet_render_positions:
		bullet_render_positions[id] += bullet_velocities.get(id, Vector2.ZERO) * delta

	prediction_visual_offset = prediction_visual_offset.lerp(
		Vector2.ZERO,
		1.0 - exp(-CORRECTION_DECAY * delta)
	)
	_update_effects(delta)
	queue_redraw()


func _physics_process(delta: float) -> void:
	if menu_open or player_id == 0 or socket.get_ready_state() != WebSocketPeer.STATE_OPEN:
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
			menu_open = false
			menu_root.visible = false
			connect_button.disabled = false
		"rejected":
			status = str(message.get("reason", "Connection rejected"))
			menu_status.text = status
			connection_requested = false
			connect_button.disabled = false
		"snapshot":
			var next_players: Array = message.get("players", [])
			var next_bullets: Array = message.get("bullets", [])
			var next_phase := str(message.get("phase", "waiting"))
			var next_time := float(message.get("time_left", 0.0))
			_capture_snapshot_effects(next_players, next_bullets, next_phase, next_time)
			players = next_players
			bullets = next_bullets
			phase = next_phase
			time_left = next_time
			round_number = int(message.get("round_number", 0))
			rounds_to_win = int(message.get("rounds_to_win", 3))
			round_winner_id = message.get("round_winner_id")
			winner_id = message.get("winner_id")
			reconnect_grace_left = float(message.get("reconnect_grace_left", 0.0))
			server_move_speed = float(message.get("move_speed", MOVE_SPEED))
			server_dash_speed = float(message.get("dash_speed", DASH_SPEED))
			server_dash_duration = float(message.get("dash_duration", DASH_DURATION))
			server_dash_cooldown = float(message.get("dash_cooldown", DASH_COOLDOWN))
			_update_positions_from_snapshot()
			_update_bullets_from_snapshot()


func _capture_snapshot_effects(
	next_players: Array,
	next_bullets: Array,
	next_phase: String,
	next_time: float
) -> void:
	var previous_players: Dictionary = {}
	for player in players:
		previous_players[int(player.get("id", 0))] = player

	for player in next_players:
		var id := int(player.get("id", 0))
		if not previous_players.has(id):
			continue
		var old: Dictionary = previous_players[id]
		var position := _to_vector(player.get("position", {}))
		var color := _player_color_for_list(id, next_players)
		if int(player.get("hp", 0)) < int(old.get("hp", 0)):
			_spawn_burst(position, color, 14, 120.0)
			screen_flash = 0.45
			screen_shake = 5.0
			_play_sfx(SFX_HIT, -4.0)
		if bool(old.get("alive", true)) and not bool(player.get("alive", true)):
			_spawn_burst(position, WHITE, 26, 180.0)
			screen_shake = 8.0
		if not bool(old.get("reloading", false)) and bool(player.get("reloading", false)):
			_play_sfx(SFX_RELOAD, -8.0)
		if not bool(old.get("dashing", false)) and bool(player.get("dashing", false)):
			_spawn_burst(position, color, 8, 80.0)
			_play_sfx(SFX_DASH, -7.0)

	var next_bullet_ids: Dictionary = {}
	for bullet in next_bullets:
		var id := int(bullet.get("id", 0))
		next_bullet_ids[id] = true
		if not bullet_render_positions.has(id):
			var position := _to_vector(bullet.get("position", {}))
			var velocity := _to_vector(bullet.get("velocity", {}))
			var color := _player_color_for_list(int(bullet.get("owner_id", 0)), next_players)
			sparkles.append({
				"position": position - velocity.normalized() * 9.0,
				"life": 0.09,
				"max_life": 0.09,
				"color": color,
			})
			_play_sfx(SFX_SHOT, -10.0)
	for id in bullet_render_positions.keys():
		if not next_bullet_ids.has(id):
			_spawn_burst(bullet_render_positions[id], WHITE, 5, 65.0)

	var next_second := int(ceil(next_time))
	if next_phase == "countdown" and next_second != countdown_second:
		countdown_second = next_second
		_play_sfx(SFX_COUNTDOWN, -7.0)
	if next_phase != phase:
		if next_phase == "running":
			_play_sfx(SFX_ROUND_START, -5.0)
			screen_flash = 0.28
		elif next_phase == "round_end" or next_phase == "match_finished":
			_play_sfx(SFX_ROUND_END, -5.0)
		if next_phase != "countdown":
			countdown_second = -1


func _spawn_burst(position: Vector2, color: Color, count: int, speed: float) -> void:
	for index in range(count):
		var angle := TAU * float(index) / float(count) + randf_range(-0.25, 0.25)
		var life := randf_range(0.18, 0.42)
		particles.append({
			"position": position,
			"velocity": Vector2.from_angle(angle) * randf_range(speed * 0.45, speed),
			"life": life,
			"max_life": life,
			"color": color,
			"size": randi_range(2, 4),
		})


func _update_effects(delta: float) -> void:
	for index in range(particles.size() - 1, -1, -1):
		var particle: Dictionary = particles[index]
		particle.life = float(particle.life) - delta
		if particle.life <= 0.0:
			particles.remove_at(index)
			continue
		particle.position += Vector2(particle.velocity) * delta
		particle.velocity = Vector2(particle.velocity) * exp(-4.0 * delta)
		particles[index] = particle
	for index in range(sparkles.size() - 1, -1, -1):
		sparkles[index].life = float(sparkles[index].life) - delta
		if sparkles[index].life <= 0.0:
			sparkles.remove_at(index)
	screen_flash = maxf(screen_flash - delta * 2.8, 0.0)
	screen_shake = maxf(screen_shake - delta * 28.0, 0.0)


func _play_sfx(stream: AudioStream, volume_db: float) -> void:
	var audio_player := AudioStreamPlayer.new()
	audio_player.stream = stream
	audio_player.volume_db = volume_db
	add_child(audio_player)
	audio_player.finished.connect(func(): audio_player.queue_free())
	audio_player.play()


func _update_positions_from_snapshot() -> void:
	var active_remote_ids: Dictionary = {}
	for player in players:
		var id := int(player.get("id", 0))
		var server_position := _to_vector(player.get("position", {}))
		if id == player_id:
			_reconcile_local_player(player, server_position)
		else:
			active_remote_ids[id] = true
			remote_target_positions[id] = server_position
			if not remote_render_positions.has(id):
				remote_render_positions[id] = server_position
	for id in remote_target_positions.keys():
		if not active_remote_ids.has(id):
			remote_target_positions.erase(id)
			remote_render_positions.erase(id)


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
			bullet_render_positions[id] = (
				server_position if error > 32.0 else current.lerp(server_position, 0.5)
			)
		else:
			bullet_render_positions[id] = server_position
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
		predicted_dash_time = server_dash_duration
		predicted_dash_cooldown = server_dash_cooldown

	var direction := movement
	var speed := server_move_speed
	if predicted_dash_time > 0.0:
		predicted_dash_time = maxf(predicted_dash_time - delta, 0.0)
		direction = predicted_dash_direction
		speed = server_dash_speed
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
	if menu_open:
		draw_texture_rect(TITLE_TEXTURE, Rect2(0, 0, 640, 360), false)
		draw_rect(Rect2(0, 0, 640, 360), Color("#05060955"))
		draw_string(
			PIXEL_FONT_BOLD,
			Vector2(360, 318),
			"MONOCHROME ARENA // CYAN VS MAGENTA",
			HORIZONTAL_ALIGNMENT_CENTER,
			250,
			11,
			WHITE
		)
		draw_rect(Rect2(0, 360, 640, 40), Color("#050609"))
		draw_string(PIXEL_FONT, Vector2(12, 385), "ESC: MENU", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, MUTED)
		return

	var shake_offset := Vector2.ZERO
	if screen_shake > 0.0:
		shake_offset = Vector2(randf_range(-screen_shake, screen_shake), randf_range(-screen_shake, screen_shake)).round()
	draw_set_transform(shake_offset)
	_draw_arena()
	for bullet in bullets:
		var bullet_color := _player_color(int(bullet.get("owner_id", 0)))
		var bullet_id := int(bullet.get("id", 0))
		var position: Vector2 = bullet_render_positions.get(
			bullet_id,
			_to_vector(bullet.get("position", {}))
		)
		var velocity: Vector2 = bullet_velocities.get(bullet_id, Vector2.ZERO)
		draw_line(position - velocity.normalized() * 9.0, position, Color(bullet_color, 0.32), 3.0)
		draw_rect(Rect2(position - Vector2(3, 3), Vector2(6, 6)), bullet_color)

	for player in players:
		_draw_player(player)
	_draw_effects()
	draw_set_transform(Vector2.ZERO)
	_draw_hud()
	if screen_flash > 0.0:
		draw_rect(Rect2(0, 0, 640, 360), Color(WHITE, screen_flash * 0.3))


func _draw_arena() -> void:
	draw_rect(Rect2(Vector2.ZERO, ARENA_SIZE), PANEL)
	for y in range(0, 360, 32):
		for x in range(0, 640, 32):
			if int(x / 32 + y / 32) % 2 == 0:
				draw_rect(Rect2(x, y, 32, 32), Color("#101722"))
	for x in range(0, 641, 32):
		draw_line(Vector2(x, 0), Vector2(x, 360), Color("#1a222d"), 1.0)
	for y in range(0, 361, 32):
		draw_line(Vector2(0, y), Vector2(640, y), Color("#1a222d"), 1.0)
	draw_rect(Rect2(Vector2(1, 1), ARENA_SIZE - Vector2(2, 2)), WHITE, false, 2.0)
	for obstacle in OBSTACLES:
		draw_texture_rect_region(TILEMAP_TEXTURE, obstacle, Rect2(0, 0, 32, 32))
		draw_rect(obstacle.grow(-4), Color("#202834"))


func _draw_effects() -> void:
	for particle in particles:
		var ratio: float = clampf(float(particle.life) / float(particle.max_life), 0.0, 1.0)
		var color: Color = particle.color
		color.a = ratio
		var size := float(particle.size)
		draw_rect(Rect2(Vector2(particle.position) - Vector2.ONE * size * 0.5, Vector2.ONE * size), color)
	for sparkle in sparkles:
		var ratio: float = clampf(float(sparkle.life) / float(sparkle.max_life), 0.0, 1.0)
		var color: Color = sparkle.color
		color.a = ratio
		draw_texture_rect(
			SPARKLE_TEXTURE,
			Rect2(Vector2(sparkle.position) - Vector2(15, 5), Vector2(30, 10)),
			false,
			color
		)


func _draw_player(player: Dictionary) -> void:
	var id := int(player.get("id", 0))
	var position := _render_position(player)
	var color := _player_color(id)
	if not bool(player.get("connected", true)):
		draw_string(PIXEL_FONT, position + Vector2(-38, -21), "DISCONNECTED", HORIZONTAL_ALIGNMENT_CENTER, 76, 10, color)
	var alive := bool(player.get("alive", false))
	if not alive:
		var respawn := float(player.get("respawn_left", 0.0))
		draw_string(PIXEL_FONT_BOLD, position + Vector2(-34, 4), "RESPAWN %.1f" % respawn, HORIZONTAL_ALIGNMENT_CENTER, 68, 10, color)
		return

	var aim := _to_vector(player.get("aim", {}))
	var invulnerable := float(player.get("invulnerable_left", 0.0)) > 0.0
	if invulnerable and Time.get_ticks_msec() % 120 >= 65:
		return
	var dashing := bool(player.get("dashing", false))
	var moving := dashing
	if id == player_id:
		moving = moving or Input.get_vector("move_left", "move_right", "move_up", "move_down").length_squared() > 0.01
	elif remote_target_positions.has(id) and remote_render_positions.has(id):
		moving = moving or Vector2(remote_target_positions[id]).distance_to(remote_render_positions[id]) > 0.8
	var frame := int(Time.get_ticks_msec() / 95) % 4
	var texture := PLAYER_RUN if moving else PLAYER_STAND
	var source := Rect2(frame * 32, 0, 32, 32) if moving else Rect2(0, 0, 32, 32)

	if dashing:
		for trail_index in range(1, 4):
			var trail_position := position - aim * float(trail_index * 8)
			draw_texture_rect_region(
				texture,
				Rect2(trail_position - Vector2(16, 16), Vector2(32, 32)),
				source,
				Color(color, 0.25 / trail_index)
			)
	for offset in [Vector2(-2, 0), Vector2(2, 0), Vector2(0, -2), Vector2(0, 2)]:
		draw_texture_rect_region(
			texture,
			Rect2(position - Vector2(16, 16) + offset, Vector2(32, 32)),
			source,
			color
		)
	draw_texture_rect_region(texture, Rect2(position - Vector2(16, 16), Vector2(32, 32)), source)
	draw_line(position + aim * 4.0, position + aim * 23.0, DARK, 7.0)
	draw_line(position + aim * 5.0, position + aim * 22.0, color, 4.0)
	var name_text := str(player.get("name", "Player"))
	draw_string(PIXEL_FONT_BOLD, position + Vector2(-30, -20), name_text, HORIZONTAL_ALIGNMENT_CENTER, 60, 10, color)


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
		var x := 10.0 if i == 0 else 455.0
		var color := _player_color(int(player.get("id", 0)))
		var hp := int(player.get("hp", 0))
		var score := int(player.get("score", 0))
		var round_wins := int(player.get("round_wins", 0))
		var ammo := int(player.get("ammo", 0))
		var max_ammo := int(player.get("max_ammo", 6))
		var label := "%s  R%d K%d  %d/%d" % [
			str(player.get("name", "P")),
			round_wins,
			score,
			ammo,
			max_ammo
		]
		if bool(player.get("reloading", false)):
			label = "%s RELOAD %.1f" % [label, float(player.get("reload_left", 0.0))]
		draw_string(PIXEL_FONT_BOLD, Vector2(x, 372), label, HORIZONTAL_ALIGNMENT_LEFT, 178, 10, color)
		for heart in range(5):
			var heart_color := color if heart < hp else Color("#28313d")
			draw_rect(Rect2(x + heart * 22.0, 377, 18, 10), heart_color)
			draw_rect(Rect2(x + heart * 22.0 + 3, 379, 12, 2), Color(WHITE, 0.45) if heart < hp else Color.TRANSPARENT)
		var dash_ratio: float = 1.0 - clampf(
			float(player.get("dash_cooldown_left", 0.0)) / server_dash_cooldown,
			0.0,
			1.0
		)
		draw_rect(Rect2(x, 391, 106, 3), Color("#28313d"))
		draw_rect(Rect2(x, 391, 106 * dash_ratio, 3), color)

	var center_text := "%02d" % int(ceil(time_left))
	if phase == "waiting":
		center_text = "WAITING FOR 2 PLAYERS"
	elif phase == "countdown":
		center_text = "ROUND %d  %d" % [round_number, int(ceil(time_left))]
	elif phase == "overtime":
		center_text = "OVERTIME  %02d" % int(ceil(time_left))
	elif phase == "round_end":
		center_text = "ROUND WINNER"
		if round_winner_id != null:
			center_text = "PLAYER %d TAKES ROUND" % int(round_winner_id)
	elif phase == "paused":
		center_text = "RECONNECTING... %.1f" % reconnect_grace_left
	elif phase == "match_finished":
		center_text = "DRAW"
		if winner_id != null:
			center_text = "PLAYER %d WINS MATCH" % int(winner_id)
	draw_string(PIXEL_FONT_BOLD, Vector2(210, 386), center_text, HORIZONTAL_ALIGNMENT_CENTER, 220, 17, WHITE)
	draw_string(PIXEL_FONT, Vector2(7, 18), status, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, MUTED)
	draw_string(PIXEL_FONT, Vector2(438, 18), "ESC MENU", HORIZONTAL_ALIGNMENT_LEFT, -1, 10, MUTED)

	if phase == "countdown":
		draw_string(
			PIXEL_FONT_BOLD,
			Vector2(250, 210),
			str(int(ceil(time_left))),
			HORIZONTAL_ALIGNMENT_CENTER,
			140,
			64,
			WHITE
		)
	elif phase == "match_finished":
		draw_texture_rect(GAMEOVER_TEXTURE, Rect2(80, 45, 480, 270), false, Color(WHITE, 0.42))


func _player_color(id: int) -> Color:
	return _player_color_for_list(id, players)


func _player_color_for_list(id: int, player_list: Array) -> Color:
	var sorted_ids: Array[int] = []
	for player in player_list:
		sorted_ids.append(int(player.get("id", 0)))
	sorted_ids.sort()
	return CYAN if sorted_ids.is_empty() or id == sorted_ids[0] else MAGENTA


func _to_vector(value: Dictionary) -> Vector2:
	return Vector2(float(value.get("x", 0.0)), float(value.get("y", 0.0)))


func _is_playing_phase() -> bool:
	return phase == "running" or phase == "overtime"
