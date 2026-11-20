//! Replays a reference recording (`reference/` in the repository root) through the core's PhysBones, and scores
//! the result as `reference/compare.py` does. Shared by the `physbones` example and the tests.
#![allow(dead_code)]

pub mod animator;
pub mod dynamics;
pub mod run;
mod names;
pub use names::{args, names};

use std::collections::BTreeMap;
use std::io::Read;
use std::ops::Range;
use std::path::Path;

use avatar_core::contacts::Sender;
use avatar_core::evaluate::{Animated, Evaluator, Field, Input, Output};
use avatar_core::physbones::{Frame, Nodes, Toucher, Transform};
use avatar_core::Avatar;
use glam::{Quat, Vec3};

/// A recording file, or its gzipped copy beside it.
pub fn text(path: &Path) -> String {
    if let Ok(s) = std::fs::read_to_string(path) {
        return s;
    }
    let gz = path.with_extension("tsv.gz");
    let file = std::fs::File::open(&gz).unwrap_or_else(|e| panic!("{}: {e}", gz.display()));
    let mut s = String::new();
    flate2::read::GzDecoder::new(file).read_to_string(&mut s).unwrap();
    s
}

fn rows(path: &Path) -> Vec<Vec<String>> {
    text(path).lines().map(|l| l.split('\t').map(str::to_string).collect()).collect()
}

fn floats(s: &str) -> Vec<f32> {
    s.split(',').map(|x| x.parse().unwrap()).collect()
}

/// Unity's left-handed values to glTF's: mirror X.
pub fn unity(v: &[f32]) -> Transform {
    Transform {
        translation: Vec3::new(-v[0], v[1], v[2]),
        rotation: Quat::from_xyzw(v[3], -v[4], -v[5], v[6]).normalize(),
        scale: if v.len() >= 10 { Vec3::new(v[7], v[8], v[9]) } else { Vec3::ONE },
    }
}

/// A path with VRCFury's per-build object numbers (`[VF12]`) dropped, so an export and a recording of different
/// builds name the same objects alike.
pub fn canonical(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(at) = rest.find("[VF") {
        let digits = rest[at + 3..].bytes().take_while(u8::is_ascii_digit).count();
        let close = at + 3 + digits;
        out.push_str(&rest[..at]);
        if digits > 0 && rest[close..].starts_with(']') {
            out.push_str("[VF]");
            rest = &rest[close + 1..];
        } else {
            out.push_str("[VF");
            rest = &rest[at + 3..];
        }
    }
    out.push_str(rest);
    out
}

/// Each node's path from the avatar root as the harness names it, `#n` marking the n-th same-named sibling.
pub fn paths(avatar: &Avatar, nodes: &Nodes) -> Vec<Option<String>> {
    let scene = avatar.gltf.get("scene").and_then(|s| s.as_u64()).unwrap_or(0) as usize;
    let root = avatar.gltf["scenes"][scene]["nodes"][0].as_u64().unwrap() as usize;
    let name = |i: usize| avatar.gltf["nodes"][i]["name"].as_str().unwrap_or("").to_string();
    let mut out = vec![None; nodes.children.len()];
    out[root] = Some(String::new());
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut seen = BTreeMap::<String, usize>::new();
        for &c in &nodes.children[n] {
            let base = name(c);
            let twin = seen.entry(base.clone()).or_default();
            let own = if *twin == 0 { base.clone() } else { format!("{base}#{twin}") };
            *twin += 1;
            let prefix = out[n].clone().unwrap();
            out[c] = Some(if prefix.is_empty() { own } else { format!("{prefix}/{own}") });
            stack.push(c);
        }
    }
    out.into_iter().map(|p| p.map(|p| canonical(&p))).collect()
}

pub struct Grab {
    pub node: usize,
    pub event: String,
    pub at: Option<Vec3>,
}

pub struct Replay {
    pub avatar: Avatar,
    pub nodes: Nodes,
    pub names: Vec<Option<String>>,
    /// The first recorded frame replayed.
    pub first: usize,
    /// What the host gives the core each recorded frame: the root, every node's root-space pose (those the
    /// recording does not hold at rest), its animated local pose, and the active and PhysBone enabled flags.
    pub frames: Vec<Frame>,
    /// The nodes whose root-space pose the recording holds, and the root node, which it holds at the origin
    /// unscaled: its poses are metres from the root whatever its scale.
    pub posed: Vec<usize>,
    /// Each constraint and contact field a curve drives, by frame when it changes, signed as the model holds it.
    pub fields: BTreeMap<usize, Vec<(Field, f32)>>,
    pub grabs: BTreeMap<usize, Grab>,
}

