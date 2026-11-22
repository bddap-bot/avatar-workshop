//! The core's PhysBones against reference traces recorded by `reference/` from the VRChat SDK's own simulation:
//! the probe (one chain per field value), the fixture's scripted motion, and the fixture's grabs.
//! `fixtures/reference/` holds the recordings, trimmed to what the replays read.

mod reference;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reference::{errors, median, parameters, Replay};

fn path(p: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(p)
}

fn replay(recording: &str, model: &str, frames: std::ops::Range<usize>) -> Replay {
    let avatar = avatar_core::read(&std::fs::read(path(model)).unwrap()).unwrap();
    Replay::load(&path(recording), avatar, frames)
}

/// Each probe, the fields it isolates, and the largest tip RMS error over chain length any of its bones may
/// show (each measured, with a margin). The error grows where the motion is chaotic: a chain this lightly
/// damped turns a small difference into a different swing, as the high-spring probe's flailing does.
const PROBES: &[(&str, &str, f32)] = &[
    ("Base", "pull 0.2 and spring 0.2, simplified", 0.02),
    ("Pull05", "pull", 0.025),
    ("Pull50", "pull", 0.01),
    ("Pull90", "pull", 0.015),
    ("PullCurve", "a curve along the chain", 0.01),
    ("Spring0", "spring", 0.01),
    ("Spring60", "spring", 0.09),
    ("Spring95", "spring", 0.85),
    ("Version10", "version 1.0", 0.01),
    ("Momentum20Stiff0", "advanced integration: momentum", 0.05),
    ("Momentum20Stiff50", "advanced integration: stiffness", 0.11),
    ("Momentum80Stiff20", "advanced integration: momentum and stiffness", 0.04),
    ("Momentum80Stiff80", "advanced integration: momentum and stiffness", 0.09),
    ("Advanced10", "advanced integration, version 1.0", 0.025),
    ("Gravity30", "gravity, version 1.1", 0.01),
    ("Gravity100", "gravity, version 1.1", 0.01),
    ("Gravity50Down", "gravity along the rest direction", 0.01),
    ("Gravity50Falloff50", "gravity falloff", 0.05),
    ("Gravity50Falloff100", "gravity falloff", 0.015),
    ("Gravity50Version10", "gravity, version 1.0", 0.02),
    ("Gravity50Advanced", "gravity, advanced integration", 0.015),
    ("Immobile50", "immobile, all motion", 0.01),
    ("Immobile100", "immobile, all motion", 0.01),
    ("Immobile50World", "immobile, world", 0.01),
    ("Angle30", "angle limit", 0.01),
    ("Angle30Rotated", "angle limit with rotation", 0.01),
    ("Hinge40", "hinge limit", 0.01),
    ("Hinge40Rotated", "hinge limit with rotation", 0.02),
    ("Polar50x10", "polar limit", 0.01),
    ("Stretch50", "max stretch without stretch motion", 0.01),
    ("Stretch50Motion100", "max stretch with stretch motion", 0.075),
    ("Squish50Motion100", "max squish", 0.05),
    ("Stretch50Motion25", "max stretch with partial stretch motion", 0.04),
    ("Stretch50Motion50", "max stretch with partial stretch motion", 0.05),
    ("Stretch50Motion75", "max stretch with partial stretch motion", 0.05),
    ("Stretch50Motion50Spring60", "partial stretch motion with spring", 0.04),
    ("Stretch50Motion50Level", "partial stretch motion, level", 0.05),
    ("Stretch50Motion50LevelRight", "partial stretch motion, level, across the motion", 0.045),
    ("Stretch50Motion100Level", "full stretch motion, level", 0.05),
    ("Stretch50Motion50Sphere", "partial stretch motion against a sphere collider", 0.05),
    ("Squish50Motion50", "max squish with partial stretch motion", 0.015),
    ("Stretch50Advanced", "max stretch without stretch motion, advanced integration", 0.01),
    ("Stretch50Motion50Advanced", "partial stretch motion, advanced integration", 0.02),
    ("Stretch50Motion100Advanced", "full stretch motion, advanced integration", 0.025),
    ("Stretch50Motion50Stiff70Advanced", "partial stretch motion, advanced integration, high stiffness", 0.03),
    ("Stretch50Motion50LevelAdvanced", "partial stretch motion, level, advanced integration", 0.035),
    ("Stretch50Motion50LevelRightAdvanced", "partial stretch motion, level, across the motion, advanced integration", 0.035),
    ("Stretch50Motion100LevelAdvanced", "full stretch motion, level, advanced integration", 0.045),
    ("Stretch50Motion50LevelStiff70Advanced", "partial stretch motion, level, advanced integration, high stiffness", 0.045),
    ("Stretch30Squish40Motion50Advanced", "stretch and squish with partial stretch motion, advanced integration", 0.05),
    ("Endpoint", "endpoint position", 0.01),
    ("Sphere", "sphere collider, bone radius 0", 0.01),
    ("SphereInside", "inside-bounds sphere collider", 0.01),
    ("Capsule", "rotated capsule collider, bone radius 0", 0.01),
    ("CapsuleRadius", "capsule collider, bone radius", 0.04),
    ("CapsuleBonesAsSpheres", "capsule collider treating bones as spheres", 0.025),
    ("Plane", "rotated plane collider", 0.27),
    ("BentGravity", "a bent version 1.0 chain: gravity", 0.015),
    ("BentGravity11", "a bent chain: gravity, version 1.1", 0.01),
    ("BentGravityCurve", "a bent chain: gravity curve", 0.015),
    ("BentPullCurve", "a bent chain: pull curve", 0.015),
    ("BentFalloff", "a bent chain: gravity falloff", 0.05),
    ("BentFalloff11", "a bent chain: gravity falloff, version 1.1", 0.04),
    ("BentAngle", "a bent chain: angle limit", 0.01),
    ("BentAngleRotated", "a bent chain: angle limit with rotation", 0.025),
    ("BentAngle11", "a bent chain: angle limit, version 1.1", 0.01),
    ("BentRootInside", "a bent chain: its root inside a sphere collider", 0.05),
    ("BentTail", "a bent chain: the fixture tail's fields but its colliders", 0.025),
];

