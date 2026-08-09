extends Node2D

signal exit_requested
signal map_load_failed(reason: String)

const DEFAULT_MOVE_SPEED := 150.0
const DEFAULT_DASH_SPEED := 520.0
const DEFAULT_DASH_DURATION := 0.13
const DEFAULT_DASH_COOLDOWN := 1.1
const INTERPOLATION_SPEED := 14.0
## カメラの拡大率。1.0だとマップ全体が画面へ収まり、追う余地が無い。
##
## 1.5では横427px・縦約210pxが見える。マップは640×352なので、横は約2/3。
## これ以上寄せると、弾（340px/s）を撃った相手が画面の外にいる状況が増える。
## 寄りの好みはここだけ変えれば効く。
const FOLLOW_ZOOM := 1.2
const CORRECTION_DECAY := 18.0
const CYAN := Color("#27e5ff")
const MAGENTA := Color("#ff38c7")
const YELLOW := Color("#ffe66d")
const GREEN := Color("#7cff6b")

const PLAYER_VIEW_SCENE := preload("res://src/actors/player/player_view.tscn")
const BULLET_VIEW_SCENE := preload("res://src/combat/projectiles/bullet_view.tscn")
const ITEM_VIEW_SCENE := preload("res://src/combat/items/item_view.tscn")
const LAROKIN_VIEW_SCENE := preload("res://src/combat/items/larokin_poppos_view.tscn")
const GHOST_THIEF_VIEW_SCENE := preload("res://src/combat/items/ghost_thief_view.tscn")

@onready var world: Node2D = %World
@onready var follow_camera: Camera2D = %FollowCamera
@onready var arena_view = $World/Arena
@onready var item_layer: Node2D = %ItemLayer
@onready var larokin_layer: Node2D = %LarokinLayer
@onready var ghost_thief_layer: Node2D = %GhostThiefLayer
@onready var player_layer: Node2D = %PlayerLayer
@onready var bullet_layer: Node2D = %BulletLayer
@onready var effect_layer: Node2D = %EffectLayer
@onready var hud = %HUD
@onready var shot_player: AudioStreamPlayer = %ShotPlayer
@onready var dry_fire_player: AudioStreamPlayer = %DryFirePlayer
@onready var hit_player: AudioStreamPlayer = %HitPlayer
@onready var dash_player: AudioStreamPlayer = %DashPlayer
@onready var reload_player: AudioStreamPlayer = %ReloadPlayer
@onready var countdown_player: AudioStreamPlayer = %CountdownPlayer
@onready var match_start_player: AudioStreamPlayer = %MatchStartPlayer
@onready var match_end_player: AudioStreamPlayer = %MatchEndPlayer
@onready var exit_confirm_modal = %ExitConfirmModal

var session_active := false
var player_id := 0
var sequence := 0
var players: Array = []
var players_by_id: Dictionary = {}
var player_views: Dictionary = {}
var phase := "waiting"
var time_left := 0.0
var winner_id = null
var reconnect_grace_left := 0.0
var connection_status := "READY"
var countdown_second := -1

var move_speed := DEFAULT_MOVE_SPEED
var dash_speed := DEFAULT_DASH_SPEED
var dash_duration := DEFAULT_DASH_DURATION
var dash_cooldown := DEFAULT_DASH_COOLDOWN

# ローカルプレイヤーの入力予測。規則の実体は MovementPredictor にあり、
# サーバーとの一致は frontend/tests/movement_prediction_golden_test.gd が検証する。
var predictor := MovementPredictor.new()
var pending_inputs: Array = []
var prediction_visual_offset := Vector2.ZERO

# 他プレイヤーの補間位置。
var remote_render_positions: Dictionary = {}
var remote_target_positions: Dictionary = {}

# 弾の外挿位置と表示ノード。
var bullet_views: Dictionary = {}
var bullet_positions: Dictionary = {}
var bullet_velocities: Dictionary = {}

