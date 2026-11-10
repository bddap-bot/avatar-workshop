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
    ("additive playable", &["=1:0:Breathe"]),
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
    let wrong = |k: &str| rep.parameters.keys.contains_key(k) || rep.states.keys.contains_key(k) || rep.values.keys.contains_key(k);
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
    leaves(&tree, &param, Leaf { clip: 0, weight: 1.0, time_scale: 1.0, cycle_offset: 0.0, direct: false, id: 0 }, &mut out);
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
