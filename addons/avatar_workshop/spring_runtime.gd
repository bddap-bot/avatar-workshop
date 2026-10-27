extends RefCounted

const Definition = preload("spring_definition.gd")
const EVERYONE := Definition.WEARER | Definition.OTHERS
const SkeletonBone = preload("skeleton_bone.gd")

const STEP := 1.0 / 60.0

var chains: Array = []
var colliders: Array = []
var targets := {}
## World-space spheres [centre, radius, WEARER or OTHERS], set before advance.
var hands: Array = []
var contacts := 0
var point_count := 0
var _time := 0.0
var _native: Array[SpringBoneSimulator3D] = []


func _init(definition: Dictionary, nodes: Dictionary) -> void:
	for source: Dictionary in definition.colliders:
		if nodes.has(source.node):
			var collider := source.duplicate()
			collider.node = nodes[source.node]
			colliders.append(collider)
		else:
			colliders.append({})
	var driven := {}
	for source: Dictionary in definition.springs:
		var joints: Array = source.joints
		if joints.is_empty() or not joints.all(func(j: Dictionary) -> bool: return nodes.has(j.node) and not driven.has(j.node)):
			continue
		if not range(joints.size() - 1).all(func(i: int) -> bool: return nodes[joints[i].node].is_ancestor_of(nodes[joints[i + 1].node])):
			continue
		var end_length: float = source.get("end_length", 0.0)
		var count := joints.size() if end_length > 0.0 else joints.size() - 1
		if count == 0:
			continue
		var chain := {"index": chains.size(), "native": null, "endpoint": nodes[joints[-1].node] if count < joints.size() else null, "joints": [], "colliders": source.colliders, "collide": source.get("collide", EVERYONE), "grab": source.get("grab", EVERYONE), "center": nodes.get(source.center), "current": PackedVector3Array(), "previous": PackedVector3Array()}
		for i in count:
			var joint: Dictionary = joints[i].duplicate()
			var node = nodes[joint.node]
			driven[joint.node] = true
			point_count += 1
			joint.node = node
			joint.rest = node.transform
			var tail := Vector3(0, -end_length, 0)
			if i + 1 < joints.size():
				tail = node.global_transform.affine_inverse() * nodes[joints[i + 1].node].global_position
			elif i > 0:
				var direction: Vector3 = node.global_position - nodes[joints[i - 1].node].global_position
				tail = node.global_basis.inverse() * direction.normalized() * end_length
			joint.tail = tail
			chain.joints.append(joint)
		chains.append(chain)
	reset()
	_bind_native()


func reset() -> void:
	for chain: Dictionary in chains:
		var center := _center(chain)
		chain.current = PackedVector3Array()
		for joint: Dictionary in chain.joints:
			var node = joint.node
			node.transform = joint.rest
			chain.current.append(center.affine_inverse() * (node.global_transform * joint.tail))
		chain.previous = chain.current.duplicate()
	_time = 0.0
	for simulator in _native:
		simulator.reset()


func advance(delta: float) -> void:
	for simulator in _native:
		simulator.update_hands(hands)
		var run: bool = simulator.chains.all(func(chain: Dictionary) -> bool: return not targets.has(chain.index) and _unit_scale(chain))
		if run and not simulator.active:
			run = simulator.chains.all(_rested)
			if run:
				for chain: Dictionary in simulator.chains:
					for joint: Dictionary in chain.joints:
						joint.node.transform = joint.rest
		simulator.active = run
	if delta > 0.25:
		reset()
		return
	_time += maxf(delta, 0.0)
	while _time >= STEP:
		_time -= STEP
		_step()


