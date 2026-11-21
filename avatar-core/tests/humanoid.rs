//! The humanoid bones posed from muscles and the body against Unity's own `HumanPoseHandler` on the fixture
//! (`reference/Editor/HumanSamples.cs`): every muscle alone, pairs of muscles, and random poses.

mod reference;

use std::collections::BTreeMap;
use std::path::Path;

use avatar_core::humanoid::{muscle_names, Body, Human};
use avatar_core::model::HumanBodyBone;
use avatar_core::physbones::Nodes;
use glam::{Quat, Vec3};

/// The worst rotation and, on the hips, position by sample kind and whether the bone is the hips, and the worst
/// position of any other bone, posing `glb` as Unity posed it in `samples`.
fn worst(glb: &str, samples: &str) -> (BTreeMap<(String, bool), (f32, f32)>, f32, usize) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let avatar = avatar_core::read(&std::fs::read(root.join(glb)).unwrap()).unwrap();
    let nodes = Nodes::read(&avatar).unwrap();
    let human = Human::new(&avatar.model.humanoid).unwrap();
    let dir = root.join(samples);
    let unity: Vec<String> = std::fs::read_to_string(root.join("fixtures/reference/human-samples/muscles.tsv")).unwrap().lines().map(str::to_string).collect();
    let ours = muscle_names();
    let index: Vec<usize> = unity.iter().map(|n| ours.iter().position(|o| o == n).unwrap_or_else(|| panic!("{n} is no muscle of the core's"))).collect();
    let mut worst: BTreeMap<(String, bool), (f32, f32)> = BTreeMap::new();
    let mut placed = 0.0f32;
    let mut samples = 0;
    for line in reference::text(&dir.join("humansamples.tsv")).lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        let values: Vec<f32> = f[2].split(',').map(|x| x.parse().unwrap()).collect();
        let mut muscles = vec![0.0; ours.len()];
        for (i, v) in values.iter().enumerate() {
            muscles[index[i]] = *v;
        }
        let b: Vec<f32> = f[3].split(',').map(|x| x.parse().unwrap()).collect();
        let body = Body { position: Vec3::new(b[0], b[1], b[2]), rotation: Quat::from_xyzw(b[3], b[4], b[5], b[6]) };
        let mut locals = nodes.rest.clone();
        human.pose(&nodes.parent, &mut locals, &muscles, body);
        for cell in &f[4..] {
            let (name, pose) = cell.split_once('=').unwrap();
            let bone: HumanBodyBone = serde_json::from_value(serde_json::Value::String(name.into())).unwrap();
            let want = reference::unity(&pose.split(',').map(|x| x.parse().unwrap()).collect::<Vec<f32>>());
            let got = locals[avatar.model.humanoid.bones[&bone].node.0 as usize];
            let hips = bone == HumanBodyBone::Hips;
            let e = worst.entry((f[1].to_string(), hips)).or_default();
            e.0 = e.0.max(got.rotation.angle_between(want.rotation).to_degrees());
            let d = (got.translation - want.translation).length();
            if hips {
                e.1 = e.1.max(d);
            } else {
                placed = placed.max(d);
            }
        }
        samples += 1;
    }
    (worst, placed, samples)
}

#[test]
fn bones_follow_unity_s_human_pose() {
    let (worst, placed, samples) = worst("fixtures/unity-synthetic.glb", "fixtures/reference/human-samples");
    assert_eq!(samples, 95 * 16 + 400);
    assert!(placed < 1e-6, "a bone but the hips {placed} m from Unity's");
    for ((kind, hips), (angle, distance)) in &worst {
        println!("{kind} {}: {angle:.5}°, {distance:.2e} m", if *hips { "hips" } else { "other bones" });
        // The body frame turns about its up axis by up to 0.15° from Unity's when the spine bends and twists in
        // several muscles at once (docs/model.md, "Humanoid poses"), which moves the hips with it.
        let (degrees, metres) = if *hips && kind == "random" { (0.15, 2e-4) } else { (0.1, 1e-4) };
        assert!(*angle < degrees && *distance < metres, "{kind} {}: {angle}°, {distance} m", if *hips { "hips" } else { "other bones" });
    }
}

/// Unity's human stream puts every humanoid bone but the hips where the human description's skeleton has it, whatever
/// the node's own position: the hips follow from those positions.
#[test]
fn bones_stand_at_the_skeleton_s_positions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let avatar = avatar_core::read(&std::fs::read(root.join("fixtures/unity-synthetic.glb")).unwrap()).unwrap();
    let nodes = Nodes::read(&avatar).unwrap();
    let human = Human::new(&avatar.model.humanoid).unwrap();
    let h = &avatar.model.humanoid;
    let head = h.bones[&HumanBodyBone::Head].node.0 as usize;
    let hips = h.bones[&HumanBodyBone::Hips].node.0 as usize;
    let at = Vec3::from_array(h.skeleton.iter().find(|s| s.node.0 as usize == head).unwrap().position);
    let body = Body { position: Vec3::new(0.0, 1.0, 0.0), rotation: Quat::IDENTITY };
    let mut moved = nodes.rest.clone();
    moved[head].translation += Vec3::new(0.0, 0.05, 0.01);
    let mut rest = nodes.rest.clone();
    let muscles = vec![0.0; muscle_names().len()];
    human.pose(&nodes.parent, &mut moved, &muscles, body);
    human.pose(&nodes.parent, &mut rest, &muscles, body);
    assert!((moved[head].translation - at).length() < 1e-6, "{} (want {at})", moved[head].translation);
    assert!((moved[hips].translation - rest[hips].translation).length() < 1e-6);
}

/// An unmapped bone sharing the head's name comes first under the hips and the head stands away from its skeleton
/// position: the export still gives the head its own skeleton entry, so it stands where Unity puts it.
#[test]
fn a_bone_named_like_a_humanoid_bone_takes_none_of_its_skeleton() {
    let (worst, placed, samples) = worst("fixtures/unity-duplicate.glb", "fixtures/reference/human-samples/duplicate");
    assert_eq!(samples, 95 * 8);
    assert!(placed < 1e-6, "a bone but the hips {placed} m from Unity's");
    for ((kind, hips), (angle, distance)) in &worst {
        assert!(*angle < 0.1 && *distance < 1e-4, "{kind} {}: {angle}°, {distance} m", if *hips { "hips" } else { "other bones" });
    }
}