# 得点アイテムは移動しないため、IDと表示ノードだけを同期する。
var item_views: Dictionary = {}
var larokin_views: Dictionary = {}
var ghost_thief_views: Dictionary = {}
var arena_map: ArenaMapData
var map_ready := false


func _ready() -> void:
	NetworkClient.map_definition_received.connect(_on_map_definition_received)
	NetworkClient.snapshot_received.connect(_on_snapshot_received)
	exit_confirm_modal.exit_confirmed.connect(_confirm_exit)


func expect_map() -> void:
	map_ready = false
	predictor.invalidate()
	pending_inputs.clear()


func is_map_ready() -> bool:
	return map_ready and arena_map != null


func start_session(id: int) -> void:
	end_session()
	player_id = id
	session_active = true
	visible = true
	hud.visible = true
	# メニュー画面はCanvasLayerの外のControlなので、カメラを有効にしたままだと
	# メニューまで拡大・移動してしまう。対戦中だけ有効にする。
	follow_camera.enabled = true
	_update_camera()
	set_process(true)
	set_physics_process(true)


func resume_session(id: int) -> void:
	player_id = id
	session_active = true
	hud.visible = true
	follow_camera.enabled = true
	predictor.invalidate()
	pending_inputs.clear()


func end_session() -> void:
	session_active = false
	if is_instance_valid(follow_camera):
		follow_camera.enabled = false
	if is_instance_valid(hud):
		hud.visible = false
	if is_instance_valid(dry_fire_player):
		dry_fire_player.stop()
	if is_instance_valid(exit_confirm_modal):
		exit_confirm_modal.close_modal()
	player_id = 0
	sequence = 0
	players.clear()
	players_by_id.clear()
	phase = "waiting"
	predictor.invalidate()
	pending_inputs.clear()
	prediction_visual_offset = Vector2.ZERO
	remote_render_positions.clear()
	remote_target_positions.clear()
	for view in player_views.values():
		view.queue_free()
	player_views.clear()
	for view in bullet_views.values():
		view.queue_free()
	bullet_views.clear()
	bullet_positions.clear()
	bullet_velocities.clear()
	for view in item_views.values():
		view.queue_free()
	item_views.clear()
	for view in larokin_views.values():
		view.queue_free()
	larokin_views.clear()
	for view in ghost_thief_views.values():
		view.queue_free()
	ghost_thief_views.clear()
	effect_layer.clear()
	world.position = Vector2.ZERO


func set_connection_status(text: String) -> void:
	connection_status = text
	if is_instance_valid(hud):
		hud.set_connection_status(text)


func _unhandled_input(event: InputEvent) -> void:
	if not visible or not session_active:
		return
	var mouse_event := event as InputEventMouseButton
	if (
		mouse_event != null
		and mouse_event.button_index == MOUSE_BUTTON_LEFT
		and mouse_event.pressed
		and _local_player_has_no_ammo()
	):
		dry_fire_player.play()
	if not event.is_action_pressed("ui_cancel"):
		return
	if exit_confirm_modal.is_open():
		exit_confirm_modal.close_modal()
	else:
		exit_confirm_modal.open_modal()
	get_viewport().set_input_as_handled()


func _local_player_has_no_ammo() -> bool:
	if not _is_playing_phase() or exit_confirm_modal.is_open() or not players_by_id.has(player_id):
		return false
	var local_player: Dictionary = players_by_id[player_id]
	return bool(local_player.get("alive", false)) and int(local_player.get("ammo", 0)) == 0


func _process(delta: float) -> void:
	if not session_active:
		return
	var interpolation_weight := 1.0 - exp(-INTERPOLATION_SPEED * delta)
	for id in remote_target_positions:
		var current: Vector2 = remote_render_positions.get(id, remote_target_positions[id])
		remote_render_positions[id] = current.lerp(remote_target_positions[id], interpolation_weight)
	for id in bullet_positions:
		bullet_positions[id] += bullet_velocities.get(id, Vector2.ZERO) * delta
		if bullet_views.has(id):
			bullet_views[id].position = bullet_positions[id]
	prediction_visual_offset = prediction_visual_offset.lerp(
		Vector2.ZERO,
		1.0 - exp(-CORRECTION_DECAY * delta)
	)
	world.position = effect_layer.current_shake_offset()
	_update_player_views()
	_update_camera()


