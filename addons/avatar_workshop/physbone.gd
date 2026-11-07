extends RefCounted

## VRChat's PhysBone step (VRC.Dynamics), for chains whose joints carry the converter's extras "physbone": whole 60 Hz
## steps with the anchor moved part way for each, and a part step for time left over that only shows.

const STEP := 1.0 / 60.0
const MOST_STEPS := 6.0
const EPSILON := 1.1920929e-7

enum Limit { NONE, ANGLE, HINGE, POLAR }

## One bone of a chain: its node, its rest, its PhysBone settings and limit, and its tail's state.
class Bone:
	## Steady motion of the chain's anchor carries its tails along and only a change in that motion swings them, at the
	## share immobile leaves felt; VRChat lets velocity alone swing them.
	static var accelerations_only := true

	var node: Variant
	## Where the next node rests, in this one's axes, and that direction.
	var tail: Vector3
	var along: Vector3
	var resting: Quaternion
	var scale: Vector3
	## The bone's rest turn and scale, the tail where it rests in its parent's axes, and the way back.
	var rest_basis: Basis
	var rest_tail: Vector3
	var rest_inverse: Basis
	var rest_translation: Vector3
	## The world's down in the bone's rest axes, which gravity falloff weighs the bone's turn against.
	var down: Vector3
	var first: bool
	var advanced: bool
	var pull: float
	var spring: float
	var stiffness: float
	var gravity: float
	var falloff: float
	var immobile: float
	var squish: float
	var stretch: float
	var stretch_motion: float
	var radius: Vector2
	var endpoint: bool
	var limit := Limit.NONE
	var limit_x: float
	var limit_z: float
	var limit_rotation: Quaternion
	var rotation: Quaternion
	var translation: Vector3
	var now: Vector3
	var velocity: Vector3
	## The tail's direction and velocity at the last whole step, beside the motion of its carrying frame it felt then.
	var vector: Vector3
	var felt: Vector3
	## The tail in the immobile frame at the last whole step.
	var anchored: Vector3
	var length: float
	## Where the step leaves the bone in the world.
	var world: Transform3D

	func _init(at: Variant, rest: Transform3D, to: Vector3, settings: Dictionary, bound: Dictionary, model: Basis) -> void:
		node = at
		tail = to
		along = to.normalized()
		rebase(rest)
		down = _normalized_or(model.inverse() * Vector3.DOWN, Vector3.DOWN)
		first = settings.first
		advanced = settings.advanced
		pull = settings.pull
		spring = settings.spring
		stiffness = settings.stiffness
		gravity = settings.gravity
		falloff = settings.falloff
		immobile = settings.immobile
		squish = settings.squish
		stretch = settings.stretch
		stretch_motion = settings.stretch_motion
		radius = settings.radius
		endpoint = settings.endpoint
		if not bound.is_empty():
			limit = {"angle": Limit.ANGLE, "hinge": Limit.HINGE, "polar": Limit.POLAR}[bound.shape]
			limit_x = bound.x
			limit_z = bound.z
			limit_rotation = bound.rotation

	## Sets the pose the bone swings about, its transform in its parent's axes.
	func rebase(rest: Transform3D) -> void:
		var turned := rest.basis.get_rotation_quaternion()
		down = (turned.inverse() * resting) * down
		resting = turned
		scale = rest.basis.get_scale()
		rest_basis = Basis(resting).scaled_local(scale)
		rest_tail = rest_basis * tail
		rest_inverse = rest_basis.inverse()
		rest_translation = rest.origin

	func local(parent: Transform3D, turned: Quaternion) -> Transform3D:
		return parent * Transform3D(Basis(turned).scaled_local(scale), translation)

	## How long the bone shows: an endpoint's squish and stretch never show, as it is no transform in VRChat.
	func shown() -> float:
		return tail.length() if endpoint else length

	## One VRChat PhysBone step of `bone` hanging from `parent`, its tail's state kept in it, leaving its turn and frame in it.
	static func swing(bone: Bone, parent: Transform3D, immobile: Transform3D, carrier: Transform3D, ratio: float, whole: bool, touching: Array) -> void:
		var frame := parent.basis
		var begin := parent * bone.translation
		var current := frame * Basis(bone.rotation).scaled_local(bone.scale)
		var reach := current * (bone.along * bone.length)
		var length := reach.length()
		if length < 8e-6:
			bone.world = Transform3D(current, begin)
			return
		var scale := maxf(current.x.length(), maxf(current.y.length(), current.z.length()))
		var target := begin + frame * bone.rest_tail
		var rest := maxf((target - begin).length(), 8e-6)
		var start := bone.now
		var held := (immobile * bone.anchored - start) * bone.immobile
		var felt := carrier * start - start - held if accelerations_only else Vector3.ZERO
		var end := start + held + felt
		var from := end
		var velocity := bone.velocity + bone.felt - felt
		var vector := carrier.basis * bone.vector if accelerations_only else bone.vector
		var weight := 1.0
		if bone.falloff > 0.0:
			weight = 1.0 - bone.falloff + bone.falloff * minf(1.0 - _normalized_or(current * bone.down, Vector3.DOWN).dot(Vector3.DOWN), 1.0)
		var pull := bone.pull
		var gravity := bone.gravity
		var step: Vector3
		if bone.first:
			var restore := (rest - length) * pull * 0.5
			length += restore
			step = (end - begin).normalized() * restore
			if bone.advanced:
				step += velocity * bone.spring
				step += (target - (end + step)) * pull
				if gravity != 0.0:
					step += Vector3.DOWN * gravity * length * weight
				step += vector * bone.stiffness
			else:
				step += (target - end) * pull
				if gravity != 0.0:
					var toward := vector.normalized()
					var axis := toward.cross(Vector3.DOWN).normalized()
					var across := Quaternion(axis, PI / 2.0) * toward if axis != Vector3.ZERO else toward
					step += across * gravity * length * weight * across.dot(Vector3.DOWN)
				step = step.lerp(velocity, minf(0.99 * bone.spring, 1.0))
				if whole:
					bone.velocity = step
			step = begin + (end + step - begin).normalized() * length - end
		else:
			if gravity != 0.0:
				var pulled := Vector3.DOWN * signf(gravity) * rest
				target = begin + (target - begin).lerp(pulled, absf(gravity) * weight).normalized() * rest
			if bone.advanced:
				step = velocity * bone.spring
				step += (target - (end + step)) * pull
				if bone.stiffness > 0.0:
					step += (vector - (end + step - begin)) * bone.stiffness
			else:
				step = ((target - end) * pull).lerp(velocity, minf(0.99 * bone.spring, 1.0))
				if whole:
					bone.velocity = step
			if bone.stretch_motion < 1.0:
				var toward := end + step - begin
				var reached := toward.length()
				var was := vector.length()
				var kept := clampf(reached, was, rest) if was < rest else clampf(reached, rest, was)
				step = begin + toward.normalized() * (kept + (reached - kept) * bone.stretch_motion) - end
		end += step
		var shortest := rest * bone.squish
		var fallback := _clamp_length(end - begin, shortest, rest * bone.stretch, reach)
		# A collider may squish the bone but, as in VRChat, never stretch it past the length it reached this step.
		var reached := begin + fallback
		if not touching.is_empty():
			var pressed := [reached, shortest, fallback.length(), fallback, bone.radius * scale]
			var hit := false
			for placed: Array in touching:
				hit = _press(placed, begin, pressed) or hit
			if hit and not bone.advanced and whole:
				bone.velocity *= 0.75
			reached = pressed[0]
		var stretched := (reached - begin).length()
		var blend := ratio / (1.0 + (ratio - 1.0) * pull) if ratio > 0.0 else 0.0
		end = begin + (from.lerp(reached, blend) - begin).normalized() * stretched
		bone.length = bone.tail.length() * (stretched / rest)
		var axis := _normalized_or(bone.rest_inverse * (frame.inverse() * (end - begin)), bone.along)
		if bone.limit != Limit.NONE:
			axis = keep(bone, axis)
		bone.rotation = (bone.resting * turn(bone.along, axis)).normalized()
		bone.world = Transform3D(frame * Basis(bone.rotation).scaled_local(bone.scale), begin)
		if whole:
			end = bone.world * (bone.along * bone.length)
			bone.anchored = immobile.affine_inverse() * end
			bone.now = end
			bone.vector = end - begin
			bone.felt = felt
			if bone.advanced:
				bone.velocity = end - from


	## Moves `pressed`'s end ([end, shortest, longest, fallback, radii]) out of `placed`, or inside it, as VRChat does: a bone
	## with a radius at its start is a tapered capsule from there.
	static func _press(placed: Array, begin: Vector3, pressed: Array) -> bool:
		var end: Vector3 = pressed[0]
		var radius: Vector2 = pressed[4]
		var moved := end
		if placed.size() == 2:
			var normal: Vector3 = placed[1]
			var depth := normal.dot(placed[0] - end) + radius.y
			if depth <= 0.0:
				return false
			moved = end + normal * depth
		else:
			var center: Vector3 = placed[0]
			var span: Vector3 = placed[1] - center
			var nearest := center + span * clampf((end - center).dot(span) / maxf(span.length_squared(), 1.175494e-38), 0.0, 1.0)
			var reach: float = placed[2]
			if placed[3]:
				var depth := (end - nearest).length() + radius.y - reach
				if depth <= 0.0:
					return false
				moved = end + (nearest - end).normalized() * depth
			elif radius.y > 0.0:
				var at := end
				var size := radius.y
				var bone := end - begin
				if radius.x > 0.0 and bone.length_squared() > 0.0:
					var clear := 1.0 - maxf((bone.length() - radius.x) / bone.length(), 0.0)
					var share := maxf(clampf((nearest - begin).dot(bone) / bone.length_squared(), 0.0, 1.0), clear)
					at = begin.lerp(end, share)
					size = radius.x + (radius.y - radius.x) * share
				var gap := (at - nearest).length() - (size + reach)
				if gap >= 0.0:
					return false
				moved = end + (nearest - at).normalized() * gap
			else:
				return false
		pressed[0] = begin + _clamp_length(moved - begin, pressed[1], pressed[2], pressed[3])
		return true


	## `vector` lengthened or shortened into `shortest..longest`, or `fallback` at the shortest if it has none.
	static func _clamp_length(vector: Vector3, shortest: float, longest: float, fallback: Vector3) -> Vector3:
		shortest = maxf(shortest, 1e-5)
		var length := vector.length()
		if length <= EPSILON:
			return fallback.normalized() * shortest
		return vector * (clampf(length, shortest, maxf(longest, shortest)) / length)


	## The shortest turn from one direction to another, however small: rounding small turns to none would hold a bone that
	## gravity or a slow pull moves in smaller steps at rest forever.
	static func turn(from: Vector3, to: Vector3) -> Quaternion:
		var axis := from.cross(to)
		var sine := axis.length()
		if sine <= EPSILON * absf(from.dot(to)):
			return Quaternion.IDENTITY if from.dot(to) >= 0.0 else Quaternion(_orthogonal(from), PI)
		return Quaternion(axis / sine, atan2(sine, from.dot(to)))


	## `toward`, a direction in `bone`'s rest axes, brought back inside its limit: within limit_x of the axis every way
	## round (angle), in the plane across the limit's own X (hinge), or limit_x round toward its Z and limit_z over toward
	## its X, each on its own as VRChat clamps them (polar).
	static func keep(bone: Bone, toward: Vector3) -> Vector3:
		var up := bone.limit_rotation * Vector3.UP
		var side := bone.limit_rotation * Vector3.RIGHT
		var front := side.cross(up)
		var x := toward.dot(side)
		var y := toward.dot(up)
		var z := toward.dot(front)
		if bone.limit == Limit.POLAR:
			var local := _normalized_or(Vector3(x, y, z), Vector3.UP)
			var around := clampf(atan2(local.z, local.y), -bone.limit_x, bone.limit_x)
			var over := clampf(asin(clampf(local.x, -1.0, 1.0)), -bone.limit_z, bone.limit_z)
			return up * cos(over) * cos(around) + side * sin(over) + front * cos(over) * sin(around)
		var hinge := bone.limit == Limit.HINGE
		if hinge:
			x = 0.0
		var across := Vector3(x, 0.0, z)
		var swung := atan2(across.length(), y)
		if swung <= bone.limit_x and not hinge:
			return toward
		swung = minf(swung, bone.limit_x)
		across = across.normalized()
		return up * cos(swung) + (side * across.x + front * across.z) * sin(swung)


	static func _normalized_or(value: Vector3, fallback: Vector3) -> Vector3:
		var length := value.length()
		return value / length if length > 0.0 and is_finite(1.0 / length) else fallback


	## A unit vector at right angles to the unit `value`, by the construction glam's any_orthonormal_vector uses.
	static func _orthogonal(value: Vector3) -> Vector3:
		var sign := 1.0 if value.z >= 0.0 else -1.0
		var a := -1.0 / (sign + value.z)
		var b := value.x * value.y * a
		return Vector3(b, sign + value.y * value.y * a, -value.y)


