//! Replays a reference recording's inputs through the core's animator and compares its parameters, states and
//! node, blendshape and material-property values with the recording's, frame by frame.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use avatar_core::animator::motion::Target;
use avatar_core::animator::{Animator, BUILT_INS};
use avatar_core::model::Axis;
use avatar_core::physbones::{Nodes, Transform};
use glam::{Quat, Vec3};
use avatar_core::Avatar;

use super::{canonical, paths, text};

/// The emulator's playable-graph time after each frame's evaluation, from the frame the graph exists.
fn graph_times(dir: &Path) -> BTreeMap<usize, f64> {
    text(&dir.join("time.tsv"))
        .lines()
        .map(|l| {
            let (f, t) = l.split_once('\t').unwrap();
            (f.parse().unwrap(), t.parse().unwrap())
        })
        .collect()
}

struct Clocks {
    columns: HashMap<String, usize>,
    frames: BTreeMap<usize, Vec<(f32, Option<f32>)>>,
}

fn clocks(dir: &Path) -> Clocks {
    let text = text(&dir.join("clocks.tsv"));
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().expect("clocks.tsv has no header").split('\t').skip(1).collect();
    let columns = header.iter().enumerate().map(|(i, k)| (k.to_string(), i)).collect();
    let mut frames = BTreeMap::new();
    for line in lines {
        let mut f = line.split('\t');
        let frame: usize = f.next().unwrap().parse().unwrap();
        let cells: Vec<(f32, Option<f32>)> = f
            .map(|c| match c.split_once('>') {
                Some((cur, next)) => (cur.parse().unwrap(), Some(next.parse().unwrap())),
                None => (c.parse().unwrap(), None),
            })
            .collect();
        assert_eq!(cells.len(), header.len(), "frame {frame} of clocks.tsv holds {} of {} layers", cells.len(), header.len());
        frames.insert(frame, cells);
    }
    Clocks { columns, frames }
}

const BOOLEAN_FIELDS: &[&str] = &["m_Enabled", "IsActive", "Locked", "FreezeToWorld", "SolveInLocalSpace", "RebakeOffsetsWhenUnfrozen", "allowSelf", "allowOthers", "localOnly", "UseUpTransform", "m_Active"];

/// A change log: key → frame → value as recorded.
type Log = BTreeMap<String, BTreeMap<usize, String>>;

fn log(dir: &Path, file: &str) -> Log {
    let mut out: Log = BTreeMap::new();
    let text = text(&dir.join(file));
    let mut records: Vec<String> = Vec::new();
    for line in text.lines() {
        let numbered = line.split_once('\t').is_some_and(|(f, _)| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()));
        match records.last_mut() {
            Some(last) if !numbered => {
                last.push('\n');
                last.push_str(line);
            }
            _ => records.push(line.to_string()),
        }
    }
    for line in &records {
        let mut f = line.splitn(3, '\t');
        let (Some(frame), Some(key), Some(value)) = (f.next(), f.next(), f.next()) else { continue };
        let Ok(frame) = frame.parse() else { continue };
        out.entry(canonical(key)).or_default().insert(frame, value.to_string());
    }
    out
}

fn at(log: &BTreeMap<usize, String>, frame: usize) -> Option<&str> {
    log.range(..=frame).next_back().map(|(_, v)| v.as_str())
}

/// The value the playables hold for a parameter, each having converted it to the type its controller declares.
fn held_value(name: &str, frame: usize, values: &[f32]) -> f32 {
    let typed = |v: f32, w: f32| w == v || w == (v != 0.0) as u8 as f32 || w == (v as f64).round_ties_even() as f32;
    *values
        .iter()
        .find(|&&v| values.iter().all(|&w| typed(v, w)))
        .unwrap_or_else(|| panic!("the playables hold {name} at {values:?} at frame {frame}"))
}