func _physics_process(delta: float) -> void:
	if (
		not session_active
		or not map_ready
		or arena_map == null
		or player_id == 0
		or not NetworkClient.is_open()
	):
		return
	sequence += 1
	var input_blocked: bool = exit_confirm_modal.is_open()
	var movement := (
		Vector2.ZERO
		if input_blocked
		else Input.get_vector("move_left", "move_right", "move_up", "move_down")
	)
	var origin := (
		predictor.position
		if predictor.ready
		else arena_map.size_pixels() * 0.5
	)
	# 画面座標ではなくワールド座標で取る。カメラが動くと両者はずれるため、
	# 画面座標のまま引くと、狙った所と実際に撃つ向きが食い違う。
	var aim := (mouse_world_position() - origin).normalized()
	if aim == Vector2.ZERO:
		aim = Vector2.RIGHT
	var input_record := {
		"sequence": sequence,
		"delta": delta,
		"movement": movement,
		"dash_pressed": not input_blocked and Input.is_action_just_pressed("dash"),
		"use_item_pressed": not input_blocked and Input.is_action_just_pressed("use_item"),
		"held_kind": _local_held_item_kind(),
	}
	if _is_playing_phase() and not input_blocked:
		pending_inputs.append(input_record)
		predictor.simulate(input_record)
	else:
		pending_inputs.clear()
	NetworkClient.send_input({
		"type": "input",
		"sequence": sequence,
		"move_x": movement.x,
		"move_y": movement.y,
		"aim_x": aim.x,
		"aim_y": aim.y,
		"shooting": not input_blocked and Input.is_mouse_button_pressed(MOUSE_BUTTON_LEFT),
		"reload_pressed": not input_blocked and Input.is_action_just_pressed("reload"),
		"dash_pressed": bool(input_record.dash_pressed),
		"use_item_pressed": bool(input_record.use_item_pressed),
	})


func _confirm_exit() -> void:
	exit_requested.emit()


func _on_map_definition_received(definition: Dictionary) -> void:
	var next_map := ArenaMapData.from_dictionary(definition)
	if next_map == null:
		map_ready = false
		set_connection_status("INVALID MAP DEFINITION")
		map_load_failed.emit("Server sent an invalid map definition")
		return
	arena_map = next_map
	map_ready = true
	_apply_camera_limits()
	arena_view.set_arena_map(arena_map)
	predictor.set_map(arena_map)
	pending_inputs.clear()


func _on_snapshot_received(snapshot: Dictionary) -> void:
	if not session_active or not map_ready:
		return
	var next_players: Array = snapshot.get("players", [])
	var next_bullets: Array = snapshot.get("bullets", [])
	var next_items: Array = snapshot.get("items", [])
	var next_larokin: Array = snapshot.get("larokin_poppos", [])
	var next_ghost_thieves: Array = snapshot.get("ghost_thieves", [])
	var next_phase := str(snapshot.get("phase", "waiting"))
	var next_time := float(snapshot.get("time_left", 0.0))
	_capture_snapshot_effects(next_players, next_bullets, next_items, next_phase, next_time)
	players = next_players
	players_by_id.clear()
	for player in players:
		players_by_id[int(player.get("id", 0))] = player
	phase = next_phase
	time_left = next_time
	winner_id = snapshot.get("winner_id")
	reconnect_grace_left = float(snapshot.get("reconnect_grace_left", 0.0))
	move_speed = float(snapshot.get("move_speed", DEFAULT_MOVE_SPEED))
	dash_speed = float(snapshot.get("dash_speed", DEFAULT_DASH_SPEED))
	dash_duration = float(snapshot.get("dash_duration", DEFAULT_DASH_DURATION))
	dash_cooldown = float(snapshot.get("dash_cooldown", DEFAULT_DASH_COOLDOWN))
	predictor.set_gameplay(move_speed, dash_speed, dash_duration, dash_cooldown)
	_sync_players()
	_sync_bullets(next_bullets)
	_sync_items(next_items)
	_sync_larokin(next_larokin)
	_sync_ghost_thieves(next_ghost_thieves)
	hud.apply_snapshot(
		players,
		player_id,
		phase,
		time_left,
		winner_id,
		reconnect_grace_left,
		dash_cooldown,
		players_by_id.get(player_id, {}),
		bool(snapshot.get("room", {}).get("settings", {}).get("sandbox", false))
	)


