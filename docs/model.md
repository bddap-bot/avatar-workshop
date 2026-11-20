# Avatar model

One data model for creator-made avatars, designed from what VRChat avatars carry. VRM is one importer into it.
Every runtime reads only this model. This document is the contract the importers, the evaluation core and the
renderers implement.

## Why VRChat comes first

A VRChat avatar carries far more than VRM can hold. Squeezing it through VRM loses most of that, while VRM fits
inside the VRChat feature set with little loss. A pipeline built VRM-first (VRM file, plus side extensions for
whatever did not fit) has these losses, and they show as visible faults:

| VRChat feature | VRM-first representation | Loss |
|---|---|---|
| PhysBone (pull, spring/momentum, stiffness, gravity + falloff, immobile, limits, stretch/squish, curves, multi-child type, endpoint, grab/pose/collision filters, parameters) | four spring-bone scalars per joint (stiffness, drag, gravity power, hit radius) run by the VRM Verlet formula | the PhysBone solver never runs; authored gravity becomes a force or 0; no immobile, so chains swing with every move of the body; no limits; per-joint curves flattened; trees split into chains with synthetic endpoint bones; everyone may grab everything |
| PhysBone colliders (sphere, capsule, plane, inside, rotation) | VRM sphere/capsule | planes become spheres; inside colliders push out; rotated capsules wrong |
| Contacts | none | receiver parameters never driven |
| Constraints (parent, position, rotation, aim, look-at, scale) | none in VRM; a side extension with three kinds | rotation/aim/scale lost |
| Playable layers and animator controllers (transitions with durations, blend trees by algorithm, layer masks, additive layers, write defaults, state behaviours) | a side extension holding the FX layer with instant transitions and one 2D blend | crossfades snap, 2D blends weigh wrongly, Gesture/Action layers ignored |
| Built-in parameters (gestures, viseme, voice, IsLocal, velocity, grounded…) | none | gesture faces, viseme mouths and IsLocal gates never run |
| Expression parameters and menu (saved, synced, defaults; toggle, button, radial, two/four-axis, submenu) | side extension; `saved` ignored; buttons as fixed pulses | settings do not persist as authored; controls behave differently |
| Build-time components (avatar build tools that merge menus, toggles and controllers at upload) | partly reimplemented by hand | menus missing what the uploaded avatar has |
| Animated material properties and material swaps | a texture atlas with hue/colour/tone/decal regions; swaps as UV jumps | emission, matcap, rim, dissolve, alpha and most other animated properties dropped; swaps cannot change shader state |
| Per-material shader state (toon shader families' saved properties) | one atlas material with base colour only | normal, emission, matcap, rim, outline, two-sided and blended alpha lost |
| Renderer material slots | inferred from the source mesh file's first-use order and names | wrong material on a submesh |
| Visemes (15), eyelids, eye look | VRM expression presets and look-at range maps, or nothing | lip sync and blink lost or collapsed |
| Initial visibility (the authored active state, then the controller's default states) | three encodings: omitted nodes, node-visibility extension, a side list | hidden-by-default props drawn when a reader honours the wrong encoding |
| Humanoid and rest pose | VRM bone names; T-pose assumed | A-pose avatars stand differently per runtime |
| Mesh (8+ bone weights, every UV set, vertex colours, all blendshapes with normals) | 4 weights, one UV set, blendshape budget baked into vertices | the importer edits meshes |

## Principles

- The model holds VRChat semantics as authored: field names and units follow the VRChat SDK and Unity, not VRM.
- The source of truth for a VRChat avatar is the avatar the SDK uploads: after the SDK's build preprocess, with
  build-time tools applied. The importer reads that, not the raw project, so it never reimplements a build tool.
- One encoding per fact. Nothing is stored twice in different shapes.
- The importer never edits a mesh: every vertex, weight, UV set, colour and blendshape arrives unchanged.
- A per-asset fault is fixed in that asset or its importer, never by a runtime override that touches every avatar.
- Anything the runtime does not support is listed in the file's `unsupported` report and shown to the user, never
  dropped silently.
- Capabilities, not identities: the runtime chooses fidelity from measured cost, never from a device or avatar name.

## File

A glTF 2.0 binary (`.glb`) with one extension, `AW_avatar`, on the root. glTF carries what it is good at:
nodes, meshes (one primitive per renderer material slot, in slot order), skins with every joint set,
morph targets with normals, images and samplers. Each glTF material is a plain PBR fallback for generic viewers;
the shading the runtime uses is the `AW_avatar.materials` record of the same index.

Every position, direction and rotation is in glTF's right-handed space. That covers the glTF nodes, meshes and
skins, every `AW_avatar` record, and the clip curves that animate them. Unity's left-handed values map into it by
mirroring X:
- positions, directions and vertex deltas negate X;
- quaternions and Euler angles (Unity's ZXY order) negate Y and Z, and a roll negates;
- matrices are conjugated by the mirror;
- tangents become `(-x, y, z, -w)`, and triangle winding reverses.

These stay as Unity holds them:
- scales;
- humanoid curves (muscles, root and IK goals) and the humanoid rest values, which are in Unity's humanoid space;
- a PhysBone's per-bone curves (`curve` on a field, the limit rotation's `xCurve`/`yCurve`/`zCurve`), which scale
  its mirrored value;
- blendshape curve values, on Unity's scale where a shape's last frame weight is fully applied. A node's glTF
  `weights` divide by that weight, so 1 is fully applied.

Unity UV channel `n` is glTF `TEXCOORD_n`: its first two components, with `v` replaced by `1 - v`. Material tiling
and offset still apply to Unity's `v`. A channel's third and fourth components keep Unity's values in
`_TEXCOORD_n_ZW`, and a one-component channel is `_TEXCOORD_n`. A curve keeps Unity's stepped keys as infinite
tangents, written `"Infinity"` and `"-Infinity"`.

`AW_avatar` sections. Node, mesh, material and texture references are glTF indices; a blendshape is named
by its mesh's `extras.targetNames` entry.

| Section | Contents |
|---|---|
| `source` | importer name and version, source format, licence text and author as found in the source (absent when the source has none; never invented) |
| `humanoid` | Unity HumanBodyBones name → node; the source's rest pose as given (no assumed T-pose); muscle limits from the human description; the body and the four IK goals as Unity's humanoid stream holds them with every muscle at rest |
| `descriptor` | view position, lip-sync mode and the 15 viseme blendshapes, eyelid mode and shapes, eye bones and their rotation limits; the 14 body colliders (head, torso, hands, feet, four fingers a side) as stored: state (automatic, custom or disabled), bone, radius, height, position and rotation offsets. Each is a capsule at its offsets on its bone, a finger's on its bone's parent at its position plus the bone's local position, as Av3Emulator places a standard contact sender on each enabled one with a bone, tagged by part and side |
| `renderers` | per mesh node: initially enabled, material slots |
| `nodes` | per node: active as the uploaded avatar holds it; the evaluation core's animator applies the default states |
| `materials` | per material: shader family (`poiyomi`, `liltoon`, `vrchat-mobile`, `mtoon`, `standard`, `unknown`), version, its saved properties as a typed map (floats, colours, vectors, texture references with tiling/offset, keywords) and each property its shader declares at the value the material resolves for it; render queue, cull, blend and depth state |
| `physbones` | per component: root, ignored nodes, whether another PhysBone's root ends the chain, endpoint, multi-child type, integration type and version, pull/spring (momentum)/stiffness/gravity/gravity falloff/immobile and immobile type, each with its curve, limit type and angles and rotation, radius with curve, max stretch, stretch motion, max squish, collider indices, allow collision/grabbing/posing filters, grab movement, snap to hand, parameter name, is-animated, reset-when-disabled, initially enabled |
| `colliders` | per collider: node, shape (sphere, capsule, plane), position, rotation, radius, height, inside bounds, bones as spheres |
| `contacts` | senders and receivers: node, shape, radius, height, position, rotation, collision tags, allow self/others, local only, receiver type (constant, on enter, proximity), parameter, min velocity |
| `constraints` | one schema for the six kinds (parent, position, rotation, aim, look-at, scale): target node, sources (node and weight, or a fixed world matrix), at-rest and offset values, axis masks, weight, world-up settings, freeze-to-world, solve-in-local, initially enabled |
| `parameters` | expression parameters: name, type, default, saved, synced; built-in parameters are named by the model, not listed |
| `menu` | controls as authored: name, type, parameter, value, sub-parameters, labels, icon, submenu |
| `layers` | the playable layers (base, additive, gesture, action, FX, sitting, T-pose, IK pose) each with the controller and mask it runs (the SDK's default controller and imposed mask where the avatar keeps the default): layers with weight, blending (override/additive), avatar mask and sync source; state machines with sub-machines kept; states with speed and its parameter, cycle offset, mirror, motion, write defaults, behaviours; transitions with conditions, exit time, duration (fixed or normalised), offset, interruption source, ordered interruption, can-transition-to-self; blend trees with their algorithm (1D, 2D simple directional, 2D freeform directional, 2D freeform cartesian, direct) and children |
| `behaviours` | parameter drivers (set, add, random, copy with ranges, local only), tracking control, locomotion control, playable and animator layer control, temporary pose space; play-audio reported unsupported |
| `clips` | length (zero for a clip without curves, which Unity reports as a second long and counts as none in a blend's duration), sample rate (the frames per second of its frame grid), whether it keeps high-quality curves (Unity's `m_UseHighQualityCurve`: without them, a curve no two of whose keys lie less than a frame apart, a transform property's components counted as one curve, is sampled on its frames and linearly between them, a stepped curve as authored), loop, cycle offset, additive reference pose (a clip and a time), and on a humanoid clip its mirror, loop pose and root-motion settings (bake into pose and based-upon for rotation, height and XZ, and the rotation and height offsets) as Unity's clip settings hold them; and curves with Hermite keys (value, in/out tangents and weights). Each curve binds a typed property: node active, renderer enabled, blendshape weight, material property by name (on the first of the renderer's materials whose shader declares it; a texture's `_ST` tiling and offset by the texture's name), material slot (a real material swap), node translation/rotation/scale, humanoid muscle, animator parameter, physbone/constraint/contact field |
| `unsupported` | every source component or property the importer could not represent, by kind and count, e.g. particles, audio sources, unknown shaders |

The JSON schema is generated from the core's types, so the schema, the reader and the writer cannot drift.

## Evaluation core

One implementation, in Rust, of everything that turns a model plus inputs into a pose. Every runtime links it:
natively, as a Godot extension, or compiled to WebAssembly. A runtime does not reimplement any part of it.

- Parameters: expression parameters with authored defaults and saved/synced semantics, plus the built-ins,
  which the host drives from its inputs: `GestureLeft/Right` and their weights from hand tracking or controller
  pose, `Viseme` and `Voice` from speech, `IsLocal`, `VelocityX/Y/Z`, `AngularY`, `Grounded`, `Seated`, `AFK`,
  `TrackingType`, `VRMode`, `Upright`, `ScaleFactor` and the rest. An input the host lacks keeps VRChat's
  documented default and is reported.
- Animator: Unity state-machine semantics, including crossfades, interruption, layer masks, additive layers,
  write defaults, every blend-tree algorithm and Hermite clip sampling; behaviours run as VRChat runs them.
- PhysBones: VRChat's documented semantics on a fixed timestep, in the avatar's own space and scale, with
  immobile, limits, gravity and falloff, stretch and squish, curves, colliders of every shape, grab and pose
  with their filters, and the `_IsGrabbed`, `_IsPosed`, `_Angle`, `_Stretch` and `_Squish` parameters.
- Constraints and contacts: all six constraint kinds; contacts between the avatar, other avatars and hands, driving
  receiver parameters.
- Output per frame: node local transforms, blendshape weights, node active and renderer enabled states, material
  property overrides and slot choices. Renderers apply it; they hold no avatar logic.

## Importers

- **VRChat avatars: a Unity editor exporter.** Creators already build their avatars in Unity. The exporter runs in
  the avatar's project, interactive or batch mode, after the SDK's build preprocess, and writes the model through
  Unity's own APIs: meshes and their submesh-to-material slots as Unity holds them, every animator controller,
  clip and curve with tangents, materials' saved properties and their shaders' declared properties, and every VRChat component. Legacy DynamicBones are
  migrated with the SDK's own conversion first. Nothing is inferred from file names or from re-importing the
  source mesh file.
- **VRM 0.x and 1.0.** VRM maps into the model: humanoid to HumanBodyBones, spring bones to PhysBones (VRM's
  stiffness, drag and gravity converted to the PhysBone parameters whose step equals VRM's, measured against a
  VRM reference runtime, three-vrm), MToon to an `mtoon` material record, expressions to visemes, eyelids and animator clips.
  VRM 0.x quirks (coordinate flip, renamed thumbs, its MToon properties) live only here.

## Measurement

A reference harness runs the same avatar in Unity with the VRChat SDK's editor simulation of PhysBones and
contacts and an open-source avatar emulator for the animator and menu. It records traces under scripted inputs:
body and hand motion, every menu control swept, built-in parameters stepped. The core's output on the same inputs
is compared trace for trace: PhysBone tip positions relative to chain length, parameter and state sequences, node
states, blendshape weights, material property values; renderers are compared by image at matched camera and light.
A change that moves a metric away from the reference does not land. The harness is `reference/`, described in the README.

### Animator coverage

The core's animator replays the fixture and every corpus avatar's recording with no wrong parameter, state, state
time or muscle, constraint and contact binding sample, every node, blendshape and material value within 1e-3, and
every menu control matching. `avatar-core/tests/animator.rs` checks each listed semantic against the fixture's trace
and each probe against its own.

Probe residuals, each held to its count by its test, with no corpus avatar showing them:

| probe | residual |
|---|---|
| generic-rig playable | 335,400 values |
| generic-rig hold | 179,330 values |
| frame-grid fill and hold | 10,620 values, 838 pose bindings |
| frame-grid rule | 2,040 values, all bindings an earlier case left held |
| additive crossfade | up to 150 values in each of the two cases fading a generic property only one side writes |
| humanoid composition | up to 134 pose bindings, none after a case's first frame |

The generic-rig playable and hold probes still disagree with Unity about what a layer without write defaults holds
under an additive layer: a held blendshape under a playing additive layer reads part of the way to its default, and
an idle layer that held a node position while it played repeats a value that already carries the additive offset.

No recording shows these, so the core claims nothing for them:

- Blend trees: leaves filling different channels of one node; direct trees whose weights sum above one; goal offsets
  of a blend-tree state in an additive layer.
- Curves: muscle components judged together; a stepped component beside smooth ones of one transform property; a
  rotation group; a transform property whose components' keys differ by an ulp.
- Crossfades: a snap from a source without write defaults; a transition from an interrupted pose; a humanoid snap on
  the Base or Action playable. For the source negated when only it keys a rotation in part: a destination keyed in
  part, generic rotations and the Action playable. For an Action crossfade negated when both its rotations lie
  opposite the value beneath: another playable, an interrupted crossfade and a generic node rotation.
- Root motion: XZ baked with the rotation unbaked under Loop Pose on a clip that yaws; foot goals whose own Loop
  Pose drift is non-zero under the unbaked Feet height.
- Additive playable on a generic rig: a turned rotation beneath it at partial weight; the composed rotation's hemisphere under a later
  partial override playable; rotations other controllers bind but it does not.
- Generic rigs: an additive layer repeating parameters, or offsetting a parameter whose clip does not start at zero,
  or one no override layer covers; a second holding layer; discrete properties held across controllers; a hold below
  full layer weight updating the memory; direct trees and interrupted transitions without write defaults holding other
  controllers' properties; a material slot under a partial playable; a playable at weight 0.
- Behaviours: a normalized pose-space delay on a state whose speed is not 1, still counted in seconds.

### Constraint and contact coverage

The core's constraints and contacts replay the fixture, the dynamics probe and every corpus avatar's recording,
with what they read where PhysBones put it taken from `moved.tsv`. Every constrained target lies within 1e-4 m
and 0.1° of the SDK's on every frame (largest differences 1e-5 m and 0.056°), and every receiver parameter equals
the reference on every frame. `avatar-core/tests/dynamics.rs` checks every probe setting, the fixture and the
standard senders. What the recordings showed:

- Order: constraints whose sources, world-up object or target's parent sit at or under a node a PhysBone moves, or
  under such a constraint's target, run after PhysBones; the rest before them; within each, a constraint after
  those whose targets move what it reads, constraints sharing a target in component order. Contacts read the final
  poses.
- A constraint or receiver first runs the frame after it becomes enabled and active; a receiver not running leaves
  its parameter as the animator holds it.
- Position and parent constraints sum the weighted world positions of their sources, dividing by the total weight
  only past one, so a total below one pulls toward the world origin; aim and look-at aim at that point. Rotation
  and parent constraints blend their sources' rotations normalised by the total; scale constraints take the
  weighted geometric mean of the sources' world scales. The global weight blends from the at-rest value: linearly
  for position and scale, by a normalised linear blend for rotation. Offsets apply before that blend.
- An axis a constraint does not affect keeps the target's animated value. Masking passes rotations through Euler
  angles in Unity's order, and a rotation whose X angle's sine lies within 2e-6 of one loses its flip at the
  gimbal lock.
- Aim with no world up turns the aim axis by the shortest arc from the world's axis; look-at's up object gives the
  up direction toward its position.
- Freeze to world holds the affected channels at their world pose from the frame it froze; a position or parent
  constraint unfreezing with rebaked offsets keeps the target's local pose of that frame and follows the sources
  from it.
- A proximity receiver reads zero on the frame a sender enters; on-enter fires for one frame when the sender's
  speed since the previous frame reaches the minimum velocity.
- The avatar's own senders include a standard one on each enabled descriptor collider, as Av3Emulator places
  them: a capsule with the collider's dimensions and offsets on its bone, tagged by part and, for hands, feet and
  fingers, by side (`Hand` and `HandL`, `FingerIndex` and `FingerIndexL`); a finger's sits on its bone's parent,
  moved by the bone's rest position.

Not measured:

- Where VRChat runs a PhysBone that sits under a bone a constraint after PhysBones turns (one chain each on corpus
  2 and 5). The replay takes that chain's transforms from the recording, so the targets' bar holds whichever order
  VRChat uses; the core's order runs every PhysBone before those constraints, and how far a host's chain then lies
  from the SDK's is not measured.
- Whether VRChat's finger senders also carry `Finger` and `FingerL` or `FingerR`, which VRChat's tag list defines as
  any finger. Av3Emulator's do not, and every receiver of the avatar's own fingers in the corpus also listens for a
  tag the emulator places, so no recording tells the two apart.

## What goes

Code that exists because VRM came first:

- spring-bone readers (VRM 1.0 and 0.x) as a runtime input, the VRM Verlet spring solver, and the engine's
  VRM-shaped spring simulator; synthetic endpoint bones and tree-to-chain splitting
- one collider group per chain; sphere stand-ins for planes
- side extensions on a VRM file for expressions and constraints, and the three visibility encodings
- the texture atlas, its hue/colour/tone/decal regions and swaps-as-UV-jumps
- material slot inference from mesh-file material order and names
- renaming bones to VRM names; spring nodes carried in the humanoid map; view position converted to a head offset
- expression preset tables and look-at range maps as the runtime's face model; the forced T-pose
- import-time mesh edits: blendshape baking, weight truncation, UV clamping, dropped UV sets and colours
- an invented VRM licence block on converted avatars
- a project-reading converter that reimplements build tools and resolves assets by hand, once the Unity
  exporter covers its inputs
