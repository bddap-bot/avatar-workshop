extends SceneTree

const AvatarScene = preload("res://addons/avatar_workshop/avatar_scene.gd")
const FIXTURES := "res://avatar-core/fixtures/"
var failures := 0
var checks := 0


func check(ok: bool, label: String) -> void:
	checks += 1
	if not ok:
		printerr("FAIL: " + label)
		failures += 1


func near(a: Transform3D, b: Transform3D, by := 1e-5) -> bool:
	return a.origin.distance_to(b.origin) < by and (a.basis.x - b.basis.x).length() < by and (a.basis.y - b.basis.y).length() < by and (a.basis.z - b.basis.z).length() < by


func read(file: String) -> Variant:
	return AvatarModel.read(FileAccess.get_file_as_bytes(FIXTURES + file))


func _init() -> void:
	check(AvatarModel.read(PackedByteArray([1, 2, 3])) is String, "a file that is no glTF binary is refused with a reason")
	var plain := GLTFDocument.new()
	var gstate := GLTFState.new()
	var cube := MeshInstance3D.new()
	cube.mesh = BoxMesh.new()
	plain.append_from_scene(cube, gstate)
	cube.free()
	var refused: Variant = AvatarModel.read(plain.generate_buffer(gstate))
	check(refused is String and not (refused as String).is_empty(), "a glTF binary with neither AW_avatar nor VRM is refused with a reason")
	for file in ["synthetic.glb", "unity-synthetic.glb", "unity-human.glb", "vrm/crimsom-1.0.vrm", "vrm/crimsom-0.x.vrm"]:
		var model: Variant = read(file)
		check(model is AvatarModel, "%s reads: %s" % [file, model])
		if not model is AvatarModel:
			continue
		var opened := AvatarScene.open(model)
		check(not opened.has("error"), "%s opens: %s" % [file, opened.get("error")])
		if opened.has("error"):
			continue
		var root: Node3D = opened["root"]
		var ev: AvatarEvaluator = opened["evaluator"]
		var state: GLTFState = opened["state"]
		for f in 30:
			ev.evaluate(0.0 if f == 0 else 1.0 / 60.0, Transform3D.IDENTITY, [])
		ev.apply()
		var nodes := state.get_nodes()
		var off := 0
		for i in nodes.size():
			var n: GLTFNode = nodes[i]
			if n.skeleton < 0:
				continue
			var sk: Skeleton3D = state.get_skeletons()[n.skeleton].get_godot_skeleton()
			if not near(sk.get_bone_pose(sk.find_bone(n.resource_name)), ev.local(i)):
				off += 1
		check(off == 0, "%s: every bone is posed as the core's output (%d off)" % [file, off])
		var json: Dictionary = JSON.parse_string(model.model())
		var shown := 0
		for r in json.get("renderers", []).size():
			shown += 1 if ev.shown(r) else 0
		check(json.get("renderers", []).is_empty() or shown > 0, "%s: renderers are shown" % file)
		var humanoid: Dictionary = json.get("humanoid", {}).get("bones", {})
		if humanoid.has("LeftHand"):
			var hand := int(humanoid["LeftHand"]["node"])
			var at := Transform3D(Basis(Vector3.UP, 0.5), Vector3(0.3, 1.1, 0.2))
			ev.pose(hand, at)
			ev.evaluate(1.0 / 60.0, Transform3D.IDENTITY, [])
			check(near(ev.space(hand), at.scaled_local(ev.space(hand).basis.get_scale()), 1e-4), "%s: a posed hand stands where the host put it" % file)
			var turned := Transform3D(Basis(Vector3.UP, PI / 2).scaled_local(Vector3(1, 1, 0.5)), Vector3(0.1, 0.2, 0.3))
			ev.pose(hand, turned)
			ev.evaluate(1.0 / 60.0, Transform3D(Basis(Vector3.UP, 0.3).scaled_local(Vector3(-1, 1, 1)), Vector3.ZERO), [])
			check(near(ev.space(hand), turned, 1e-4), "%s: a non-uniformly scaled pose survives the boundary under a mirrored root" % file)
		ev.pose(-1, Transform3D.IDENTITY)
		check(not ev.grab(-1, 0, Vector3.ZERO, false), "%s: a grab of no PhysBone is refused" % file)
		ev.evaluate(1.0 / 60.0, Transform3D.IDENTITY, [])
		var other: Variant = AvatarEvaluator.create(model)
		other.evaluate(1.0 / 60.0, Transform3D(Basis.IDENTITY, Vector3(0.2, 0, 0)), [ev.hands(1)])
		check(other.hands(2) is AvatarHands, "%s: one avatar's hands reach another's frame" % file)
		check(ev.materials() is Array, "%s: material properties read" % file)
		root.free()
	var task := WorkerThreadPool.add_task(func() -> void:
		var m: Variant = read("vrm/crimsom-1.0.vrm")
		var o := AvatarScene.open(m)
		check(o.has("root"), "a model reads and opens off the main thread")
		if o.has("root"):
			(o["root"] as Node).free())
	WorkerThreadPool.wait_for_task_completion(task)
	print("core: %s (%d checks)" % ["ok" if failures == 0 else "FAILED", checks])
	quit(0 if failures == 0 else 1)
