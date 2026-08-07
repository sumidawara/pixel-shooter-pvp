extends SceneTree

## 生成済みスプライトが実際に見える状態かを検証する。
##
## アセットはAsepriteの原本から書き出すが、書き出しに失敗してもPNG自体は
## 生成されるため、透明なまま気付かずコミットされうる。実際に
## lalokinpoppos.png が alpha=7/255（ほぼ完全に透明）で入っていた。
## 形も色も正しく、ゲームも落ちないので、画面を見るまで分からなかった。
##
##     godot --headless --path frontend --script res://tests/sprite_assets_test.gd

const MANIFEST_PATH := "res://assets/aseprite-assets.json"

## 「見えている」とみなす最低のalpha。
## 半分でも不透明な画素が1つも無いスプライトは、書き出しが壊れていると判断する。
## ふちがぼけた絵でも中心は不透明になるため、この閾値なら誤検出しない。
const MINIMUM_VISIBLE_ALPHA := 128

var _failures: PackedStringArray = PackedStringArray()


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	var file := FileAccess.open(MANIFEST_PATH, FileAccess.READ)
	if file == null:
		push_error("sprite assets: %s を読めない" % MANIFEST_PATH)
		quit(1)
		return
	var manifest = JSON.parse_string(file.get_as_text())
	if typeof(manifest) != TYPE_DICTIONARY or typeof(manifest.get("assets")) != TYPE_ARRAY:
		push_error("sprite assets: マニフェストの形式が不正")
		quit(1)
		return

	var assets: Array = manifest["assets"]
	if assets.is_empty():
		push_error("sprite assets: マニフェストにアセットが1件も無い")
		quit(1)
		return

	for asset in assets:
		_check_asset("res://assets/" + str(asset.get("output", "")))

	if not _failures.is_empty():
		push_error(
			"sprite assets: 書き出しが壊れているスプライトがある:\n  "
			+ "\n  ".join(_failures)
			+ "\nAsepriteの原本から make assets-build で書き出し直すこと"
		)
		quit(1)
		return

	print("sprite assets: %d 件すべてに不透明な画素があった" % assets.size())
	quit(0)


func _check_asset(path: String) -> void:
	if not ResourceLoader.exists(path):
		_failures.append("%s: 生成物が無い" % path)
		return
	var texture: Texture2D = load(path)
	if texture == null:
		_failures.append("%s: テクスチャとして読み込めない" % path)
		return
	var image := texture.get_image()
	if image == null:
		_failures.append("%s: 画像を取り出せない" % path)
		return
	if image.is_compressed():
		image.decompress()
	image.convert(Image.FORMAT_RGBA8)

	# get_pixelを1画素ずつ呼ぶと大きな絵で遅いため、生バイトを直接見る。
	var data := image.get_data()
	var largest_alpha := 0
	var index := 3
	while index < data.size():
		largest_alpha = maxi(largest_alpha, data[index])
		if largest_alpha >= 255:
			break
		index += 4

	if largest_alpha < MINIMUM_VISIBLE_ALPHA:
		_failures.append(
			"%s: 最大alphaが %d しかない（%dx%d）。ほぼ透明で画面に出ない"
			% [path, largest_alpha, image.get_width(), image.get_height()]
		)
