//! Replays a reference recording's constraints and contacts through the core and scores the constrained targets
//! against `constrained.tsv` and the receivers' parameters against `params.tsv`.

use std::collections::BTreeMap;
use std::path::Path;

use avatar_core::constraints::{Constraints, Scene};
use avatar_core::contacts::{Contacts, Sender};
use avatar_core::model::ContactShape;
use avatar_core::physbones::{Toucher, Transform};
use glam::{Quat, Vec3};

use super::{canonical, text, unity, Replay};

fn relative(parent: &Transform, child: &Transform) -> Transform {
    parent.relative(child)
}

/// One target's error against the reference: the largest position (metres) and rotation (degrees) differences
/// and the frame of each.
#[derive(Clone, Debug, Default)]
pub struct Error {
    pub position: (f32, usize),
    pub rotation: (f32, usize),
    pub frames: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Component {
    Constraint,
    Sender,
    Receiver,
}

/// Each constraint and contact field a curve drives, by frame: (component, index, field, value), signed as the
/// model holds it.
fn fields(dir: &Path, replay: &Replay) -> BTreeMap<usize, Vec<(Component, usize, String, f32)>> {
    let m = &replay.avatar.model;
    let mut out = BTreeMap::<usize, Vec<_>>::new();
    for line in text(&dir.join("values.tsv")).lines() {
        let r: Vec<&str> = line.split('\t').collect();
        let Some((kind, rest)) = r[1].split_once(':') else { continue };
        let (component, nodes): (Component, Vec<u32>) = match kind {
            "constraint" => (Component::Constraint, m.constraints.iter().map(|c| c.node.0).collect()),
            "sender" => (Component::Sender, m.contacts.senders.iter().map(|c| c.node.0).collect()),
            "receiver" => (Component::Receiver, m.contacts.receivers.iter().map(|c| c.node.0).collect()),
            _ => continue,
        };
        let Some((rest, field)) = rest.rsplit_once(':') else { continue };
        let Some((path, k)) = rest.rsplit_once('#') else { continue };
        let Some(node) = replay.names.iter().position(|n| n.as_deref() == Some(canonical(path).as_str())) else { continue };
        let k: usize = k.parse().unwrap();
        let Some(index) = nodes.iter().enumerate().filter(|(_, n)| **n as usize == node).nth(k).map(|(i, _)| i) else { continue };
        let v: f32 = r[2].parse().unwrap();
        out.entry(r[0].parse().unwrap()).or_default().push((component, index, field.to_string(), v));
    }
    out
}

/// The senders outside the avatar a dynamics probe recording moves through its receivers, and their poses by
/// frame in the avatar's root space.
fn outside(dir: &Path) -> (Vec<Sender>, BTreeMap<usize, Vec<(usize, Transform)>>) {
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("meta.json")).unwrap()).unwrap();
    let floats = |v: &serde_json::Value| -> Vec<f32> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect() };
    let mut senders = vec![];
    let mut paths = vec![];
    for (id, s) in meta.get("outside_senders").and_then(|s| s.as_array()).into_iter().flatten().enumerate() {
        let (radius, height) = (s["radius"].as_f64().unwrap() as f32, s["height"].as_f64().unwrap() as f32);
        let p = floats(&s["position"]);
        let q = floats(&s["rotation"]);
        senders.push(Sender {
            id: id as u64,
            at: Transform::IDENTITY,
            shape: if s["shape"] == "capsule" { ContactShape::Capsule { radius, height } } else { ContactShape::Sphere { radius } },
            position: Vec3::new(-p[0], p[1], p[2]),
            rotation: Quat::from_xyzw(q[0], -q[1], -q[2], q[3]),
            tags: s["tags"].as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_string()).collect(),
            owner: Toucher::Other,
        });
        paths.push(s["transform"].as_str().unwrap().to_string());
    }
    let mut poses = BTreeMap::<usize, Vec<(usize, Transform)>>::new();
    if !paths.is_empty() {
        for line in text(&dir.join("poses.tsv")).lines() {
            let r: Vec<&str> = line.split('\t').collect();
            if r[1] != "root" {
                continue;
            }
            if let Some(k) = paths.iter().position(|p| p == r[2]) {
                let v: Vec<f32> = r[3].split(',').map(|x| x.parse().unwrap()).collect();
                poses.entry(r[0].parse().unwrap()).or_default().push((k, unity(&v)));
            }
        }
    }
    (senders, poses)
}

