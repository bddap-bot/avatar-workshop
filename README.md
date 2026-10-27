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

`spring_runtime.gd.new(definition, nodes)` binds indices to caller-owned
`Node3D` objects or `skeleton_bone.gd.new(skeleton, bone_index)` handles.
Their local transforms define the rest pose. Call `advance(delta)` after
posing the body. Adjacent skeleton chains with unit transforms (allowing bind-matrix roundoff) use
one Godot 4.6 `SpringBoneSimulator3D` per skeleton, sharing colliders. Groups with
skipped bones, materially scaled transforms, or colliders/centers driven by another
chain retain the 60 Hz constraint solver. Branches run in definition order. This capability selection is identical on every device.
Sphere and capsule colliders follow their bound nodes.

`targets[chain_index] = world_position` holds a tip within its reach; erase
the entry to release it. Held tips yield to collisions and length constraints.
A grip hands its native group to the constraint solver with the captured momentum.
The group returns to native simulation once settled, without snapping to rest. Pauses longer than
250 ms reset momentum. `tip(chain_index)` returns the last visible endpoint;
native poses are captured when the skeleton modifier finishes.

Each spring also says whose hands touch it, as `collide` and `grab` bits of
`WEARER` and `OTHERS`, read from the spring's (or VRM 0.x bone group's) glTF
`extras`: `"collide"` and `"grab"` each list `"wearer"` and `"others"`, as a
VRChat PhysBone's allowCollision and allowGrabbing filters do; a spring without
them is everyone's to touch. Set `hands` before `advance` to spheres
`[world_center, radius, WEARER or OTHERS]`: each pushes the chains whose
`collide` bits share its own, so a poke moves hair, ears or a tail, and a held
chain follows its holder instead. `grabs(chain_index, who)` says whether a hand
of `who` may hold that chain; the caller checks it before setting a target.

The caller owns input arbitration and networking. `state()` returns world-space
tail positions; `apply_state(points)` poses a replica without running another
simulation. It rejects wrong counts and nonfinite positions. Validate network
ownership and spatial bounds in the caller. Do not advance a replica while
applying snapshots. Moving nodes into a scene, loading files, decoding meshes,
and deciding a rendering budget are also caller responsibilities.

The solver adapts the Verlet update and length/collision projection approach
from [godot-vrm](https://github.com/V-Sekai/godot-vrm), under the retained
[MIT notice](addons/avatar_workshop/LICENSE-godot-vrm). Held and unsupported chains use this solver without an editor plugin or
native extension. Collider geometry is prepared once per chain and conservative
bounds reject impossible contacts before exact constraint projection.

## Tests

Run `godot4 --headless --path . --script tests/expressions.gd` and
`godot4 --headless --path . --script tests/springs.gd`.
`godot4 --headless --path . --script tests/spring_motion.gd` checks a reference
trace with moving and scaled skeletons, colliding dependent chains, held tips
and a pause reset. `godot4 --headless --path . --script tests/native_springs.gd`
checks native motion, grip/release handoff and replicated poses.
The seven checks cover shared targets, separate meshes, neutral reset, empty
input, clamping and reusable binding data. No asset downloads are needed.

## License

Original code is available under either [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option. Third-party code, if added later,
retains its original license notices. The spring solver includes the godot-vrm
MIT notice linked above.

## Skin bindings

`skin_bindings.gd.compact(mesh, skin)` returns an `ArrayMesh` and `Skin` with
unreferenced bindings removed and identical bindings shared. Call it on the main
thread before instances use the resources: serialized mesh surfaces access the
renderer. Reuse the returned pair for instances sharing the same mesh and skin. Vertex weights and geometry, morphs, LODs,
materials, shadow geometry and bone bounds remain intact. The original resources
are unchanged; already compact resources are returned directly. This reduces the
transforms the renderer uploads without reducing mesh or skeleton detail.

Run `godot4 --headless --path . --script tests/skin_bindings.gd` for binding,
geometry, bounds and saved-resource checks with four and eight bone influences.
