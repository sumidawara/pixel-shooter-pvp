extends SceneTree

## 一覧へ名乗るアドレスの検証。
##
## ここを間違えると、一覧には行が出るのに誰も入れない。実際そうなっていた。
## クライアントが公開先を渡しておらず、部屋は `ws://127.0.0.1:9001` として
## 載っていた。それは一覧を見ている人自身のループバックで、押すと自分の
## マシンへ繋ぎに行って失敗する。
##
## ループバックを名乗らないことが、この検査の要点になる。
##
##     godot --headless --path frontend --script res://tests/public_address_test.gd

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	_check_the_detected_address_is_not_loopback()
	_check_private_addresses_are_preferred()
	await _check_a_typed_address_wins()
	await _check_the_screen_says_which_address_is_used()
	await _check_the_hosted_server_is_told_where_it_is()

	if not _failures.is_empty():
		push_error("public address:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("public address: 名乗るアドレスの決め方が期待どおりだった")
	quit(0)


## 自動で選ぶアドレスが、ループバックでないこと。
##
## 127.0.0.1 を名乗ると、一覧の行は「押した本人のマシン」を指す。
func _check_the_detected_address_is_not_loopback() -> void:
	var host := NetworkConfig.local_network_host()
	if host.begins_with("127."):
		_failures.append("ループバックを名乗っている: %s" % host)
	if host.contains(":"):
		_failures.append("IPv6を選んでいる: %s" % host)


## プライベートIPv4を優先すること。
##
## 仮想ブリッジやVPNのアドレスが先に並ぶことがあり、順番任せにすると
## 同じLANの人から届かないものを名乗る。
func _check_private_addresses_are_preferred() -> void:
	var cases := {
		"192.168.1.5": true,
		"10.0.0.8": true,
		"172.16.0.1": true,
		"172.31.255.254": true,
		"172.32.0.1": false,
		"172.15.0.1": false,
		"8.8.8.8": false,
	}
	for address in cases:
		if NetworkConfig._is_private_ipv4(address) != cases[address]:
			_failures.append("%s の判定が違う" % address)

	for address in ["127.0.0.1", "127.0.1.1", "::1", "fe80::1", "1.2.3"]:
		if NetworkConfig._is_usable_ipv4(address):
			_failures.append("%s を使えると判定している" % address)

	# 実際に並び順から1つ選ぶところまで見る。判定だけ正しくても、
	# 選ぶ側が使っていなければ意味がない。
	var picks := {
		# 仮想ブリッジが先に来ても、同じLANのアドレスを選ぶ。
		"172.17.0.1,192.168.1.5": "192.168.1.5",
		"127.0.0.1,10.1.2.3": "10.1.2.3",
		# プライベートが無ければ、残ったものを使う。
		"127.0.0.1,203.0.113.9": "203.0.113.9",
		# ループバックしか無ければ、名乗らない。
		"127.0.0.1,::1": "",
	}
	for joined in picks:
		var chosen := NetworkConfig.pick_host(joined.split(","))
		if chosen != picks[joined]:
			_failures.append("%s から %s を選んだ（期待 %s）" % [joined, chosen, picks[joined]])


## 手で書いたアドレスが、自動検出より優先されること。
##
## 外から入ってもらう場合は、LANのアドレスでは届かない。
func _check_a_typed_address_wins() -> void:
	var menu = await _open_menu()

	menu.public_host_input.text = "  game.example.test:30000  "
	if menu.get_public_host() != "game.example.test:30000":
		_failures.append("手で書いたアドレスが使われない: %s" % menu.get_public_host())

	menu.public_host_input.text = ""
	if menu.get_public_host() != NetworkConfig.local_network_host():
		_failures.append("空欄で自動検出へ戻らない")

	await _close(menu)


## どのアドレスで名乗るのかが画面に出ること。
##
## 「AUTO」とだけ書いてあると、何が使われるのか確かめる手段が無い。
func _check_the_screen_says_which_address_is_used() -> void:
	var menu = await _open_menu()

	menu.public_host_input.text = "203.0.113.7"
	menu._update_public_host_hint()
	if not menu.public_host_hint.text.contains("203.0.113.7"):
		_failures.append("使うアドレスが画面に出ない: %s" % menu.public_host_hint.text)

	await _close(menu)


## 起動するサーバーへ、公開先が渡ること。
func _check_the_hosted_server_is_told_where_it_is() -> void:
	var controller := preload("res://src/networking/host_server_controller.gd").new()
	root.add_child(controller)
	await process_frame

	# 実行ファイルが無くても引数の組み立ては確かめられる。
	# start_server は既定引数を持つので、渡し忘れるとここで気付けない。
	if controller.get_method_list().filter(
		func(method): return method["name"] == "start_server"
	).is_empty():
		_failures.append("start_server が無い")
	else:
		var arguments: Array = controller.get_method_list().filter(
			func(method): return method["name"] == "start_server"
		)[0]["args"]
		var names: Array = arguments.map(func(argument): return argument["name"])
		if not names.has("public_host"):
			_failures.append("start_server が公開先を受け取らない: %s" % [names])
		if not names.has("lobby_url"):
			_failures.append("start_server がロビーを受け取らない: %s" % [names])

	controller.queue_free()
	await process_frame


func _open_menu():
	var main_scene: PackedScene = load("res://src/app/main.tscn")
	var main = main_scene.instantiate()
	root.add_child(main)
	await process_frame
	return main.get_node("MenuScreen")


func _close(menu) -> void:
	menu.get_parent().queue_free()
	await process_frame