/// Unity's `Animator.StringToHash`, which the harness prints for a state it cannot name.
fn crc32(s: &str) -> u32 {
    let mut c = !0u32;
    for b in s.bytes() {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// The head bone's pose each frame from `track.tsv`, in the avatar's space (Unity's mirrored on x).
fn heads(dir: &Path) -> HashMap<usize, Transform> {
    let mut out = HashMap::new();
    for line in text(&dir.join("track.tsv")).lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        let (Some(frame), Some(head)) = (f.first().and_then(|x| x.parse().ok()), f.get(2)) else { continue };
        let v: Vec<f32> = head.split(',').filter_map(|x| x.parse().ok()).collect();
        if v.len() != 7 {
            continue;
        }
        out.insert(
            frame,
            Transform { translation: Vec3::new(-v[0], v[1], v[2]), rotation: Quat::from_xyzw(v[3], -v[4], -v[5], v[6]), scale: Vec3::ONE },
        );
    }
    out
}

/// Whether the avatar emulator holds the eyes shut on a frame: at its default blink rate of 0.1 it blinks twice,
/// for ten frames thirty apart, every ⌊10 + 40 / 0.1²⌋ frames, 0.1² rounding up in single precision.
fn emulator_blink(frame: usize) -> bool {
    let rate = 0.1f32;
    let interval = (10.0 + 40.0 / (rate * rate)) as usize;
    let k = frame % interval;
    k < 60 && k % 30 < 10
}

pub struct Step {
    pub frame: usize,
    pub frames: usize,
    pub control: String,
    pub label: String,
}

pub fn steps(dir: &Path) -> Vec<Step> {
    text(&dir.join("steps.tsv"))
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 5 {
                return None;
            }
            Some(Step { frame: f[1].parse().unwrap(), frames: f[2].parse().unwrap(), control: f[3].to_string(), label: f[4].to_string() })
        })
        .collect()
}

fn unescaped(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars();
    while let Some(c) = chars.next() {
        let escaped = if c == '\\' { chars.next() } else { None };
        out.push(match escaped {
            Some('t') => '\t',
            Some('n') => '\n',
            Some('r') => '\r',
            Some(other) => other,
            None => c,
        });
    }
    out
}

/// The parameters the harness set on each frame, in the order it applied them.
pub fn inputs(dir: &Path) -> BTreeMap<usize, Vec<(String, f32)>> {
    text(&dir.join("inputs.tsv"))
        .lines()
        .map(|l| {
            let mut f = l.split('\t');
            let frame = f.next().unwrap().parse().unwrap();
            let set = f.skip(2).map(|kv| {
                let (k, v) = kv.rsplit_once('=').unwrap_or_else(|| panic!("frame {frame}'s input {kv} is not <name>=<value>"));
                (canonical(&unescaped(k)), v.parse().unwrap())
            });
            (frame, set.collect())
        })
        .collect()
}

/// A recorded value key and the core's target for it, with the component of a multi-value record.
#[derive(Clone, Debug)]
enum Key {
    Scalar(Target),
    Slot(Target),
    Vector(Vec<Option<Target>>),
    Transform(u32),
    Playable(usize),
    Weight(usize, usize),
    Tracking(usize),
    Locomotion,
    View,
    Curve(usize, String),
    Binding(Target),
}

#[derive(Default)]
pub struct Mismatches {
    pub compared: usize,
    /// Samples of the first recorded frame, whose animation read PhysBone and contact parameters from a frame
    /// the recording lacks.
    pub skipped: usize,
    pub wrong: usize,
    pub keys: BTreeMap<String, (usize, usize, String)>,
    pub frames: BTreeSet<usize>,
    /// Each wrong sample's frame and key.
    pub samples: BTreeSet<(usize, String)>,
}

impl Mismatches {
    fn note(&mut self, key: &str, frame: usize, detail: String) {
        if std::env::var("AW_TRACE").is_ok_and(|t| key.contains(&t)) {
            eprintln!("{frame}\t{key}\t{detail}");
        }
        self.wrong += 1;
        self.frames.insert(frame);
        self.samples.insert((frame, key.to_string()));
        let e = self.keys.entry(key.to_string()).or_insert((0, frame, detail));
        e.0 += 1;
    }

    pub fn summary(&self, limit: usize) -> String {
        let mut s = format!("{} of {} compared samples differ ({} first-frame samples not compared)", self.wrong, self.compared, self.skipped);
        let mut v: Vec<_> = self.keys.iter().collect();
        v.sort_by_key(|(_, (_, f, _))| *f);
        for (k, (n, f, d)) in v.into_iter().take(limit) {
            s.push_str(&format!("\n  {k}: {n} frames, first at {f}: {d}"));
        }
        s
    }
}

