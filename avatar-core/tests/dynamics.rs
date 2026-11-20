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

/// The core places VRChat's standard senders on custom colliders other than the fingers' where the recording found
/// them: on each collider's bone, with the collider's shape, offsets and the part and side tags. The fingers' and the
/// automatic ones', which the client places apart from the emulator, have their own tests.
#[test]
fn standard_senders_as_the_reference_placed_them() {
    let avatar = avatar_core::read(&std::fs::read(path("fixtures/unity-synthetic-dynamics.glb")).unwrap()).unwrap();
    let nodes = avatar_core::physbones::Nodes::read(&avatar).unwrap();
    let names = reference::paths(&avatar, &nodes);
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path("fixtures/reference/fixture-dynamics/meta.json")).unwrap()).unwrap();
    let c = &avatar.model.descriptor.colliders;
    let custom: Vec<&str> = [("Head", &c.head), ("Torso", &c.torso), ("HandL", &c.hand_left), ("HandR", &c.hand_right), ("FootL", &c.foot_left), ("FootR", &c.foot_right)]
        .into_iter()
        .filter(|(_, c)| c.state == avatar_core::model::DescriptorColliderState::Custom)
        .map(|(t, _)| t)
        .collect();
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
        .filter(|s| s.1.iter().any(|t| custom.contains(&t.as_str())))
        .collect();
    let mut got: Vec<(String, Vec<String>, f32, f32, Vec3, Quat)> = avatar_core::contacts::standard_senders(&avatar.model.descriptor, &avatar.model.humanoid, &nodes)
        .into_iter()
        .map(|s| {
            let ContactShape::Capsule { radius, height } = s.shape else { panic!("a standard sender is a capsule") };
            let mut tags = s.collision_tags;
            tags.sort();
            (names[s.node.0 as usize].clone().unwrap(), tags, radius, height, Vec3::from(s.position), Quat::from_array(s.rotation))
        })
        .filter(|s| s.1.iter().any(|t| custom.contains(&t.as_str())))
        .collect();
    want.sort_by(|a, b| a.1.cmp(&b.1));
    got.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(got.len(), 3);
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
    let mut got: Vec<Vec<String>> = avatar_core::contacts::standard_senders(&avatar.model.descriptor, &avatar.model.humanoid, &nodes)
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

