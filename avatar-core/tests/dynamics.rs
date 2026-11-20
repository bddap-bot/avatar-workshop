//! The core's constraints and contacts against reference traces recorded by `reference/` from the VRChat SDK's own
//! dynamics: the dynamics probe (one constraint or receiver per setting value, and senders outside the avatar) and
//! the fixture. `fixtures/reference/` holds the recordings, trimmed to what the replay reads.

mod reference;

use std::path::{Path, PathBuf};

use avatar_core::model::ContactShape;
use avatar_core::physbones::Transform;
use glam::{Quat, Vec3};
use reference::dynamics::{score, Report};

fn path(p: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(p)
}

fn replay(recording: &str, model: &str) -> Report {
    let avatar = avatar_core::read(&std::fs::read(path(model)).unwrap()).unwrap();
    let dir = path(recording);
    let replay = reference::Replay::load(&dir, avatar, 0..usize::MAX);
    score(&dir, &replay)
}

/// The bar of the core's constraints: metres for position and scale, degrees for rotation.
const POSITION: f32 = 1e-4;
const ROTATION: f32 = 0.1;

fn assert_targets(report: &Report) {
    let mut failed = vec![];
    for (target, e) in &report.targets {
        assert!(e.frames > 0, "{target}: no frame scored");
        if e.position.0 > POSITION || e.rotation.0 > ROTATION {
            failed.push(format!("{target}: {:.6} m at frame {}, {:.4}° at frame {}", e.position.0, e.position.1, e.rotation.0, e.rotation.1));
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

fn assert_receivers(report: &Report) {
    let failed: Vec<String> = report
        .receivers
        .iter()
        .filter(|(_, r)| r.wrong > 0 || r.frames == 0)
        .map(|(name, r)| format!("{name}: {} of {} frames wrong, first {:?}", r.wrong, r.frames, r.first_wrong))
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
}

/// Each of the six kinds with one and two weighted sources, each axis mask, at-rest and offset values, aim's five
/// world-up types and its axes, look-at's roll and up object, freeze-to-world with and without rebaked offsets,
/// local solving, sources summing past one, constraints chained on one another, and one following a PhysBone's
/// tip where the SDK's PhysBones put it.
#[test]
fn each_constraint_setting_against_the_probe() {
    let report = replay("fixtures/reference/dynamics", "fixtures/unity-dynamics.glb");
    assert_eq!(report.targets.len(), 65);
    assert_targets(&report);
}

/// Every receiver type and shape, allow self and others, local only, collision tags and minimum velocity, against
/// the avatar's own sender and two senders outside it standing for another avatar's part and a hand.
#[test]
fn each_receiver_setting_against_the_probe() {
    let report = replay("fixtures/reference/dynamics", "fixtures/unity-dynamics.glb");
    assert_eq!(report.receivers.len(), 14);
    assert_receivers(&report);
}

#[test]
fn fixture_constraints_and_receivers() {
    let report = replay("fixtures/reference/fixture-dynamics", "fixtures/unity-synthetic-dynamics.glb");
    assert_eq!(report.targets.len(), 8);
    assert_targets(&report);
    assert_receivers(&report);
}

fn probe() -> (avatar_core::Avatar, avatar_core::physbones::Nodes, Vec<String>) {
    let avatar = avatar_core::read(&std::fs::read(path("fixtures/unity-dynamics.glb")).unwrap()).unwrap();
    let nodes = avatar_core::physbones::Nodes::read(&avatar).unwrap();
    let names = (0..nodes.parent.len()).map(|i| avatar.gltf["nodes"][i]["name"].as_str().unwrap_or("").to_string()).collect();
    (avatar, nodes, names)
}

fn world(nodes: &avatar_core::physbones::Nodes, locals: &[Transform]) -> Vec<Transform> {
    (0..locals.len())
        .map(|n| {
            let mut t = locals[n];
            let mut at = nodes.parent[n];
            while let Some(p) = at {
                t = locals[p].mul(&t);
                at = nodes.parent[p];
            }
            t
        })
        .collect()
}


/// VRChat registers a constraint when its object becomes active and first solves it the next frame.
#[test]
fn a_constraint_enabled_this_frame_first_moves_its_target_the_next() {
    let (avatar, nodes, names) = probe();
    let mut constraints = avatar_core::constraints::Constraints::new(&avatar).unwrap();
    let target = names.iter().position(|n| n == "Position one source").unwrap();
    let mut active = vec![true; nodes.parent.len()];
    let mut solve = |active: &[bool]| {
        let mut locals = nodes.rest.clone();
        let mut scene = avatar_core::constraints::Scene { root: Transform::IDENTITY, locals: &mut locals, active };
        constraints.solve_before_physbones(&mut scene);
        locals[target]
    };
    active[target] = false;
    assert_eq!(solve(&active), nodes.rest[target]);
    active[target] = true;
    assert_eq!(solve(&active), nodes.rest[target], "solved on the frame it was enabled");
    assert!((solve(&active).translation - nodes.rest[target].translation).length() > 0.1);
}

/// A receiver leaves its parameter to the animator while it is not running and on the frame it starts.
#[test]
fn a_receiver_drives_its_parameter_from_the_frame_after_it_starts() {
    let (avatar, nodes, names) = probe();
    let mut contacts = avatar_core::contacts::Contacts::new(&avatar).unwrap();
    let receiver = names.iter().position(|n| n == "Constant Sphere").unwrap();
    let k = contacts.receivers().iter().position(|r| r.node.0 as usize == receiver).unwrap();
    let mut locals = nodes.rest.clone();
    locals[names.iter().position(|n| n == "SelfSender").unwrap()].translation = nodes.rest[receiver].translation;
    let mut active = vec![true; nodes.parent.len()];
    let mut step = |active: &[bool]| {
        contacts.advance(1.0 / 60.0, &world(&nodes, &locals), active, &[], true);
        contacts.values()[k]
    };
    active[receiver] = false;
    assert_eq!(step(&active), None);
    active[receiver] = true;
    assert_eq!(step(&active), None, "written on the frame it started");
    assert_eq!(step(&active), Some(1.0));
}

/// The core places VRChat's standard senders where the recording found them: on each enabled descriptor
/// collider's bone, a finger's on its parent, with the collider's shape, offsets and the part and side tags; the
/// client's `Finger` tags, which the emulator leaves out, have their own test.
#[test]
fn standard_senders_as_the_reference_placed_them() {
    let avatar = avatar_core::read(&std::fs::read(path("fixtures/unity-synthetic-dynamics.glb")).unwrap()).unwrap();
    let nodes = avatar_core::physbones::Nodes::read(&avatar).unwrap();
    let names = reference::paths(&avatar, &nodes);
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path("fixtures/reference/fixture-dynamics/meta.json")).unwrap()).unwrap();
    let mut want: Vec<(String, Vec<String>, f32, f32, Vec3, Quat)> = meta["standard_senders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let f = |k: &str| -> Vec<f32> { s[k].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect() };
            let t = reference::unity(&[f("position"), f("rotation")].concat());
            let mut tags: Vec<String> = s["tags"].as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_string()).collect();
            tags.sort();
            (s["transform"].as_str().unwrap().to_string(), tags, s["radius"].as_f64().unwrap() as f32, s["height"].as_f64().unwrap() as f32, t.translation, t.rotation)
        })
        .collect();
    let mut got: Vec<(String, Vec<String>, f32, f32, Vec3, Quat)> = avatar_core::contacts::standard_senders(&avatar.model.descriptor.colliders, &nodes)
        .into_iter()
        .map(|s| {
            let ContactShape::Capsule { radius, height } = s.shape else { panic!("a standard sender is a capsule") };
            let mut tags: Vec<String> = s.collision_tags.into_iter().filter(|t| !["Finger", "FingerL", "FingerR"].contains(&t.as_str())).collect();
            tags.sort();
            (names[s.node.0 as usize].clone().unwrap(), tags, radius, height, Vec3::from(s.position), Quat::from_array(s.rotation))
        })
        .collect();
    want.sort_by(|a, b| a.1.cmp(&b.1));
    got.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(got.len(), 5);
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(&want) {
        assert_eq!((&g.0, &g.1, g.2, g.3), (&w.0, &w.1, w.2, w.3));
        assert!((g.4 - w.4).length() < 1e-6 && g.5.angle_between(w.5) < 1e-5, "{:?}: {:?} against {:?}", g.1, (g.4, g.5), (w.4, w.5));
    }
}

