mod reference;

use std::path::Path;

use avatar_core::animator::Animator;
use reference::animator::{Mismatches, Run};

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
    assert_eq!(rep.bindings.wrong, 0, "bindings: {}", rep.bindings.summary(20));
}

/// The composition probe plays one composition of the human stream at a time. Nothing differs from the reference
/// while a case plays; the frames on which playables and layers change weight between cases are not yet exact.
#[test]
fn each_humanoid_composition_follows_its_reference_trace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("fixtures/reference/human-animator");
    let avatar = avatar_core::read(&std::fs::read(root.join("fixtures/unity-human.glb")).unwrap()).unwrap();
    let rep = Run::new(&dir, avatar).run(None);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert_eq!(rep.values.wrong, 0, "values: {}", rep.values.summary(20));
    assert_eq!(rep.clocks.wrong, 0, "state times: {}", rep.clocks.summary(20));
    assert!(rep.bindings.compared > 2_400_000, "{} bindings compared", rep.bindings.compared);
    let steps = reference::animator::steps(&dir);
    let cases: Vec<_> = steps.iter().filter(|s| s.label.starts_with("case ")).collect();
    assert_eq!(cases.len(), 116, "58 cases, each unmirrored and mirrored");
    for s in cases {
        let wrong: Vec<_> = rep.bindings.samples.range((s.frame + 1, String::new())..(s.frame + s.frames, String::new())).map(|w| w.0).collect();
        assert!(wrong.is_empty(), "{} differs at frames {wrong:?}", s.label);
    }
    assert!(rep.bindings.wrong <= 134, "bindings: {}", rep.bindings.summary(20));
}

/// The rotation probe turns the body and the IK goals about all three axes, one clip and root-motion setting at a
/// time, in Base and Action at full and partial weight. Every case follows the reference but a moving clip under Loop
/// Pose and the Feet height basis left unbaked.
#[test]
fn each_humanoid_rotation_follows_its_reference_trace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("fixtures/reference/rotation-animator");
    let avatar = avatar_core::read(&std::fs::read(root.join("fixtures/unity-rotation.glb")).unwrap()).unwrap();
    let rep = Run::new(&dir, avatar).run(None);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert_eq!(rep.values.wrong, 0, "values: {}", rep.values.summary(20));
    assert_eq!(rep.clocks.wrong, 0, "state times: {}", rep.clocks.summary(20));
    assert!(rep.bindings.compared > 1_900_000, "{} bindings compared", rep.bindings.compared);
    let steps = reference::animator::steps(&dir);
    let cases: Vec<_> = steps.iter().filter(|s| s.label.starts_with("case ")).collect();
    assert_eq!(cases.len(), 96, "48 cases, each unmirrored and mirrored");
    for s in cases.iter().filter(|s| !s.label.ends_with("moving xyz loop pose") && !s.label.ends_with("height feet")) {
        let wrong: Vec<_> = rep.bindings.samples.range((s.frame, String::new())..(s.frame + s.frames, String::new())).take(20).collect();
        assert!(wrong.is_empty(), "{} differs: {wrong:?}", s.label);
    }
    assert!(rep.bindings.wrong <= 8882, "bindings: {}", rep.bindings.summary(20));
}

/// The blend duration probe plays one blend tree at a time; the core keeps every state's time. The values of every
/// case but the first tree without write defaults (case 75), which starts from what the case before it left, follow
/// the reference, released zero-length children of 2D trees included.
#[test]
fn each_blend_tree_follows_its_reference_trace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("fixtures/reference/blend-animator");
    let avatar = avatar_core::read(&std::fs::read(root.join("fixtures/unity-blend.glb")).unwrap()).unwrap();
    let rep = Run::new(&dir, avatar).run(None);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert!(rep.clocks.compared > 170_000, "{} state times compared", rep.clocks.compared);
    let steps = reference::animator::steps(&dir);
    let cases: Vec<_> = steps.iter().filter(|s| s.label.starts_with("case ")).collect();
    assert_eq!(cases.len(), 78);
    assert_eq!(rep.clocks.wrong, 0, "state times: {}", rep.clocks.summary(20));
    for s in cases {
        let n: u32 = s.label["case ".len()..].split(':').next().unwrap().parse().unwrap();
        if n == 75 {
            continue;
        }
        let wrong: Vec<_> = rep.values.samples.range((s.frame, String::new())..(s.frame + s.frames, String::new())).collect();
        assert!(wrong.is_empty(), "{} differs: {wrong:?}", s.label);
    }
    assert!(rep.values.wrong <= 221, "values: {}", rep.values.summary(20));
}

/// The write-defaults layering probe plays one composition of write defaults across layers at a time. Every value
/// follows the reference but one: Unity resets the first layer's toggle a frame late when the layer beneath it sits
/// in a state with neither a motion nor write defaults.
#[test]
fn each_write_defaults_layering_follows_its_reference_trace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("fixtures/reference/layering-animator");
    let avatar = avatar_core::read(&std::fs::read(root.join("fixtures/unity-layering.glb")).unwrap()).unwrap();
    let rep = Run::new(&dir, avatar).run(None);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert!(rep.clocks.compared > 110_000, "{} state times compared", rep.clocks.compared);
    assert_eq!(rep.clocks.wrong, 0, "state times: {}", rep.clocks.summary(20));
    let steps = reference::animator::steps(&dir);
    let cases: Vec<_> = steps.iter().filter(|s| s.label.starts_with("case ")).collect();
    assert_eq!(cases.len(), 29);
    assert!(rep.values.compared > 770_000, "{} values compared", rep.values.compared);
    for s in cases {
        let wrong: Vec<_> = rep.values.samples.range((s.frame, String::new())..(s.frame + s.frames, String::new())).collect();
        let late = s.label.starts_with("case 2:") && wrong.iter().all(|w| w.0 == s.frame + 90 && w.1 == "shape:Body:Open");
        assert!(wrong.is_empty() || late, "{} differs: {wrong:?}", s.label);
    }
    assert!(rep.values.wrong <= 1, "values: {}", rep.values.summary(20));
}