/// Per host frame (a fractional recording frame at other rates): tips in metres from the root along Unity's
/// axes, as `tips.tsv` holds them, and each PhysBone's `{parameter}_…` values.
pub struct Trace {
    pub frames: Vec<f64>,
    pub tips: Vec<BTreeMap<String, Vec3>>,
    pub parameters: Vec<BTreeMap<String, f32>>,
}

impl Replay {
    /// The recording's frames in `frames`, the state before them carried in from the change logs.
    pub fn load(dir: &Path, avatar: Avatar, frames: Range<usize>) -> Replay {
        let nodes = Nodes::read(&avatar).unwrap();
        let names = paths(&avatar, &nodes);
        let index: BTreeMap<&str, usize> = names.iter().enumerate().filter_map(|(i, n)| n.as_deref().map(|n| (n, i))).collect();
        let count = nodes.children.len();
        let root = names.iter().position(|n| n.as_deref() == Some("")).unwrap();
        let mut space = vec![Transform::IDENTITY; count];
        let mut stack = vec![(root, Transform::IDENTITY)];
        while let Some((n, parent)) = stack.pop() {
            space[n] = if n == root { Transform::IDENTITY } else { parent.mul(&nodes.rest[n]) };
            stack.extend(nodes.children[n].iter().map(|&c| (c, space[n])));
        }
        let mut locals = nodes.rest.clone();
        let mut posed = std::collections::BTreeSet::from([root]);
        let mut active = vec![true; count];
        let mut enabled = vec![true; avatar.model.physbones.len()];
        let physbones: BTreeMap<String, usize> = avatar
            .model
            .physbones
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let k = avatar.model.physbones[..i].iter().filter(|o| o.node == p.node).count();
                (format!("physbone:{}#{k}", names[p.node.0 as usize].as_deref().unwrap_or("")), i)
            })
            .collect();
        let mut changes = BTreeMap::<usize, Vec<Vec<String>>>::new();
        for file in ["poses.tsv", "values.tsv"] {
            for line in text(&dir.join(file)).lines() {
                let r: Vec<String> = line.splitn(4, '\t').map(str::to_string).collect();
                let f: usize = r[0].parse().unwrap();
                if f >= frames.end {
                    break;
                }
                changes.entry(f).or_default().push(r);
            }
        }
        let roots: BTreeMap<usize, Transform> = rows(&dir.join("track.tsv"))[1..]
            .iter()
            .map(|r| (r[0].parse().unwrap(), unity(&floats(&r[1]))))
            .filter(|(f, _)| frames.contains(f))
            .collect();
        let start = changes.keys().next().copied().unwrap_or(0).min(frames.start);
        let end = roots.keys().next_back().map_or(start, |f| f + 1);
        let mut out = vec![];
        for f in start..end {
            for r in changes.get(&f).into_iter().flatten() {
                match (r[1].as_str(), r.len()) {
                    ("root", 4) | ("local", 4) => {
                        if let Some(&n) = index.get(canonical(&r[2]).as_str()) {
                            let t = unity(&floats(&r[3]));
                            if r[1] == "root" {
                                space[n] = t;
                                posed.insert(n);
                            } else {
                                locals[n] = t
                            }
                        }
                    }
                    (key, 3) => {
                        if let Some(path) = key.strip_prefix("active:") {
                            if let Some(&n) = index.get(canonical(path).as_str()) {
                                active[n] = r[2] == "1";
                            }
                        } else if let Some(&i) = physbones.get(&canonical(key)) {
                            enabled[i] = r[2] == "1";
                        }
                    }
                    _ => {}
                }
            }
            if let Some(&root) = roots.get(&f) {
                out.push((f, Frame { root, nodes: space.clone(), locals: locals.clone(), active: active.clone(), enabled: enabled.clone() }));
            }
        }
        let first = out.first().map_or(frames.start, |(f, _)| *f);
        let mut grabs = BTreeMap::new();
        if dir.join("grabs.tsv").exists() || dir.join("grabs.tsv.gz").exists() {
            for r in rows(&dir.join("grabs.tsv")) {
                let f: usize = r[0].parse().unwrap();
                if !frames.contains(&f) {
                    continue;
                }
                let node = names.iter().position(|n| n.as_deref().is_some_and(|n| n.ends_with(&format!("/{}", r[2])))).unwrap();
                let at = (r[3] != "-").then(|| unity(&[r[3].parse().unwrap(), r[4].parse().unwrap(), r[5].parse().unwrap(), 0.0, 0.0, 0.0, 1.0]).translation);
                grabs.insert(f, Grab { node, event: r[1].clone(), at });
            }
        }
        let mut replay = Replay {
            avatar,
            nodes,
            names,
            first,
            frames: out.into_iter().map(|(_, fr)| fr).collect(),
            posed: posed.into_iter().collect(),
            fields: BTreeMap::new(),
            grabs,
        };
        replay.fields = fields(dir, &replay, frames.end);
        replay
    }

    /// Steps the core at `rate` Hz on the recording's input, interpolated between recorded frames, replaying the
    /// recorded grabs as the wearer's and the recorded constraint and contact fields. `held` gives, by recorded frame,
    /// root-space poses the core holds after PhysBones (each kept until it changes), and `senders` the senders outside
    /// the avatar on a recorded frame. The host poses `posed` (parents first) where the frame places them. `each` sees
    /// every host frame (a fractional recording frame at other rates).
    pub fn drive(
        &self,
        rate: f64,
        posed: &[usize],
        held: &BTreeMap<usize, Vec<(usize, Transform)>>,
        senders: &mut dyn FnMut(usize, &Transform) -> Vec<Sender>,
        each: &mut dyn FnMut(f64, &Evaluator),
    ) {
        let mut ev = Evaluator::new(&self.avatar).unwrap();
        let mut fields: Vec<(Field, f32)> = vec![];
        let change = |fields: &mut Vec<(Field, f32)>, changes: &[(Field, f32)]| {
            for (f, v) in changes {
                match fields.iter_mut().find(|(g, _)| g == f) {
                    Some((_, w)) => *w = *v,
                    None => fields.push((f.clone(), *v)),
                }
            }
        };
        for (_, changes) in self.fields.range(..self.first) {
            change(&mut fields, changes);
        }
        let span = (self.frames.len() - 1) as f64 / 60.0;
        let (mut starts, mut firsts) = (BTreeMap::new(), BTreeMap::new());
        let physbones = &self.avatar.model.physbones;
        let mut holding = BTreeMap::<usize, Transform>::new();
        for (_, poses) in held.range(..self.first) {
            holding.extend(poses.iter().copied());
        }
        let mut outside = vec![];
        for k in 0.. {
            let t = k as f64 / rate;
            if t > span + 1e-9 {
                break;
            }
            let x = t * 60.0;
            let i = (x.floor() as usize).min(self.frames.len() - 1);
            let whole = (x - x.round()).abs() < 1e-6;
            let at = self.first + x.round() as usize;
            let frame = match (whole, i + 1 < self.frames.len()) {
                (true, _) => self.frames[(at - self.first).min(self.frames.len() - 1)].clone(),
                (false, true) => blend(&self.frames[i], &self.frames[i + 1], (x - i as f64) as f32),
                (false, false) => self.frames[i].clone(),
            };
            if whole {
                change(&mut fields, self.fields.get(&at).map_or(&[][..], Vec::as_slice));
                holding.extend(held.get(&at).into_iter().flatten().copied());
                outside = senders(at, &frame.root);
            }
            if let Some(g) = self.grabs.get(&at).filter(|_| whole) {
                let chain = (0..physbones.len()).find(|&c| ev.physbones().bones(c).any(|n| n == g.node)).unwrap();
                let world = g.at.map(|a| frame.root.point(a));
                match g.event.as_str() {
                    "grab" | "miss" => {
                        let held = ev.grab(chain, g.node, world.unwrap(), Toucher::Wearer);
                        assert_eq!(held, g.event == "grab", "frame {}: the reference's grab was a {}", self.first as f64 + x, g.event);
                        starts.insert(chain, world.unwrap());
                        firsts.remove(&chain);
                    }
                    "hold" => {
                        let first = *firsts.entry(chain).or_insert(world.unwrap());
                        ev.hold(chain, world.unwrap() - first + starts[&chain]);
                    }
                    "release" => ev.release(chain, false, Toucher::Wearer),
                    _ => ev.release(chain, true, Toucher::Wearer),
                }
            }
            let poses: Vec<(usize, Transform)> = posed.iter().map(|&n| (n, frame.nodes[n])).collect();
            let pinned: Vec<(usize, Transform)> = holding.iter().map(|(n, t)| (*n, *t)).collect();
            let input = Input {
                dt: if k == 0 { 0.0 } else { (1.0 / rate) as f32 },
                root: frame.root,
                poses: &poses,
                held: &pinned,
                senders: &outside,
            };
            ev.evaluate_animated(&input, &Animated { locals: &frame.locals, active: &frame.active, physbones: &frame.enabled, fields: &fields });
            each(self.first as f64 + x, &ev);
        }
    }

    /// The PhysBone tips and parameters at `rate` Hz.
    pub fn run(&self, rate: f64) -> Trace {
        let mut trace = Trace { frames: vec![], tips: vec![], parameters: vec![] };
        self.drive(rate, &self.posed, &BTreeMap::new(), &mut |_, _| vec![], &mut |f, ev| {
            trace.frames.push(f);
            trace.tips.push(self.tips(ev, ev.output()));
            trace.parameters.push(physbone_parameters(&self.avatar, ev));
        });
        trace
    }

    pub fn tips(&self, ev: &Evaluator, out: &Output) -> BTreeMap<String, Vec3> {
        tips(&self.avatar, &self.nodes, &self.names, ev, out)
    }
}