#[test]
fn each_field_against_its_probe() {
    let dir = path("fixtures/reference/probe");
    let replay = replay("fixtures/reference/probe", "fixtures/unity-probe.glb", 0..usize::MAX);
    let errors = errors(&dir, &replay.run(60.0), reference::window(&dir));
    let mut worst = BTreeMap::<&str, f32>::new();
    for (e, tip) in &errors {
        assert!(e.is_finite(), "{tip}: {e}");
        let chain = tip.split('|').next().unwrap().rsplit('/').next().unwrap();
        let (probe, ..) = PROBES.iter().find(|p| p.0 == chain).unwrap_or_else(|| panic!("{chain} is not in PROBES"));
        let w = worst.entry(probe).or_default();
        *w = w.max(*e);
    }
    let mut failed = vec![];
    for (probe, field, bound) in PROBES {
        let e = worst.get(probe).copied().unwrap_or_else(|| panic!("the recording has no {probe} tips"));
        eprintln!("{probe:<22} {e:.4} (bound {bound}) {field}");
        if e > *bound {
            failed.push(format!("{probe} ({field}): {e:.4} over {bound}"));
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

#[test]
fn fixture_median_and_multi_child_types() {
    let dir = path("fixtures/reference/fixture");
    let replay = replay("fixtures/reference/fixture", "fixtures/unity-synthetic.glb", 0..usize::MAX);
    let errors = errors(&dir, &replay.run(60.0), reference::window(&dir));
    for (e, tip) in &errors {
        eprintln!("{e:.4} {tip}");
    }
    assert_eq!(errors.len(), 13);
    let median = median(&errors);
    assert!(median <= 0.05, "median {median}");
    assert!(errors.iter().all(|(e, _)| e.is_finite()));
    let worst = |chain: &str| {
        let chain: Vec<f32> = errors.iter().filter(|(_, t)| t.starts_with(chain)).map(|(e, _)| *e).collect();
        assert!(!chain.is_empty(), "no tips of {chain:?}");
        chain.into_iter().fold(0.0f32, f32::max)
    };
    assert!(worst("Armature/Hips/Spine/Chest/Neck/Head/Hair|") <= 0.035, "first child drives the root");
    assert!(worst("Armature/Hips/Skirt|") <= 0.03, "the children's average drives the root");
    assert!(worst("Armature/Hips/Spine/Chest/Neck/Head/Antenna|") <= 0.12, "a root with several children stays");
}

#[test]
fn grabs_poses_and_their_filters() {
    let dir = path("fixtures/reference/fixture-grabs");
    let (grab, ear, end) = (reference::step(&dir, "grab Tail"), reference::step(&dir, "grab EarL"), reference::step(&dir, "final"));
    let replay = replay("fixtures/reference/fixture-grabs", "fixtures/unity-synthetic.glb", grab - 103..end);
    assert!(replay.grabs.values().any(|g| g.event == "miss"), "the antenna, which allows no grabbing, refuses its grab");
    let trace = replay.run(60.0);
    let recorded = parameters(&dir);
    for name in ["Tail_IsGrabbed", "Tail_IsPosed", "EarL_IsGrabbed"] {
        for (f, ours) in trace.frames.iter().zip(&trace.parameters) {
            let f = f.round() as usize;
            assert_eq!(ours[name], reference::at(&recorded[name], f), "{name} at frame {f}");
        }
    }
    let tips = |id: &str, frames: std::ops::Range<usize>| -> Vec<glam::Vec3> {
        frames.map(|f| trace.tips[f - replay.first][id]).collect()
    };
    let recorded_tips = |id: &str, frames: std::ops::Range<usize>| -> Vec<glam::Vec3> {
        let text = reference::text(&dir.join("tips.tsv"));
        let rows: BTreeMap<usize, glam::Vec3> = text
            .lines()
            .map(|l| l.split('\t').collect::<Vec<_>>())
            .filter(|r| r[1] == id)
            .map(|r| (r[0].parse().unwrap(), glam::Vec3::new(r[2].parse().unwrap(), r[3].parse().unwrap(), r[4].parse().unwrap())))
            .collect();
        frames.map(|f| rows[&f]).collect()
    };
    let held = "Armature/Hips/Spine/Chest/Neck/Head/EarL.0|Armature/Hips/Spine/Chest/Neck/Head/EarL.0/EarL.1/EarL.2/EarL.3";
    let (ours, theirs) = (tips(held, ear..end), recorded_tips(held, ear..end));
    let rms = (ours.iter().zip(&theirs).map(|(a, b)| a.distance_squared(*b)).sum::<f32>() / ours.len() as f32).sqrt();
    assert!(rms / 0.1746 <= 0.2, "held ear tip RMS error {rms} m");
    let tail = "Armature/Hips/Tail.0|Armature/Hips/Tail.0/Tail.1/Tail.2/Tail.3/Tail.4";
    let moved = |p: Vec<glam::Vec3>| p[p.len() - 1] - p[0];
    let (ours, theirs) = (moved(tips(tail, grab..grab + 47)), moved(recorded_tips(tail, grab..grab + 47)));
    assert!(ours.distance(theirs) <= 0.03, "the held tail moved {ours}, the reference's {theirs}");
}

#[test]
fn angle_stretch_and_squish_parameters() {
    let dir = path("fixtures/reference/fixture");
    let replay = replay("fixtures/reference/fixture", "fixtures/unity-synthetic.glb", 0..usize::MAX);
    let trace = replay.run(60.0);
    let recorded = parameters(&dir);
    let window = reference::window(&dir);
    for (name, bound) in [("EarL_Angle", 0.02), ("Tail_Angle", 0.1), ("EarL_Stretch", 0.06), ("EarL_Squish", 0.02), ("Tail_Stretch", 0.0)] {
        let diffs: Vec<f32> = trace
            .frames
            .iter()
            .zip(&trace.parameters)
            .filter(|(f, _)| window.contains(&(f.round() as usize)))
            .map(|(f, p)| (p[name] - reference::at(&recorded[name], f.round() as usize)).abs())
            .collect();
        let mean = diffs.iter().sum::<f32>() / diffs.len() as f32;
        eprintln!("{name} mean |difference| {mean:.4}");
        assert!(mean <= bound, "{name}: mean difference {mean} over {bound}");
    }
}

/// Runs at 72, 90 and 120 Hz agree with the 60 Hz run, at the times both hold a frame, within 1% of chain length over
/// the motion; over the hand probe's sweeps, whose hands move at constant speeds so steps between host frames meet
/// them exactly where they are, within 0.1%.
#[test]
fn host_rates_agree() {
    for (recording, model) in [
        ("fixtures/reference/fixture", "fixtures/unity-synthetic.glb"),
        ("fixtures/reference/probe", "fixtures/unity-probe.glb"),
        ("fixtures/reference/hands", "fixtures/unity-hands.glb"),
    ] {
        let dir = path(recording);
        let hands = recording.ends_with("hands");
        let window = if hands { reference::step(&dir, "self sweep")..reference::step(&dir, "final") } else { reference::window(&dir) };
        let bound = if hands { 0.001 } else { 0.01 };
        let replay = replay(recording, model, 0..window.end);
        let lengths: BTreeMap<String, f32> = reference::text(&dir.join("chains.tsv")).lines().skip(1).map(|l| l.split_once('\t').unwrap()).map(|(t, l)| (t.to_string(), l.parse().unwrap())).collect();
        let base = replay.run(60.0);
        let at60: BTreeMap<usize, &BTreeMap<String, glam::Vec3>> = base.frames.iter().zip(&base.tips).map(|(f, t)| (f.round() as usize, t)).collect();
        for rate in [72.0, 90.0, 120.0] {
            let run = replay.run(rate);
            let mut sums = BTreeMap::<&str, (f32, usize)>::new();
            for (f, tips) in run.frames.iter().zip(&run.tips) {
                if (f - f.round()).abs() > 1e-6 || !window.contains(&(f.round() as usize)) {
                    continue;
                }
                for (tip, p) in tips {
                    let Some(length) = lengths.get(tip) else { continue };
                    let d = p.distance(at60[&(f.round() as usize)][tip]) / length;
                    let s = sums.entry(tip).or_default();
                    s.0 += d * d;
                    s.1 += 1;
                }
            }
            assert_eq!(sums.len(), lengths.len(), "{recording} at {rate} Hz compared every tip");
            assert!(sums.values().all(|(s, _)| s.is_finite()));
            let worst = sums.iter().map(|(tip, (s, n))| ((s / *n as f32).sqrt(), *tip)).fold((0.0f32, ""), |a, b| if b.0 > a.0 { b } else { a });
            eprintln!("{recording} at {rate} Hz: largest RMS difference from 60 Hz {:.5} of chain length ({})", worst.0, worst.1);
            assert!(worst.0 <= bound, "{recording} at {rate} Hz: {} differs from 60 Hz by {} of its length", worst.1, worst.0);
        }
    }
}

/// The filters decide by who touches: a grab or pose refused to that toucher leaves the chain free.
#[test]
fn filters_decide_by_toucher() {
    let mut avatar = avatar_core::read(&std::fs::read(path("fixtures/unity-synthetic.glb")).unwrap()).unwrap();
    let tail = avatar.model.physbones.iter().position(|p| p.parameter.as_deref() == Some("Tail")).unwrap();
    avatar.model.physbones[tail].grabbing.allow_others = false;
    avatar.model.physbones[tail].posing.allow_self = false;
    let start = reference::step(&path("fixtures/reference/fixture-grabs"), "grab Tail") - 103;
    let replay = Replay::load(&path("fixtures/reference/fixture-grabs"), avatar, start..start + 10);
    let mut pb = avatar_core::physbones::PhysBones::new(&replay.avatar).unwrap();
    pb.advance(0.0, &replay.frames[0]);
    let node = pb.bones(tail).last().unwrap();
    let at = glam::Vec3::new(0.0, 0.6, -0.3);
    assert!(!pb.grab(tail, node, at, avatar_core::physbones::Toucher::Other));
    assert!(!pb.parameters(tail).is_grabbed);
    assert!(pb.grab(tail, node, at, avatar_core::physbones::Toucher::Wearer));
    assert!(pb.parameters(tail).is_grabbed);
    pb.release(tail, true, avatar_core::physbones::Toucher::Wearer);
    let p = pb.parameters(tail);
    assert!(!p.is_grabbed && !p.is_posed, "posing is not the wearer's");
}

/// Hands push a chain only when its collision filter admits their owner, whatever the chain's own colliders: the
/// wearer's own hand collider, its node moved beside the tail's tip, and another player's beside it, the avatar moved
/// and turned in the world.
#[test]
fn hands_push_only_chains_their_filter_admits() {
    use avatar_core::model::Permission;
    use avatar_core::physbones::{HandCollider, PhysBones, Toucher, Transform};
    let dir = path("fixtures/reference/fixture-grabs");
    let start = reference::step(&dir, "grab Tail") - 103;
    let mut base = avatar_core::read(&std::fs::read(path("fixtures/unity-synthetic.glb")).unwrap()).unwrap();
    let tail = base.model.physbones.iter().position(|p| p.parameter.as_deref() == Some("Tail")).unwrap();
    base.model.physbones[tail].radius.value = 0.03;
    base.model.physbones[tail].radius.curve = None;
    let hand = base.model.descriptor.colliders.hand_left.bone.unwrap().0 as usize;
    let run = |collision: Permission, toucher: Option<Toucher>| -> Vec<Vec<Transform>> {
        let mut avatar = base.clone();
        avatar.model.physbones[tail].collision = collision;
        let mut replay = Replay::load(&dir, avatar, start..start + 40);
        let placed = Transform { translation: glam::Vec3::new(1.0, 0.0, 2.0), rotation: glam::Quat::from_rotation_y(1.2), scale: glam::Vec3::ONE };
        for frame in &mut replay.frames {
            frame.root = placed.mul(&frame.root);
        }
        let mut pb = PhysBones::new(&replay.avatar).unwrap();
        let first = &replay.frames[0];
        let tip = pb.bones(tail).last().unwrap();
        let rigid = Transform { scale: glam::Vec3::ONE, ..first.root };
        let beside = rigid.mul(&first.nodes[tip]).translation + glam::Vec3::new(0.02, 0.0, 0.0);
        let own = pb.hands(0, &first.root, &first.nodes)[0];
        let shift = rigid.rotation.inverse() * (beside - own.at.point(own.position));
        pb.advance(0.0, first);
        let mut poses = vec![];
        for frame in &replay.frames[1..] {
            let mut frame = frame.clone();
            match toucher {
                Some(Toucher::Wearer) => frame.nodes[hand].translation += shift,
                Some(Toucher::Other) => frame.hands = vec![HandCollider {
                    player: 1,
                    part: 0,
                    at: Transform { translation: beside, ..Transform::IDENTITY },
                    radius: 0.05,
                    height: 0.2,
                    position: glam::Vec3::ZERO,
                    rotation: glam::Quat::IDENTITY,
                }],
                None => {}
            }
            pb.advance(1.0 / 60.0, &frame);
            poses.push(pb.local(tail));
        }
        poses
    };
    let moved = |a: &[Vec<Transform>], b: &[Vec<Transform>]| {
        a.iter().flatten().zip(b.iter().flatten()).map(|(x, y)| x.rotation.angle_between(y.rotation)).fold(0.0f32, f32::max)
    };
    for (allow_self, allow_others) in [(false, false), (true, false), (false, true), (true, true)] {
        let collision = Permission { allow_self, allow_others };
        let free = run(collision, None);
        for (owner, allowed) in [(Toucher::Wearer, allow_self), (Toucher::Other, allow_others)] {
            let touched = run(collision, Some(owner));
            let turn = moved(&free, &touched);
            eprintln!("self {allow_self} others {allow_others}, {owner:?}'s hand: largest turn {:.2} degrees", turn.to_degrees());
            if allowed {
                assert!(turn.to_degrees() > 1.0, "a {owner:?} hand the filter admits pushed the tail {} degrees", turn.to_degrees());
            } else {
                assert!(free == touched, "a {owner:?} hand the filter refuses moved the tail");
            }
        }
    }
}

/// The recordings' stand-ins for the wearer's hand and finger colliders sit where the core puts them: the probe's and
/// the fixture's custom hand and index finger (the fixture's automatic fingers have no finger bones to stand on).
#[test]
fn wearer_hand_stand_ins_sit_where_the_core_puts_hands() {
    for (dir, model, count) in [("fixtures/reference/hands", "fixtures/unity-hands.glb", 2), ("fixtures/reference/fixture-animator", "fixtures/unity-synthetic.glb", 2)] {
        let avatar = avatar_core::read(&std::fs::read(path(model)).unwrap()).unwrap();
        let (stood, off) = reference::wearer_hand_mismatches(&path(dir), &avatar);
        assert!(off.is_empty(), "{dir}: {off:#?}");
        assert_eq!(stood, count, "{dir}");
    }
}

/// The hand collider probe: the wearer's hand, another player's and both sweep through one chain per collision filter
/// setting. Over every frame each tip's RMS error is within 5% of its chain's length of the recording, M5's bar, and
/// no frame's error passes 7% (measured: 6.6%, on three-frame runs where a chain swings back over the hand); each
/// sweep moves exactly the chains whose filter admits its hands, in the recording and in the core.
#[test]
fn hands_push_each_chain_their_filter_admits() {
    let dir = path("fixtures/reference/hands");
    let replay = replay("fixtures/reference/hands", "fixtures/unity-hands.glb", 0..usize::MAX);
    assert_eq!(replay.hands.len(), 1, "the other player's hand");
    let trace = replay.run(60.0);
    let lengths: BTreeMap<String, f32> = reference::rows(&dir.join("chains.tsv"))[1..].iter().map(|r| (r[0].clone(), r[1].parse().unwrap())).collect();
    let mut recorded = BTreeMap::<String, BTreeMap<usize, glam::Vec3>>::new();
    for r in reference::rows(&dir.join("tips.tsv")) {
        recorded.entry(r[1].clone()).or_default().insert(r[0].parse().unwrap(), glam::Vec3::new(r[2].parse().unwrap(), r[3].parse().unwrap(), r[4].parse().unwrap()));
    }
    let ours: BTreeMap<usize, &BTreeMap<String, glam::Vec3>> = trace.frames.iter().zip(&trace.tips).map(|(f, t)| (f.round() as usize, t)).collect();
    let mut failed = vec![];
    for (tip, length) in lengths.iter().filter(|(t, _)| t.starts_with("HandRow/")) {
        let errors: Vec<(f32, usize)> = recorded[tip].iter().map(|(f, p)| (ours[f][tip].distance(*p) / length, *f)).collect();
        assert_eq!(errors.len(), ours.len(), "{tip}: every frame compared");
        let rms = (errors.iter().map(|(e, _)| e * e).sum::<f32>() / errors.len() as f32).sqrt();
        let (worst, at) = errors.iter().copied().fold((0.0f32, 0), |a, b| if b.0 > a.0 { b } else { a });
        eprintln!("{tip:<60} RMS {rms:.4}, worst {worst:.4} at frame {at}, of its length");
        if rms > 0.05 || worst > 0.07 {
            failed.push(format!("{tip}: RMS {rms:.4}, worst {worst:.4} at frame {at}"));
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
    for (step, wearer, other) in [("self sweep", true, false), ("other sweep", false, true), ("both sweep", true, true)] {
        let (start, end) = (reference::step(&dir, step), reference::step(&dir, &step.replace("sweep", "away")));
        for (chain, allow_self, allow_others) in [
            ("CollideAll", true, true),
            ("CollideNone", false, false),
            ("SelfOnly", true, false),
            ("OthersOnly", false, true),
            ("SelfAndOthers", true, true),
            ("Nobody", false, false),
        ] {
            let pushed = (wearer && allow_self) || (other && allow_others);
            for (who, tips) in [("the recording", None), ("the core", Some(&ours))] {
                let moved = lengths
                    .keys()
                    .filter(|t| t.starts_with(&format!("HandRow/{chain}|")))
                    .map(|t| {
                        let at = |f: usize| tips.map_or(recorded[t][&f], |o| o[&f][t]);
                        (start..end).map(|f| at(f).distance(at(start))).fold(0.0f32, f32::max)
                    })
                    .fold(0.0f32, f32::max);
                eprintln!("{step}: {chain} moved {moved:.4} m in {who}");
                assert_eq!(moved > 0.005, pushed, "{step}: {chain} moved {moved} m in {who}");
            }
        }
    }
}

/// The core's reference run, fed only the recording's script and its body pose, meets the 5% bar on the fixture's
/// recording, and its tips start the motion where the recording's do: each within 5% of its chain's length, the
/// median within 1%. On the recording's first frame, the one the SDK's PhysBones start on, the same, and every tip
/// within 1%.
#[test]
fn the_scripted_run_meets_the_bar_on_the_fixture() {
    let dir = path("fixtures/reference/fixture-animator");
    let avatar = avatar_core::read(&std::fs::read(path("fixtures/unity-synthetic.glb")).unwrap()).unwrap();
    let window = reference::window(&dir);
    let run = reference::run::run(&avatar, &dir, Some(window.end));
    let mut tips = BTreeMap::<usize, BTreeMap<String, glam::Vec3>>::new();
    for line in run.tips.lines() {
        let r: Vec<&str> = line.split('\t').collect();
        tips.entry(r[0].parse().unwrap()).or_default().insert(r[1].to_string(), glam::Vec3::new(r[2].parse().unwrap(), r[3].parse().unwrap(), r[4].parse().unwrap()));
    }
    let trace = reference::Trace { frames: tips.keys().map(|&f| f as f64).collect(), parameters: vec![BTreeMap::new(); tips.len()], tips: tips.into_values().collect() };
    let motion = errors(&dir, &trace, window.clone());
    for (e, tip) in &motion {
        eprintln!("{e:.4} {tip}");
    }
    assert_eq!(motion.len(), reference::rows(&dir.join("chains.tsv")).len() - 1);
    assert!(median(&motion) <= 0.05, "median {}", median(&motion));
    let first = *trace.frames.first().unwrap() as usize;
    assert_eq!(first, 0, "the recording starts on the frame the emulator attaches");
    for (at, label) in [(window.start, "the motion's start"), (first, "the first frame")] {
        let start = errors(&dir, &trace, at..at + 1);
        for (e, tip) in &start {
            eprintln!("{label} {e:.4} {tip}");
        }
        assert!(median(&start) <= 0.01, "median at {label} {}", median(&start));
        let far: Vec<_> = start.iter().filter(|(e, _)| *e > 0.05).collect();
        assert!(far.is_empty(), "tips over 5% of their chain length from the recording's at {label}: {far:?}");
    }
    let held = errors(&dir, &trace, first..first + 1);
    assert!(held.len() >= 10 && held.iter().all(|(_, tip)| trace.tips[0].contains_key(tip)), "{} tips held to 1%, each from the run", held.len());
    let off: Vec<_> = held.iter().filter(|(e, _)| *e > 0.01).collect();
    assert!(off.is_empty(), "tips over 1% of their chain length from the recording's at the first frame: {off:?}");
}

/// Two components on one root: VRChat poses the bones by the first alone, as the second, which would hold them still,
/// does not run, nor report its parameters over the first's, nor take a grab.
#[test]
fn a_second_component_on_a_root_poses_nothing() {
    let dir = path("fixtures/reference/fixture-animator");
    let avatar = avatar_core::read(&std::fs::read(path("fixtures/unity-synthetic.glb")).unwrap()).unwrap();
    let tail = avatar.model.physbones.iter().position(|p| p.parameter.as_deref() == Some("Tail")).unwrap();
    let mut doubled = avatar.clone();
    let mut still = doubled.model.physbones[tail].clone();
    still.gravity.value = 0.0;
    still.pull.value = 1.0;
    still.limit = avatar_core::model::Limit::None {};
    still.endpoint_position = [0.0, 0.1, 0.0];
    doubled.model.physbones.push(still);
    let frames = Some(reference::window(&dir).end);
    let (one, two) = (reference::run::run(&avatar, &dir, frames), reference::run::run(&doubled, &dir, frames));
    let root = format!("{}|", reference::paths(&avatar, &avatar_core::physbones::Nodes::read(&avatar).unwrap())[avatar.model.physbones[tail].root.0 as usize].as_deref().unwrap());
    let of = |log: &str, key: &str| log.lines().filter(|l| l.contains(key)).map(str::to_string).collect::<Vec<_>>();
    assert!(of(&one.tips, &root).len() > 100);
    assert_eq!(of(&one.tips, &root), of(&two.tips, &root));
    assert!(of(&one.parameters, "Tail_Angle").len() > 10);
    assert_eq!(of(&one.parameters, "Tail_"), of(&two.parameters, "Tail_"));
    let mut pb = avatar_core::physbones::PhysBones::new(&doubled).unwrap();
    let shadowed = doubled.model.physbones.len() - 1;
    let node = pb.bones(tail).last().unwrap();
    assert!(!pb.grab(shadowed, node, glam::Vec3::ZERO, avatar_core::physbones::Toucher::Wearer));
}
