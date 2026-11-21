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
    Input { dt, root: Transform::IDENTITY, poses: &[], held: &[], senders: &[], hands: &[] }
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
    assert!(ev.grab(c, tip, at, Toucher::Wearer));
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

/// Another avatar's hands, as its own evaluator places them and given as host input, push a chain that allows others
/// and none that refuses them; the avatar's own hand and finger are the descriptor's.
#[test]
fn another_avatars_hands_push_what_allows_others() {
    let mut base = fixture();
    let tail = base.model.physbones.iter().position(|p| p.parameter.as_deref() == Some("Tail")).unwrap();
    base.model.physbones[tail].radius.value = 0.03;
    let run = |allow_others: bool, hands: &[avatar_core::physbones::HandCollider]| -> Vec<glam::Vec3> {
        let mut avatar = base.clone();
        avatar.model.physbones[tail].collision = avatar_core::model::Permission { allow_self: true, allow_others };
        let mut ev = Evaluator::new(&avatar).unwrap();
        let tip = ev.physbones().bones(tail).last().unwrap();
        (0..40)
            .map(|k| {
                ev.evaluate(&Input { hands: if k < 10 { &[] } else { hands }, ..input(if k == 0 { 0.0 } else { 1.0 / 60.0 }) });
                ev.output().space[tip].translation
            })
            .collect()
    };
    let free = run(true, &[]);
    let mut other = Evaluator::new(&base).unwrap();
    let mut step = |root: glam::Vec3, dt: f32| {
        other.evaluate(&Input { root: Transform { translation: root, ..Transform::IDENTITY }, ..input(dt) });
        other.hands(1)
    };
    step(glam::Vec3::ZERO, 0.0);
    let mut hand = vec![];
    for _ in 0..10 {
        hand = step(glam::Vec3::ZERO, 1.0 / 60.0);
    }
    assert_eq!(hand.len(), 2, "the fixture's left hand and index finger");
    let shift = free[10] - hand[0].at.point(hand[0].position) + glam::Vec3::new(0.06, 0.0, 0.0);
    let hands = step(shift, 1.0 / 60.0);
    let moved = |a: &[glam::Vec3], b: &[glam::Vec3]| a.iter().zip(b).map(|(x, y)| x.distance(*y)).fold(0.0f32, f32::max);
    let pushed = moved(&free, &run(true, &hands));
    assert!(pushed > 0.01, "a hand that the chain allows moved its tip {pushed} m");
    assert!(run(false, &[]) == run(false, &hands), "a hand that the chain refuses moved its tip");
}

#[test]
fn a_renderer_draws_only_under_active_ancestors() {
    let base = fixture();
    let parents = Evaluator::new(&base).unwrap().parents().to_vec();
    let (r, ancestor) = base
        .model
        .renderers
        .iter()
        .enumerate()
        .find_map(|(r, x)| parents[x.node.0 as usize].map(|p| (r, p)))
        .expect("a renderer below another node");
    let mut e = Evaluator::new(&base).unwrap();
    e.evaluate(&input(0.0));
    let mut hidden = base.clone();
    hidden.model.nodes[ancestor].active = false;
    let mut h = Evaluator::new(&hidden).unwrap();
    h.evaluate(&input(0.0));
    assert!(e.shown(r), "the renderer draws under active nodes");
    assert!(!h.shown(r), "a renderer below an inactive node does not draw");
}
