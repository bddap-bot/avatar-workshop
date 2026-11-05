extends SpringBoneSimulator3D

var chains: Array = []
var components: Array = []
var _collisions: Array = []


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
		_configure_chain(i)
		var kept: Array = []
		for index: int in chains[i].colliders:
			if collisions[index] != null:
				kept.append(collisions[index])
		_collisions.append(kept)
		set_collision_count(i, kept.size())
		for j in kept.size():
			set_collision_path(i, j, get_path_to(kept[j]))
		chains[i].native_setting = i
		chains[i].native_active = true
		set_chain_active(i, false)


func set_chain_active(index: int, enabled: bool) -> void:
	var chain: Dictionary = chains[index]
	if chain.native_active == enabled:
		return
	chain.native_active = enabled
	if enabled:
		_configure_chain(index)
	else:
		# A chain with one bone and no extended tail simulates no segment.
		set_extend_end_bone(index, false)
		set_end_bone(index, get_root_bone(index))


func _configure_chain(i: int) -> void:
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

