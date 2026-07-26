class_name NetworkConfig
extends RefCounted

## Godotクライアントが利用する接続先をまとめた設定。
## 開発環境の接続先を変える場合は、まずこのファイルを変更する。

const SERVER_URL_ENV := "PIXEL_SHOOTER_SERVER_URL"

const MATCHMAKER_PORT := 8080
const DEFAULT_MATCHMAKER_URL := "http://127.0.0.1:8080"
const DEFAULT_GAME_SERVER_URL := "ws://127.0.0.1:9001"

const LOCAL_SERVER_HOST := "127.0.0.1"


static func initial_connection_url() -> String:
	var environment_url := OS.get_environment(SERVER_URL_ENV).strip_edges()
	return environment_url if not environment_url.is_empty() else DEFAULT_MATCHMAKER_URL


static func local_server_bind_address(port: int) -> String:
	return "%s:%d" % [LOCAL_SERVER_HOST, port]


static func local_game_server_url(port: int) -> String:
	return "ws://%s:%d" % [LOCAL_SERVER_HOST, port]
