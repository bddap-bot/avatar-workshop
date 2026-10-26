extends RefCounted

var skeleton: Skeleton3D
var bone: int

var transform: Transform3D:
	get:
		return skeleton.get_bone_pose(bone)
	set(value):
		skeleton.set_bone_pose(bone, value)

var global_transform: Transform3D:
	get:
		return skeleton.global_transform * skeleton.get_bone_global_pose(bone)
	set(value):
		skeleton.set_bone_global_pose(bone, skeleton.global_transform.affine_inverse() * value)

var global_position: Vector3:
	get:
		return global_transform.origin

var global_basis: Basis:
	get:
		return global_transform.basis


func _init(on: Skeleton3D, index: int) -> void:
	skeleton = on
	bone = index


func is_ancestor_of(other: Object) -> bool:
	if other.get("skeleton") != skeleton:
		return false
	var parent := skeleton.get_bone_parent(other.bone)
	while parent >= 0:
		if parent == bone:
			return true
		parent = skeleton.get_bone_parent(parent)
	return false
