extends SceneTree
const Bindings = preload("res://addons/avatar_workshop/skin_bindings.gd")
var failures := 0
var completed := 0
func check(ok: bool, label: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", label)
func _initialize() -> void:
	for influences in [4, 8]:
		_test(influences)
	check(completed == 2, "both influence layouts completed")
	print("skin bindings: ", "ok" if failures == 0 else str(failures) + " failures")
	quit(0 if failures == 0 else 1)
func _test(influences: int) -> void:
	var skin := Skin.new()
	for i in 100:
		skin.add_bind(i, Transform3D(Basis.IDENTITY, Vector3(i, 0, 0)))
	skin.set_bind_pose(50, skin.get_bind_pose(71))
	skin.set_bind_bone(93, 71)
	skin.set_bind_pose(93, skin.get_bind_pose(71))
	skin.set_bind_name(71, "bone")
	skin.set_bind_name(93, "bone")
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = PackedVector3Array([Vector3.ZERO, Vector3.RIGHT, Vector3.UP])
	arrays[Mesh.ARRAY_INDEX] = PackedInt32Array([0, 1, 2, 0, 2, 1])
	var bones := PackedInt32Array()
	var weights := PackedFloat32Array()
	for vertex in 3:
		for influence in influences:
			bones.append((50 if vertex == 1 else 71) if influence == 0 else 93 if influence == 1 else 2)
			weights.append(0.75 if influence == 0 else 0.25 if influence == 1 else 0.0)
	arrays[Mesh.ARRAY_BONES] = bones
	arrays[Mesh.ARRAY_WEIGHTS] = weights
	var mesh := ArrayMesh.new()
	mesh.add_blend_shape("move")
	var blend := []
	blend.resize(Mesh.ARRAY_MAX)
	blend[Mesh.ARRAY_VERTEX] = PackedVector3Array([Vector3.UP, Vector3.ONE, Vector3.UP * 2])
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays, [blend], {2.0: PackedInt32Array([0, 2, 1])}, Mesh.ARRAY_FLAG_USE_8_BONE_WEIGHTS if influences == 8 else 0)
	mesh.surface_set_name(0, "surface")
	mesh.surface_set_material(0, StandardMaterial3D.new())
	mesh.shadow_mesh = mesh.duplicate()
	var original := mesh.surface_get_arrays(0)
	var old_surface := RenderingServer.mesh_get_surface(mesh.get_rid(), 0)
	var result := Bindings.compact(mesh, skin)
	var compacted: ArrayMesh = result[0]
	var kept: Skin = result[1]
	var remapped := compacted.surface_get_arrays(0)
	check(kept.get_bind_count() == 2, "unused and duplicate bindings removed")
	check(kept.get_bind_name(1) == "bone" and kept.get_bind_pose(1) == skin.get_bind_pose(71), "binding identity and inverse pose preserved")
	check(mesh.surface_get_arrays(0)[Mesh.ARRAY_BONES] == original[Mesh.ARRAY_BONES] and skin.get_bind_count() == 100, "source resources untouched")
	check(compacted.get_blend_shape_name(0) == "move" and compacted.surface_get_blend_shape_arrays(0) == mesh.surface_get_blend_shape_arrays(0), "morphs preserved")
	check(compacted.surface_get_material(0) == mesh.surface_get_material(0) and compacted.surface_get_name(0) == "surface", "material and name preserved")
	var new_surface := RenderingServer.mesh_get_surface(compacted.get_rid(), 0)
	check(new_surface.lods == old_surface.lods, "LOD indices preserved")
	for channel in Mesh.ARRAY_MAX:
		if channel != Mesh.ARRAY_BONES:
			check(original[channel] == remapped[channel], "vertex channel %d preserved" % channel)
	for i in bones.size():
		check(remapped[Mesh.ARRAY_BONES][i] == (1 if bones[i] in [71, 93] else 0), "indices remapped")
		if original[Mesh.ARRAY_WEIGHTS][i] > 0:
			check(skin.get_bind_pose(original[Mesh.ARRAY_BONES][i]) == kept.get_bind_pose(remapped[Mesh.ARRAY_BONES][i]), "weighted transform unchanged")
	check(compacted.shadow_mesh.surface_get_arrays(0)[Mesh.ARRAY_BONES] == remapped[Mesh.ARRAY_BONES], "shadow mesh remapped")
	var bounds: Array = compacted._get_surfaces()[0].bone_aabbs
	var old_bounds: Array = mesh._get_surfaces()[0].bone_aabbs
	check(bounds.size() == 2 and bounds[1] == old_bounds[71].merge(old_bounds[93]), "bounds of merged bindings preserved")
	var path := "user://skin_bindings.res"
	check(ResourceSaver.save(compacted, path) == OK, "compacted mesh saved")
	var loaded: ArrayMesh = ResourceLoader.load(path, "ArrayMesh", ResourceLoader.CACHE_MODE_IGNORE)
	check(loaded.surface_get_arrays(0)[Mesh.ARRAY_BONES] == remapped[Mesh.ARRAY_BONES], "saved indices are remapped")
	DirAccess.remove_absolute(path)
	var again := Bindings.compact(compacted, kept)
	check(again[0] == compacted and again[1] == kept, "already compact resources reused")

	completed += 1