/// The clip sample-time probe plays identity clips, whose value is the time Unity sampled them at, at every length,
/// speed, container and entry frame at once. State times and values are held to Unity's bits.
#[test]
fn each_clip_sample_time_follows_its_reference_trace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("fixtures/reference/sampletime-animator");
    let avatar = avatar_core::read(&std::fs::read(root.join("fixtures/unity-sampletime.glb")).unwrap()).unwrap();
    let rep = Run::new(&dir, avatar).run(None);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert!(rep.clocks.compared > 660_000, "{} state times compared", rep.clocks.compared);
    assert_eq!(rep.clocks.wrong, 0, "state times: {}", rep.clocks.summary(20));
    assert_eq!(rep.clock_bits_differ, 0, "state times not the recorded bits");
    assert!(rep.values.compared > 1_700_000, "{} values compared", rep.values.compared);
    assert_eq!(rep.values.wrong, 0, "values: {}", rep.values.summary(20));
    assert_eq!(rep.worst_value, 0.0, "values not the recorded bits");
}

/// The dense clip probe plays clips keyed on, between and off their frame grid, with high-quality curves and
/// without: a low-quality curve whose keys all lie on the grid is sampled on its frames and linearly between them.
#[test]
fn each_dense_clip_follows_its_reference_trace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("fixtures/reference/dense-animator");
    let avatar = avatar_core::read(&std::fs::read(root.join("fixtures/unity-dense.glb")).unwrap()).unwrap();
    assert!(avatar.model.clips.iter().filter(|c| !c.high_quality_curve).count() >= 32);
    let rep = Run::new(&dir, avatar).run(None);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert_eq!(rep.clocks.wrong, 0, "state times: {}", rep.clocks.summary(20));
    assert!(rep.values.compared > 4_000_000, "{} values compared", rep.values.compared);
    assert_eq!(rep.values.wrong, 0, "values: {}", rep.values.summary(20));
    assert!(rep.bindings.compared > 3_000_000, "{} bindings compared", rep.bindings.compared);
    assert_eq!(rep.bindings.wrong, 0, "bindings: {}", rep.bindings.summary(20));
}

/// The crossfade probe crossfades an additive humanoid layer between states with additive reference poses, whose
/// body rotation offsets lie in either hemisphere, over fixed and normalised durations and interrupted midway, with a
/// generic property one side writes, after walks played mirrored and unmirrored, their time driven by a parameter at
/// several values or by their own clock. Parameters, states, state times and pose bindings follow the reference. Each
/// case's wrong values are held to today's counts, zero where the core already has the rule; the core does not yet add
/// a generic property only one side of the fade writes as Unity does.
#[test]
fn each_additive_crossfade_and_scrubbed_mirror_follows_its_reference_trace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("fixtures/reference/crossfade-animator");
    let avatar = avatar_core::read(&std::fs::read(root.join("fixtures/unity-crossfade.glb")).unwrap()).unwrap();
    let rep = Run::new(&dir, avatar).run(None);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert_eq!(rep.clocks.wrong, 0, "state times: {}", rep.clocks.summary(20));
    assert!(rep.clocks.compared > 60_000, "{} state times compared", rep.clocks.compared);
    assert!(rep.values.compared > 700_000, "{} values compared", rep.values.compared);
    assert!(rep.bindings.compared > 600_000, "{} bindings compared", rep.bindings.compared);
    assert_eq!(rep.bindings.wrong, 0, "pose bindings: {}", rep.bindings.summary(20));
    // (case label, wrong value samples) at most, in case order.
    let held: [(&str, usize); 29] = [
        ("scrub mirrored loop own_clock", 0),
        ("scrub mirrored loop time_0", 0),
        ("scrub mirrored loop time_25", 0),
        ("scrub mirrored loop time_60", 0),
        ("scrub mirrored loop time_100", 0),
        ("scrub mirrored loop time_ramp", 0),
        ("scrub mirrored once own_clock", 0),
        ("scrub mirrored once time_0", 0),
        ("scrub mirrored once time_25", 0),
        ("scrub mirrored once time_60", 0),
        ("scrub mirrored once time_100", 0),
        ("scrub mirrored once time_ramp", 0),
        ("scrub unmirrored loop own_clock", 0),
        ("scrub unmirrored loop time_0", 0),
        ("scrub unmirrored loop time_25", 0),
        ("scrub unmirrored loop time_ramp", 0),
        ("crossfade same fixed", 0),
        ("crossfade other fixed", 0),
        ("crossfade negative_alike fixed", 0),
        ("crossfade negative_pair fixed", 0),
        ("crossfade same normalized", 0),
        ("crossfade other normalized", 0),
        ("crossfade negative_alike normalized", 0),
        ("crossfade negative_pair normalized", 0),
        ("crossfade same fixed interrupted", 0),
        ("crossfade other fixed interrupted", 0),
        ("crossfade other normalized interrupted", 0),
        ("generic source only", 150),
        ("generic destination only", 150),
    ];
    let steps = reference::animator::steps(&dir);
    let cases: Vec<_> = steps.iter().filter(|s| s.label.starts_with("case ")).collect();
    assert_eq!(cases.len(), held.len());
    for (s, (name, values)) in cases.iter().zip(held) {
        assert!(s.label.ends_with(&format!(" {name}")) || s.label.contains(&format!(" {name} source;")), "{} is not {name}", s.label);
        let wrong = |samples: &std::collections::BTreeSet<(usize, String)>| samples.range((s.frame, String::new())..(s.frame + s.frames, String::new())).count();
        assert!(wrong(&rep.values.samples) <= values, "{}: {} wrong values", s.label, wrong(&rep.values.samples));
    }
}