/// Every simulated bone below each root, and a root with no simulated children, with the endpoint on leaves, in
/// metres from the root along Unity's axes.
pub fn tips(avatar: &Avatar, nodes: &Nodes, names: &[Option<String>], ev: &Evaluator, out: &Output) -> BTreeMap<String, Vec3> {
    let pb = ev.physbones();
    let mut tips = BTreeMap::new();
    for (c, model) in avatar.model.physbones.iter().enumerate() {
        let bones: Vec<usize> = pb.bones(c).collect();
        let endpoint = Vec3::from(model.endpoint_position);
        let root = names[model.root.0 as usize].clone().unwrap();
        for (k, n) in bones.iter().enumerate() {
            let leaf = !nodes.children[*n].iter().any(|c| bones.contains(c));
            if k == 0 && !leaf {
                continue;
            }
            let s = out.space[*n];
            let p = if leaf && endpoint != Vec3::ZERO { s.point(endpoint) } else { s.translation };
            tips.insert(format!("{root}|{}", names[*n].clone().unwrap()), Vec3::new(-p.x, p.y, p.z));
        }
    }
    tips
}

/// Each PhysBone's `{parameter}_…` values.
pub fn physbone_parameters(avatar: &Avatar, ev: &Evaluator) -> BTreeMap<String, f32> {
    let mut parameters = BTreeMap::new();
    for (c, model) in avatar.model.physbones.iter().enumerate() {
        let Some(name) = &model.parameter else { continue };
        let p = ev.physbones().parameters(c);
        for (suffix, value) in [("IsGrabbed", p.is_grabbed as u8 as f32), ("IsPosed", p.is_posed as u8 as f32), ("Angle", p.angle), ("Stretch", p.stretch), ("Squish", p.squish)] {
            parameters.insert(format!("{name}_{suffix}"), value);
        }
    }
    parameters
}

