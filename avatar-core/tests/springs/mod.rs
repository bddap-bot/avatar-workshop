//! Shared by the VRM tests and the `vrm` example: three-vrm spring recordings replayed through the core's
//! PhysBones, and CPU skinning of a glTF document.

#![allow(dead_code)]

use avatar_core::physbones::{Frame, Nodes, PhysBones, Transform, STEP};
use avatar_core::Avatar;
use glam::{Mat4, Quat, Vec3};
use serde_json::{Map, Value};

/// A `vrm-reference/run.sh` recording.
pub struct Recording {
    /// Each tip's node and the rest length of its path from its spring's root.
    pub tips: Vec<(usize, f32)>,
    pub roots: Vec<Transform>,
    pub heads: Vec<Quat>,
    /// Per frame, each tip's position in the avatar root's space.
    pub at: Vec<Vec<Vec3>>,
}

pub fn parse(text: &str) -> Recording {
    let mut r = Recording { tips: vec![], roots: vec![], heads: vec![], at: vec![] };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let n = |i: usize| f[i].parse::<f32>().unwrap();
        match f[0] {
            "tip" => r.tips.push((f[1].parse().unwrap(), n(2))),
            "frame" => {
                r.roots.push(Transform { translation: Vec3::new(n(2), n(3), n(4)), rotation: Quat::from_xyzw(n(5), n(6), n(7), n(8)), scale: Vec3::ONE });
                r.heads.push(Quat::from_xyzw(n(9), n(10), n(11), n(12)));
                r.at.push(vec![]);
            }
            "at" => r.at.last_mut().unwrap().push(Vec3::new(n(3), n(4), n(5))),
            other => panic!("unknown recording row {other}"),
        }
    }
    r
}

/// Every node's pose relative to the avatar root from its local poses.
pub fn forward(nodes: &Nodes, locals: &[Transform]) -> Vec<Transform> {
    let mut out: Vec<Option<Transform>> = vec![None; locals.len()];
    fn at(i: usize, nodes: &Nodes, locals: &[Transform], out: &mut Vec<Option<Transform>>) -> Transform {
        if let Some(t) = out[i] {
            return t;
        }
        let t = match nodes.parent[i] {
            Some(p) => at(p, nodes, locals, out).mul(&locals[i]),
            None => locals[i],
        };
        out[i] = Some(t);
        t
    }
    (0..locals.len()).map(|i| at(i, nodes, locals, &mut out)).collect()
}

/// The core's tip positions for every recorded frame, the head node posed as recorded.
pub fn replay(avatar: &Avatar, recording: &Recording, head: Option<usize>) -> Vec<Vec<Vec3>> {
    let nodes = Nodes::read(avatar).unwrap();
    let mut physbones = PhysBones::new(avatar).unwrap();
    let count = nodes.rest.len();
    let mut out = vec![];
    for (f, root) in recording.roots.iter().enumerate() {
        let mut locals = nodes.rest.clone();
        if let Some(h) = head {
            locals[h].rotation = recording.heads[f];
        }
        let frame = Frame { root: *root, nodes: forward(&nodes, &locals), locals: locals.clone(), active: vec![true; count], enabled: vec![true; avatar.model.physbones.len()] };
        physbones.advance(if f == 0 { 0.0 } else { STEP }, &frame);
        for p in 0..avatar.model.physbones.len() {
            let bones: Vec<usize> = physbones.bones(p).collect();
            for (n, t) in bones.into_iter().zip(physbones.local(p)) {
                locals[n] = t;
            }
        }
        let posed = forward(&nodes, &locals);
        out.push(recording.tips.iter().map(|(n, _)| posed[*n].translation).collect());
    }
    out
}

/// Each tip's RMS distance from the recording over its path length.
pub fn errors(recording: &Recording, replayed: &[Vec<Vec3>]) -> Vec<(usize, f32)> {
    recording
        .tips
        .iter()
        .enumerate()
        .map(|(k, (node, length))| {
            let sum: f32 = recording.at.iter().zip(replayed).map(|(r, c)| (r[k] - c[k]).length_squared()).sum();
            (*node, (sum / recording.at.len() as f32).sqrt() / length)
        })
        .collect()
}

