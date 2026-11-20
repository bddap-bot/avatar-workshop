//! The core's reference run of a model under a recording's scripted input: each frame the harness's parameter
//! inputs (`inputs.tsv`) and its root, head and hands (`track.tsv`), stepped at 60 Hz through
//! [`Evaluator::evaluate`], the head and hands posed where the script holds them. It writes, per frame, every
//! PhysBone tip, and as change logs the animator's parameters and states and the frame output.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;

use avatar_core::animator::BUILT_INS;
use avatar_core::evaluate::{Evaluator, Input, Output};
use avatar_core::model::HumanBodyBone;
use avatar_core::physbones::{Nodes, Transform};
use avatar_core::Avatar;

use super::{animator::inputs, paths, text, tips, unity};

#[derive(Default, PartialEq, Eq, Debug)]
pub struct Run {
    pub tips: String,
    pub parameters: String,
    pub states: String,
    pub output: String,
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
    let tracked = [bone(HumanBodyBone::Head), bone(HumanBodyBone::LeftHand), bone(HumanBodyBone::RightHand)];
    let track: BTreeMap<usize, Vec<Transform>> = text(&dir.join("track.tsv"))
        .lines()
        .skip(1)
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let poses = f[1..5].iter().map(|p| unity(&p.split(',').map(|x| x.parse().unwrap()).collect::<Vec<f32>>())).collect();
            (f[0].parse().unwrap(), poses)
        })
        .collect();
    let script = inputs(dir);
    let first = *script.keys().next().expect("the script has no frames");
    let last = *script.keys().next_back().unwrap();
    let end = frames.map_or(last + 1, |n| (first + n).min(last + 1));
    let builtin: BTreeSet<&str> = BUILT_INS.iter().map(|b| b.0).collect();
    let mut ev = Evaluator::new(avatar).unwrap();
    let mut run = Run::default();
    let (mut params, mut states, mut outputs) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let mut root = Transform::IDENTITY;
    let mut poses: Vec<(usize, Transform)> = vec![];
    for f in first..end {
        for (n, v) in script.get(&f).into_iter().flatten() {
            if builtin.contains(n.as_str()) {
                ev.set_builtin(n, *v);
            } else {
                ev.set(n, *v);
            }
        }
        if let Some(head) = tracked[0] {
            let h = ev.output().space[head];
            ev.set_head(Transform { scale: glam::Vec3::ONE, ..h });
        }
        if let Some(t) = track.get(&f) {
            root = t[0];
            poses = tracked.iter().zip(&t[1..]).filter_map(|(n, p)| n.map(|n| (n, *p))).collect();
        }
        let dt = if f == first { 0.0 } else { 1.0 / 60.0 };
        ev.evaluate(&Input { dt, root, poses: &poses, held: &[], senders: &[] });
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
        for (file, s) in [("tips.tsv", &self.tips), ("params.tsv", &self.parameters), ("states.tsv", &self.states), ("output.tsv", &self.output)] {
            std::fs::write(out.join(file), s).unwrap();
        }
    }
}
