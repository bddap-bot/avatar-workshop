extends SceneTree

## Synthetic PhysBone chains stepped by the spring runtime: each check holds to something VRChat's PhysBone step does and
## a VRM spring does not.

const SpringDefinition = preload("res://addons/avatar_workshop/spring_definition.gd")
const SpringRuntime = preload("res://addons/avatar_workshop/spring_runtime.gd")
const SkeletonBone = preload("res://addons/avatar_workshop/skeleton_bone.gd")
var failures := 0


func check(ok: bool, label: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", label)


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_native_never_takes_a_physbone()
	_limit_holds_the_swing()
	_immobile_holds_the_tail_to_its_anchor()
	_steady_motion_leaves_the_tail_at_rest()
	_collider_squishes_but_never_stretches()
	_release_sets_off_from_the_grip()
	print("physbone: ", "ok" if failures == 0 else str(failures) + " failures")
	quit(0 if failures == 0 else 1)


static func settings(overrides := {}) -> Dictionary:
	var physbone := {"version": "1.1", "integration": "simplified", "pull": 0.2, "spring": 0.2, "stiffness": 0.2, "gravity": 0.0, "gravityFalloff": 0.0, "immobile": 0.0, "immobileType": "allMotion", "maxSquish": 0.0, "maxStretch": 0.0, "stretchMotion": 0.0, "radius": [0.0, 0.0], "depth": 0}
	physbone.merge(overrides, true)
	return physbone


## One bone 0.1 long pointing along +Z from an anchor at the origin, its PhysBone `physbone`, and `extras` beside it;
## [runtime, anchor, bone, tail], the anchor node index 2 for colliders to name.
func strand(physbone: Dictionary, extras := {}, springs := {}) -> Array:
	var anchor := Node3D.new()
	root.add_child(anchor)
	var bone := Node3D.new()
	anchor.add_child(bone)
	var tail := Node3D.new()
	tail.position = Vector3(0, 0, 0.1)
	bone.add_child(tail)
	extras["physbone"] = physbone
	springs.merge({"springs": [{"joints": [{"node": 0, "extras": extras}, {"node": 1}], "colliderGroups": [0] if springs.has("colliders") else []}], "colliderGroups": [{"colliders": [0]}] if springs.has("colliders") else []})
	var definition := SpringDefinition.read({"nodes": [{}, {}, {}], "extensions": {"VRMC_springBone": springs}})
	return [SpringRuntime.new(definition, {0: bone, 1: tail, 2: anchor}), anchor, bone, tail]


func _native_never_takes_a_physbone() -> void:
	var skeleton := Skeleton3D.new()
	root.add_child(skeleton)
	skeleton.add_bone("root")
	skeleton.add_bone("bone")
	skeleton.add_bone("tail")
	skeleton.set_bone_parent(1, 0)
	skeleton.set_bone_parent(2, 1)
	skeleton.set_bone_rest(2, Transform3D(Basis.IDENTITY, Vector3(0, 0, 0.1)))
	skeleton.reset_bone_poses()
	var definition := SpringDefinition.read({"nodes": [{}, {}], "extensions": {"VRMC_springBone": {"springs": [{"joints": [{"node": 0, "extras": {"physbone": settings({"gravity": 1.0})}}, {"node": 1}]}]}}})
	var springs := SpringRuntime.new(definition, {0: SkeletonBone.new(skeleton, 1), 1: SkeletonBone.new(skeleton, 2)})
	check(springs.chains[0].native == null, "a PhysBone chain steps by the PhysBone step, never the native VRM solver")
	for i in 60:
		springs.advance(1.0 / 60.0)
	var tip: Vector3 = skeleton.get_bone_global_pose(2).origin
	check(tip.y < -0.05, "a PhysBone's gravity bends the bone down: %s" % tip)
	skeleton.free()


func _limit_holds_the_swing() -> void:
	var made := strand(settings({"gravity": 1.0}), {"limit": {"shape": "angle", "x": 30.0, "z": 0.0, "rotation": [0, 0, 0, 1]}})
	for i in 120:
		made[0].advance(1.0 / 60.0)
	var angle := rad_to_deg(Vector3.BACK.angle_to(made[3].global_position - made[2].global_position))
	check(absf(angle - 30.0) < 0.5, "full gravity holds the bone at its 30 degree limit: %.2f" % angle)
	made[1].free()


func _immobile_holds_the_tail_to_its_anchor() -> void:
	var made := strand(settings({"immobile": 1.0, "spring": 0.8}))
	var most := 0.0
	for i in 90:
		made[1].position.x += 0.05 * sin(i * 0.4)
		made[0].advance(1.0 / 60.0)
		most = maxf(most, (made[1].global_transform.affine_inverse() * made[3].global_position).distance_to(Vector3(0, 0, 0.1)))
	check(most < 1e-4, "immobile 1 keeps the tail where its anchor carries it: %.6f m off" % most)
	made[1].free()


## Bones wobble with acceleration, not velocity, even where VRChat lets velocity swing them.
func _steady_motion_leaves_the_tail_at_rest() -> void:
	var made := strand(settings())
	var anchor: Node3D = made[1]
	var off := func() -> float: return (anchor.global_transform.affine_inverse() * made[3].global_position).distance_to(Vector3(0, 0, 0.1))
	for i in 30:
		made[0].advance(1.0 / 60.0)
	anchor.position.x = 0.02
	made[0].advance(1.0 / 60.0)
	var steady := 0.0
	for i in 60:
		anchor.position.x += 0.02
		made[0].advance(1.0 / 60.0)
		if i >= 30:
			steady = maxf(steady, off.call())
	check(steady < 1e-4, "a steady 1.2 m/s leaves the tail at rest: %.6f m off" % steady)
	var stopped := 0.0
	for i in 30:
		made[0].advance(1.0 / 60.0)
		stopped = maxf(stopped, off.call())
	check(stopped > 0.003, "stopping swings it: %.4f m" % stopped)
	made[1].free()


func _collider_squishes_but_never_stretches() -> void:
	var collider := {"node": 2, "shape": {"sphere": {"offset": [0, 0, 0.11], "radius": 0.02}}}
	var made := strand(settings({"maxSquish": 0.5, "maxStretch": 1.0, "radius": [0.0, 0.01]}), {}, {"colliders": [collider]})
	for i in 60:
		made[0].advance(1.0 / 60.0)
	var length: float = made[3].position.length()
	check(length > 0.075 and length < 0.085, "a sphere pressing the tip along the bone squishes it to 0.08: %.4f" % length)
	made[0].colliders[0].offset = Vector3(0, 0, 0.3)
	made[0].colliders[0].tail = Vector3(0, 0, 0.3)
	for i in 60:
		made[0].advance(1.0 / 60.0)
	length = made[3].position.length()
	check(absf(length - 0.1) < 1e-3, "and it springs back to its rest length: %.4f" % length)
	made[0].colliders[0].offset = Vector3(0, 0, 0.09)
	made[0].colliders[0].tail = Vector3(0, 0, 0.09)
	var longest := 0.0
	for i in 60:
		made[0].advance(1.0 / 60.0)
		longest = maxf(longest, made[3].position.length())
	check(longest < 0.1 + 1e-4, "a sphere pressing the tip outward never stretches the bone: %.4f" % longest)
	made[1].free()


func _release_sets_off_from_the_grip() -> void:
	var made := strand(settings({"gravity": 0.5}))
	for i in 30:
		made[0].advance(1.0 / 60.0)
	made[0].targets[0] = Vector3(0.06, 0.0, 0.08)
	for i in 30:
		made[0].advance(1.0 / 60.0)
	var held: Vector3 = made[0].tip(0)
	check(held.x > 0.04, "a grip pulls the tip: %s" % held)
	made[0].targets.erase(0)
	made[0].advance(1.0 / 60.0)
	var released: Vector3 = made[0].tip(0)
	check(released.distance_to(held) < 0.01, "release sets off from the grip, not from where the grip began: %.4f m" % released.distance_to(held))
	made[1].free()
