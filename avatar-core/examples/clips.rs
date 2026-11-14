//! Compares the core's sampling of each clip with Unity's, from the reference harness's clip sampling of a
//! recording (`AW_CLIPS`): each muscle curve's value from the clip's one-clip graph (the stream) beside the core's
//! sample of that clip alone at the same normalized time, and beside `AnimationCurve.Evaluate`.
//!
//! usage: clips SAMPLES AVATAR.glb [--frames FIRST-LAST]
//! Prints, per clip, how many muscle samples differ by more than 1e-3 between the stream and Evaluate, and, where
//! the core samples the muscle, between the core and the stream and the core and Evaluate. With `--frames`, it also prints each clip played on those
//! frames: frame, `<playable>:<layer>`, current or next state, clip, normalized time, and the three counts and
//! largest differences over that sample's muscle curves.

use std::collections::HashMap;
use std::path::Path;

use avatar_core::animator::motion::Target;

const TOLERANCE: f32 = 1e-3;

struct Clip {
    model: Option<usize>,
    curves: Vec<String>,
}

#[derive(Default, Clone, Copy)]
struct Tally {
    read: usize,
    compared: usize,
    core_stream: usize,
    stream_evaluate: usize,
    core_evaluate: usize,
    worst_core_stream: f32,
    worst_stream_evaluate: f32,
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        self.read += o.read;
        self.compared += o.compared;
        self.core_stream += o.core_stream;
        self.stream_evaluate += o.stream_evaluate;
        self.core_evaluate += o.core_evaluate;
        self.worst_core_stream = self.worst_core_stream.max(o.worst_core_stream);
        self.worst_stream_evaluate = self.worst_stream_evaluate.max(o.worst_stream_evaluate);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = Path::new(&args[1]);
    let avatar = avatar_core::read(&std::fs::read(&args[2]).unwrap()).unwrap();
    let frames = args.iter().position(|a| a == "--frames").map(|i| {
        let (a, b) = args[i + 1].split_once('-').unwrap();
        (a.parse::<u32>().unwrap(), b.parse::<u32>().unwrap())
    });
    let (targets, compiled) = avatar_core::animator::compile_clips(&avatar);
    let defaults = avatar_core::animator::authored_all(&avatar, &targets);
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));

    let mut unmatched = 0;
    let clips: Vec<Clip> = read("clips.tsv")
        .lines()
        .skip(1)
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            let (name, length): (&str, f32) = (c[1], c[2].parse().unwrap());
            let curves: Vec<String> = c[6..].iter().map(|s| s.to_string()).collect();
            let muscles = |k: &[String]| -> Vec<String> {
                k.iter().filter_map(|k| k.strip_prefix("muscle:").or_else(|| k.strip_prefix("animator:")).map(str::to_string)).collect()
            };
            let ours = muscles(&curves);
            let found: Vec<usize> = avatar
                .model
                .clips
                .iter()
                .enumerate()
                .filter(|(_, m)| m.name == name && m.length == length)
                .filter(|(_, m)| {
                    let mut theirs: Vec<String> = m.curves.iter().filter_map(|x| match &x.binding {
                        avatar_core::model::Binding::Muscle { name } | avatar_core::model::Binding::AnimatorParameter { name } => Some(name.clone()),
                        _ => None,
                    }).collect();
                    theirs.sort();
                    let mut o = ours.clone();
                    o.sort();
                    theirs == o
                })
                .map(|(i, _)| i)
                .collect();
            let model = (found.len() == 1).then(|| found[0]);
            if model.is_none() {
                unmatched += 1;
            }
            Clip { model, curves }
        })
        .collect();

    let mut per_sample: HashMap<(usize, u32), Tally> = HashMap::new();
    let mut per_clip = vec![Tally::default(); clips.len()];
    for line in read("samples.tsv").lines().skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        let id: usize = c[0].parse().unwrap();
        let n: f32 = c[1].parse().unwrap();
        let clip = &clips[id];
        let Some(m) = clip.model else { continue };
        let cc = compiled[m].variant(false);
        let mut core: HashMap<u32, f32> = HashMap::new();
        cc.sample(n + cc.cycle_offset, &defaults, false, &mut |t, v| {
            core.insert(t, v);
        });
        let mut t = Tally::default();
        for (k, cell) in clip.curves.iter().zip(&c[3..]) {
            let Some(name) = k.strip_prefix("muscle:") else { continue };
            if avatar_core::animator::human::goal(name).is_some() {
                continue;
            }
            let (s, e) = cell.split_once(',').unwrap();
            let (Ok(s), Ok(e)) = (s.parse::<f32>(), e.parse::<f32>()) else { continue };
            t.read += 1;
            let se = (s - e).abs();
            t.stream_evaluate += (se > TOLERANCE) as usize;
            t.worst_stream_evaluate = t.worst_stream_evaluate.max(se);
            let Some(id) = targets.index_of(&Target::Muscle(name.to_string())) else { continue };
            let Some(&v) = core.get(&(id as u32)) else { continue };
            t.compared += 1;
            let (cs, ce) = ((v - s).abs(), (v - e).abs());
            t.core_stream += (cs > TOLERANCE) as usize;
            t.core_evaluate += (ce > TOLERANCE) as usize;
            t.worst_core_stream = t.worst_core_stream.max(cs);
        }
        per_clip[id].add(&t);
        if frames.is_some() {
            per_sample.insert((id, n.to_bits()), t);
        }
    }

    let mut total = Tally::default();
    for t in &per_clip {
        total.add(t);
    }
    let unmatched_ids: Vec<String> = clips.iter().enumerate().filter(|(_, c)| c.model.is_none()).map(|(i, _)| i.to_string()).collect();
    println!("{} clips, {unmatched} without one model clip of the same name, length and animator curves: {}", clips.len(), unmatched_ids.join(" "));
    println!(
        "muscle samples: stream vs Evaluate {} of {} differ (largest {}); of the {} the core samples, core vs stream {} (largest {}), core vs Evaluate {}",
        total.stream_evaluate, total.read, total.worst_stream_evaluate, total.compared, total.core_stream, total.worst_core_stream, total.core_evaluate
    );
    for (id, t) in per_clip.iter().enumerate().filter(|(_, t)| t.core_stream > 0 || t.stream_evaluate > 0 || t.core_evaluate > 0) {
        println!(
            "  clip {id}: stream vs Evaluate {} of {} (largest {}); of the {} the core samples, core vs stream {} (largest {}), core vs Evaluate {}",
            t.stream_evaluate, t.read, t.worst_stream_evaluate, t.compared, t.core_stream, t.worst_core_stream, t.core_evaluate
        );
    }
    if let Some((first, last)) = frames {
        println!("frame\tplayable:layer\tstate\tclip\tnormalized time\tcompared\tcore vs stream\tlargest\tstream vs Evaluate\tlargest\tcore vs Evaluate");
        for line in read("played.tsv").lines().skip(1) {
            let c: Vec<&str> = line.split('\t').collect();
            let f: u32 = c[0].parse().unwrap();
            if f < first || f > last {
                continue;
            }
            let id: usize = c[3].parse().unwrap();
            let n: f32 = c[4].parse().unwrap();
            match per_sample.get(&(id, n.to_bits())) {
                Some(t) => println!(
                    "{f}\t{}\t{}\t{id}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    c[1], c[2], c[4], t.compared, t.core_stream, t.worst_core_stream, t.stream_evaluate, t.worst_stream_evaluate, t.core_evaluate
                ),
                None => println!("{f}\t{}\t{}\t{id}\t{}\tno model clip", c[1], c[2], c[4]),
            }
        }
    }
}
