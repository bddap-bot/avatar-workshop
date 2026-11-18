//! Replays a reference recording's constraints and contacts through the core and scores the constrained targets
//! against `constrained.tsv` and the receivers' parameters against `params.tsv`.

use std::collections::BTreeMap;
use std::path::Path;

use avatar_core::constraints::{Constraints, Scene};
use avatar_core::contacts::{Contacts, Sender};
use avatar_core::model::{ConstraintKind, ContactShape};
use avatar_core::physbones::{PhysBones, Toucher, Transform};
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
    /// Frames of a constraint that runs after PhysBones and whose output depends on where they put a bone.
    pub unmeasured: usize,
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
    /// Whether a sender or receiver it depends on sits where PhysBones put it, which the recording does not hold.
    pub unmeasured: bool,
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

/// The nodes whose pose after the SDK's dynamics depends on where PhysBones put a bone, which no recording holds:
/// each bone a PhysBone moves, the target of each constraint that reads one (a scale constraint's aside, which
/// sets scale alone), and every node under them.
fn unmeasured_nodes(replay: &Replay, constraints: &Constraints) -> Vec<bool> {
    let physbones = PhysBones::new(&replay.avatar).unwrap();
    let count = replay.nodes.parent.len();
    let mut moved = vec![false; count];
    for c in 0..replay.avatar.model.physbones.len() {
        for n in physbones.bones(c) {
            moved[n] = true;
        }
    }
    for (i, c) in constraints.model().iter().enumerate() {
        if constraints.after_physbones(i) && !matches!(c.kind, ConstraintKind::Scale { .. }) {
            moved[c.target.0 as usize] = true;
        }
    }
    (0..count)
        .map(|n| {
            let mut at = Some(n);
            while let Some(x) = at {
                if moved[x] {
                    return true;
                }
                at = replay.nodes.parent[x];
            }
            false
        })
        .collect()
}

/// Replays the recording's constraints and contacts frame by frame and scores each constrained target in the
/// avatar's root space and each receiver parameter. The recording holds no pose the SDK's PhysBones produce, so a
/// constraint that runs after them is solved on the bones' poses before them and scored on what does not depend
/// on those poses, a scale constraint's scale; the rest are counted in [`Error::unmeasured`], and a receiver
/// parameter a sender or receiver on such a node can change is marked [`Receiver::unmeasured`].
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
    let unmeasured = unmeasured_nodes(replay, &constraints);
    let m = &replay.avatar.model;
    let parameter_names: Vec<String> = m.contacts.receivers.iter().map(|r| r.parameter.clone()).filter(|p| !p.is_empty()).collect();
    let expected = receiver_parameters(dir, &parameter_names);
    let mut receivers = BTreeMap::<String, Receiver>::new();
    for r in &m.contacts.receivers {
        if !expected.contains_key(&r.parameter) {
            continue;
        }
        let e = receivers.entry(r.parameter.clone()).or_default();
        e.unmeasured |= unmeasured[r.node.0 as usize]
            || m.contacts.senders.iter().any(|s| unmeasured[s.node.0 as usize] && s.collision_tags.iter().any(|t| r.collision_tags.contains(t)) && r.allow_self);
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
        constraints.solve_after_physbones(&mut scene);
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
        for (i, c) in constraints.model().iter().enumerate() {
            let target = c.target.0 as usize;
            let path = replay.names[target].clone().unwrap();
            let Some(want) = last.get(&path) else { continue };
            let space = relative(&locals[root_node], &world_of(Transform::IDENTITY, &locals, parent, target));
            let e = errors.entry(path.clone()).or_default();
            let scale_only = constraints.after_physbones(i);
            if scale_only && !matches!(c.kind, ConstraintKind::Scale { .. }) {
                e.unmeasured += 1;
                continue;
            }
            let ds = (space.scale - want.scale).abs().max_element();
            let dp = if scale_only { ds } else { (space.translation - want.translation).length().max(ds) };
            let d = space.rotation * want.rotation.inverse();
            let dq = if scale_only { 0.0 } else { 2.0 * Vec3::new(d.x, d.y, d.z).length().min(1.0).asin().to_degrees() };
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
