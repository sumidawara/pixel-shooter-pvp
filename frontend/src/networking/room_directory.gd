class_name RoomDirectory
extends Node

## ロビーから、今開いているルームの一覧を取ってくる。
##
## クライアントが知ってよいURLはロビー1つだけにしてある。AdminServerには試合を
## 止める・1tick進める操作の口があり、そのURLを配ると誰でも他人の試合を触れる。
## ロビー（Matchmaker）が公開用の一覧だけを返す窓口になっている。
##
## 一覧は必ず古くなる。GameServerの報告間隔ぶん遅れるので、「押した瞬間に満室」は
## 起こる前提で作る。参加が弾かれたら一覧へ戻して取り直す。

signal rooms_received(rooms: Array)
signal fetch_failed(reason: String)

const ROOMS_PATH := "/v1/rooms"
## 応答を待つ上限。ここを過ぎたら諦めて理由を出す。
##
## 黙って空の一覧を見せると「部屋が無い」と「繋がらない」が区別できない。
const REQUEST_TIMEOUT_SECONDS := 5.0

var _request: HTTPRequest
var _fetching := false


func _ready() -> void:
	_request = HTTPRequest.new()
	_request.timeout = REQUEST_TIMEOUT_SECONDS
	add_child(_request)
	_request.request_completed.connect(_on_request_completed)


func is_fetching() -> bool:
	return _fetching


## `lobby_url` のロビーへ一覧を求める。
func fetch(lobby_url: String) -> void:
	if _fetching:
		return
	var base := _as_http_url(lobby_url.strip_edges())
	if base.is_empty():
		fetch_failed.emit("LOBBY ADDRESS IS EMPTY")
		return
	_fetching = true
	var error := _request.request(base + ROOMS_PATH)
	if error != OK:
		_fetching = false
		fetch_failed.emit("COULD NOT REACH %s" % base)


## WebSocketのURLを渡されても一覧を引けるようにする。
##
## 画面には接続先が1つしか出ないので、人は ws:// と http:// を区別しない。
func _as_http_url(url: String) -> String:
	if url.begins_with("ws://"):
		return "http://" + url.substr(5)
	if url.begins_with("wss://"):
		return "https://" + url.substr(6)
	if url.begins_with("http://") or url.begins_with("https://"):
		return url
	if url.is_empty():
		return ""
	return "http://" + url


func _on_request_completed(
	result: int, response_code: int, _headers: PackedStringArray, body: PackedByteArray
) -> void:
	_fetching = false
	if result != HTTPRequest.RESULT_SUCCESS:
		fetch_failed.emit("LOBBY DID NOT ANSWER")
		return
	if response_code != 200:
		fetch_failed.emit("LOBBY RETURNED %d" % response_code)
		return
	var parsed = JSON.parse_string(body.get_string_from_utf8())
	if typeof(parsed) != TYPE_DICTIONARY or not parsed.has("rooms"):
		fetch_failed.emit("LOBBY RETURNED SOMETHING ELSE")
		return
	rooms_received.emit(parsed["rooms"])