pub struct Report {
    /// Keys the replay compared with the reference, each with the distinct values the reference gave it.
    pub seen: BTreeMap<String, BTreeSet<String>>,
    pub frames: usize,
    pub parameters: Mismatches,
    pub states: Mismatches,
    pub values: Mismatches,
    pub worst_value: f32,
    pub clocks: Mismatches,
    pub clock_bits_differ: usize,
    pub worst_clock: f32,
    pub bindings: Mismatches,
    pub worst_binding: f32,
}

impl Report {
    /// Each menu control the recording swept and whether its effect matched the reference: every frame from its
    /// step to the next, on every parameter, state and value, within the tolerance.
    pub fn controls(&self, dir: &Path) -> Vec<(String, bool)> {
        let steps = steps(dir);
        let mut out = Vec::new();
        for (i, s) in steps.iter().enumerate() {
            if s.control == "-" || s.control.is_empty() || s.control.starts_with("builtin") {
                continue;
            }
            let end = steps.get(i + 1).map_or(usize::MAX, |n| n.frame);
            let bad = [&self.parameters, &self.states, &self.values].iter().any(|m| m.frames.range(s.frame..end).next().is_some());
            match out.iter_mut().find(|(c, _): &&mut (String, bool)| *c == s.control) {
                Some((_, ok)) => *ok &= !bad,
                None => out.push((s.control.clone(), !bad)),
            }
        }
        out
    }
}

pub struct Run {
    pub dir: std::path::PathBuf,
    pub avatar: Avatar,
    pub names: Vec<Option<String>>,
    /// Nodes PhysBones or constraints move after the animator, whose recorded pose is not the animator's.
    pub physics_nodes: BTreeSet<u32>,
}

fn axis(i: usize) -> Axis {
    [Axis::X, Axis::Y, Axis::Z, Axis::W][i]
}

impl Run {
    pub fn new(dir: &Path, avatar: Avatar) -> Run {
        let nodes = Nodes::read(&avatar).unwrap();
        let names = paths(&avatar, &nodes);
        let mut physics_nodes = BTreeSet::new();
        for p in &avatar.model.physbones {
            let root = p.root.0;
            let ignored: BTreeSet<u32> = p.ignore_transforms.iter().map(|n| n.0).collect();
            let mut stack: Vec<u32> = nodes.children[root as usize].iter().map(|&c| c as u32).filter(|c| !ignored.contains(c)).collect();
            while let Some(n) = stack.pop() {
                physics_nodes.insert(n);
                stack.extend(nodes.children[n as usize].iter().map(|&c| c as u32).filter(|c| !ignored.contains(c)));
            }
        }
        for c in &avatar.model.constraints {
            physics_nodes.insert(c.target.0);
        }
        Run { dir: dir.to_path_buf(), avatar, names, physics_nodes }
    }

    fn node(&self, path: &str) -> Option<u32> {
        self.names.iter().position(|n| n.as_deref() == Some(path)).map(|i| i as u32)
    }

    /// The recording also tracks renderers the model reports unsupported (particle systems).
    fn renderer(&self, path: &str) -> Option<u32> {
        self.node(path).filter(|n| self.avatar.model.renderers.iter().any(|r| r.node.0 == *n))
    }

