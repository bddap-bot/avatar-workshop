extends SceneTree

const Definition = preload("res://addons/avatar_workshop/spring_definition.gd")
const Springs = preload("res://addons/avatar_workshop/spring_runtime.gd")
const SkeletonBone = preload("res://addons/avatar_workshop/skeleton_bone.gd")
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
	check(parsed.springs[0].collide == Definition.WEARER | Definition.OTHERS and parsed.springs[0].grab == Definition.WEARER | Definition.OTHERS, "a spring without extras is everyone's to touch")
	var guarded := modern.duplicate(true)
	guarded.extensions.VRMC_springBone.springs[0]["extras"] = {"collide": ["others"], "grab": []}
	var kept: Dictionary = Definition.read(guarded).springs[0]
	check(kept.collide == Definition.OTHERS and kept.grab == 0, "extras limit whose hands push and hold a spring")
	var model := Node3D.new()
	root.add_child(model)
	var first := Node3D.new()
	model.add_child(first)
	var second := Node3D.new()
	first.add_child(second)
	second.position = Vector3(0, -0.2, 0)
	var endpoint := Springs.new(parsed, {0: model, 1: first, 2: second})
	check(endpoint.point_count == 1, "VRM 1 last joint is an endpoint, not an extra segment")
	var definition := {"springs": [{"joints": [Definition.joint_data(1, {"stiffness": 1.0}, false), Definition.joint_data(2, {"stiffness": 1.0}, false)], "center": -1, "end_length": 0.07, "colliders": []}], "colliders": []}
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
	_bones_swing_as_nodes_do(definition)
	_hands_push(definition)
	print("springs: ", "ok" if failures == 0 else str(failures) + " failures")
	quit(0 if failures == 0 else 1)


func _bones_swing_as_nodes_do(definition: Dictionary) -> void:
	var model := Node3D.new()
	root.add_child(model)
	var first := Node3D.new()
	model.add_child(first)
	first.position = Vector3(0.1, 0.3, 0)
	var second := Node3D.new()
	first.add_child(second)
	second.position = Vector3(0, -0.2, 0)
	var skeleton := Skeleton3D.new()
	model.add_child(skeleton)
	skeleton.add_bone("first")
	skeleton.add_bone("second")
	skeleton.set_bone_parent(1, 0)
	skeleton.set_bone_rest(0, first.transform)
	skeleton.set_bone_rest(1, second.transform)
	skeleton.reset_bone_poses()
	var nodes := Springs.new(definition, {1: first, 2: second})
	var handles := {1: SkeletonBone.new(skeleton, 0), 2: SkeletonBone.new(skeleton, 1)}
	check(handles[1].is_ancestor_of(handles[2]) and not handles[2].is_ancestor_of(handles[1]), "a bone is an ancestor of its descendants only")
	var bones := Springs.new(definition, handles)
	check(bones.chains.size() == 1, "skeleton bones form a chain")
	var apart := 0.0
	for i in 120:
		model.position = Vector3(0.15 * sin(i * 0.2), 0, 0.1 * cos(i * 0.3))
		nodes.advance(1.0 / 60.0)
		bones.advance(1.0 / 60.0)
		apart = maxf(apart, nodes.tip(0).distance_to(bones.tip(0)))
	check(apart < 1e-4, "skeleton bones swing as nodes do (%f apart)" % apart)
	check(skeleton.get_bone_global_pose(1).origin.distance_to(second.transform.origin + first.transform.origin) > 0.0 and handles[2].global_position.distance_to(second.global_position) < 1e-4, "the skeleton holds the swung pose")
	model.free()


func _hands_push(definition: Dictionary) -> void:
	var model := Node3D.new()
	root.add_child(model)
	var first := Node3D.new()
	model.add_child(first)
	var second := Node3D.new()
	first.add_child(second)
	second.position = Vector3(0, -0.2, 0)
	var simulation := Springs.new(definition, {1: first, 2: second})
	var rest := second.global_position
	simulation.hands = [[rest + Vector3(0.02, 0, 0), 0.05, Definition.OTHERS]]
	for i in 30:
		simulation.advance(1.0 / 60.0)
	check(second.global_position.distance_to(simulation.hands[0][0]) >= 0.049, "a hand pushes the joint it pokes")
	check(simulation.contacts > 0, "a poke counts as a contact")
	simulation.chains[0].collide = Definition.WEARER
	simulation.reset()
	for i in 30:
		simulation.advance(1.0 / 60.0)
	check(second.global_position.distance_to(rest) < 0.001, "a hand the chain does not collide with passes through")
	simulation.chains[0].grab = Definition.WEARER
	check(simulation.grabs(0, Definition.WEARER) and not simulation.grabs(0, Definition.OTHERS), "grab bits say whose hand may hold a chain")
	model.free()