/// The curve probe samples clips through Unity's clip playable beside `AnimationCurve.Evaluate`, at identity curves of
/// the sample-time probe's lengths and at random curves of every span; both match to the bit.
#[test]
fn each_curve_sample_follows_its_reference_trace() {
    use avatar_core::model::{AnimationCurve, Keyframe, WeightedMode};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/reference/curves");
    let curves: Vec<(bool, AnimationCurve)> = reference::text(&dir.join("curves.tsv"))
        .lines()
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let keys = f[2..]
                .iter()
                .map(|k| {
                    let v: Vec<f32> = k.split(',').map(|x| x.parse().unwrap()).collect();
                    Keyframe { time: v[0], value: v[1], in_tangent: v[2], out_tangent: v[3], in_weight: 1.0 / 3.0, out_weight: 1.0 / 3.0, weighted_mode: WeightedMode::None }
                })
                .collect();
            (f[1] == "loop", AnimationCurve { keys })
        })
        .collect();
    assert_eq!(curves.len(), 88);
    let (mut compared, mut clip_wrong, mut evaluate_wrong) = (0, Vec::new(), Vec::new());
    for l in reference::text(&dir.join("samples.tsv")).lines().skip(1) {
        let f: Vec<&str> = l.split('\t').collect();
        let (c, t, clip, evaluate): (usize, f32, f32, f32) = (f[0].parse().unwrap(), f[1].parse().unwrap(), f[2].parse().unwrap(), f[3].parse().unwrap());
        let (looping, curve) = &curves[c];
        let length = curve.keys.last().unwrap().time;
        compared += 1;
        let got = curve.evaluate(avatar_core::curve::clip_time(t, length, *looping));
        if got.to_bits() != clip.to_bits() {
            clip_wrong.push((c, t, got, clip));
        }
        if curve.evaluate(t).to_bits() != evaluate.to_bits() {
            evaluate_wrong.push((c, t, curve.evaluate(t), evaluate));
        }
    }
    assert_eq!(compared, 35_200);
    assert!(clip_wrong.is_empty(), "{} clip samples differ: {:?}", clip_wrong.len(), &clip_wrong[..clip_wrong.len().min(10)]);
    assert!(evaluate_wrong.is_empty(), "{} evaluations differ: {:?}", evaluate_wrong.len(), &evaluate_wrong[..evaluate_wrong.len().min(10)]);
}

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
#[should_panic(expected = "no scripted inputs at frame 500")]
fn a_recording_missing_a_frames_inputs_fails() {
    let dir = edited_recording("inputs", "inputs.tsv", |l| !l.starts_with("500\t"));
    Run::new(&dir, fixture()).run(None);
}

#[test]
fn an_input_keeps_a_name_with_tabs_line_breaks_and_equals_signs() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("input-names");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("inputs.tsv"), "7\t0,0,0,0,0,0,1\t-\tA\\tB\\nC\\\\D=E=0.5\tPlain=1\n8\t-\t-\n").unwrap();
    let inputs = reference::animator::inputs(&dir);
    assert_eq!(inputs[&7], vec![("A\tB\nC\\D=E".to_string(), 0.5), ("Plain".to_string(), 1.0)]);
    assert!(inputs[&8].is_empty());
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
        "root motion: loop pose, baked rotation and height, the feet basis, both offsets, unbaked XZ",
        &["muscle:RootQ.y", "muscle:RootQ.w", "muscle:RootT.y"],
    ),
    ("root motion: an unbaked centre-of-mass height under the sitting playable", &["muscle:RootT.y", "playable:2"]),
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

/// A direct tree's weight parameter below zero plays its leaf at weight zero: it adds nothing, as the reference's
/// frame-time smoothing shows the frame a timer restarts, but still writes, as a ramp ending a hair below zero shows.
#[test]
fn a_negative_direct_weight_plays_its_leaf_at_zero() {
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
    assert_eq!(out.iter().map(|l| (l.clip, l.weight)).collect::<Vec<_>>(), vec![(0, 0.0), (1, 0.5)]);
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

fn constant(binding: avatar_core::model::Binding, value: f32) -> avatar_core::model::Curve {
    use avatar_core::model::{AnimationCurve, Curve, Keyframe, WeightedMode};
    let key = Keyframe { time: 0.0, value, in_tangent: 0.0, out_tangent: 0.0, in_weight: 1.0 / 3.0, out_weight: 1.0 / 3.0, weighted_mode: WeightedMode::None };
    Curve { binding, curve: AnimationCurve { keys: vec![key] } }
}

fn muscle(name: &str, value: f32) -> avatar_core::model::Curve {
    constant(avatar_core::model::Binding::Muscle { name: name.into() }, value)
}

fn blend_shape(name: &str, value: f32) -> avatar_core::model::Curve {
    constant(avatar_core::model::Binding::BlendShape { node: avatar_core::model::NodeIndex(BODY), name: name.into() }, value)
}

/// The fixture's face mesh.
const BODY: u32 = 75;

fn clip_mut<'a>(avatar: &'a mut avatar_core::Avatar, name: &str) -> &'a mut avatar_core::model::Clip {
    avatar.model.clips.iter_mut().find(|c| c.name == name).unwrap()
}

fn value(a: &Animator, name: &str) -> f32 {
    target_value(a, avatar_core::animator::motion::Target::Muscle(name.into()))
}

fn shape(a: &Animator, name: &str) -> f32 {
    target_value(a, avatar_core::animator::motion::Target::BlendShape(BODY, name.into()))
}

