//! Replays a reference recording's constraints and contacts through the core and scores each constrained target
//! and receiver parameter.
//!
//! usage: dynamics RECORDING AVATAR.glb [--names]
//!
//! RECORDING is a `reference/run.sh` output and AVATAR.glb its own build's export. Each target prints its largest
//! position or scale (metres) and rotation (degrees) difference from `constrained.tsv`, each receiver parameter the
//! frames whose value differs from `params.tsv`; by name with `--names`, by number without.

#[path = "../tests/reference/mod.rs"]
mod reference;

use std::path::Path;

fn main() {
    let names = reference::names();
    let args = reference::args();
    let dir = Path::new(&args[1]);
    let avatar = avatar_core::read(&std::fs::read(&args[2]).unwrap()).unwrap();
    let replay = reference::Replay::load(dir, avatar, 0..usize::MAX);
    let report = reference::dynamics::score(dir, &replay);
    let (mut p, mut q) = (0.0f32, 0.0f32);
    for (k, (path, e)) in report.targets.iter().enumerate() {
        let label = if names { path.clone() } else { format!("target {k}") };
        p = p.max(e.position.0);
        q = q.max(e.rotation.0);
        println!("{:.6} m @{}\t{:.4}° @{}\t{label}", e.position.0, e.position.1, e.rotation.0, e.rotation.1);
    }
    let mut wrong = 0;
    for (k, (name, r)) in report.receivers.iter().enumerate() {
        let label = if names { name.clone() } else { format!("receiver {k}") };
        wrong += (r.wrong > 0) as usize;
        let first = r.first_wrong.map_or(String::new(), |(f, v, w)| format!(" first @{f}: {v} (want {w})"));
        println!("{} of {} frames wrong, worst {:.6}{first}\t{label}", r.wrong, r.frames, r.worst);
    }
    println!(
        "targets {}\tworst {p:.6} m\t{q:.4}°\treceiver parameters {} wrong {wrong}",
        report.targets.len(),
        report.receivers.len()
    );
}
