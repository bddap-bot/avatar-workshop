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
| `humanoid` | Unity HumanBodyBones name → node; the source's rest pose as given (no assumed T-pose); muscle limits from the human description; the body and the four IK goals as Unity's humanoid stream holds them with every muscle at rest; each humanoid bone's transform in the collider pose, the pose the VRChat client computes automatic colliders on (below), as Unity poses the avatar; and, from a Unity build, the frames Unity's Avatar poses each bone's muscles in (pre and post rotations, limit signs and limits), each bone's share of the body's mass, the human scale, the T-pose body frame and the twist shares |
| `descriptor` | view position, lip-sync mode and the 15 viseme blendshapes, eyelid mode and shapes, eye bones and their rotation limits; the 14 body colliders (head, torso, hands, feet, four fingers a side) as stored: state (automatic, custom or disabled), bone, radius, height, position and rotation offsets. A standard contact sender sits on each enabled one with a bone, tagged by part and side: a capsule at its offsets on its bone, and a finger's from its bone's parent to its bone, or past it to its height, as the VRChat client places them. On a humanoid avatar an automatic one's stored values go unused: its collider is computed from the skeleton |
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
- Humanoid: the animator's human stream (muscles, `RootT`, `RootQ`) poses the humanoid bones' local rotations and
  the hips' position as Unity's humanoid Avatar does.
- Output per frame: node local transforms, blendshape weights, node active and renderer enabled states, material
  property overrides and slot choices. Renderers apply it; they hold no avatar logic.
- Order per frame, as `avatar_core::evaluate::Evaluator` runs it: animator and the humanoid pose, constraints that read nothing a
  PhysBone moves, PhysBones, the constraints that do, contacts; receiver and PhysBone parameters reach the next
  frame's animator.

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

### Humanoid poses