/// Each receiver parameter the recording lists, by frame as the reference's dynamics left it; a receiver whose
/// parameter no animator declares drives nothing the recording can show.
fn receiver_parameters(dir: &Path, names: &[String]) -> BTreeMap<String, BTreeMap<usize, f32>> {
    let mut out: BTreeMap<String, BTreeMap<usize, f32>> = names.iter().map(|n| (n.clone(), BTreeMap::new())).collect();
    for line in text(&dir.join("params.tsv")).lines() {
        let r: Vec<&str> = line.split('\t').collect();
        if let Some(log) = out.get_mut(r[1]) {
            let v = match r[2] {
                "True" => 1.0,
                "False" => 0.0,
                v => v.parse().unwrap_or(0.0),
            };
            log.insert(r[0].parse().unwrap(), v);
        }
    }
    out.retain(|_, log| !log.is_empty());
    out
}

/// One receiver parameter against the reference.
#[derive(Clone, Debug, Default)]
pub struct Receiver {
    pub frames: usize,
    /// Frames whose value differs from the reference by more than [`PARAMETER_TOLERANCE`], and the first of them.
    pub wrong: usize,
    pub first_wrong: Option<(usize, f32, f32)>,
    pub worst: f32,
}

/// A proximity value is a float the reference writes rounded to single precision.
pub const PARAMETER_TOLERANCE: f32 = 1e-4;

pub struct Report {
    pub targets: BTreeMap<String, Error>,
    pub receivers: BTreeMap<String, Receiver>,
}

fn world_of(root: Transform, locals: &[Transform], parent: &[Option<usize>], node: usize) -> Transform {
    let mut chain = vec![node];
    while let Some(p) = parent[*chain.last().unwrap()] {
        chain.push(p);
    }
    let mut t = root;
    for n in chain.into_iter().rev() {
        t = t.mul(&locals[n]);
    }
    t
}

/// Each transform a constraint or contact reads that the SDK's PhysBones move, in the avatar's root space as
/// `moved.tsv` holds it after the SDK's dynamics, by frame when it changes; a constraint target among them is
/// left out, as the core solves it.
fn moved(dir: &Path, replay: &Replay) -> BTreeMap<usize, Vec<(usize, Transform)>> {
    let targets: Vec<usize> = replay.avatar.model.constraints.iter().map(|c| c.target.0 as usize).collect();
    let index: BTreeMap<&str, usize> = replay.names.iter().enumerate().filter_map(|(i, n)| n.as_deref().map(|n| (n, i))).collect();
    let mut out = BTreeMap::<usize, Vec<(usize, Transform)>>::new();
    for line in text(&dir.join("moved.tsv")).lines() {
        let r: Vec<&str> = line.split('\t').collect();
        let node = *index.get(canonical(r[2]).as_str()).unwrap_or_else(|| panic!("moved.tsv: no node {}", r[2]));
        if r[1] != "root" || targets.contains(&node) {
            continue;
        }
        let v: Vec<f32> = r[3].split(',').map(|x| x.parse().unwrap()).collect();
        out.entry(r[0].parse().unwrap()).or_default().push((node, unity(&v)));
    }
    out
}

/// Places each node at its pose in the avatar's root space, parents first.
fn pin(scene: &mut Scene, parent: &[Option<usize>], poses: &[(usize, Transform)]) {
    for (n, t) in poses {
        scene.locals[*n] = match parent[*n] {
            Some(p) => relative(&world_of(Transform::IDENTITY, scene.locals, parent, p), t),
            None => *t,
        };
    }
}

fn depth(parent: &[Option<usize>], mut node: usize) -> usize {
    let mut d = 0;
    while let Some(p) = parent[node] {
        d += 1;
        node = p;
    }
    d
}

