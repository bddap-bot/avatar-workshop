extends SceneTree
const Springs = preload("res://addons/avatar_workshop/spring_runtime.gd")
const Bone = preload("res://addons/avatar_workshop/skeleton_bone.gd")
const Definition = preload("res://addons/avatar_workshop/spring_definition.gd")
var failures := 0
func check(ok: bool, label: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", label)
func _initialize() -> void:
	_run.call_deferred()
func tick(skeleton: Skeleton3D, simulation: RefCounted) -> void:
	skeleton.reset_physics_interpolation()
	simulation.advance(1.0 / 60.0)
	skeleton.advance(1.0 / 60.0)
	skeleton.notification(Skeleton3D.NOTIFICATION_UPDATE_SKELETON)
func _run() -> void:
	var skeleton := Skeleton3D.new()
	skeleton.modifier_callback_mode_process = Skeleton3D.MODIFIER_CALLBACK_MODE_PROCESS_MANUAL
	root.add_child(skeleton)
	for name in ["root", "first", "second", "other", "other_end"]:
		skeleton.add_bone(name)
	skeleton.set_bone_parent(1, 0)
	skeleton.set_bone_parent(2, 1)
	skeleton.set_bone_rest(2, Transform3D(Basis.IDENTITY, Vector3(0, -0.2, 0)))
	skeleton.set_bone_parent(3, 0)
	skeleton.set_bone_parent(4, 3)
	skeleton.set_bone_rest(3, Transform3D(Basis.IDENTITY, Vector3(0.8, 0, 0)))
	skeleton.set_bone_rest(4, Transform3D(Basis.IDENTITY, Vector3(0, -0.2, 0)))
	skeleton.reset_bone_poses()
	var nodes := {0: Bone.new(skeleton, 0), 1: Bone.new(skeleton, 1), 2: Bone.new(skeleton, 2), 3: Bone.new(skeleton, 3), 4: Bone.new(skeleton, 4)}
	var definition := {"springs": [{"joints": [Definition.joint_data(1, {"stiffness": 1.0}, false), Definition.joint_data(2, {"stiffness": 1.0}, false)], "center": -1, "end_length": 0.07, "colliders": []}], "colliders": []}
	var other: Dictionary = definition.springs[0].duplicate(true)
	other.joints[0].node = 3
	other.joints[1].node = 4
	definition.springs.append(other)
	var simulation := Springs.new(definition, nodes)
	await process_frame
	await process_frame
	check(simulation._native.size() == 1 and simulation._native[0].setting_count == 2, "independent chains share one native modifier")
	tick(skeleton, simulation)
	var start := simulation.tip(0)
	skeleton.position.x = 0.1
	tick(skeleton, simulation)
	check(simulation.tip(0).x < 0.099, "native motion creates lag")
	for i in 300:
		tick(skeleton, simulation)
	check(simulation.tip(0).distance_to(start + skeleton.position) < 0.015, "native chain settles")
	skeleton.position.x += 0.05
	tick(skeleton, simulation)
	var moving_neighbor := simulation.tip(1)
	simulation.targets[0] = Vector3(0.25, -0.1, 0)
	tick(skeleton, simulation)
	check(simulation.tip(1).distance_to(moving_neighbor) < 0.03, "a moving neighbor keeps its captured state when another chain is grabbed")
	for i in 60:
		tick(skeleton, simulation)
	check(not simulation._native[0].active, "held tip uses constraints")
	check(simulation.tip(0).distance_to(simulation.targets[0]) < 0.01, "held tip reaches hand")
	check(simulation.tip(1).distance_to(start + skeleton.position + Vector3(0.8, 0, 0)) < 0.015, "unheld chain keeps its pose through shared handoff")
	var held := simulation.tip(0)
	simulation.targets.clear()
	tick(skeleton, simulation)
	check(not simulation._native[0].active and simulation.tip(0).distance_to(held) < 0.05, "release retains momentum without a rest-pose snap")
	for i in 300:
		tick(skeleton, simulation)
	check(simulation.tip(0).distance_to(start + skeleton.position) < 0.015 and simulation._native[0].active, "settled release returns to native simulation")
	var state := simulation.state()
	check(simulation.apply_state(state) and not simulation._native[0].active, "replica disables native simulation")
	check(simulation.tip(0).distance_to(state[1]) < 0.0001, "replica displays captured native tip")
	skeleton.scale = Vector3(1.3, 1.1, 1.2)
	tick(skeleton, simulation)
	check(not simulation._native[0].active, "nonuniform scale retains exact constraint math")
	skeleton.free()
	await _branches()
	await _collisions()
	await _hand_collisions()
	await _overlapping_chains()
	await _scaled_components()
	print("native springs: ", "ok" if failures == 0 else str(failures) + " failures")
	quit(0 if failures == 0 else 1)


func _collisions() -> void:
	for tail in [Vector3.ZERO, Vector3(0, -0.1, 0)]:
		var skeleton := Skeleton3D.new()
		skeleton.modifier_callback_mode_process = Skeleton3D.MODIFIER_CALLBACK_MODE_PROCESS_MANUAL
		root.add_child(skeleton)
		for name in ["root", "joint", "endpoint", "collider"]:
			skeleton.add_bone(name)
		skeleton.set_bone_parent(1, 0)
		skeleton.set_bone_parent(2, 1)
		skeleton.set_bone_parent(3, 0)
		skeleton.set_bone_rest(2, Transform3D(Basis.IDENTITY, Vector3(0, -0.2, 0)))
		skeleton.set_bone_rest(3, Transform3D(Basis.IDENTITY, Vector3(0.03, -0.19, 0)))
		skeleton.reset_bone_poses()
		var nodes := {}
		for b in 4:
			nodes[b] = Bone.new(skeleton, b)
		var definition := {"springs": [{"joints": [Definition.joint_data(1, {"stiffness": 1.0}, false), Definition.joint_data(2, {}, false)], "center": -1, "end_length": 0.0, "colliders": [0]}], "colliders": [{"node": 3, "offset": Vector3.ZERO, "tail": tail, "radius": 0.06}]}
		var simulation := Springs.new(definition, nodes)
		await process_frame
		await process_frame
		check(simulation._native[0] != null and not simulation._native[0].is_end_bone_extended(0), "VRM endpoint is not extended")
		for i in 120:
			tick(skeleton, simulation)
		var origin: Vector3 = nodes[3].global_position
		var nearest := Geometry3D.get_closest_point_to_segment(simulation.tip(0), origin, origin + tail)
		check(simulation.tip(0).distance_to(nearest) >= 0.059, "native sphere/capsule excludes endpoint")
		skeleton.free()


func _branches() -> void:
	var skeleton := Skeleton3D.new()
	skeleton.modifier_callback_mode_process = Skeleton3D.MODIFIER_CALLBACK_MODE_PROCESS_MANUAL
	root.add_child(skeleton)
	for b in 5:
		skeleton.add_bone(str(b))
		skeleton.set_bone_parent(b, [-1, 0, 1, 1, 3][b])
	skeleton.set_bone_rest(1, Transform3D(Basis.from_scale(Vector3(0.999974, 1, 1.00002)), Vector3.ZERO))
	skeleton.set_bone_rest(2, Transform3D(Basis.IDENTITY, Vector3(0, -0.2, 0)))
	skeleton.set_bone_rest(3, Transform3D(Basis.IDENTITY, Vector3(0.1, -0.1, 0)))
	skeleton.set_bone_rest(4, Transform3D(Basis.IDENTITY, Vector3(0, -0.1, 0)))
	skeleton.reset_bone_poses()
	var original := skeleton.get_bone_pose_scale(1)
	var nodes := {}
	for b in 5:
		nodes[b] = Bone.new(skeleton, b)
	var definition := {"springs": [], "colliders": []}
	for bones: Array in [[1, 2], [3, 4]]:
		definition.springs.append({"joints": [Definition.joint_data(bones[0], {"stiffness": 1.0}, false), Definition.joint_data(bones[1], {}, false)], "center": -1, "end_length": 0.0, "colliders": []})
	var simulation := Springs.new(definition, nodes)
	await process_frame
	await process_frame
	check(simulation._native.size() == 1, "branches with bind roundoff share native simulation")
	if simulation._native.is_empty():
		skeleton.free()
		return
	var posed := {}
	simulation._native[0].modification_processed.connect(func() -> void:
		posed.anchor = nodes[3].global_position
		posed.end = nodes[4].global_position)
	tick(skeleton, simulation)
	skeleton.position.x = 0.1
	for i in 300:
		tick(skeleton, simulation)
	check(simulation.tip(1).distance_to(posed.end) < 0.00001, "branch snapshot follows the parent's completed native pose")
	check(skeleton.get_bone_pose_scale(1).is_equal_approx(original), "native simulation preserves imported scale")
	simulation.targets[1] = posed.anchor + Vector3(0.05, -sqrt(0.0075), 0)
	for i in 60:
		tick(skeleton, simulation)
	check(not simulation._native[0].active and simulation.tip(1).distance_to(simulation.targets[1]) < 0.015, "held branch keeps its constraint when its parent is also simulated")
	skeleton.free()


func _hand_collisions() -> void:
	var skeleton := Skeleton3D.new()
	skeleton.modifier_callback_mode_process = Skeleton3D.MODIFIER_CALLBACK_MODE_PROCESS_MANUAL
	root.add_child(skeleton)
	for b in 3:
		skeleton.add_bone(str(b))
		skeleton.set_bone_parent(b, b - 1)
	skeleton.set_bone_rest(2, Transform3D(Basis.IDENTITY, Vector3(0, -0.2, 0)))
	skeleton.reset_bone_poses()
	var nodes := {0: Bone.new(skeleton, 0), 1: Bone.new(skeleton, 1), 2: Bone.new(skeleton, 2)}
	var definition := {"springs": [{"joints": [Definition.joint_data(1, {"stiffness": 1.0}, false), Definition.joint_data(2, {}, false)], "center": -1, "end_length": 0.0, "colliders": [], "collide": Definition.WEARER, "grab": Definition.OTHERS}], "colliders": []}
	var simulation := Springs.new(definition, nodes)
	await process_frame
	await process_frame
	var hand := Vector3(0.03, -0.19, 0)
	simulation.hands = [[hand, 0.06, Definition.WEARER]]
	for i in 120:
		tick(skeleton, simulation)
	check(simulation._native[0].active and simulation.tip(0).distance_to(hand) >= 0.059, "a permitted hand pushes the native spring")
	check(simulation.grabs(0, Definition.OTHERS) and not simulation.grabs(0, Definition.WEARER), "native chains retain their grab permissions")
	simulation.hands[0][2] = Definition.OTHERS
	for i in 300:
		tick(skeleton, simulation)
	check(simulation.tip(0).distance_to(Vector3(0, -0.2, 0)) < 0.015, "changing the hand owner updates native collision permissions")
	simulation.hands[0][2] = Definition.WEARER
	simulation.hands[0][0] = Vector3(-0.03, -0.19, 0)
	for i in 120:
		tick(skeleton, simulation)
	check(simulation.tip(0).distance_to(simulation.hands[0][0]) >= 0.059, "native hand colliders follow moving hands")
	simulation.hands.clear()
	for i in 300:
		tick(skeleton, simulation)
	check(simulation.tip(0).distance_to(Vector3(0, -0.2, 0)) < 0.015, "a departing hand removes its native collider")
	skeleton.free()


func _overlapping_chains() -> void:
	var skeleton := Skeleton3D.new()
	skeleton.modifier_callback_mode_process = Skeleton3D.MODIFIER_CALLBACK_MODE_PROCESS_MANUAL
	root.add_child(skeleton)
	for b in 3:
		skeleton.add_bone(str(b))
		skeleton.set_bone_parent(b, b - 1)
		skeleton.set_bone_rest(b, Transform3D(Basis.IDENTITY, Vector3(0, -0.2, 0)))
	skeleton.reset_bone_poses()
	var chain := {"joints": [Definition.joint_data(1, {}, false), Definition.joint_data(2, {}, false)], "center": -1, "end_length": 0.0, "colliders": []}
	var simulation := Springs.new({"springs": [chain, chain.duplicate(true)], "colliders": []}, {1: Bone.new(skeleton, 1), 2: Bone.new(skeleton, 2)})
	await process_frame
	await process_frame
	check(simulation.point_count == 2 and simulation._native[0].setting_count == 2, "overlapping components each get a native setting")
	skeleton.position.x = 0.1
	tick(skeleton, simulation)
	check(simulation.chains.all(func(c): return c.current.size() == 1 and c.current[0].is_finite()), "overlapping native chains capture finite independent state")
	check(simulation.apply_state(simulation.state()), "overlapping native chains replicate every endpoint")
	skeleton.free()


func _scaled_components() -> void:
	for coupled in [false, true]:
		var skeleton := Skeleton3D.new()
		skeleton.modifier_callback_mode_process = Skeleton3D.MODIFIER_CALLBACK_MODE_PROCESS_MANUAL
		root.add_child(skeleton)
		for b in 5:
			skeleton.add_bone(str(b))
			skeleton.set_bone_parent(b, [-1, 0, 1, 0, 3][b])
			skeleton.set_bone_rest(b, Transform3D(Basis.IDENTITY, Vector3(0, -0.2, 0)))
		skeleton.set_bone_rest(1, Transform3D(Basis.IDENTITY.scaled(Vector3(2, 1, 1)), Vector3.ZERO))
		skeleton.set_bone_rest(3, Transform3D(Basis.IDENTITY, Vector3(0.7, 0, 0)))
		skeleton.reset_bone_poses()
		var definition := {"springs": [], "colliders": [{"node": 1, "offset": Vector3.ZERO, "tail": Vector3.ZERO, "radius": 0.01}]}
		for first in [1, 3]:
			definition.springs.append({"joints": [Definition.joint_data(first, {}, false), Definition.joint_data(first + 1, {}, false)], "center": -1, "end_length": 0.0, "colliders": [0] if coupled and first == 3 else []})
		var nodes := {}
		for b in 5: nodes[b] = Bone.new(skeleton, b)
		var simulation := Springs.new(definition, nodes)
		await process_frame
		await process_frame
		check(simulation.point_count == 2, "scaled and unscaled chains both remain simulated")
		check(simulation.chains[0].native == null, "nonuniform scale uses the constraint solver")
		check((simulation.chains[1].native == null) == coupled, "only a dependency on the scaled chain requires the other chain to fall back")
		skeleton.position.x = 0.1
		for i in 10: tick(skeleton, simulation)
		check(simulation.state().size() == 2 and simulation.state()[1].is_finite(), "mixed simulation retains every replicated endpoint")
		skeleton.free()
