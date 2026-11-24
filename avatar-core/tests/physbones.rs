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
/// damped turns a small difference into a different swing, as the plane probe's bouncing does.
const PROBES: &[(&str, &str, f32)] = &[
    ("Base", "pull 0.2 and spring 0.2, simplified", 0.02),
    ("Pull05", "pull", 0.025),
    ("Pull50", "pull", 0.01),
    ("Pull90", "pull", 0.015),
    ("PullCurve", "a curve along the chain", 0.01),
    ("Spring0", "spring", 0.01),
    ("Spring60", "spring", 0.015),
    ("Spring95", "spring", 0.09),
    ("Version10", "version 1.0", 0.01),
    ("Momentum20Stiff0", "advanced integration: momentum", 0.005),
    ("Momentum20Stiff50", "advanced integration: stiffness", 0.105),
    ("Momentum80Stiff20", "advanced integration: momentum and stiffness", 0.035),
    ("Momentum80Stiff80", "advanced integration: momentum and stiffness", 0.085),
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
    let errors = errors(&dir, &replay.run(60.0), reference::window(&dir)).into_iter().map(|(e, tip)| (e, chain(&tip).to_string()));
    check_bounds(PROBES.iter().map(|&(probe, field, bound)| (probe, bound, field)), errors);
}

/// Each chain of the probe's jump rows (`Fixture.JumpChains`) and the largest tip RMS error over chain length any of
/// its bones may show from the frame before its parent's jump through the 150 after, in either row (each measured, with
/// a margin). One row jumps on the step after the first recorded frame, the other once its chains have settled; a chain
/// without gravity misses alike in both. A bound above `GAP` is a gap in the core, not a tolerance.
const JUMPS: &[(&str, f32)] = &[
    ("Advanced", 0.005),
    ("AdvancedLevel", 0.005),
    ("Angle45", 0.05),
    ("Branches", 0.02),
    ("BranchesHair30", 0.04),
    ("FiveBones", 0.015),
    ("Gravity100", 0.08),
    ("Gravity100Falloff100", 0.03),
    ("HairSpring30", 0.025),
    ("HairSpring70", 0.065),
    ("HairSpring70Curves", 0.075),
    ("HairSpring70Gravity", 0.08),
    ("Immobile30", 0.005),
    ("OneBone", 0.005),
    ("OneBoneSpring100", 0.02),
    ("OneBoneSpring100Polar", 0.05),
    ("OneBoneSpring85", 0.015),
    ("OneBoneSpring90", 0.015),
    ("OneBoneSpring95", 0.02),
    ("OneBoneSpring98", 0.02),
    ("Polar60x20", 0.005),
    ("ShortBones", 0.01),
    ("ShortBonesHair30", 0.105),
    ("Simplified", 0.005),
    ("SimplifiedWorld", 0.065),
    ("Spring100", 0.025),
    ("Spring70", 0.005),
    ("Spring70Stiff15", 0.01),
    ("Spring90", 0.01),
    ("Spring95", 0.015),
    ("Stretch50Motion10", 0.005),
    ("Tail", 0.045),
    ("TailEnd", 0.175),
];

/// The chain a tip of `errors` belongs to.
fn chain(tip: &str) -> &str {
    tip.split('|').next().unwrap().rsplit('/').next().unwrap()
}

/// A bound above this records a gap in the core, not a tolerance.
const GAP: f32 = 0.05;

/// The tips under the parent of `row`, which jumps on `jump`: each one's RMS error over chain length from the frame before
/// the jump through the 150 after, with its chain's name.
fn jump_row(dir: &Path, trace: &reference::Trace, row: &str, jump: usize) -> Vec<(f32, String)> {
    let on_row = |tip: &str| tip.split('/').next() == Some(row);
    let errors: Vec<(f32, String)> = errors(dir, trace, jump - 1..jump + 150).into_iter().filter(|(_, t)| on_row(t)).collect();
    let tips = reference::rows(&dir.join("chains.tsv"));
    assert_eq!(errors.len(), tips.iter().filter(|r| on_row(&r[0])).count(), "{row}: a recorded tip is missing from the window");
    errors.into_iter().map(|(e, tip)| (e, chain(&tip).to_string())).collect()
}

