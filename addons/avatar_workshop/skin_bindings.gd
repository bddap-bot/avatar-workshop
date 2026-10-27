extends RefCounted


static func compact(mesh: ArrayMesh, skin: Skin) -> Array:
	var used := {}
	var meshes: Array[ArrayMesh] = [mesh]
	if mesh.shadow_mesh != null:
		meshes.append(mesh.shadow_mesh)
	for part in meshes:
		for surface in part.get_surface_count():
			var arrays := part.surface_get_arrays(surface)
			if arrays[Mesh.ARRAY_BONES] == null:
				continue
			var bones: PackedInt32Array = arrays[Mesh.ARRAY_BONES]
			var weights: PackedFloat32Array = arrays[Mesh.ARRAY_WEIGHTS]
			for i in bones.size():
				if weights[i] > 0.0:
					used[bones[i]] = true
	if used.is_empty():
		return [mesh, skin]
	var indices := PackedInt32Array()
	indices.resize(skin.get_bind_count())
	var bindings := {}
	var kept := Skin.new()
	for i in skin.get_bind_count():
		if not used.has(i):
			continue
		var key := [skin.get_bind_bone(i), skin.get_bind_name(i), skin.get_bind_pose(i)]
		if not bindings.has(key):
			var index := kept.get_bind_count()
			kept.add_bind(key[0], key[2])
			kept.set_bind_name(index, key[1])
			bindings[key] = index
		indices[i] = bindings[key]
	if kept.get_bind_count() == skin.get_bind_count():
		return [mesh, skin]
	var result := _remapped(mesh, indices, used, kept.get_bind_count())
	if mesh.shadow_mesh != null:
		result.shadow_mesh = _remapped(mesh.shadow_mesh, indices, used, kept.get_bind_count())
	return [result, kept]


static func _remapped(mesh: ArrayMesh, indices: PackedInt32Array, used: Dictionary, count: int) -> ArrayMesh:
	var result := mesh.duplicate() as ArrayMesh
	# Serialized surfaces retain compressed attributes, morphs and LODs bit for bit.
	var surfaces: Array = mesh._get_surfaces()
	for surface: Dictionary in surfaces:
		if not surface.has("skin_data"):
			continue
		var format: int = surface.format
		var vertices: int = surface.vertex_count
		var stride := RenderingServer.mesh_surface_get_format_skin_stride(format, vertices)
		var offset := RenderingServer.mesh_surface_get_format_offset(format, vertices, Mesh.ARRAY_BONES)
		var data: PackedByteArray = surface.skin_data
		var weights := 8 if format & Mesh.ARRAY_FLAG_USE_8_BONE_WEIGHTS else 4
		for vertex in vertices:
			for weight in weights:
				var at := vertex * stride + offset + weight * 2
				data.encode_u16(at, indices[data.decode_u16(at)])
		surface.skin_data = data
		var bounds: Array[AABB] = []
		bounds.resize(count)
		bounds.fill(AABB(Vector3.ZERO, -Vector3.ONE))
		var original: Array = surface.get("bone_aabbs", [])
		for i in original.size():
			if used.has(i) and original[i].size.x >= 0.0:
				var index := indices[i]
				bounds[index] = original[i] if bounds[index].size.x < 0.0 else bounds[index].merge(original[i])
		surface.bone_aabbs = bounds
	result._set_surfaces(surfaces)
	return result