/// Each constraint and contact field a curve drives, by frame before `end`.
fn fields(dir: &Path, replay: &Replay, end: usize) -> BTreeMap<usize, Vec<(Field, f32)>> {
    let m = &replay.avatar.model;
    let index: BTreeMap<&str, usize> = replay.names.iter().enumerate().filter_map(|(i, n)| n.as_deref().map(|n| (n, i))).collect();
    let mut out = BTreeMap::<usize, Vec<_>>::new();
    for line in text(&dir.join("values.tsv")).lines() {
        let r: Vec<&str> = line.split('\t').collect();
        let f: usize = r[0].parse().unwrap();
        if f >= end {
            break;
        }
        let Some((kind, rest)) = r[1].split_once(':') else { continue };
        let nodes: Vec<u32> = match kind {
            "constraint" => m.constraints.iter().map(|c| c.node.0).collect(),
            "sender" => m.contacts.senders.iter().map(|c| c.node.0).collect(),
            "receiver" => m.contacts.receivers.iter().map(|c| c.node.0).collect(),
            _ => continue,
        };
        let Some((rest, field)) = rest.rsplit_once(':') else { continue };
        let Some((path, k)) = rest.rsplit_once('#') else { continue };
        let Some(&node) = index.get(canonical(path).as_str()) else { continue };
        let k: usize = k.parse().unwrap();
        let Some(index) = nodes.iter().enumerate().filter(|(_, n)| **n as usize == node).nth(k).map(|(i, _)| i) else { continue };
        let v: f32 = r[2].parse().unwrap();
        let field = match kind {
            "constraint" => Field::Constraint(index, field.to_string()),
            "sender" => Field::Sender(index, field.to_string()),
            _ => Field::Receiver(index, field.to_string()),
        };
        out.entry(f).or_default().push((field, v));
    }
    out
}

