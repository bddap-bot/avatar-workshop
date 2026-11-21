//! The core's reference run of a model under a recording's scripted input.
//!
//! usage: run RECORDING AVATAR.glb OUT [--frames N]
//!
//! RECORDING is a `reference/run.sh` output (its `inputs.tsv` and `track.tsv` are the script). The run steps the
//! core at 60 Hz through its frame evaluation and writes to OUT: `tips.tsv`, every PhysBone tip each frame in metres
//! from the root along Unity's axes; and as change logs `params.tsv`, the animator's parameters, `states.tsv`, each
//! layer's state, and `output.tsv`, the frame output (node local poses, active and renderer enabled flags, bound
//! blendshapes, material properties and slots); and `script.tsv`, what each frame fed the core, so another runtime
//! can feed its own the same. Two runs write the same bytes.

#[path = "../tests/reference/mod.rs"]
mod reference;

use std::path::Path;

fn main() {
    reference::names();
    let args = reference::args();
    let frames = args.iter().position(|a| a == "--frames").map(|i| args[i + 1].parse().unwrap());
    let avatar = avatar_core::read(&std::fs::read(&args[2]).unwrap()).unwrap();
    let run = reference::run::run(&avatar, Path::new(&args[1]), frames);
    run.write(Path::new(&args[3]));
    println!("{} frames", run.tips.lines().map(|l| l.split('\t').next().unwrap()).collect::<std::collections::BTreeSet<_>>().len());
}
