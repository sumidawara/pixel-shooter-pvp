@tool
extends RefCounted

## Asepriteファイル(.aseprite / .ase)を読み、1枚のImageへ合成する。
##
## Asepriteの実行ファイルを呼ばず、GDScriptだけで完結させる。これにより
## 原本の.asepriteだけをGitで管理でき、書き出し済みPNGを持たなくてよくなる。
## 「原本とPNGのどちらが正しいのか」という状態が構造的に発生しない。
##
## 対応しているのはこのプロジェクトが使う範囲に限る。想定外の機能に出会ったら、
## 黙って違う絵を出さずにエラーを返す。書き出しの誤りは画面を見るまで
## 気付けないため、静かに間違えるより止まるほうがよい。
##
## 形式: https://github.com/aseprite/aseprite/blob/main/docs/ase-file-specs.md

const MAGIC_FILE := 0xA5E0
const MAGIC_FRAME := 0xF1FA
const HEADER_SIZE := 128
const FRAME_HEADER_SIZE := 16

const CHUNK_LAYER := 0x2004
const CHUNK_CEL := 0x2005

const CEL_RAW := 0
const CEL_LINKED := 1
const CEL_COMPRESSED_IMAGE := 2

const BLEND_NORMAL := 0
const LAYER_FLAG_VISIBLE := 1

## 解析結果。`error`が空文字なら成功。
class Result:
	var image: Image
	var frame_count: int
	var error: String

	func _init(result_image: Image, frames: int, message: String) -> void:
		image = result_image
		frame_count = frames
		error = message


static func _fail(message: String) -> Result:
	return Result.new(null, 0, message)


## ファイルを読み込み、全フレームを横一列に並べた1枚のImageを返す。
##
## 1フレームなら結果はキャンバスと同じ寸法になる。複数フレームは
## `aseprite --sheet-type horizontal` と同じ並びにする。
static func load_from_file(path: String) -> Result:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return _fail("開けない: %s" % path)
	var data := file.get_buffer(file.get_length())
	file.close()
	return parse(data, path)


static func parse(data: PackedByteArray, path: String = "") -> Result:
	var where := " (%s)" % path if not path.is_empty() else ""
	if data.size() < HEADER_SIZE:
		return _fail("ヘッダより短い%s" % where)
	if data.decode_u16(4) != MAGIC_FILE:
		return _fail("Asepriteファイルではない%s" % where)

	var frame_count := data.decode_u16(6)
	var width := data.decode_u16(8)
	var height := data.decode_u16(10)
	var depth := data.decode_u16(12)
	if frame_count <= 0 or width <= 0 or height <= 0:
		return _fail("寸法かフレーム数が不正%s" % where)
	if depth != 32:
		# 8bit(インデックス)と16bit(グレースケール)はパレット解決が必要になる。
		# 使っていないので、必要になったときに実装する。
		return _fail("RGBA(32bit)以外の色深度には未対応: %dbit%s" % [depth, where])

	var sheet := Image.create_empty(width * frame_count, height, false, Image.FORMAT_RGBA8)
	# 前フレームのセルを層ごとに覚えておく。リンクセルが参照する。
	var cels_by_frame: Array[Dictionary] = []
	var layers: Array[Dictionary] = []

	var position := HEADER_SIZE
	for frame_index in range(frame_count):
		if position + FRAME_HEADER_SIZE > data.size():
			return _fail("フレーム%dのヘッダが切れている%s" % [frame_index, where])
		var frame_size := data.decode_u32(position)
		if data.decode_u16(position + 4) != MAGIC_FRAME:
			return _fail("フレーム%dのマジックが不正%s" % [frame_index, where])
		var chunk_count := data.decode_u32(position + 12)
		if chunk_count == 0:
			chunk_count = data.decode_u16(position + 6)

		var cels: Dictionary = {}
		var chunk_position := position + FRAME_HEADER_SIZE
		for _chunk in range(chunk_count):
			if chunk_position + 6 > data.size():
				return _fail("フレーム%dのチャンクが切れている%s" % [frame_index, where])
			var chunk_size := data.decode_u32(chunk_position)
			var chunk_type := data.decode_u16(chunk_position + 4)
			if chunk_size < 6 or chunk_position + chunk_size > data.size():
				return _fail("チャンクの長さが不正%s" % where)
			var body := data.slice(chunk_position + 6, chunk_position + chunk_size)

			match chunk_type:
				CHUNK_LAYER:
					var blend := body.decode_u16(10)
					if blend != BLEND_NORMAL:
						return _fail("Normal以外のブレンドモードには未対応: %d%s" % [blend, where])
					layers.append({
						"visible": (body.decode_u16(0) & LAYER_FLAG_VISIBLE) != 0,
						"opacity": body[12],
					})
				CHUNK_CEL:
					var cel := _read_cel(body, frame_index, cels_by_frame, where)
					if cel.has("error"):
						return _fail(str(cel["error"]))
					cels[int(cel["layer"])] = cel
			chunk_position += chunk_size

		cels_by_frame.append(cels)
		var compose_error := _compose_frame(sheet, frame_index * width, cels, layers, width, height)
		if not compose_error.is_empty():
			return _fail("%s%s" % [compose_error, where])
		position += frame_size

	return Result.new(sheet, frame_count, "")


