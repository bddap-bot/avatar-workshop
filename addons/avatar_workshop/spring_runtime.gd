extends RefCounted

const Definition = preload("spring_definition.gd")
const EVERYONE := Definition.WEARER | Definition.OTHERS
const SkeletonBone = preload("skeleton_bone.gd")
const PhysBone = preload("physbone.gd")

const STEP := PhysBone.STEP
## The most a hand carries a point in one step, so a snap of the hand or its tracking does not fling what it holds.
const CARRY := 0.05
## How near a hand's surface a point it carried stays held, against the float error of a point just pushed onto it.
const SKIN := 0.002

var chains: Array = []
var colliders: Array = []
var targets := {}
## World-space capsules {"key", "from", "to", "radius", "who": WEARER or OTHERS}, a sphere where the ends meet, set
## before advance; "key" tells a hand from the others across frames.
var hands: Array = []
var point_count := 0
var _owed := 0.0
var _native: Array[SpringBoneSimulator3D] = []
var _hands_before := {}
var _share := 1.0
## The PhysBone chains, parents' before their children's, so each sets off from where its parent's step left it.
var _physbones: Array = []


func _init(definition: Dictionary, nodes: Dictionary) -> void:
	for source: Dictionary in definition.colliders:
		if nodes.has(source.node):
			var collider := source.duplicate()
			collider.node = nodes[source.node]
			colliders.append(collider)
		else:
			colliders.append({})
	for source: Dictionary in unshadowed(definition.springs):
		var joints: Array = source.joints
		if joints.is_empty() or not joints.all(func(j: Dictionary) -> bool: return nodes.has(j.node)):
			continue
		if not range(joints.size() - 1).all(func(i: int) -> bool: return nodes[joints[i].node].is_ancestor_of(nodes[joints[i + 1].node])):
			continue
		var end_length: float = source.get("end_length", 0.0)
		var count := joints.size() if end_length > 0.0 else joints.size() - 1
		if count == 0:
			continue
		var chain := {"index": chains.size(), "native": null, "native_active": false, "endpoint": nodes[joints[-1].node] if count < joints.size() else null, "joints": [], "colliders": source.colliders, "collide": source.get("collide", EVERYONE), "grab": source.get("grab", EVERYONE), "center": nodes.get(source.center), "current": PackedVector3Array(), "previous": PackedVector3Array(), "reach": 0.0, "gripped": {}, "stepped_center": Transform3D.IDENTITY}
		for i in count:
			var joint: Dictionary = joints[i].duplicate()
			var node = nodes[joint.node]
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
			var scale: Vector3 = node.global_basis.get_scale().abs()
			chain.reach = maxf(chain.reach, joint.radius * maxf(scale.x, maxf(scale.y, scale.z)))
			chain.joints.append(joint)
		for joint: Dictionary in chain.joints:
			chain.reach += (joint.node.global_basis * joint.tail).length()
		chain.physbone = _physbone(chain)
		chains.append(chain)
	_physbones.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return _depth(a.joints[0].node) < _depth(b.joints[0].node))
	reset()
	_bind_native()


## `springs` without a PhysBone chain every joint of which an earlier PhysBone chain already moves: in VRChat the first
## PhysBone on a bone poses it, and a later one over the same bones changes nothing.
static func unshadowed(springs: Array) -> Array:
	var moved := {}
	var out := []
	for chain: Dictionary in springs:
		var physbone: bool = not chain.joints.is_empty() and chain.joints.all(func(j: Dictionary) -> bool: return j.has("physbone"))
		if physbone and chain.joints.all(func(j: Dictionary) -> bool: return moved.has(j.node)):
			continue
		if physbone:
			for joint: Dictionary in chain.joints:
				moved[joint.node] = true
		out.append(chain)
	return out


## `chain` for the PhysBone step when every joint of it carries the converter's "physbone" settings and hangs from the
## one before, or null; the step's anchor is the parent of the PhysBone's root, "depth" transforms above.
func _physbone(chain: Dictionary) -> PhysBone.Chain:
	var joints: Array = chain.joints
	if not joints.all(func(j: Dictionary) -> bool: return j.has("physbone")):
		return null
	var links: Array = joints.map(func(j: Dictionary): return j.node)
	if chain.endpoint != null:
		links.append(chain.endpoint)
	for i in range(1, links.size()):
		if not _same(_parent(links[i]), links[i - 1]):
			return null
	var root: Variant = joints[0].node
	for i in joints[0].physbone.depth:
		var above: Variant = _parent(root)
		if above is SkeletonBone or root is Node3D and above is Node3D:
			root = above
	var physbone := PhysBone.Chain.new()
	physbone.anchor = _parent(root)
	if physbone.anchor == null:
		return null
	for joint: Dictionary in joints:
		var model: Basis = joint.node.skeleton.get_bone_global_rest(joint.node.bone).basis if joint.node is SkeletonBone else joint.node.global_basis
		physbone.bones.append(PhysBone.Bone.new(joint.node, joint.rest, joint.tail, joint.physbone, joint.get("limit", {}), model))
	physbone.parent = _parent(joints[0].node)
	physbone.travel = joints[0].physbone.travel
	physbone.end = chain.endpoint
	physbone.end_rest = chain.endpoint.transform if chain.endpoint != null else Transform3D.IDENTITY
	_physbones.append(chain)
	return physbone


