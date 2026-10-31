mod fixture;

use std::path::PathBuf;

use avatar_core::model::{LayerContent, PlayableLayerType};
use avatar_core::{read, schema, write, Error};
use serde_json::json;

fn path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// `AW_BLESS=1 cargo test` rewrites a checked-in artifact from the code instead of comparing.
fn check(relative: &str, generated: &[u8]) {
    let file = path(relative);
    if std::env::var_os("AW_BLESS").is_some() {
        let fresh = file.with_extension("blessing");
        std::fs::write(&fresh, generated).unwrap();
        std::fs::rename(fresh, file).unwrap();
        return;
    }
    let checked_in = std::fs::read(&file).unwrap_or_default();
    assert!(
        checked_in == generated,
        "{relative} differs from the generated one; regenerate with `AW_BLESS=1 cargo test`, then run the tests again"
    );
}

#[test]
fn schema_is_generated() {
    let mut text = serde_json::to_string_pretty(&schema()).unwrap();
    text.push('\n');
    check("schema/AW_avatar.schema.json", text.as_bytes());
}

#[test]
fn fixture_is_generated() {
    check(
        "fixtures/synthetic.glb",
        &write(&fixture::avatar()).unwrap(),
    );
}

#[test]
fn fixture_round_trips_unchanged() {
    let bytes = std::fs::read(path("fixtures/synthetic.glb")).unwrap();
    let avatar = read(&bytes).unwrap();
    assert_eq!(avatar, fixture::avatar());
    assert!(
        write(&avatar).unwrap() == bytes,
        "writing the read fixture changed its bytes"
    );
}

fn assert_every_section_filled(file: &str) {
    let model = read(&std::fs::read(path(file)).unwrap()).unwrap().model;
    let serde_json::Value::Object(sections) = serde_json::to_value(&model).unwrap() else {
        unreachable!()
    };
    for (name, value) in &sections {
        let empty = match value {
            serde_json::Value::Array(items) => items.is_empty(),
            serde_json::Value::Object(fields) => fields
                .values()
                .all(|v| v.as_array().is_some_and(Vec::is_empty)),
            _ => false,
        };
        assert!(!empty, "{file}'s {name} section is empty");
    }
    assert_eq!(sections.len(), 16);
}

#[test]
fn fixture_fills_every_section() {
    assert_every_section_filled("fixtures/synthetic.glb");
}

/// The synthetic fixture as the Unity exporter writes it from its Unity build of the fixture.
#[test]
fn unity_export_fills_every_section() {
    assert_every_section_filled("fixtures/unity-synthetic.glb");
}

#[test]
fn unknown_fields_are_refused() {
    let mut avatar = fixture::avatar();
    avatar.model.source.importer = "x".repeat(4);
    let mut bytes = write(&avatar).unwrap();
    let needle = br#""importer":"xxxx""#;
    let at = bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap();
    bytes[at + 1..at + 9].copy_from_slice(b"imported");
    let error = read(&bytes).unwrap_err().to_string();
    assert!(error.contains("imported"), "{error}");
}

#[test]
fn truncated_files_are_refused() {
    let bytes = write(&fixture::avatar()).unwrap();
    for cut in [0, 4, 12, 20, bytes.len() - 1] {
        assert!(
            read(&bytes[..cut]).is_err(),
            "a file cut at {cut} bytes was read"
        );
    }
}

fn json_chunk(bytes: &[u8]) -> serde_json::Value {
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[20..20 + len]).unwrap()
}

fn with_json(bytes: &[u8], edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let mut json = json_chunk(bytes);
    edit(&mut json);
    let mut avatar = read(bytes).unwrap();
    let serde_json::Value::Object(mut gltf) = json else {
        unreachable!()
    };
    let model = gltf["extensions"]["AW_avatar"].take();
    gltf.shift_remove("extensions");
    gltf.shift_remove("extensionsUsed");
    avatar.gltf = gltf;
    let out = write(&avatar).unwrap();
    let mut doc = json_chunk(&out);
    doc["extensions"]["AW_avatar"] = model;
    with_json_chunk(out, &doc)
}