Unity's Avatar holds the frame it poses each bone's muscles in, the bones' masses, the human scale, the T-pose body
frame and the twist shares; the exporter writes them as Unity holds them. The rules below that use them were
measured on Unity's own posing. A degree of freedom's angle is the muscle times its limit (the upper limit for a positive muscle, the lower for
a negative one) times its sign, and the bone's local rotation is `pre · r · post⁻¹` where `r` is the twist about x
composed after the swing about y and z, each from the tangent of half its angle. An avatar without an UpperChest
adds the UpperChest's muscles to the Chest's angles at Unity's default ±20°. Every humanoid bone but the hips stands
where the human description's skeleton has it, whatever its node's own position (on one corpus avatar the two
differ). The upper arm, forearm, upper leg and
lower leg each keep their twist share (0.5 on every avatar measured) of their own twist and pass the rest to the
child bone, leaving the child's pose in the world unchanged. The body is the skeleton's centre of mass and frame:
the centre is each bone's mass at a mean of bone positions (a bone the avatar lacks left out of the mean) (the hips at the mean of both upper legs and the spine, a
limb bone at the middle of it and its child, the chest at the mean of itself, the next spine bone and both shoulders,
the left upper arm one eighth of the way to its forearm, a hand, foot, toe or head bone at itself), in units of the
human scale; the frame's up runs from the upper legs' midpoint to the upper arms', its right is the sum of the
spans across both, made perpendicular to up; `RootQ` is that frame relative to the T-pose's. The hips are placed so
both hold. These rules come from Unity's `HumanPoseHandler` on the fixture and a corpus avatar
(`avatar-core/tests/humanoid.rs` holds the fixture's samples) and are scored on the recordings.

IK goals move no bone: Unity applies them only under a layer's IK pass or a state's Foot IK, and no recording,
corpus avatar or VRChat default controller uses either. Frames past the bounds (0.1° on a bone's local rotation,
1e-4 m on the hips' position) are listed by the animator replay by cause. On the recordings:

| recording | bone samples compared | past the bounds | frames past, by cause |
|---|---|---|---|
| fixture | 129,003 | 18,213 | tracked 6,115 |
| humanoid composition, rotation, Loop Pose drift, Feet height, dense clip, blend, layering, sample time, frame-grid rule, generic and dynamics probes | 4,616,724 | 0 | none |
| additive crossfade probe | 106,680 | 33 | animator 33 |
| frame-grid fill and hold probe | 138,810 | 296 | animator 296 |
| corpus 1 | 205,269 | 120 | tracked 60 |
| corpus 2 | 1,093,019 | 120 | tracked 60 |
| corpus 3 | 190,638 | 120 | tracked 60 |
| corpus 4 | 1,567,893 | 4 | tracked 2 |
| corpus 5 | 2,335,869 | 435 | tracked 433 |
| corpus 6 | 1,943,934 | 435 | tracked 433 |

On the probes with none past the bounds the largest differences are 0.056° and 5.6e-6 m. `animator` frames are the human stream's own
differences: the frame-grid probe's listed pose-binding residual, and on the crossfade probe a body value within 7e-4
of the recording's, under the binding tolerance; posing the recorded stream puts their bones within the bounds. `tracked` frames are the emulator's tracking: the fixture's
tracked left arm and chest, a tracked hip crouching at `Upright` 0.4 or turning at `TrackingType` 6, and one menu
control on corpus 5 and 6 that moves the tracked hips by up to 1.2e-4 m. Where the fixture's tracking hands the parts
to animation, every bone is within the bounds. The PhysBone probe plays no controller, so it has no human stream.

Not measured:

- An avatar with an UpperChest (no avatar measured has one): its own pose and its mass centre.
- An avatar missing a Neck, Chest or shoulder: whether Unity passes its muscles to a neighbour, and where it then
  puts the mass centres that would have named it.
- IK goals under an IK pass or Foot IK, and translation degrees of freedom (the harness refuses an avatar with them).
- The body frame's turn about its up axis when the spine bends and twists in several muscles at once: on Unity's
  random poses of the fixture the core's hips lie up to 0.15° and 1.7e-4 m from Unity's, and up to 0.3° at the
  extremes of two such muscles together; no recording reaches it.
- Muscle frames for a VRM avatar: the VRM importer writes none, so a humanoid curve would pose no bone of one; its
  importer writes no humanoid curve either.

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
- The avatar's own senders include a standard one on each enabled descriptor collider, tagged by part and, for
  hands, feet and fingers, by side (`Hand` and `HandL`, `FingerIndex` and `FingerIndexL`). The head's, torso's,
  hands' and feet's are capsules with the collider's dimensions and offsets on its bone, as Av3Emulator places
  them; a finger's is placed as the VRChat client places it, below. On a humanoid avatar an automatic collider's
  stored bone and values go unused, as below.
- In the VRChat client each finger's standard sender also carries `Finger` and `Finger` with its side (`FingerL`),
  which Av3Emulator leaves out, and no other finger's or side's tag. Measured on variants of the fixture as local
  test avatars with one finger collider enabled per hand, all eight fingers over four builds: receivers allowing
  self, each listening for one tag alone in a zone around each hand's fingers, read through the client's OSC
  output, heard exactly those four tags; one listening for a tag nothing sends stayed silent, and ones for the
  fixture's own sender and the head's standard sender fired.
- In the VRChat client a finger's standard sender is a capsule whose spheres' centres are its bone's parent's
  origin and a point in the bone's own space: the bone's local offset from its parent, scaled to the length by
  which the collider's height exceeds that offset's length (none when it does not). Its radius is the collider's
  times the bone's scale; the collider's position and rotation are not used. So the bone's rotation
  turns the part beyond the bone and its scale lengthens it, while turning the parent turns the whole capsule.
  Measured on a variant of the fixture as a local test avatar, its eight finger colliders on bones of their own
  under each hand and one configuration each: height short of, equal to and past the bone, zero height, a bone
  turned 90 degrees off its offset's direction, a bone at twice its parent's scale, a turned parent, a bone directly on
  the hand, and position and rotation offsets. Twelve proximity receivers per finger, each listening for that
  finger's own tag and allowing self, sat on the hand at two radii, read through the client's OSC output. Every
  reading is one less the distance from the receiver's centre to the sender's surface over the receiver's
  radius, as the core computes it: fitting each of the seven custom fingers' two centres and radius to its readings left residuals
  under 0.0006 mm, the placement above predicted every reading to 0.0013 mm, and readings varied by at most
  0.0032 mm over the 24 seconds they were read, so the placement holds to 0.01 mm. One listening for a tag
  nothing sends and one for a disabled hand's stayed silent, and one for the fixture's own sender fired. The
  same build's receivers around the head's, left hand's and left foot's custom colliders read their stored
  capsules to 0.0005 mm.
- In the VRChat client an automatic collider on a humanoid avatar is the one the SDK's descriptor computes from the
  humanoid skeleton in its collider pose, whatever the descriptor stores, and is then placed as a custom one of its
  part. A finger's sits
  on its distal bone (its intermediate when the humanoid has no distal), with radius 0.4 and height 2 times the
  distance from the finger's humanoid bone before it, over the bone's scale; one whose humanoid lacks the proximal or intermediate
  places no sender. The torso's sits on the chest (the spine without one), its height the distance from hips to
  neck, its radius half the lesser of that and the distance between the upper arms, centred between hips and neck
  and along them. A foot's is a sphere on the toes, radius a tenth of the leg's length from upper leg through
  lower leg to foot. A palm's sits on the hand, centred at 0.85 of the way to the mean of the finger proximals,
  height that distance and radius a quarter of it, across the hand. The head's is a sphere on the head, at the
  view position's height (the eyes' mean height when the humanoid has eye bones) straight above it, radius the larger
  of 0.8 times that height and 0.33 times the distance between the upper arms. Measured on the fixture as a local
  test avatar with three-bone humanoid chains under each hand (one with a turned and one with a scaled distal bone,
  a two-bone little finger on the left, a right little finger with only a proximal), every collider automatic with
  stored values nothing like the rule's except a custom right ring finger with no bone and a disabled right foot.
  Proximity receivers listening for each part's own tag and allowing self sat on the bone each end of its capsule
  moves with, so posing the fingers could not move what they read, and were read through the client's OSC output;
  four more per finger on the bone before it, around the capsule's middle, read up to 9.6 mm off the skeleton at
  rest, as fingers posed while read would.
  Each finger's two capsule ends and radius, the torso's and the left foot's capsule, and the palms' centres and
  radii read the rule's to 0.0008 mm; fits left residuals under 0.0004 mm and readings varied by at most 0.0023 mm
  over the 47 seconds they were read, so these hold to 0.01 mm. The right little finger, the custom finger with no
  bone and the disabled foot were heard by none of their receivers, one listening for a tag nothing sends stayed
  silent, and one for the fixture's own sender fired.
- The collider pose is a fixed human pose, whatever the avatar's rest: every muscle at zero but each side's Upper
  Leg Front-Back at 0.6001087, Lower Leg Stretch and Forearm Stretch at 0.9999163 and Arm Front-Back at 0.3000543,
  the values Unity gives the fixture's T-pose, with the arms' Down-Up at zero rather than that T-pose's 0.4, and
  the body at height one and unturned. The exporter poses the avatar so with Unity's human pose handler and records
  each humanoid bone's local transform. The pose turns the hands against the world's up, which sets the palms'
  axis, and puts the head higher against the view position than at rest, which fits the arms' fall moving the
  body's centre of mass. Measured as above on that build and on two more builds of the fixture, each with the head
  and both palms automatic and the other colliders disabled, worn in turn in one session: one with the upper arms
  rested 30 degrees lower and 15 forward and the knees bent 15 degrees, where a pose taken from the avatar's rest
  (that rest with the arms' Down-Up at zero, or lowered by a further 0.4) puts the palms' capsule ends 5.2 mm or
  more and the head 2.2 mm or more from where the client read them, and the fixed pose with the body placed as at
  rest puts the head 1.5 mm off; and one with eye bones 0.2 m above the head and the hands turned 25 degrees about
  their length, where the palms' axis follows the turn and the head's sphere sits at the eyes with radius 0.16, set
  by their height. The skeleton at rest puts the fixture's palms' axes about 11.7 degrees off, turned about the
  hand's length, and its head 14.4 mm high. The fixed pose predicted every palm's capsule and head's sphere to
  0.001 mm; fits left residuals under 0.0005 mm and readings varied by at most 0.0023 mm over the 42 to 47 seconds
  each build was read, so the pose holds to 0.01 mm. Round values (0.6, 1 and 0.3) move the prediction by at most
  0.0012 mm, which this does not resolve.

Not measured:

- Where VRChat runs a PhysBone that sits under a bone a constraint after PhysBones turns (one chain each on corpus
  2 and 5). The replay takes that chain's transforms from the recording, so the targets' bar holds whichever order
  VRChat uses; the core's order runs every PhysBone before those constraints, and how far a host's chain then lies
  from the SDK's is not measured.
- The collider pose's muscles that move neither the hands' turn against the world's up nor the head's height, such
  as the spine's and chest's Twist Left-Right: the exporter holds them at zero, which these readings cannot tell
  from another value.
- An automatic torso, foot or finger on an avatar whose rest differs from the collider pose in the bones its rule
  reads: every build that read them had both agree there.
- An automatic collider on an avatar with no humanoid definition, a palm with no finger proximals, a foot whose
  humanoid has no toes, a torso with no upper arms or chest, and an automatic collider on a scaled bone other than
  a finger's or under a scaled ancestor.
- A finger's sender on a bone with uneven scale, under a scaled ancestor, past the descriptor's size limit, or on
  a bone moved after the avatar loads.

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
