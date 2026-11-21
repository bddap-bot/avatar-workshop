## A model's scene, built by Godot's glTF importer from `AvatarModel.glb()`, with an `AvatarEvaluator` bound to it.
## Safe to call off the main thread; the scene is not in the tree.


## `{root, evaluator, state}`, or `{error}` naming why the model has no scene.
static func open(model: AvatarModel) -> Dictionary:
	var evaluator: Variant = AvatarEvaluator.create(model)
	if evaluator is String:
		return {"error": evaluator}
	var doc := GLTFDocument.new()
	var state := GLTFState.new()
	var err := doc.append_from_buffer(model.glb(), "", state)
	if err != OK:
		return {"error": "glTF: %s" % error_string(err)}
	var root := doc.generate_scene(state) as Node3D
	if root == null:
		return {"error": "glTF: no scene"}
	var json: Dictionary = JSON.parse_string(model.model())
	var nodes := state.get_nodes()
	var skeletons := state.get_skeletons()
	var named := {}
	_name_all(root, named)
	var held: Array = []
	var bones := PackedInt32Array()
	var placed: Array = []
	for i in nodes.size():
		var n: GLTFNode = nodes[i]
		var sk: Skeleton3D = skeletons[n.skeleton].get_godot_skeleton() if n.skeleton >= 0 else null
		var b := sk.find_bone(n.resource_name) if sk else -1
		held.append(sk if b >= 0 else null)
		bones.append(b)
		placed.append(null if b >= 0 else named.get(n.resource_name))
	var renderers: Array = []
	for r in json.get("renderers", []):
		var node: Variant = placed[int(r["node"])]
		renderers.append(node if node is MeshInstance3D else null)
	evaluator.bind(held, bones, placed, renderers, state.get_materials())
	return {"root": root, "evaluator": evaluator, "state": state}


static func _name_all(n: Node, named: Dictionary) -> void:
	if n is Node3D and not n is Skeleton3D:
		named[String(n.name)] = n
	for c in n.get_children():
		_name_all(c, named)