func _step() -> void:
	for c in chains.size():
		if chains[c].native != null and chains[c].native.active:
			continue
		var chain: Dictionary = chains[c]
		var center := _center(chain)
		var inverse := center.affine_inverse()
		var current: PackedVector3Array = chain.current
		var previous: PackedVector3Array = chain.previous
		var points := PackedVector3Array()
		var lengths := PackedFloat32Array()
		var radii := PackedFloat32Array()
		var rests: Array[Transform3D] = []
		for i in chain.joints.size():
			var joint: Dictionary = chain.joints[i]
			var node = joint.node
			node.transform = joint.rest
			var rest: Transform3D = node.global_transform
			rests.append(rest)
			if i == 0:
				points.append(rest.origin)
			var along: Vector3 = rest.basis * joint.tail
			lengths.append(along.length())
			var scale: Vector3 = rest.basis.get_scale().abs()
			radii.append(joint.radius * maxf(scale.x, maxf(scale.y, scale.z)))
			var now := center * current[i]
			var before := center * previous[i]
			points.append(now + (now - before) * (1.0 - joint.drag) + along.normalized() * joint.stiffness * STEP + joint.gravity * STEP)
		# Poses stay fixed until _apply; prepare collision geometry once per chain.
		var starts := PackedVector3Array()
		var segments := PackedVector3Array()
		var squared := PackedFloat32Array()
		var extents := PackedFloat32Array()
		var centers := PackedVector3Array()
		var bounds := PackedFloat32Array()
		for index: int in chain.colliders:
			var collider: Dictionary = colliders[index]
			if collider.is_empty():
				continue
			var placed: Transform3D = collider.node.global_transform
			var start: Vector3 = placed * collider.offset
			var segment: Vector3 = placed * collider.tail - start
			var scale: Vector3 = placed.basis.get_scale().abs()
			var extent: float = collider.radius * maxf(scale.x, maxf(scale.y, scale.z))
			starts.append(start)
			segments.append(segment)
			squared.append(maxf(segment.length_squared(), 1e-12))
			extents.append(extent)
			centers.append(start + segment * 0.5)
			bounds.append(segment.length() * 0.5 + extent)
		if not targets.get(c) is Vector3:
			for hand: Array in hands:
				if chain.collide & hand[2]:
					starts.append(hand[0])
					segments.append(Vector3.ZERO)
					squared.append(1e-12)
					extents.append(hand[1])
					centers.append(hand[0])
					bounds.append(hand[1])
		var anchor := points[0]
		var held: Variant = targets.get(c)
		if held is Vector3:
			var reach := 0.0
			for length in lengths:
				reach += length
			held = anchor + (held - anchor).limit_length(reach)
		for iteration in 12 if held is Vector3 else 3:
			if held is Vector3:
				points[-1] = held
				for i in range(lengths.size() - 1, -1, -1):
					points[i] = points[i + 1] + _direction(points[i] - points[i + 1], -(rests[i].basis * chain.joints[i].tail)) * lengths[i]
			points[0] = anchor
			for i in lengths.size():
				points[i + 1] = points[i] + _direction(points[i + 1] - points[i], rests[i].basis * chain.joints[i].tail) * lengths[i]
				var point := points[i + 1]
				for j in starts.size():
					# A capsule fits inside this sphere: rejection preserves exact contacts.
					var bound := radii[i] + bounds[j]
					if point.distance_squared_to(centers[j]) >= bound * bound:
						continue
					var closest := starts[j] + segments[j] * clampf((point - starts[j]).dot(segments[j]) / squared[j], 0.0, 1.0)
					var away := point - closest
					var extent := radii[i] + extents[j]
					if away.length_squared() < extent * extent:
						var before := point
						point = closest + _direction(away, points[i] - closest) * extent
						point = points[i] + _direction(point - points[i], Vector3.DOWN) * lengths[i]
						if before.distance_squared_to(point) > 1e-12:
							contacts += 1
				points[i + 1] = point
		chain.previous = current.duplicate()
		for i in lengths.size():
			chain.current[i] = inverse * points[i + 1]
		_apply(chain, points)


static func _direction(value: Vector3, fallback: Vector3) -> Vector3:
	return value.normalized() if value.length_squared() > 1e-12 else fallback.normalized()


static func _center(chain: Dictionary) -> Transform3D:
	return chain.center.global_transform if chain.center != null else Transform3D.IDENTITY


static func _apply(chain: Dictionary, points: PackedVector3Array) -> void:
	for i in chain.joints.size():
		var joint: Dictionary = chain.joints[i]
		var node = joint.node
		node.transform = joint.rest
		var frame: Transform3D = node.global_transform
		var from: Vector3 = frame.basis * joint.tail
		var to := points[i + 1] - points[i]
		if from.length_squared() > 1e-12 and to.length_squared() > 1e-12:
			frame.basis = Basis(Quaternion(from.normalized(), to.normalized())) * frame.basis
			node.global_transform = frame


func grabs(chain: int, who: int) -> bool:
	return chains[chain].grab & who != 0


func tip(chain: int) -> Vector3:
	if chains[chain].native != null and chains[chain].native.active:
		return _center(chains[chain]) * chains[chain].current[-1]
	var joint: Dictionary = chains[chain].joints[-1]
	return joint.node.global_transform * joint.tail