    fn key(&self, k: &str) -> Option<Key> {
        if k == "view" {
            return Some(Key::View);
        }
        if let Some(target) = self.binding(k) {
            return Some(Key::Binding(target));
        }
        let (kind, rest) = k.split_once(':')?;
        Some(match kind {
            "active" => Key::Scalar(Target::NodeActive(self.node(rest)?)),
            "enabled" => Key::Scalar(Target::RendererEnabled(self.renderer(rest)?)),
            "slot" => {
                let (p, i) = rest.rsplit_once(':')?;
                Key::Slot(Target::Slot(self.renderer(p)?, i.parse().ok()?))
            }
            "shape" => {
                let (p, n) = rest.split_once(':').filter(|(p, _)| self.node(p).is_some()).or_else(|| {
                    let i = rest.rfind(':')?;
                    Some((&rest[..i], &rest[i + 1..]))
                })?;
                Key::Scalar(Target::BlendShape(self.node(p)?, n.to_string()))
            }
            "physbone" => {
                let (p, k) = rest.rsplit_once('#')?;
                let node = self.node(p)?;
                let k: usize = k.parse().ok()?;
                let idx = self.avatar.model.physbones.iter().enumerate().filter(|(_, b)| b.node.0 == node).nth(k)?.0;
                Key::Scalar(Target::PhysBone(idx as u32, "m_Enabled".into()))
            }
            "mat" => {
                let (p, prop) = rest.rsplit_once(':')?;
                let n = self.renderer(p)?;
                Key::Vector((0..4).map(|i| Some(Target::MaterialProperty(n, format!("{prop}.{}", ["r", "g", "b", "a"][i])))).collect())
            }
            "xform" => Key::Transform(self.node(rest).filter(|n| !self.physics_nodes.contains(n))?),
            "playable" => Key::Playable(rest.parse().ok()?),
            "weight" => {
                let mut f = rest.splitn(3, ':');
                Key::Weight(f.next()?.parse().ok()?, f.next()?.parse().ok()?)
            }
            "tracking" => Key::Tracking(avatar_core::animator::TRACKED_PARTS.iter().position(|p| *p == rest)?),
            "locomotion" => Key::Locomotion,
            "curve" => {
                let (p, n) = rest.split_once(':')?;
                Key::Curve(p.parse().ok()?, n.to_string())
            }
            _ => return None,
        })
    }

    fn binding(&self, k: &str) -> Option<Target> {
        let (kind, rest) = k.split_once(':')?;
        if kind == "muscle" {
            return Some(Target::Muscle(rest.to_string()));
        }
        if !matches!(kind, "constraint" | "sender" | "receiver") {
            return None;
        }
        let (rest, field) = rest.rsplit_once(':')?;
        let (path, k) = rest.rsplit_once('#')?;
        let node = self.node(path)?;
        let k: usize = k.parse().ok()?;
        let nth = |nodes: Vec<u32>| nodes.iter().enumerate().filter(|(_, n)| **n == node).nth(k).map(|(i, _)| i as u32);
        let m = &self.avatar.model;
        Some(match kind {
            "constraint" => Target::Constraint(nth(m.constraints.iter().map(|c| c.node.0).collect())?, field.to_string()),
            "sender" => Target::ContactSender(nth(m.contacts.senders.iter().map(|c| c.node.0).collect())?, field.to_string()),
            _ => Target::ContactReceiver(nth(m.contacts.receivers.iter().map(|c| c.node.0).collect())?, field.to_string()),
        })
    }

    /// Parameters the host's PhysBones and contacts produce, which the replay takes from the recording.
    fn dynamic(&self) -> BTreeSet<String> {
        let m = &self.avatar.model;
        let mut out = BTreeSet::new();
        for p in &m.physbones {
            if let Some(n) = &p.parameter {
                for s in ["_IsGrabbed", "_IsPosed", "_Angle", "_Stretch", "_Squish"] {
                    out.insert(format!("{n}{s}"));
                }
            }
        }
        for r in &m.contacts.receivers {
            out.insert(r.parameter.clone());
        }
        out
    }