func _sync_items(next_items: Array) -> void:
	var active_ids: Dictionary = {}
	for item in next_items:
		var id := int(item.get("id", 0))
		active_ids[id] = true
		if not item_views.has(id):
			var view = ITEM_VIEW_SCENE.instantiate()
			item_layer.add_child(view)
			view.configure(str(item.get("kind", "energy_cell")), int(item.get("points", 0)))
			item_views[id] = view
		item_views[id].position = _to_vector(item.get("position", {}))
	for id in item_views.keys():
		if not active_ids.has(id):
			item_views[id].queue_free()
			item_views.erase(id)


func _sync_larokin(next_attackers: Array) -> void:
	var active_ids: Dictionary = {}
	for attacker in next_attackers:
		var id := int(attacker.get("id", 0))
		active_ids[id] = true
		if not larokin_views.has(id):
			var view = LAROKIN_VIEW_SCENE.instantiate()
			larokin_layer.add_child(view)
			larokin_views[id] = view
		larokin_views[id].apply_state(attacker)
		larokin_views[id].position = _to_vector(attacker.get("position", {}))
	for id in larokin_views.keys():
		if not active_ids.has(id):
			larokin_views[id].queue_free()
			larokin_views.erase(id)


## Ghostの奪取演出を同期する。
##
## 位置も進み具合もサーバーが決めた値をそのまま使う。所持アイテムの移動という
## 状態差分から「誰が奪ったか」を推測すると、同じtickに複数の変化が起きたときに
## 取り違える。
func _sync_ghost_thieves(next_thieves: Array) -> void:
	var active_ids: Dictionary = {}
	for thief in next_thieves:
		var id := int(thief.get("id", 0))
		active_ids[id] = true
		if not ghost_thief_views.has(id):
			var view = GHOST_THIEF_VIEW_SCENE.instantiate()
			ghost_thief_layer.add_child(view)
			ghost_thief_views[id] = view
		ghost_thief_views[id].apply_state(thief, _player_color(int(thief.get("owner_id", 0))))
	for id in ghost_thief_views.keys():
		if not active_ids.has(id):
			ghost_thief_views[id].queue_free()
			ghost_thief_views.erase(id)


func _sync_players() -> void:
	var active_ids: Dictionary = {}
	for player in players:
		var id := int(player.get("id", 0))
		active_ids[id] = true
		var server_position := _to_vector(player.get("position", {}))
		if not player_views.has(id):
			var view = PLAYER_VIEW_SCENE.instantiate()
			player_layer.add_child(view)
			player_views[id] = view
		if id == player_id:
			_reconcile_local_player(player, server_position)
		else:
			remote_target_positions[id] = server_position
			if not remote_render_positions.has(id):
				remote_render_positions[id] = server_position
	for id in player_views.keys():
		if not active_ids.has(id):
			player_views[id].queue_free()
			player_views.erase(id)
			remote_target_positions.erase(id)
			remote_render_positions.erase(id)
	_update_player_views()


