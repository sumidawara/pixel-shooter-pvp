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


## 同じLANの人から見える自分のアドレス。見つからなければ空。
##
## ループバックは外す。それを名乗ると、一覧を見ている人自身のマシンへ
## 案内することになり、押しても繋がらない行が並ぶ。
##
## 複数のインターフェース（有線・無線・仮想）があるので、プライベートIPv4を
## 優先する。仮想ブリッジやVPNのアドレスが先に来ることがあり、順番任せにすると
## 同じLANの人から届かないものを名乗ることがある。
static func local_network_host() -> String:
	return pick_host(IP.get_local_addresses())


## 候補の中から名乗るアドレスを1つ選ぶ。
##
## 実際のインターフェースに依存しない形で切り出してある。この選び方こそが
## 壊れやすい部分で、機械が動かせる入力で確かめられないと意味がない。
static func pick_host(addresses: Array) -> String:
	var best := ""
	var best_rank := 99
	for address in addresses:
		var text := str(address)
		if not _is_usable_ipv4(text):
			continue
		var rank := _address_rank(text)
		if rank < best_rank:
			best = text
			best_rank = rank
	return best


## 家庭やオフィスのLANらしさの順位。小さいほど優先。
##
## 172.16-31 はプライベートだが、Dockerやpodmanの既定ブリッジ（172.17〜）も
## ここに入る。アドレスだけでは仮想ブリッジと本物のLANを区別できないので、
## 実際に使われることの多い 192.168 と 10 を上に置き、こちらは最後に回す。
##
## 順位付けはあくまで当て推量である。当たらない環境のために、設定画面から
## 手で指定できるようにしてある。
static func _address_rank(address: String) -> int:
	if address.begins_with("192.168."):
		return 0
	if address.begins_with("10."):
		return 1
	if _is_private_ipv4(address):
		return 2
	return 3


static func _is_usable_ipv4(address: String) -> bool:
	if address.count(".") != 3 or address.contains(":"):
		return false
	return not address.begins_with("127.")


static func _is_private_ipv4(address: String) -> bool:
	if address.begins_with("192.168.") or address.begins_with("10."):
		return not address.begins_with("127.")
	if not address.begins_with("172."):
		return false
	var second := address.split(".")[1].to_int()
	return second >= 16 and second <= 31