fn with_json_chunk(mut out: Vec<u8>, doc: &serde_json::Value) -> Vec<u8> {
    let mut text = serde_json::to_vec(doc).unwrap();
    text.resize(text.len().next_multiple_of(4), b' ');
    let old = u32::from_le_bytes(out[12..16].try_into().unwrap()) as usize;
    out.splice(20..20 + old, text.iter().copied());
    out[12..16].copy_from_slice(&(text.len() as u32).to_le_bytes());
    let total = out.len() as u32;
    out[8..12].copy_from_slice(&total.to_le_bytes());
    out
}

#[test]
fn unknown_fields_on_fieldless_variants_are_refused() {
    let bytes = write(&fixture::avatar()).unwrap();
    let junk = with_json(&bytes, |json| {
        json["extensions"]["AW_avatar"]["colliders"][2]["shape"]["junk"] = 1.into();
    });
    let error = read(&junk).unwrap_err().to_string();
    assert!(error.contains("junk"), "{error}");
}

#[test]
fn values_json_cannot_carry_are_refused_on_write() {
    let mut avatar = fixture::avatar();
    avatar.model.descriptor.view_position[0] = f32::NAN;
    assert!(write(&avatar).is_err());
    let mut avatar = fixture::avatar();
    let Some(LayerContent::StateMachine { state_machine }) = avatar
        .model
        .layers
        .get_mut(&PlayableLayerType::Fx)
        .and_then(|layer| layer.controller.as_mut())
        .map(|controller| &mut controller.layers[0].content)
    else {
        unreachable!()
    };
    state_machine.states[0].transitions[0].exit_time = Some(f32::INFINITY);
    assert!(matches!(write(&avatar), Err(Error::Unrepresentable)));
}

#[test]
fn malformed_gltf_roots_are_refused_on_write() {
    let mut avatar = fixture::avatar();
    avatar
        .gltf
        .insert("extensions".into(), serde_json::json!([]));
    assert!(write(&avatar).is_err());
    let mut avatar = fixture::avatar();
    avatar
        .gltf
        .insert("extensionsUsed".into(), serde_json::json!({}));
    assert!(write(&avatar).is_err());
}

#[test]
fn extension_is_declared_once() {
    let mut avatar = fixture::avatar();
    avatar.gltf.insert(
        "extensionsUsed".into(),
        serde_json::json!(["KHR_materials_emissive_strength", "AW_avatar"]),
    );
    let used = json_chunk(&write(&avatar).unwrap())["extensionsUsed"].clone();
    assert_eq!(
        used,
        serde_json::json!(["KHR_materials_emissive_strength", "AW_avatar"])
    );
}

#[test]
fn unknown_chunks_are_skipped() {
    let mut bytes = write(&fixture::avatar()).unwrap();
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(b"XTRA");
    bytes.extend_from_slice(&[1, 2, 3, 4]);
    let total = bytes.len() as u32;
    bytes[8..12].copy_from_slice(&total.to_le_bytes());
    assert_eq!(read(&bytes).unwrap(), fixture::avatar());
}

#[test]
fn one_entry_per_gltf_node_and_material() {
    let mut avatar = fixture::avatar();
    avatar.model.nodes.pop();
    assert!(matches!(write(&avatar), Err(Error::Invalid(i)) if i.at == "/nodes"));
    let mut avatar = fixture::avatar();
    avatar.model.materials.pop();
    assert!(matches!(write(&avatar), Err(Error::Invalid(i)) if i.at == "/materials"));
    let bytes = write(&fixture::avatar()).unwrap();
    let short = with_json(&bytes, |json| {
        json["extensions"]["AW_avatar"]["nodes"]
            .as_array_mut()
            .unwrap()
            .pop();
    });
    assert!(matches!(read(&short), Err(Error::Invalid(i)) if i.at == "/nodes"));
}

#[test]
fn duplicate_set_entries_are_refused() {
    let bytes = write(&fixture::avatar()).unwrap();
    let doubled = with_json(&bytes, |json| {
        json["extensions"]["AW_avatar"]["layers"]["gesture"]["mask"]["bodyParts"] =
            serde_json::json!(["LeftFingers", "LeftFingers"]);
    });
    let error = read(&doubled).unwrap_err().to_string();
    assert!(error.contains("duplicate"), "{error}");
}