fn blend(a: &Frame, b: &Frame, s: f32) -> Frame {
    let mix = |x: &Transform, y: &Transform| Transform {
        translation: x.translation.lerp(y.translation, s),
        rotation: x.rotation.slerp(y.rotation, s),
        scale: x.scale.lerp(y.scale, s),
    };
    Frame {
        root: mix(&a.root, &b.root),
        nodes: a.nodes.iter().zip(&b.nodes).map(|(x, y)| mix(x, y)).collect(),
        locals: a.locals.iter().zip(&b.locals).map(|(x, y)| mix(x, y)).collect(),
        active: b.active.clone(),
        enabled: b.enabled.clone(),
    }
}

/// The first frame of the step labelled `label`.
pub fn step(dir: &Path, label: &str) -> usize {
    rows(&dir.join("steps.tsv")).iter().find(|r| r[4] == label).map(|r| r[1].parse().unwrap()).unwrap()
}

/// The scored window: from the motion step's start to the settle step's end.
pub fn window(dir: &Path) -> Range<usize> {
    let steps = rows(&dir.join("steps.tsv"));
    let step = |label: &str| steps.iter().find(|r| r[4] == label).map(|r| (r[1].parse::<usize>().unwrap(), r[2].parse::<usize>().unwrap())).unwrap();
    let (motion, _) = step("motion");
    let (settle, frames) = step("settle");
    motion..settle + frames
}

/// Each tip's RMS error over its rest chain length in `window`, a tip the replay lacks counting as unmoving, as
/// `compare.py` scores it; sorted by error.
pub fn errors(dir: &Path, trace: &Trace, window: Range<usize>) -> Vec<(f32, String)> {
    let lengths: BTreeMap<String, f32> = rows(&dir.join("chains.tsv"))[1..].iter().map(|r| (canonical(&r[0]), r[1].parse().unwrap())).collect();
    let mut reference = BTreeMap::<String, BTreeMap<usize, Vec3>>::new();
    for line in text(&dir.join("tips.tsv")).lines() {
        let r: Vec<&str> = line.split('\t').collect();
        let f: usize = r[0].parse().unwrap();
        if f >= window.end {
            break;
        }
        if window.contains(&f) {
            reference.entry(canonical(r[1])).or_default().insert(f, Vec3::new(r[2].parse().unwrap(), r[3].parse().unwrap(), r[4].parse().unwrap()));
        }
    }
    let ours: BTreeMap<usize, &BTreeMap<String, Vec3>> =
        trace.frames.iter().zip(&trace.tips).filter(|(f, _)| (*f - f.round()).abs() < 1e-6).map(|(f, t)| (f.round() as usize, t)).collect();
    let mut errors = vec![];
    for (tip, length) in &lengths {
        let Some(r) = reference.get(tip).filter(|r| !r.is_empty() && *length > 0.0) else { continue };
        let rest = *r.values().next().unwrap();
        let sum: f64 = r.iter().map(|(f, p)| (ours.get(f).and_then(|t| t.get(tip)).copied().unwrap_or(rest) - *p).length_squared() as f64).sum();
        errors.push(((sum / r.len() as f64).sqrt() as f32 / length, tip.clone()));
    }
    errors.sort_by(|a, b| a.0.total_cmp(&b.0));
    errors
}

pub fn median(errors: &[(f32, String)]) -> f32 {
    let n = errors.len();
    if n % 2 == 1 { errors[n / 2].0 } else { (errors[n / 2 - 1].0 + errors[n / 2].0) / 2.0 }
}

/// Each animator parameter the recording logs, as a value per frame from its first change.
pub fn parameters(dir: &Path) -> BTreeMap<String, BTreeMap<usize, f32>> {
    let mut out = BTreeMap::<String, BTreeMap<usize, f32>>::new();
    for r in rows(&dir.join("params.tsv")) {
        let value = match r[2].as_str() {
            "True" => 1.0,
            "False" => 0.0,
            v => v.parse().unwrap_or(0.0),
        };
        out.entry(r[1].clone()).or_default().insert(r[0].parse().unwrap(), value);
    }
    out
}

/// The value a change log holds at `frame`.
pub fn at(log: &BTreeMap<usize, f32>, frame: usize) -> f32 {
    log.range(..=frame).next_back().map_or(0.0, |(_, v)| *v)
}
