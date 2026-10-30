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
| Initial visibility (the active state after the controller's default states run) | three encodings: omitted nodes, node-visibility extension, a side list | hidden-by-default props drawn when a reader honours the wrong encoding |
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

`AW_avatar` sections. Node, mesh, material and texture references are glTF indices.

| Section | Contents |
|---|---|
| `source` | importer name and version, source format, licence text and author as found in the source (absent when the source has none; never invented) |
| `humanoid` | Unity HumanBodyBones name → node; the source's rest pose as given (no assumed T-pose); muscle limits from the human description |
| `descriptor` | view position, lip-sync mode and the 15 viseme blendshapes, eyelid mode and shapes, eye bones and their rotation limits |
| `renderers` | per mesh node: initially enabled, material slots |
| `nodes` | per node: initially active, after evaluating every playable layer's default state at time 0 |
| `materials` | per material: shader family (`poiyomi`, `liltoon`, `vrchat-mobile`, `mtoon`, `standard`, `unknown`), version, and its saved properties as a typed map (floats, colours, vectors, texture references with tiling/offset, keywords); render queue, cull, blend and depth state |
| `physbones` | per component: root, ignored nodes, endpoint, multi-child type, integration type and version, pull/spring (momentum)/stiffness/gravity/gravity falloff/immobile and immobile type, each with its curve, limit type and angles and rotation, radius with curve, max stretch, stretch motion, max squish, collider indices, allow collision/grabbing/posing filters, grab movement, snap to hand, parameter name, is-animated, reset-when-disabled, initially enabled |
| `colliders` | per collider: node, shape (sphere, capsule, plane), position, rotation, radius, height, inside bounds, bones as spheres |
| `contacts` | senders and receivers: node, shape, radius, height, position, rotation, collision tags, allow self/others, local only, receiver type (constant, on enter, proximity), parameter, min velocity |
| `constraints` | one schema for the six kinds (parent, position, rotation, aim, look-at, scale): target node, sources (node and weight, or a fixed world matrix), at-rest and offset values, axis masks, weight, world-up settings, freeze-to-world, solve-in-local, initially enabled |
| `parameters` | expression parameters: name, type, default, saved, synced; built-in parameters are named by the model, not listed |
| `menu` | controls as authored: name, type, parameter, value, sub-parameters, labels, icon, submenu |
| `layers` | the playable layers (base, additive, gesture, action, FX, sitting, T-pose, IK pose) each with its controller: layers with weight, blending (override/additive), avatar mask and sync source; state machines with sub-machines kept; states with speed and its parameter, cycle offset, mirror, motion, write defaults, behaviours; transitions with conditions, exit time, duration (fixed or normalised), offset, interruption source, ordered interruption, can-transition-to-self; blend trees with their algorithm (1D, 2D simple directional, 2D freeform directional, 2D freeform cartesian, direct) and children |
| `behaviours` | parameter drivers (set, add, random, copy with ranges, local only), tracking control, locomotion control, playable and animator layer control, temporary pose space; play-audio reported unsupported |
| `clips` | length, loop, and curves with Hermite keys (value, in/out tangents and weights). Each curve binds a typed property: node active, renderer enabled, blendshape weight, material property by name, material slot (a real material swap), node translation/rotation/scale, humanoid muscle, animator parameter, physbone/constraint/contact field |
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
  clip and curve with tangents, materials' saved properties, and every VRChat component. Legacy DynamicBones are
  migrated with the SDK's own conversion first. Nothing is inferred from file names or from re-importing the
  source mesh file.
- **VRM 0.x and 1.0.** VRM maps into the model: humanoid to HumanBodyBones, spring bones to PhysBones (VRM's
  stiffness, drag and gravity converted to the PhysBone parameters that reproduce VRM motion, measured against a
  VRM reference runtime), MToon to an `mtoon` material record, expressions to visemes, eyelids and animator clips.
  VRM 0.x quirks (coordinate flip, renamed thumbs, its MToon properties) live only here.

## Measurement

A reference harness runs the same avatar in Unity with the VRChat SDK's editor simulation of PhysBones and
contacts and an open-source avatar emulator for the animator and menu. It records traces under scripted inputs:
body and hand motion, every menu control swept, built-in parameters stepped. The core's output on the same inputs
is compared trace for trace: PhysBone tip positions relative to chain length, parameter and state sequences, node
states, blendshape weights, material property values; renderers are compared by image at matched camera and light.
A change that moves a metric away from the reference does not land.

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
