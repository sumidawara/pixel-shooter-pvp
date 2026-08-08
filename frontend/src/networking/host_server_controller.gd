extends Node

## CREATE ROOM で同梱のRustサーバーを子プロセスとして起動する。
##
## 子プロセスは画面もログも持たないため、失敗したときに何も分からなくなりやすい。
## 起動できない理由、落ちたこと、ログの場所は、いずれもここから伝える。

signal server_started(url: String)
signal server_failed(reason: String)
## 起動には成功したが、その後に落ちた。
signal server_exited(exit_code: int)
## 希望のポートが埋まっていたので、別の番号で開いた。
signal port_changed(requested: int, used: int)

## 制御APIの待受ポートは、ゲーム用ポートからこの値だけずらして決める。
## 既定の 9001 / 9101 と同じ関係にしてある。
const CONTROL_PORT_OFFSET := 100
## 生存確認の間隔。落ちたことに気付くのが目的なので、細かく見る必要はない。
const LIVENESS_CHECK_SECONDS := 0.5
## 希望のポートが埋まっていたときに、いくつ先まで空きを探すか。
##
## 配布版を遊ぶ人にとって、ポート番号は本来どうでもよい。埋まっているだけで
## 部屋を作れなくなるより、空いている所で開いて番号を見せるほうがよい。
const PORT_SEARCH_RANGE := 20

var server_pid := -1
var log_path := ""

var _liveness_left := 0.0


func start_server(port: int) -> void:
	stop_server()
	if OS.has_feature("web"):
		server_failed.emit("CREATE ROOM IS NOT AVAILABLE IN WEB BUILDS")
		return

	# 希望のポートから順に、ゲーム用と制御API用が両方空いている組を探す。
	# 制御APIだけ衝突した場合もサーバーは起動を続けてしまい、
	# 気付かないまま管理機能だけが死ぬため、両方を見る。
	var chosen_port := _find_free_port(port)
	if chosen_port < 0:
		server_failed.emit(
			"NO FREE PORT BETWEEN %d AND %d — CLOSE SOME APPS OR CHANGE THE PORT IN SETTINGS"
			% [port, port + PORT_SEARCH_RANGE - 1]
		)
		return
	var control_port := chosen_port + CONTROL_PORT_OFFSET

	var server_path := _find_server_executable()
	if server_path.is_empty():
		server_failed.emit(
			"RUST SERVER BINARY WAS NOT FOUND\nLOOKED IN:\n  %s"
			% "\n  ".join(_server_executable_candidates())
		)
		return

	log_path = _log_path()
	server_pid = OS.create_process(
		server_path,
		[
			"--bind", NetworkConfig.local_server_bind_address(chosen_port),
			"--debug-bind", NetworkConfig.local_server_bind_address(control_port),
			"--log-file", log_path,
		],
		false
	)
	if server_pid <= 0:
		server_pid = -1
		server_failed.emit("COULD NOT START THE RUST SERVER\n%s" % server_path)
		return
	_liveness_left = LIVENESS_CHECK_SECONDS
	if chosen_port != port:
		# 番号が変わったことは伝える。他の人が入るときに必要になる。
		port_changed.emit(port, chosen_port)
	server_started.emit(NetworkConfig.local_game_server_url(chosen_port))


func stop_server() -> void:
	if server_pid > 0:
		OS.kill(server_pid)
		server_pid = -1


func owns_server() -> bool:
	return server_pid > 0


func _process(delta: float) -> void:
	if server_pid <= 0:
		return
	_liveness_left -= delta
	if _liveness_left > 0.0:
		return
	_liveness_left = LIVENESS_CHECK_SECONDS
	if OS.is_process_running(server_pid):
		return
	# 落ちた。放っておくと「SERVER OFFLINE — RETRYING」としか出ず、
	# 通信の問題なのかサーバーが死んだのか区別できない。
	var exit_code := OS.get_process_exit_code(server_pid)
	server_pid = -1
	server_exited.emit(exit_code)


func _exit_tree() -> void:
	stop_server()


## ゲーム用と制御API用が両方空いている番号を、希望の値から順に探す。
## 見つからなければ -1。
func _find_free_port(preferred: int) -> int:
	for offset in range(PORT_SEARCH_RANGE):
		var candidate := preferred + offset
		if candidate + CONTROL_PORT_OFFSET > 65535:
			break
		if _port_is_free(candidate) and _port_is_free(candidate + CONTROL_PORT_OFFSET):
			return candidate
	return -1


func _port_is_free(port: int) -> bool:
	var probe := TCPServer.new()
	if probe.listen(port, NetworkConfig.LOCAL_SERVER_HOST) != OK:
		return false
	probe.stop()
	return true


## サーバーのログの置き場所。
##
## 配布版では子プロセスの出力がどこにも出ないため、ファイルへ残す。
## user:// はプラットフォームごとの書き込み可能な場所を指す。
func _log_path() -> String:
	return ProjectSettings.globalize_path("user://server.log")


func _server_executable_candidates() -> Array[String]:
	var executable_name := (
		"pixel-shooter-server.exe" if OS.has_feature("windows") else "pixel-shooter-server"
	)
	var executable_dir := OS.get_executable_path().get_base_dir()
	var candidates: Array[String] = []
	if OS.has_feature("macos") and not OS.has_feature("editor"):
		candidates.append(executable_dir.path_join(executable_name))
	else:
		candidates.append(executable_dir.path_join("server").path_join(executable_name))
		candidates.append(executable_dir.path_join(executable_name))
	if OS.has_feature("editor"):
		candidates.append(ProjectSettings.globalize_path("res://../target/debug/" + executable_name))
		candidates.append(ProjectSettings.globalize_path("res://../target/release/" + executable_name))
	return candidates


func _find_server_executable() -> String:
	for candidate in _server_executable_candidates():
		if FileAccess.file_exists(candidate):
			return candidate
	return ""