func _sync_bullets(next_bullets: Array) -> void:
	var active_ids: Dictionary = {}
	for bullet in next_bullets:
		var id := int(bullet.get("id", 0))
		var server_position := _to_vector(bullet.get("position", {}))
		var velocity := _to_vector(bullet.get("velocity", {}))
		active_ids[id] = true
		bullet_velocities[id] = velocity
		if bullet_positions.has(id):
			var current: Vector2 = bullet_positions[id]
			var error := current.distance_to(server_position)
			bullet_positions[id] = (
				server_position if error > 32.0 else current.lerp(server_position, 0.5)
			)
		else:
			bullet_positions[id] = server_position
		if not bullet_views.has(id):
			var view = BULLET_VIEW_SCENE.instantiate()
			bullet_layer.add_child(view)
			bullet_views[id] = view
		bullet_views[id].configure(_player_color(int(bullet.get("owner_id", 0))), velocity)
		bullet_views[id].position = bullet_positions[id]
	for id in bullet_views.keys():
		if not active_ids.has(id):
			bullet_views[id].queue_free()
			bullet_views.erase(id)
			bullet_positions.erase(id)
			bullet_velocities.erase(id)


## 画面に描いているローカルプレイヤーの位置。
##
## カメラと自機の表示で別々に計算すると、補正が片方にだけ効いて自機が中心から
## ずれる。同じ値を使う。
func _local_player_render_position() -> Vector2:
	if predictor.ready:
		return predictor.position + prediction_visual_offset
	if players_by_id.has(player_id):
		return _to_vector(players_by_id[player_id].get("position", {}))
	if arena_map != null:
		return arena_map.size_pixels() * 0.5
	return Vector2.ZERO


## マウスのワールド座標。
##
## カメラが動くと画面座標とワールド座標はずれる。画面の中心にいる自機から
## 画面座標のマウスへ向きを取ると、狙った所と実際に撃つ向きが食い違う。
func mouse_world_position() -> Vector2:
	return get_global_mouse_position()


## 自機を画面の中心へ置く。
func _update_camera() -> void:
	if not follow_camera.enabled:
		return
	follow_camera.position = _local_player_render_position()


## カメラが寄れる範囲をマップの大きさから決める。
##
## マップの端をHUDの帯の下へ潜らせないよう、帯の高さぶんだけ外側へ広げる。
## 広げないと、マップの上端にいるとき一番上の行が帯に隠れる。
func _apply_camera_limits() -> void:
	if arena_map == null:
		return
	follow_camera.zoom = Vector2(FOLLOW_ZOOM, FOLLOW_ZOOM)
	var map_size := arena_map.size_pixels()
	var screen := get_viewport_rect().size
	var view := screen / FOLLOW_ZOOM
	var top_inset: float = hud.WORLD_VIEW_TOP / FOLLOW_ZOOM
	var bottom_inset: float = (screen.y - hud.WORLD_VIEW_BOTTOM) / FOLLOW_ZOOM
	follow_camera.limit_left = 0
	follow_camera.limit_top = int(-top_inset)
	# マップが表示範囲より小さいと上限と下限が逆転する。最低でも1画面分は取る。
	follow_camera.limit_right = int(maxf(map_size.x, view.x))
	follow_camera.limit_bottom = int(maxf(map_size.y + bottom_inset, -top_inset + view.y))


func _update_player_views() -> void:
	for id in player_views:
		if not players_by_id.has(id):
			continue
		var player: Dictionary = players_by_id[id]
		var moving := bool(player.get("dashing", false))
		if id == player_id:
			moving = moving or Input.get_vector(
				"move_left", "move_right", "move_up", "move_down"
			).length_squared() > 0.01
			player_views[id].position = _local_player_render_position()
		else:
			moving = moving or (
				remote_target_positions.has(id)
				and remote_render_positions.has(id)
				and Vector2(remote_target_positions[id]).distance_to(remote_render_positions[id]) > 0.8
			)
			player_views[id].position = remote_render_positions.get(
				id,
				_to_vector(player.get("position", {}))
			)
		player_views[id].apply_state(player, _player_color(id), moving, id == player_id)


