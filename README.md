# Avatar Workshop

Portable Godot 4 avatar runtime components, licensed under MIT OR Apache-2.0.
Copy `addons/avatar_workshop/` into a Godot 4.6 project. No editor plugin activation,
native library, network access, file loading or third-party dependency is required.

## Avatar model

The data model every importer writes and every runtime reads, designed from VRChat avatars
with VRM as one importer, is specified in [docs/model.md](docs/model.md).

The Rust crate [`avatar-core`](avatar-core) holds its types, the `.glb` reader and writer
(`read`, `write`) and [the `AW_avatar` JSON schema](avatar-core/schema/AW_avatar.schema.json)
generated from the types. `avatar-core/fixtures/synthetic.glb` is the model's synthetic
fixture, with every section filled.

The reader parses a JSON chunk of at most `MAX_JSON_BYTES`. Reader and writer refuse a model
whose references do not resolve, in the glTF document or in the model itself; a refusal names
its place as a JSON pointer into `AW_avatar`. Whether an avatar is affordable (triangles,
texture sizes, PhysBone counts) is the host's quality policy, applied to a model after it is
read; the crate has none.

In `avatar-core/`, `nix-shell --run 'cargo test'` checks that the fixture round-trips through
reader and writer unchanged, that the checked-in schema and fixture equal the generated ones
(after changing the types, `AW_BLESS=1 cargo test` regenerates both), and that 10,000 fuzzed
invalid files are each refused with a reason and none panics. `nix-shell --run 'cargo build
--lib --target aarch64-linux-android --target wasm32-unknown-unknown'` builds it for Android
and the web.

## PhysBones

`avatar_core::physbones` runs a model's PhysBones. `PhysBones::new(&avatar)` builds each component's chain
from the glTF node tree; `advance(dt, &frame)` steps every chain at the fixed `STEP` (1/60 s) however often the
host calls it, interpolating the host's frames for steps between them and extrapolating past the last one, and
`local(physbone)` returns each simulated bone's local pose at the host's current time, blended between the
steps around it. A `Frame` carries, in the avatar's own space and scale (glTF axes, metres): the avatar root in
the world, every node's pose relative to it after animation and IK, every node's animated local pose (read only
for chains marked animated; others keep the authored rest), every node's active flag and every component's
enabled flag. A chain runs while its component is enabled and active in the hierarchy. `grab`, `hold`,
`release` and `unpose` drive a hand, honouring the grab and pose filters; `parameters(physbone)` gives
`_IsGrabbed`, `_IsPosed`, `_Angle`, `_Stretch` and `_Squish`.

The semantics were measured against the SDK's own simulation through the reference harness, not taken from its
binaries. Simplified integration blends velocity toward pull by spring; advanced keeps momentum and pull,
both scaled down by stiffness, and carries each bone with its parent's movement by stiffness. Version 1.1 gravity
turns the rest direction toward down by the gravity value less its falloff; version 1.0 gravity is a force.
Immobile all-motion follows the root's parent, world only the avatar's position. Curves are read by depth along
the chain. Bones collide as spheres of their radius at their ends, and bones of radius 0 pass through spheres
and capsules but not planes.

