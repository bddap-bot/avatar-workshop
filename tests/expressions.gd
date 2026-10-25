extends SceneTree

const Mixer = preload("res://addons/avatar_workshop/expression_mixer.gd")
var failures := 0


func check(ok: bool, label: String) -> void:
	if not ok:
		printerr("FAIL: " + label)
		failures += 1


func _init() -> void:
	var bindings := [[["face", 0, 0.75], ["face", 1, 1.0]], [["face", 0, 0.5]], [["eyes", 0, 1.0]]]
	var result := Mixer.mix([1.0, 1.0, 0.25], bindings)
	check(result[["face", 0]] == 1.0, "overlapping channels saturate after accumulation")
	check(result[["face", 1]] == 1.0, "separate shape on same mesh")
	check(result[["eyes", 0]] == 0.25, "same shape index on separate mesh")
	result = Mixer.mix([0.0, 0.0, 0.0], bindings)
	check(result.size() == 3 and result.values().all(func(v: float) -> bool: return v == 0.0), "neutral resets previously driven targets")
	check(Mixer.mix([], []).is_empty(), "empty avatar")
	check(Mixer.mix([-1.0], [[["face", 0, 1.0]]])[["face", 0]] == 0.0, "negative inputs cannot produce negative morphs")
	check(bindings[0][0][2] == 0.75, "binding data remains reusable")
	print("expressions: %s (7 checks)" % ("ok" if failures == 0 else "FAILED"))
	quit(0 if failures == 0 else 1)
