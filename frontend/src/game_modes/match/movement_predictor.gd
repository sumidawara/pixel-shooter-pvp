class_name MovementPredictor
extends RefCounted

## ローカルプレイヤーの位置を先読みするクライアント予測。
##
## ここはサーバーの権威計算と同じ規則を、GDScriptで再実装したものである。
## 対応するサーバー実装:
##
## - backend/game-core/src/game/movement.rs (move_players)
## - backend/game-core/src/game/items.rs (アイテムのダッシュ)
## - backend/game-core/src/arena.rs (move_with_collision / valid_player_position)
##
## 2つの実装が一致していることは型では保証できないため、
## backend/game-core/tests/movement_prediction_golden.rs が「入力列 → 位置列」の
## ゴールデンベクタを生成し、frontend/tests/movement_prediction_golden_test.gd が
## 同じ入力をこのクラスへ流して一致を検証する。
## どちらか一方だけを変更すると、必ずテストが落ちる。

const PLAYER_RADIUS := 12.0
## 移動入力とみなす最小の長さの二乗。サーバーの 0.001 と揃える。
const MIN_MOVEMENT_LENGTH_SQUARED := 0.001

var position := Vector2.ZERO
var dash_time_left := 0.0
var dash_cooldown_left := 0.0
var dash_direction := Vector2.ZERO
var ready := false

# サーバーが決める状態。スナップショット受信時に取り込み、
# 未処理入力の再適用中はサーバーと同じ規則で自前に進める。
var alive := true
var berserk_left := 0.0

var _map: ArenaMapData = null
var _move_speed := 150.0
var _dash_speed := 520.0
var _dash_duration := 0.13
var _dash_cooldown := 1.1


func set_map(map: ArenaMapData) -> void:
	_map = map
	ready = false


## サーバーが配信する操作パラメーターを取り込む。
## server.json を変えてもサーバーの確定計算と一致させるため、定数化しない。
func set_gameplay(move_speed: float, dash_speed: float, dash_duration: float, dash_cooldown: float) -> void:
	_move_speed = move_speed
	_dash_speed = dash_speed
	_dash_duration = dash_duration
	_dash_cooldown = dash_cooldown


func invalidate() -> void:
	ready = false


## サーバーの確定値で予測を巻き戻す。この後に未処理入力を再適用する。
func reset_to(
	server_position: Vector2,
	server_dash_time_left: float,
	server_dash_cooldown_left: float,
	server_alive: bool,
	server_berserk_left: float
) -> void:
	position = server_position
	dash_time_left = server_dash_time_left
	dash_cooldown_left = server_dash_cooldown_left
	alive = server_alive
	berserk_left = server_berserk_left
	ready = true


## 1tick分の入力を適用する。
##
## `input` に必要なキー: delta, movement, dash_pressed, use_item_pressed, held_kind
##
## System の実行順（update_items → move_players）を保つ必要がある。
## 順序を入れ替えるとサーバーと結果がずれる。
func simulate(input: Dictionary) -> void:
	if not ready:
		return
	var delta := float(input.get("delta", 0.0))
	var movement: Vector2 = Vector2(input.get("movement", Vector2.ZERO)).limit_length(1.0)

	# --- update_items 相当 ---
	# バーサクの残り時間は生死に関係なく減る。
	berserk_left = maxf(berserk_left - delta, 0.0)
	if (
		alive
		and bool(input.get("use_item_pressed", false))
		and str(input.get("held_kind", "")) == "dash"
		and movement.length_squared() > MIN_MOVEMENT_LENGTH_SQUARED
	):
		# アイテムのダッシュはクールダウンを消費しない。
		dash_direction = movement.normalized()
		dash_time_left = _dash_duration

	# --- move_players 相当 ---
	# 死亡中は移動もクールダウンの進行も止まる。
	if not alive:
		return
	dash_cooldown_left = maxf(dash_cooldown_left - delta, 0.0)
	if (
		bool(input.get("dash_pressed", false))
		and dash_cooldown_left <= 0.0
		and movement.length_squared() > MIN_MOVEMENT_LENGTH_SQUARED
	):
		dash_direction = movement.normalized()
		dash_time_left = _dash_duration
		dash_cooldown_left = _dash_cooldown

	var direction := movement
	var speed := _move_speed * (0.5 if berserk_left > 0.0 else 1.0)
	if dash_time_left > 0.0:
		dash_time_left = maxf(dash_time_left - delta, 0.0)
		direction = dash_direction
		speed = _dash_speed
	_move_with_collision(direction * speed * delta)


## X軸とY軸を別々に判定する。
## まとめて移動すると、片方の軸が壁に当たっただけで両方向とも止まってしまう。
func _move_with_collision(delta: Vector2) -> void:
	var next := position
	next.x += delta.x
	if _valid_player_position(next):
		position.x = next.x
	next = position
	next.y += delta.y
	if _valid_player_position(next):
		position.y = next.y


func _valid_player_position(candidate: Vector2) -> bool:
	if _map == null:
		return false
	var arena_size := _map.size_pixels()
	return (
		candidate.x >= PLAYER_RADIUS
		and candidate.x <= arena_size.x - PLAYER_RADIUS
		and candidate.y >= PLAYER_RADIUS
		and candidate.y <= arena_size.y - PLAYER_RADIUS
		and not _map.obstacle_at(candidate, PLAYER_RADIUS)
	)
