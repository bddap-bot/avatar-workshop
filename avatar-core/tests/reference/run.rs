//! The core's reference run of a model under a recording's scripted input: each frame the harness's parameter
//! inputs (`inputs.tsv`) and its root (`track.tsv`), stepped at 60 Hz through [`Evaluator::evaluate`], the body
//! standing in the recorded pose (`poses.tsv`): the humanoid trunk and limbs, which VRChat's IK places and the core
//! takes from its host, posed where the recording holds them, their scale included, while fingers, eyes and jaw stay
//! the animator's. It writes, per frame, every PhysBone tip, and as change logs the animator's parameters and states
//! and the frame output.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;

use avatar_core::animator::BUILT_INS;
use avatar_core::evaluate::{Evaluator, Input, Output};
use avatar_core::model::HumanBodyBone;
use avatar_core::physbones::{Nodes, Transform};
use avatar_core::Avatar;

use super::{animator::inputs, canonical, floats, paths, rows, text, tips, unity};

#[derive(Default, PartialEq, Eq, Debug)]
pub struct Run {
    pub tips: String,
    pub parameters: String,
    pub states: String,
    pub output: String,
    /// What each frame fed the core, for a runtime to feed its own: `frame dt root`, then `set name value builtin`,
    /// `head pose` and `pose node pose` lines, each value the hex of its f32 bits.
    pub script: String,
}

fn log(out: &mut String, last: &mut BTreeMap<String, String>, frame: usize, key: String, value: String) {
    if last.get(&key) != Some(&value) {
        writeln!(out, "{frame}\t{key}\t{value}").unwrap();
        last.insert(key, value);
    }
}

fn pose(t: &Transform) -> String {
    let (p, q, s) = (t.translation, t.rotation, t.scale);
    [p.x, p.y, p.z, q.x, q.y, q.z, q.w, s.x, s.y, s.z].iter().map(|v| format!("{v:?}")).collect::<Vec<_>>().join(",")
}

/// A pose's ten values as the hex of their bits, so a reader gets exactly them.
fn bits(t: &Transform) -> String {
    let (p, q, s) = (t.translation, t.rotation, t.scale);
    [p.x, p.y, p.z, q.x, q.y, q.z, q.w, s.x, s.y, s.z].iter().map(|v| format!("{:08x}", v.to_bits())).collect::<Vec<_>>().join(",")
}

fn output(out: &Output, frame: usize, file: &mut String, last: &mut BTreeMap<String, String>) {
    for (n, t) in out.locals.iter().enumerate() {
        log(file, last, frame, format!("local:{n}"), pose(t));
    }
    for (n, a) in out.active.iter().enumerate() {
        log(file, last, frame, format!("active:{n}"), (*a as u8).to_string());
    }
    for (r, e) in out.renderers.iter().enumerate() {
        log(file, last, frame, format!("enabled:{r}"), (*e as u8).to_string());
    }
    for (n, s, v) in &out.blendshapes {
        log(file, last, frame, format!("shape:{n}:{s}"), format!("{v:?}"));
    }
    for (n, p, v) in &out.materials {
        log(file, last, frame, format!("mat:{n}:{p}"), format!("{v:?}"));
    }
    for (n, s, m) in &out.slots {
        log(file, last, frame, format!("slot:{n}:{s}"), m.to_string());
    }
}