## The node `node` hangs from: a bone's parent bone, or its skeleton for a root bone.
static func _parent(node: Variant) -> Variant:
	if node is SkeletonBone:
		var bone: int = node.skeleton.get_bone_parent(node.bone)
		return SkeletonBone.new(node.skeleton, bone) if bone >= 0 else node.skeleton
	return node.get_parent() if node is Node else null


static func _same(a: Variant, b: Variant) -> bool:
	if a is SkeletonBone and b is SkeletonBone:
		return a.skeleton == b.skeleton and a.bone == b.bone
	return a == b


static func _depth(node: Variant) -> int:
	var depth := 0
	node = _parent(node)
	while node != null:
		depth += 1
		node = _parent(node)
	return depth


func reset() -> void:
	for chain: Dictionary in chains:
		var center := _center(chain)
		chain.current = PackedVector3Array()
		for joint: Dictionary in chain.joints:
			var node = joint.node
			node.transform = joint.rest
			chain.current.append(center.affine_inverse() * (node.global_transform * joint.tail))
		chain.previous = chain.current.duplicate()
		chain.gripped = {}
		chain.stepped_center = center
	_hands_before.clear()
	for chain: Dictionary in _physbones:
		chain.physbone.unseed()
	_owed = 0.0
	for simulator in _native:
		simulator.reset()


## Moves the pose `node`'s joint swings about to `rest`, its transform in its parent's axes, as VRChat swings a PhysBone
## about the pose the avatar's animator left; false when no chain swings `node`.
func set_rest(node: Variant, rest: Transform3D) -> bool:
	var found := false
	for chain: Dictionary in chains:
		for i in chain.joints.size():
			var joint: Dictionary = chain.joints[i]
			if _same(joint.node, node):
				joint.rest = rest
				if chain.physbone != null:
					chain.physbone.bones[i].rebase(rest)
				if chain.native_active:
					joint.node.transform = rest
				elif i == 0:
					# The PhysBone step takes its root's place from the node.
					joint.node.transform = Transform3D(joint.node.transform.basis, rest.origin)
				found = true
	return found


func advance(delta: float) -> void:
	for simulator in _native:
		for component: Array in simulator.components:
			var run: bool = component.all(func(chain: Dictionary) -> bool: return not targets.has(chain.index) and _unit_scale(chain) and not _within_hand(chain))
			if run == component[0].native_active:
				continue
			if run:
				if not component.all(_rested):
					continue
				for chain: Dictionary in component:
					for joint: Dictionary in chain.joints:
						joint.node.transform = joint.rest
			for chain: Dictionary in component:
				simulator.set_chain_active(chain.native_setting, run)
		simulator.active = simulator.chains.any(func(c: Dictionary) -> bool: return c.native_active)
	if delta > 0.25:
		reset()
		return
	_owed += maxf(delta, 0.0)
	# Divided, not multiplied, as VRChat does: a frame of exactly STEP makes exactly one step, not one and a sliver.
	var owed := _owed / STEP
	var executions := minf(owed, PhysBone.MOST_STEPS)
	var steps := floori(owed)
	if owed >= 1.0:
		_owed = (executions - floorf(executions)) * STEP
	_share = 1.0 / maxi(steps, 1)
	if not _physbones.is_empty():
		_advance_physbones(executions)
	for i in steps:
		_step()
	if steps > 0:
		_hands_before.clear()
		for hand: Dictionary in hands:
			_hands_before[hand.key] = [hand.from, hand.to]


func _advance_physbones(executions: float) -> void:
	var placed := {}
	for chain: Dictionary in _physbones:
		var physbone: PhysBone.Chain = chain.physbone
		if targets.has(chain.index):
			if physbone.seeded:
				physbone.unseed()
			continue
		var touching := []
		for index: int in chain.colliders:
			if not placed.has(index):
				placed[index] = _placed(colliders[index])
			if not placed[index].is_empty():
				touching.append(placed[index])
		for hand: Dictionary in hands:
			if chain.collide & hand.who:
				touching.append(PhysBone.sphere(hand.from, hand.to, hand.radius, false))
		physbone.advance(executions, _place(chain), touching)
		var inverse := _center(chain).affine_inverse()
		chain.previous = chain.current
		chain.current = PackedVector3Array()
		for tail in physbone.tails:
			chain.current.append(inverse * tail)


