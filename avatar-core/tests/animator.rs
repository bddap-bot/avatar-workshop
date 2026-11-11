mod reference;

use std::path::Path;

use avatar_core::animator::Animator;
use reference::animator::Run;

fn fixture() -> avatar_core::Avatar {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    avatar_core::read(&std::fs::read(root.join("fixtures/unity-synthetic.glb")).unwrap()).unwrap()
}

#[test]
fn fixture_follows_the_reference() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/reference/fixture-animator");
    let rep = Run::new(&dir, fixture()).run(None);
    assert!(rep.frames > 4900, "{} frames", rep.frames);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert_eq!(rep.values.wrong, 0, "values: {}", rep.values.summary(20));
    let controls = rep.controls(&dir);
    assert!(controls.len() >= 9 && controls.iter().all(|c| c.1), "{controls:?}");
    // Only the transform components the animator binds are scored.
    assert!(rep.values.compared > 1_250_000, "{} values compared", rep.values.compared);
    assert!(rep.clocks.compared > 150_000, "{} state times compared", rep.clocks.compared);
    assert_eq!(rep.clock_bits_differ, 0, "state times not the recorded bits");
    assert!(rep.bindings.compared > 400_000, "{} bindings compared", rep.bindings.compared);
    for kind in ["muscle:", "constraint:", "sender:", "receiver:"] {
        assert!(rep.seen.keys().any(|k| k.starts_with(kind)), "no {kind} binding was compared");
    }
    assert_eq!(rep.clocks.wrong, 0, "state times: {}", rep.clocks.summary(20));
    let wrong: Vec<(&str, usize)> = rep.bindings.keys.iter().map(|(k, v)| (k.as_str(), v.0)).collect();
    assert_eq!(wrong, ROOT_MOTION_NOT_YET_EVALUATED, "bindings: {}", rep.bindings.summary(20));
}

/// The body under the fixture's root-motion settings (Walk's bakes, bases and offsets, Sit's centre-of-mass height),
/// which the core does not evaluate yet.
const ROOT_MOTION_NOT_YET_EVALUATED: &[(&str, usize)] = &[
    ("muscle:RootQ.w", 53),
    ("muscle:RootQ.y", 92),
    ("muscle:RootT.x", 92),
    ("muscle:RootT.y", 117),
    ("muscle:RootT.z", 92),
];

/// A copy of the fixture's recording with one of its files rewritten.
fn edited_recording(name: &str, file: &str, edit: impl Fn(&str) -> bool) -> std::path::PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/reference/fixture-animator");
    let to = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&to).unwrap();
    for e in std::fs::read_dir(&from).unwrap() {
        let e = e.unwrap();
        std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
    }
    let text = reference::text(&from.join(file));
    let kept: Vec<&str> = text.lines().filter(|l| edit(l)).collect();
    std::fs::write(to.join(file), kept.join("\n") + "\n").unwrap();
    to
}

#[test]
#[should_panic(expected = "no state times at frame 500")]
fn a_recording_missing_a_frames_state_times_fails() {
    let dir = edited_recording("clocks", "clocks.tsv", |l| !l.starts_with("500\t"));
    Run::new(&dir, fixture()).run(None);
}

#[test]
#[should_panic(expected = "no value of the curve binding")]
fn a_recording_missing_a_binding_fails() {
    let dir = edited_recording("binding", "values.tsv", |l| !l.contains("\tsender:Probe#0:m_Enabled\t"));
    Run::new(&dir, fixture()).run(None);
}