static func _read_cel(
	body: PackedByteArray,
	frame_index: int,
	cels_by_frame: Array[Dictionary],
	where: String
) -> Dictionary:
	if body.size() < 20:
		return {"error": "セルチャンクが短すぎる%s" % where}
	var layer := body.decode_u16(0)
	var x := body.decode_s16(2)
	var y := body.decode_s16(4)
	var opacity := body[6]
	var cel_type := body.decode_u16(7)

	match cel_type:
		CEL_RAW, CEL_COMPRESSED_IMAGE:
			var cel_width := body.decode_u16(16)
			var cel_height := body.decode_u16(18)
			var expected := cel_width * cel_height * 4
			var pixels: PackedByteArray
			if cel_type == CEL_RAW:
				pixels = body.slice(20, 20 + expected)
			else:
				# Asepriteはzlib形式で格納する。Godotの COMPRESSION_DEFLATE は
				# zlibヘッダ込みを期待するので、先頭2バイトは外さずに渡す。
				pixels = body.slice(20).decompress(expected, FileAccess.COMPRESSION_DEFLATE)
			if pixels.size() != expected:
				return {"error": "セルの画素数が合わない (%d != %d)%s" % [pixels.size(), expected, where]}
			return {
				"layer": layer, "x": x, "y": y, "opacity": opacity,
				"w": cel_width, "h": cel_height, "pixels": pixels,
			}
		CEL_LINKED:
			var source_frame := body.decode_u16(16)
			if source_frame >= cels_by_frame.size():
				return {"error": "リンクセルが未読のフレーム%dを参照している%s" % [source_frame, where]}
			var source: Dictionary = cels_by_frame[source_frame]
			if not source.has(layer):
				return {"error": "リンク元のフレーム%dに層%dのセルが無い%s" % [source_frame, layer, where]}
			var linked: Dictionary = (source[layer] as Dictionary).duplicate()
			# 位置と不透明度はリンク側の値を使う。
			linked["x"] = x
			linked["y"] = y
			linked["opacity"] = opacity
			return linked
		_:
			return {"error": "未対応のセル種別: %d（タイルマップ等）%s" % [cel_type, where]}


static func _compose_frame(
	sheet: Image,
	offset_x: int,
	cels: Dictionary,
	layers: Array[Dictionary],
	width: int,
	height: int
) -> String:
	# レイヤーチャンクの出現順が下から上。その順に重ねる。
	var indices := cels.keys()
	indices.sort()
	for layer_index: int in indices:
		if layer_index >= layers.size():
			return "セルが未知の層%dを参照している" % layer_index
		var layer: Dictionary = layers[layer_index]
		if not bool(layer["visible"]):
			continue
		var layer_alpha := float(layer["opacity"]) / 255.0
		var cel: Dictionary = cels[layer_index]
		var cel_alpha := float(cel["opacity"]) / 255.0
		var pixels: PackedByteArray = cel["pixels"]
		var cel_width: int = cel["w"]
		for row in range(int(cel["h"])):
			var y: int = int(cel["y"]) + row
			if y < 0 or y >= height:
				continue
			for column in range(cel_width):
				var x: int = int(cel["x"]) + column
				if x < 0 or x >= width:
					continue
				var index := (row * cel_width + column) * 4
				var alpha := pixels[index + 3] / 255.0 * cel_alpha * layer_alpha
				if alpha <= 0.0:
					continue
				var source := Color(
					pixels[index] / 255.0,
					pixels[index + 1] / 255.0,
					pixels[index + 2] / 255.0,
					alpha)
				sheet.set_pixel(
					offset_x + x, y, _over(source, sheet.get_pixel(offset_x + x, y)))
	return ""


## 通常ブレンドのsrc-over合成。
static func _over(source: Color, destination: Color) -> Color:
	var alpha := source.a + destination.a * (1.0 - source.a)
	if alpha <= 0.0:
		return Color(0.0, 0.0, 0.0, 0.0)
	return Color(
		(source.r * source.a + destination.r * destination.a * (1.0 - source.a)) / alpha,
		(source.g * source.a + destination.g * destination.a * (1.0 - source.a)) / alpha,
		(source.b * source.a + destination.b * destination.a * (1.0 - source.a)) / alpha,
		alpha)