#[test]
fn a_binary_chunk_after_another_chunk_is_refused() {
    let mut bytes = write(&fixture::avatar()).unwrap();
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(b"BIN\0");
    bytes.extend_from_slice(&[1, 2, 3, 4]);
    let total = bytes.len() as u32;
    bytes[8..12].copy_from_slice(&total.to_le_bytes());
    assert!(read(&bytes).is_err());
}

#[test]
fn stepped_keys_round_trip() {
    let mut avatar = fixture::avatar();
    let key = &mut avatar.model.clips[0].curves[0].curve.keys[0];
    key.in_tangent = f32::NEG_INFINITY;
    key.out_tangent = f32::INFINITY;
    assert_eq!(read(&write(&avatar).unwrap()).unwrap(), avatar);
}

#[test]
fn unity_export_keeps_what_the_fixture_authored() {
    use avatar_core::model::Limit;
    let avatar = read(&std::fs::read(path("fixtures/unity-synthetic.glb")).unwrap()).unwrap();
    let nodes = avatar.gltf["nodes"].as_array().unwrap();
    let node = |name: &str| {
        nodes
            .iter()
            .position(|n| n["name"] == name)
            .unwrap_or_else(|| panic!("no node {name}"))
    };
    let x = nodes[node("LeftShoulder")]["translation"][0]
        .as_f64()
        .unwrap();
    assert!(
        (x - 0.05).abs() < 1e-6,
        "Unity's x = -0.05 mirrors to 0.05, got {x}"
    );
    let model = &avatar.model;
    assert!(
        !model.nodes[node("Hat")].active,
        "the hat is authored off"
    );
    let body = model
        .renderers
        .iter()
        .find(|r| r.node.0 as usize == node("Body"))
        .unwrap();
    assert_eq!(body.materials.len(), 2);
    let limits: std::collections::BTreeSet<&str> = model
        .physbones
        .iter()
        .map(|p| match p.limit {
            Limit::None {} => "none",
            Limit::Angle { .. } => "angle",
            Limit::Hinge { .. } => "hinge",
            Limit::Polar { .. } => "polar",
        })
        .collect();
    assert_eq!(limits, ["angle", "hinge", "none", "polar"].into());
    assert!(
        model
            .unsupported
            .iter()
            .any(|u| u.kind == "animation curve on a missing object"),
        "the empty clip's curve on a missing object is not reported: {:?}",
        model.unsupported
    );
}

fn fx(avatar: &mut avatar_core::Avatar) -> &mut avatar_core::model::Controller {
    avatar
        .model
        .layers
        .get_mut(&PlayableLayerType::Fx)
        .and_then(|layer| layer.controller.as_mut())
        .unwrap()
}

/// FX with a third layer whose one state, 9000, no other layer declares.
fn with_foreign_state() -> avatar_core::Avatar {
    let mut avatar = fixture::avatar();
    let controller = fx(&mut avatar);
    let mut foreign = controller.layers[0].clone();
    let LayerContent::StateMachine { state_machine } = &mut foreign.content else {
        unreachable!()
    };
    state_machine.id = avatar_core::model::StateMachineId(9000);
    state_machine.state_machines.clear();
    state_machine.states.truncate(1);
    state_machine.states[0].id = avatar_core::model::StateId(9000);
    state_machine.states[0].transitions.clear();
    state_machine.any_state_transitions.clear();
    state_machine.entry_transitions.clear();
    state_machine.state_machine_transitions.clear();
    state_machine.default_state = Some(avatar_core::model::StateId(9000));
    controller.layers.push(foreign);
    write(&avatar).unwrap();
    avatar
}

fn refused_at(avatar: &avatar_core::Avatar, suffix: &str) {
    match write(avatar) {
        Err(Error::Invalid(invalid)) => assert!(
            invalid.at.ends_with(suffix),
            "{} ({})",
            invalid.at,
            invalid.reason
        ),
        other => panic!("not refused at {suffix}: {other:?}"),
    }
}

