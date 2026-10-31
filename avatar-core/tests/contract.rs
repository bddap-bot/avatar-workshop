mod fixture;

use std::path::PathBuf;

use avatar_core::model::{LayerContent, PlayableLayerType};
use avatar_core::{read, schema, write, Error};

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
    let mut out = write(&avatar).unwrap();
    let mut doc = json_chunk(&out);
    doc["extensions"]["AW_avatar"] = model;
    let mut text = serde_json::to_vec(&doc).unwrap();
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
    assert!(matches!(write(&avatar), Err(Error::Unwritable(_))));
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
    assert!(matches!(write(&avatar), Err(Error::Mismatch("nodes"))));
    let mut avatar = fixture::avatar();
    avatar.model.materials.pop();
    assert!(matches!(write(&avatar), Err(Error::Mismatch("materials"))));
    let bytes = write(&fixture::avatar()).unwrap();
    let short = with_json(&bytes, |json| {
        json["extensions"]["AW_avatar"]["nodes"]
            .as_array_mut()
            .unwrap()
            .pop();
    });
    assert!(matches!(read(&short), Err(Error::Mismatch("nodes"))));
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
        "the FX default state keeps the hat off"
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
