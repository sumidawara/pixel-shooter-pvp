extends SceneTree

## プレイヤーの絵が進む向きを向くかの検証。
##
## 原本の絵は右向きにしか描かれていないので、左へ歩いている間も右を向いていた。
## 撃ち合いの最中にどちらへ逃げたかが絵から読めないのは、見た目の問題ではなく
## 追う・待ち伏せるの判断に効く。
##
## 止まったときに正面へ戻さないことも合わせて見る。戻すと、手を離すたびに
## 勝手に反転して「今どちらを向いているのか」が信用できなくなる。
##
##     godot --headless --path frontend --script res://tests/player_facing_test.gd

const PLAYER_VIEW_SCENE := preload("res://src/actors/player/player_view.tscn")

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	await _check_the_sprite_turns_the_way_it_moves()
	await _check_standing_still_keeps_the_last_direction()
	await _check_both_sprites_turn_together()
	await _check_other_players_turn_too()

	if not _failures.is_empty():
		push_error("player facing:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("player facing: 進む向きを向くことを確認した")
	quit(0)


## 右へ動けば右、左へ動けば左を向くこと。
func _check_the_sprite_turns_the_way_it_moves() -> void:
	var view = await _open_view()

	view.apply_state(_alive_player(), Color.WHITE, true, true, -1.0)
	if not view.character_sprite.flip_h:
		_failures.append("左へ動いているのに左を向かない")

	view.apply_state(_alive_player(), Color.WHITE, true, true, 1.0)
	if view.character_sprite.flip_h:
		_failures.append("右へ動いているのに右を向かない")

	await _close(view)


## 横へ動いていない間は、直前の向きを保つこと。
##
## 0を「正面」と解釈して右へ戻すと、左へ逃げて止まった相手が右を向く。
func _check_standing_still_keeps_the_last_direction() -> void:
	var view = await _open_view()

	view.apply_state(_alive_player(), Color.WHITE, true, true, -1.0)
	view.apply_state(_alive_player(), Color.WHITE, false, true, 0.0)
	if not view.character_sprite.flip_h:
		_failures.append("左を向いて止まると右へ戻ってしまう")

	# 止まっている間に絵を描き直しても保つこと。
	view._process(0.016)
	if not view.character_sprite.flip_h:
		_failures.append("止まったまま描き直すと右へ戻ってしまう")

	await _close(view)


## 本体と縁取りが同じ向きを向くこと。
##
## 縁取りは一回り大きい同じ絵を重ねている。片方だけ反転すると、
## 反対側にはみ出して輪郭が二重に見える。
func _check_both_sprites_turn_together() -> void:
	var view = await _open_view()

	view.apply_state(_alive_player(), Color.WHITE, true, true, -1.0)
	if view.outline_sprite.flip_h != view.character_sprite.flip_h:
		_failures.append(
			"縁取りと本体の向きが違う: 縁取り %s / 本体 %s"
			% [view.outline_sprite.flip_h, view.character_sprite.flip_h]
		)

	await _close(view)


## 自分以外のプレイヤーも向きを変えること。
##
## 自機は入力から向きを取れるが、他プレイヤーの入力は届かない。位置の変化から
## 導く必要があり、ここを落とすと「自分だけ向く」状態になる。
func _check_other_players_turn_too() -> void:
	var main = await _open_main()
	var game = main.get_node("GameScreen")
	game.start_session(1)
	game._on_map_definition_received(_map_definition())

	# まず2人を置き、表示位置を落ち着かせる。
	await _send_snapshot(game, Vector2(320.0, 176.0), Vector2(400.0, 176.0))
	if not await _settle(game, 2, Vector2(400.0, 176.0)):
		_failures.append("相手の表示位置が落ち着かない")
		await _close_main(main)
		return

	# 相手を左へ飛ばす。補間で追いかける間、左を向いているはず。
	await _send_snapshot(game, Vector2(320.0, 176.0), Vector2(240.0, 176.0))
	await process_frame
	if not game.player_views[2].character_sprite.flip_h:
		_failures.append("相手が左へ動いても左を向かない")

	# 右へ戻せば右を向くこと。
	await _send_snapshot(game, Vector2(320.0, 176.0), Vector2(420.0, 176.0))
	await process_frame
	if game.player_views[2].character_sprite.flip_h:
		_failures.append("相手が右へ動いても右を向かない")

	await _close_main(main)


## 表示位置が目標へ届くまで待つ。届いたらtrue。
func _settle(game, id: int, target: Vector2) -> bool:
	for _attempt in range(600):
		await process_frame
		if (
			game.player_views.has(id)
			and game.player_views[id].global_position.distance_to(target) < 0.5
		):
			return true
	return false


func _send_snapshot(game, local: Vector2, other: Vector2) -> void:
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
		"players": [
			_snapshot_player(1, local),
			_snapshot_player(2, other),
		],
		"bullets": [],
		"items": [],
		"larokin_poppos": [],
		"ghost_thieves": [],
		"room": {"host_player_id": 1, "can_start": false, "max_players": 4, "settings": {}},
	})
	await process_frame


func _snapshot_player(id: int, position: Vector2) -> Dictionary:
	return {
		"id": id,
		"name": "P%d" % id,
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
	}


## 絵を出すのに最低限必要な状態。倒れていると描き直さない。
func _alive_player() -> Dictionary:
	return {
		"alive": true,
		"connected": true,
		"aim": {"x": 1.0, "y": 0.0},
		"ammo": 6,
		"hp": 5,
	}


## 検査用の20×11マップ。
func _map_definition() -> Dictionary:
	var rows: Array[String] = []
	for y in range(11):
		if y == 0 or y == 10:
			rows.append("#".repeat(20))
		else:
			rows.append("#" + ".".repeat(18) + "#")
	return {
		"schema_version": 1,
		"id": "player_facing_test",
		"revision": "1",
		"name": "Player Facing Test",
		"width": 20,
		"height": 11,
		"tile_size": 32,
		"tiles": rows,
		"spawn_points": [[1, 1], [18, 9], [18, 1], [1, 9]],
		"item_spawn_points": [[5, 5], [14, 5]],
	}


func _open_view():
	var view = PLAYER_VIEW_SCENE.instantiate()
	root.add_child(view)
	await process_frame
	return view


func _close(view) -> void:
	view.queue_free()
	await process_frame


func _open_main():
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame
	return main


func _close_main(main) -> void:
	main.queue_free()
	await process_frame
