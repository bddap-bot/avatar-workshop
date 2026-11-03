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
    ("Momentum20Stiff50", "advanced integration: stiffness", 0.21),
    ("Momentum80Stiff20", "advanced integration: momentum and stiffness", 0.04),
    ("Momentum80Stiff80", "advanced integration: momentum and stiffness", 0.11),
    ("Advanced10", "advanced integration, version 1.0", 0.025),
    ("Gravity30", "gravity, version 1.1", 0.01),
    ("Gravity100", "gravity, version 1.1", 0.01),
    ("Gravity50Down", "gravity along the rest direction", 0.01),
    ("Gravity50Falloff50", "gravity falloff", 0.18),
    ("Gravity50Falloff100", "gravity falloff", 0.045),
    ("Gravity50Version10", "gravity, version 1.0", 0.25),
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
    ("Endpoint", "endpoint position", 0.01),
    ("Sphere", "sphere collider, bone radius 0", 0.01),
    ("SphereInside", "inside-bounds sphere collider", 0.01),
    ("Capsule", "rotated capsule collider, bone radius 0", 0.01),
    ("CapsuleRadius", "capsule collider, bone radius", 0.04),
    ("CapsuleBonesAsSpheres", "capsule collider treating bones as spheres", 0.025),
    ("Plane", "rotated plane collider", 0.27),
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
    let replay = replay("fixtures/reference/fixture-grabs", "fixtures/unity-synthetic.glb", 3960..4408);
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
    let ear = "Armature/Hips/Spine/Chest/Neck/Head/EarL.0|Armature/Hips/Spine/Chest/Neck/Head/EarL.0/EarL.1/EarL.2/EarL.3";
    let (ours, theirs) = (tips(ear, 4318..4408), recorded_tips(ear, 4318..4408));
    let rms = (ours.iter().zip(&theirs).map(|(a, b)| a.distance_squared(*b)).sum::<f32>() / ours.len() as f32).sqrt();
    assert!(rms / 0.1746 <= 0.2, "held ear tip RMS error {rms} m");
    let tail = "Armature/Hips/Tail.0|Armature/Hips/Tail.0/Tail.1/Tail.2/Tail.3/Tail.4";
    let moved = |p: Vec<glam::Vec3>| p[p.len() - 1] - p[0];
    let (ours, theirs) = (moved(tips(tail, 4063..4110)), moved(recorded_tips(tail, 4063..4110)));
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

/// Runs at 72, 90 and 120 Hz agree with the 60 Hz run, at the times both hold a frame, within 1% of chain length.
#[test]
fn host_rates_agree() {
    for (recording, model) in [("fixtures/reference/fixture", "fixtures/unity-synthetic.glb"), ("fixtures/reference/probe", "fixtures/unity-probe.glb")] {
        let dir = path(recording);
        let window = reference::window(&dir);
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
            assert!(worst.0 <= 0.01, "{recording} at {rate} Hz: {} differs from 60 Hz by {} of its length", worst.1, worst.0);
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
    let replay = Replay::load(&path("fixtures/reference/fixture-grabs"), avatar, 3960..3970);
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