`cargo test --test physbones` replays recordings in `avatar-core/fixtures/reference/` (the probe, the
fixture's motion and its grabs, trimmed to what the replays read) and checks each field's probe, the fixture's
median error, grabs, poses and their filters, the parameters, and that 72, 90 and 120 Hz hosts agree with 60 Hz
within 1% of chain length. `cargo run --release --example physbones -- RECORDING AVATAR.glb [--chains]` scores
any recording, and `--example physbones_cost -- AVATAR.glb` times a joint step.

## Unity exporter

[`unity/com.avatarworkshop.exporter`](unity/com.avatarworkshop.exporter) is a Unity 2022.3 editor
package for projects with the VRChat Avatars SDK. It writes an avatar, as the SDK would upload it, to a
`.glb` with `AW_avatar`. It exports a copy of the avatar: legacy DynamicBones converted by the SDK, then
the SDK's build preprocess run, which applies build-time tools and strips EditorOnly objects. The scene
is left unchanged. Add the package to the project's `Packages/manifest.json` as
`"com.avatarworkshop.exporter": "file:<path to the package>"`. Then export the selected avatar with
**Tools ▸ Avatar Workshop ▸ Export Avatar**, or in batch mode:

```sh
Unity -batchmode -projectPath <project> -executeMethod AvatarWorkshop.Exporter.Batch.Run \
  -awOut <avatar.glb> [-awScene <scene path>] [-awAvatar <avatar object name>]
```

Without `-awAvatar`, the scene must hold exactly one active avatar.
`avatar-core/fixtures/unity-synthetic.glb` is the export of the [reference harness](#reference-harness)'s
fixture, and `cargo test` reads it. In a scratch project holding both packages, generate the fixture with
`-executeMethod AvatarWorkshop.Reference.Fixture.Build -quit`, then export it with
`-awScene Assets/AWFixture/Fixture.unity`. `cargo run --example read -- <file.glb>...`, run in `avatar-core/`, reads exports and prints
their sections and their `unsupported` report.

`unity/check.sh <project> <out> [scene] [avatar]` checks an export against Unity itself. With
`-awCensus <census.json>` the exporter also writes what Unity holds for the preprocessed avatar, read
straight from Unity's APIs: every serialized PhysBone field, component counts by kind, renderer slots,
every vertex stream, menus, controllers and curve bindings. A playable layer left at the default counts and
exports the SDK's own default controller for it, with the mask imposed on it, as Av3Emulator runs them. The script exports with it, records the wall
time and peak RSS, and runs `avatar-core`'s `census` example, which prints one line per category and fails
when a count, a PhysBone field or any vertex value differs, or when a component the model cannot hold is
missing from `unsupported`.

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
one Godot 4.6 `SpringBoneSimulator3D` per skeleton, sharing colliders. Chains connected
through driven ancestors, shared joints, centers or colliders stay together. A component
with skipped bones, materially scaled transforms or a collider dependency unsupported
by the native solver retains the 60 Hz constraint solver; independent supported
components remain native. Branches run in definition order. This capability selection
is identical on every device.
Sphere and capsule colliders follow their bound nodes.

`targets[chain_index] = world_position` holds a tip within its reach; erase
the entry to release it. Held tips yield to collisions and length constraints.
A grip, a hand within reach or a scaled ancestor hands only its dependent component to the constraint solver
with the captured momentum. Independent components keep running in the same native
modifier. The component returns to native simulation once settled, without snapping to rest. Pauses longer than
250 ms reset momentum. `tip(chain_index)` returns the last visible endpoint;
native poses are captured when the skeleton modifier finishes. `touches(chain_index, slack)` says
whether a captured point of the chain lies within `slack` metres of one of its declared colliders,
whichever solver placed it.

Each spring also says whose hands touch it, as `collide` and `grab` bits of
`WEARER` and `OTHERS`, read from the spring's (or VRM 0.x bone group's) glTF
`extras`: `"collide"` and `"grab"` each list `"wearer"` and `"others"`, as a
VRChat PhysBone's allowCollision and allowGrabbing filters do; a spring without
them is everyone's to touch. Set `hands` before `advance` to world-space
capsules `{"key", "from", "to", "radius", "who"}` (a sphere where the ends
meet, `who` WEARER or OTHERS, `key` the same hand's name every frame): each
pushes the chains whose `collide` bits share its own, so a poke moves hair, ears
or a tail, and a point it pushes holds to its surface, carried as the hand moves
but never pulled as it draws away, so a strand caught in a hand drags with it
until the hand leaves it or the chain's length draws it off; a held chain follows
its holder instead. `grabs(chain_index, who)` says whether a hand
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

## MToon

`mtoon.gd` turns a VRM material into Godot's: `material(gltf_material,
texture)` takes a glTF material dictionary carrying VRM 1.0
`VRMC_materials_mtoon` and a callable answering a glTF texture index with its
`Texture2D`, and returns a `ShaderMaterial` whose next pass is its outline.
`from_vrm0(entry, gltf_material, render_queues(entries))` converts a VRM 0.x
`materialProperties` entry (MToon or VRM's unlit shaders) to that form, as
three-vrm's v0 compatibility plugin does. `mtoon.gdshaderinc` lights it with the
scene's own lights: lit and shade colours and textures, shading shift and
toony ramp, shading-shift texture, tangent-space normal maps, emission (with
`KHR_materials_emissive_strength` or the VRM emissive multiplier), parametric
rim, matcap and rim texture, UV animation, opaque, cut-out and blended alpha
with transparent depth writes and render queue offsets, one- or two-sided
culling, and world- or screen-width outlines with their width texture, colour
and lighting mix. Materials with the same features share one shader. Global
illumination equalization is not applied, as three-vrm does not; scene ambient
light reaches the lit colour. Malformed values fall back to MToon's defaults.

## Tests

Run `godot4 --headless --path . --script tests/expressions.gd` and
`godot4 --headless --path . --script tests/springs.gd`.
`godot4 --headless --path . --script tests/spring_motion.gd` checks a reference
trace with moving and scaled skeletons, colliding dependent chains, held tips
and a pause reset. `godot4 --headless --path . --script tests/native_springs.gd`
checks native motion, grip/release handoff and replicated poses.
`godot4 --headless --path . --script tests/mtoon.gd` checks the 0.x conversion,
render modes, optional features, linear colours and malformed input.
The seven checks cover shared targets, separate meshes, neutral reset, empty
input, clamping and reusable binding data. No asset downloads are needed.

## Reference harness

`reference/` is a Unity editor package that records how an avatar behaves under the VRChat SDK,
for comparing runtimes against it, as `docs/model.md` ("Measurement") describes. It needs
Unity 2022.3 with graphics (a display; Xvfb with software GL works), `com.vrchat.avatars` 3.7 or
later and [Av3Emulator](https://github.com/lyuma/Av3Emulator) 3.4 (MIT) in the avatar's project;
the SDK and the emulator are installed there, never copied here.

`reference/run.sh PROJECT OUT [SCENE] [AVATAR]` adds the package to `PROJECT`, opens `SCENE`, enters play
mode in batch mode (the emulator runs the SDK's build preprocess, so build-tool output is what is
recorded) and drives the avatar at a fixed 60 Hz step with one scripted input: rest, six seconds of
root and arm motion, settling, every expression-menu control swept (each toggle and button to its
non-default value, each radial at 0, 0.25, 0.5, 0.75 and 1, each puppet axis to its extremes, inside
its submenus), then the built-in parameters stepped (gestures, visemes, locomotion, tracking and
state flags). `UNITY` names the editor command. With no `SCENE`, it generates the fixture into
`PROJECT`'s `Assets/AWFixture` and records it, so point it at a scratch project. With `SCENE` `probe`, it generates and
records the probe there: the fixture's body with one three-bone chain per PhysBone field value (`Fixture.Probes` lists
them), so each field's effect has a reference trace of its own. The probe rests 15 s before its motion, so even its lightly damped chains start it settled. It restores
`PROJECT`'s package manifest on exit and refuses an `OUT` that holds something other than an earlier recording.
`OUT` receives:

| File | Contents |
|---|---|
| `steps.tsv` | each step: index, first frame, frames, menu control, label, parameters set |
| `params.tsv` | every animator parameter on each change, values printed round-trip exact |
| `states.tsv` | every playable layer's state and transition on each change |
| `values.tsv` | node active, renderer enabled, PhysBone enabled (`physbone:<path>#<k>`, the k-th on that object), material slots, blendshape weights, animated material properties and animated transforms, and what the behaviours set: each playable layer's weight (`playable:<i>`, numbered as in `states.tsv`), each animator layer's weight (`weight:<i>:<l>:<name>`, numbered as in `states.tsv`), each animator parameter a clip curve drives (`curve:<i>:<name>`), the tracking of each body part (`tracking:<part>`), `locomotion:disabled` and the view position (`view`); each on change |
| `tips.tsv`, `chains.tsv` | every PhysBone leaf (with its endpoint; for the probe, every bone below each root) each frame, in metres from the avatar root along its axes (its scale ignored), and its rest chain length |
| `track.tsv` | the avatar root in the world, and its head, hands and hips in metres from the root along its axes, each frame, for replaying the same input elsewhere |
| `poses.tsv`, `drivers.tsv` | the input the SDK's dynamics read, taken each frame after animation and before its constraints, PhysBones and contacts run: every transform they read besides the bones they simulate (each PhysBone root's parent and scene root, every collider and contact, each constraint's target, its parent, its sources and world-up transform) in metres from the root along its axes (`root` rows, scales world), each written when it changes, and each PhysBone bone's local pose (`local` rows) whenever it differs from the pose the SDK left it in the frame before, which is when animation set it. A row is `frame, space, transform, px,py,pz,qx,qy,qz,qw,sx,sy,sz`; the root itself is the empty path. `drivers.tsv` lists each transform with its roles; `moved-by-dynamics` marks one at or under a simulated bone or under a constraint target, whose recorded pose is the one before the dynamics moved it. A recording fails if any of them is gone or unposed on a frame |
| `grabs.tsv` | empty but for the fixture, which after the built-in steps has its tail, antenna and left ear grabbed through the SDK's own grab call (a ray at the bone, as its editor grab helper casts), the held point moved and let go, the tail once posed; each row is `frame, event, bone, x,y,z`, the event `grab` (at the point the SDK grabbed), `miss`, `hold` (the held point that frame), `release` or `pose` |
| `render/`, `renders.tsv`, `camera.json` | a render at the end of every swept step from a fixed camera and light |
| `menu.tsv` | every menu control: indexed path, name path (control names joined by `/`), type, parameter |
| `coverage.json`, `meta.json` | the avatar's feature counts; versions and totals |

The fixture (`reference/Editor/Fixture.cs`, licensed as this repository) is a synthetic humanoid
authored for the harness: PhysBones covering both immobile types, every limit type, gravity with
falloff, stretch and squish, curves, the three multi-child types, an endpoint and both versions
and integrations, colliders of every shape including an inside-bounds sphere; a contact pair;
the six constraint kinds, one frozen to world; a menu with every control type; transitions with fixed and normalised
durations; every blend-tree algorithm; parameter drivers of each kind, one local-only; mixed write defaults. It
has its own controller for every playable layer (the SDK's default controllers are not in this repository), together
covering tracking, locomotion, playable-layer and animator-layer control and temporary pose space behaviours,
body-part and transform masks, additive and synced layers, sub-machine entry and exit transitions, exit-time
transitions with offsets, speed, time, cycle-offset and mirror parameters, a trigger, an animator-parameter curve,
stepped and weighted keys, Euler rotation curves and material swaps. A fixture recording fails if any of these is
missing from `coverage.json`.
`reference/check.sh PROJECT OUT [SCENE] [AVATAR]` records the avatar (the fixture by default) twice and requires every output but the log,
`tips.tsv` and `poses.tsv` to be byte-identical, and PhysBone tips and every pose value within 1e-5 (`reference/poses.py`, Python 3.10 or later).

A runtime replays the recording from `track.tsv`, `poses.tsv`, `values.tsv`, `grabs.tsv` and `steps.tsv` and writes `tips.tsv` and `render/`
in the same layout; `reference/compare.py REFERENCE RUNTIME` (Python 3 with numpy and Pillow) scores
it: PhysBone tip RMS error over chain length (a leaf or frame the runtime lacks counts as unmoving), the
fraction of menu controls that match, and mean image error. A control matches when the runtime's
`menu.tsv` lists its name path in the first column, before a tab (names compare with runs of whitespace
collapsed, so a name may span lines), and each of its swept renders changes the same region as
the reference's (intersection over union of changed pixels at least 0.5, or both unchanged); a control
the runtime does not offer fails. It writes every source control's result to `compare-controls.tsv` and each
swept step's to `compare-steps.tsv` in `RUNTIME`.

## License

Original code is available under either [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option. Third-party code, if added later,
retains its original license notices. The spring solver includes the godot-vrm
MIT notice linked above. The MToon shader follows three-vrm's MToon 1.0 shader and
its v0 conversion, under the retained [MIT notice](addons/avatar_workshop/LICENSE-three-vrm).

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