#[test]
fn states_are_reached_only_within_their_layer() {
    use avatar_core::model::{Destination, StateId};
    let mut avatar = with_foreign_state();
    let LayerContent::StateMachine { state_machine } = &mut fx(&mut avatar).layers[0].content
    else {
        unreachable!()
    };
    state_machine.states[0].transitions[0].destination = Destination::State { id: StateId(9000) };
    refused_at(&avatar, "/destination/id");

    let mut avatar = with_foreign_state();
    let LayerContent::StateMachine { state_machine } = &mut fx(&mut avatar).layers[0].content
    else {
        unreachable!()
    };
    state_machine.default_state = Some(StateId(9000));
    refused_at(&avatar, "/defaultState");

    let mut avatar = with_foreign_state();
    let synced = fx(&mut avatar)
        .layers
        .iter_mut()
        .find_map(|layer| match &mut layer.content {
            LayerContent::Synced { states, .. } => Some(states),
            _ => None,
        })
        .unwrap();
    synced[0].state = StateId(9000);
    refused_at(&avatar, "/states/0/state");
}

#[test]
fn an_animator_layer_control_reaches_an_existing_layer() {
    use avatar_core::model::Behaviour;
    let mut avatar = fixture::avatar();
    let layers = fx(&mut avatar).layers.len() as u32;
    for behaviour in &mut avatar.model.behaviours {
        if let Behaviour::AnimatorLayerControl { layer, .. } = behaviour {
            *layer = layers;
        }
    }
    refused_at(&avatar, "/layer");
}

#[test]
fn a_number_past_f32_is_refused_on_read() {
    let bytes = write(&fixture::avatar()).unwrap();
    let huge = with_json(&bytes, |json| {
        json["extensions"]["AW_avatar"]["descriptor"]["viewPosition"][0] = json!(1e39);
    });
    assert!(matches!(read(&huge), Err(Error::Unrepresentable)));
}

#[test]
fn a_malformed_extensions_used_is_refused_on_read() {
    let bytes = write(&fixture::avatar()).unwrap();
    let mut doc = json_chunk(&bytes);
    doc["extensionsUsed"] = json!("AW_avatar");
    let odd = with_json_chunk(bytes, &doc);
    assert!(matches!(read(&odd), Err(Error::Root(_))));
}

#[test]
fn renderers_match_their_meshes() {
    let mut avatar = fixture::avatar();
    let renderer = avatar.model.renderers[0].clone();
    avatar.model.renderers.push(renderer);
    refused_at(&avatar, "/renderers/1/node");

    let mut avatar = fixture::avatar();
    let slot = avatar.model.renderers[0].materials[0];
    avatar.model.renderers[0].materials.push(slot);
    refused_at(&avatar, "/renderers/0/materials");
}

#[test]
fn a_slot_curve_names_an_existing_slot() {
    let mut avatar = fixture::avatar();
    let curve = avatar
        .model
        .clips
        .iter_mut()
        .find_map(|clip| clip.slot_curves.first_mut())
        .unwrap();
    curve.slot = 1_000_000;
    refused_at(&avatar, "/slot");
}

#[test]
fn a_synced_layer_follows_another_state_machine_layer() {
    let mut avatar = fixture::avatar();
    let controller = fx(&mut avatar);
    let (index, _) = controller
        .layers
        .iter()
        .enumerate()
        .find(|(_, layer)| matches!(layer.content, LayerContent::Synced { .. }))
        .unwrap();
    let LayerContent::Synced { layer, .. } = &mut controller.layers[index].content else {
        unreachable!()
    };
    *layer = index as u32;
    refused_at(&avatar, "/content/layer");

    let mut avatar = fixture::avatar();
    let controller = fx(&mut avatar);
    let synced = controller.layers[index].clone();
    controller.layers.push(synced);
    let last = controller.layers.len() - 1;
    let LayerContent::Synced { layer, .. } = &mut controller.layers[last].content else {
        unreachable!()
    };
    *layer = index as u32;
    refused_at(&avatar, "/content/layer");
}
