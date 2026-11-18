//! The core's constraints and contacts against reference traces recorded by `reference/` from the VRChat SDK's own
//! dynamics: the dynamics probe (one constraint or receiver per setting value, and senders outside the avatar) and
//! the fixture. `fixtures/reference/` holds the recordings, trimmed to what the replay reads.

mod reference;

use std::path::{Path, PathBuf};

use avatar_core::physbones::Transform;
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

fn assert_targets(report: &Report, unmeasured: &[&str]) {
    let mut failed = vec![];
    for (target, e) in &report.targets {
        if unmeasured.contains(&target.as_str()) {
            assert!(e.unmeasured > 0 && e.frames == 0, "{target} reads no PhysBone");
            continue;
        }
        assert!(e.frames > 0 && e.unmeasured == 0, "{target}: {} frames scored, {} unmeasured", e.frames, e.unmeasured);
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
/// local solving, sources summing past one, and constraints chained on one another; the one that follows a
/// PhysBone's tip reads a pose the recording does not hold.
#[test]
fn each_constraint_setting_against_the_probe() {
    let report = replay("fixtures/reference/dynamics", "fixtures/unity-dynamics.glb");
    assert_eq!(report.targets.len(), 65);
    assert_targets(&report, &["Constraints/Parent PhysBone source"]);
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
    assert_targets(&report, &[]);
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
