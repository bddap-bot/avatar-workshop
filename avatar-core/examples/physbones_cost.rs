//! Measures the core's PhysBones cost per simulated joint per step.
//!
//! usage: physbones_cost AVATAR.glb [SECONDS] [--names]
//!
//! Steps every PhysBone of the avatar for SECONDS (default 20) of simulated time at a 72 Hz host rate, the avatar
//! root swinging and turning so every chain moves, and prints the time per joint per step. A failure's message,
//! which may name a part of the avatar, prints with `--names`.

#[path = "../tests/reference/names.rs"]
mod names;

use std::time::Instant;

use avatar_core::physbones::{Frame, Nodes, PhysBones, Transform, STEP};
use glam::{Quat, Vec3};

fn main() {
    names::names();
    let args = names::args();
    let avatar = avatar_core::read(&std::fs::read(&args[1]).unwrap()).unwrap();
    let seconds: f64 = args.get(2).map_or(20.0, |s| s.parse().unwrap());
    let nodes = Nodes::read(&avatar).unwrap();
    let count = nodes.children.len();
    let mut space = vec![Transform::IDENTITY; count];
    let roots: Vec<usize> = (0..count).filter(|&n| nodes.parent[n].is_none()).collect();
    let mut stack: Vec<(usize, Transform)> = roots.iter().map(|&r| (r, Transform::IDENTITY)).collect();
    while let Some((n, parent)) = stack.pop() {
        space[n] = if nodes.parent[n].is_none() { Transform::IDENTITY } else { parent.mul(&nodes.rest[n]) };
        stack.extend(nodes.children[n].iter().map(|&c| (c, space[n])));
    }
    let mut frame = Frame {
        root: Transform::IDENTITY,
        nodes: space,
        locals: nodes.rest.clone(),
        active: vec![true; count],
        enabled: vec![true; avatar.model.physbones.len()],
    };
    let mut pb = PhysBones::new(&avatar).unwrap();
    let joints: usize = (0..avatar.model.physbones.len()).map(|c| pb.bones(c).count()).sum();
    let dt = 1.0 / 72.0;
    let frames = (seconds / dt) as usize;
    pb.advance(0.0, &frame);
    let start = Instant::now();
    for k in 1..=frames {
        let t = (k as f64 * dt) as f32;
        frame.root.translation = Vec3::new(0.25 * (t * 2.1).sin(), 0.04 * (t * 8.4).sin(), 0.4 * (t * 3.1).sin());
        frame.root.rotation = Quat::from_rotation_y(0.8 * (t * 1.05).sin());
        pb.advance(dt as f32, &frame);
    }
    let elapsed = start.elapsed().as_secs_f64();
    let steps = (frames as f64 * dt / STEP as f64).round();
    println!(
        "{} PhysBones, {joints} joints, {steps} steps in {elapsed:.3} s: {:.1} ns per joint per step, {:.3} ms per 72 Hz frame",
        avatar.model.physbones.len(),
        elapsed / steps / joints as f64 * 1e9,
        elapsed / frames as f64 * 1e3
    );
}