/// Each semantic with the recorded keys that exercise it in the fixture's trace: every key must be compared with
/// the reference every frame, stay right, and take at least two values in the reference, so a semantic the recording
/// never drives cannot pass. A key starting with `=` is a state that stays put, only compared.
const SEMANTICS: &[(&str, &[&str])] = &[
    ("saved parameter", &["Hat"]),
    ("synced parameter", &["Smile"]),
    ("unsaved, unsynced parameter", &["Hue"]),
    (
        "built-in parameters",
        &["GestureLeft", "Viseme", "VelocityZ", "Upright", "Grounded", "Seated"],
    ),
    ("base playable", &["0:0:Locomotion"]),
    ("additive playable, which leaves the humanoid pose alone", &["=1:0:Breathe", "=muscle:Chest Front-Back"]),
    ("sitting playable", &["2:0:Sit", "playable:2"]),
    ("gesture playable", &["5:0:LeftHand", "5:1:RightHand"]),
    ("action playable", &["6:0:Action", "playable:6"]),
    ("fx playable", &["7:0:Hat"]),
    ("crossfade, fixed duration", &["7:0:Hat", "active:Hat"]),
    ("crossfade, normalized duration", &["7:1:Outfit"]),
    ("transition offset and exit time", &["7:8:Wave", "0:0:Locomotion"]),
    ("interruption source, ordered interruption", &["7:9:Gestures"]),
    ("sub-machine entry and exit", &["0:0:Locomotion"]),
    ("trigger parameter", &["7:23:Trigger"]),
    ("synced layer", &["7:14:HatSync", "xform:Orbs/Sync"]),
    ("transform mask", &["xform:Orbs/Unmasked"]),
    ("additive layer", &["=7:12:Additive", "xform:Orbs/Puppet"]),
    ("write defaults on and off", &["shape:Body:Smile", "shape:Body:Fist"]),
    ("blend tree, simple 1D", &["shape:Body:Smile"]),
    ("blend tree, simple directional 2D", &["shape:Body:LeanUp", "xform:Orbs/Puppet"]),
    ("blend tree, freeform directional and cartesian 2D", &["xform:Orbs/Free"]),
    ("blend tree, direct", &["shape:Body:Fist", "shape:Body:Open", "shape:Body:Point", "shape:Body:Victory"]),
    ("clip sampling and rotation curves", &["xform:Orbs/Bob", "xform:Orbs/Spin", "xform:Orbs/Pulse", "xform:Orbs/Scrub"]),
    ("curve binding: node active", &["active:Hat"]),
    ("curve binding: renderer enabled", &["enabled:Orbs/Sync", "enabled:Orbs/Rotation"]),
    ("curve binding: blend shape", &["shape:Body:vrc.v_aa"]),
    ("curve binding: material property", &["mat:Body:_Color"]),
    ("curve binding: material slot", &["slot:Body:0"]),
    ("curve binding: animator parameter", &["curve:7:Driven"]),
    ("curve binding: humanoid muscle", &["muscle:Left Upper Leg Front-Back", "muscle:Right Upper Leg Front-Back"]),
    ("mirror parameter", &["Flip", "muscle:Left Arm Down-Up", "muscle:Right Arm Down-Up", "muscle:Spine Twist Left-Right"]),
    ("clip mirror, combined with the state's", &["Flip", "muscle:Left Arm Down-Up", "muscle:Spine Twist Left-Right"]),
    ("clip cycle offset", &["muscle:Left Upper Leg Front-Back", "muscle:Right Upper Leg Front-Back"]),
    ("additive reference pose", &["=7:12:Additive", "xform:Orbs/Puppet"]),
    ("humanoid layer mask", &["muscle:Left Arm Down-Up", "muscle:Spine Front-Back"]),
    (
        "curve binding: constraint field",
        &["constraint:Probe#0:GlobalWeight", "constraint:Probe#0:Sources.source0.Weight", "constraint:Probe#0:PositionOffset.x"],
    ),
    ("curve binding: contact sender field", &["sender:Probe#0:m_Enabled"]),
    ("curve binding: contact receiver field", &["receiver:Probe#0:minVelocity", "receiver:Probe#0:allowSelf"]),
    ("driver: set", &["Setter"]),
    ("driver: add", &["Counter"]),
    ("driver: random", &["Rand", "Chance"]),
    ("driver: copy with range", &["Copied"]),
    ("driver: local only", &["Local"]),
    ("tracking control", &["tracking:head", "tracking:leftHand", "tracking:rightHand", "tracking:hip"]),
    ("locomotion control", &["locomotion:disabled"]),
    ("playable layer control", &["playable:2", "playable:6"]),
    ("animator layer control", &["weight:7:16:Bob"]),
    ("temporary pose space", &["view"]),
];