/// Each standard finger sender lies where the VRChat client put it on the fixture's hands, one finger collider
/// configuration per finger: a capsule from the bone's parent to the bone, run on along the bone's offset in the
/// bone's own turned and scaled space to the collider's height, its radius scaled by the bone, its position and
/// rotation unused. The client's segment ends and radii, in the hand's space in Unity's axes, are fitted from
/// proximity receivers to within `CLIENT_FIT` (`docs/model.md`).
#[test]
fn standard_finger_senders_lie_where_the_client_put_them() {
    const CLIENT_FIT: f32 = 1e-5;
    struct Finger {
        name: &'static str,
        side: &'static str,
        parent: Option<([f32; 3], [f32; 4])>,
        bone: ([f32; 3], [f32; 4], f32),
        radius: f32,
        height: f32,
        position: [f32; 3],
        rotation: [f32; 4],
        client: ([f32; 3], [f32; 3], f32),
    }
    let id = [0.0, 0.0, 0.0, 1.0];
    let z90 = [0.0, 0.0, 0.70710677, 0.70710677];
    let y45 = [0.0, 0.38268343, 0.0, 0.9238795];
    let x90 = [0.70710677, 0.0, 0.0, 0.70710677];
    let fingers = [
        Finger { name: "Index", side: "L", parent: Some(([-0.04, 0.0, 0.15], id)), bone: ([-0.04, 0.0, 0.0], id, 1.0), radius: 0.01, height: 0.02, position: [0.0, 0.05, 0.0], rotation: z90, client: ([-0.04, 0.0, 0.15], [-0.08, 0.0, 0.15], 0.01) },
        Finger { name: "Middle", side: "L", parent: Some(([-0.04, 0.0, 0.05], id)), bone: ([-0.04, 0.0, 0.0], id, 1.0), radius: 0.015, height: 0.08, position: [0.0; 3], rotation: id, client: ([-0.04, 0.000001, 0.05], [-0.119999, 0.0, 0.05], 0.015) },
        Finger { name: "Ring", side: "L", parent: Some(([-0.04, 0.0, -0.05], id)), bone: ([-0.04, 0.0, 0.0], z90, 1.0), radius: 0.01, height: 0.08, position: [0.0; 3], rotation: id, client: ([-0.04, 0.0, -0.05], [-0.08, -0.04, -0.05], 0.01) },
        Finger { name: "Little", side: "L", parent: Some(([-0.04, 0.0, -0.15], id)), bone: ([-0.02, 0.02, 0.01], id, 1.0), radius: 0.02, height: 0.0, position: [0.0; 3], rotation: id, client: ([-0.040001, 0.0, -0.150001], [-0.06, 0.020001, -0.139999], 0.02) },
        Finger { name: "Index", side: "R", parent: Some(([0.04, 0.0, 0.15], id)), bone: ([0.03, 0.0, 0.0], id, 2.0), radius: 0.01, height: 0.06, position: [0.0; 3], rotation: id, client: ([0.04, 0.0, 0.15], [0.13, 0.0, 0.15], 0.02) },
        Finger { name: "Middle", side: "R", parent: Some(([0.04, 0.0, 0.05], y45)), bone: ([0.04, 0.0, 0.0], id, 1.0), radius: 0.03, height: 0.04, position: [0.0, 0.0, 0.03], rotation: id, client: ([0.04, 0.0, 0.050001], [0.068284, 0.0, 0.021715], 0.03) },
        Finger { name: "Ring", side: "R", parent: None, bone: ([0.06, 0.0, -0.05], id, 1.0), radius: 0.01, height: 0.1, position: [0.0; 3], rotation: x90, client: ([0.0, 0.0, 0.0], [0.076823, 0.0, -0.064018], 0.01) },
    ];
    let unity = |p: [f32; 3], r: [f32; 4], s: f32| reference::unity(&[p.to_vec(), r.to_vec(), vec![s; 3]].concat());
    let mut nodes = avatar_core::physbones::Nodes { children: vec![vec![]], parent: vec![None], rest: vec![Transform::IDENTITY], names: vec![String::new()] };
    let mut add = |parent: usize, t: Transform| {
        let i = nodes.rest.len();
        nodes.rest.push(t);
        nodes.parent.push(Some(parent));
        nodes.children.push(vec![]);
        nodes.children[parent].push(i);
        nodes.names.push(String::new());
        i
    };
    let avatar = avatar_core::read(&std::fs::read(path("fixtures/unity-synthetic-dynamics.glb")).unwrap()).unwrap();
    let mut colliders = avatar.model.descriptor.colliders.clone();
    let disabled = avatar_core::model::DescriptorCollider { state: avatar_core::model::DescriptorColliderState::Disabled, ..colliders.hand_right.clone() };
    for c in [&mut colliders.head, &mut colliders.torso, &mut colliders.hand_left, &mut colliders.hand_right, &mut colliders.foot_left, &mut colliders.foot_right, &mut colliders.finger_little_right] {
        *c = disabled.clone();
    }
    let mut world = vec![Transform::IDENTITY];
    for f in &fingers {
        let parent = match f.parent {
            Some((p, r)) => add(0, unity(p, r, 1.0)),
            None => 0,
        };
        let bone = add(parent, unity(f.bone.0, f.bone.1, f.bone.2));
        let slot = match (f.name, f.side) {
            ("Index", "L") => &mut colliders.finger_index_left,
            ("Middle", "L") => &mut colliders.finger_middle_left,
            ("Ring", "L") => &mut colliders.finger_ring_left,
            ("Little", "L") => &mut colliders.finger_little_left,
            ("Index", "R") => &mut colliders.finger_index_right,
            ("Middle", "R") => &mut colliders.finger_middle_right,
            ("Ring", "R") => &mut colliders.finger_ring_right,
            _ => unreachable!(),
        };
        let at = unity(f.position, f.rotation, 1.0);
        *slot = avatar_core::model::DescriptorCollider {
            state: avatar_core::model::DescriptorColliderState::Custom,
            bone: Some(avatar_core::model::NodeIndex(bone as u32)),
            radius: f.radius,
            height: f.height,
            position: at.translation.to_array(),
            rotation: at.rotation.to_array(),
        };
    }
    for i in 1..nodes.rest.len() {
        world.push(world[nodes.parent[i].unwrap()].mul(&nodes.rest[i]));
    }
    let descriptor = avatar_core::model::Descriptor { colliders, ..avatar.model.descriptor.clone() };
    let senders = avatar_core::contacts::standard_senders(&descriptor, &avatar.model.humanoid, &nodes);
    assert_eq!(senders.len(), fingers.len());
    for f in &fingers {
        let tag = format!("Finger{}{}", f.name, f.side);
        let s = senders.iter().find(|s| s.collision_tags.contains(&tag)).unwrap();
        let ContactShape::Capsule { radius, height } = s.shape else { panic!("a finger sender is a capsule") };
        let at = &world[s.node.0 as usize];
        let center = at.point(Vec3::from(s.position));
        let half = (at.rotation * Quat::from_array(s.rotation)) * Vec3::Y * ((height * 0.5 - radius) * at.scale.x).max(0.0);
        let (a, b) = (unity(f.client.0, id, 1.0).translation, unity(f.client.1, id, 1.0).translation);
        let ends = [center - half, center + half];
        let near = ends.iter().map(|e| (*e - a).length()).fold(f32::MAX, f32::min);
        let far = ends.iter().map(|e| (*e - b).length()).fold(f32::MAX, f32::min);
        let r = radius * at.scale.x;
        assert!(near < CLIENT_FIT && far < CLIENT_FIT && (r - f.client.2).abs() < CLIENT_FIT, "{tag}: ends {ends:?} radius {r} against the client's {a} {b} {}", f.client.2);
    }
}

