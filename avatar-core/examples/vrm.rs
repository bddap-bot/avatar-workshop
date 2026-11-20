//! Imports a VRM file into the model and scores its springs against a three-vrm recording.
//!
//! usage: vrm AVATAR.vrm [--out MODEL.glb] [--springs RECORDING.tsv [--trace NODE]]
//!
//! Prints the model's unsupported report and, with `--springs` (a `vrm-reference/run.sh` output), each recorded
//! tip's RMS distance from three-vrm's over its path length, worst last; `--trace` prints one tip's positions
//! and distance each frame.

#[path = "../tests/springs/mod.rs"]
mod springs;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).map(|i| args[i + 1].clone());
    let avatar = avatar_core::vrm::import(&std::fs::read(&args[1]).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", args[1]));
    let m = &avatar.model;
    println!("{} nodes, {} materials, {} physbones, {} colliders, {} clips", m.nodes.len(), m.materials.len(), m.physbones.len(), m.colliders.len(), m.clips.len());
    for u in &m.unsupported {
        println!("unsupported\t{}\t{}", u.count, u.kind);
    }
    if let Some(out) = flag("--out") {
        std::fs::write(out, avatar_core::write(&avatar).unwrap()).unwrap();
    }
    if let Some(path) = flag("--springs") {
        let recording = springs::parse(&std::fs::read_to_string(path).unwrap());
        let replayed = springs::replay(&avatar, &recording, springs::head(&avatar));
        if let Some(node) = flag("--trace").map(|n| n.parse::<usize>().unwrap()) {
            let k = recording.tips.iter().position(|t| t.0 == node).expect("a recorded tip");
            for (f, (r, c)) in recording.at.iter().zip(&replayed).enumerate() {
                println!("trace\t{f}\t{}\t{}\t{}", r[k], c[k], (r[k] - c[k]).length());
            }
        }
        let mut errors = springs::errors(&recording, &replayed);
        errors.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (tip, e) in &errors {
            println!("tip\t{tip}\t{e:.4}");
        }
        println!("tips {}\tworst {:.4}", errors.len(), errors.last().map_or(0.0, |e| e.1));
    }
}
