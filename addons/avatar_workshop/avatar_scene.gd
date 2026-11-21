## A model's scene, built once by Godot's glTF importer from `AvatarModel.glb()` and packed, then instanced with its own
## `AvatarEvaluator` for each avatar that wears it.


## `{scene, model, json, textures, ...binding}` safe to share between instances, or `{error}` naming why the model has
## no scene. `state` carries what the host's glTF document extensions read; the scene nodes it names are freed on
## return. Safe to call off the main thread.
static func pack(model: AvatarModel, state := GLTFState.new()) -> Dictionary:
	var doc := GLTFDocument.new()
	var err := doc.append_from_buffer(model.glb(), "", state)
	if err != OK:
		return {"error": "glTF: %s" % error_string(err)}
	var root := doc.generate_scene(state) as Node3D
	if root == null:
		return {"error": "glTF: no scene"}
	var json: Dictionary = JSON.parse_string(model.model())
	var named := {}
	_name_all(root, root, named)
	var skeletons: Array[NodePath] = []
	var bones := PackedStringArray()
	var nodes: Array[NodePath] = []
	var gltf_skeletons := state.get_skeletons()
	for n: GLTFNode in state.get_nodes():
		var sk: Skeleton3D = gltf_skeletons[n.skeleton].get_godot_skeleton() if n.skeleton >= 0 else null
		var boned := sk != null and sk.find_bone(n.resource_name) >= 0
		skeletons.append(root.get_path_to(sk) if boned else NodePath())
		bones.append(n.resource_name if boned else "")
		var at: NodePath = named.get(n.resource_name, NodePath())
		# A joint's own mesh hangs under a BoneAttachment3D, by the joint's name.
		nodes.append(at if not boned or not at.is_empty() and root.get_node(at) is MeshInstance3D else NodePath())
	var renderers: Array[NodePath] = []
	for r: Dictionary in json.get("renderers", []):
		var at: NodePath = nodes[int(r["node"])]
		renderers.append(at if not at.is_empty() and root.get_node(at) is MeshInstance3D else NodePath())
	for n in root.find_children("*", "", true, false):
		n.owner = root
	var images := state.get_images()
	var textures: Array[Texture2D] = []
	for t: GLTFTexture in state.get_textures():
		textures.append(images[t.src_image] if t.src_image >= 0 and t.src_image < images.size() else null)
	var scene := PackedScene.new()
	err = scene.pack(root)
	root.free()
	if err != OK:
		return {"error": "pack: %s" % error_string(err)}
	return {"scene": scene, "model": model, "json": json, "skeletons": skeletons, "bones": bones, "nodes": nodes, "renderers": renderers, "materials": state.get_materials(), "textures": textures}


## A new instance of a packed scene with its own evaluator bound to it and its own materials: `{root, evaluator}`, or
## `{error}`. Safe off the main thread; the root is outside the tree.
static func instance(packed: Dictionary) -> Dictionary:
	var evaluator: Variant = AvatarEvaluator.create(packed.model)
	if evaluator is String:
		return {"error": evaluator}
	var root := (packed.scene as PackedScene).instantiate() as Node3D
	var held: Array = []
	var bones := PackedInt32Array()
	var placed: Array = []
	for i in (packed.nodes as Array).size():
		var sk := root.get_node_or_null(packed.skeletons[i]) as Skeleton3D if not (packed.skeletons[i] as NodePath).is_empty() else null
		held.append(sk)
		bones.append(sk.find_bone(packed.bones[i]) if sk else -1)
		placed.append(root.get_node_or_null(packed.nodes[i]) if not (packed.nodes[i] as NodePath).is_empty() else null)
	var renderers: Array = (packed.renderers as Array).map(func(p: NodePath) -> Variant: return root.get_node_or_null(p) if not p.is_empty() else null)
	var materials: Array[Material] = []
	materials.assign((packed.materials as Array).map(func(m: Material) -> Material: return m.duplicate()))
	evaluator.bind(held, bones, placed, renderers, materials)
	return {"root": root, "evaluator": evaluator}


static func _name_all(n: Node, root: Node, named: Dictionary) -> void:
	if n is Node3D and not n is Skeleton3D and n != root:
		named[String(n.name)] = root.get_path_to(n)
	for c in n.get_children():
		_name_all(c, root, named)
