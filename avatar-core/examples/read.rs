//! Reads each `.glb` given and prints its section sizes and its `unsupported` report.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut failed = false;
    for path in std::env::args().skip(1) {
        let avatar = match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| avatar_core::read(&bytes).map_err(|e| e.to_string()))
        {
            Ok(avatar) => avatar,
            Err(e) => {
                eprintln!("{path}: {e}");
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
            "{path}: {} nodes, {} meshes, {} materials, {} textures, {} humanoid bones, {} renderers, {} physbones, {} colliders, {}+{} contacts, {} constraints, {} parameters, {} menu controls, {} playable layers, {} behaviours, {} clips",
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
        for u in &m.unsupported {
            println!("  unsupported {} × {}", u.count, u.kind);
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
