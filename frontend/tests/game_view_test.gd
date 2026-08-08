extends SceneTree

## 対戦画面の見え方の検証。
##
## 見た目の話に見えるが、どれも遊べるかどうかに直結する。HUDがマップの上へ
## 重なれば遮蔽の裏が読めず、カメラがマップの外を映せば端で方向を見失い、
## 照準が画面座標のままだと狙った所と撃つ向きが食い違う。
##
##     godot --headless --path frontend --script res://tests/game_view_test.gd

## 対戦中ずっと出ているわけではない表示。中央に出るのが正しいので、
## マップ帯との重なりは見ない。ここに書いていないHUDはすべて検査対象になる。
const TRANSIENT_OVERLAYS := ["CountdownLabel", "ResultOverlay"]

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	await _check_the_minimap_is_gone()
	await _check_the_hud_stays_out_of_the_map()
	await _check_the_camera_centers_the_local_player()
	await _check_the_camera_never_leaves_the_map()
	await _check_aim_is_measured_in_world_space()
	await _check_the_camera_is_off_outside_a_match()

	if not _failures.is_empty():
		push_error("game view:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("game view: HUDの配置とカメラの追従が期待どおりだった")
	quit(0)


## 右上の小マップが無いこと。
func _check_the_minimap_is_gone() -> void:
	var main = await _open_main()
	var hud_root = main.get_node("GameScreen/HUD/HUDRoot")
	if hud_root.has_node("RadarDisplay"):
		_failures.append("小マップが残っている")
	if ResourceLoader.exists("res://src/ui/hud/radar_display.gd"):
		_failures.append("小マップの描画スクリプトが残っている")
	await _close(main)


## 常時出ているHUDが、マップを映す帯と重ならないこと。
##
## 重なると、その裏にいる相手やアイテムが見えない。持ち物や残り時間は
## ずっと出ているので、覗き込む場所を奪い続けることになる。
func _check_the_hud_stays_out_of_the_map() -> void:
	var main = await _open_main()
	var hud = main.get_node("GameScreen/HUD")
	var hud_root = hud.get_node("HUDRoot")
	var band := Rect2(0.0, hud.WORLD_VIEW_TOP, 640.0, hud.WORLD_VIEW_BOTTOM - hud.WORLD_VIEW_TOP)

	var checked := 0
	for child in hud_root.get_children():
		if child.name in TRANSIENT_OVERLAYS or not child is Control:
			continue
		checked += 1
		var rect := Rect2(child.position, child.size)
		if rect.intersects(band):
			_failures.append(
				"%s がマップの上に重なっている: %s" % [child.name, rect]
			)
	if checked < 5:
		_failures.append("検査の前提が崩れている: HUDを%d個しか見ていない" % checked)
	await _close(main)


## 自機が画面の中心に来ること。
func _check_the_camera_centers_the_local_player() -> void:
	var main = await _open_main()
	var game = main.get_node("GameScreen")
	# 端で寄せ止めされない、マップの真ん中あたりに置く。
	var target := Vector2(320.0, 176.0)
	await _start_match(game, target)

	var camera: Camera2D = game.get_node("FollowCamera")
	var center := Vector2(320.0, 200.0)
	# 送られてきた座標ではなく、実際に描いている位置で見る。クライアントは
	# 補間するので、両者は一致しない。中心に来るべきなのは描いている方。
	var on_screen := _screen_position(game, camera, _drawn_local_player(game))
	if on_screen.distance_to(center) > 2.0:
		_failures.append("自機が画面の中心にいない: %s" % on_screen)

	# 動いたらカメラも追うこと。置いただけで追わないと、端まで歩くと見失う。
	# 移動先も寄せ止めの外側に取る。端に寄せると中心から外れるのが正しい挙動で、
	# それは別の検査で見ている。
	await _place_local_player(game, target + Vector2(80.0, 44.0))
	var moved_on_screen := _screen_position(game, camera, _drawn_local_player(game))
	if moved_on_screen.distance_to(center) > 2.0:
		_failures.append("自機が動いてもカメラが追わない: %s" % moved_on_screen)

	await _close(main)


## マップの外や、HUDの帯の下をカメラが映さないこと。
##
## 端まで寄せたときにマップの外の黒が出ると、どこが壁でどこが場外か分からない。
## 帯の下へ潜ると、一番端の行に立ったとき自分の足元が見えない。
func _check_the_camera_never_leaves_the_map() -> void:
	var main = await _open_main()
	var game = main.get_node("GameScreen")
	var hud = game.get_node("HUD")
	var camera: Camera2D = game.get_node("FollowCamera")
	await _start_match(game, Vector2(320.0, 176.0))

	# 隅にいるときは寄せ止めされ、マップの角が帯の角とちょうど重なるのが正しい。
	# 内側へずれるとマップの外の黒が見え、外側へずれると端の行がHUDの下へ潜る。
	await _place_local_player(game, Vector2(4.0, 4.0))
	var top_left := _screen_position(game, camera, Vector2.ZERO)
	if absf(top_left.x) > 1.0 or absf(top_left.y - hud.WORLD_VIEW_TOP) > 1.0:
		_failures.append(
			"左上の隅で、マップの角が帯の角に合わない: %s / 期待 (0, %s)"
			% [top_left, hud.WORLD_VIEW_TOP]
		)

	await _place_local_player(game, Vector2(636.0, 348.0))
	var bottom_right := _screen_position(game, camera, Vector2(640.0, 352.0))
	if absf(bottom_right.x - 640.0) > 1.0 or absf(bottom_right.y - hud.WORLD_VIEW_BOTTOM) > 1.0:
		_failures.append(
			"右下の隅で、マップの角が帯の角に合わない: %s / 期待 (640, %s)"
			% [bottom_right, hud.WORLD_VIEW_BOTTOM]
		)

	await _close(main)


## 照準がワールド座標で取られていること。
##
## 画面座標のまま扱うと、カメラが動いたぶんだけ狙いがずれる。カメラが
## 動くようになった今、この2つは一致しない。
func _check_aim_is_measured_in_world_space() -> void:
	var main = await _open_main()
	var game = main.get_node("GameScreen")
	var camera: Camera2D = game.get_node("FollowCamera")
	await _start_match(game, Vector2(320.0, 176.0))

	var screen_mouse: Vector2 = game.get_viewport().get_mouse_position()
	var world_mouse: Vector2 = game.mouse_world_position()
	var expected: Vector2 = (
		camera.get_screen_center_position()
		- Vector2(320.0, 200.0) / game.FOLLOW_ZOOM
		+ screen_mouse / game.FOLLOW_ZOOM
	)
	if world_mouse.distance_to(expected) > 1.0:
		_failures.append(
			"マウスのワールド座標が合わない: %s / 期待 %s" % [world_mouse, expected]
		)
	if world_mouse.distance_to(screen_mouse) < 1.0:
		_failures.append("画面座標のまま扱っている。カメラの移動ぶん狙いがずれる")

	await _close(main)


## 対戦していないあいだはカメラを切ること。
##
## メニューはCanvasLayerの外のControlなので、有効なままだと拡大・移動される。
func _check_the_camera_is_off_outside_a_match() -> void:
	var main = await _open_main()
	var game = main.get_node("GameScreen")
	var camera: Camera2D = game.get_node("FollowCamera")

	if camera.enabled:
		_failures.append("メニューにいるのにカメラが効いている")
	await _start_match(game, Vector2(320.0, 176.0))
	if not camera.enabled:
		_failures.append("対戦中なのにカメラが効いていない")
	game.end_session()
	await process_frame
	if camera.enabled:
		_failures.append("対戦を抜けてもカメラが効いたまま")

	await _close(main)


## 実際に描いている自機の位置。画面揺れのぶんも含む。
func _drawn_local_player(game) -> Vector2:
	return game.player_views[1].global_position


## ワールド座標が画面のどこに出るか。
func _screen_position(game, camera: Camera2D, world: Vector2) -> Vector2:
	var zoom: float = game.FOLLOW_ZOOM
	return (world - camera.get_screen_center_position()) * zoom + Vector2(320.0, 200.0)


func _start_match(game, position: Vector2) -> void:
	game.start_session(1)
	game._on_map_definition_received(_map_definition())
	await _place_local_player(game, position)


func _place_local_player(game, position: Vector2) -> void:
	game._on_snapshot_received({
		"tick": 1,
		"phase": "running",
		"time_left": 90.0,
		"winner_id": null,
		"reconnect_grace_left": 0.0,
		"move_speed": 150.0,
		"dash_speed": 520.0,
		"dash_duration": 0.13,
		"dash_cooldown": 1.1,
		"players": [{
			"id": 1,
			"name": "P1",
			"position": {"x": position.x, "y": position.y},
			"aim": {"x": 1.0, "y": 0.0},
			"hp": 5,
			"max_hp": 5,
			"score": 0,
			"is_cpu": false,
			"is_dummy": false,
			"connected": true,
			"alive": true,
			"ammo": 6,
			"max_ammo": 6,
			"last_input_sequence": 0,
		}],
		"bullets": [],
		"items": [],
		"larokin_poppos": [],
		"ghost_thieves": [],
		"room": {"host_player_id": 1, "can_start": false, "max_players": 4, "settings": {}},
	})
	# クライアントは表示位置を補間するので、落ち着くまで待つ。
	# 補間そのものは検査の対象ではない。動きが止まってからカメラを見る。
	var previous := Vector2.INF
	for _attempt in range(240):
		await process_frame
		if not game.player_views.has(1):
			continue
		var current: Vector2 = game.player_views[1].global_position
		if current.distance_to(previous) < 0.05:
			return
		previous = current
	_failures.append("表示位置が落ち着かない。補間が終わらない")


## 検査用の20×11マップ。実際に配っているものと同じ寸法にする。
func _map_definition() -> Dictionary:
	var rows: Array[String] = []
	for y in range(11):
		if y == 0 or y == 10:
			rows.append("#".repeat(20))
		else:
			rows.append("#" + ".".repeat(18) + "#")
	return {
		"schema_version": 1,
		"id": "game_view_test",
		"revision": "1",
		"name": "Game View Test",
		"width": 20,
		"height": 11,
		"tile_size": 32,
		"tiles": rows,
		"spawn_points": [[1, 1], [18, 9], [18, 1], [1, 9]],
		"item_spawn_points": [[5, 5], [14, 5]],
	}


func _open_main():
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame
	return main


func _close(main) -> void:
	main.queue_free()
	await process_frame