/// Runs `avatar` under the script in `dir` for at most `frames` frames.
pub fn run(avatar: &Avatar, dir: &Path, frames: Option<usize>) -> Run {
    let nodes = Nodes::read(avatar).unwrap();
    let names = paths(avatar, &nodes);
    let bone = |b: HumanBodyBone| avatar.model.humanoid.bones.get(&b).map(|b| b.node.0 as usize);
    let index: BTreeMap<&str, usize> = names.iter().enumerate().filter_map(|(i, n)| n.as_deref().map(|n| (n, i))).collect();
    let body: BTreeSet<usize> = avatar.model.humanoid.bones.iter().filter(|(b, _)| **b <= HumanBodyBone::RightToes).map(|(_, b)| b.node.0 as usize).collect();
    let roots: BTreeMap<usize, Transform> = rows(&dir.join("track.tsv"))[1..].iter().map(|r| (r[0].parse().unwrap(), unity(&floats(&r[1])))).collect();
    let mut stood = BTreeMap::<usize, Vec<(usize, Transform)>>::new();
    for line in text(&dir.join("poses.tsv")).lines() {
        let r: Vec<&str> = line.splitn(4, '\t').collect();
        if r[1] != "root" {
            continue;
        }
        if let Some(&n) = index.get(canonical(r[2]).as_str()).filter(|n| body.contains(n)) {
            stood.entry(r[0].parse().unwrap()).or_default().push((n, unity(&floats(r[3]))));
        }
    }
    let script = inputs(dir);
    let first = *script.keys().next().expect("the script has no frames");
    let unstood: Vec<_> = body.iter().filter(|n| !stood.get(&first).is_some_and(|s| s.iter().any(|(m, _)| m == *n))).map(|&n| &names[n]).collect();
    assert!(unstood.is_empty(), "poses.tsv stands no body bone {unstood:?} on the script's first frame");
    let last = *script.keys().next_back().unwrap();
    let end = frames.map_or(last + 1, |n| (first + n).min(last + 1));
    let builtin: BTreeSet<&str> = BUILT_INS.iter().map(|b| b.0).collect();
    let mut ev = Evaluator::new(avatar).unwrap();
    let mut run = Run::default();
    let (mut params, mut states, mut outputs) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let mut root = Transform::IDENTITY;
    let mut poses: Vec<(usize, Transform)> = vec![];
    for f in first..end {
        let dt: f32 = if f == first { 0.0 } else { 1.0 / 60.0 };
        for (n, v) in script.get(&f).into_iter().flatten() {
            let b = builtin.contains(n.as_str());
            if b {
                ev.set_builtin(n, *v);
            } else {
                ev.set(n, *v);
            }
            writeln!(run.script, "{f}\tset\t{n}\t{:08x}\t{}", v.to_bits(), b as u8).unwrap();
        }
        if let Some(head) = bone(HumanBodyBone::Head) {
            let h = Transform { scale: glam::Vec3::ONE, ..ev.output().space[head] };
            ev.set_head(h);
            writeln!(run.script, "{f}\thead\t{}", bits(&h)).unwrap();
        }
        if let Some(&r) = roots.get(&f) {
            root = r;
        }
        for &(n, p) in stood.get(&f).into_iter().flatten() {
            match poses.iter_mut().find(|(m, _)| *m == n) {
                Some(at) => at.1 = p,
                None => poses.push((n, p)),
            }
        }
        for (n, p) in &poses {
            writeln!(run.script, "{f}\tpose\t{n}\t{}", bits(p)).unwrap();
        }
        writeln!(run.script, "{f}\tframe\t{:08x}\t{}", dt.to_bits(), bits(&root)).unwrap();
        ev.evaluate(&Input { dt, root, poses: &poses, held: &[], senders: &[], hands: &[] });
        for (tip, p) in tips(avatar, &nodes, &names, &ev, ev.output()) {
            writeln!(run.tips, "{f}\t{tip}\t{:?}\t{:?}\t{:?}", p.x, p.y, p.z).unwrap();
        }
        for p in ev.animator().parameters() {
            log(&mut run.parameters, &mut params, f, p.name.clone(), format!("{:?}", p.value));
        }
        for s in ev.animator().states() {
            let at = match &s.next {
                Some(n) => format!("{} -> {n}", s.current),
                None => s.current.clone(),
            };
            log(&mut run.states, &mut states, f, format!("{}:{}:{}", s.playable, s.layer, s.name), at);
        }
        output(ev.output(), f, &mut run.output, &mut outputs);
    }
    run
}

impl Run {
    pub fn write(&self, out: &Path) {
        std::fs::create_dir_all(out).unwrap();
        for (file, s) in [("tips.tsv", &self.tips), ("params.tsv", &self.parameters), ("states.tsv", &self.states), ("output.tsv", &self.output), ("script.tsv", &self.script)] {
            std::fs::write(out.join(file), s).unwrap();
        }
    }
}
