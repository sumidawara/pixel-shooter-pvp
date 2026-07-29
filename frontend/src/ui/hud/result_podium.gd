class_name ResultPodium
extends Control

## 試合終了時のポイント順位を表彰台で表示する。
## 上位3人が台に乗り、4位は台の横に立つ。

## 台の外に立つ順位を含めた表示枠の数。
const SLOT_COUNT := 4

@onready var podium_group: Control = %PodiumGroup
@onready var slots: Array = [%First, %Second, %Third, %Fourth]


func apply(players: Array, colors: Dictionary) -> void:
	var ranking := build_ranking(players)
	for index in range(slots.size()):
		if index < ranking.size():
			var entry: Dictionary = ranking[index]
			slots[index].apply_entry(entry, colors.get(int(entry["id"]), Color.WHITE))
		else:
			slots[index].clear()
	podium_group.position.x = group_offset(ranking.size())


## ポイント降順に並べ、同点は同じ順位番号にする。
## 同点でも並び順を固定するため、ポイントが同じ場合はIDの小さい方を先にする。
static func build_ranking(players: Array) -> Array:
	var entries: Array = []
	for player in players:
		entries.append({
			"id": int(player.get("id", 0)),
			"name": str(player.get("name", "PLAYER")),
			"score": int(player.get("score", 0)),
			"is_cpu": bool(player.get("is_cpu", false)),
		})
	entries.sort_custom(func(left, right):
		if left["score"] != right["score"]:
			return left["score"] > right["score"]
		return left["id"] < right["id"])

	var rank := 0
	for index in range(entries.size()):
		if index == 0 or entries[index]["score"] != entries[index - 1]["score"]:
			rank = index + 1
		entries[index]["rank"] = rank
	return entries


## 使う枠が減っても表彰台が画面中央に収まるよう、横方向のずれを返す。
static func group_offset(count: int) -> float:
	if count >= 4:
		return 0.0
	if count == 2:
		return 115.0
	return 65.0