    pub fn run(&self, frames: Option<usize>) -> Report {
        let mut anim = Animator::new(&self.avatar);
        let steps = steps(&self.dir);
        let params = log(&self.dir, "params.tsv");
        let states = log(&self.dir, "states.tsv");
        let values = log(&self.dir, "values.tsv");
        let last = text(&self.dir.join("done")).trim().parse::<usize>().unwrap_or(0);
        let first = steps.first().expect("the recording has no steps").frame;
        let end = frames.map_or(last, |f| (first + f).min(last));
        let times = graph_times(&self.dir);
        let created = *times.keys().next().expect("the recording has no graph time");
        assert!(times.get(&created) == Some(&0.0) || created == 0, "the graph's time is {:?} on the first frame recorded after frame 0", times.get(&created));
        for f in created.min(first)..end {
            assert!(times.contains_key(&f), "the recording has no graph time at frame {f}");
        }
        let mut clock = 0.0;
        let mut by_name: BTreeMap<&str, Vec<&BTreeMap<usize, String>>> = BTreeMap::new();
        let held = log(&self.dir, "dynamics.tsv");
        for (k, l) in &held {
            by_name.entry(k.split_once(':').expect("a dynamics key is <playable>:<name>").1).or_default().push(l);
        }
        let mut dynamic: BTreeMap<String, Vec<&BTreeMap<usize, String>>> = BTreeMap::new();
        for n in self.dynamic().into_iter().filter(|n| anim.get(n).is_some()) {
            let logs = by_name.remove(canonical(&n).as_str()).unwrap_or_else(|| panic!("the recording holds no animator value of {n}"));
            dynamic.insert(n, logs);
        }
        let mut raw: HashMap<String, String> = HashMap::new();
        for p in anim.parameters() {
            if let Some(other) = raw.insert(canonical(&p.name), p.name.clone()) {
                panic!("parameters {other} and {} share the name {}", p.name, canonical(&p.name));
            }
        }
        let authored: std::cell::RefCell<HashMap<Target, f32>> = Default::default();
        let heads = heads(&self.dir);
        let builtin: BTreeSet<&str> = BUILT_INS.iter().map(|b| b.0).collect();
        let kinds = anim.playable_kinds();
        let order: Vec<usize> = kinds
            .iter()
            .map(|k| avatar_core::animator::PLAYABLE_ORDER.iter().position(|o| o == k).unwrap())
            .collect();
        let blink_shape = match &self.avatar.model.descriptor.eyes.eyelids {
            avatar_core::model::Eyelids::BlendShapes { mesh, blink: Some(n), .. } => Some(Target::BlendShape(mesh.0, n.clone())),
            _ => None,
        };
        // The emulator binds the animator during the blink window of its first frame, so the eyelid's bound default
        // is the shut shape.
        if let Some(t) = blink_shape.as_ref().filter(|_| emulator_blink(created)) {
            anim.bind_default(t, 100.0);
        }
        let keys: Vec<(String, Key)> = values.keys().filter_map(|k| self.key(k).map(|t| (k.clone(), t))).collect();
        let materials: Vec<String> = self.avatar.gltf["materials"]
            .as_array()
            .map(|a| a.iter().map(|m| m["name"].as_str().unwrap_or("").to_string()).collect())
            .unwrap_or_default();
        let index: HashMap<Target, usize> = anim.targets().iter().enumerate().map(|(i, t)| (t.clone(), i)).collect();
        let inputs = inputs(&self.dir);
        for f in first..end {
            assert!(inputs.contains_key(&f), "the recording has no scripted inputs at frame {f}");
        }
        let mut rep = Report {
            seen: BTreeMap::new(),
            frames: 0,
            parameters: Mismatches::default(),
            states: Mismatches::default(),
            values: Mismatches::default(),
            worst_value: 0.0,
            clocks: Mismatches::default(),
            clock_bits_differ: 0,
            worst_clock: 0.0,
            bindings: Mismatches::default(),
            worst_binding: 0.0,
        };
        let recorded: BTreeSet<&Target> = keys.iter().filter_map(|(_, k)| if let Key::Binding(t) = k { Some(t) } else { None }).collect();
        let curves = self.avatar.model.clips.iter().flat_map(|c| &c.curves).filter_map(|c| avatar_core::animator::motion::target_of(&c.binding));
        for t in curves.collect::<BTreeSet<_>>().iter() {
            if matches!(t, Target::Muscle(_) | Target::Constraint(..) | Target::ContactSender(..) | Target::ContactReceiver(..)) && !recorded.contains(t) {
                panic!("the recording holds no value of the curve binding {t:?}");
            }
        }
        let held_clocks = clocks(&self.dir);
        for f in created.min(first)..end {
            assert!(held_clocks.frames.contains_key(&f), "the recording has no state times at frame {f}");
        }
        for f in 0..end {
            for (n, v) in inputs.get(&f).into_iter().flatten() {
                if builtin.contains(n.as_str()) {
                    anim.set_builtin(raw.get(n).map_or(n.as_str(), String::as_str), *v);
                } else {
                    anim.set(raw.get(n).map_or(n.as_str(), String::as_str), *v);
                }
            }
            for (n, logs) in &dynamic {
                if f < created {
                    continue;
                }
                let values: Vec<f32> = logs
                    .iter()
                    .map(|l| at(l, f).unwrap_or_else(|| panic!("the recording holds no animator value of {n} at frame {f}")).parse().unwrap())
                    .collect();
                anim.set(n, held_value(n, f, &values));
            }
            if let Some(h) = f.checked_sub(1).and_then(|g| heads.get(&g)) {
                anim.set_head(*h);
            }
            anim.set_eyes(emulator_blink(f) as u8 as f32, 0.0);
            if let Some(&t) = times.get(&f) {
                assert!(t >= clock, "the graph's time went back at frame {f}");
                if t > clock {
                    anim.step((t - clock) as f32);
                    clock = t;
                }
            }
            if let Ok(k) = std::env::var("AW_CLOCK") {
                for s in anim.states().iter().filter(|s| format!("{}:{}", order[s.playable], s.layer) == k) {
                    eprintln!("{f}\t{k}\t{} {} {:?} {:?}", s.current, s.current_time, s.next, s.next_time);
                }
            }
            if f < first {
                continue;
            }
            if f == first {
                let ours: BTreeSet<String> = anim.parameters().iter().map(|p| canonical(&p.name)).collect();
                for k in params.keys().filter(|k| !ours.contains(*k)) {
                    rep.parameters.compared += 1;
                    rep.parameters.note(k, f, "missing from the core".into());
                }
                let ours: BTreeSet<String> =
                    anim.states().into_iter().map(|s| canonical(&format!("{}:{}:{}", order[s.playable], s.layer, s.name))).collect();
                for k in states.keys().filter(|k| !ours.contains(*k)) {
                    rep.states.compared += 1;
                    rep.states.note(k, f, "missing from the core".into());
                }
            }
            rep.frames += 1;
            for p in anim.parameters().iter().filter(|p| !dynamic.contains_key(&p.name)) {
                let Some(l) = params.get(&canonical(&p.name)) else { continue };
                let Some(want) = at(l, f) else { continue };
                rep.seen.entry(p.name.clone()).or_default().insert(want.to_string());
                rep.parameters.compared += 1;
                let want: f32 = want.parse().unwrap();
                if want != p.value {
                    rep.parameters.note(&p.name, f, format!("{} (want {want})", p.value));
                }
            }
            let layer_states = anim.states();
            if f == first {
                let ours: BTreeSet<String> = layer_states.iter().map(|s| format!("{}:{}", order[s.playable], s.layer)).collect();
                for k in held_clocks.columns.keys().filter(|k| !ours.contains(*k)) {
                    rep.clocks.compared += 1;
                    rep.clocks.note(k, f, "missing from the core".into());
                }
            }
            for s in &layer_states {
                let (cur, next) = (s.current.clone(), s.next.clone());
                let key = canonical(&format!("{}:{}:{}", order[s.playable], s.layer, s.name));
                let Some(l) = states.get(&key) else { continue };
                let Some(want) = at(l, f) else { continue };
                rep.seen.entry(key.clone()).or_default().insert(want.to_string());
                rep.states.compared += 1;
                let ours: Vec<String> = std::iter::once(cur).chain(next).collect();
                let wants: Vec<&str> = want.split(" -> ").collect();
                let named = |o: &String, w: &str| if w.starts_with('#') && !o.starts_with('#') { format!("#{:08x}", crc32(o)) } else { canonical(o) };
                let got = ours.iter().zip(wants.iter().chain(std::iter::repeat(&""))).map(|(o, w)| named(o, w)).collect::<Vec<_>>().join(" -> ");
                let want = wants.iter().map(|w| canonical(w)).collect::<Vec<_>>().join(" -> ");
                if got != want {
                    rep.states.note(&key, f, format!("{got} (want {want})"));
                }
            }
            let cells = &held_clocks.frames[&f];
            for s in &layer_states {
                let (cur, next) = (s.current_time, s.next_time);
                let key = format!("{}:{}", order[s.playable], s.layer);
                let column = *held_clocks.columns.get(&key).unwrap_or_else(|| panic!("clocks.tsv holds no layer {key}"));
                let (want_cur, want_next) = cells[column];
                let mut compare = |got: f32, want: f32, which: &str| {
                    rep.clocks.compared += 1;
                    if got.to_bits() != want.to_bits() {
                        rep.clock_bits_differ += 1;
                    }
                    let d = (got - want).abs();
                    rep.worst_clock = rep.worst_clock.max(d);
                    if d > 1e-3 {
                        rep.clocks.note(&key, f, format!("{which} {got} (want {want})"));
                    }
                };
                compare(cur, want_cur, "current");
                match (next, want_next) {
                    (Some(g), Some(w)) => compare(g, w, "next"),
                    (None, None) => {}
                    (g, w) => {
                        rep.clocks.compared += 1;
                        rep.clocks.note(&key, f, format!("next {g:?} (want {w:?})"));
                    }
                }
            }
            let weights = anim.weights();
            let curves = anim.curve_parameters();
            let slot = |pi: usize| kinds.iter().position(|k| *k == avatar_core::animator::PLAYABLE_ORDER[pi]);
            let scalar = |rep: &mut Report, k: &str, got: Option<f32>, want: &str| {
                let Some(v) = got else { return };
                let w: f32 = want.parse().unwrap();
                rep.values.compared += 1;
                let d = (v - w).abs();
                if d > 1e-3 {
                    rep.values.note(k, f, format!("{v} (want {w})"));
                }
                rep.worst_value = rep.worst_value.max(d);
            };
            for (k, key) in &keys {
                let Some(want) = at(&values[k], f) else { continue };
                match key {
                    Key::Playable(i) => scalar(&mut rep, k, slot(*i).map(|s| weights[s].0), want),
                    Key::Weight(i, l) => scalar(&mut rep, k, slot(*i).and_then(|s| weights[s].1.get(*l).copied()), want),
                    Key::Curve(i, n) => {
                        scalar(&mut rep, k, slot(*i).and_then(|s| curves[s].iter().find(|c| c.0 == *n).map(|c| c.1)), want)
                    }
                    Key::Locomotion => scalar(&mut rep, k, Some(anim.locomotion_disabled as u8 as f32), want),
                    Key::Tracking(i) => {
                        rep.values.compared += 1;
                        let got = format!("{:?}", anim.tracking[*i]);
                        if got != want {
                            rep.values.note(k, f, format!("{got} (want {want})"));
                        }
                    }
                    Key::View => {
                        let w: Vec<f32> = want.split(',').map(|x| x.parse().unwrap()).collect();
                        let v = anim.view;
                        let got = [-v.x, v.y, v.z];
                        rep.values.compared += 1;
                        let d = (0..3).map(|i| (got[i] - w[i]).abs()).fold(0.0, f32::max);
                        if d > 1e-3 {
                            rep.values.note(k, f, format!("{got:?} (want {w:?})"));
                        }
                        rep.worst_value = rep.worst_value.max(d);
                    }
                    _ => {}
                }
            }
            let vals = anim.values();
            let animated = |t: &Target| -> Option<f32> { index.get(t).map(|&i| vals[i]) };
            let get = |t: &Target| -> Option<f32> {
                animated(t).or_else(|| Some(*authored.borrow_mut().entry(t.clone()).or_insert_with(|| avatar_core::animator::authored(&self.avatar, t))))
            };
            for (k, key) in &keys {
                let Some(want) = at(&values[k], f) else { continue };
                if want == "-" {
                    continue;
                }
                if matches!(key, Key::Binding(t) if !index.contains_key(t)) {
                    continue;
                }
                rep.seen.entry(k.clone()).or_default().insert(want.to_string());
                if f == first && !matches!(key, Key::Slot(_)) {
                    rep.values.skipped += 1;
                    continue;
                }
                match key {
                    Key::Scalar(t) => {
                        let Some(v) = get(t) else { continue };
                        let v = match t {
                            Target::NodeActive(_) | Target::RendererEnabled(_) | Target::PhysBone(..) => (v != 0.0) as u8 as f32,
                            _ => v,
                        };
                        let w: f32 = want.parse().unwrap();
                        let d = (v - w).abs();
                        rep.values.compared += 1;
                        if d > 1e-3 {
                            rep.values.note(k, f, format!("{v} (want {w})"));
                        }
                        rep.worst_value = rep.worst_value.max(d);
                    }
                    Key::Slot(t) => {
                        let Some(v) = get(t) else { continue };
                        rep.values.compared += 1;
                        let got = materials.get(v as usize).map(String::as_str).unwrap_or("null");
                        if got != want {
                            rep.values.note(k, f, format!("{got} (want {want})"));
                        }
                    }
                    Key::Vector(ts) => {
                        let w: Vec<f32> = want.split(',').filter_map(|x| x.parse().ok()).collect();
                        let alt = |i: usize| -> Option<Target> {
                            let Target::MaterialProperty(n, p) = ts[i].as_ref()? else { return None };
                            let base = p.rsplit_once('.').unwrap().0;
                            Some(Target::MaterialProperty(*n, format!("{base}.{}", ["x", "y", "z", "w"][i])))
                        };
                        let scalar = ts[0].as_ref().and_then(|t| {
                            let Target::MaterialProperty(n, p) = t else { return None };
                            Some(Target::MaterialProperty(*n, p.rsplit_once('.').unwrap().0.to_string()))
                        });
                        for (i, wv) in w.iter().enumerate() {
                            let t = if w.len() == 1 { scalar.clone() } else { ts[i].clone() };
                            let v = t.as_ref().and_then(animated).or_else(|| alt(i).as_ref().and_then(animated)).or_else(|| t.as_ref().and_then(get));
                            let Some(v) = v else { continue };
                            let d = (v - wv).abs();
                            rep.values.compared += 1;
                            if d > 1e-3 {
                                rep.values.note(k, f, format!("[{i}] {v} (want {wv})"));
                            }
                            rep.worst_value = rep.worst_value.max(d);
                        }
                    }
                    Key::Binding(t) => {
                        let Some(v) = animated(t) else { continue };
                        let v = match t {
                            Target::Constraint(_, field) | Target::ContactSender(_, field) | Target::ContactReceiver(_, field) if BOOLEAN_FIELDS.contains(&field.as_str()) || field.starts_with("Affects") || field.starts_with("m_Affect") => {
                                (v != 0.0) as u8 as f32
                            }
                            _ => v,
                        };
                        let w: f32 = want.parse().unwrap();
                        let d = (v - w).abs();
                        rep.bindings.compared += 1;
                        if d > 1e-3 {
                            rep.bindings.note(k, f, format!("{v} (want {w})"));
                        }
                        rep.worst_binding = rep.worst_binding.max(d);
                    }
                    Key::Playable(_) | Key::Weight(..) | Key::Curve(..) | Key::Locomotion | Key::Tracking(_) | Key::View => {}
                    Key::Transform(n) => {
                        let w: Vec<f32> = want.split(',').map(|x| x.parse().unwrap()).collect();
                        let comp = |t: Target| animated(&t);
                        let mut got = [0.0f32; 10];
                        let mut any = false;
                        for i in 0..3 {
                            if let Some(v) = comp(Target::Translation(*n, axis(i))) {
                                got[i] = v;
                                any = true;
                            } else {
                                got[i] = f32::NAN;
                            }
                            if let Some(v) = comp(Target::Scale(*n, axis(i))) {
                                got[7 + i] = v;
                                any = true;
                            } else {
                                got[7 + i] = f32::NAN;
                            }
                        }
                        let mut q = [0.0f32; 4];
                        let mut has_q = true;
                        for i in 0..4 {
                            match comp(Target::Rotation(*n, axis(i))) {
                                Some(v) => {
                                    q[i] = v;
                                    any = true;
                                }
                                None => has_q = false,
                            }
                        }
                        if !any {
                            continue;
                        }
                        rep.seen.entry(k.clone()).or_default().insert(want.to_string());
                        let len = (q.iter().map(|x| x * x).sum::<f32>()).sqrt();
                        let unity = [-got[0], got[1], got[2]];
                        let mut uq = [q[0] / len, -q[1] / len, -q[2] / len, q[3] / len];
                        if uq[3] * w[6] + uq[0] * w[3] + uq[1] * w[4] + uq[2] * w[5] < 0.0 {
                            uq.iter_mut().for_each(|x| *x = -*x);
                        }
                        for i in 0..10 {
                            let v = match i {
                                0..=2 => unity[i],
                                3..=6 if has_q => uq[i - 3],
                                3..=6 => continue,
                                _ => got[i],
                            };
                            if v.is_nan() {
                                continue;
                            }
                            let d = (v - w[i]).abs();
                            rep.values.compared += 1;
                            if d > 1e-3 {
                                rep.values.note(k, f, format!("[{i}] {v} (want {})", w[i]));
                            }
                            rep.worst_value = rep.worst_value.max(d);
                        }
                    }
                }
            }
        }
        rep
    }
}
