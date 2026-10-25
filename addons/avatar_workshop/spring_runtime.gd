extends RefCounted

const STEP := 1.0 / 60.0

var chains: Array = []
var colliders: Array = []
var targets := {}
var contacts := 0
var point_count := 0
var _time := 0.0


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
		if not range(joints.size() - 1).all(func(i: int) -> bool: return (nodes[joints[i].node] as Node3D).is_ancestor_of(nodes[joints[i + 1].node])):
			continue
		var chain := {"joints": [], "colliders": source.colliders, "center": nodes.get(source.center), "current": PackedVector3Array(), "previous": PackedVector3Array()}
		for i in joints.size():
			var joint: Dictionary = joints[i].duplicate()
			var node: Node3D = nodes[joint.node]
			driven[joint.node] = true
			point_count += 1
			joint.node = node
			joint.rest = node.transform
			var tail := Vector3(0, -0.07, 0)
			if i + 1 < joints.size():
				tail = node.global_transform.affine_inverse() * (nodes[joints[i + 1].node] as Node3D).global_position
			elif i > 0:
				var direction := node.global_position - (nodes[joints[i - 1].node] as Node3D).global_position
				tail = node.global_basis.inverse() * direction.normalized() * 0.07
			joint.tail = tail
			chain.joints.append(joint)
		chains.append(chain)
	reset()


func reset() -> void:
	for chain: Dictionary in chains:
		var center := _center(chain)
		chain.current = PackedVector3Array()
		for joint: Dictionary in chain.joints:
			var node: Node3D = joint.node
			node.transform = joint.rest
			chain.current.append(center.affine_inverse() * (node.global_transform * joint.tail))
		chain.previous = chain.current.duplicate()
	_time = 0.0


func advance(delta: float) -> void:
	if delta > 0.25:
		reset()
		return
	_time += maxf(delta, 0.0)
	while _time >= STEP:
		_time -= STEP
		_step()


func _step() -> void:
	for c in chains.size():
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
			var node: Node3D = joint.node
			node.transform = joint.rest
			var rest := node.global_transform
			rests.append(rest)
			if i == 0:
				points.append(rest.origin)
			var along: Vector3 = rest.basis * joint.tail
			lengths.append(along.length())
			var scale := rest.basis.get_scale().abs()
			radii.append(joint.radius * maxf(scale.x, maxf(scale.y, scale.z)))
			var now := center * current[i]
			var before := center * previous[i]
			points.append(now + (now - before) * (1.0 - joint.drag) + along.normalized() * joint.stiffness * STEP + joint.gravity * STEP)
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
				for index: int in chain.colliders:
					var collider: Dictionary = colliders[index]
					if not collider.is_empty():
						var before := points[i + 1]
						points[i + 1] = _collide(before, points[i], lengths[i], radii[i], collider)
						if before.distance_squared_to(points[i + 1]) > 1e-12:
							contacts += 1
		chain.previous = current.duplicate()
		for i in lengths.size():
			chain.current[i] = inverse * points[i + 1]
		_apply(chain, points)


static func _direction(value: Vector3, fallback: Vector3) -> Vector3:
	return value.normalized() if value.length_squared() > 1e-12 else fallback.normalized()


static func _collide(point: Vector3, origin: Vector3, length: float, radius: float, collider: Dictionary) -> Vector3:
	var transform: Transform3D = collider.node.global_transform
	var start: Vector3 = transform * collider.offset
	var end: Vector3 = transform * collider.tail
	var scale: Vector3 = transform.basis.get_scale().abs()
	var extent: float = radius + collider.radius * maxf(scale.x, maxf(scale.y, scale.z))
	var segment := end - start
	var closest := start + segment * clampf((point - start).dot(segment) / maxf(segment.length_squared(), 1e-12), 0.0, 1.0)
	var away := point - closest
	if away.length_squared() < extent * extent:
		point = closest + _direction(away, origin - closest) * extent
		point = origin + _direction(point - origin, Vector3.DOWN) * length
	return point


static func _center(chain: Dictionary) -> Transform3D:
	return chain.center.global_transform if chain.center != null else Transform3D.IDENTITY


static func _apply(chain: Dictionary, points: PackedVector3Array) -> void:
	for i in chain.joints.size():
		var joint: Dictionary = chain.joints[i]
		var node: Node3D = joint.node
		node.transform = joint.rest
		var frame := node.global_transform
		var from: Vector3 = frame.basis * joint.tail
		var to := points[i + 1] - points[i]
		if from.length_squared() > 1e-12 and to.length_squared() > 1e-12:
			frame.basis = Basis(Quaternion(from.normalized(), to.normalized())) * frame.basis
			node.global_transform = frame


func tip(chain: int) -> Vector3:
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