#[test]
fn each_semantic_follows_its_reference_trace() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/reference/fixture-animator");
    let rep = Run::new(&dir, fixture()).run(None);
    let wrong = |k: &str| [&rep.parameters, &rep.states, &rep.values, &rep.bindings].iter().any(|m| m.keys.contains_key(k));
    let mut problems = Vec::new();
    for (name, keys) in SEMANTICS {
        for key in *keys {
            let (key, varies) = key.strip_prefix('=').map_or((*key, true), |k| (k, false));
            match rep.seen.get(key) {
                None => problems.push(format!("{name}: {key} was never compared")),
                Some(values) if varies && values.len() < 2 => problems.push(format!("{name}: {key} never varies in the reference")),
                Some(_) if wrong(key) => problems.push(format!("{name}: {key} differs from the reference")),
                Some(_) => {}
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A direct tree's weight parameter below zero contributes nothing, as the reference's frame-time smoothing shows
/// the frame a timer restarts.
#[test]
fn a_negative_direct_weight_contributes_nothing() {
    use avatar_core::animator::motion::{leaves, Leaf};
    use avatar_core::model::{BlendAlgorithm, BlendTree, ChildDirect, ClipIndex, Motion};
    let child = |p: &str, c: u32| ChildDirect { motion: Some(Motion::Clip(ClipIndex(c))), direct_blend_parameter: p.into(), time_scale: 1.0, cycle_offset: 0.0, mirror: false };
    let tree = Motion::BlendTree(Box::new(BlendTree {
        name: String::new(),
        algorithm: BlendAlgorithm::Direct { normalized_blend_values: false, children: vec![child("a", 0), child("b", 1)] },
    }));
    let param = |n: &str| if n == "a" { -163.0 } else { 0.5 };
    let mut out = Vec::new();
    leaves(&tree, &param, Leaf { clip: 0, weight: 1.0, time_scale: 1.0, cycle_offset: 0.0, direct: false, id: 0, mirror: false }, &mut out);
    assert_eq!(out.iter().map(|l| (l.clip, l.weight)).collect::<Vec<_>>(), vec![(1, 0.5)]);
}

fn wave(local: bool) -> Animator {
    let mut a = Animator::new(&fixture());
    a.set_builtin("IsLocal", local as u8 as f32);
    a.set("Wave", 1.0);
    for _ in 0..30 {
        a.step(1.0 / 60.0);
    }
    a
}

#[test]
fn local_only_drivers_run_for_the_wearer_alone() {
    let (worn, remote) = (wave(true), wave(false));
    assert_eq!((worn.get("Setter"), worn.get("Local")), (Some(1.0), Some(1.0)));
    assert_eq!((remote.get("Setter"), remote.get("Local")), (Some(1.0), Some(0.0)));
}

#[test]
fn saved_parameters_survive_a_reload() {
    let mut a = Animator::new(&fixture());
    a.set("Hat", 1.0);
    a.set("Hue", 0.9);
    let saved = a.saved();
    assert!(saved.iter().any(|(n, _)| n == "Hat") && !saved.iter().any(|(n, _)| n == "Hue"));
    let mut b = Animator::new(&fixture());
    b.restore(&saved);
    b.restore(&[("Hue".to_string(), 0.9)]);
    assert_eq!((b.get("Hat"), b.get("Hue")), (Some(1.0), Some(0.25)));
}

#[test]
fn an_animated_material_property_defaults_from_the_first_material_declaring_it() {
    use avatar_core::animator::{authored, motion::Target};
    use avatar_core::model::{MaterialIndex, MaterialProperty};
    let mut avatar = fixture();
    let m = &mut avatar.model;
    let mut locked = m.materials[0].clone();
    locked.properties.insert("_Probe".into(), MaterialProperty::Float(1.0));
    locked.declared.remove("_Probe");
    locked.declared.insert("_Tint".into(), MaterialProperty::Color([0.1, 0.2, 0.3, 0.4]));
    let mut open = locked.clone();
    open.declared.insert("_Probe".into(), MaterialProperty::Float(2.0));
    m.materials.extend([locked, open]);
    let n = m.materials.len();
    m.renderers[0].materials = vec![MaterialIndex(n as u32 - 2), MaterialIndex(n as u32 - 1)];
    let node = m.renderers[0].node.0;
    let value = |avatar: &avatar_core::Avatar, prop: &str| authored(avatar, &Target::MaterialProperty(node, prop.into()));
    assert_eq!(value(&avatar, "_Probe"), 2.0);
    assert_eq!(value(&avatar, "_Tint.b"), 0.3);
    assert_eq!(value(&avatar, "_Unknown"), 0.0);
    avatar.model.materials[n - 2].declared.insert("_Probe".into(), MaterialProperty::Float(1.0));
    assert_eq!(value(&avatar, "_Probe"), 1.0);
}

fn muscle(name: &str, value: f32) -> avatar_core::model::Curve {
    use avatar_core::model::{AnimationCurve, Binding, Curve, Keyframe, WeightedMode};
    let key = Keyframe { time: 0.0, value, in_tangent: 0.0, out_tangent: 0.0, in_weight: 1.0 / 3.0, out_weight: 1.0 / 3.0, weighted_mode: WeightedMode::None };
    Curve { binding: Binding::Muscle { name: name.into() }, curve: AnimationCurve { keys: vec![key] } }
}

fn clip_mut<'a>(avatar: &'a mut avatar_core::Avatar, name: &str) -> &'a mut avatar_core::model::Clip {
    avatar.model.clips.iter_mut().find(|c| c.name == name).unwrap()
}

fn value(a: &Animator, name: &str) -> f32 {
    let t = avatar_core::animator::motion::Target::Muscle(name.into());
    a.values()[a.targets().iter().position(|x| *x == t).unwrap()]
}

/// The fixture with left finger bones, whose base pose bends the index and ring fingers the gesture layer's clips
/// leave alone or set.
fn fingered() -> avatar_core::Avatar {
    use avatar_core::model::{HumanBodyBone, HumanBone};
    let mut avatar = fixture();
    let hand = avatar.model.humanoid.bones[&HumanBodyBone::LeftHand].node;
    for b in [HumanBodyBone::LeftIndexProximal, HumanBodyBone::LeftMiddleProximal, HumanBodyBone::LeftRingProximal] {
        avatar.model.humanoid.bones.insert(b, HumanBone { node: hand, limit: None });
    }
    clip_mut(&mut avatar, "Stand").curves.extend([muscle("LeftHand.Index.1 Stretched", 0.5), muscle("LeftHand.Ring.1 Stretched", 0.5)]);
    avatar
}

fn run(a: &mut Animator, frames: usize) {
    for _ in 0..frames {
        a.step(1.0 / 60.0);
    }
}

/// A humanoid clip is a whole pose: when the gesture layer's clips have curves for the index and middle fingers
/// alone, its open hand rests the ring finger over the base layer's bend.
#[test]
fn a_humanoid_clip_writes_a_whole_pose() {
    let mut avatar = fingered();
    for clip in ["LeftOpen", "LeftFist", "RightOpen", "RightFist"] {
        clip_mut(&mut avatar, clip).curves.retain(|c| matches!(&c.binding, avatar_core::model::Binding::Muscle { name } if name.contains("Index.1") || name.contains("Middle.1")));
    }
    let mut a = Animator::new(&avatar);
    run(&mut a, 10);
    assert_eq!((value(&a, "LeftHand.Index.1 Stretched"), value(&a, "LeftHand.Ring.1 Stretched")), (1.0, 0.0));
}

/// A controller covers the muscles it binds with its last output while a motionless state writes nothing.
#[test]
fn a_controller_holds_what_a_motionless_state_leaves() {
    let mut avatar = fingered();
    let gesture = avatar.model.layers.get_mut(&avatar_core::model::PlayableLayerType::Gesture).unwrap();
    let avatar_core::model::LayerContent::StateMachine { state_machine } = &mut gesture.controller.as_mut().unwrap().layers[0].content else { panic!() };
    state_machine.states.iter_mut().find(|s| s.name == "Open").unwrap().motion = None;
    let mut a = Animator::new(&avatar);
    a.set_builtin("GestureLeft", 1.0);
    run(&mut a, 30);
    assert_eq!(value(&a, "LeftHand.Index.1 Stretched"), -1.0);
    a.set_builtin("GestureLeft", 0.0);
    run(&mut a, 30);
    assert_eq!(value(&a, "LeftHand.Index.1 Stretched"), -1.0);
}

#[test]
fn body_and_goal_rotations_rest_at_the_identity_and_stay_unit() {
    use avatar_core::animator::{authored, motion::Target};
    let mut avatar = fixture();
    for (n, w) in [("RootQ.w", 1.0), ("LeftFootQ.w", 1.0), ("RootQ.x", 0.0), ("RootT.y", 0.0)] {
        assert_eq!(authored(&avatar, &Target::Muscle(n.into())), w, "{n}");
    }
    clip_mut(&mut avatar, "Stand").curves.extend([muscle("RootQ.x", 0.0), muscle("RootQ.y", 0.0), muscle("RootQ.z", 0.0), muscle("RootQ.w", 0.5)]);
    let mut a = Animator::new(&avatar);
    run(&mut a, 3);
    assert_eq!(value(&a, "RootQ.w"), 1.0);
}

#[test]
fn component_fields_default_to_their_authored_values() {
    use avatar_core::animator::{authored, motion::Target};
    let mut avatar = fixture();
    let m = &mut avatar.model;
    m.constraints[0].weight = 0.7;
    m.constraints[0].sources[0].weight = 0.4;
    m.contacts.receivers[0].enabled = true;
    m.physbones[0].collision = avatar_core::model::Permission { allow_self: true, allow_others: false };
    m.physbones[0].grabbing = avatar_core::model::Permission { allow_self: true, allow_others: true };
    let field = |t: Target| authored(&avatar, &t);
    assert_eq!(field(Target::Constraint(0, "GlobalWeight".into())), 0.7);
    assert_eq!(field(Target::Constraint(0, "Sources.source0.Weight".into())), 0.4);
    assert_eq!(field(Target::ContactReceiver(0, "m_Enabled".into())), 1.0);
    assert_eq!(field(Target::PhysBone(0, "allowCollision".into())), 2.0);
    assert_eq!(field(Target::PhysBone(0, "allowGrabbing".into())), 1.0);
}

/// An additive clip's reference pose subtracts only what the clip itself animates: a reference clip that also moves
/// another node leaves that node alone.
#[test]
fn a_reference_pose_subtracts_only_what_its_clip_animates() {
    use avatar_core::model::{AnimationCurve, Axis, Binding, Curve, Keyframe, NodeIndex, TransformProperty, WeightedMode};
    let plain = fixture();
    let mut wider = fixture();
    let bob = wider.gltf["nodes"].as_array().unwrap().iter().position(|n| n["name"] == "Bob").expect("a Bob node");
    let key = Keyframe { time: 0.0, value: 0.5, in_tangent: 0.0, out_tangent: 0.0, in_weight: 1.0 / 3.0, out_weight: 1.0 / 3.0, weighted_mode: WeightedMode::None };
    let reference = clip_mut(&mut wider, "AdditiveReference");
    reference.curves.push(Curve {
        binding: Binding::Transform { node: NodeIndex(bob as u32), property: TransformProperty::Translation, component: Axis::X },
        curve: AnimationCurve { keys: vec![key] },
    });
    assert!(fixture().model.clips.iter().any(|c| c.additive_reference_pose.is_some()));
    let (mut a, mut b) = (Animator::new(&plain), Animator::new(&wider));
    for _ in 0..120 {
        a.step(1.0 / 60.0);
        b.step(1.0 / 60.0);
        assert_eq!(a.values(), b.values());
    }
}
