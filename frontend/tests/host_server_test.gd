extends SceneTree

## CREATE ROOM でサーバーをホストするときの、接続先の受け取りと失敗経路の検証。
##
## 成功経路は room_flow_test が通しで見ている。ここで見るのは、
## 実際に開いたポートを取り違えないか、起動できなかった／落ちたときに
## 原因が分かる形で伝わるか。この経路は普段動かないので、壊れても気付けない。
##
##     godot --headless --path frontend --script res://tests/host_server_test.gd

const HostServerController := preload("res://src/networking/host_server_controller.gd")

## 他の試験やCompose環境と重ならない範囲を使う。
const GAME_PORT := 9061

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	await _check_the_reported_address_is_used()
	await _check_control_port_collision_does_not_block_hosting()
	await _check_busy_port_falls_back_to_a_free_one()
	await _check_a_fixed_port_is_not_moved()
	await _check_death_is_noticed()
	await _check_failed_hosting_leaves_the_room_screen()
	_check_missing_binary_message_lists_paths()

	if not _failures.is_empty():
		push_error("host server:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("host server: 接続先の受け渡しと失敗経路が期待どおりだった")
	quit(0)


## 起動できること、接続先が返ること、ログがファイルへ残ること。
##
## 接続先はサーバーが待受に成功して初めて書き出す。届いたということが
## 「本当に繋がる状態になった」ことの証拠になる。
func _check_the_reported_address_is_used() -> void:
	var log_path := _clear_log()
	var probe := _Probe.new(self)
	await probe.start(GAME_PORT)

	if probe.started.is_empty():
		_failures.append("接続先が返ってこない。ルーム画面が接続先を出せない")
	elif probe.started[0] != "ws://%s:%d" % [NetworkConfig.LOCAL_SERVER_HOST, GAME_PORT]:
		_failures.append("空いているのに希望どおりの接続先にならない: %s" % probe.started[0])
	if not probe.moved.is_empty():
		_failures.append("動かす必要がないのに番号を変えたと通知した")

	var log_text := FileAccess.get_file_as_string(log_path)
	if log_text.is_empty():
		_failures.append("ログファイルに何も書かれていない: %s" % log_path)
	elif not log_text.contains(str(GAME_PORT)):
		_failures.append("ログに待受ポートが出てこない:\n%s" % log_text)

	await probe.finish()


## 制御APIのポートだけが埋まっていても、部屋は開けること。
##
## 以前はクライアント側が両方の空きを見てから起動しており、制御APIが
## 埋まっているだけでゲーム用ポートまでずらしていた。今はサーバーが
## それぞれ空きを探すので、対戦側は希望どおりの番号で開く。
func _check_control_port_collision_does_not_block_hosting() -> void:
	var control_port := GAME_PORT + HostServerController.CONTROL_PORT_OFFSET
	var blocker := TCPServer.new()
	if blocker.listen(control_port, NetworkConfig.LOCAL_SERVER_HOST) != OK:
		_failures.append("検査用にポート%dを確保できなかった" % control_port)
		return

	var log_path := _clear_log()
	var probe := _Probe.new(self)
	await probe.start(GAME_PORT)

	if probe.started.is_empty():
		_failures.append("制御APIのポートが埋まっているだけで部屋を開けない")
	elif not probe.started[0].ends_with(":%d" % GAME_PORT):
		_failures.append("対戦側まで巻き添えで動いた: %s" % probe.started[0])

	# 制御APIは黙って死なず、別の番号で開いていること。
	# 黙って死ぬと、管理機能だけが使えないことに誰も気付けない。
	var log_text := FileAccess.get_file_as_string(log_path)
	if not log_text.contains("control API listening"):
		_failures.append("制御APIが開けたかどうかログから分からない:\n%s" % log_text)
	elif log_text.contains("control API listening on http://%s:%d" % [
		NetworkConfig.LOCAL_SERVER_HOST, control_port
	]):
		_failures.append("塞いだはずの制御APIポートで開いたことになっている")

	await probe.finish()
	blocker.stop()


## 希望のポートが埋まっていても、空いている番号で開けること。
##
## 配布版を遊ぶ人にとってポート番号は本来どうでもよく、埋まっているだけで
## 部屋を作れないのは行き止まりになる。
func _check_busy_port_falls_back_to_a_free_one() -> void:
	var blocker := TCPServer.new()
	if blocker.listen(GAME_PORT, NetworkConfig.LOCAL_SERVER_HOST) != OK:
		_failures.append("検査用にポート%dを確保できなかった" % GAME_PORT)
		return

	_clear_log()
	var probe := _Probe.new(self)
	await probe.start(GAME_PORT)

	if probe.started.is_empty():
		_failures.append("希望のポートが埋まっていると起動できない")
	elif probe.started[0].ends_with(":%d" % GAME_PORT):
		_failures.append("塞いだはずのポートで起動している: %s" % probe.started[0])
	if probe.moved.is_empty():
		_failures.append("ポートを変えたことが伝わらない。他の人が接続先を知れない")
	elif not probe.started.is_empty() and not probe.started[0].ends_with(":%d" % probe.moved[0]):
		_failures.append(
			"通知した番号と実際の接続先が違う: %s / %d" % [probe.started[0], probe.moved[0]]
		)

	await probe.finish()
	blocker.stop()


## ポートを固定する設定なら、埋まっていても勝手に動かさないこと。
##
## 探索するか固定するかは server.json の port_search_range が決める。
## 公開先を他所へ知らせている場合、黙ってずらされると誰も繋がらなくなる。
## ここでは同じ設定を環境変数から与えて、固定側の挙動を確かめる。
func _check_a_fixed_port_is_not_moved() -> void:
	var blocker := TCPServer.new()
	if blocker.listen(GAME_PORT, NetworkConfig.LOCAL_SERVER_HOST) != OK:
		_failures.append("検査用にポート%dを確保できなかった" % GAME_PORT)
		return

	OS.set_environment("PIXEL_SHOOTER_PORT_SEARCH_RANGE", "0")
	_clear_log()
	var probe := _Probe.new(self, false)
	await probe.start(GAME_PORT)
	OS.unset_environment("PIXEL_SHOOTER_PORT_SEARCH_RANGE")

	if not probe.started.is_empty():
		_failures.append("固定指定なのに別の番号で開いた: %s" % probe.started[0])
	if probe.failed.is_empty():
		_failures.append("開けなかったことが伝わらない。ルーム画面で無言のまま固まる")
	elif not probe.failed[0].contains(str(GAME_PORT)):
		_failures.append("どのポートで失敗したのか伝えていない: %s" % probe.failed[0])
	elif not probe.failed[0].contains("server.log"):
		_failures.append("理由を調べる場所を伝えていない: %s" % probe.failed[0])

	await probe.finish()
	blocker.stop()


## 起動後に落ちたことに気付くこと。
##
## 気付けないと「SERVER OFFLINE — RETRYING」としか出ず、通信の問題なのか
## サーバーが死んだのか区別できない。
func _check_death_is_noticed() -> void:
	_clear_log()
	var probe := _Probe.new(self)
	await probe.start(GAME_PORT)
	if probe.started.is_empty():
		_failures.append("生存監視の検査用にサーバーを起動できなかった")
		await probe.finish()
		return

	# 外から落とす。CREATE ROOM 側は stop_server を通らない。
	OS.kill(probe.controller.server_pid)
	for _attempt in range(60):
		await create_timer(0.05).timeout
		if not probe.exited.is_empty():
			break

	if probe.exited.is_empty():
		_failures.append("サーバーが落ちても server_exited が飛んでこない")
	if probe.controller.owns_server():
		_failures.append("落ちたあともプロセスを掴んだままになっている")

	await probe.finish()


## 起動できなかったとき、操作できないルーム画面に取り残されないこと。
##
## ルーム画面は起動を待たずに開くため、失敗しても閉じないと
## ADD CPU も START GAME も効かない画面で詰む。
func _check_failed_hosting_leaves_the_room_screen() -> void:
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame

	var menu = main.get_node("MenuScreen")
	# CREATE ROOM と同じ順序で、先にルーム画面を開いてから失敗させる。
	menu.show_room(true, "ws://127.0.0.1:9001")
	if not menu.create_page.visible:
		_failures.append("検査の前提が崩れている: ルーム画面が開かない")
	main._on_local_server_failed("PORT 9001 IS ALREADY IN USE")
	await process_frame

	if menu.create_page.visible:
		_failures.append("起動に失敗してもルーム画面に留まっている")
	if not menu.status_label.text.contains("9001"):
		_failures.append("理由が表示されていない: %s" % menu.status_label.text)

	main.queue_free()
	await process_frame


## バイナリが見つからないとき、どこを探したかを伝えること。
func _check_missing_binary_message_lists_paths() -> void:
	var controller = HostServerController.new()
	var candidates: Array[String] = controller._server_executable_candidates()
	if candidates.is_empty():
		_failures.append("探索候補が空。どこを探したか伝えられない")
	for candidate in candidates:
		if not candidate.is_absolute_path():
			_failures.append("候補が絶対パスでない: %s" % candidate)
	controller.free()


## 前回の内容と混ざらないようログを消し、その置き場所を返す。
func _clear_log() -> String:
	var log_path := ProjectSettings.globalize_path("user://server.log")
	DirAccess.remove_absolute(log_path)
	return log_path


## HostServerController を1回起動し、飛んできた合図を集める。
##
## 接続先はサーバーが待受してから返るため、start_server は同期では終わらない。
## 待ち方をここへ集めて、各検査は結果だけを見る。
class _Probe:
	var controller
	var started: Array[String] = []
	var failed: Array[String] = []
	var moved: Array[int] = []
	var exited: Array[int] = []

	var _tree: SceneTree
	## 起動できるはずの検査かどうか。できないはずの検査では failed を待つ。
	var _expect_success: bool

	func _init(tree: SceneTree, expect_success := true) -> void:
		_tree = tree
		_expect_success = expect_success
		controller = HostServerController.new()
		_tree.root.add_child(controller)
		controller.server_started.connect(func(url: String) -> void: started.append(url))
		controller.server_failed.connect(func(text: String) -> void: failed.append(text))
		controller.server_exited.connect(func(code: int) -> void: exited.append(code))
		controller.port_changed.connect(
			func(_requested: int, used: int) -> void: moved.append(used)
		)

	## 起動し、結果が出るまで待つ。
	func start(port: int) -> void:
		controller.start_server(port)
		for _attempt in range(160):
			await _tree.create_timer(0.05).timeout
			if not started.is_empty() or not failed.is_empty():
				break
		if _expect_success and started.is_empty() and not failed.is_empty():
			# 呼び出し側の検査項目より先に、起動できなかったことを言う。
			push_warning("サーバーを起動できなかった: %s" % failed[0])

	func finish() -> void:
		controller.stop_server()
		controller.queue_free()
		await _tree.process_frame