/// On a humanoid avatar each automatic collider's standard sender lies where the VRChat client put it on the fixture
/// with humanoid finger chains, whatever the descriptor stores: the collider the SDK computes from the skeleton at rest,
/// placed as the client places a finger's or a body part's. The client's capsule ends and radii, each in the frame of
/// the bone that end moves with and in Unity's axes, are fitted from proximity receivers to within `CLIENT_FIT`
/// (`docs/model.md`). The palms' centres, heights and radii and the head's radius match; the palms' axis and the head's
/// height do not (not modelled).
/// A finger the humanoid lacks an intermediate bone for, a custom collider with no bone and a disabled one place none.
#[test]
fn automatic_senders_lie_where_the_client_put_them() {
    const CLIENT_FIT: f32 = 1e-5;
    let bones: [(&str, &str, [f32; 3], [f32; 4], f32, &str); 43] = [
        ("Armature", "", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, ""),
        ("Hips", "Armature", [0.0, 0.95, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "Hips"),
        ("Spine", "Hips", [0.0, 0.1, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "Spine"),
        ("Chest", "Spine", [0.0, 0.15, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "Chest"),
        ("Neck", "Chest", [0.0, 0.2, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "Neck"),
        ("Head", "Neck", [0.0, 0.1, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "Head"),
        ("LeftShoulder", "Chest", [-0.05, 0.15, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftShoulder"),
        ("LeftUpperArm", "LeftShoulder", [-0.1, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftUpperArm"),
        ("LeftLowerArm", "LeftUpperArm", [-0.26, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftLowerArm"),
        ("LeftHand", "LeftLowerArm", [-0.24, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftHand"),
        ("LeftIndexProximal", "LeftHand", [-0.08, 0.0, 0.03], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftIndexProximal"),
        ("LeftIndexIntermediate", "LeftIndexProximal", [-0.03, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftIndexIntermediate"),
        ("LeftIndexDistal", "LeftIndexIntermediate", [-0.025, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftIndexDistal"),
        ("LeftMiddleProximal", "LeftHand", [-0.085, 0.0, 0.01], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftMiddleProximal"),
        ("LeftMiddleIntermediate", "LeftMiddleProximal", [-0.035, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftMiddleIntermediate"),
        ("LeftMiddleDistal", "LeftMiddleIntermediate", [-0.03, 0.006, 0.0], [0.0, 0.0, 0.0, 1.0], 1.5, "LeftMiddleDistal"),
        ("LeftRingProximal", "LeftHand", [-0.08, 0.0, -0.01], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftRingProximal"),
        ("LeftRingIntermediate", "LeftRingProximal", [-0.032, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftRingIntermediate"),
        ("LeftRingDistal", "LeftRingIntermediate", [-0.026, 0.0, 0.0], [0.0, 0.2588191, 0.0, 0.9659259], 1.0, "LeftRingDistal"),
        ("LeftLittleProximal", "LeftHand", [-0.075, 0.0, -0.03], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftLittleProximal"),
        ("LeftLittleIntermediate", "LeftLittleProximal", [-0.025, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftLittleIntermediate"),
        ("RightShoulder", "Chest", [0.05, 0.15, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightShoulder"),
        ("RightUpperArm", "RightShoulder", [0.1, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightUpperArm"),
        ("RightLowerArm", "RightUpperArm", [0.26, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightLowerArm"),
        ("RightHand", "RightLowerArm", [0.24, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightHand"),
        ("RightIndexProximal", "RightHand", [0.08, 0.0, 0.03], [0.0, 0.0, -0.1736482, 0.9848078], 1.0, "RightIndexProximal"),
        ("RightIndexIntermediate", "RightIndexProximal", [0.03, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightIndexIntermediate"),
        ("RightIndexDistal", "RightIndexIntermediate", [0.028, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightIndexDistal"),
        ("RightMiddleProximal", "RightHand", [0.085, 0.0, 0.01], [0.0, 0.0, 0.0, 1.0], 1.0, "RightMiddleProximal"),
        ("RightMiddleIntermediate", "RightMiddleProximal", [0.035, 0.0, 0.0], [0.0, 0.0, 0.1305262, 0.9914449], 1.0, "RightMiddleIntermediate"),
        ("RightMiddleDistal", "RightMiddleIntermediate", [0.03, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightMiddleDistal"),
        ("RightRingProximal", "RightHand", [0.08, 0.0, -0.01], [0.0, 0.0, 0.0, 1.0], 1.0, "RightRingProximal"),
        ("RightRingIntermediate", "RightRingProximal", [0.032, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightRingIntermediate"),
        ("RightRingDistal", "RightRingIntermediate", [0.026, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightRingDistal"),
        ("RightLittleProximal", "RightHand", [0.075, 0.0, -0.03], [0.0, 0.0, 0.0, 1.0], 1.0, "RightLittleProximal"),
        ("LeftUpperLeg", "Hips", [-0.1, -0.05, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftUpperLeg"),
        ("LeftLowerLeg", "LeftUpperLeg", [0.0, -0.42, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftLowerLeg"),
        ("LeftFoot", "LeftLowerLeg", [0.0, -0.42, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftFoot"),
        ("LeftToes", "LeftFoot", [0.0, -0.04, 0.12], [0.0, 0.0, 0.0, 1.0], 1.0, "LeftToes"),
        ("RightUpperLeg", "Hips", [0.1, -0.05, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightUpperLeg"),
        ("RightLowerLeg", "RightUpperLeg", [0.0, -0.42, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightLowerLeg"),
        ("RightFoot", "RightLowerLeg", [0.0, -0.42, 0.0], [0.0, 0.0, 0.0, 1.0], 1.0, "RightFoot"),
        ("RightToes", "RightFoot", [0.0, -0.04, 0.12], [0.0, 0.0, 0.0, 1.0], 1.0, "RightToes"),
    ];
    let fingers = [
        ("FingerIndexL", [-2e-07, 4e-07, -1e-07], [-0.025, -2e-07, -3e-07], 0.01, 0.0100001),
        ("FingerIndexR", [-7e-07, -1e-07, -2e-07], [0.028, 4e-07, -2e-07], 0.0111996, 0.0112002),
        ("FingerMiddleL", [7e-07, 0.0, 0.0], [-0.0100004, 0.002, -2e-07], 0.0122372, 0.0081581),
        ("FingerMiddleR", [-3e-07, 0.0, 0.0], [0.0300004, 3e-07, 0.0], 0.0119995, 0.0119997),
        ("FingerRingL", [-3e-07, -1e-07, -0.0], [-0.0260002, -0.0, 2e-07], 0.0104002, 0.0104002),
        ("FingerLittleL", [-0.0, 3e-07, 2e-07], [-0.0249997, -2e-07, -1e-07], 0.0099998, 0.0099998),
    ];
    let bodies = [
        ("Torso", [0.0, -0.0999996, 4e-07], [0.0, 0.05, 3e-07], 0.1500002),
        ("FootL", [-1e-07, -1e-07, -0.0], [-2e-07, 4e-07, -0.0], 0.0839999),
        ("HandL", [-0.068, 0.0040672, 0.0195814], [-0.0680001, -0.0040678, -0.0195826], 0.0199999),
        ("HandR", [0.0679998, 0.0040673, 0.019582], [0.0680002, -0.0040673, -0.0195811], 0.02),
        ("Head", [2e-07, 0.1056022, -2e-07], [1e-07, 0.1056023, -3e-07], 0.0990002),
    ];

    let unity = |p: [f32; 3], r: [f32; 4], s: f32| reference::unity(&[p.to_vec(), r.to_vec(), vec![s; 3]].concat());
    let point = |p: [f32; 3]| Vec3::new(-p[0], p[1], p[2]);
    let mut nodes = avatar_core::physbones::Nodes { children: vec![], parent: vec![], rest: vec![], names: vec![] };
    let index = |name: &str| bones.iter().position(|b| b.0 == name).unwrap();
    for (i, b) in bones.iter().enumerate() {
        let parent = (!b.1.is_empty()).then(|| index(b.1));
        nodes.children.push(vec![]);
        nodes.parent.push(parent);
        nodes.rest.push(unity(b.2, b.3, b.4));
        nodes.names.push(b.0.to_string());
        if let Some(p) = parent {
            nodes.children[p].push(i);
        }
    }
    let avatar = avatar_core::read(&std::fs::read(path("fixtures/unity-synthetic-dynamics.glb")).unwrap()).unwrap();
    let mut humanoid = avatar.model.humanoid.clone();
    humanoid.bones = bones
        .iter()
        .enumerate()
        .filter(|(_, b)| !b.5.is_empty())
        .map(|(i, b)| {
            let bone: avatar_core::model::HumanBodyBone = serde_json::from_value(serde_json::Value::from(b.5)).unwrap();
            (bone, avatar_core::model::HumanBone { node: avatar_core::model::NodeIndex(i as u32), limit: None })
        })
        .collect();
    use avatar_core::model::{DescriptorCollider, DescriptorColliderState as State, NodeIndex};
    let stale = |state: State, bone: Option<usize>| DescriptorCollider {
        state,
        bone: bone.map(|b| NodeIndex(b as u32)),
        radius: 0.005,
        height: 0.01,
        position: [0.0, 0.3, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    let hips = Some(index("Hips"));
    let mut descriptor = avatar.model.descriptor.clone();
    descriptor.view_position = [0.0, 1.62, 0.08];
    let c = &mut descriptor.colliders;
    for slot in [&mut c.head, &mut c.torso, &mut c.hand_left, &mut c.hand_right, &mut c.foot_left, &mut c.finger_index_left, &mut c.finger_middle_left, &mut c.finger_ring_left, &mut c.finger_index_right, &mut c.finger_little_right] {
        *slot = stale(State::Automatic, hips);
    }
    c.finger_little_left = stale(State::Automatic, None);
    c.finger_middle_right = stale(State::Automatic, None);
    c.foot_right = stale(State::Disabled, Some(index("RightToes")));
    c.finger_ring_right = DescriptorCollider { radius: 0.008, height: 0.07, ..stale(State::Custom, None) };
    let senders = avatar_core::contacts::standard_senders(&descriptor, &humanoid, &nodes);
    assert_eq!(senders.len(), 11);
    let sender = |tag: &str| senders.iter().find(|s| s.collision_tags.iter().any(|t| t == tag));
    for tag in ["FingerLittleR", "FingerRingR", "FootR"] {
        assert!(sender(tag).is_none(), "{tag} placed a sender the client did not");
    }
    let local = |s: &avatar_core::model::ContactSender| {
        let ContactShape::Capsule { radius, height } = s.shape else { panic!("a standard sender is a capsule") };
        let half = Quat::from_array(s.rotation) * Vec3::Y * (height * 0.5 - radius).max(0.0);
        let center = Vec3::from(s.position);
        ([center - half, center + half], radius)
    };
    let near = |ends: &[Vec3; 2], p: Vec3| ends.iter().map(|e| (*e - p).length()).fold(f32::MAX, f32::min);
    for (tag, a, b, ra, rb) in fingers {
        let s = sender(tag).unwrap_or_else(|| panic!("{tag} placed no sender"));
        let (ends, radius) = local(s);
        let [first, second] = ends;
        let (start, end) = if first.length() < second.length() { (first, second) } else { (second, first) };
        let bone = nodes.children[s.node.0 as usize].iter().copied().find(|&c| humanoid.bones.values().any(|h| h.node.0 as usize == c)).unwrap();
        let t = &nodes.rest[bone];
        let end_in_bone = (t.rotation.inverse() * (end - t.translation)) / t.scale;
        let rb_got = radius / t.scale.max_element();
        assert!(
            (start - point(a)).length() < CLIENT_FIT && (end_in_bone - point(b)).length() < CLIENT_FIT && (radius - ra).abs() < CLIENT_FIT && (rb_got - rb).abs() < CLIENT_FIT,
            "{tag}: ends {start} {end_in_bone} radius {radius} against the client's {a:?} {b:?} {ra}"
        );
    }
    for (tag, a, b, r) in bodies {
        let s = sender(tag).unwrap_or_else(|| panic!("{tag} placed no sender"));
        let on = match tag {
            "Torso" => "Chest",
            "FootL" => "LeftToes",
            "HandL" => "LeftHand",
            "HandR" => "RightHand",
            _ => "Head",
        };
        assert_eq!(nodes.names[s.node.0 as usize], on, "{tag}'s bone");
        let (ends, radius) = local(s);
        let (a, b) = (point(a), point(b));
        let centre = (ends[0] + ends[1]) * 0.5;
        let shape = match tag {
            "Head" => (centre.x - a.x).abs() < CLIENT_FIT && (centre.z - a.z).abs() < CLIENT_FIT,
            "HandL" | "HandR" => (centre - (a + b) * 0.5).length() < CLIENT_FIT && ((ends[0] - ends[1]).length() - (a - b).length()).abs() < CLIENT_FIT,
            _ => near(&ends, a) < CLIENT_FIT && near(&ends, b) < CLIENT_FIT,
        };
        assert!(shape && (radius - r).abs() < CLIENT_FIT, "{tag}: ends {ends:?} radius {radius} against the client's {a} {b} {r}");
    }
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
