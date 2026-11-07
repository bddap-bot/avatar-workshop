//! Replays a reference recording's inputs through the core's animator and reports where its parameters,
//! states and values differ from the recording's.
//!
//! usage: animator RECORDING AVATAR.glb [--frames N] [--limit N]

#[path = "../tests/reference/mod.rs"]
mod reference;

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).map(|i| args[i + 1].parse::<usize>().unwrap());
    let avatar = avatar_core::read(&std::fs::read(&args[2]).unwrap()).unwrap();
    let run = reference::animator::Run::new(Path::new(&args[1]), avatar);
    let rep = run.run(flag("--frames"));
    let limit = flag("--limit").unwrap_or(20);
    println!("{} frames", rep.frames);
    println!("parameters: {}", rep.parameters.summary(limit));
    println!("states: {}", rep.states.summary(limit));
    println!("values: {} (largest difference {})", rep.values.summary(limit), rep.worst_value);
}