## `collider` placed in the world as PhysBone.advance meets it, its extended shape where it has one, or empty for a
## collider that names no node.
static func _placed(collider: Dictionary) -> Array:
	if collider.is_empty():
		return []
	var world: Transform3D = collider.node.global_transform
	var shape: Dictionary = collider.get("extended", collider)
	var at: Vector3 = world * shape.offset
	if shape.has("normal"):
		return PhysBone.plane(at, (world.basis * shape.normal).normalized())
	return PhysBone.sphere(at, world * shape.tail, shape.radius * world.basis.x.length(), shape.get("inside", false))


## The figure's origin, which a PhysBone that is immobile in the world holds its tails to.
static func _place(chain: Dictionary) -> Vector3:
	var node: Variant = chain.joints[0].node
	if node is SkeletonBone:
		return node.skeleton.global_position
	while node.get_parent() is Node3D:
		node = node.get_parent()
	return node.global_position


func _step() -> void:
	for c in chains.size():
		if chains[c].native_active or chains[c].physbone and not targets.has(c):
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
			points.append(now if chain.gripped.has(i) else now + (now - before) * (1.0 - joint.drag) + along.normalized() * joint.stiffness * STEP + joint.gravity * STEP)
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
		var keys: Array[String] = []
		keys.resize(starts.size())
		var gripped := {}
		var carried := {}
		if not targets.get(c) is Vector3:
			var now := {}
			for hand: Dictionary in hands:
				if _reaches(chain, hand):
					now[hand.key] = hand
					var segment: Vector3 = hand.to - hand.from
					starts.append(hand.from)
					segments.append(segment)
					squared.append(maxf(segment.length_squared(), 1e-12))
					extents.append(hand.radius)
					centers.append(hand.from + segment * 0.5)
					bounds.append(segment.length() * 0.5 + hand.radius)
					keys.append(hand.key)
			var moved: Transform3D = center * chain.stepped_center.affine_inverse()
			for i: int in chain.gripped:
				var key: String = chain.gripped[i]
				if now.has(key) and _hands_before.has(key):
					var was: Array = _hands_before[key]
					var carry := _carried(points[i + 1], [moved * was[0], moved * was[1]], now[key], _share).limit_length(CARRY)
					points[i + 1] += carry
					carried[i] = carry
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
					if keys[j] != "" and away.length_squared() < (extent + SKIN) * (extent + SKIN):
						gripped[i] = keys[j]
					if away.length_squared() < extent * extent:
						point = closest + _direction(away, points[i] - closest) * extent
						point = points[i] + _direction(point - points[i], Vector3.DOWN) * lengths[i]
				points[i + 1] = point
		chain.gripped = gripped
		chain.stepped_center = center
		chain.previous = current.duplicate()
		for i: int in carried:
			chain.previous[i] += inverse.basis * carried[i]
		for i in lengths.size():
			chain.current[i] = inverse * points[i + 1]
		_apply(chain, points)


## Whether a hand that may push `chain` is within its reach.
func _within_hand(chain: Dictionary) -> bool:
	for hand: Dictionary in hands:
		if _reaches(chain, hand):
			return true
	return false


## Whether `hand` may push `chain` and comes within the chain's reach of its first joint.
static func _reaches(chain: Dictionary, hand: Dictionary) -> bool:
	if not chain.collide & hand.who:
		return false
	var anchor: Vector3 = chain.joints[0].node.global_position
	var segment: Vector3 = hand.to - hand.from
	var closest: Vector3 = hand.from + segment * clampf((anchor - hand.from).dot(segment) / maxf(segment.length_squared(), 1e-12), 0.0, 1.0)
	return anchor.distance_to(closest) < chain.reach + hand.radius


## How far a hand that pushed the point at `at` carries it, `share` of its move from `before` ([from, to]) to `now`:
## as the nearest point of its axis moved, but never pulled along as the hand draws away, so what a hand catches drags
## with it until its own pull wins or the hand leaves it.
static func _carried(at: Vector3, before: Array, now: Dictionary, share: float) -> Vector3:
	var span: Vector3 = before[1] - before[0]
	var along := clampf((at - before[0]).dot(span) / maxf(span.length_squared(), 1e-12), 0.0, 1.0)
	var near: Vector3 = before[0] + span * along
	var moved: Vector3 = ((now.from as Vector3).lerp(now.to, along) - near) * share
	var normal := (at - near).normalized()
	return moved - normal * minf(moved.dot(normal), 0.0)


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
	return _center(chains[chain]) * chains[chain].current[-1]


