extends SceneTree

## Aseprite解析そのものの検証。
##
## インポータはAsepriteの実行ファイルを使わず、GDScriptで形式を解釈している。
## つまり「Asepriteが出す絵」との一致は、こちらが自力で保たねばならない。
## 実アセットの既知の内容と、未対応機能に対する挙動を固定する。
##
##     godot --headless --path frontend --script res://tests/aseprite_document_test.gd

const AsepriteDocument := preload("res://addons/aseprite_importer/aseprite_document.gd")

## 移行前にAseprite CLIが書き出していたPNGから採取した実測値。
## 移行時に、インポート結果が旧PNGとアルファ・可視色ともに
## 画素単位で一致することを確認している。
const EXPECTED := {
	"res://assets/aseprite/actors/player/player_stand.aseprite":
		{"w": 32, "h": 32, "opaque": 526},
	"res://assets/aseprite/actors/player/player_run.aseprite":
		{"w": 128, "h": 32, "opaque": 2089},
	"res://assets/aseprite/actors/lalokinpoppos/lalokinpoppos.aseprite":
		{"w": 32, "h": 23, "opaque": 506},
	"res://assets/aseprite/effects/sparkle.aseprite":
		{"w": 15, "h": 5, "opaque": 15},
	"res://assets/aseprite/ui/menu/cursor.aseprite":
		{"w": 24, "h": 24, "opaque": 204},
}

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	_check_known_assets()
	_check_rejects_broken_input()
	_check_rejects_unsupported_features()

	if not _failures.is_empty():
		push_error("aseprite document:\n  " + "\n  ".join(_failures))
		quit(1)
		return
	print("aseprite document: 解析結果と未対応機能の扱いが期待どおりだった")
	quit(0)


## 実アセットが既知の寸法と不透明画素数で読めること。
func _check_known_assets() -> void:
	for path in EXPECTED:
		var result = AsepriteDocument.load_from_file(path)
		if not result.error.is_empty():
			_failures.append("%s: %s" % [path, result.error])
			continue
		var expected: Dictionary = EXPECTED[path]
		var image: Image = result.image
		if image.get_width() != expected.w or image.get_height() != expected.h:
			_failures.append("%s: 寸法が %dx%d（期待 %dx%d）"
				% [path, image.get_width(), image.get_height(), expected.w, expected.h])
			continue
		var opaque := 0
		var data := image.get_data()
		var index := 3
		while index < data.size():
			if data[index] > 0:
				opaque += 1
			index += 4
		if opaque != expected.opaque:
			_failures.append("%s: 不透明画素が %d（期待 %d）" % [path, opaque, expected.opaque])


## 壊れた入力を「読めた」ことにしないこと。
func _check_rejects_broken_input() -> void:
	var empty := AsepriteDocument.parse(PackedByteArray())
	if empty.error.is_empty():
		_failures.append("空のデータを受け入れてしまった")

	var not_aseprite := PackedByteArray()
	not_aseprite.resize(256)
	var wrong_magic = AsepriteDocument.parse(not_aseprite)
	if wrong_magic.error.is_empty():
		_failures.append("Asepriteでないデータを受け入れてしまった")


## 未対応の機能に出会ったら、違う絵を出さずにエラーにすること。
##
## 静かに間違えると画面を見るまで気付けない。ここは黙って通してはいけない。
func _check_rejects_unsupported_features() -> void:
	var file := FileAccess.open(
		"res://assets/aseprite/actors/player/player_stand.aseprite", FileAccess.READ)
	if file == null:
		_failures.append("検査用の原本を開けない")
		return
	var original := file.get_buffer(file.get_length())
	file.close()

	# 色深度をインデックスカラー(8bit)へ書き換える。
	var indexed := original.duplicate()
	indexed.encode_u16(12, 8)
	var indexed_result = AsepriteDocument.parse(indexed)
	if indexed_result.error.is_empty():
		_failures.append("RGBA以外の色深度を黙って読んでしまった")

	# 最初のレイヤーのブレンドモードをMultiply(1)へ書き換える。
	var multiply := original.duplicate()
	var position := AsepriteDocument.HEADER_SIZE
	var chunk_position := position + AsepriteDocument.FRAME_HEADER_SIZE
	var patched := false
	while chunk_position + 6 < multiply.size():
		var chunk_size := multiply.decode_u32(chunk_position)
		if chunk_size < 6:
			break
		if multiply.decode_u16(chunk_position + 4) == AsepriteDocument.CHUNK_LAYER:
			multiply.encode_u16(chunk_position + 6 + 10, 1)
			patched = true
			break
		chunk_position += chunk_size
	if not patched:
		_failures.append("検査用にレイヤーチャンクを書き換えられなかった")
		return
	var multiply_result = AsepriteDocument.parse(multiply)
	if multiply_result.error.is_empty():
		_failures.append("Normal以外のブレンドモードを黙って読んでしまった")