func _capture_snapshot_effects(
	next_players: Array,
	next_bullets: Array,
	next_items: Array,
	next_phase: String,
	next_time: float
) -> void:
	for player in next_players:
		var id := int(player.get("id", 0))
		if not players_by_id.has(id):
			continue
		var old: Dictionary = players_by_id[id]
		var position := _to_vector(player.get("position", {}))
		var color := _player_color_for_list(id, next_players)
		if int(player.get("hp", 0)) < int(old.get("hp", 0)):
			effect_layer.spawn_burst(position, color, 14, 120.0)
			effect_layer.flash(0.45)
			effect_layer.shake(5.0)
			hit_player.play()
		if bool(old.get("alive", true)) and not bool(player.get("alive", true)):
			effect_layer.spawn_burst(position, Color.WHITE, 26, 180.0)
			effect_layer.shake(8.0)
		if not bool(old.get("reloading", false)) and bool(player.get("reloading", false)):
			reload_player.play()
		if not bool(old.get("dashing", false)) and bool(player.get("dashing", false)):
			effect_layer.spawn_burst(position, color, 8, 80.0)
			dash_player.play()

	var next_bullet_ids: Dictionary = {}
	for bullet in next_bullets:
		var id := int(bullet.get("id", 0))
		next_bullet_ids[id] = true
		if not bullet_positions.has(id):
			var position := _to_vector(bullet.get("position", {}))
			var velocity := _to_vector(bullet.get("velocity", {}))
			var color := _player_color_for_list(int(bullet.get("owner_id", 0)), next_players)
			effect_layer.spawn_sparkle(position - velocity.normalized() * 9.0, color)
			shot_player.play()
	for id in bullet_positions.keys():
		if not next_bullet_ids.has(id):
			effect_layer.spawn_burst(bullet_positions[id], Color.WHITE, 5, 65.0)

	var next_item_ids: Dictionary = {}
	for item in next_items:
		next_item_ids[int(item.get("id", 0))] = true
	if next_phase == "running":
		for id in item_views:
			if not next_item_ids.has(id):
				effect_layer.spawn_burst(item_views[id].position, Color.WHITE, 16, 105.0)

	var next_second := int(ceil(next_time))
	if next_phase == "countdown" and next_second != countdown_second:
		countdown_second = next_second
		countdown_player.play()
	if next_phase != phase:
		if next_phase == "running":
			match_start_player.play()
			effect_layer.flash(0.28)
		elif next_phase == "match_finished":
			match_end_player.play()
		if next_phase != "countdown":
			countdown_second = -1


func _reconcile_local_player(player: Dictionary, server_position: Vector2) -> void:
	var old_visual_position := predictor.position + prediction_visual_offset
	var acknowledged_sequence := int(player.get("last_input_sequence", 0))
	var remaining_inputs: Array = []
	for input_record in pending_inputs:
		if int(input_record.get("sequence", 0)) > acknowledged_sequence:
			remaining_inputs.append(input_record)
	pending_inputs = remaining_inputs
	# サーバーの確定状態へ巻き戻し、まだ処理されていない入力だけを再適用する。
	predictor.reset_to(
		server_position,
		float(player.get("dash_time_left", 0.0)),
		float(player.get("dash_cooldown_left", 0.0)),
		bool(player.get("alive", true)),
		float(player.get("berserk_left", 0.0))
	)
	for input_record in pending_inputs:
		predictor.simulate(input_record)
	prediction_visual_offset = old_visual_position - predictor.position


func _player_color(id: int) -> Color:
	return _player_color_for_list(id, players)


func _player_color_for_list(id: int, player_list: Array) -> Color:
	var sorted_ids: Array[int] = []
	for player in player_list:
		sorted_ids.append(int(player.get("id", 0)))
	sorted_ids.sort()
	var index := sorted_ids.find(id)
	var colors := [CYAN, MAGENTA, YELLOW, GREEN]
	return colors[maxi(index, 0) % colors.size()]


func _to_vector(value: Dictionary) -> Vector2:
	return Vector2(float(value.get("x", 0.0)), float(value.get("y", 0.0)))


func _is_playing_phase() -> bool:
	return phase == "running"


func _local_held_item_kind() -> String:
	if not players_by_id.has(player_id):
		return ""
	var held = players_by_id[player_id].get("held_item")
	return str(held.get("kind", "")) if typeof(held) == TYPE_DICTIONARY else ""
