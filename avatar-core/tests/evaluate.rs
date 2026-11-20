//! The core's frame evaluation: its reference run is deterministic, and dynamics reach the next frame's animator.

mod reference;

use std::path::Path;

use avatar_core::evaluate::{Evaluator, Input};
use avatar_core::physbones::{Toucher, Transform};

fn fixture() -> avatar_core::Avatar {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    avatar_core::read(&std::fs::read(root.join("fixtures/unity-synthetic.glb")).unwrap()).unwrap()
}

fn input(dt: f32) -> Input<'static> {
    Input { dt, root: Transform::IDENTITY, poses: &[], held: &[], senders: &[], local: true }
}

#[test]
fn two_reference_runs_are_identical() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/reference/fixture-animator");
    let avatar = fixture();
    let a = reference::run::run(&avatar, &dir, Some(1200));
    let b = reference::run::run(&avatar, &dir, Some(1200));
    assert_eq!(a.tips.lines().count(), b.tips.lines().count());
    assert!(a == b, "two runs differ");
    assert!(a.tips.lines().count() >= 1200 * 13, "{} tip samples", a.tips.lines().count());
    for key in ["\tlocal:", "\tactive:", "\tshape:", "\tmat:", "\tslot:"] {
        assert!(a.output.contains(key), "the output logs no {key}");
    }
    assert!(a.parameters.lines().count() > 100 && a.states.lines().count() > 10);
}

#[test]
fn a_grab_reaches_the_animator_after_its_frame() {
    let avatar = fixture();
    let (c, name) = avatar
        .model
        .physbones
        .iter()
        .enumerate()
        .find_map(|(i, p)| p.parameter.clone().filter(|_| p.grabbing.allow_self).map(|n| (i, n)))
        .expect("the fixture has a grabbable PhysBone with a parameter");
    let mut ev = Evaluator::new(&avatar).unwrap();
    let key = format!("{name}_IsGrabbed");
    assert!(ev.animator().get(&key).is_some(), "the animator declares {key}");
    ev.evaluate(&input(0.0));
    ev.evaluate(&input(1.0 / 60.0));
    assert_eq!(ev.animator().get(&key), Some(0.0));
    let tip = ev.physbones().bones(c).last().unwrap();
    let at = ev.output().space[tip].translation;
    assert!(ev.physbones_mut().grab(c, tip, at, Toucher::Wearer));
    ev.evaluate(&input(1.0 / 60.0));
    assert_eq!(ev.animator().get(&key), Some(1.0));
}

/// The root's motion in the world swings a chain in the avatar's root space, and the output carries it.
#[test]
fn moving_the_root_swings_a_chain() {
    let avatar = fixture();
    let mut ev = Evaluator::new(&avatar).unwrap();
    ev.evaluate(&input(0.0));
    let rest = ev.output().space.clone();
    let mut swing = 0.0f32;
    for k in 1..=30 {
        let root = Transform { translation: glam::Vec3::new(0.0, 0.0, k as f32 * 0.05), ..Transform::IDENTITY };
        ev.evaluate(&Input { root, ..input(1.0 / 60.0) });
        for n in ev.physbones().bones(0).skip(1) {
            swing = swing.max((ev.output().space[n].translation - rest[n].translation).length());
        }
    }
    assert!(swing > 1e-3, "the chain swung {swing} m");
}