/// Replays the recording's constraints and contacts frame by frame and scores each constrained target in the
/// avatar's root space and each receiver parameter. What the core's constraints after PhysBones and its contacts
/// read where PhysBones put it is placed where the SDK's PhysBones left it, so their error does not enter the
/// score: before those constraints and again after them, as one can turn a bone above a PhysBone and `moved.tsv`
/// holds the poses after all of the SDK's dynamics.
pub fn score(dir: &Path, replay: &Replay) -> Report {
    let mut reference = BTreeMap::<usize, Vec<(String, Transform)>>::new();
    for line in text(&dir.join("constrained.tsv")).lines() {
        let r: Vec<&str> = line.split('\t').collect();
        if r[1] != "root" {
            continue;
        }
        let v: Vec<f32> = r[3].split(',').map(|x| x.parse().unwrap()).collect();
        reference.entry(r[0].parse().unwrap()).or_default().push((canonical(r[2]), unity(&v)));
    }
    let fields = fields(dir, replay);
    let mut constraints = Constraints::new(&replay.avatar).unwrap();
    let mut contacts = Contacts::new(&replay.avatar).unwrap();
    let (mut outside, outside_poses) = outside(dir);
    let mut outside_at = vec![Transform::IDENTITY; outside.len()];
    let moved_poses = moved(dir, replay);
    let mut moved_at = BTreeMap::<usize, Transform>::new();
    let m = &replay.avatar.model;
    let parameter_names: Vec<String> = m.contacts.receivers.iter().map(|r| r.parameter.clone()).filter(|p| !p.is_empty()).collect();
    let expected = receiver_parameters(dir, &parameter_names);
    let mut receivers = BTreeMap::<String, Receiver>::new();
    for r in &m.contacts.receivers {
        if expected.contains_key(&r.parameter) {
            receivers.entry(r.parameter.clone()).or_default();
        }
    }
    let mut last = BTreeMap::<String, Transform>::new();
    let mut errors = BTreeMap::<String, Error>::new();
    let parent = &replay.nodes.parent;
    let root_node = (0..parent.len()).find(|&n| parent[n].is_none()).unwrap();
    for (k, frame) in replay.frames.iter().enumerate() {
        let f = replay.first + k;
        for (component, i, field, v) in fields.get(&f).into_iter().flatten() {
            match component {
                Component::Constraint => constraints.set(*i, field, *v),
                Component::Sender => contacts.set_sender(*i, field, *v),
                Component::Receiver => contacts.set_receiver(*i, field, *v),
            }
        }
        for (i, t) in outside_poses.get(&f).into_iter().flatten() {
            outside_at[*i] = *t;
        }
        let mut locals: Vec<Transform> = (0..frame.nodes.len())
            .map(|n| match parent[n] {
                Some(p) => relative(&frame.nodes[p], &frame.nodes[n]),
                None => frame.nodes[n],
            })
            .collect();
        let root = Transform { scale: Vec3::ONE, ..frame.root };
        let mut scene = Scene { root, locals: &mut locals, active: &frame.active };
        constraints.solve_before_physbones(&mut scene);
        for (n, t) in moved_poses.get(&f).into_iter().flatten() {
            moved_at.insert(*n, *t);
        }
        let mut pinned: Vec<(usize, Transform)> = moved_at.iter().map(|(n, t)| (*n, *t)).collect();
        pinned.sort_by_key(|(n, _)| depth(parent, *n));
        pin(&mut scene, parent, &pinned);
        constraints.solve_after_physbones(&mut scene);
        pin(&mut scene, parent, &pinned);
        let world: Vec<Transform> = (0..parent.len()).map(|n| world_of(root, &locals, parent, n)).collect();
        for (s, at) in outside.iter_mut().zip(&outside_at) {
            s.at = root.mul(at);
        }
        contacts.advance(if k == 0 { 0.0 } else { 1.0 / 60.0 }, &world, &frame.active, &outside, true);
        let mut values = BTreeMap::<&str, f32>::new();
        for (r, v) in contacts.receivers().iter().zip(contacts.values()) {
            if let (false, Some(v)) = (r.parameter.is_empty(), v) {
                let e = values.entry(r.parameter.as_str()).or_default();
                *e = e.max(*v);
            }
        }
        for (name, v) in values {
            let Some(log) = expected.get(name) else { continue };
            let want = super::at(log, f);
            let e = receivers.get_mut(name).unwrap();
            let d = (v - want).abs();
            e.frames += 1;
            e.worst = e.worst.max(d);
            if d > PARAMETER_TOLERANCE {
                e.wrong += 1;
                e.first_wrong.get_or_insert((f, v, want));
            }
        }
        for (path, t) in reference.get(&f).into_iter().flatten() {
            last.insert(path.clone(), *t);
        }
        for c in constraints.model() {
            let target = c.target.0 as usize;
            let path = replay.names[target].clone().unwrap();
            let Some(want) = last.get(&path) else { continue };
            let space = relative(&locals[root_node], &world_of(Transform::IDENTITY, &locals, parent, target));
            let e = errors.entry(path.clone()).or_default();
            let ds = (space.scale - want.scale).abs().max_element();
            let dp = (space.translation - want.translation).length().max(ds);
            let d = space.rotation * want.rotation.inverse();
            let dq = 2.0 * Vec3::new(d.x, d.y, d.z).length().min(1.0).asin().to_degrees();
            e.frames += 1;
            if dp > e.position.0 {
                e.position = (dp, f);
            }
            if dq > e.rotation.0 {
                e.rotation = (dq, f);
            }
        }
    }
    Report { targets: errors, receivers }
}
