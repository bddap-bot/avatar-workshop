//! Replays a reference recording's inputs through the core's animator and reports where its parameters,
//! states and values differ from the recording's.
//!
//! usage: animator RECORDING AVATAR.glb [--frames N] [--limit N] [--names]
//! Each differing key and menu control prints as a number, and with `--names` as that number and its name.
//! With `--names`, AW_TRACE=<part of a key> prints each wrong sample of matching keys and
//! AW_CLOCK=<playable>:<layer> that layer's states and clocks every frame.

#[path = "../tests/reference/mod.rs"]
mod reference;

use std::path::Path;

fn main() {
    let names = reference::names();
    let args = reference::args();
    let flag = |name: &str| args.iter().position(|a| a == name).map(|i| args[i + 1].parse::<usize>().unwrap());
    let avatar = avatar_core::read(&std::fs::read(&args[2]).unwrap()).unwrap();
    let run = reference::animator::Run::new(Path::new(&args[1]), avatar);
    let rep = run.run(flag("--frames"));
    let limit = flag("--limit").unwrap_or(20);
    println!("{} frames", rep.frames);
    let summary = |m: &reference::animator::Mismatches| if names { m.summary(limit) } else { m.numbered(limit) };
    println!("parameters: {}", summary(&rep.parameters));
    println!("states: {}", summary(&rep.states));
    println!("values: {} (largest difference {})", summary(&rep.values), rep.worst_value);
    println!("state times: {} ({} not the recorded bits, largest difference {})", summary(&rep.clocks), rep.clock_bits_differ, rep.worst_clock);
    println!("muscle, constraint and contact bindings: {} (largest difference {})", summary(&rep.bindings), rep.worst_binding);
    let controls = rep.controls(Path::new(&args[1]));
    let matched = controls.iter().filter(|c| c.1).count();
    println!("menu controls: {matched} of {} match", controls.len());
    for (i, (c, _)) in controls.iter().enumerate().filter(|(_, c)| !c.1).take(limit) {
        println!("  {i}{}", if names { format!(" {c}") } else { String::new() });
    }
}