fn target_value(a: &Animator, t: avatar_core::animator::motion::Target) -> f32 {
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

/// A layer covers the muscles its controller binds with what it last wrote while a motionless state writes nothing.
#[test]
fn a_layer_holds_what_a_motionless_state_leaves() {
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

/// A layer starts in its machine's default state; the entry node's transitions apply only when a transition enters
/// the machine.
#[test]
fn a_layer_starts_in_its_default_state() {
    use avatar_core::model::{Destination, EntryTransition, LayerContent, PlayableLayerType};
    let mut avatar = fixture();
    let gesture = avatar.model.layers.get_mut(&PlayableLayerType::Gesture).unwrap();
    let LayerContent::StateMachine { state_machine } = &mut gesture.controller.as_mut().unwrap().layers[0].content else { panic!() };
    let default = state_machine.default_state.unwrap();
    let other = state_machine.states.iter().find(|s| s.id != default).unwrap();
    let (default_name, other_name) = (state_machine.states.iter().find(|s| s.id == default).unwrap().name.clone(), other.name.clone());
    state_machine.entry_transitions.push(EntryTransition { destination: Destination::State { id: other.id }, conditions: vec![], mute: false, solo: false });
    let mut a = Animator::new(&avatar);
    a.step(1.0 / 60.0);
    let gesture = a.playable_kinds().iter().position(|k| *k == PlayableLayerType::Gesture).unwrap();
    let state = a.states().into_iter().find(|s| s.playable == gesture && s.layer == 0).unwrap().current;
    assert!(state.ends_with(&format!(".{default_name}")), "started in {state}, not {default_name} (the entry node leads to {other_name})");
}

/// A 1D blend weights each end by its own distance, (b - x) / (b - a) and (x - a) / (b - a), and a state's clock
/// advances by dt · speed / duration: one corpus state, whose duration is the weight of a 1/60 s clip as its
/// parameter nears the other end, reaches the recorded clock bit for bit only so.
#[test]
fn one_dimensional_weights_and_state_clocks_round_as_unity_does() {
    use avatar_core::animator::motion::weights_1d;
    let w = weights_1d(&[0.251, 1.0], 0.9999943);
    let (near, far) = (w[0].unwrap(), w[1].unwrap());
    assert_eq!(near, (1.0f32 - 0.9999943) / (1.0 - 0.251));
    assert_eq!(far, (0.9999943f32 - 0.251) / (1.0 - 0.251));
    let dt = 1.0f32 / 60.0;
    let duration = near * (1.0 / 60.0);
    assert_eq!(309838.563f32 + dt * 1.0 / duration, 440735.781f32);
}

/// A goal a humanoid clip's curves leave at the body sits at its rest offset from the clip's body; a mirrored clip
/// takes the mirror of the other side's offset and turns the goal's frame half a revolution, about Y for a foot.
#[test]
fn an_uncurved_goal_follows_the_body_and_mirrors_with_its_clip() {
    let mut avatar = fixture();
    let walk = clip_mut(&mut avatar, "Walk");
    walk.curves.extend(["LeftFootT.x", "LeftFootT.y", "LeftFootT.z", "LeftFootQ.x", "LeftFootQ.y", "LeftFootQ.z"].map(|n| muscle(n, 0.0)));
    walk.curves.push(muscle("LeftFootQ.w", 1.0));
    let rest = avatar.model.humanoid.rest.as_mut().unwrap();
    rest.left_foot.position[0] = -0.06;
    rest.right_foot.position[0] = 0.07;
    // Stand mirrors its clip, and the Flip parameter mirrors the state, undoing it.
    let standing = |mirrored: bool| {
        let mut a = Animator::new(&avatar);
        a.set("Flip", !mirrored as u8 as f32);
        run(&mut a, 10);
        ["RootT.x", "LeftFootT.x", "LeftFootQ.y", "LeftFootQ.w"].map(|n| value(&a, n))
    };
    let (plain, mirrored) = (standing(false), standing(true));
    assert_eq!(plain[1] - plain[0], -0.06, "{plain:?}");
    assert_eq!((plain[2].abs(), plain[3].abs()), (0.0, 1.0), "{plain:?}");
    assert!((mirrored[1] - mirrored[0] + 0.07).abs() < 1e-6, "{mirrored:?}");
    assert_eq!((mirrored[2].abs(), mirrored[3].abs()), (1.0, 0.0), "{mirrored:?}");
}

fn clip(name: &str, curves: Vec<avatar_core::model::Curve>) -> avatar_core::model::Clip {
    avatar_core::model::Clip { name: name.into(), length: 0.0, sample_rate: 60.0, high_quality_curve: true, looping: false, cycle_offset: 0.0, additive_reference_pose: None, humanoid: None, curves, slot_curves: vec![] }
}

/// Under a write-defaults direct tree, a shape only a 1D tree's end at weight zero writes is left to the layers
/// beneath: a toggle's off end, its other end binding the shape, keeps the radial below it. A transition out of the
/// state starts from that value, the source covering none of it.
#[test]
fn a_direct_trees_unweighted_end_leaves_the_value_beneath() {
    use avatar_core::model::{AnimatorLayer, AnimatorParameter, AnimatorParameterValue, PlayableLayerType};
    let mut avatar = fixture();
    let m = &mut avatar.model;
    let c = m.clips.len() as u32;
    m.clips.extend([clip("Below", vec![blend_shape("AwShape", 42.5)]), clip("Off", vec![blend_shape("AwOther", 0.0)]), clip("On", vec![blend_shape("AwShape", 100.0)])]);
    let state = |id: u32, wd: bool, motion: serde_json::Value, transitions: serde_json::Value| {
        serde_json::json!({"id": id, "name": format!("S{id}"), "speed": 1, "cycleOffset": 0, "mirror": false, "motion": motion, "writeDefaults": wd, "ikOnFeet": false, "behaviours": [], "transitions": transitions})
    };
    let layer = |id: u32, states: Vec<serde_json::Value>| -> AnimatorLayer {
        serde_json::from_value(serde_json::json!({"name": format!("L{id}"), "defaultWeight": 1, "blending": "override", "ikPass": false,
            "content": {"type": "stateMachine", "stateMachine": {"id": id, "name": format!("L{id}"), "states": states, "stateMachines": [], "defaultState": id, "anyStateTransitions": [], "entryTransitions": [], "stateMachineTransitions": [], "behaviours": []}}}))
        .unwrap()
    };
    let toggle = serde_json::json!({"blendTree": {"name": "T", "algorithm": {"type": "direct", "normalizedBlendValues": false, "children": [
        {"motion": {"blendTree": {"name": "Toggle", "algorithm": {"type": "simple1D", "blendParameter": "AwToggle", "children": [
            {"motion": {"clip": c + 1}, "threshold": 0, "timeScale": 1, "cycleOffset": 0, "mirror": false},
            {"motion": {"clip": c + 2}, "threshold": 1, "timeScale": 1, "cycleOffset": 0, "mirror": false}]}}},
         "directBlendParameter": "AwOne", "timeScale": 1, "cycleOffset": 0, "mirror": false}]}}});
    let leave = serde_json::json!([{"destination": {"type": "state", "id": 9102}, "conditions": [{"mode": "if", "parameter": "AwLeave", "threshold": 0}],
        "duration": {"fixed": 0.5}, "offset": 0, "interruptionSource": "none", "orderedInterruption": true, "canTransitionToSelf": false, "mute": false, "solo": false}]);
    let fx = m.layers.get_mut(&PlayableLayerType::Fx).unwrap().controller.as_mut().unwrap();
    for (name, default) in [("AwOne", 1.0), ("AwToggle", 0.0)] {
        fx.parameters.push(AnimatorParameter { name: name.into(), default: AnimatorParameterValue::Float(default) });
    }
    fx.parameters.push(AnimatorParameter { name: "AwLeave".into(), default: AnimatorParameterValue::Bool(false) });
    fx.layers.push(layer(9000, vec![state(9000, false, serde_json::json!({"clip": c}), serde_json::json!([]))]));
    fx.layers.push(layer(9100, vec![state(9100, true, toggle, leave), state(9102, true, serde_json::json!({"clip": c + 1}), serde_json::json!([]))]));
    let mut a = Animator::new(&avatar);
    run(&mut a, 3);
    assert_eq!(shape(&a, "AwShape"), 42.5);
    a.set("AwLeave", 1.0);
    let leaving: Vec<f32> = (0..4).map(|_| {
        run(&mut a, 1);
        shape(&a, "AwShape")
    }).collect();
    assert!(leaving.iter().all(|&v| v > 35.0), "{leaving:?}");
    run(&mut a, 40);
    assert_eq!(shape(&a, "AwShape"), 42.5);
    let mut b = Animator::new(&avatar);
    b.set("AwToggle", 1.0);
    run(&mut b, 3);
    assert_eq!(shape(&b, "AwShape"), 100.0);
}

/// Handed to animation, lip sync stops writing: a viseme shape no controller binds keeps the value lip sync last gave
/// it, and one a controller binds is the animator's.
#[test]
fn lip_sync_handed_to_animation_leaves_its_last_shape() {
    let speak_then_hand_over = |avatar: &avatar_core::Avatar| {
        let mut a = Animator::new(avatar);
        a.set_builtin("Viseme", 10.0);
        run(&mut a, 3);
        assert_eq!(shape(&a, "vrc.v_aa"), 100.0);
        a.tracking[avatar_core::animator::MOUTH] = avatar_core::model::TrackingType::Animation;
        a.set_builtin("Viseme", 0.0);
        run(&mut a, 3);
        shape(&a, "vrc.v_aa")
    };
    assert_eq!(speak_then_hand_over(&fixture()), 100.0);
    let mut bound = fixture();
    clip_mut(&mut bound, "Spin").curves.push(blend_shape("vrc.v_aa", 25.0));
    assert_eq!(speak_then_hand_over(&bound), 25.0);
}

/// A layer playing no humanoid clip leaves the human pose to the layers beneath, as an emote under a state-tracking
/// layer of empty clips shows.
#[test]
fn a_layer_of_generic_clips_gives_no_human_pose() {
    use avatar_core::model::{AnimatorLayer, PlayableLayerType};
    let mut avatar = fingered();
    let mut a = Animator::new(&avatar);
    run(&mut a, 5);
    let beneath = value(&a, "Left Upper Leg Front-Back");
    assert_ne!(beneath, 0.0);
    let m = &mut avatar.model;
    let c = m.clips.len() as u32;
    m.clips.push(clip("Empty generic", vec![]));
    let layer: AnimatorLayer = serde_json::from_value(serde_json::json!({"name": "Regulator", "defaultWeight": 1, "blending": "override", "ikPass": false,
        "content": {"type": "stateMachine", "stateMachine": {"id": 9200, "name": "Regulator", "states": [{"id": 9200, "name": "Wait", "speed": 1, "cycleOffset": 0, "mirror": false,
            "motion": {"clip": c}, "writeDefaults": false, "ikOnFeet": false, "behaviours": [], "transitions": []}],
            "stateMachines": [], "defaultState": 9200, "anyStateTransitions": [], "entryTransitions": [], "stateMachineTransitions": [], "behaviours": []}}}))
    .unwrap();
    m.layers.get_mut(&PlayableLayerType::Base).unwrap().controller.as_mut().unwrap().layers.extend([layer.clone(), layer]);
    let mut a = Animator::new(&avatar);
    run(&mut a, 5);
    assert_eq!(value(&a, "Left Upper Leg Front-Back"), beneath);
}

#[test]
fn a_numbered_summary_holds_no_name() {
    let mut m = Mismatches::default();
    m.note("active:Armature/Tail", 7, "0 (want 1)".into());
    m.note("2:0:Sit", 9, "Upright (want Slouch)".into());
    m.note("2:0:Sit", 10, "Upright (want Slouch)".into());
    assert_eq!(m.numbered(20), "3 of 0 compared samples differ (0 first-frame samples not compared)\n  0: 1 frames, first at 7\n  1: 2 frames, first at 9");
    let named = m.summary(20);
    assert!(named.contains("\n  0 active:Armature/Tail: 1 frames, first at 7: 0 (want 1)"), "{named}");
    assert!(named.contains("\n  1 2:0:Sit: 2 frames, first at 9: Upright (want Slouch)"), "{named}");
}

/// A crossfade blends each IK goal from the source's rotation toward the destination's as it lies in the source's
/// hemisphere, as the die pose's foot goal (w below zero) shows on entering it from a standing pose; once the
/// transition ends the goal takes the clip's own sign.
#[test]
fn a_crossfade_turns_a_goal_the_short_way() {
    let mut avatar = fixture();
    let goal = |q: [f32; 4]| ["LeftFoot", "RightFoot"].into_iter().flat_map(move |g| ["x", "y", "z", "w"].into_iter().zip(q).map(move |(c, v)| muscle(&format!("{g}Q.{c}"), v)));
    clip_mut(&mut avatar, "Stand").curves.extend(goal([0.0, 0.0, 0.0, 1.0]));
    clip_mut(&mut avatar, "Walk").curves.extend(goal([0.6, 0.0, 0.0, -0.8]));
    let mut a = Animator::new(&avatar);
    a.set("Flip", 1.0);
    run(&mut a, 10);
    a.set_builtin("VelocityZ", 2.0);
    run(&mut a, 6);
    for g in ["LeftFoot", "RightFoot"] {
        let (x, w) = (value(&a, &format!("{g}Q.x")), value(&a, &format!("{g}Q.w")));
        assert!(x < -0.1 && w > 0.9, "{g} mid-transition: ({x}, {w})");
    }
    run(&mut a, 30);
    for g in ["LeftFoot", "RightFoot"] {
        let w = value(&a, &format!("{g}Q.w"));
        assert!(w < -0.5, "{g} after the transition keeps its clip's sign: {w}");
    }
}

/// A crossfade blends a node's rotation from the source's toward the destination's as it lies in the source's
/// hemisphere, so it turns the short way, as a corpus toggle crossfading from an empty write-defaults state into a
/// clip holding a bone half a revolution round shows. A transition interrupted midway starts from that short-way pose.
#[test]
fn a_crossfade_turns_a_node_the_short_way() {
    use avatar_core::animator::motion::Target;
    use avatar_core::model::{Axis, Binding, InterruptionSource, LayerContent, NodeIndex, PlayableLayerType, TransformProperty};
    const HAT: u32 = 76;
    let turned = [(Axis::X, 0.0), (Axis::Y, 0.0), (Axis::Z, std::f32::consts::FRAC_1_SQRT_2), (Axis::W, -std::f32::consts::FRAC_1_SQRT_2)];
    let angles = |interrupt: Option<usize>| {
        let mut avatar = fixture();
        let fx = avatar.model.layers.get_mut(&PlayableLayerType::Fx).unwrap().controller.as_mut().unwrap();
        let LayerContent::StateMachine { state_machine } = &mut fx.layers.iter_mut().find(|l| l.name == "Hat").unwrap().content else { panic!() };
        for s in &mut state_machine.states {
            s.write_defaults = true;
            s.transitions.iter_mut().for_each(|t| t.interruption_source = InterruptionSource::Destination);
        }
        clip_mut(&mut avatar, "HatOn")
            .curves
            .extend(turned.map(|(component, v)| constant(Binding::Transform { node: NodeIndex(HAT), property: TransformProperty::Rotation, component }, v)));
        let mut a = Animator::new(&avatar);
        run(&mut a, 10);
        a.set("Hat", 1.0);
        let mut angles = Vec::new();
        for frame in 0..60 {
            if Some(frame) == interrupt {
                a.set("Hat", 0.0);
            }
            run(&mut a, 1);
            let q = [Axis::X, Axis::Y, Axis::Z, Axis::W].map(|c| target_value(&a, Target::Rotation(HAT, c)));
            let len = q.iter().map(|x| x * x).sum::<f32>().sqrt();
            angles.push(2.0 * (q[3].abs() / len).min(1.0).acos().to_degrees());
        }
        angles
    };
    let through = angles(None);
    assert!(through.iter().any(|d| (10.0..80.0).contains(d)), "never mid-transition: {through:?}");
    assert!(through.iter().all(|d| *d < 90.01), "turned the long way: {through:?}");
    assert!((through.last().unwrap() - 90.0).abs() < 1e-2, "after the transition: {through:?}");
    let back = angles(Some(8));
    assert!(back.iter().all(|d| *d < 90.01), "interrupted, turned the long way: {back:?}");
    assert!(back.iter().all(|d| *d < 60.0) && back[9] > 30.0 && back.last().unwrap() < &20.0, "interrupted, never turned back: {back:?}");
}

fn ramp(name: &str) -> avatar_core::model::Curve {
    use avatar_core::model::{AnimationCurve, Binding, Curve, Keyframe, WeightedMode};
    let key = |t: f32| Keyframe { time: t, value: t, in_tangent: 1.0, out_tangent: 1.0, in_weight: 1.0 / 3.0, out_weight: 1.0 / 3.0, weighted_mode: WeightedMode::None };
    Curve { binding: Binding::Muscle { name: name.into() }, curve: AnimationCurve { keys: vec![key(0.0), key(1.0)] } }
}

fn base_layer(blending: &str, states: Vec<serde_json::Value>) -> avatar_core::model::AnimatorLayer {
    let id = states[0]["id"].clone();
    serde_json::from_value(serde_json::json!({"name": "Probe", "defaultWeight": 1, "blending": blending, "ikPass": false,
        "content": {"type": "stateMachine", "stateMachine": {"id": 9300, "name": "Probe", "states": states, "stateMachines": [], "defaultState": id,
            "anyStateTransitions": [], "entryTransitions": [], "stateMachineTransitions": [], "behaviours": []}}}))
    .unwrap()
}

/// An additive layer crossfading between two states adds each state's offset from its reference by its share of the
/// transition, the destination's rotation offset turned into the source's hemisphere until the transition ends, when
/// it takes its own sign: corpus 3 fades a sitting pose in so, its body rotation offset in the other hemisphere. A
/// destination whose clip and reference turn alike adds no turn, whatever hemisphere both lie in.
#[test]
fn an_additive_crossfade_blends_both_states_offsets() {
    use avatar_core::model::{AnimatorParameter, AnimatorParameterValue, PlayableLayerType};
    let half = 15f32.to_radians().sin_cos();
    let body = |w: f32| [muscle("RootQ.x", 0.0), muscle("RootQ.y", w * half.0), muscle("RootQ.z", 0.0), muscle("RootQ.w", w * half.1)];
    let fade = |reference_turned: bool| {
        let mut avatar = fixture();
        let m = &mut avatar.model;
        let c = m.clips.len() as u32;
        let settings: avatar_core::model::HumanoidClip = serde_json::from_value(serde_json::json!({"mirror": false, "loopPose": false,
            "rotation": {"bakeIntoPose": true, "basedUpon": "original", "offsetDegrees": 0},
            "height": {"bakeIntoPose": true, "basedUpon": "original", "offset": 0}, "xz": {"bakeIntoPose": true, "basedUpon": "original"}}))
        .unwrap();
        let mut still = clip("AwStill", vec![muscle("Spine Front-Back", 0.1)]);
        let mut reference = clip("AwReference", [vec![muscle("Spine Front-Back", 0.1)], body(if reference_turned { -1.0 } else { 0.0 }).into()].concat());
        if !reference_turned {
            reference.curves.last_mut().unwrap().curve.keys[0].value = 1.0;
        }
        let mut lean = clip("AwLean", [vec![muscle("Spine Front-Back", 0.6)], body(-1.0).into()].concat());
        for x in [&mut still, &mut reference, &mut lean] {
            x.humanoid = Some(settings.clone());
        }
        lean.additive_reference_pose = Some(serde_json::from_value(serde_json::json!({"clip": c + 1, "time": 0})).unwrap());
        m.clips.extend([still, reference, lean]);
        let go = serde_json::json!([{"destination": {"type": "state", "id": 9302}, "conditions": [{"mode": "if", "parameter": "AwGo", "threshold": 0}],
            "duration": {"fixed": 0.25}, "offset": 0, "interruptionSource": "none", "orderedInterruption": true, "canTransitionToSelf": false, "mute": false, "solo": false}]);
        let state = |id: u32, clip: u32, transitions: serde_json::Value| {
            serde_json::json!({"id": id, "name": format!("S{id}"), "speed": 1, "cycleOffset": 0, "mirror": false, "motion": {"clip": clip}, "writeDefaults": true, "ikOnFeet": false, "behaviours": [], "transitions": transitions})
        };
        let base = m.layers.get_mut(&PlayableLayerType::Base).unwrap().controller.as_mut().unwrap();
        base.parameters.push(AnimatorParameter { name: "AwGo".into(), default: AnimatorParameterValue::Bool(false) });
        base.layers.push(base_layer("additive", vec![state(9301, c, go), state(9302, c + 2, serde_json::json!([]))]));
        let mut a = Animator::new(&avatar);
        run(&mut a, 10);
        let beneath = value(&a, "Spine Front-Back");
        let turn = |a: &Animator| 2.0 * value(a, "RootQ.w").abs().min(1.0).acos().to_degrees();
        a.set("AwGo", 1.0);
        (0..20)
            .map(|_| {
                run(&mut a, 1);
                (value(&a, "Spine Front-Back") - beneath, turn(&a), value(&a, "RootQ.w"))
            })
            .collect::<Vec<_>>()
    };
    let fading = fade(false);
    let mid = fading.iter().find(|f| f.0 > 0.2).unwrap();
    assert!(mid.0 < 0.3 && (10.0..20.0).contains(&mid.1), "halfway: {fading:?}");
    assert!(fading.iter().all(|f| f.1 < 30.01 && (-0.01..0.51).contains(&f.0)), "{fading:?}");
    let end = fading.last().unwrap();
    assert!((end.0 - 0.5).abs() < 1e-5 && (end.1 - 30.0).abs() < 1e-2 && end.2 < 0.0, "after: {end:?}");
    let alike = fade(true);
    assert!(alike.iter().all(|f| f.1 < 1e-2), "turned by a destination that adds no turn: {alike:?}");
    assert!((alike.last().unwrap().0 - 0.5).abs() < 1e-5, "{alike:?}");
}

/// A mirrored state whose time a parameter drives plays its clip at that time, without the half cycle a mirrored
/// state's clock otherwise adds: corpus 3 scrubs mirrored sitting poses so.
#[test]
fn a_mirrored_state_driven_by_a_time_parameter_plays_at_that_time() {
    use avatar_core::model::{AnimatorParameter, AnimatorParameterValue, PlayableLayerType};
    let mut avatar = fixture();
    let m = &mut avatar.model;
    let c = m.clips.len() as u32;
    let mut scrub = clip("AwScrub", vec![ramp("Left Upper Leg Front-Back")]);
    scrub.length = 1.0;
    m.clips.push(scrub);
    let base = m.layers.get_mut(&PlayableLayerType::Base).unwrap().controller.as_mut().unwrap();
    base.parameters.push(AnimatorParameter { name: "AwTime".into(), default: AnimatorParameterValue::Float(0.0) });
    let state = |mirror: bool| {
        serde_json::json!({"id": 9301, "name": "Scrub", "speed": 1, "cycleOffset": 0, "mirror": mirror, "timeParameter": "AwTime", "motion": {"clip": c},
            "writeDefaults": true, "ikOnFeet": false, "behaviours": [], "transitions": []})
    };
    let leg = |mirror: bool, time: f32| {
        let mut avatar = avatar.clone();
        let base = avatar.model.layers.get_mut(&PlayableLayerType::Base).unwrap().controller.as_mut().unwrap();
        base.layers.push(base_layer("override", vec![state(mirror)]));
        let mut a = Animator::new(&avatar);
        a.set("AwTime", time);
        run(&mut a, 3);
        value(&a, if mirror { "Right Upper Leg Front-Back" } else { "Left Upper Leg Front-Back" })
    };
    for time in [0.0, 0.25] {
        assert!((leg(false, time) - time).abs() < 1e-5, "unmirrored at {time}: {}", leg(false, time));
        assert!((leg(true, time) - time).abs() < 1e-5, "mirrored at {time}: {}", leg(true, time));
    }
}

/// The generic probe plays a blendshape and a node's position through an additive layer in the Base, Additive and FX
/// playables, against a reference pose with and without the property, alone and written by only one side of a
/// crossfade; then clips with and without high-quality curves binding position, rotation, Euler rotation, scale and a
/// blendshape, stepped keys, curves on and off the frame grid in one clip, inner keys at offsets either side of the
/// grid threshold, and clips sampled at and past their end. Parameters, states, state times and pose bindings follow
/// the reference, and so does every value.
#[test]
fn each_generic_additive_and_curve_quality_case_follows_its_reference_trace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("fixtures/reference/generic-animator");
    let avatar = avatar_core::read(&std::fs::read(root.join("fixtures/unity-generic.glb")).unwrap()).unwrap();
    assert_eq!(avatar.model.clips.iter().filter(|c| !c.high_quality_curve).count(), 15);
    let rep = Run::new(&dir, avatar).run(None);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert_eq!(rep.clocks.wrong, 0, "state times: {}", rep.clocks.summary(20));
    assert_eq!(rep.bindings.wrong, 0, "pose bindings: {}", rep.bindings.summary(20));
    assert!(rep.clocks.compared > 130_000, "{} state times compared", rep.clocks.compared);
    assert!(rep.values.compared > 2_000_000, "{} values compared", rep.values.compared);
    assert_eq!(rep.values.wrong, 0, "values: {}", rep.values.summary(20));
}

/// The frame grid, fill and hold probe plays an additive body offset over a body a layer beneath holds turned or
/// turns; low-quality clips whose one curve lies 0.15 to 0.37 of a frame off the grid, alone, beside on-grid curves with
/// all, one or two inner keys off, stepped, or as a muscle curve; override states with and without write defaults
/// animating one position and one scale channel of a node, with and without an additive layer above; and an additive
/// layer idling after a motion in empty states with and without write defaults, and leaving a held empty state through
/// a fade. Parameters, states and state times follow the reference; each case's wrong samples of the values and pose
/// bindings it probes are held to today's counts.
#[test]
fn each_frame_grid_fill_and_hold_case_follows_its_reference_trace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("fixtures/reference/gridhold-animator");
    let avatar = avatar_core::read(&std::fs::read(root.join("fixtures/unity-gridhold.glb")).unwrap()).unwrap();
    assert_eq!(avatar.model.clips.iter().filter(|c| !c.high_quality_curve).count(), 30);
    let rep = Run::new(&dir, avatar).run(None);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert_eq!(rep.clocks.wrong, 0, "state times: {}", rep.clocks.summary(20));
    assert!(rep.clocks.compared > 90_000, "{} state times compared", rep.clocks.compared);
    assert!(rep.values.compared > 1_000_000, "{} values compared", rep.values.compared);
    assert_eq!(rep.values.wrong, 20_112, "values: {}", rep.values.summary(20));
    assert_eq!(rep.bindings.wrong, 838, "pose bindings: {}", rep.bindings.summary(20));
    // (case label, wrong samples of its probed values, of its probed pose bindings), in case order.
    let held: [(&str, usize, usize); 38] = HELD;
    let probed = |id: usize, label: &str| -> Vec<String> {
        let w: Vec<&str> = label.split(' ').collect();
        match w[0] {
            "body" => ["x", "y", "z", "w"].iter().map(|a| format!("muscle:RootQ.{a}")).collect(),
            "grid" => ["GridOff", "GridOn", "GridOnOther"].iter().map(|s| format!("shape:Body:{s}")).collect(),
            "fill" => vec![format!("xform:Fill{id}"), format!("xform:Mover{id}")],
            _ => vec![format!("shape:Body:Hold{id}"), format!("xform:Holder{id}")],
        }
    };
    let steps = reference::animator::steps(&dir);
    let cases: Vec<_> = steps.iter().filter(|s| s.label.starts_with("case ")).collect();
    assert_eq!(cases.len(), held.len());
    let mut found = Vec::new();
    for (i, (s, (name, _, _))) in cases.iter().zip(held).enumerate() {
        assert!(s.label.contains(&format!(": {} {name}", i + 1)), "{} is not {name}", s.label);
        let keys = probed(i + 1, name);
        let within = |m: &Mismatches| m.samples.range((s.frame, String::new())..(s.frame + s.frames, String::new())).filter(|(_, k)| keys.contains(k)).count();
        found.push((name, within(&rep.values), within(&rep.bindings)));
    }
    assert_eq!(found, held, "wrong samples of each case's probed values and bindings");
}

