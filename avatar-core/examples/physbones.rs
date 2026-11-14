//! Replays a reference recording through the core's PhysBones and scores its tips against it.
//!
//! usage: physbones RECORDING AVATAR.glb [--rate HZ] [--all] [--out DIR] [--chains] [--names]
//!
//! RECORDING is a `reference/run.sh` output and AVATAR.glb the same avatar's Unity export. The replay runs at
//! the host rate (default 60 Hz, the recording's) from the recording's first frame to the end of its scored
//! window, or to its end with `--all`, and prints the median over chains of tip RMS error over chain length in
//! that window, as `reference/compare.py` scores it. `--chains` prints each chain's error, by name with `--names`,
//! and `--out` writes `tips.tsv` and `params.tsv` (the PhysBone parameters) there in the recording's layout.

#[path = "../tests/reference/mod.rs"]
mod reference;

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

fn main() {
    let names = reference::names();
    let args = reference::args();
    let dir = Path::new(&args[1]);
    let avatar = avatar_core::read(&std::fs::read(&args[2]).unwrap()).unwrap();
    let flag = |name: &str| args.iter().position(|a| a == name).map(|i| args[i + 1].clone());
    let rate: f64 = flag("--rate").map_or(60.0, |r| r.parse().unwrap());
    let window = reference::window(dir);
    let end = if args.iter().any(|a| a == "--all") { usize::MAX } else { window.end };
    let replay = reference::Replay::load(dir, avatar, 0..end);
    let trace = replay.run(rate);
    if let Some(out) = flag("--out") {
        let (mut tips, mut params, mut last) = (String::new(), String::new(), BTreeMap::new());
        for ((f, t), p) in trace.frames.iter().zip(&trace.tips).zip(&trace.parameters) {
            if (f - f.round()).abs() > 1e-6 {
                continue;
            }
            let f = f.round() as usize;
            for (tip, x) in t {
                writeln!(tips, "{f}\t{tip}\t{}\t{}\t{}", x.x, x.y, x.z).unwrap();
            }
            for (name, value) in p {
                if last.insert(name.clone(), *value) != Some(*value) {
                    writeln!(params, "{f}\t{name}\t{value}").unwrap();
                }
            }
        }
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(Path::new(&out).join("tips.tsv"), tips).unwrap();
        std::fs::write(Path::new(&out).join("params.tsv"), params).unwrap();
    }
    let errors = reference::errors(dir, &trace, window);
    if args.iter().any(|a| a == "--chains") {
        for (e, tip) in &errors {
            println!("{e:.4}{}", if names { format!("\t{tip}") } else { String::new() });
        }
    }
    let mean = errors.iter().map(|e| e.0).sum::<f32>() / errors.len().max(1) as f32;
    let median = if errors.is_empty() { f32::NAN } else { reference::median(&errors) };
    println!("chains {}\tmedian {median:.4}\tmean {mean:.4}\tworst {:.4}", errors.len(), errors.last().map_or(0.0, |e| e.0));
}
