extends SceneTree

const Definition = preload("res://addons/avatar_workshop/spring_definition.gd")
const Springs = preload("res://addons/avatar_workshop/spring_runtime.gd")
var failures := 0


func _initialize() -> void:
	_run.call_deferred()


func check(ok: bool, label: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", label)


func _run() -> void:
	var nodes := [{"children": [1]}, {"children": [2]}, {}]
	var modern := {"nodes": nodes, "extensions": {"VRMC_springBone": {"colliders": [{"node": 0, "shape": {"capsule": {"offset": [0, 0, 0], "tail": [0, 1, 0], "radius": 0.2}}}], "colliderGroups": [{"colliders": [0]}], "springs": [{"center": 0, "colliderGroups": [0], "joints": [{"node": 1}, {"node": 2}]}]}}}
	var parsed := Definition.read(modern)
	check(parsed.springs.size() == 1 and parsed.springs[0].joints.size() == 2, "VRM 1 chain")
	check(parsed.colliders[0].tail == Vector3.UP and parsed.springs[0].colliders == [0], "capsule and group")
	var legacy := {"nodes": nodes, "extensions": {"VRM": {"secondaryAnimation": {"boneGroups": [{"bones": [1], "stiffiness": 2.0, "gravityDir": {"x": 0, "y": 0, "z": 1}, "gravityPower": 0.5}], "colliderGroups": [{"node": 0, "colliders": [{"offset": {"x": 0, "y": 0, "z": 1}, "radius": 0.2}]}]}}}}
	var old := Definition.read(legacy)
	check(old.springs[0].joints.size() == 2 and old.springs[0].joints[0].stiffness == 2.0, "legacy descendants and spelling")
	check(old.springs[0].joints[0].gravity == Vector3(0, 0, -0.5) and old.colliders[0].offset.z == -1.0, "legacy coordinate conversion")
	var branched := legacy.duplicate(true)
	branched.nodes = [{}, {"children": [2, 3]}, {}, {}]
	var forked := Definition.read(branched)
	var driven := []
	for chain: Dictionary in forked.springs:
		for joint: Dictionary in chain.joints:
			check(not joint.node in driven, "forked joints are driven once")
			driven.append(joint.node)
	check(driven.size() == 3, "legacy branches retain every joint")
	check(Definition.read({"nodes": null, "extensions": [0]}).springs.is_empty(), "malformed fields")
	var model := Node3D.new()
	root.add_child(model)
	var first := Node3D.new()
	model.add_child(first)
	var second := Node3D.new()
	first.add_child(second)
	second.position = Vector3(0, -0.2, 0)
	var definition := {"springs": [{"joints": [Definition.joint_data(1, {"stiffness": 1.0}, false), Definition.joint_data(2, {"stiffness": 1.0}, false)], "center": -1, "colliders": []}], "colliders": []}
	var simulation := Springs.new(definition, {1: first, 2: second})
	var start := simulation.tip(0)
	model.position.x = 0.1
	simulation.advance(1.0 / 60.0)
	check(simulation.tip(0).x < 0.099, "motion creates lag")
	for i in 300:
		simulation.advance(1.0 / 60.0)
	check(simulation.tip(0).distance_to(start + model.position) < 0.015, "chain settles")
	simulation.targets[0] = Vector3(0.25, -0.1, 0)
	for i in 60:
		simulation.advance(1.0 / 60.0)
	check(simulation.tip(0).distance_to(simulation.targets[0]) < 0.01, "tip held at reachable target")
	simulation.targets.clear()
	for i in 300:
		simulation.advance(1.0 / 60.0)
	check(simulation.tip(0).distance_to(start + model.position) < 0.015, "elastic release settles")
	var collider := Node3D.new()
	model.add_child(collider)
	collider.position = Vector3(0.03, -0.19, 0)
	var shape := {"node": collider, "offset": Vector3.ZERO, "tail": Vector3.ZERO, "radius": 0.06}
	simulation.colliders = [shape]
	simulation.chains[0].colliders = [0]
	for i in 120:
		simulation.advance(1.0 / 60.0)
	check(second.global_position.distance_to(collider.global_position) >= 0.059, "sphere excludes the chain joint")
	shape.tail = Vector3(0, -0.1, 0)
	for i in 120:
		simulation.advance(1.0 / 60.0)
	var capsule_point := Geometry3D.get_closest_point_to_segment(simulation.tip(0), collider.global_position, collider.global_position + shape.tail)
	check(simulation.tip(0).distance_to(capsule_point) >= 0.059, "capsule excludes the tip")
	model.scale = Vector3.ONE * 2.0
	simulation.chains[0].joints[0].radius = 0.02
	simulation.reset()
	for i in 120:
		simulation.advance(1.0 / 60.0)
	var nearest := Geometry3D.get_closest_point_to_segment(second.global_position, collider.global_position, collider.global_transform * shape.tail)
	check(second.global_position.distance_to(nearest) >= 0.159, "joint hit radius follows avatar scale")
	var state := simulation.state()
	check(simulation.apply_state(state), "snapshot round trip")
	check(not simulation.apply_state(PackedVector3Array([Vector3(NAN, 0, 0)])), "bad snapshot refused")
	model.free()
	print("springs: ", "ok" if failures == 0 else str(failures) + " failures")
	quit(0 if failures == 0 else 1)
