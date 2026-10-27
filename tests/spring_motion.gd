extends SceneTree

const Springs = preload("res://addons/avatar_workshop/spring_runtime.gd")
const Bone = preload("res://addons/avatar_workshop/skeleton_bone.gd")
const Definition = preload("res://addons/avatar_workshop/spring_definition.gd")
var failures := 0

func _initialize() -> void:
	_run.call_deferred()

func _run() -> void:
	var skeleton := Skeleton3D.new()
	root.add_child(skeleton)
	var parents := [-1, 0, 1, 2, 3, 4, 0, 0, 0, 8]
	var offsets := [Vector3.ZERO, Vector3(0.1, 0.3, 0), Vector3(0, -0.05, 0), Vector3(0, -0.1, 0), Vector3(0, -0.1, 0), Vector3(0, -0.1, 0), Vector3(0.14, 0.05, 0), Vector3(-0.1, 0.2, 0), Vector3(-0.3, 0.4, 0), Vector3(0, -0.2, 0)]
	for b in parents.size():
		skeleton.add_bone(str(b))
		skeleton.set_bone_parent(b, parents[b])
		skeleton.set_bone_rest(b, Transform3D(Basis.IDENTITY, offsets[b]))
	skeleton.reset_bone_poses()
	var definition := {"springs": [], "colliders": [{"node": 6, "offset": Vector3.ZERO, "tail": Vector3(0, -0.1, 0), "radius": 0.07}, {"node": 4, "offset": Vector3(0.1, 0, 0), "tail": Vector3(0.1, 0, 0), "radius": 0.04}]}
	for bones: Array in [[1, 3], [4, 5], [7], [8, 9]]:
		var joints: Array = []
		for b: int in bones:
			var joint := Definition.joint_data(b, {"stiffness": 1.2, "dragForce": 0.3, "hitRadius": 0.01}, false)
			joint.gravity = Vector3(0.01, -0.02, 0.005)
			joints.append(joint)
		definition.springs.append({"joints": joints, "center": 0, "end_length": 0.07, "colliders": [0, 1]})
	var nodes := {}
	for b in parents.size():
		nodes[b] = Bone.new(skeleton, b)
	var simulation := Springs.new(definition, nodes)
	# Reference trace from the original solver, including tip holds and a pause reset.
	var expected: Array = JSON.parse_string(FileAccess.get_file_as_string("res://tests/spring_motion.json"))
	var samples := []
	var greatest := 0.0
	for frame in 360:
		skeleton.position = Vector3(0.1 * sin(frame * 0.1), 0.03 * cos(frame * 0.08), 0.02 * sin(frame * 0.13))
		skeleton.scale = Vector3(1.3, 1.1, 1.2)
		skeleton.set_bone_pose_rotation(0, Quaternion(Vector3.UP, sin(frame * 0.02) * 0.2))
		if frame >= 80 and frame < 160:
			simulation.targets[0] = skeleton.position + Vector3(0.2, 0.14, 0.04)
		else:
			simulation.targets.clear()
		simulation.advance(0.3 if frame == 240 else 1.0 / 72.0)
		if frame % 6 == 0:
			var points := simulation.state()
			var row := []
			for i in points.size():
				row.append([points[i].x, points[i].y, points[i].z])
				if not expected.is_empty():
					var p: Array = expected[frame / 6][i]
					greatest = maxf(greatest, points[i].distance_to(Vector3(p[0], p[1], p[2])))
			samples.append(row)
	if expected.size() != samples.size() or greatest > 0.00001:
		failures += 1
		printerr("FAIL: moving, scaled, held and colliding chains differ by ", greatest)
	skeleton.free()
	print("spring motion: ", "ok" if failures == 0 else "failed", " (maximum distance ", greatest, " m)")
	quit(0 if failures == 0 else 1)