/// Each standard finger sender carries the tags the VRChat client puts on it: the finger and any finger, each
/// alone and with its side, and no other side's or finger's.
#[test]
fn standard_finger_senders_carry_the_clients_tags() {
    let mut avatar = avatar_core::read(&std::fs::read(path("fixtures/unity-synthetic-dynamics.glb")).unwrap()).unwrap();
    let nodes = avatar_core::physbones::Nodes::read(&avatar).unwrap();
    let c = &mut avatar.model.descriptor.colliders;
    let enabled = c.finger_index_left.clone();
    for f in [
        &mut c.finger_middle_left,
        &mut c.finger_ring_left,
        &mut c.finger_little_left,
        &mut c.finger_index_right,
        &mut c.finger_middle_right,
        &mut c.finger_ring_right,
        &mut c.finger_little_right,
    ] {
        *f = enabled.clone();
    }
    let mut got: Vec<Vec<String>> = avatar_core::contacts::standard_senders(&avatar.model.descriptor.colliders, &nodes)
        .into_iter()
        .map(|s| s.collision_tags)
        .filter(|tags| tags.iter().any(|t| t.starts_with("Finger")))
        .map(|mut tags| {
            tags.sort();
            tags
        })
        .collect();
    got.sort();
    let mut want: Vec<Vec<String>> = ["Index", "Middle", "Ring", "Little"]
        .iter()
        .flat_map(|f| {
            ["L", "R"].map(|side| {
                let mut tags = vec!["Finger".to_string(), format!("Finger{side}"), format!("Finger{f}"), format!("Finger{f}{side}")];
                tags.sort();
                tags
            })
        })
        .collect();
    want.sort();
    assert_eq!(got, want);
}

