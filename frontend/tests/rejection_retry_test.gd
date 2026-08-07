extends SceneTree

## 満室・試合開始済みで拒否されたとき、別のルームを取り直すことを検証する。
##
## AdminServerは試合中のルームを避けて割り当てるが、heartbeat間隔ぶんの
## すれ違いは残る。そこで拒否された側が諦めると、空いているGameServerが
## あってもプレイヤーはエラー表示のまま行き止まりになる。
##
##     godot --headless --path frontend --script res://tests/rejection_retry_test.gd

## 実際には応答しない宛先。取り直しを「開始した」ことだけを見る。
const UNREACHABLE_MATCHMAKER := "http://127.0.0.1:1"

var _statuses: Array[String] = []
var _rejections: Array[String] = []


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame

	var network = root.get_node("NetworkClient")
	network.status_changed.connect(func(text: String) -> void: _statuses.append(text))
	network.rejected.connect(func(reason: String) -> void: _rejections.append(reason))

	# 1) 取り直せる拒否は、諦めずに次のルームを探す。
	_reset(network, UNREACHABLE_MATCHMAKER, 1)
	network._receive(_rejection(true, "The room already has four players."))
	if not _rejections.is_empty():
		push_error("rejection retry: 取り直せる拒否で諦めてしまった: %s" % [_rejections])
		return _fail(main, network)
	if network.matchmake_attempts != 2:
		push_error(
			"rejection retry: 取り直しが始まっていない (attempts=%d)" % network.matchmake_attempts
		)
		return _fail(main, network)

	# 2) 上限まで試したら諦めて、理由をユーザーへ返す。
	_reset(network, UNREACHABLE_MATCHMAKER, network.MATCHMAKE_MAX_ATTEMPTS)
	network._receive(_rejection(true, "The match has already started."))
	if not _rejected_only("The match has already started."):
		push_error("rejection retry: 上限到達後も拒否をユーザーへ返していない: %s" % [_rejections])
		return _fail(main, network)

	# 3) 別のルームでも直らない拒否は、その場でユーザーへ返す。
	_reset(network, UNREACHABLE_MATCHMAKER, 1)
	network._receive(_rejection(false, "Invalid join ticket: ticket expired."))
	if not _rejected_only("Invalid join ticket: ticket expired."):
		push_error("rejection retry: 再試行しても直らない拒否を取り直してしまった")
		return _fail(main, network)
	if network.matchmake_attempts != 1:
		push_error("rejection retry: 再試行不可の拒否でルームを取り直してしまった")
		return _fail(main, network)

	# 4) URL直指定の接続には取り直し先が無いので、そのまま返す。
	_reset(network, "", 0)
	network._receive(_rejection(true, "The room already has four players."))
	if not _rejected_only("The room already has four players."):
		push_error("rejection retry: URL直指定の接続で取り直そうとした")
		return _fail(main, network)

	print("rejection retry: 取り直しの可否がすべて期待どおりだった")
	network.disconnect_from_server()
	main.queue_free()
	await process_frame
	quit(0)


## 期待した理由がちょうど1件だけユーザーへ返ったか。
func _rejected_only(expected: String) -> bool:
	return _rejections.size() == 1 and _rejections[0] == expected


func _reset(network, matchmaker_url: String, attempts: int) -> void:
	_statuses.clear()
	_rejections.clear()
	network.matchmaker_url = matchmaker_url
	network.matchmake_attempts = attempts
	network.connection_requested = true


func _rejection(retryable: bool, reason: String) -> String:
	return JSON.stringify({"type": "rejected", "reason": reason, "retryable": retryable})


func _fail(main, network) -> void:
	network.disconnect_from_server()
	main.queue_free()
	quit(1)