## Whether a point of the chain rests on one of its declared colliders, whichever solver placed it.
func touches(c: int, slack: float) -> bool:
	var chain: Dictionary = chains[c]
	var center := _center(chain)
	for i in chain.joints.size():
		var joint: Dictionary = chain.joints[i]
		var scale: Vector3 = joint.node.global_basis.get_scale().abs()
		var radius: float = joint.radius * maxf(scale.x, maxf(scale.y, scale.z))
		var point: Vector3 = center * chain.current[i]
		for index: int in chain.colliders:
			var collider: Dictionary = colliders[index]
			if collider.is_empty():
				continue
			var placed: Transform3D = collider.node.global_transform
			var start: Vector3 = placed * collider.offset
			var segment: Vector3 = placed * collider.tail - start
			var closest := start + segment * clampf((point - start).dot(segment) / maxf(segment.length_squared(), 1e-12), 0.0, 1.0)
			var size: Vector3 = placed.basis.get_scale().abs()
			if point.distance_to(closest) < radius + collider.radius * maxf(size.x, maxf(size.y, size.z)) + slack:
				return true
	return false


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
		for chain: Dictionary in simulator.chains:
			simulator.set_chain_active(chain.native_setting, false)
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
		if chain.physbone:
			chain.physbone.unseed()
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
		var supported := {}
		var components := []
		for component: Array in _components(groups[skeleton], skeleton):
			if _native_supported(component, skeleton):
				components.append(component)
				for chain: Dictionary in component:
					supported[chain.index] = true
		var bound: Array = groups[skeleton].filter(func(c: Dictionary) -> bool: return supported.has(c.index))
		if bound.is_empty():
			continue
		var simulator := preload("spring_native.gd").new()
		simulator.active = false
		skeleton.add_child(simulator)
		simulator.components = components
		simulator.configure(bound, colliders)
		simulator.modification_processed.connect(_capture.bind(simulator))
		_native.append(simulator)
		for chain: Dictionary in bound:
			chain.native = simulator


## A fallback chain and everything whose poses or colliders it drives must run together.
func _components(bound: Array, skeleton: Skeleton3D) -> Array:
	var owners := {}
	var parents := PackedInt32Array(range(bound.size()))
	for c in bound.size():
		for joint: Dictionary in bound[c].joints:
			owners.get_or_add(joint.node.bone, []).append(c)
	for c in bound.size():
		var chain: Dictionary = bound[c]
		var inputs: Array = chain.joints.map(func(j: Dictionary): return j.node)
		if chain.center != null:
			inputs.append(chain.center)
		for index: int in chain.colliders:
			if not colliders[index].is_empty():
				inputs.append(colliders[index].node)
		for node in inputs:
			if not node is SkeletonBone or node.skeleton != skeleton:
				continue
			var bone: int = node.bone
			while bone >= 0:
				for other: int in owners.get(bone, []):
					parents[_component(parents, c)] = _component(parents, other)
				bone = skeleton.get_bone_parent(bone)
	var result := {}
	for c in bound.size():
		result.get_or_add(_component(parents, c), []).append(bound[c])
	return result.values()


static func _component(parents: PackedInt32Array, index: int) -> int:
	while parents[index] != index:
		index = parents[index]
	return index


func _native_supported(bound: Array, skeleton: Skeleton3D) -> bool:
	var driven := {}
	for chain: Dictionary in bound:
		for joint: Dictionary in chain.joints:
			driven[joint.node.bone] = chain.index
	for chain: Dictionary in bound:
		if chain.joints.any(func(j: Dictionary) -> bool: return j.has("physbone")):
			return false
		if not _unit_basis(skeleton.get_bone_global_rest(chain.joints[0].node.bone).basis):
			return false
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
	var basis: Basis = chain.joints[0].node.global_basis
	if chain.get("native_basis") != basis:
		chain.native_basis = basis
		chain.native_unit = _unit_basis(basis)
	return chain.native_unit


func _capture(simulator: SpringBoneSimulator3D) -> void:
	for chain: Dictionary in simulator.chains:
		if not chain.native_active:
			continue
		var skeleton: Skeleton3D = chain.joints[0].node.skeleton
		var into_center := _center(chain).affine_inverse() * skeleton.global_transform
		var current := PackedVector3Array()
		current.resize(chain.joints.size())
		for i in chain.joints.size():
			var joint: Dictionary = chain.joints[i]
			current[i] = into_center * (skeleton.get_bone_global_pose(joint.node.bone) * joint.tail)
		chain.previous = chain.current
		chain.current = current


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