## A PhysBone chain: its bones root first, the node its root hangs from, and the PhysBone root's parent it is anchored to.
class Chain:
	var bones: Array[Bone] = []
	var parent: Variant
	var anchor: Variant
	## Immobile holds the tails to the figure's travel through the world, not to every motion of the anchor.
	var travel: bool
	var seeded := false
	var last_position: Vector3
	var last_rotation: Quaternion
	var place: Vector3
	## The node the last bone's tail reaches, if any, and its rest.
	var end: Variant
	var end_rest: Transform3D
	## Each bone's tail in the world where the last step left it.
	var tails := PackedVector3Array()

	## Starts the next step afresh from wherever the bones then lie, so a released grip or a reset sets off without the
	## motion it had.
	func unseed() -> void:
		seeded = false
		if end != null:
			end.transform = end_rest

	## Steps by `executions` of STEP, a whole step for each whole one and then a part step that only shows, and poses
	## the bones. `place_now` is the figure's origin, which immobile "world" holds the tails to; `touching` is every
	## placed shape the bones meet.
	func advance(executions: float, place_now: Vector3, touching: Array) -> void:
		var now: Transform3D = anchor.global_transform
		var position := now.origin
		var rotation := now.basis.get_rotation_quaternion()
		var shape := Transform3D(Basis(rotation), position).affine_inverse() * now
		var fresh := not seeded
		if fresh:
			last_position = position
			last_rotation = rotation
			place = place_now
			seeded = true
			for bone in bones:
				bone.rotation = bone.node.transform.basis.get_rotation_quaternion()
				bone.translation = bone.rest_translation
				bone.length = bone.tail.length()
		var from_position := last_position
		var from_rotation := last_rotation
		var from_place := place
		var steps := maxi(ceili(executions), 1)
		var before := Transform3D(Basis(from_rotation), from_position) * shape
		var into: Transform3D = now.affine_inverse() * parent.global_transform
		bones[0].translation = bones[0].node.transform.origin
		for step in steps:
			var through := 1.0 if steps == 1 else minf(step + 1.0, executions) / executions
			var ratio := executions if steps == 1 else minf(executions - step, 1.0)
			var held_position := from_position.lerp(position, through)
			var held_rotation := from_rotation.slerp(rotation, through)
			var anchored := Transform3D(Basis(held_rotation), held_position) * shape
			var immobile := Transform3D(Basis.IDENTITY, from_place.lerp(place_now, through)) if travel else anchored
			var carrier := anchored * before.affine_inverse()
			var whole := ratio >= 1.0
			var frame := anchored * into
			for i in bones.size():
				var bone := bones[i]
				if fresh:
					bone.world = bone.local(frame, bone.rotation)
					var tip := bone.world * bone.tail
					bone.now = tip
					bone.vector = tip - bone.world.origin
					bone.velocity = Vector3.ZERO
					bone.felt = Vector3.ZERO
					bone.anchored = immobile.affine_inverse() * tip
				else:
					Bone.swing(bone, frame, immobile, carrier, ratio, whole, touching)
				frame = bone.world
				if i + 1 < bones.size():
					bones[i + 1].translation = bone.along * bone.shown()
			if whole:
				last_position = held_position
				last_rotation = held_rotation
			before = anchored
		if executions >= 1.0:
			place = place_now
		tails.resize(bones.size())
		for i in bones.size():
			var bone := bones[i]
			bone.node.transform = Transform3D(Basis(bone.rotation).scaled_local(bone.scale), bone.translation)
			tails[i] = bone.world * (bone.along * bone.shown())
		if end != null:
			end.transform = Transform3D(end_rest.basis, bones[-1].along * bones[-1].shown())


## A sphere, or with `far` beyond `center` a capsule, of radius `reach`, keeping bones outside it or, `inside`, within.
static func sphere(center: Vector3, far: Vector3, reach: float, inside: bool) -> Array:
	return [center, far, reach, inside]


## A plane through `point` that keeps bones on the side `normal` points to.
static func plane(point: Vector3, normal: Vector3) -> Array:
	return [point, normal]
