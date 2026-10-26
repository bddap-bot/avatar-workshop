# Avatar Workshop

Portable Godot 4 avatar runtime components, licensed under MIT OR Apache-2.0.
Copy `addons/avatar_workshop/` into a Godot 4.6 project. No editor plugin activation,
native library, network access, file loading or third-party dependency is required.

## Expression mixer

Preload `res://addons/avatar_workshop/expression_mixer.gd` and call
`mix(weights, bindings)`. Both arrays have one entry per expression channel.
Each channel's bindings contain `[target, shape_index, influence]` triples;
`target` is a caller-owned identifier (including a mesh instance).
The result maps `[target, shape_index]` to the accumulated weight, clamped to
0–1 after all contributions. Zero-weight channels still emit zero for their
bound targets so callers can reset the previous frame. Inputs remain unchanged.
The caller validates channel lengths and binding triples when loading assets.
The mixer does not access nodes; the caller applies returned weights.

## Spring chains

`spring_definition.gd` reads a glTF JSON dictionary with VRM 1.0
`VRMC_springBone` or VRM 0.x `secondaryAnimation`. `read(document)` returns
`springs` and `colliders`: node indices, joint stiffness, drag, gravity and hit
radius, sphere or capsule geometry, collider groups and optional centers.
Legacy offsets and gravity are converted from Unity coordinates. Branches
retain every joint, with each joint driven once.

`spring_runtime.gd.new(definition, nodes)` binds those indices to caller-owned
`Node3D` objects already in a scene tree, or to bones of a skinned
`Skeleton3D` through `skeleton_bone.gd.new(skeleton, bone_index)`, which reads
and writes the bone's pose as a node's transform. Each node's or bone's local
transform is its rest pose. Call `advance(delta)` after posing the body. The solver uses a
60 Hz fixed step; pauses longer than 250 ms reset momentum. Sphere and capsule
colliders follow their bound nodes. `targets[chain_index] = world_position`
holds a chain tip within its reach; erase the entry to release it.
`tip(chain_index)` returns the visible endpoint. Held tips yield to collision
and length constraints when the requested position is obstructed or unreachable.

The caller owns input arbitration and networking. `state()` returns world-space
tail positions; `apply_state(points)` poses a replica without running another
simulation. It rejects wrong counts and nonfinite positions. Validate network
ownership and spatial bounds in the caller. Do not advance a replica while
applying snapshots. Moving nodes into a scene, loading files, decoding meshes,
and deciding a rendering budget are also caller responsibilities.

The solver adapts the Verlet update and length/collision projection approach
from [godot-vrm](https://github.com/V-Sekai/godot-vrm), under the retained
[MIT notice](addons/avatar_workshop/LICENSE-godot-vrm). It binds ordinary nodes or skeleton
bones and supports tip constraints without an editor plugin or native extension.

## Tests

Run `godot4 --headless --path . --script tests/expressions.gd` and
`godot4 --headless --path . --script tests/springs.gd`.
The seven checks cover shared targets, separate meshes, neutral reset, empty
input, clamping and reusable binding data. No asset downloads are needed.

## License

Original code is available under either [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option. Third-party code, if added later,
retains its original license notices. The spring solver includes the godot-vrm
MIT notice linked above.