pub fn head(avatar: &Avatar) -> Option<usize> {
    avatar.model.humanoid.bones.get(&avatar_core::model::HumanBodyBone::Head).map(|b| b.node.0 as usize)
}

fn accessor(gltf: &Map<String, Value>, bin: &[u8], index: u64) -> Vec<Vec<f32>> {
    let a = &gltf["accessors"][index as usize];
    let view = &gltf["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
    let width = match a["type"].as_str().unwrap() {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        other => panic!("accessor type {other}"),
    };
    let component = a["componentType"].as_u64().unwrap();
    let size = match component {
        5121 => 1,
        5123 => 2,
        5126 => 4,
        other => panic!("component type {other}"),
    };
    let normalized = a["normalized"].as_bool() == Some(true);
    let stride = view["byteStride"].as_u64().map_or(width * size, |s| s as usize);
    let start = view["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    (0..a["count"].as_u64().unwrap() as usize)
        .map(|i| {
            (0..width)
                .map(|c| {
                    let at = start + i * stride + c * size;
                    let b = &bin[at..at + size];
                    match (component, normalized) {
                        (5126, _) => f32::from_le_bytes(b.try_into().unwrap()),
                        (5121, true) => b[0] as f32 / 255.0,
                        (5123, true) => u16::from_le_bytes(b.try_into().unwrap()) as f32 / 65535.0,
                        (5121, false) => b[0] as f32,
                        (5123, false) => u16::from_le_bytes(b.try_into().unwrap()) as f32,
                        _ => unreachable!(),
                    }
                })
                .collect()
        })
        .collect()
}

fn matrix(t: &Transform) -> Mat4 {
    Mat4::from_scale_rotation_translation(t.scale, t.rotation, t.translation)
}

/// Every skinned vertex of the document with its nodes' local poses `locals`, in the root's space, by mesh node
/// and primitive.
pub fn skin(gltf: &Map<String, Value>, bin: &[u8], locals: &[Transform]) -> Vec<Vec3> {
    let nodes = Nodes::of(gltf).unwrap();
    let world = forward(&nodes, locals);
    let mut out = vec![];
    for node in gltf["nodes"].as_array().unwrap() {
        let (Some(mesh), Some(skin)) = (node["mesh"].as_u64(), node["skin"].as_u64()) else { continue };
        let skin = &gltf["skins"][skin as usize];
        let joints: Vec<usize> = skin["joints"].as_array().unwrap().iter().map(|j| j.as_u64().unwrap() as usize).collect();
        let binds = accessor(gltf, bin, skin["inverseBindMatrices"].as_u64().unwrap());
        let palette: Vec<Mat4> = joints.iter().zip(&binds).map(|(&j, b)| matrix(&world[j]) * Mat4::from_cols_slice(b)).collect();
        for p in gltf["meshes"][mesh as usize]["primitives"].as_array().unwrap() {
            let attrs = &p["attributes"];
            let positions = accessor(gltf, bin, attrs["POSITION"].as_u64().unwrap());
            let mut sets = vec![];
            for k in 0.. {
                let (Some(j), Some(w)) = (attrs[format!("JOINTS_{k}")].as_u64(), attrs[format!("WEIGHTS_{k}")].as_u64()) else { break };
                sets.push((accessor(gltf, bin, j), accessor(gltf, bin, w)));
            }
            for (v, p) in positions.iter().enumerate() {
                let p = Vec3::new(p[0], p[1], p[2]);
                let mut sum = Vec3::ZERO;
                for (j, w) in &sets {
                    for c in 0..4 {
                        if w[v][c] > 0.0 {
                            sum += palette[j[v][c] as usize].transform_point3(p) * w[v][c];
                        }
                    }
                }
                out.push(sum);
            }
        }
    }
    out
}
