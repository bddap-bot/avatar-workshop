extends RefCounted


static func read(document: Dictionary) -> Dictionary:
	var nodes := array(document.get("nodes"))
	var extensions := object(document.get("extensions"))
	var result := {"springs": [], "colliders": []}
	if extensions.has("VRMC_springBone"):
		var source := object(extensions.VRMC_springBone)
		for item in array(source.get("colliders")):
			var collider := object(item)
			var shape := object(collider.get("shape"))
			var capsule := shape.has("capsule")
			var geometry := object(shape.get("capsule" if capsule else "sphere"))
			result.colliders.append({"node": index(collider.get("node"), nodes.size()), "offset": vector(geometry.get("offset")), "tail": vector(geometry.get("tail" if capsule else "offset")), "radius": number(geometry.get("radius"), 0.0)})
		var groups := array(source.get("colliderGroups"))
		for item in array(source.get("springs")):
			var spring := object(item)
			var joints := []
			for entry in array(spring.get("joints")):
				var joint := object(entry)
				var node := index(joint.get("node"), nodes.size())
				if node < 0 or joints.any(func(j: Dictionary) -> bool: return j.node == node):
					joints.clear()
					break
				joints.append(joint_data(node, joint, false))
			if not joints.is_empty():
				result.springs.append({"joints": joints, "center": index(spring.get("center"), nodes.size()), "colliders": collider_indices(spring.get("colliderGroups"), groups, result.colliders.size())})
	else:
		var source := object(object(extensions.get("VRM")).get("secondaryAnimation"))
		var groups := []
		for item in array(source.get("colliderGroups")):
			var group := object(item)
			var indices := []
			for entry in array(group.get("colliders")):
				var collider := object(entry)
				var offset := vector(collider.get("offset"), true)
				indices.append(result.colliders.size())
				result.colliders.append({"node": index(group.get("node"), nodes.size()), "offset": offset, "tail": offset, "radius": number(collider.get("radius"), 0.0)})
			groups.append({"colliders": indices})
		for item in array(source.get("boneGroups")):
			var group := object(item)
			for root in array(group.get("bones")):
				var first := index(root, nodes.size())
				if first < 0:
					continue
				var pending := [[first, []]]
				while not pending.is_empty():
					var branch: Array = pending.pop_back()
					var node: int = branch[0]
					var joints: Array = branch[1]
					if joints.any(func(j: Dictionary) -> bool: return j.node == node):
						continue
					joints = joints.duplicate()
					joints.append(joint_data(node, group, true))
					var children := array(object(nodes[node]).get("children")).filter(func(child: Variant) -> bool: return index(child, nodes.size()) >= 0)
					if children.is_empty():
						result.springs.append({"joints": joints, "end_length": 0.07, "center": index(group.get("center"), nodes.size()), "colliders": collider_indices(group.get("colliderGroups"), groups, result.colliders.size())})
					else:
						for i in range(children.size() - 1, -1, -1):
							pending.append([int(children[i]), joints if i == 0 else []])
	return result


static func joint_data(node: int, source: Dictionary, legacy: bool) -> Dictionary:
	return {"node": node, "stiffness": number(source.get("stiffiness" if legacy else "stiffness"), 1.0), "drag": clampf(number(source.get("dragForce"), 0.4 if legacy else 0.5), 0.0, 1.0), "radius": number(source.get("hitRadius"), 0.0), "gravity": vector(source.get("gravityDir"), legacy, Vector3.DOWN) * number(source.get("gravityPower"), 0.0)}


static func collider_indices(indices: Variant, groups: Array, count: int) -> Array:
	var result := []
	for value in array(indices):
		var group := index(value, groups.size())
		if group < 0:
			continue
		for collider in array(object(groups[group]).get("colliders")):
			var at := index(collider, count)
			if at >= 0 and not at in result:
				result.append(at)
	return result


static func object(value: Variant) -> Dictionary:
	return value if value is Dictionary else {}


static func array(value: Variant) -> Array:
	return value if value is Array else []


static func index(value: Variant, count: int) -> int:
	return int(value) if (value is float or value is int) and is_finite(value) and value == floor(value) and value >= 0 and value < count else -1


static func number(value: Variant, fallback: float) -> float:
	return maxf(value, 0.0) if (value is float or value is int) and is_finite(value) else fallback


static func vector(value: Variant, legacy := false, fallback := Vector3.ZERO) -> Vector3:
	var components: Array = [object(value).get("x"), object(value).get("y"), object(value).get("z")] if legacy else array(value)
	if components.size() != 3 or not components.all(func(v: Variant) -> bool: return (v is float or v is int) and is_finite(v)):
		return fallback
	return Vector3(components[0], components[1], -components[2] if legacy else components[2])