const HELD: [(&str, usize, usize); 38] = [
    ("body turned", 0, 450),
    ("body turning", 0, 388),
    ("grid offset_0_15 alone", 0, 0),
    ("grid offset_0_20 alone", 0, 0),
    ("grid offset_0_25 alone", 76, 0),
    ("grid offset_0_30 alone", 76, 0),
    ("grid offset_0_37 alone", 76, 0),
    ("grid offset_0_15 beside_on_grid", 0, 0),
    ("grid offset_0_20 beside_on_grid", 0, 0),
    ("grid offset_0_25 beside_on_grid", 142, 0),
    ("grid offset_0_30 beside_on_grid", 142, 0),
    ("grid offset_0_37 beside_on_grid", 142, 0),
    ("grid offset_0_15 one_key", 0, 0),
    ("grid offset_0_20 one_key", 0, 0),
    ("grid offset_0_25 one_key", 137, 0),
    ("grid offset_0_30 one_key", 137, 0),
    ("grid offset_0_37 one_key", 137, 0),
    ("grid offset_0_15 two_keys", 0, 0),
    ("grid offset_0_20 two_keys", 0, 0),
    ("grid offset_0_25 two_keys", 137, 0),
    ("grid offset_0_30 two_keys", 137, 0),
    ("grid offset_0_37 two_keys", 137, 0),
    ("grid offset_0_15 stepped", 0, 0),
    ("grid offset_0_20 stepped", 0, 0),
    ("grid offset_0_25 stepped", 132, 0),
    ("grid offset_0_30 stepped", 132, 0),
    ("grid offset_0_37 stepped", 132, 0),
    ("grid offset_0_15 muscle", 0, 0),
    ("grid offset_0_20 muscle", 0, 0),
    ("grid offset_0_25 muscle", 0, 0),
    ("grid offset_0_30 muscle", 0, 0),
    ("grid offset_0_37 muscle", 0, 0),
    ("fill write_defaults alone", 150, 0),
    ("fill no_write_defaults alone", 150, 0),
    ("hold idle", 240, 0),
    ("hold fade_out", 60, 0),
    ("fill write_defaults additive_above", 150, 0),
    ("fill no_write_defaults additive_above", 300, 0),
];