/// Each named chain's worst error against its bound (and a note printed beside it): every chain in `errors` has a bound,
/// and every bound a chain.
fn check_bounds<'a>(bounds: impl IntoIterator<Item = (&'a str, f32, &'a str)>, errors: impl IntoIterator<Item = (f32, String)>) {
    let bounds: Vec<_> = bounds.into_iter().collect();
    let mut worst = BTreeMap::<String, f32>::new();
    for (e, name) in errors {
        assert!(e.is_finite(), "{name}: {e}");
        assert!(bounds.iter().any(|b| b.0 == name), "{name} has no bound");
        let w = worst.entry(name).or_default();
        *w = w.max(e);
    }
    let mut failed = vec![];
    for (name, bound, note) in bounds {
        let e = *worst.get(name).unwrap_or_else(|| panic!("the recording has no {name} tips"));
        eprintln!("{name:<26} {e:.4} (bound {bound}) {note}");
        if e > bound {
            failed.push(format!("{name} {note}: {e:.4} over {bound}"));
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

#[test]
fn chains_under_a_jumping_parent() {
    let dir = path("fixtures/reference/probe");
    let replay = replay("fixtures/reference/probe", "fixtures/unity-probe.glb", 0..450);
    let trace = replay.run(60.0);
    let errors = [("Jump", replay.first + 1), ("JumpLate", replay.first + 300)].into_iter().flat_map(|(row, jump)| {
        jump_row(&dir, &trace, row, jump)
            .into_iter()
            .map(move |(e, chain)| (e, chain.strip_prefix(row).unwrap_or_else(|| panic!("{chain} is not on its row")).to_string()))
    });
    check_bounds(JUMPS.iter().map(|&(name, bound)| (name, bound, "")), errors);
}

/// The rows of `Fixture.JumpCases`, each jumping its parent by the size its name gives in cm, along the bones, against
/// them or across them, once its chains have settled. The first nine hold pull 0.1 and spring 0.3; the rest hold the pull
/// and spring their names give in hundredths.
const CASE_ROWS: &[&str] = &[
    "JumpAlong3", "JumpAlong8", "JumpAlong20", "JumpAgainst3", "JumpAgainst8", "JumpAgainst20", "JumpAcross3", "JumpAcross8", "JumpAcross20",
    "JumpAlong8Pull0Spring0", "JumpAlong8Pull0Spring90", "JumpAlong8Pull30Spring0", "JumpAlong8Pull30Spring90",
    "JumpAlong20Pull0Spring0", "JumpAlong20Pull0Spring90", "JumpAlong20Pull30Spring0", "JumpAlong20Pull30Spring90",
    "JumpAgainst8Pull0Spring0", "JumpAgainst8Pull0Spring90", "JumpAgainst8Pull30Spring0", "JumpAgainst8Pull30Spring90",
    "JumpAgainst20Pull0Spring0", "JumpAgainst20Pull0Spring90", "JumpAgainst20Pull30Spring0", "JumpAgainst20Pull30Spring90",
    "JumpAcross8Pull0Spring0", "JumpAcross8Pull0Spring90", "JumpAcross8Pull30Spring0", "JumpAcross8Pull30Spring90",
    "JumpAcross20Pull0Spring0", "JumpAcross20Pull0Spring90", "JumpAcross20Pull30Spring0", "JumpAcross20Pull30Spring90",
];

/// Each chain of `CASE_ROWS` by row and name, and the largest tip RMS error over chain length any of its bones may show
/// from the frame before its parent's jump through the 150 after (each measured, with a margin). A bound above `GAP` is a
/// gap in the core, not a tolerance.
const CASES: &[(&str, f32)] = &[
    ("JumpAlong3OneBone2", 0.01),
    ("JumpAlong3OneBone2Angle", 0.01),
    ("JumpAlong3Endpoint2", 0.02),
    ("JumpAlong3Endpoint2Angle", 0.01),
    ("JumpAlong3Branches2", 0.01),
    ("JumpAlong3Branches2Angle", 0.005),
    ("JumpAlong3OneBone4", 0.005),
    ("JumpAlong3OneBone4Angle", 0.005),
    ("JumpAlong3Endpoint4", 0.005),
    ("JumpAlong3Endpoint4Angle", 0.005),
    ("JumpAlong3Branches4", 0.005),
    ("JumpAlong3Branches4Angle", 0.005),
    ("JumpAlong3OneBone6", 0.005),
    ("JumpAlong3OneBone6Angle", 0.005),
    ("JumpAlong3Endpoint6", 0.005),
    ("JumpAlong3Endpoint6Angle", 0.005),
    ("JumpAlong3Branches6", 0.005),
    ("JumpAlong3Branches6Angle", 0.005),
    ("JumpAlong3OneBone10", 0.005),
    ("JumpAlong3OneBone10Angle", 0.005),
    ("JumpAlong3Endpoint10", 0.005),
    ("JumpAlong3Endpoint10Angle", 0.005),
    ("JumpAlong3Branches10", 0.005),
    ("JumpAlong3Branches10Angle", 0.005),
    ("JumpAlong3OneBone20", 0.005),
    ("JumpAlong3OneBone20Angle", 0.005),
    ("JumpAlong3Endpoint20", 0.005),
    ("JumpAlong3Endpoint20Angle", 0.005),
    ("JumpAlong3Branches20", 0.005),
    ("JumpAlong3Branches20Angle", 0.005),
    ("JumpAlong8OneBone2", 0.015),
    ("JumpAlong8OneBone2Angle", 0.01),
    ("JumpAlong8Endpoint2", 0.015),
    ("JumpAlong8Endpoint2Angle", 0.01),
    ("JumpAlong8Branches2", 0.01),
    ("JumpAlong8Branches2Angle", 0.01),
    ("JumpAlong8OneBone4", 0.015),
    ("JumpAlong8OneBone4Angle", 0.01),
    ("JumpAlong8Endpoint4", 0.02),
    ("JumpAlong8Endpoint4Angle", 0.01),
    ("JumpAlong8Branches4", 0.01),
    ("JumpAlong8Branches4Angle", 0.005),
    ("JumpAlong8OneBone6", 0.01),
    ("JumpAlong8OneBone6Angle", 0.01),
    ("JumpAlong8Endpoint6", 0.015),
    ("JumpAlong8Endpoint6Angle", 0.01),
    ("JumpAlong8Branches6", 0.005),
    ("JumpAlong8Branches6Angle", 0.005),
    ("JumpAlong8OneBone10", 0.005),
    ("JumpAlong8OneBone10Angle", 0.005),
    ("JumpAlong8Endpoint10", 0.005),
    ("JumpAlong8Endpoint10Angle", 0.005),
    ("JumpAlong8Branches10", 0.005),
    ("JumpAlong8Branches10Angle", 0.005),
    ("JumpAlong8OneBone20", 0.005),
    ("JumpAlong8OneBone20Angle", 0.005),
    ("JumpAlong8Endpoint20", 0.005),
    ("JumpAlong8Endpoint20Angle", 0.005),
    ("JumpAlong8Branches20", 0.005),
    ("JumpAlong8Branches20Angle", 0.005),
    ("JumpAlong20OneBone2", 0.015),
    ("JumpAlong20OneBone2Angle", 0.015),
    ("JumpAlong20Endpoint2", 0.015),
    ("JumpAlong20Endpoint2Angle", 0.015),
    ("JumpAlong20Branches2", 0.01),
    ("JumpAlong20Branches2Angle", 0.01),
    ("JumpAlong20OneBone4", 0.02),
    ("JumpAlong20OneBone4Angle", 0.01),
    ("JumpAlong20Endpoint4", 0.015),
    ("JumpAlong20Endpoint4Angle", 0.01),
    ("JumpAlong20Branches4", 0.01),
    ("JumpAlong20Branches4Angle", 0.01),
    ("JumpAlong20OneBone6", 0.015),
    ("JumpAlong20OneBone6Angle", 0.01),
    ("JumpAlong20Endpoint6", 0.015),
    ("JumpAlong20Endpoint6Angle", 0.01),
    ("JumpAlong20Branches6", 0.01),
    ("JumpAlong20Branches6Angle", 0.01),
    ("JumpAlong20OneBone10", 0.015),
    ("JumpAlong20OneBone10Angle", 0.01),
    ("JumpAlong20Endpoint10", 0.02),
    ("JumpAlong20Endpoint10Angle", 0.01),
    ("JumpAlong20Branches10", 0.01),
    ("JumpAlong20Branches10Angle", 0.005),
    ("JumpAlong20OneBone20", 0.01),
    ("JumpAlong20OneBone20Angle", 0.005),
    ("JumpAlong20Endpoint20", 0.005),
    ("JumpAlong20Endpoint20Angle", 0.01),
    ("JumpAlong20Branches20", 0.005),
    ("JumpAlong20Branches20Angle", 0.005),
    ("JumpAgainst3OneBone2", 0.005),
    ("JumpAgainst3OneBone2Angle", 0.005),
    ("JumpAgainst3Endpoint2", 0.005),
    ("JumpAgainst3Endpoint2Angle", 0.005),
    ("JumpAgainst3Branches2", 0.005),
    ("JumpAgainst3Branches2Angle", 0.005),
    ("JumpAgainst3OneBone4", 0.005),
    ("JumpAgainst3OneBone4Angle", 0.005),
    ("JumpAgainst3Endpoint4", 0.005),
    ("JumpAgainst3Endpoint4Angle", 0.005),
    ("JumpAgainst3Branches4", 0.005),
    ("JumpAgainst3Branches4Angle", 0.005),
    ("JumpAgainst3OneBone6", 0.005),
    ("JumpAgainst3OneBone6Angle", 0.005),
    ("JumpAgainst3Endpoint6", 0.005),
    ("JumpAgainst3Endpoint6Angle", 0.005),
    ("JumpAgainst3Branches6", 0.005),
    ("JumpAgainst3Branches6Angle", 0.005),
    ("JumpAgainst3OneBone10", 0.005),
    ("JumpAgainst3OneBone10Angle", 0.005),
    ("JumpAgainst3Endpoint10", 0.005),
    ("JumpAgainst3Endpoint10Angle", 0.005),
    ("JumpAgainst3Branches10", 0.005),
    ("JumpAgainst3Branches10Angle", 0.005),
    ("JumpAgainst3OneBone20", 0.005),
    ("JumpAgainst3OneBone20Angle", 0.005),
    ("JumpAgainst3Endpoint20", 0.005),
    ("JumpAgainst3Endpoint20Angle", 0.005),
    ("JumpAgainst3Branches20", 0.005),
    ("JumpAgainst3Branches20Angle", 0.005),
    ("JumpAgainst8OneBone2", 0.005),
    ("JumpAgainst8OneBone2Angle", 0.005),
    ("JumpAgainst8Endpoint2", 0.005),
    ("JumpAgainst8Endpoint2Angle", 0.005),
    ("JumpAgainst8Branches2", 0.005),
    ("JumpAgainst8Branches2Angle", 0.005),
    ("JumpAgainst8OneBone4", 0.005),
    ("JumpAgainst8OneBone4Angle", 0.005),
    ("JumpAgainst8Endpoint4", 0.005),
    ("JumpAgainst8Endpoint4Angle", 0.005),
    ("JumpAgainst8Branches4", 0.005),
    ("JumpAgainst8Branches4Angle", 0.005),
    ("JumpAgainst8OneBone6", 0.005),
    ("JumpAgainst8OneBone6Angle", 0.005),
    ("JumpAgainst8Endpoint6", 0.005),
    ("JumpAgainst8Endpoint6Angle", 0.005),
    ("JumpAgainst8Branches6", 0.005),
    ("JumpAgainst8Branches6Angle", 0.005),
    ("JumpAgainst8OneBone10", 0.005),
    ("JumpAgainst8OneBone10Angle", 0.005),
    ("JumpAgainst8Endpoint10", 0.005),
    ("JumpAgainst8Endpoint10Angle", 0.005),
    ("JumpAgainst8Branches10", 0.005),
    ("JumpAgainst8Branches10Angle", 0.005),
    ("JumpAgainst8OneBone20", 0.005),
    ("JumpAgainst8OneBone20Angle", 0.005),
    ("JumpAgainst8Endpoint20", 0.005),
    ("JumpAgainst8Endpoint20Angle", 0.005),
    ("JumpAgainst8Branches20", 0.005),
    ("JumpAgainst8Branches20Angle", 0.005),
    ("JumpAgainst20OneBone2", 0.015),
    ("JumpAgainst20OneBone2Angle", 0.01),
    ("JumpAgainst20Endpoint2", 0.02),
    ("JumpAgainst20Endpoint2Angle", 0.015),
    ("JumpAgainst20Branches2", 0.01),
    ("JumpAgainst20Branches2Angle", 0.005),
    ("JumpAgainst20OneBone4", 0.005),
    ("JumpAgainst20OneBone4Angle", 0.005),
    ("JumpAgainst20Endpoint4", 0.005),
    ("JumpAgainst20Endpoint4Angle", 0.005),
    ("JumpAgainst20Branches4", 0.005),
    ("JumpAgainst20Branches4Angle", 0.005),
    ("JumpAgainst20OneBone6", 0.005),
    ("JumpAgainst20OneBone6Angle", 0.005),
    ("JumpAgainst20Endpoint6", 0.005),
    ("JumpAgainst20Endpoint6Angle", 0.005),
    ("JumpAgainst20Branches6", 0.005),
    ("JumpAgainst20Branches6Angle", 0.005),
    ("JumpAgainst20OneBone10", 0.005),
    ("JumpAgainst20OneBone10Angle", 0.005),
    ("JumpAgainst20Endpoint10", 0.005),
    ("JumpAgainst20Endpoint10Angle", 0.005),
    ("JumpAgainst20Branches10", 0.005),
    ("JumpAgainst20Branches10Angle", 0.005),
    ("JumpAgainst20OneBone20", 0.005),
    ("JumpAgainst20OneBone20Angle", 0.005),
    ("JumpAgainst20Endpoint20", 0.005),
    ("JumpAgainst20Endpoint20Angle", 0.005),
    ("JumpAgainst20Branches20", 0.005),
    ("JumpAgainst20Branches20Angle", 0.005),
    ("JumpAcross3OneBone2", 0.01),
    ("JumpAcross3OneBone2Angle", 0.01),
    ("JumpAcross3Endpoint2", 0.01),
    ("JumpAcross3Endpoint2Angle", 0.01),
    ("JumpAcross3Branches2", 0.01),
    ("JumpAcross3Branches2Angle", 0.005),
    ("JumpAcross3OneBone4", 0.01),
    ("JumpAcross3OneBone4Angle", 0.01),
    ("JumpAcross3Endpoint4", 0.01),
    ("JumpAcross3Endpoint4Angle", 0.005),
    ("JumpAcross3Branches4", 0.005),
    ("JumpAcross3Branches4Angle", 0.005),
    ("JumpAcross3OneBone6", 0.01),
    ("JumpAcross3OneBone6Angle", 0.01),
    ("JumpAcross3Endpoint6", 0.005),
    ("JumpAcross3Endpoint6Angle", 0.005),
    ("JumpAcross3Branches6", 0.005),
    ("JumpAcross3Branches6Angle", 0.005),
    ("JumpAcross3OneBone10", 0.005),
    ("JumpAcross3OneBone10Angle", 0.005),
    ("JumpAcross3Endpoint10", 0.005),
    ("JumpAcross3Endpoint10Angle", 0.005),
    ("JumpAcross3Branches10", 0.005),
    ("JumpAcross3Branches10Angle", 0.005),
    ("JumpAcross3OneBone20", 0.005),
    ("JumpAcross3OneBone20Angle", 0.005),
    ("JumpAcross3Endpoint20", 0.005),
    ("JumpAcross3Endpoint20Angle", 0.005),
    ("JumpAcross3Branches20", 0.005),
    ("JumpAcross3Branches20Angle", 0.005),
    ("JumpAcross8OneBone2", 0.01),
    ("JumpAcross8OneBone2Angle", 0.01),
    ("JumpAcross8Endpoint2", 0.01),
    ("JumpAcross8Endpoint2Angle", 0.01),
    ("JumpAcross8Branches2", 0.01),
    ("JumpAcross8Branches2Angle", 0.01),
    ("JumpAcross8OneBone4", 0.01),
    ("JumpAcross8OneBone4Angle", 0.01),
    ("JumpAcross8Endpoint4", 0.01),
    ("JumpAcross8Endpoint4Angle", 0.01),
    ("JumpAcross8Branches4", 0.01),
    ("JumpAcross8Branches4Angle", 0.01),
    ("JumpAcross8OneBone6", 0.01),
    ("JumpAcross8OneBone6Angle", 0.01),
    ("JumpAcross8Endpoint6", 0.01),
    ("JumpAcross8Endpoint6Angle", 0.01),
    ("JumpAcross8Branches6", 0.01),
    ("JumpAcross8Branches6Angle", 0.005),
    ("JumpAcross8OneBone10", 0.01),
    ("JumpAcross8OneBone10Angle", 0.01),
    ("JumpAcross8Endpoint10", 0.01),
    ("JumpAcross8Endpoint10Angle", 0.005),
    ("JumpAcross8Branches10", 0.005),
    ("JumpAcross8Branches10Angle", 0.005),
    ("JumpAcross8OneBone20", 0.005),
    ("JumpAcross8OneBone20Angle", 0.005),
    ("JumpAcross8Endpoint20", 0.005),
    ("JumpAcross8Endpoint20Angle", 0.005),
    ("JumpAcross8Branches20", 0.005),
    ("JumpAcross8Branches20Angle", 0.005),
    ("JumpAcross20OneBone2", 0.01),
    ("JumpAcross20OneBone2Angle", 0.015),
    ("JumpAcross20Endpoint2", 0.01),
    ("JumpAcross20Endpoint2Angle", 0.03),
    ("JumpAcross20Branches2", 0.01),
    ("JumpAcross20Branches2Angle", 0.015),
    ("JumpAcross20OneBone4", 0.015),
    ("JumpAcross20OneBone4Angle", 0.01),
    ("JumpAcross20Endpoint4", 0.01),
    ("JumpAcross20Endpoint4Angle", 0.015),
    ("JumpAcross20Branches4", 0.01),
    ("JumpAcross20Branches4Angle", 0.01),
    ("JumpAcross20OneBone6", 0.01),
    ("JumpAcross20OneBone6Angle", 0.01),
    ("JumpAcross20Endpoint6", 0.01),
    ("JumpAcross20Endpoint6Angle", 0.01),
    ("JumpAcross20Branches6", 0.01),
    ("JumpAcross20Branches6Angle", 0.01),
    ("JumpAcross20OneBone10", 0.01),
    ("JumpAcross20OneBone10Angle", 0.01),
    ("JumpAcross20Endpoint10", 0.01),
    ("JumpAcross20Endpoint10Angle", 0.01),
    ("JumpAcross20Branches10", 0.01),
    ("JumpAcross20Branches10Angle", 0.01),
    ("JumpAcross20OneBone20", 0.01),
    ("JumpAcross20OneBone20Angle", 0.01),
    ("JumpAcross20Endpoint20", 0.01),
    ("JumpAcross20Endpoint20Angle", 0.005),
    ("JumpAcross20Branches20", 0.005),
    ("JumpAcross20Branches20Angle", 0.005),
    ("JumpAlong8Pull0Spring0OneBone2", 0.005),
    ("JumpAlong8Pull0Spring0OneBone2Angle", 0.005),
    ("JumpAlong8Pull0Spring0Endpoint2", 0.005),
    ("JumpAlong8Pull0Spring0Endpoint2Angle", 0.005),
    ("JumpAlong8Pull0Spring0OneBone4", 0.005),
    ("JumpAlong8Pull0Spring0OneBone4Angle", 0.005),
    ("JumpAlong8Pull0Spring0Endpoint4", 0.005),
    ("JumpAlong8Pull0Spring0Endpoint4Angle", 0.005),
    ("JumpAlong8Pull0Spring0OneBone6", 0.005),
    ("JumpAlong8Pull0Spring0OneBone6Angle", 0.005),
    ("JumpAlong8Pull0Spring0Endpoint6", 0.005),
    ("JumpAlong8Pull0Spring0Endpoint6Angle", 0.005),
    ("JumpAlong8Pull0Spring0OneBone10", 0.005),
    ("JumpAlong8Pull0Spring0OneBone10Angle", 0.005),
    ("JumpAlong8Pull0Spring0Endpoint10", 0.005),
    ("JumpAlong8Pull0Spring0Endpoint10Angle", 0.005),
    ("JumpAlong8Pull0Spring0OneBone20", 0.005),
    ("JumpAlong8Pull0Spring0OneBone20Angle", 0.005),
    ("JumpAlong8Pull0Spring0Endpoint20", 0.005),
    ("JumpAlong8Pull0Spring0Endpoint20Angle", 0.005),
    ("JumpAlong8Pull0Spring90OneBone2", 0.015),
    ("JumpAlong8Pull0Spring90OneBone2Angle", 0.02),
    ("JumpAlong8Pull0Spring90Endpoint2", 0.01),
    ("JumpAlong8Pull0Spring90Endpoint2Angle", 0.02),
    ("JumpAlong8Pull0Spring90OneBone4", 0.01),
    ("JumpAlong8Pull0Spring90OneBone4Angle", 0.015),
    ("JumpAlong8Pull0Spring90Endpoint4", 0.005),
    ("JumpAlong8Pull0Spring90Endpoint4Angle", 0.02),
    ("JumpAlong8Pull0Spring90OneBone6", 0.005),
    ("JumpAlong8Pull0Spring90OneBone6Angle", 0.01),
    ("JumpAlong8Pull0Spring90Endpoint6", 0.005),
    ("JumpAlong8Pull0Spring90Endpoint6Angle", 0.01),
    ("JumpAlong8Pull0Spring90OneBone10", 0.01),
    ("JumpAlong8Pull0Spring90OneBone10Angle", 0.01),
    ("JumpAlong8Pull0Spring90Endpoint10", 0.005),
    ("JumpAlong8Pull0Spring90Endpoint10Angle", 0.05),
    ("JumpAlong8Pull0Spring90OneBone20", 0.005),
    ("JumpAlong8Pull0Spring90OneBone20Angle", 0.005),
    ("JumpAlong8Pull0Spring90Endpoint20", 0.005),
    ("JumpAlong8Pull0Spring90Endpoint20Angle", 0.005),
    ("JumpAlong8Pull30Spring0OneBone2", 0.03),
    ("JumpAlong8Pull30Spring0OneBone2Angle", 0.01),
    ("JumpAlong8Pull30Spring0Endpoint2", 0.02),
    ("JumpAlong8Pull30Spring0Endpoint2Angle", 0.01),
    ("JumpAlong8Pull30Spring0OneBone4", 0.025),
    ("JumpAlong8Pull30Spring0OneBone4Angle", 0.01),
    ("JumpAlong8Pull30Spring0Endpoint4", 0.035),
    ("JumpAlong8Pull30Spring0Endpoint4Angle", 0.01),
    ("JumpAlong8Pull30Spring0OneBone6", 0.015),
    ("JumpAlong8Pull30Spring0OneBone6Angle", 0.01),
    ("JumpAlong8Pull30Spring0Endpoint6", 0.015),
    ("JumpAlong8Pull30Spring0Endpoint6Angle", 0.01),
    ("JumpAlong8Pull30Spring0OneBone10", 0.01),
    ("JumpAlong8Pull30Spring0OneBone10Angle", 0.01),
    ("JumpAlong8Pull30Spring0Endpoint10", 0.005),
    ("JumpAlong8Pull30Spring0Endpoint10Angle", 0.005),
    ("JumpAlong8Pull30Spring0OneBone20", 0.005),
    ("JumpAlong8Pull30Spring0OneBone20Angle", 0.005),
    ("JumpAlong8Pull30Spring0Endpoint20", 0.005),
    ("JumpAlong8Pull30Spring0Endpoint20Angle", 0.005),
    ("JumpAlong8Pull30Spring90OneBone2", 0.03),
    ("JumpAlong8Pull30Spring90OneBone2Angle", 0.015),
    ("JumpAlong8Pull30Spring90Endpoint2", 0.025),
    ("JumpAlong8Pull30Spring90Endpoint2Angle", 0.02),
    ("JumpAlong8Pull30Spring90OneBone4", 0.035),
    ("JumpAlong8Pull30Spring90OneBone4Angle", 0.015),
    ("JumpAlong8Pull30Spring90Endpoint4", 0.035),
    ("JumpAlong8Pull30Spring90Endpoint4Angle", 0.02),
    ("JumpAlong8Pull30Spring90OneBone6", 0.015),
    ("JumpAlong8Pull30Spring90OneBone6Angle", 0.015),
    ("JumpAlong8Pull30Spring90Endpoint6", 0.015),
    ("JumpAlong8Pull30Spring90Endpoint6Angle", 0.015),
    ("JumpAlong8Pull30Spring90OneBone10", 0.01),
    ("JumpAlong8Pull30Spring90OneBone10Angle", 0.01),
    ("JumpAlong8Pull30Spring90Endpoint10", 0.01),
    ("JumpAlong8Pull30Spring90Endpoint10Angle", 0.01),
    ("JumpAlong8Pull30Spring90OneBone20", 0.005),
    ("JumpAlong8Pull30Spring90OneBone20Angle", 0.005),
    ("JumpAlong8Pull30Spring90Endpoint20", 0.005),
    ("JumpAlong8Pull30Spring90Endpoint20Angle", 0.005),
    ("JumpAlong20Pull0Spring0OneBone2", 0.005),
    ("JumpAlong20Pull0Spring0OneBone2Angle", 0.005),
    ("JumpAlong20Pull0Spring0Endpoint2", 0.005),
    ("JumpAlong20Pull0Spring0Endpoint2Angle", 0.005),
    ("JumpAlong20Pull0Spring0OneBone4", 0.005),
    ("JumpAlong20Pull0Spring0OneBone4Angle", 0.005),
    ("JumpAlong20Pull0Spring0Endpoint4", 0.005),
    ("JumpAlong20Pull0Spring0Endpoint4Angle", 0.005),
    ("JumpAlong20Pull0Spring0OneBone6", 0.005),
    ("JumpAlong20Pull0Spring0OneBone6Angle", 0.005),
    ("JumpAlong20Pull0Spring0Endpoint6", 0.005),
    ("JumpAlong20Pull0Spring0Endpoint6Angle", 0.005),
    ("JumpAlong20Pull0Spring0OneBone10", 0.005),
    ("JumpAlong20Pull0Spring0OneBone10Angle", 0.005),
    ("JumpAlong20Pull0Spring0Endpoint10", 0.005),
    ("JumpAlong20Pull0Spring0Endpoint10Angle", 0.005),
    ("JumpAlong20Pull0Spring0OneBone20", 0.005),
    ("JumpAlong20Pull0Spring0OneBone20Angle", 0.005),
    ("JumpAlong20Pull0Spring0Endpoint20", 0.005),
    ("JumpAlong20Pull0Spring0Endpoint20Angle", 0.005),
    ("JumpAlong20Pull0Spring90OneBone2", 0.015),
    ("JumpAlong20Pull0Spring90OneBone2Angle", 0.015),
    ("JumpAlong20Pull0Spring90Endpoint2", 0.01),
    ("JumpAlong20Pull0Spring90Endpoint2Angle", 0.015),
    ("JumpAlong20Pull0Spring90OneBone4", 0.01),
    ("JumpAlong20Pull0Spring90OneBone4Angle", 0.015),
    ("JumpAlong20Pull0Spring90Endpoint4", 0.005),
    ("JumpAlong20Pull0Spring90Endpoint4Angle", 0.015),
    ("JumpAlong20Pull0Spring90OneBone6", 0.01),
    ("JumpAlong20Pull0Spring90OneBone6Angle", 0.015),
    ("JumpAlong20Pull0Spring90Endpoint6", 0.01),
    ("JumpAlong20Pull0Spring90Endpoint6Angle", 0.02),
    ("JumpAlong20Pull0Spring90OneBone10", 0.01),
    ("JumpAlong20Pull0Spring90OneBone10Angle", 0.015),
    ("JumpAlong20Pull0Spring90Endpoint10", 0.005),
    ("JumpAlong20Pull0Spring90Endpoint10Angle", 0.02),
    ("JumpAlong20Pull0Spring90OneBone20", 0.005),
    ("JumpAlong20Pull0Spring90OneBone20Angle", 0.005),
    ("JumpAlong20Pull0Spring90Endpoint20", 0.005),
    ("JumpAlong20Pull0Spring90Endpoint20Angle", 0.01),
    ("JumpAlong20Pull30Spring0OneBone2", 0.03),
    ("JumpAlong20Pull30Spring0OneBone2Angle", 0.01),
    ("JumpAlong20Pull30Spring0Endpoint2", 0.02),
    ("JumpAlong20Pull30Spring0Endpoint2Angle", 0.01),
    ("JumpAlong20Pull30Spring0OneBone4", 0.03),
    ("JumpAlong20Pull30Spring0OneBone4Angle", 0.01),
    ("JumpAlong20Pull30Spring0Endpoint4", 0.02),
    ("JumpAlong20Pull30Spring0Endpoint4Angle", 0.01),
    ("JumpAlong20Pull30Spring0OneBone6", 0.025),
    ("JumpAlong20Pull30Spring0OneBone6Angle", 0.01),
    ("JumpAlong20Pull30Spring0Endpoint6", 0.02),
    ("JumpAlong20Pull30Spring0Endpoint6Angle", 0.01),
    ("JumpAlong20Pull30Spring0OneBone10", 0.025),
    ("JumpAlong20Pull30Spring0OneBone10Angle", 0.01),
    ("JumpAlong20Pull30Spring0Endpoint10", 0.035),
    ("JumpAlong20Pull30Spring0Endpoint10Angle", 0.01),
    ("JumpAlong20Pull30Spring0OneBone20", 0.01),
    ("JumpAlong20Pull30Spring0OneBone20Angle", 0.01),
    ("JumpAlong20Pull30Spring0Endpoint20", 0.01),
    ("JumpAlong20Pull30Spring0Endpoint20Angle", 0.01),
    ("JumpAlong20Pull30Spring90OneBone2", 0.01),
    ("JumpAlong20Pull30Spring90OneBone2Angle", 0.015),
    ("JumpAlong20Pull30Spring90Endpoint2", 0.02),
    ("JumpAlong20Pull30Spring90Endpoint2Angle", 0.02),
    ("JumpAlong20Pull30Spring90OneBone4", 0.01),
    ("JumpAlong20Pull30Spring90OneBone4Angle", 0.015),
    ("JumpAlong20Pull30Spring90Endpoint4", 0.01),
    ("JumpAlong20Pull30Spring90Endpoint4Angle", 0.02),
    ("JumpAlong20Pull30Spring90OneBone6", 0.035),
    ("JumpAlong20Pull30Spring90OneBone6Angle", 0.015),
    ("JumpAlong20Pull30Spring90Endpoint6", 0.03),
    ("JumpAlong20Pull30Spring90Endpoint6Angle", 0.02),
    ("JumpAlong20Pull30Spring90OneBone10", 0.035),
    ("JumpAlong20Pull30Spring90OneBone10Angle", 0.015),
    ("JumpAlong20Pull30Spring90Endpoint10", 0.035),
    ("JumpAlong20Pull30Spring90Endpoint10Angle", 0.02),
    ("JumpAlong20Pull30Spring90OneBone20", 0.01),
    ("JumpAlong20Pull30Spring90OneBone20Angle", 0.01),
    ("JumpAlong20Pull30Spring90Endpoint20", 0.01),
    ("JumpAlong20Pull30Spring90Endpoint20Angle", 0.01),
    ("JumpAgainst8Pull0Spring0OneBone2", 0.005),
    ("JumpAgainst8Pull0Spring0OneBone2Angle", 0.005),
    ("JumpAgainst8Pull0Spring0Endpoint2", 0.01),
    ("JumpAgainst8Pull0Spring0Endpoint2Angle", 0.01),
    ("JumpAgainst8Pull0Spring0OneBone4", 0.005),
    ("JumpAgainst8Pull0Spring0OneBone4Angle", 0.005),
    ("JumpAgainst8Pull0Spring0Endpoint4", 0.005),
    ("JumpAgainst8Pull0Spring0Endpoint4Angle", 0.005),
    ("JumpAgainst8Pull0Spring0OneBone6", 0.005),
    ("JumpAgainst8Pull0Spring0OneBone6Angle", 0.005),
    ("JumpAgainst8Pull0Spring0Endpoint6", 0.005),
    ("JumpAgainst8Pull0Spring0Endpoint6Angle", 0.005),
    ("JumpAgainst8Pull0Spring0OneBone10", 0.005),
    ("JumpAgainst8Pull0Spring0OneBone10Angle", 0.005),
    ("JumpAgainst8Pull0Spring0Endpoint10", 0.005),
    ("JumpAgainst8Pull0Spring0Endpoint10Angle", 0.005),
    ("JumpAgainst8Pull0Spring0OneBone20", 0.005),
    ("JumpAgainst8Pull0Spring0OneBone20Angle", 0.005),
    ("JumpAgainst8Pull0Spring0Endpoint20", 0.005),
    ("JumpAgainst8Pull0Spring0Endpoint20Angle", 0.005),
    ("JumpAgainst8Pull0Spring90OneBone2", 0.025),
    ("JumpAgainst8Pull0Spring90OneBone2Angle", 0.01),
    ("JumpAgainst8Pull0Spring90Endpoint2", 0.01),
    ("JumpAgainst8Pull0Spring90Endpoint2Angle", 0.015),
    ("JumpAgainst8Pull0Spring90OneBone4", 0.01),
    ("JumpAgainst8Pull0Spring90OneBone4Angle", 0.01),
    ("JumpAgainst8Pull0Spring90Endpoint4", 0.005),
    ("JumpAgainst8Pull0Spring90Endpoint4Angle", 0.015),
    ("JumpAgainst8Pull0Spring90OneBone6", 0.01),
    ("JumpAgainst8Pull0Spring90OneBone6Angle", 0.01),
    ("JumpAgainst8Pull0Spring90Endpoint6", 0.01),
    ("JumpAgainst8Pull0Spring90Endpoint6Angle", 0.01),
    ("JumpAgainst8Pull0Spring90OneBone10", 0.005),
    ("JumpAgainst8Pull0Spring90OneBone10Angle", 0.005),
    ("JumpAgainst8Pull0Spring90Endpoint10", 0.005),
    ("JumpAgainst8Pull0Spring90Endpoint10Angle", 0.005),
    ("JumpAgainst8Pull0Spring90OneBone20", 0.005),
    ("JumpAgainst8Pull0Spring90OneBone20Angle", 0.005),
    ("JumpAgainst8Pull0Spring90Endpoint20", 0.005),
    ("JumpAgainst8Pull0Spring90Endpoint20Angle", 0.005),
    ("JumpAgainst8Pull30Spring0OneBone2", 0.005),
    ("JumpAgainst8Pull30Spring0OneBone2Angle", 0.005),
    ("JumpAgainst8Pull30Spring0Endpoint2", 0.005),
    ("JumpAgainst8Pull30Spring0Endpoint2Angle", 0.005),
    ("JumpAgainst8Pull30Spring0OneBone4", 0.005),
    ("JumpAgainst8Pull30Spring0OneBone4Angle", 0.005),
    ("JumpAgainst8Pull30Spring0Endpoint4", 0.005),
    ("JumpAgainst8Pull30Spring0Endpoint4Angle", 0.005),
    ("JumpAgainst8Pull30Spring0OneBone6", 0.005),
    ("JumpAgainst8Pull30Spring0OneBone6Angle", 0.005),
    ("JumpAgainst8Pull30Spring0Endpoint6", 0.005),
    ("JumpAgainst8Pull30Spring0Endpoint6Angle", 0.005),
    ("JumpAgainst8Pull30Spring0OneBone10", 0.005),
    ("JumpAgainst8Pull30Spring0OneBone10Angle", 0.005),
    ("JumpAgainst8Pull30Spring0Endpoint10", 0.005),
    ("JumpAgainst8Pull30Spring0Endpoint10Angle", 0.005),
    ("JumpAgainst8Pull30Spring0OneBone20", 0.005),
    ("JumpAgainst8Pull30Spring0OneBone20Angle", 0.005),
    ("JumpAgainst8Pull30Spring0Endpoint20", 0.005),
    ("JumpAgainst8Pull30Spring0Endpoint20Angle", 0.005),
    ("JumpAgainst8Pull30Spring90OneBone2", 0.035),
    ("JumpAgainst8Pull30Spring90OneBone2Angle", 0.015),
    ("JumpAgainst8Pull30Spring90Endpoint2", 0.035),
    ("JumpAgainst8Pull30Spring90Endpoint2Angle", 0.02),
    ("JumpAgainst8Pull30Spring90OneBone4", 0.015),
    ("JumpAgainst8Pull30Spring90OneBone4Angle", 0.015),
    ("JumpAgainst8Pull30Spring90Endpoint4", 0.015),
    ("JumpAgainst8Pull30Spring90Endpoint4Angle", 0.015),
    ("JumpAgainst8Pull30Spring90OneBone6", 0.01),
    ("JumpAgainst8Pull30Spring90OneBone6Angle", 0.01),
    ("JumpAgainst8Pull30Spring90Endpoint6", 0.01),
    ("JumpAgainst8Pull30Spring90Endpoint6Angle", 0.01),
    ("JumpAgainst8Pull30Spring90OneBone10", 0.005),
    ("JumpAgainst8Pull30Spring90OneBone10Angle", 0.005),
    ("JumpAgainst8Pull30Spring90Endpoint10", 0.01),
    ("JumpAgainst8Pull30Spring90Endpoint10Angle", 0.01),
    ("JumpAgainst8Pull30Spring90OneBone20", 0.005),
    ("JumpAgainst8Pull30Spring90OneBone20Angle", 0.005),
    ("JumpAgainst8Pull30Spring90Endpoint20", 0.005),
    ("JumpAgainst8Pull30Spring90Endpoint20Angle", 0.005),
    ("JumpAgainst20Pull0Spring0OneBone2", 0.005),
    ("JumpAgainst20Pull0Spring0OneBone2Angle", 0.005),
    ("JumpAgainst20Pull0Spring0Endpoint2", 0.005),
    ("JumpAgainst20Pull0Spring0Endpoint2Angle", 0.005),
    ("JumpAgainst20Pull0Spring0OneBone4", 0.005),
    ("JumpAgainst20Pull0Spring0OneBone4Angle", 0.005),
    ("JumpAgainst20Pull0Spring0Endpoint4", 0.005),
    ("JumpAgainst20Pull0Spring0Endpoint4Angle", 0.005),
    ("JumpAgainst20Pull0Spring0OneBone6", 0.005),
    ("JumpAgainst20Pull0Spring0OneBone6Angle", 0.005),
    ("JumpAgainst20Pull0Spring0Endpoint6", 0.005),
    ("JumpAgainst20Pull0Spring0Endpoint6Angle", 0.005),
    ("JumpAgainst20Pull0Spring0OneBone10", 0.005),
    ("JumpAgainst20Pull0Spring0OneBone10Angle", 0.005),
    ("JumpAgainst20Pull0Spring0Endpoint10", 0.005),
    ("JumpAgainst20Pull0Spring0Endpoint10Angle", 0.005),
    ("JumpAgainst20Pull0Spring0OneBone20", 0.005),
    ("JumpAgainst20Pull0Spring0OneBone20Angle", 0.005),
    ("JumpAgainst20Pull0Spring0Endpoint20", 0.005),
    ("JumpAgainst20Pull0Spring0Endpoint20Angle", 0.005),
    ("JumpAgainst20Pull0Spring90OneBone2", 0.035),
    ("JumpAgainst20Pull0Spring90OneBone2Angle", 0.02),
    ("JumpAgainst20Pull0Spring90Endpoint2", 0.01),
    ("JumpAgainst20Pull0Spring90Endpoint2Angle", 0.02),
    ("JumpAgainst20Pull0Spring90OneBone4", 0.025),
    ("JumpAgainst20Pull0Spring90OneBone4Angle", 0.015),
    ("JumpAgainst20Pull0Spring90Endpoint4", 0.015),
    ("JumpAgainst20Pull0Spring90Endpoint4Angle", 0.015),
    ("JumpAgainst20Pull0Spring90OneBone6", 0.01),
    ("JumpAgainst20Pull0Spring90OneBone6Angle", 0.01),
    ("JumpAgainst20Pull0Spring90Endpoint6", 0.01),
    ("JumpAgainst20Pull0Spring90Endpoint6Angle", 0.02),
    ("JumpAgainst20Pull0Spring90OneBone10", 0.01),
    ("JumpAgainst20Pull0Spring90OneBone10Angle", 0.01),
    ("JumpAgainst20Pull0Spring90Endpoint10", 0.005),
    ("JumpAgainst20Pull0Spring90Endpoint10Angle", 0.015),
    ("JumpAgainst20Pull0Spring90OneBone20", 0.005),
    ("JumpAgainst20Pull0Spring90OneBone20Angle", 0.005),
    ("JumpAgainst20Pull0Spring90Endpoint20", 0.005),
    ("JumpAgainst20Pull0Spring90Endpoint20Angle", 0.005),
    ("JumpAgainst20Pull30Spring0OneBone2", 0.005),
    ("JumpAgainst20Pull30Spring0OneBone2Angle", 0.005),
    ("JumpAgainst20Pull30Spring0Endpoint2", 0.005),
    ("JumpAgainst20Pull30Spring0Endpoint2Angle", 0.005),
    ("JumpAgainst20Pull30Spring0OneBone4", 0.005),
    ("JumpAgainst20Pull30Spring0OneBone4Angle", 0.005),
    ("JumpAgainst20Pull30Spring0Endpoint4", 0.005),
    ("JumpAgainst20Pull30Spring0Endpoint4Angle", 0.005),
    ("JumpAgainst20Pull30Spring0OneBone6", 0.005),
    ("JumpAgainst20Pull30Spring0OneBone6Angle", 0.005),
    ("JumpAgainst20Pull30Spring0Endpoint6", 0.005),
    ("JumpAgainst20Pull30Spring0Endpoint6Angle", 0.005),
    ("JumpAgainst20Pull30Spring0OneBone10", 0.005),
    ("JumpAgainst20Pull30Spring0OneBone10Angle", 0.005),
    ("JumpAgainst20Pull30Spring0Endpoint10", 0.005),
    ("JumpAgainst20Pull30Spring0Endpoint10Angle", 0.005),
    ("JumpAgainst20Pull30Spring0OneBone20", 0.005),
    ("JumpAgainst20Pull30Spring0OneBone20Angle", 0.005),
    ("JumpAgainst20Pull30Spring0Endpoint20", 0.005),
    ("JumpAgainst20Pull30Spring0Endpoint20Angle", 0.005),
    ("JumpAgainst20Pull30Spring90OneBone2", 0.035),
    ("JumpAgainst20Pull30Spring90OneBone2Angle", 0.02),
    ("JumpAgainst20Pull30Spring90Endpoint2", 0.03),
    ("JumpAgainst20Pull30Spring90Endpoint2Angle", 0.025),
    ("JumpAgainst20Pull30Spring90OneBone4", 0.035),
    ("JumpAgainst20Pull30Spring90OneBone4Angle", 0.015),
    ("JumpAgainst20Pull30Spring90Endpoint4", 0.03),
    ("JumpAgainst20Pull30Spring90Endpoint4Angle", 0.02),
    ("JumpAgainst20Pull30Spring90OneBone6", 0.035),
    ("JumpAgainst20Pull30Spring90OneBone6Angle", 0.015),
    ("JumpAgainst20Pull30Spring90Endpoint6", 0.03),
    ("JumpAgainst20Pull30Spring90Endpoint6Angle", 0.02),
    ("JumpAgainst20Pull30Spring90OneBone10", 0.015),
    ("JumpAgainst20Pull30Spring90OneBone10Angle", 0.015),
    ("JumpAgainst20Pull30Spring90Endpoint10", 0.015),
    ("JumpAgainst20Pull30Spring90Endpoint10Angle", 0.015),
    ("JumpAgainst20Pull30Spring90OneBone20", 0.01),
    ("JumpAgainst20Pull30Spring90OneBone20Angle", 0.01),
    ("JumpAgainst20Pull30Spring90Endpoint20", 0.01),
    ("JumpAgainst20Pull30Spring90Endpoint20Angle", 0.01),
    ("JumpAcross8Pull0Spring0OneBone2", 0.005),
    ("JumpAcross8Pull0Spring0OneBone2Angle", 0.005),
    ("JumpAcross8Pull0Spring0Endpoint2", 0.005),
    ("JumpAcross8Pull0Spring0Endpoint2Angle", 0.005),
    ("JumpAcross8Pull0Spring0OneBone4", 0.005),
    ("JumpAcross8Pull0Spring0OneBone4Angle", 0.005),
    ("JumpAcross8Pull0Spring0Endpoint4", 0.005),
    ("JumpAcross8Pull0Spring0Endpoint4Angle", 0.005),
    ("JumpAcross8Pull0Spring0OneBone6", 0.005),
    ("JumpAcross8Pull0Spring0OneBone6Angle", 0.005),
    ("JumpAcross8Pull0Spring0Endpoint6", 0.005),
    ("JumpAcross8Pull0Spring0Endpoint6Angle", 0.005),
    ("JumpAcross8Pull0Spring0OneBone10", 0.005),
    ("JumpAcross8Pull0Spring0OneBone10Angle", 0.005),
    ("JumpAcross8Pull0Spring0Endpoint10", 0.005),
    ("JumpAcross8Pull0Spring0Endpoint10Angle", 0.005),
    ("JumpAcross8Pull0Spring0OneBone20", 0.005),
    ("JumpAcross8Pull0Spring0OneBone20Angle", 0.005),
    ("JumpAcross8Pull0Spring0Endpoint20", 0.005),
    ("JumpAcross8Pull0Spring0Endpoint20Angle", 0.005),
    ("JumpAcross8Pull0Spring90OneBone2", 0.01),
    ("JumpAcross8Pull0Spring90OneBone2Angle", 0.015),
    ("JumpAcross8Pull0Spring90Endpoint2", 0.015),
    ("JumpAcross8Pull0Spring90Endpoint2Angle", 0.02),
    ("JumpAcross8Pull0Spring90OneBone4", 0.02),
    ("JumpAcross8Pull0Spring90OneBone4Angle", 0.02),
    ("JumpAcross8Pull0Spring90Endpoint4", 0.01),
    ("JumpAcross8Pull0Spring90Endpoint4Angle", 0.02),
    ("JumpAcross8Pull0Spring90OneBone6", 0.01),
    ("JumpAcross8Pull0Spring90OneBone6Angle", 0.015),
    ("JumpAcross8Pull0Spring90Endpoint6", 0.005),
    ("JumpAcross8Pull0Spring90Endpoint6Angle", 0.015),
    ("JumpAcross8Pull0Spring90OneBone10", 0.005),
    ("JumpAcross8Pull0Spring90OneBone10Angle", 0.005),
    ("JumpAcross8Pull0Spring90Endpoint10", 0.01),
    ("JumpAcross8Pull0Spring90Endpoint10Angle", 0.005),
    ("JumpAcross8Pull0Spring90OneBone20", 0.01),
    ("JumpAcross8Pull0Spring90OneBone20Angle", 0.01),
    ("JumpAcross8Pull0Spring90Endpoint20", 0.005),
    ("JumpAcross8Pull0Spring90Endpoint20Angle", 0.005),
    ("JumpAcross8Pull30Spring0OneBone2", 0.015),
    ("JumpAcross8Pull30Spring0OneBone2Angle", 0.01),
    ("JumpAcross8Pull30Spring0Endpoint2", 0.015),
    ("JumpAcross8Pull30Spring0Endpoint2Angle", 0.01),
    ("JumpAcross8Pull30Spring0OneBone4", 0.015),
    ("JumpAcross8Pull30Spring0OneBone4Angle", 0.01),
    ("JumpAcross8Pull30Spring0Endpoint4", 0.01),
    ("JumpAcross8Pull30Spring0Endpoint4Angle", 0.01),
    ("JumpAcross8Pull30Spring0OneBone6", 0.01),
    ("JumpAcross8Pull30Spring0OneBone6Angle", 0.01),
    ("JumpAcross8Pull30Spring0Endpoint6", 0.01),
    ("JumpAcross8Pull30Spring0Endpoint6Angle", 0.01),
    ("JumpAcross8Pull30Spring0OneBone10", 0.01),
    ("JumpAcross8Pull30Spring0OneBone10Angle", 0.01),
    ("JumpAcross8Pull30Spring0Endpoint10", 0.01),
    ("JumpAcross8Pull30Spring0Endpoint10Angle", 0.01),
    ("JumpAcross8Pull30Spring0OneBone20", 0.01),
    ("JumpAcross8Pull30Spring0OneBone20Angle", 0.01),
    ("JumpAcross8Pull30Spring0Endpoint20", 0.005),
    ("JumpAcross8Pull30Spring0Endpoint20Angle", 0.005),
    ("JumpAcross8Pull30Spring90OneBone2", 0.03),
    ("JumpAcross8Pull30Spring90OneBone2Angle", 0.02),
    ("JumpAcross8Pull30Spring90Endpoint2", 0.03),
    ("JumpAcross8Pull30Spring90Endpoint2Angle", 0.025),
    ("JumpAcross8Pull30Spring90OneBone4", 0.025),
    ("JumpAcross8Pull30Spring90OneBone4Angle", 0.02),
    ("JumpAcross8Pull30Spring90Endpoint4", 0.025),
    ("JumpAcross8Pull30Spring90Endpoint4Angle", 0.025),
    ("JumpAcross8Pull30Spring90OneBone6", 0.02),
    ("JumpAcross8Pull30Spring90OneBone6Angle", 0.015),
    ("JumpAcross8Pull30Spring90Endpoint6", 0.02),
    ("JumpAcross8Pull30Spring90Endpoint6Angle", 0.02),
    ("JumpAcross8Pull30Spring90OneBone10", 0.015),
    ("JumpAcross8Pull30Spring90OneBone10Angle", 0.015),
    ("JumpAcross8Pull30Spring90Endpoint10", 0.015),
    ("JumpAcross8Pull30Spring90Endpoint10Angle", 0.015),
    ("JumpAcross8Pull30Spring90OneBone20", 0.01),
    ("JumpAcross8Pull30Spring90OneBone20Angle", 0.01),
    ("JumpAcross8Pull30Spring90Endpoint20", 0.01),
    ("JumpAcross8Pull30Spring90Endpoint20Angle", 0.01),
    ("JumpAcross20Pull0Spring0OneBone2", 0.005),
    ("JumpAcross20Pull0Spring0OneBone2Angle", 0.005),
    ("JumpAcross20Pull0Spring0Endpoint2", 0.005),
    ("JumpAcross20Pull0Spring0Endpoint2Angle", 0.005),
    ("JumpAcross20Pull0Spring0OneBone4", 0.005),
    ("JumpAcross20Pull0Spring0OneBone4Angle", 0.005),
    ("JumpAcross20Pull0Spring0Endpoint4", 0.005),
    ("JumpAcross20Pull0Spring0Endpoint4Angle", 0.005),
    ("JumpAcross20Pull0Spring0OneBone6", 0.005),
    ("JumpAcross20Pull0Spring0OneBone6Angle", 0.005),
    ("JumpAcross20Pull0Spring0Endpoint6", 0.005),
    ("JumpAcross20Pull0Spring0Endpoint6Angle", 0.005),
    ("JumpAcross20Pull0Spring0OneBone10", 0.005),
    ("JumpAcross20Pull0Spring0OneBone10Angle", 0.005),
    ("JumpAcross20Pull0Spring0Endpoint10", 0.005),
    ("JumpAcross20Pull0Spring0Endpoint10Angle", 0.005),
    ("JumpAcross20Pull0Spring0OneBone20", 0.005),
    ("JumpAcross20Pull0Spring0OneBone20Angle", 0.005),
    ("JumpAcross20Pull0Spring0Endpoint20", 0.005),
    ("JumpAcross20Pull0Spring0Endpoint20Angle", 0.005),
    ("JumpAcross20Pull0Spring90OneBone2", 0.01),
    ("JumpAcross20Pull0Spring90OneBone2Angle", 0.035),
    ("JumpAcross20Pull0Spring90Endpoint2", 0.01),
    ("JumpAcross20Pull0Spring90Endpoint2Angle", 0.035),
    ("JumpAcross20Pull0Spring90OneBone4", 0.01),
    ("JumpAcross20Pull0Spring90OneBone4Angle", 0.025),
    ("JumpAcross20Pull0Spring90Endpoint4", 0.02),
    ("JumpAcross20Pull0Spring90Endpoint4Angle", 0.025),
    ("JumpAcross20Pull0Spring90OneBone6", 0.02),
    ("JumpAcross20Pull0Spring90OneBone6Angle", 0.015),
    ("JumpAcross20Pull0Spring90Endpoint6", 0.015),
    ("JumpAcross20Pull0Spring90Endpoint6Angle", 0.02),
    ("JumpAcross20Pull0Spring90OneBone10", 0.02),
    ("JumpAcross20Pull0Spring90OneBone10Angle", 0.02),
    ("JumpAcross20Pull0Spring90Endpoint10", 0.01),
    ("JumpAcross20Pull0Spring90Endpoint10Angle", 0.02),
    ("JumpAcross20Pull0Spring90OneBone20", 0.01),
    ("JumpAcross20Pull0Spring90OneBone20Angle", 0.01),
    ("JumpAcross20Pull0Spring90Endpoint20", 0.01),
    ("JumpAcross20Pull0Spring90Endpoint20Angle", 0.01),
    ("JumpAcross20Pull30Spring0OneBone2", 0.015),
    ("JumpAcross20Pull30Spring0OneBone2Angle", 0.01),
    ("JumpAcross20Pull30Spring0Endpoint2", 0.015),
    ("JumpAcross20Pull30Spring0Endpoint2Angle", 0.01),
    ("JumpAcross20Pull30Spring0OneBone4", 0.015),
    ("JumpAcross20Pull30Spring0OneBone4Angle", 0.01),
    ("JumpAcross20Pull30Spring0Endpoint4", 0.015),
    ("JumpAcross20Pull30Spring0Endpoint4Angle", 0.01),
    ("JumpAcross20Pull30Spring0OneBone6", 0.015),
    ("JumpAcross20Pull30Spring0OneBone6Angle", 0.01),
    ("JumpAcross20Pull30Spring0Endpoint6", 0.01),
    ("JumpAcross20Pull30Spring0Endpoint6Angle", 0.01),
    ("JumpAcross20Pull30Spring0OneBone10", 0.015),
    ("JumpAcross20Pull30Spring0OneBone10Angle", 0.01),
    ("JumpAcross20Pull30Spring0Endpoint10", 0.01),
    ("JumpAcross20Pull30Spring0Endpoint10Angle", 0.01),
    ("JumpAcross20Pull30Spring0OneBone20", 0.01),
    ("JumpAcross20Pull30Spring0OneBone20Angle", 0.01),
    ("JumpAcross20Pull30Spring0Endpoint20", 0.01),
    ("JumpAcross20Pull30Spring0Endpoint20Angle", 0.01),
    ("JumpAcross20Pull30Spring90OneBone2", 0.03),
    ("JumpAcross20Pull30Spring90OneBone2Angle", 0.03),
    ("JumpAcross20Pull30Spring90Endpoint2", 0.025),
    ("JumpAcross20Pull30Spring90Endpoint2Angle", 0.03),
    ("JumpAcross20Pull30Spring90OneBone4", 0.03),
    ("JumpAcross20Pull30Spring90OneBone4Angle", 0.02),
    ("JumpAcross20Pull30Spring90Endpoint4", 0.03),
    ("JumpAcross20Pull30Spring90Endpoint4Angle", 0.02),
    ("JumpAcross20Pull30Spring90OneBone6", 0.035),
    ("JumpAcross20Pull30Spring90OneBone6Angle", 0.015),
    ("JumpAcross20Pull30Spring90Endpoint6", 0.025),
    ("JumpAcross20Pull30Spring90Endpoint6Angle", 0.025),
    ("JumpAcross20Pull30Spring90OneBone10", 0.025),
    ("JumpAcross20Pull30Spring90OneBone10Angle", 0.02),
    ("JumpAcross20Pull30Spring90Endpoint10", 0.025),
    ("JumpAcross20Pull30Spring90Endpoint10Angle", 0.025),
    ("JumpAcross20Pull30Spring90OneBone20", 0.015),
    ("JumpAcross20Pull30Spring90OneBone20Angle", 0.015),
    ("JumpAcross20Pull30Spring90Endpoint20", 0.02),
    ("JumpAcross20Pull30Spring90Endpoint20Angle", 0.015),
];

#[test]
fn chains_meeting_one_jump() {
    let dir = path("fixtures/reference/probe");
    let replay = replay("fixtures/reference/probe", "fixtures/unity-probe.glb", 0..450);
    let trace = replay.run(60.0);
    let errors: Vec<(f32, String)> = CASE_ROWS.iter().flat_map(|row| jump_row(&dir, &trace, row, replay.first + 300)).collect();
    let chains = reference::text(&dir.join("chains.tsv"));
    let recorded: std::collections::BTreeSet<&str> = chains
        .lines()
        .skip(1)
        .filter_map(|l| l.split('/').next())
        .filter(|r| r.starts_with("Jump"))
        .collect();
    assert_eq!(recorded, ["Jump", "JumpLate"].iter().chain(CASE_ROWS).copied().collect(), "the recording's jump rows");
    check_bounds(CASES.iter().map(|&(name, bound)| (name, bound, "")), errors);
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

/// A case-row chain with no pull or at spring 0.9, or a jump-row chain at spring 1, keeps the difference the jump's place
/// between steps makes, so its runs at other host rates part from the 60 Hz run by up to this share of its length
/// (measured, with a margin).
const UNDAMPED_RATES: f32 = 0.265;

/// A tip on a `CASE_ROWS` row with no pull or at spring 0.9, or of a jump row's chain at spring 1.
fn undamped(tip: &str) -> bool {
    let row = tip.split('/').next().unwrap();
    match row {
        "Jump" | "JumpLate" => chain(tip).contains("Spring100"),
        _ => CASE_ROWS.contains(&row) && (row.contains("Pull0Spring") || row.ends_with("Spring90")),
    }
}

/// Runs at 72, 90 and 120 Hz agree with the 60 Hz run, at the times both hold a frame, within 1% of chain length over
/// the motion, or `UNDAMPED_RATES` on the case rows it names; over the hand probe's sweeps, whose hands move at constant
/// speeds so steps between host frames meet them exactly where they are, within 0.1%.
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
        // A jump-row chain whose bound records a gap in the core swings apart at another host rate.
        let jumping = |t: &str| {
            let (row, chain) = (t.split('/').next().unwrap(), chain(t));
            let gap = |bounds: &[(&str, f32)], name: &str| bounds.iter().any(|(n, b)| *b > GAP && *n == name);
            match row {
                "Jump" | "JumpLate" => chain.strip_prefix(row).is_some_and(|c| gap(JUMPS, c)),
                _ => CASE_ROWS.contains(&row) && gap(CASES, chain),
            }
        };
        let lengths: BTreeMap<String, f32> = reference::text(&dir.join("chains.tsv"))
            .lines()
            .skip(1)
            .map(|l| l.split_once('\t').unwrap())
            .filter(|(t, _)| !jumping(t))
            .map(|(t, l)| (t.to_string(), l.parse().unwrap()))
            .collect();
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
            for slack in [false, true] {
                let worst = sums
                    .iter()
                    .filter(|(tip, _)| undamped(tip) == slack)
                    .map(|(tip, (s, n))| ((s / *n as f32).sqrt(), *tip))
                    .fold((0.0f32, ""), |a, b| if b.0 > a.0 { b } else { a });
                if worst.1.is_empty() {
                    continue;
                }
                let bound = if slack { UNDAMPED_RATES } else { bound };
                eprintln!("{recording} at {rate} Hz: largest RMS difference from 60 Hz {:.5} of chain length, bound {bound} ({})", worst.0, worst.1);
                assert!(worst.0 <= bound, "{recording} at {rate} Hz: {} differs from 60 Hz by {} of its length", worst.1, worst.0);
            }
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

/// The recordings' stand-ins for the wearer's hand and finger colliders sit where the core puts them: the probe's
/// custom hands and fingers, and the fixture's custom hand and index finger (the fixture's automatic fingers have no
/// finger bones to stand on).
#[test]
fn wearer_hand_stand_ins_sit_where_the_core_puts_hands() {
    for (dir, model, count) in [("fixtures/reference/hands", "fixtures/unity-hands.glb", 7), ("fixtures/reference/fixture-animator", "fixtures/unity-synthetic.glb", 2)] {
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

/// Behind the hand collider probe's body, each of the wearer's right hand and finger colliders stands on a bone of its
/// own, crossed by chains hanging from that bone, its parent, a further ancestor or its child, which lead out across the
/// collider's middle or from a root within its reach, under varied settings and directions: over the rest before the
/// sweeps, the tip of each is within 5% of its chain's length of the recording's (RMS), and the core pushes the chains
/// the SDK pushes. A bone crossing the finger collider's axis at an angle is pushed only when its point nearest the
/// collider's point nearest its end is within reach: one crossing 2.9 cm above the axis, within reach of it, is not.
#[test]
fn wearer_hands_push_chains_from_whichever_bone_they_hang() {
    let dir = path("fixtures/reference/hands");
    let end = reference::step(&dir, "self sweep");
    let trace = replay("fixtures/reference/hands", "fixtures/unity-hands.glb", 0..end).run(60.0);
    let ours: BTreeMap<usize, &BTreeMap<String, glam::Vec3>> = trace.frames.iter().map(|f| f.round() as usize).zip(&trace.tips).collect();
    let lengths: BTreeMap<String, f32> = reference::rows(&dir.join("chains.tsv"))[1..].iter().map(|r| (r[0].clone(), r[1].parse().unwrap())).collect();
    let mut recorded = BTreeMap::<String, BTreeMap<usize, glam::Vec3>>::new();
    for r in reference::rows(&dir.join("tips.tsv")) {
        recorded.entry(r[1].clone()).or_default().insert(r[0].parse().unwrap(), glam::Vec3::new(r[2].parse().unwrap(), r[3].parse().unwrap(), r[4].parse().unwrap()));
    }
    let reaches: Vec<(&String, &f32)> = lengths.iter().filter(|(t, _)| t.split('/').next().unwrap().contains("Run.")).collect();
    for relation in ["FingerOwn", "FingerParent", "FingerAncestor", "FingerChild", "HandOwn"] {
        for shape in ["Across", "Inside", "Mid", "Rim"] {
            assert_eq!(reaches.iter().filter(|(t, _)| t.split('|').next().unwrap().ends_with(&format!("/{relation}{shape}"))).count(), 1, "{relation}{shape}");
        }
    }
    assert_eq!(reaches.len(), 236, "chains behind the body");
    assert_eq!(reaches.iter().filter(|(t, _)| t.split('|').next().unwrap().contains("/FingerOwnCross")).count(), 60, "chains crossing the finger collider's axis at an angle");
    let mut failed = vec![];
    for (tip, length) in reaches {
        let chain = chain(tip);
        let frames: Vec<usize> = (0..end).filter(|f| ours.contains_key(f) && recorded[tip].contains_key(f)).collect();
        assert_eq!(frames.len(), end, "{chain}: every frame of the rest compared");
        let rms = (frames.iter().map(|f| (ours[f][tip].distance(recorded[tip][f]) / length).powi(2)).sum::<f32>() / end as f32).sqrt();
        let pushed = |at: &dyn Fn(usize) -> glam::Vec3| frames.iter().map(|&f| at(f).distance(at(0)) / length).fold(0.0f32, f32::max);
        let (by_sdk, by_core) = (pushed(&|f| recorded[tip][&f]), pushed(&|f| ours[&f][tip]));
        eprintln!("{chain:<34} RMS {rms:.4}; largest distance from its first tip: recording {by_sdk:.4}, core {by_core:.4}, of its length");
        let must_push = chain.ends_with("Across") || chain.contains("CrossH5A") || chain.contains("CrossH15A");
        let must_not_push = chain.ends_with("CrossH29A45F3");
        if rms > 0.05 || (by_sdk > 0.05) != (by_core > 0.05) || (must_push && by_sdk <= 0.05) || (must_not_push && by_sdk > 0.0) {
            failed.push(format!("{chain}: RMS {rms:.4}, pushed {by_sdk:.4} in the recording, {by_core:.4} in the core"));
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}
