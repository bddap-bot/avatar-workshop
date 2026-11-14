//! Reads each `.glb` given and prints its section sizes and its `unsupported` report.
//!
//! usage: read FILE.glb... [--names]
//! Each file prints as its argument number, and with `--names` as its path, its read error and its `unsupported`
//! kinds; without it the report prints as counts.

#[path = "../tests/reference/names.rs"]
mod names;

use std::process::ExitCode;

fn main() -> ExitCode {
    let names = names::names();
    let mut failed = false;
    for (i, path) in names::args().into_iter().skip(1).enumerate() {
        let shown = if names { path.clone() } else { (i + 1).to_string() };
        let avatar = match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| avatar_core::read(&bytes).map_err(|e| e.to_string()))
        {
            Ok(avatar) => avatar,
            Err(e) => {
                eprintln!("{shown}: {}", if names { e } else { "unreadable: pass --names for the error".into() });
                failed = true;
                continue;
            }
        };
        let m = &avatar.model;
        let gltf_count = |key: &str| {
            avatar
                .gltf
                .get(key)
                .and_then(|v| v.as_array())
                .map_or(0, Vec::len)
        };
        println!(
            "{shown}: {} nodes, {} meshes, {} materials, {} textures, {} humanoid bones, {} renderers, {} physbones, {} colliders, {}+{} contacts, {} constraints, {} parameters, {} menu controls, {} playable layers, {} behaviours, {} clips",
            m.nodes.len(),
            gltf_count("meshes"),
            m.materials.len(),
            gltf_count("textures"),
            m.humanoid.bones.len(),
            m.renderers.len(),
            m.physbones.len(),
            m.colliders.len(),
            m.contacts.senders.len(),
            m.contacts.receivers.len(),
            m.constraints.len(),
            m.parameters.len(),
            m.menu.controls.len(),
            m.layers.len(),
            m.behaviours.len(),
            m.clips.len(),
        );
        if names {
            for u in &m.unsupported {
                println!("  unsupported {} × {}", u.count, u.kind);
            }
        } else if !m.unsupported.is_empty() {
            println!("  unsupported {} × {} kinds", m.unsupported.iter().map(|u| u64::from(u.count)).sum::<u64>(), m.unsupported.len());
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