/// A receiver listening for the wearer's own hand hears the standard sender on it, and only while it allows self.
#[test]
fn a_receiver_hears_the_wearers_own_hand() {
    let mut avatar = avatar_core::read(&std::fs::read(path("fixtures/unity-synthetic-dynamics.glb")).unwrap()).unwrap();
    let nodes = avatar_core::physbones::Nodes::read(&avatar).unwrap();
    let hand = avatar.model.descriptor.colliders.hand_left.bone.unwrap();
    let receiver = avatar_core::model::ContactReceiver {
        node: hand,
        enabled: true,
        shape: ContactShape::Sphere { radius: 0.05 },
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        collision_tags: vec!["HandL".to_string()],
        allow_self: true,
        allow_others: false,
        local_only: false,
        receiver_type: avatar_core::model::ReceiverType::Constant {},
        parameter: "Touched".to_string(),
    };
    avatar.model.contacts.receivers = vec![receiver.clone(), avatar_core::model::ContactReceiver { allow_self: false, ..receiver }];
    avatar.model.contacts.senders.clear();
    let mut contacts = avatar_core::contacts::Contacts::new(&avatar).unwrap();
    let pose = world(&nodes, &nodes.rest);
    let active = vec![true; nodes.parent.len()];
    contacts.advance(0.0, &pose, &active, &[], true);
    contacts.advance(1.0 / 60.0, &pose, &active, &[], true);
    assert_eq!(contacts.values(), &[Some(1.0), Some(0.0)]);
}
