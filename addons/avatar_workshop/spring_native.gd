extends SpringBoneSimulator3D

var chains: Array = []
var _collisions: Array = []
var _hands: Array[SpringBoneCollisionSphere3D] = []
var _hand_masks := PackedInt32Array()


func configure(bound: Array, colliders: Array) -> void:
	chains = bound
	var collisions := {}
	for chain: Dictionary in chains:
		for index: int in chain.colliders:
			collisions[index] = null
	for index: int in collisions:
		var source: Dictionary = colliders[index]
		if source.is_empty():
			continue
		var segment: Vector3 = source.tail - source.offset
		var shape: SpringBoneCollision3D
		if segment.length_squared() > 1e-12:
			var capsule := SpringBoneCollisionCapsule3D.new()
			capsule.height = segment.length() + source.radius * 2.0
			capsule.radius = source.radius
			capsule.rotation_offset = Quaternion(Vector3.UP, segment.normalized())
			shape = capsule
		else:
			var sphere := SpringBoneCollisionSphere3D.new()
			sphere.radius = source.radius
			shape = sphere
		add_child(shape)
		shape.bone = source.node.bone
		shape.position_offset = (source.offset + source.tail) * 0.5
		collisions[index] = shape
	setting_count = chains.size()
	for i in chains.size():
		var chain: Dictionary = chains[i]
		var joints: Array = chain.joints
		set_root_bone(i, joints[0].node.bone)
		set_end_bone(i, chain.endpoint.bone if chain.endpoint != null else joints[-1].node.bone)
		set_extend_end_bone(i, chain.endpoint == null)
		set_end_bone_length(i, joints[-1].tail.length())
		set_end_bone_direction(i, BONE_DIRECTION_FROM_PARENT if joints.size() > 1 else BONE_DIRECTION_MINUS_Y)
		set_individual_config(i, true)
		for j in joints.size():
			var joint: Dictionary = joints[j]
			set_joint_drag(i, j, joint.drag)
			set_joint_stiffness(i, j, joint.stiffness)
			set_joint_gravity(i, j, joint.gravity.length())
			set_joint_gravity_direction(i, j, joint.gravity.normalized() if joint.gravity.length_squared() > 0.0 else Vector3.DOWN)
			set_joint_radius(i, j, joint.radius)
		set_center_from(i, CENTER_FROM_BONE if chain.center != null else CENTER_FROM_WORLD_ORIGIN)
		if chain.center != null:
			set_center_bone(i, chain.center.bone)
		set_enable_all_child_collisions(i, false)
		var kept: Array = []
		for index: int in chain.colliders:
			if collisions[index] != null:
				kept.append(collisions[index])
		_collisions.append(kept)
		set_collision_count(i, kept.size())
		for j in kept.size():
			set_collision_path(i, j, get_path_to(kept[j]))


func update_hands(hands: Array) -> void:
	while _hands.size() > hands.size():
		_hands.pop_back().free()
	while _hands.size() < hands.size():
		var sphere := SpringBoneCollisionSphere3D.new()
		add_child(sphere)
		_hands.append(sphere)
	var masks := PackedInt32Array()
	for i in hands.size():
		_hands[i].global_position = hands[i][0]
		_hands[i].radius = hands[i][1]
		masks.append(hands[i][2])
	if masks == _hand_masks:
		return
	_hand_masks = masks
	for i in chains.size():
		var shapes: Array = _collisions[i].duplicate()
		for j in _hands.size():
			if chains[i].collide & masks[j]:
				shapes.append(_hands[j])
		set_collision_count(i, shapes.size())
		for j in shapes.size():
			set_collision_path(i, j, get_path_to(shapes[j]))