func state() -> PackedVector3Array:
	var result := PackedVector3Array()
	for chain: Dictionary in chains:
		var center := _center(chain)
		for point: Vector3 in chain.current:
			result.append(center * point)
	return result


func apply_state(points: PackedVector3Array) -> bool:
	if points.size() != point_count:
		return false
	for point in points:
		if not point.is_finite():
			return false
	for simulator in _native:
		simulator.active = false
	var at := 0
	for chain: Dictionary in chains:
		var placed := PackedVector3Array([chain.joints[0].node.global_position])
		var inverse := _center(chain).affine_inverse()
		for i in chain.joints.size():
			placed.append(points[at])
			chain.current[i] = inverse * points[at]
			at += 1
		_apply(chain, placed)
	return true


func _bind_native() -> void:
	var groups := {}
	for chain: Dictionary in chains:
		if not chain.joints[0].node is SkeletonBone:
			continue
		var skeleton: Skeleton3D = chain.joints[0].node.skeleton
		if not groups.has(skeleton):
			groups[skeleton] = []
		groups[skeleton].append(chain)
	for skeleton: Skeleton3D in groups:
		if not _native_supported(groups[skeleton], skeleton):
			continue
		var simulator := preload("spring_native.gd").new()
		simulator.active = false
		skeleton.add_child(simulator)
		simulator.configure(groups[skeleton], colliders)
		simulator.modification_processed.connect(_capture.bind(simulator))
		_native.append(simulator)
		for chain: Dictionary in groups[skeleton]:
			chain.native = simulator


func _native_supported(bound: Array, skeleton: Skeleton3D) -> bool:
	var driven := {}
	for chain: Dictionary in bound:
		for joint: Dictionary in chain.joints:
			driven[joint.node.bone] = chain.index
	for chain: Dictionary in bound:
		if not chain.joints.all(func(j: Dictionary) -> bool: return _unit_basis(j.rest.basis)):
			return false
		for i in range(1, chain.joints.size()):
			if skeleton.get_bone_parent(chain.joints[i].node.bone) != chain.joints[i - 1].node.bone:
				return false
		if chain.endpoint != null and skeleton.get_bone_parent(chain.endpoint.bone) != chain.joints[-1].node.bone:
			return false
		var inputs: Array = []
		if chain.center != null:
			inputs.append(chain.center)
		for index: int in chain.colliders:
			if not colliders[index].is_empty():
				inputs.append(colliders[index].node)
		if not inputs.all(func(n) -> bool: return n is SkeletonBone and n.skeleton == skeleton):
			return false
		# Native colliders are synchronized before the group, not between its chains.
		for node in inputs:
			var b: int = node.bone
			while b >= 0:
				if driven.has(b) and driven[b] != chain.index:
					return false
				b = skeleton.get_bone_parent(b)
	return true


static func _unit_basis(basis: Basis) -> bool:
	if basis.determinant() <= 0.0:
		return false
	# Imported bind transforms carry roundoff at a few parts in 100,000.
	var unit := basis.orthonormalized()
	return basis.x.distance_squared_to(unit.x) < 1e-8 and basis.y.distance_squared_to(unit.y) < 1e-8 and basis.z.distance_squared_to(unit.z) < 1e-8


static func _unit_scale(chain: Dictionary) -> bool:
	return _unit_basis(chain.joints[0].node.global_basis)


func _capture(simulator: SpringBoneSimulator3D) -> void:
	for chain: Dictionary in simulator.chains:
		var inverse := _center(chain).affine_inverse()
		chain.previous = chain.current.duplicate()
		for i in chain.joints.size():
			var joint: Dictionary = chain.joints[i]
			chain.current[i] = inverse * (joint.node.global_transform * joint.tail)


static func _rested(chain: Dictionary) -> bool:
	# Native simulation cannot import velocity: hand it a settled chain, never a released grip.
	var first = chain.joints[0].node
	var skeleton: Skeleton3D = first.skeleton
	var parent: int = skeleton.get_bone_parent(first.bone)
	var frame := skeleton.global_transform
	if parent >= 0:
		frame *= skeleton.get_bone_global_pose(parent)
	var center := _center(chain)
	for i in chain.joints.size():
		var joint: Dictionary = chain.joints[i]
		frame *= joint.rest
		var now: Vector3 = center * chain.current[i]
		if now.distance_squared_to(frame * joint.tail) > 1e-6 or now.distance_squared_to(center * chain.previous[i]) > 1e-6:
			return false
	return true
