# Avatar Workshop

Portable Godot 4 avatar runtime components, licensed under MIT OR Apache-2.0.
Copy `addons/avatar_workshop/` into a Godot 4.6 project, with the evaluation core's extension built into
`addons/avatar_workshop/bin/` (below). No editor plugin activation or network access is required.

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
and capsules but not planes; a bone a collider pushes is put back at a length its stretch allows.

`cargo test --test physbones` replays recordings in `avatar-core/fixtures/reference/` (the probe, the
fixture's motion and its grabs, trimmed to what the replays read) and checks each field's probe, the fixture's
median error, grabs, poses and their filters, the parameters, and that 72, 90 and 120 Hz hosts agree with 60 Hz
within 1% of chain length. `cargo run --release --example physbones -- RECORDING AVATAR.glb [--chains]` scores
any recording (`--names` prints chain names), and `--example physbones_cost -- AVATAR.glb` times a joint step.

`avatar_core::constraints` solves VRChat's six constraint kinds and `avatar_core::contacts` its contacts, as the
SDK's dynamics do. A host gives the constraints each node's local pose and active flag as animation left them
(`Scene`), calls `solve_before_physbones` and, once the PhysBones' output is in the scene, `solve_after_physbones`
(`after_physbones` tells which constraints read a bone a PhysBone moves, directly or through another constraint),
and sets fields a curve drives with `set`. Each frame it then calls `Contacts::advance` with every node's world pose and active flag and the
senders outside the avatar (`Sender`: another player's part or a hand it tracks, the wearer's or another's, each
with an id that names it across frames); the avatar's own senders include the standard ones on the descriptor's
colliders (`contacts::standard_senders`), so the wearer's own body parts on enabled colliders need no `Sender`. It
reads each receiver's parameter from `values()`: none while a receiver is not running, which leaves the parameter to
the animator. `cargo test --test dynamics` replays the dynamics probe and the fixture from
`avatar-core/fixtures/reference/` (trimmed to what the replay reads), placing what PhysBones move where
`moved.tsv` holds it, requiring every constrained target within
1e-4 m and 0.1° of `constrained.tsv` and every receiver parameter equal to `params.tsv` on every frame.
`cargo run --release --example dynamics -- RECORDING AVATAR.glb` scores any recording, numbering each target and
receiver parameter; `--names` prints their names.

`avatar_core::animator` runs the playable layers as VRChat stacks them: Unity's state machines with crossfades,
offsets, ordered interruption, sub-machines, synced layers and write defaults, every blend-tree algorithm,
layer masks and additive layers, humanoid clips as whole poses and their mirrors, the expression parameters
with their synced clamps and saved values, VRChat's built-in parameters as host inputs, the parameter driver
(local-only ones on the wearer's copy alone),
tracking, locomotion, layer-control and temporary-pose-space behaviours, and the descriptor's lip sync and
eyelids. A host sets built-ins (`set_builtin`), menu values (`set`), the eyelids' blink and look (`set_eyes`) and the head's animated pose
(`set_head`), calls `step(dt)`, and reads `parameters()`, each layer's state, `values()` per target,
`tracking`, `locomotion_disabled` and `view`; `saved()` and `restore` carry saved parameters across loads.
Its timing and blending rules were fitted to Av3Emulator's traces, the reference the harness records.
`cargo test --test animator` replays the fixture's recording in
`avatar-core/fixtures/reference/fixture-animator/`, stepping the core by the emulator's graph time from
`time.tsv` (its graph starts at a different frame for each avatar), setting on each frame the parameters `inputs.tsv` logs for it and driving contact and PhysBone parameters
from `dynamics.tsv`, and requires every parameter but those `dynamics.tsv` drives and every state sample to equal the recording's
and every recorded node, blendshape and material value, animated or not, to match within 1e-3. It also compares
each layer's state time from `clocks.tsv`, and the muscle, constraint and contact field values every curve binds,
and fails if a recording lacks a frame's state times, a bound field's value or a frame's row of `inputs.tsv`.
`cargo run --release --example animator -- RECORDING AVATAR.glb` scores any recording,
numbering each differing key and menu control; `--names` prints their names. Where the recording has
`humanoid.tsv` it also scores every humanoid bone's local rotation and the hips' position against it, and gives
each frame past 0.1° or 1e-4 m a cause: `animator` when posing the recording's own human stream lands within them,
`dynamics` on a bone PhysBones or constraints move, `tracked` while the emulator's tracking holds the bone's body
part or blends it back to animation, else `unexplained`.

## Humanoid poses

`avatar_core::humanoid::Human` poses the humanoid bones from the human stream as Unity's humanoid Avatar does:
each bone's local rotation from its muscles through the frame the exporter took from Unity (`pre · r · post⁻¹`,
`r` the twist and swing of each degree of freedom's angle), each bone but the hips at its position in the human
description's skeleton, an absent UpperChest's muscles added to the Chest, part
of the upper arm's, forearm's and leg bones' twist passed to the child bone, and the hips placed so that the
skeleton's centre of mass and body frame are the stream's `RootT` and `RootQ`. The frame evaluation poses the bones
with it after the animator. `cargo test --test humanoid` checks it against Unity's own `HumanPoseHandler` on the
fixture: `reference/Editor/HumanSamples.cs` writes `fixtures/reference/human-samples/`, and in its `duplicate/` the
fixture with an unmapped bone named like the head, exported as `fixtures/unity-duplicate.glb` (run it with
`-executeMethod AvatarWorkshop.Reference.HumanSamples.Run` and `AW_OUT` in a project with the VRChat SDK).

## Frame evaluation

`avatar_core::evaluate::Evaluator` is the one entry point a runtime drives: it owns a model's animator,
constraints, PhysBones and contacts and evaluates a frame in VRChat's order. `Evaluator::new(&avatar)` builds
them; each frame the host sets parameters (`set_builtin`, `set`, `set_eyes`, `set_head`; the `IsLocal` built-in
also decides which receivers run), drives grabs (`grab`, `hold`, `release`), and calls `evaluate(&Input)` with
the seconds since the last frame, the avatar root in the world, the root-space poses of the bones it moves
(tracked or IK-solved, in any order) and the senders outside the avatar. The frame runs the animator (not on a
frame with no time passed), applies its node transforms and the humanoid bones its human stream poses, active flags, PhysBone enabled flags and constraint and
contact fields over the rest pose, places the host's poses, solves the constraints before PhysBones, advances
PhysBones (a stopped chain leaves its bones to animation), solves the constraints after them, advances contacts,
and sets each receiver's and PhysBone's parameters on the animator for the next frame, where a host's own `set`
before then wins. `output()` is the frame: every node's local pose and root-space pose, active flags, each
renderer's enabled flag, and the blendshape weights, material properties and slots the animator binds. The
reference replays drive it with what the recording's animation left (`evaluate_animated`) and, for the dynamics
replay, with the SDK's PhysBone output in place of the core's (`Input::held`); a runtime uses neither.

`cargo run --release --example run -- RECORDING AVATAR.glb OUT [--frames N]` writes the core's reference run
of a model under a recording's scripted input: each frame the parameters `inputs.tsv` sets and the root, head and
hands of `track.tsv`, at 60 Hz. `OUT/tips.tsv` holds every PhysBone tip each frame, and `params.tsv`,
`states.tsv` and `output.tsv` the animator's parameters, each layer's state and the frame output as change logs.
Two runs write the same bytes; `cargo test --test evaluate` checks that on the fixture's recording.

## Godot extension

[`avatar-godot`](avatar-godot) is the evaluation core as a Godot 4.6 extension (gdext), which
`addons/avatar_workshop/avatar_core.gdextension` loads from `addons/avatar_workshop/bin/<platform>-<arch>/`
for Linux x86_64 and arm64, Windows x86_64 and Android arm64. `avatar-godot/install.sh` builds it for the
host; another target is `cargo build --release --target <triple>` in `avatar-godot/` with that target's linker.

- `AvatarModel.read(bytes)` returns the model a `.glb` with `AW_avatar` holds, or a VRM's through the
  importer, or the reason it refuses as a string. `glb()` is the glTF binary the model's indices name and
  `model()` the `AW_avatar` extension as JSON. Both are safe off the main thread.
- `AvatarScene.pack(model)` (`addons/avatar_workshop/avatar_scene.gd`) builds the model's scene once with Godot's
  glTF importer and packs it with what binds an evaluator to it, or returns `{error}`; `AvatarScene.instance(packed)`
  instantiates it with its own evaluator, `{root, evaluator}`, for each avatar wearing the model. `pack` takes the `GLTFState` a host's glTF document extensions read, and returns the glTF images by index (menu icons name them). Both are safe off the
  main thread; the root is outside the tree.
- `AvatarEvaluator` drives `Evaluator`: `set_parameter`, `set_builtin`, `set_eyes`, `set_head`, `pose(node,
  root_space)` for the bones the host moves, `evaluate(dt, root, hands)` with other avatars' `hands(player)`,
  `grab`, `hold`, `release`, then `apply()` writes node poses, renderer visibility (each renderer alone, its node and every ancestor active), blendshapes and
  material slots onto the bound scene; call it each frame once the scene is in the tree, since Godot resets a renderer's visibility as it enters. `bind` gives each renderer its own slots. `shown`, `space`, `local`,
  `parameters`, `saved`, `states`, `physbone_bones` and `materials` (the animated material properties, which a
  host's material renderer applies) read the last frame. A node or PhysBone index out of range is refused
  with an error.

Run `godot4 --headless --path . --script tests/core.gd` after `avatar-godot/install.sh`.

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
`avatar-core/fixtures/unity-synthetic.glb` is the `avatar.glb` of a [reference harness](#reference-harness)
recording of the fixture, and `cargo test` reads it. `cargo run --example read -- <file.glb>...`, run in `avatar-core/`, reads exports and prints
their sections and their `unsupported` report. Like every tool here that reads an avatar or its recording, it
prints numbers and counts by default, and names only with `--names`.

`unity/check.sh <project> <out> [scene] [avatar]` checks an export against Unity itself. With
`-awCensus <census.json>` the exporter also writes what Unity holds for the preprocessed avatar, read
straight from Unity's APIs: every serialized PhysBone field, component counts by kind, renderer slots,
every vertex stream, menus, controllers and curve bindings. A playable layer left at the default counts and
exports the SDK's own default controller for it, with the mask imposed on it, as Av3Emulator runs them. The script exports with it, records the wall
time and peak RSS, and runs `avatar-core`'s `census` example, which prints one line per category and fails
when a count, a PhysBone field or any vertex value differs, or when a component the model cannot hold is
missing from `unsupported`.

## VRM importer

`avatar_core::vrm::import(bytes)` reads a VRM 0.x or 1.0 file into the model. Its accessors, skins, mesh
targets and binary chunk stay as they were, so meshes, skins and blendshapes arrive unchanged; a mesh's morph
target names move to its `extras.targetNames`, and the VRM extensions are removed. A VRM 0.x avatar, which faces
-Z, has its scene's root nodes turned half a turn about Y, and its thumbs, expression presets, collider offsets
and MToon properties are read as VRM 0.x holds them; nothing outside the importer sees VRM 0.x.

- Humanoid bones become Unity's HumanBodyBones (VRM 1.0's thumb metacarpal is Unity's thumb proximal), with every
  node's rest pose as given and VRM 0.x's muscle limits on bones whose `useDefaultValues` is false.
- Spring bones become version 1.0 PhysBones with advanced integration, one per VRM 1.0 spring or VRM 0.x root,
  whose pull, spring and gravity make the PhysBone step equal VRM's spring step before collision (derived in
  `avatar-core/src/vrm/springs.rs`); VRM's per-joint values ride on curves over the chain. Colliders keep their
  shapes.
- MToon becomes an `mtoon` material record holding every MToon property: VRM 1.0's as the `VRM10/MToon10`
  properties UniVRM gives it, VRM 0.x's as the `VRM/MToon` properties the file stores. Other materials become `standard`
  records, or `unknown` ones for unlit shaders.
- Each expression becomes a clip that holds it at full weight. The vowels become the descriptor's visemes and
  blink its eyelids when each is one blendshape at full weight on one mesh; a bone look-at becomes the eye
  rotations; the look-at offset from the head becomes the view position.
- What the model cannot hold is in `unsupported`: node constraints, expression overrides and binary
  expressions, look-at by expression, a spring centre, gravity not straight down, a texture transform's
  rotation, among others; each entry names its kind and a count.

`cargo test --test vrm` imports the files in [`avatar-core/fixtures/vrm/`](avatar-core/fixtures/vrm) (their
sources and licences are listed there) and checks that each round-trips through the writer and keeps its
skinned vertices under a bend and its blendshapes, that every MToon property reaches its record, that each
avatar faces +Z, that its visemes, eyelids, expression clips and `unsupported` report match the file, and, for
the two fixtures with spring bones, that every spring tip run through the core's PhysBones stays within 5% of
its path length of three-vrm's tip under the same motion. `cargo run --release --example vrm -- AVATAR.vrm
[--bend] [--out MODEL.glb] [--springs RECORDING.tsv]` imports any file and prints its `unsupported` report, with
`--bend` how far its skinned vertices land from the source's under that bend, and with a recording each spring
tip's RMS distance from three-vrm's as a fraction of its path length.

[`vrm-reference/run.sh AVATAR.vrm > RECORDING.tsv`](vrm-reference) records three-vrm's spring bones
(`@pixiv/three-vrm-springbone` 3.5.5 on `three` 0.181.0, MIT, installed by `npm ci` from the lockfile on first
run) moving a file's spring bones at 60 Hz under a scripted motion of the avatar and its head. The script
patches three-vrm so that each joint measures its length to its child's position in the current step, so the
bone keeps its length as the VRM spring bone does; unpatched, three-vrm measures to the child's position from
before the joint's parent moved that step.

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

`godot4 --headless --path . --script tests/core.gd` checks the Godot extension (above) and
`godot4 --headless --path . --script tests/mtoon.gd` the MToon conversion, render modes, optional features, linear
colours and malformed input. `godot4 --headless --path . --script tests/skin_bindings.gd` checks skin bindings
(below). No asset downloads are needed.

## Reference harness

`reference/` is a Unity editor package that records how an avatar behaves under the VRChat SDK,
for comparing runtimes against it, as `docs/model.md` ("Measurement") describes. It needs
Unity 2022.3 with graphics (a display; Xvfb with software GL works), `com.vrchat.avatars` 3.7 or
later and [Av3Emulator](https://github.com/lyuma/Av3Emulator) 3.4 (MIT) in the avatar's project;
the SDK and the emulator are installed there, never copied here.

`reference/run.sh PROJECT OUT [SCENE] [AVATAR]` adds the package and the exporter to `PROJECT`, opens `SCENE`,
converts legacy DynamicBones as the exporter does, enters play mode in batch mode (the emulator runs the SDK's
build preprocess, so build-tool output is what is recorded, and the exporter writes the model from that same
build) and drives the avatar at a fixed 60 Hz step with one scripted input: rest, six seconds of
root and arm motion, settling, a quiet hold as long as the avatar's longest particle lifetime (so what the
motion emitted is gone), every expression-menu control swept from a rendered base (each toggle and button to its
non-default value, each radial at 0, 0.25, 0.5, 0.75 and 1, each puppet axis to its extremes, inside
its submenus), then the built-in parameters stepped (gestures, visemes, locomotion, tracking and
state flags). `UNITY` names the editor command. With no `SCENE`, it generates the fixture into
`PROJECT`'s `Assets/AWFixture` and records it, so point it at a scratch project. With `SCENE` `probe`, it generates and
records the probe there: the fixture's body with one three-bone chain per PhysBone field value (`Fixture.Probes` lists
them), so each field's effect has a reference trace of its own. The probe rests 15 s before its motion, so even its lightly damped chains start it settled. With `SCENE` `human`, it generates and records the
composition probe (`reference/Editor/HumanProbe.cs`), with `SCENE` `blend` the blend duration probe (`reference/Editor/BlendProbe.cs`), with `SCENE` `layering` the write-defaults layering probe (`reference/Editor/LayeringProbe.cs`), with `SCENE` `sampletime` the clip sample-time probe (`reference/Editor/SampleTimeProbe.cs`), with `SCENE` `rotation` the rotation probe (`reference/Editor/RotationProbe.cs`), with `SCENE` `dense` the dense clip probe (`reference/Editor/DenseProbe.cs`), with `SCENE` `crossfade` the crossfade probe (`reference/Editor/CrossfadeProbe.cs`), with `SCENE` `generic` the generic probe (`reference/Editor/GenericProbe.cs`), with `SCENE` `gridhold` the frame grid, fill and hold probe (`reference/Editor/GridHoldProbe.cs`), with `SCENE` `gridrule` the frame-grid rule probe (`reference/Editor/RuleProbe.cs`), with `SCENE` `genericrig` the generic-rig playable probe (`reference/Editor/RigProbe.cs`), with `SCENE` `generichold` the generic-rig hold probe (`reference/Editor/HoldProbe.cs`), with `SCENE` `genericweight` the generic-rig playable-weight probe (also `reference/Editor/HoldProbe.cs`) with `SCENE` `loopdrift` the Loop Pose drift probe (`reference/Editor/LoopDriftProbe.cs`) and with `SCENE` `feet` the Feet height basis probe (`reference/Editor/FeetProbe.cs`), all described below, in place of the motion, menu and built-in steps. With `SCENE` `dynamics` it records the dynamics probe (`reference/Editor/DynamicsProbe.cs`), below, whose contact passes follow the motion in place of the menu and built-in steps. With `SCENE` `hands` it records the hand collider probe (`reference/Editor/HandsProbe.cs`), also below, whose hand sweeps replace the motion, menu and built-in steps. With `SCENE` `curves` it records the curve probe (`reference/Editor/CurveProbe.cs`), also below, which plays no avatar and refuses `AVATAR` and `AW_STEPS`. With `SCENE` `writedefaults`, it generates and records the write-defaults fixture: the fixture's body under animator controllers built from the environment variable `AW_WD`, which lists each layer's controller, weight, transition time and states, and for each state whether it writes defaults and what it animates (`reference/Editor/WriteDefaults.cs` gives the grammar; `AW_WD='T:de|dHat=1;K:fSmile=50'` is a write-defaults-on toggle of the hat beside a layer that keeps none). The environment variable `AW_STEPS` (`Name=value,...;...`, each step setting those animator parameters and held `AW_STEP_FRAMES` frames, 20 unless set, with a render on its last) replaces the motion, menu and built-in steps after the rest; the write-defaults fixture, which has no menu, requires it, and the composition, blend duration, layering, clip sample-time, rotation, dense clip, crossfade, generic, frame grid, fill and hold, frame-grid rule, generic-rig playable, generic-rig hold, generic-rig playable-weight, Loop Pose drift, Feet height basis, dynamics and hand collider probes refuse it. It restores
`PROJECT`'s package manifest on exit and refuses an `OUT` that holds something other than an earlier recording.
Recordings started on one machine take turns in the order they were asked for, each waiting for those asked
before it, so callers need not coordinate; a waiting one prints how many are ahead and how long it has waited,
and one that dies frees its turn; a signal to the script stops its editor with it. `AW_TURNS` names the queue's directory (`/tmp/avatar-workshop-recordings` unless
set). `tests/recording_turns.sh` checks the order with a stand-in editor.

With the environment variable `AW_CLIPS` naming a recording of `SCENE`'s avatar, it records no steps. It enters play mode
as a recording does, so the emulator builds the same avatar, then plays each clip that recording's playables played
alone, in a one-clip playable graph on that avatar (foot IK and playable IK off), at every normalized state time
`clocks.tsv` held it, with `states.tsv` naming the state. A blend tree's leaves each play at the tree's state time.
`OUT` receives `clips.tsv` (each clip: number, name, length, whether it loops, its own cycle offset and mirror, then
its curves: `muscle:<name>` for a humanoid curve, `animator:<name>` for another animator curve on the root,
`<path>|<component>|<property>` otherwise), `played.tsv` (each frame from the first `states.tsv` holds: layer, current
or next state, clip, normalized time, and the state's cycle offset and mirror, `parameter` where one drives it), `samples.tsv` (each clip at each of its normalized times: the clip time set,
then for each curve the value the graph produced and `AnimationCurve.Evaluate` at that time within the clip, joined
by a comma) and `done`. The graph's humanoid values are read from its output stream as `values.tsv` reads them
(`-` for a muscle of a bone the avatar does not map); other curves are read from the object they bind (`-` when it
holds no value). A run fails if a played clip lacks a sample or the stream held no humanoid values for a humanoid
clip. `cargo run --release --example clips -- OUT RECORDING/avatar.glb [--frames FIRST-LAST]` in `avatar-core/` compares
each muscle sample with the core's sampling of the same clip alone.

`OUT` receives:

| File | Contents |
|---|---|
| `steps.tsv` | each step: index, first frame, frames, menu control, label, parameters set |
| `inputs.tsv` | what the harness's script does to the avatar on each frame, except the grabs, which `grabs.tsv` holds. No header; one row for each frame from the first frame of the first step: `frame, root, arms`, then a `<name>=<value>` field for each parameter set on that frame, in the order set (the name ends at the last `=`, with a tab, line break or backslash in it written `\t`, `\n`, `\r` or `\\`). `root` is the avatar root's world pose (`px,py,pz,qx,qy,qz,qw`); `arms` is the swing added to the upper arms after animation, two values in degrees about the root's Y and Z axes, mirrored for the left arm. Each is on the first row and on a frame that sets it, `-` otherwise, and holds until set again. The first row also sets every built-in parameter; before it the script does nothing. A recording fails (`error.txt`, a non-zero exit, no `done`) if the script sets something on a frame after that frame's row is written, and at its end if the file lacks a frame's row or lacks, on a step's first frame, a parameter the step sets |
| `params.tsv` | every animator parameter on each change, values printed round-trip exact |
| `states.tsv` | every playable layer's state and transition on each change |
| `dynamics.tsv` | each parameter the avatar's contacts and PhysBones write, as each playable's animator holds it just before evaluating, every frame from the one the graph is created on, keyed `<i>:<name>` (numbered as in `states.tsv`) and written on change; `params.tsv` shows these as the dynamics left them later in the frame. A recording fails if they were not read on a frame |
| `values.tsv` | node active, renderer enabled, PhysBone enabled (`physbone:<path>#<k>`, the k-th on that object), material slots, blendshape weights, animated material properties and animated transforms, and what the behaviours set: each playable layer's weight (`playable:<i>`, numbered as in `states.tsv`), each animator layer's weight (`weight:<i>:<l>:<name>`, numbered as in `states.tsv`), each animator parameter a clip curve drives (`curve:<i>:<name>`), the tracking of each body part (`tracking:<part>`), `locomotion:disabled` and the view position (`view`); each on change; `muscle:<curve name>` for every humanoid muscle, the body (`RootT`, `RootQ`) and the four IK goals, read from the animator's output stream before the pose is applied (`-` where the avatar maps no bone for the muscle, and for a muscle curve or field the animator holds no value of); and `constraint:`, `sender:` and `receiver:<path>#<k>:<field>` for every constraint and contact field a curve binds, `<path>` and `<k>` being the node the model records and its place among that node's components of the kind. A recording fails if the output stream holds no humanoid values on a frame, if a field it could read at setup has no value on a frame, or if the avatar has translation DoF and curves that drive it, whose values the output stream does not hold |
| `tips.tsv`, `chains.tsv` | every PhysBone leaf (with its endpoint; for the probe, every bone below each root) each frame, in metres from the avatar root along its axes (its scale ignored), and its rest chain length |
| `track.tsv` | the avatar root in the world, and its head, hands and hips in metres from the root along its axes, each frame, for replaying the same input elsewhere |
| `poses.tsv`, `drivers.tsv` | the input the SDK's dynamics read, taken each frame after animation and before its constraints, PhysBones and contacts run: every transform they read besides the bones they simulate (each PhysBone root's parent and scene root, every collider and contact, each constraint's target, its parent, its sources and world-up transform) in metres from the root along its axes (`root` rows, scales world), each written when it changes, and each PhysBone bone's local pose (`local` rows) whenever it differs from the pose the SDK left it in the frame before, which is when animation set it. Every humanoid bone is recorded the same way as a `root` row, so a replay can stand the body in the recorded pose. A row is `frame, space, transform, px,py,pz,qx,qy,qz,qw,sx,sy,sz`; the root itself is the empty path. `drivers.tsv` lists each transform with its roles (`humanoid:<HumanBodyBones name>` for a humanoid bone; `outside-sender` for a dynamics probe sender outside the avatar, its path its name, its shape, offsets, tags and contact player id in `meta.json`'s `outside_senders`; `standard-sender:<collider>` for the root transform of a standard contact sender Av3Emulator places, standing for VRChat's, on an enabled descriptor collider whose bone is in the avatar (a finger's on the bone's parent) when the avatar loads, a sender the build did not hold, with its collider, shape, radius, height, local position and rotation offsets, collision tags and the number of identical copies the emulator placed in `meta.json`'s `standard_senders`; a recording fails if such a sender matches no collider, a collider has none, its copies differ or one is gone); `moved-by-dynamics` marks one at or under a simulated bone or under a constraint target, whose recorded pose is the one before the dynamics moved it. A recording fails if any of them is gone or unposed on a frame |
| `humanoid.tsv` | each humanoid bone's local pose as the animator left it, taken each frame after animation and before the scripted arm motion and the SDK's dynamics, as a `local` row in `poses.tsv`'s layout with the bone's HumanBodyBones name in place of its path, each written when it changes. A recording fails if a bone is gone, or a frame has no humanoid poses |
| `constrained.tsv` | the output of the SDK's dynamics: each VRChat constraint's effective target taken each frame directly after the SDK completes its dynamics (`VRCAvatarDynamicsComplete`), as a `root` row in `poses.tsv`'s layout and a `local` row with its local position, rotation and scale, each written when it changes. A recording fails if a target is gone, or a frame has no constrained poses or none of the driver poses taken before them |
| `moved.tsv` | the poses the SDK's dynamics left on what its constraints and contacts read from PhysBones: each transform at or under a PhysBone bone that is a constraint's source, world-up object or target's parent, or a contact sender or receiver, taken at `constrained.tsv`'s stage as a `root` row in `poses.tsv`'s layout, each written when it changes. A recording fails if one is gone, or a frame has no constrained poses |
| `clocks.tsv` | each layer's current state's normalized time as its animator holds it, every frame from the one the graph is created on, one column per `<playable>:<layer>`, with `>` and the next state's during a transition; a recording fails if a layer holds no state time on a frame, and the replay fails if a frame has no row |
| `grabs.tsv` | empty but for the fixture, which after the built-in steps has its tail, antenna and left ear grabbed through the SDK's own grab call (a ray at the bone, as its editor grab helper casts), the held point moved and let go, the tail once posed; each row is `frame, event, bone, x,y,z`, the event `grab` (at the point the SDK grabbed), `miss`, `hold` (the held point that frame), `release` or `pose` |
| `render/`, `renders.tsv`, `camera.json` | a render at the end of every swept step from a fixed camera and light, and of each control's `base` step, which comes before the control's first swept step and holds what its sweep holds with the swept parameters at rest |
| `menu.tsv` | every menu control: indexed path, name path (control names joined by `/`), type, parameter |
| `time.tsv` | the time of the emulator's playable graph after each frame's evaluation, every frame from the one the graph is created on, so the first evaluation's delta shows; a recording fails if the graph is gone on a frame |
| `avatar.glb`, `census.json`, `export.txt` | the exporter's model and census of the build this recording runs, written as the last step of its build preprocess, with the build tools' generated names and ids (and, as the emulator skips the SDK's material locking, unlocked shaders); `cargo run --release --example census -- avatar.glb census.json` in `avatar-core/` checks one; `export.txt` lists what the model does not represent |
| `coverage.json`, `meta.json` | the avatar's feature counts; versions and totals, and as `harness` the commit of the avatar-workshop checkout `run.sh` ran from (`git describe --always --dirty`) |
| `compositions.json` | the composition, blend duration, layering, clip sample-time, rotation, dense clip, crossfade, generic, frame grid, fill and hold, frame-grid rule, generic-rig playable and generic-rig hold probes' and the dynamics probe's only (the dynamics probe's counts the constraints, receivers and senders with each setting `Coverage.DynamicsSettings` lists): for each composition the probe is authored to play on its own, how many clips (composition probe), states (clip sample-time, rotation, dense clip, frame grid, fill and hold, frame-grid rule, generic-rig playable and generic-rig hold probes), transitions and states (crossfade and generic probes) or states and transitions (the others) are authored to play it |

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

The composition probe is the fixture's body with controllers that play one humanoid composition at a time, so a
replay can tell apart how Unity's humanoid output stream (`muscle:` in `values.tsv`) composes the body and the four IK goals. Its int parameter `Case`
picks the composition and 0 plays the rest pose; every state a case enters mirrors on `Mirror`. The harness plays each case for
150 frames, first unmirrored and then mirrored, with 20 frames at rest after each. Through each case it ramps
`BlendX` from 0 to 1, `BlendU` from -1 to 1 and `BlendY` up and back down; in the case the Sitting playable plays it
toggles `Seated` every 8 frames, so that playable's 0.25 s blend never completes. The cases cover clips with and
without IK-goal curves, a clip binding `RootT.y` alone, and each root-motion setting changed alone from keeping the
original rotation, height and XZ on a walking body. They also play such clips in crossfades, in 1D, 2D and direct
blend trees, under masked override and additive layers, and in the Action, Additive and Sitting playables at partial
weight. The Base controller holds only one layer: an idle override layer holds the body parts its mask lets through
at a constant pose, so a clip under it would not play alone. Layered cases play in the Action controller at full
weight, whose override and additive layers stay at weight 0 until a case raises them; one case raises the override
layer and leaves it idle over a walking body, showing that effect. A recording also fails if a case never entered one
of its states, or if a playable it plays at partial weight held a weight strictly between 0 and 1 on fewer than 90% of
the case's frames. A recording fails if any composition `Coverage.Compositions` lists is missing from
`compositions.json`.

The blend duration probe is the fixture's body with an FX controller for measuring how long Unity takes a state
to last when its motion is a blend tree: `clocks.tsv` holds each state's normalized time, which advances each frame
by the frame's time over that duration. Its clips set blendshapes: zero-length ones hold a single key; three run
1 s, 2 s and 0.4 s; and an empty one has no curves, which Unity reports as one second long. An int expression
parameter `Case` picks a state, each playing one tree, and 0 picks a state without motion; the harness sets each
value for 150 frames with 20 frames of 0 after it, ramping `BlendX`, `BlendY` and `BlendU` as in the composition
probe. A 1D tree blends by `BlendX` and a 2D tree by `BlendU` and `BlendY`. A direct tree takes its weights from
controller parameters that are not expression parameters, so they keep their defaults of 0, 0.25, 0.5 and 1 (`W0`,
`W25`, `W50`, `W100`) and the asset fixes the tree's weight sum; two direct trees take `BlendX` and `BlendY`
instead, and two more with normalized blend values.

The cases:
- Direct trees of clips with length whose weights sum below, to and above one.
- Direct trees mixing zero-length clips with clips that have length: summing below, to and above one, either kind
  heavier, either kind at weight zero, and ten zero-length clips beside two with length.
- Direct trees of zero-length clips alone.
- Direct trees in direct trees: the inner or the outer weights summing below one, an inner tree of zero-length
  clips alone, and a tree nested twice.
- Direct trees with normalized blend values (the serialized `m_NormalizedBlendValues`, which build tools set on
  the trees they merge toggles into): the weight sums, the mixes, zero-length clips alone, and nested in and around
  direct trees without it.
- 1D and each kind of 2D tree over zero-length clips alone and mixed with clips that have length, and a 1D tree in
  a 1D tree.
- Direct trees of 1D trees and of a 2D tree, the form build tools give merged toggle layers, with three and eight
  1D trees of zero-length clips beside one clip with length.
- The empty clip: alone and beside zero-length clips in direct trees; beside clips with length in a direct tree,
  one with normalized blend values, a 1D tree and a direct tree of 1D trees.
- An empty clip and a zero-length clip, each as a state's whole motion.

A second layer plays four more trees in states without write defaults, with weights summing to at most one or
normalized: without write defaults a direct tree feeds each blendshape's value back scaled by its weight sum, and
at a sum of two the recorded values doubled every frame to infinity. Two more layers each play one tree as their
default state, so from the first frame and throughout: six zero-length clips at weight one beside a 0.4 s clip, one
with normalized blend values and one without, both with write defaults. A recording fails if a case never entered its state, or if
`compositions.json` counts no state for a composition `Coverage.BlendCompositions` lists.

The write-defaults layering probe is the fixture's body with one FX controller whose cases each isolate one way
write defaults act across layers or within one. Its clips hold blendshapes, each layer its own, and no tree feeds an
earlier case's value back into a shape it writes: Unity plays a shape's last value, from any earlier case, back into a
state without write defaults, even after its layer rested in one with write defaults.
Every layer rests in a state with write defaults and no motion. An int expression parameter `Case` picks the states a
case plays, and a bool `Pulse`, which the harness sets on frames 30 to 89 of each case's 150, starts and ends the part
of a case each item below names; the harness otherwise plays each case as the blend duration probe does (150 frames,
then 20 of `Case` 0, with the blend parameters ramped). The cases:
- A write-defaults toggle holding its shape while `Pulse` is set and returning to its state with no motion: alone; on
  a layer above or below one without write defaults, which through the case either holds a shape or sits in a state
  with no motion; and on the controller's first layer, below such a layer.
- A state without write defaults holding a shape until `Pulse` clears, then one without write defaults and with no
  motion.
- Zero-length transitions, when `Pulse` clears, into a state with no motion from a direct tree, a 1D tree, a direct
  tree of a 1D tree and a 2D tree, and from a direct tree into a state without write defaults.
- Direct trees without write defaults, playing while `Pulse` is set, whose leaves write some, all or none of the
  shapes the case's `Set` state, also without write defaults, sets before and after the tree; their weights sum to
  0.5, 1 and 1.25.
- Direct trees with normalized blend values whose weights sum below one, through the case, over clips with length,
  zero-length clips and both, and one such tree over clips with length in a state without write defaults.

A recording fails if a case never entered its states, or if `compositions.json` counts zero for a composition
`Coverage.LayeringCompositions` lists. The probe's recording is a reference trace of the core
(`avatar-core/fixtures/reference/layering-animator`).

The clip sample-time probe is the fixture's body with one FX controller playing looping clips whose one curve is the
identity: keys at (0, 0) and (L, L) with unit tangents on an animator parameter, so the parameter's recorded value
(`curve:` in `values.tsv`) is the time Unity sampled the clip at, beside the state's normalized time in `clocks.tsv`.
Each clip is 1, 10, … or 10⁷ s long and plays at speed 1, 0.5 or 2, or under `SampleSpeed`, a speed parameter that
keeps its default of 0.75; as a state's whole motion, as both leaves of a 1D tree held at its first threshold, or as
the one leaf of a direct tree at weight one; and from the graph's first frame, as its layer's default state, or later,
entered when case 1 starts. Each of these 192 has a layer and parameter of its own, so all play at once without
touching one another, and every later one enters on the same frame. The harness plays case 1 as the blend duration
probe plays its cases, then holds 3,000 frames. A recording fails if case 1 never entered one of its states, or if
`compositions.json` counts zero for a composition `Coverage.SampleTimeCompositions` lists. The probe's recording is a
reference trace of the core (`avatar-core/fixtures/reference/sampletime-animator`).

The rotation probe is the fixture's body with Base and Action controllers whose humanoid clips turn the body
(`RootQ`) and the four IK goals about X and Z as well as Y, so a goal's pitch and roll meet a turned body. As in
the composition probe, `Case` picks the composition, 0 plays the rest pose, every state a case enters mirrors on
`Mirror`, and the harness plays each case for 150 frames unmirrored and then mirrored, with 20 frames at rest after
each. A moving clip walks for two seconds while its body and every goal turn from rest to a rotation of their own;
a zero-length pose holds one key per curve at time zero, its body pitched 93° (with 20° of yaw and 15° of roll).
Each turns about all three axes under every root-motion setting the composition probe changes alone, and under the
combination emote packs use (rotation baked and based upon body orientation, height baked with an offset of −0.02,
XZ baked, as the SDK's `proxy_die` sets them); under the original settings and that combination it also turns, in Base at full weight, about
X alone and Z alone. Both clips, under both of those settings, play in the Base playable at full weight (its first
layer) and at partial weight (a second layer at weight 0.5, which idles on an empty clip and so gives no human pose
until a case plays there; no layer control reaches Base's layers), and in the Action playable at full weight and at
0.5. A pose under that combination whose body and goal quaternions are 0.7 of unit length, about as long as
`proxy_die`'s, plays in Base and in Action at full weight. A recording fails if a case never entered its state, if the Action playable held a weight strictly
between 0 and 1 on fewer than 90% of a partial case's frames, or if `compositions.json` counts zero for a composition
`Coverage.RotationCompositions` lists. Each is keyed by where it plays, whether it moves, the axes it turns about and
how its root-motion settings depart from the original ones, with `.unnormalized` after the short pose and
`.mirrored` after a mirrored state (`rotation.action_full+pose.axes_xyz.root_rotation.bake+root_rotation.body+root_height.bake+root_height.offset+root_xz.bake`
is the emote pose in Action at full weight). The probe's recording is a reference trace of the core
(`avatar-core/fixtures/reference/rotation-animator`).

The dense clip probe is the fixture's body with a Base controller playing humanoid clips and an FX controller playing
generic ones, one state per case, so `values.tsv` holds the value Unity samples each frame, on keys and between them,
beside the state's time in `clocks.tsv`. `Case` picks the state and 0 plays the rest pose (Base) or no motion (FX); the harness
plays each case for 150 frames with 20 frames at rest after each. Every clip is 3 s long, does not loop, and its
probed curves are keyed on its frame grid (its sample rate, 30 or 60) on every frame, on most frames (frames 3, 8, 13 and every fifth
after left out), on every other frame, or off the grid (its first and last frame and 0.37 of a frame
after each frame between); their keys take the same seeded values in the same order in every clip, with flat
tangents, legacy `Auto` tangents, `Clamped Auto` tangents, or broken
tangents set apart. Each clip keeps high-quality curves; those with `Auto` or broken tangents also play in a copy without
them (`m_UseHighQualityCurve` off, as some emote packs ship their clips). Only those two, so the 192 cases fit the
integer case parameter. A humanoid clip probes three muscles (`Spine Front-Back`,
`Left Upper Leg Front-Back`, `Right Arm Down-Up`) and holds every other muscle and the body at rest with keys at its
first and last frame; a generic clip probes the float parameter `Dense` and the `Smile` blendshape. Each clip plays at
speed 1 and at 0.5. A recording fails if a case never entered its state or if `compositions.json` counts zero for a
composition `Coverage.DenseCompositions` lists, keyed by kind, keying, sample rate, tangents, curve quality and speed
(`dense.humanoid.every_frame.rate_30.auto.quality_low.speed_1` is a humanoid clip keyed on every 1/30 s with `Auto`
tangents and without high-quality curves at speed 1). The probe's recording is a reference trace of the core
(`avatar-core/fixtures/reference/dense-animator`).

The crossfade probe is the fixture's body with one Base controller: an override layer that holds the rest pose
until a case plays a humanoid clip there, and above it an additive layer, idle in a state with no motion, whose
cases crossfade between clips that each subtract a reference pose. `Case` picks the case; the harness plays each for
150 frames with 20 frames at rest after each, sets `Pulse` on frame 30 and clears it on frame 90. The walks below
play first, as an idle additive layer goes on holding what its last case played. In an additive
case the source plays from the case's first frame and fades to the destination when `Pulse` sets; when it clears, the
destination fades back to the source over 0.5 s. The source sways the spine and chest over a looping 2 s, its body
turned 20° about Y from a reference at rest. Each destination bends spine, arm and leg and turns the body well away,
against a reference pose; how its body rotation offset (its clip's `RootQ` less its reference's, plus the unit
quaternion) lies against the source's names the case: in the source's hemisphere (`same`), in the other one
(`other`: the same turn negated), or with clip and reference both of negative w, the reference the very same
rotation (`negative_alike`) or another (`negative_pair`). Each fades over a fixed 0.5 s and over 0.4 of the source's
length. Three fades run 2 s (fixed, or normalised over the whole source) and are interrupted at their midpoint: the
destination may interrupt them, and when `Pulse` clears it fades to a third state, the destination of the other
hemisphere. In two more fades a generic property (the `Smile` blendshape) is written by only one side: the source,
or the destination. The override layer's cases play an asymmetric 2 s walk, mirrored (looping and not) and
unmirrored (looping), its time driven by the state's own clock, by `BlendX` (ramped from 0 to 1 through the case) or
by a parameter held at a value: 0, 0.25, 0.6 or 1 for the mirrored walks, 0 or 0.25 for the unmirrored one. A
recording fails if a case never entered one of its states, or if `compositions.json` counts zero for a composition
`Coverage.CrossfadeCompositions` lists: `crossfade.<hemisphere>.<fixed|normalized>`, with `.interrupted` after an
interrupted fade, `crossfade.generic.<source|destination>_only`, each counted only from a fade `Pulse` starts, so a
fade back or on cannot stand in for a missing case, and `scrub.<mirrored|unmirrored>.<loop|once>.<time>`
(`scrub.mirrored.loop.time_25` is the looping walk mirrored with its time held at 0.25). The probe's recording is a reference trace of the core (`avatar-core/fixtures/reference/crossfade-animator`).

The generic probe is the fixture's body with its own controller in the Base, Additive and FX playables, each with an
additive layer idle in a state with no motion; Base's holds the rest pose in an override layer beneath. `Case` picks
the case; the harness plays each for 150 frames with 20 frames at rest after each, sets `Pulse` on frame 30 and clears
it on frame 90. Each case probes a blendshape (`Probe<case>`, 15 at rest) or the Y position of a node (`Marker<case>`)
that no other case animates, as an idle additive layer goes on holding what it last added. Every additive clip
subtracts a reference pose that holds a shared property (the Z position of `Common<playable>`) at rest, and either holds the probed property off its default (`reference_with`) or not
(`reference_without`). A case plays the property alone, looping over 2 s, or in a fade both of whose sides write the
shared property and only one side the probed one: the source fades to the destination over 0.5 s when `Pulse` sets and
back when it clears. The FX controller's override layer then plays, in states that write defaults, clips at 30 frames per second, each once without
high-quality curves and once with them, keyed every third frame with seeded values: three seconds of the `Sampled`
node's position, quaternion rotation, Euler rotation or scale, or of the `Pout` blendshape; a position curve with
every key stepped, or every other one; a position whose X curve lies on the frame grid beside a Y curve 0.37 of a frame
off it; a position whose inner keys lie 0.0001, 0.0005, 0.002, 0.01 or 0.1 of a frame after the grid; and a 1.5 s clip,
looping and not, whose position curve ends 0.4 s before the blendshape curve that sets its length, so
the case samples it at and past its end. A clip animating some of the node's position channels holds the others at
rest, as a state writing defaults has Unity write 0 to the ones left out. A recording fails if a case never entered one of its states, or if
`compositions.json` counts zero for a composition `Coverage.GenericCompositions` lists:
`additive.<base|additive|fx>.<blendshape|transform>.<reference_with|reference_without>.<alone|source_only|destination_only>`,
a one-sided fade counted only from a fade `Pulse` starts, and `curves.<kind>.quality_<low|high>`, the kind read from the
clip's curves (`curves.threshold.offset_5e-4.quality_low` is the clip whose inner keys lie 0.0005 of a frame off the
grid, without high-quality curves). The probe's recording is a reference trace of the core
(`avatar-core/fixtures/reference/generic-animator`).

The frame grid, fill and hold probe is the fixture's body with its own controllers in the Base and FX playables; every
layer rests in a state with no motion that keeps what it last wrote, and Base's override layer holds the rest pose.
`Case` picks the case and `Pulse` is set and cleared as in the generic probe. The two body cases come first: in the Base
controller an additive layer adds a body offset (a turn of the body and a spine muscle against the rest pose) over an
override layer beneath that holds the body turned (`body.additive_over.turned`) or turns it over 2 s
(`body.additive_over.turning`). The FX cases follow in the order below, those without an additive motion first: while an FX
additive layer plays a motion it adds their rest value to generic properties no state writes (`GridOff` 50.04 to 65.04
when the first hold case starts), and a state without write defaults beneath it makes some of them grow each frame
(`GridOff` by 30 a frame in the last case), so that case comes last. An override layer plays, in states that write defaults, 3 s clips
without high-quality curves at 30 frames per second, keyed every third frame with seeded values, one per offset of
0.15, 0.2, 0.25, 0.3 and 0.37 of a frame and shape: the `GridOff` blendshape with every inner key that far after the
grid, alone (`alone`) or beside the on-grid `GridOn` (`beside_on_grid`); with only its 10th inner key (`one_key`) or its
10th and 20th (`two_keys`) off, beside `GridOn`; stepped, beside `GridOn` and `GridOnOther` (`stepped`); or a muscle
curve (Chest Left-Right) off the grid beside those two (`muscle`), which in the FX playable moves no muscle. A second
override layer plays a looping clip of a `Fill<case>` node's X position and Y scale, the node resting off zero and off
unit scale, in a state with and without write defaults. An additive layer then plays the `Hold<case>` blendshape and `Holder<case>` node's Y position against a
reference holding them: in `hold.idle.write_defaults_then_without` the motion leaves when `Pulse` sets for an empty state
that writes defaults, and that for one without when `Pulse` clears; in `hold.fade_out` the motion leaves when `Pulse`
sets for an empty state without write defaults, which fades over 0.5 s into a second motion when it clears. Last, the fill cases play again, with and then without
write defaults, under an additive layer above that plays a motion of a `Mover<case>` node in the same case
(`additive_above`). A recording fails if a case never entered one of its states, or if
`compositions.json` counts zero for a composition `Coverage.GridHoldCompositions` lists:
`grid.offset_<0_15|0_20|0_25|0_30|0_37>.<alone|beside_on_grid|one_key|two_keys|stepped|muscle>`, each read from the
clip's curves, `fill.<write_defaults|no_write_defaults>.<alone|additive_above>`, the two hold compositions and the two
body compositions. The probe's recording is a reference trace of the core
(`avatar-core/fixtures/reference/gridhold-animator`).

The frame-grid rule probe is the fixture's body with its own FX controller: one override layer that rests in a state
with no motion and, for each case `Case` picks, plays a 3 s clip without high-quality curves at 30 frames per second in
a state that writes defaults. Each clip's one curve that lies off the grid has every inner key 0.37 of a frame after it
and ClampedAuto tangents, and its key values span a set range exactly. Each case varies one property of that curve:
- its range: 1, 100 and 0.01 at the same key times and shape (keyed every third frame, the frame grid, fill and hold
  probe's seeded `GridOff` values) (`amplitude_<1|100|0_01>`);
- its key spacing: every frame, every second and every third frame over a range of 10, each the bent ramp below at a
  frame-line error of 0.045 (`spacing_<1|2|3>`);
- its frame-line error: every third frame over a range of 2, a ramp whose alternate keys bend up and down by as much
  as makes the error 0.03, 0.04, 0.045, 0.05 or 0.06 (`error_<0_030|…|0_060>`).

The frame-line error (`Fixture.FrameLineError`) is the largest distance, over the curve's keys, between a key's value
and the straight line through the curve's values on the two frames around it, over the range of its key values. Each
of those cases binds the curve to the `RuleShape` blendshape, the X position of a `RuleMover` node, the Y scale of a
`RuleScaled` node or the animator parameter `Rule` (`rule.<blendshape|position|scale|parameter>.<variant>`). Two
more clips hold a blendshape curve and a position curve, keyed every third frame over a range of 1, one off the grid
and the other on it (`rule.mixed.<blendshape|position>_off`). Each state's name ends in its clip's frame-line error
(`err_0_0411`). At a range of 0.01 a curve's frame-grid differences fall under the core's 1e-3 value tolerance, so those cases show framing only in the recorded values themselves. A recording fails if a case never entered its state, or if `compositions.json` counts zero for a
composition `Coverage.GridRuleCompositions` lists, each read from the clip's curves. The probe's recording is a
reference trace of the core (`avatar-core/fixtures/reference/gridrule-animator`).

The generic-rig playable probe is the fixture's body and skeleton under a generic avatar (`AvatarBuilder.BuildGenericAvatar`),
so no playable carries a human pose. The Base, Additive, Gesture, Action and FX playables each get a controller of four
layers: `Full` (the first layer), `Partial` (an override layer at weight 0.5), `Override` (an override layer at 0.5) and
`Additive` (an additive layer at 1), each resting in a state with no motion and without write defaults. Each value of
`Case` plays, without write defaults, a looping 2 s clip that animates a blendshape, the Y position of a node and a float
animator parameter of its own (`Rig<n>`, `RigNode<n>`, `RigParam<n>`, the parameter defaulting to 0.25) at one weight:
in `Full` (`full`), in `Partial` (`partial_layer`), or in `Full` with its playable lowered to 0.5 by a playable-layer
control as it enters (`partial_playable`; VRChat's control reaches every playable but Base, and the harness fails a case
whose playable holds a partial weight on fewer than 90% of its frames). Each plays with nothing above it (`alone`), with the `Override` layer
holding the same three properties at other constants (`under_override`), or with the `Additive` layer adding a 1.5 s
looping offset to them (`under_additive`): `rig.<playable>.<blendshape|position|parameter>.<context>.<weight>`. The
`Full` layer's rest state returns its playable to its default weight (0 for Action, 1 for Additive, Gesture and FX; Base's has no control); each other Action
case raises the Action playable to 1 as it enters, and the `Partial` layer's rest state lowers it again. The
emulator mixes the whole Additive playable additively, so its cases measure the playable's output added at the mixer.
VRChat's avatar descriptor editor drops the Additive and Gesture playables from an avatar without a humanoid rig; the
probe sets the descriptor's layers directly, so these two record what the emulator plays, which an avatar built in the
editor would not carry. A recording fails if the avatar is humanoid (`Coverage.CountRigCompositions` then counts
nothing), if a case never entered its states, or if `compositions.json` counts zero for a composition
`Coverage.RigCompositions` lists. The probe's recording is a reference trace of the core
(`avatar-core/fixtures/reference/genericrig-animator`).

The generic-rig hold probe is the same body under a generic avatar, with the Additive, Gesture, Action and FX playables
each given a controller whose first layer, `Full`, plays each case and whose every other layer rests at weight 0 until a
case raises it with an animator-layer control, so each case's layers are the only ones that compose (the Base playable
gets a controller with one empty layer). Every state is without write defaults. Each case plays in `Full` a looping 2 s
clip animating, of its own, a node's Y position, Euler rotation (`localEulerAnglesRaw`, Y moving) and Y scale, a
blendshape and a float animator parameter (`HeldNode<n>`, `Held<n>`, `HeldParam<n>`, the parameter defaulting to 0.25).
The hold cases, in the FX controller, each have a layer of their own that the case raises to 0.25, 0.5 or 0.75 as it
enters a state with no motion (`before`: the layer has never played a motion, but binds the five properties through a
0.5 s clip in a state nothing enters), or that 0.5 s clip, which then moves on to a state with no motion (`after`).
Each plays alone, or under an additive layer of its own that the case raises to 1 as it plays a 0.75 s offset of the
five properties and then moves on to a state with no motion (`under_additive`): `hold.<property>.<w25|w50|w75>.<before|after>.<alone|under_additive>`. The playable-weight cases
lower their playable to 0.25, 0.5 or 0.75 with a playable-layer control as their `Full` state enters, alone or under
a layer of its own the case raises to 0.5 while it holds the five properties at other constants (`under_partial`):
`playable.<playable>.<property>.<w25|w50|w75>.<alone|under_partial>`. `Full`'s rest state returns its playable to its
default weight (0 for Action, 1 for the others); every other layer's rest state returns that layer to 0. A recording
fails if the avatar is humanoid, if a case never entered its states, if a playable-weight case's playable held a
partial weight on fewer than 90% of its frames, if a layer a case raises held that weight on fewer than 90% of its
frames, or if `compositions.json` counts zero for a composition
`Coverage.HoldCompositions` lists. The probe's recording is a reference trace of the core
(`avatar-core/fixtures/reference/generichold-animator`).

The generic-rig playable-weight probe is the hold probe's playable-weight cases of the Additive, Gesture and Action
playables alone: no hold case, no additive-blended layer, and an FX controller whose one layer rests in a state with no
motion, so FX never plays a motion: `playable.<additive|gesture|action>.<property>.<w25|w50|w75>.<alone|under_partial>`.
It fails as the hold probe does, if `compositions.json` counts zero for a composition `Coverage.WeightCompositions`
lists, or if a state in the FX controller or in an additive-blended layer of any controller has a motion. The probe's recording
is a reference trace of the core (`avatar-core/fixtures/reference/genericweight-animator`).

The Loop Pose drift probe is the rotation probe's body, with a Base and an Action controller of its own playing one
clip shape: a looping 1 s walk under
Loop Pose whose body ends turned from its start by 10, 45 or 120 degrees about X alone, Y alone, Z alone, the diagonal
of X and Y, or the diagonal of all three, higher and moved in XZ, its IK goals turned with it from rests of their own,
their positions ending where they started.
Each plays with its rotation baked or not and based upon body orientation or the original, height and XZ unbaked and based upon the original, in
Base at full weight and in Action at a playable weight of 0.5, each unmirrored and mirrored:
`loopdrift.<base_full|action_partial>+axes_<axes>.deg_<degrees>.<setting>[.mirrored]`, the turn read from the clip's
first and last body rotation. A recording fails as the rotation probe's does, or if `compositions.json` counts zero for
a composition `Coverage.DriftCompositions` lists. The probe's recording is a reference trace of the core
(`avatar-core/fixtures/reference/loopdrift-animator`).

The Feet height basis probe is the drift probe's body and contexts with looping clips whose height is based upon Feet,
rotation and XZ unbaked and based upon the original: the drift probe's walk, shortened to a fifth of a 150-frame case so
a case plays five cycles, and a zero-length pose, each with its body and IK goals turned by 10, 45 or 120 degrees about
X alone, Y alone, Z alone or the diagonal of all three, with Loop Pose on and off, and its height left unbaked beside
the same clip with its height baked:
`feet.<base_full|action_partial>+<moving|pose>.axes_<axes>.deg_<degrees>.<setting>[.mirrored]`, the turn read from the
clip, a pose's from identity. A recording fails as the rotation probe's does, or if `compositions.json` counts zero for
a composition `Coverage.FeetCompositions` lists. The probe's recording is a reference trace of the core
(`avatar-core/fixtures/reference/feet-animator`).

The dynamics probe is the fixture's body with a constraint or contact for each setting value, each constraint on a
target of its own: each of the six constraint kinds with one source and with two weighted ones (the hands, or for
scale two transforms under them whose scales differ, one animated), each axis switched off alone, an at-rest value
under a partial weight, an offset, and local solving (under the chest); aim under each of its five world-up types
and with other aim and up axes; look-at with a roll and with an up object; position and parent constraints whose
freeze-to-world an FX clip toggles, each with and without rebaked offsets; a parent constraint following the tip
of a PhysBone chain on the head, another carrying a PhysBone chain, and three constraints chained one on another.
Its fourteen receivers stand in a row in front of the body, one per setting: each receiver type as a sphere and as a
capsule, allowing self only, others only and nobody, local only, one collision tag, a hand's tags, a tag no sender
carries, and a minimum velocity of 1 m/s. After the motion, the avatar's own sender (an FX clip, while the
`SelfPass` parameter is set) crosses the row and back twice, then two senders outside the avatar with another contact
player id, a sphere standing for another avatar's part and a capsule carrying a hand's tags, each cross it at 0.5
and at 3 m/s, their poses in `poses.tsv`. A recording fails if `compositions.json` counts zero for a setting
`Coverage.DynamicsSettings` lists.

The hand collider probe is the fixture's body with six chains of three 0.1 m segments hanging in a row in front of it, one per
collision filter setting: allow collision on, off (its filter allowing both), and filtered to self only, others only,
both and nobody. Its descriptor's only enabled colliders are the left hand's and the left index finger's, custom. The
SDK's editor simulation puts no PhysBone collider on the descriptor's hands (the emulator extracts its senders only;
a recording fails if it left any on those bones, `placed_hand_colliders` in `meta.json`), so global PhysBone colliders stand in for
them: two on the left hand, where the core puts the hand's and the finger's colliders (their standard senders' places;
that the client's PhysBone hand colliders sit there too is assumed, not measured here), and one outside the avatar
with another player id, for another player's hand (`outside_hands` in `meta.json`, its poses in `poses.tsv`). After a
300-frame rest the wearer's left hand, placed after animation, sweeps along the row through the chains' third bones
and back at 0.5 m/s, then the other player's hand does, then both from opposite ends, each followed by 120 frames away.
A recording fails if `compositions.json` counts zero for a setting `Fixture.HandSettings` lists.

The curve probe plays no avatar. It builds one clip per curve, its only curve on a transform's X position, and
samples it at seeded random times through an `AnimationClipPlayable`, beside `AnimationCurve.Evaluate` at the same
time: identity curves at the sample-time probe's eight lengths, looping, then 80 random curves of two to five keys
and lengths up to 10⁷ s, every other one looping, 400 samples each; a looping clip is sampled up to three
lengths in, one that does not loop up to half a length past its end. It writes `curves.tsv` (each curve's loop setting and keys as time, value, in and out tangent),
`samples.tsv` (curve, time, the clip's value, `Evaluate`'s) and `done`. Its recording is a reference trace of the
core (`avatar-core/fixtures/reference/curves`).

`reference/check.sh PROJECT OUT [SCENE] [AVATAR]` records the avatar (the fixture by default) twice and requires every output but the logs (`unity.log`, `export.txt`),
`tips.tsv`, `poses.tsv` and `constrained.tsv` to be byte-identical, PhysBone tips and every driver pose value within 1e-5, and
constrained positions and scales within 1e-6 and rotations within 1e-4 degrees (`reference/poses.py`, Python 3.10 or later).

A runtime replays the recording from `track.tsv`, `poses.tsv`, `values.tsv`, `grabs.tsv` and `steps.tsv` and writes `tips.tsv` and `render/`
in the same layout; `reference/compare.py REFERENCE RUNTIME` (Python 3 with numpy and Pillow) scores
it: PhysBone tip RMS error over chain length (a leaf or frame the runtime lacks counts as unmoving), the
fraction of menu controls that match, and mean image error. A control matches when the runtime's
`menu.tsv` lists its name path in the first column, before a tab (names compare with runs of whitespace
collapsed, so a name may span lines), and each of its swept renders changes the same region from the
control's base render as the reference's (intersection over union of changed pixels at least 0.5, or both unchanged); a control
the runtime does not offer fails. It writes every source control's result to `compare-controls.tsv` and each
swept step's to `compare-steps.tsv` in `RUNTIME`.

## License

Original code is available under either [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option. Third-party code, if added later,
retains its original license notices. The MToon shader follows three-vrm's MToon 1.0 shader and
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
