mod vrm_harness;

use std::path::PathBuf;

use avatar_core::model::{HumanBodyBone, LipSync, MaterialProperty, ShaderFamily, SourceFormat};
use avatar_core::physbones::Nodes;
use avatar_core::{glb, read, vrm as importer, write, Avatar};
use glam::Vec3;
use serde_json::Value;

const FIXTURES: [&str; 4] = ["crimsom-0.x", "crimsom-1.0", "godette-0.x", "twist-1.0"];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/vrm").join(name)
}

fn source(name: &str) -> (serde_json::Map<String, Value>, Vec<u8>) {
    glb::parts(&std::fs::read(fixture(&format!("{name}.vrm"))).unwrap()).unwrap()
}

fn import(name: &str) -> Avatar {
    importer::import(&std::fs::read(fixture(&format!("{name}.vrm"))).unwrap()).unwrap()
}

#[test]
fn every_fixture_imports_and_round_trips() {
    for name in FIXTURES {
        let avatar = import(name);
        let again = read(&write(&avatar).unwrap()).unwrap();
        assert_eq!(again.model, avatar.model, "{name}");
        let used = avatar.gltf.get("extensionsUsed").and_then(Value::as_array).cloned().unwrap_or_default();
        assert!(used.iter().all(|e| !e.as_str().unwrap().starts_with("VRM")), "{name} keeps a VRM extension: {used:?}");
        let format = if name.ends_with("0.x") { SourceFormat::Vrm0 } else { SourceFormat::Vrm1 };
        assert_eq!(avatar.model.source.format, format, "{name}");
        assert!(avatar.model.humanoid.bones.len() >= 15, "{name}: {} humanoid bones", avatar.model.humanoid.bones.len());
    }
}

#[test]
fn not_a_vrm_is_refused() {
    let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/synthetic.glb")).unwrap();
    assert!(matches!(importer::import(&bytes), Err(importer::Error::NotVrm)));
}

/// The same local turn on the elbows and knees of the source and of the model skins every vertex to the same
/// place, a VRM 0.x one turned half about Y to face +Z, and every blendshape keeps its targets and its name.
#[test]
fn skinning_and_blendshapes_survive() {
    for name in FIXTURES {
        let avatar = import(name);
        let (src, bin) = source(name);
        let (worst, moved) = vrm_harness::bend(&src, &bin, &avatar);
        assert!(worst < 4e-5, "{name}: a skinned vertex is {worst} m from the source's");
        assert!(moved > 0.05, "{name}: the bend moved no vertex past {moved} m");
        for (m, mesh) in src["meshes"].as_array().unwrap().iter().enumerate() {
            let targets = |mesh: &Value| mesh["primitives"].as_array().unwrap().iter().map(|p| p["targets"].clone()).collect::<Vec<_>>();
            let model_mesh = &avatar.gltf["meshes"][m];
            assert_eq!(targets(mesh), targets(model_mesh), "{name} mesh {m}");
            let count = mesh["primitives"][0]["targets"].as_array().map_or(0, Vec::len);
            assert_eq!(model_mesh["extras"]["targetNames"].as_array().map_or(0, Vec::len), count, "{name} mesh {m} target names");
        }
        assert_eq!(src["accessors"], avatar.gltf["accessors"], "{name}");
        assert_eq!(src["skins"], avatar.gltf["skins"], "{name}");
        assert!(avatar.bin == bin, "{name}: the binary chunk changed");
    }
}

/// Each fixture's spring tips, moved through the core's PhysBones, stay within 5% of their path length of
/// three-vrm's under the same motion of the avatar and its head.
#[test]
fn springs_follow_three_vrm() {
    for name in ["godette-0.x", "twist-1.0"] {
        let avatar = import(name);
        let recording = vrm_harness::parse(&std::fs::read_to_string(fixture(&format!("{name}.springs.tsv"))).unwrap());
        let replayed = vrm_harness::replay(&avatar, &recording, vrm_harness::head(&avatar));
        let errors = vrm_harness::errors(&recording, &replayed);
        assert!(!errors.is_empty(), "{name} has no recorded tips");
        for (tip, e) in errors {
            assert!(e <= 0.05, "{name}: tip node {tip} is {e:.4} of its length from three-vrm's");
        }
        for (tip, e) in vrm_harness::worst(&recording, &replayed) {
            assert!(e <= 0.05, "{name}: tip node {tip} is {e:.4} of its length from three-vrm's on some frame");
        }
        let still = recording.at.iter().zip(&recording.at[1..]).all(|(a, b)| a == b);
        assert!(!still, "{name}: the recording does not move");
    }
}

fn srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Every field of each VRM 1.0 MToon material, and each MToon 0.x saved property, is in its material record.
#[test]
fn every_mtoon_property_survives() {
    let fields: [(&str, &str); 25] = [
        ("transparentWithZWrite", "_TransparentWithZWrite"),
        ("renderQueueOffsetNumber", "_RenderQueueOffset"),
        ("shadeColorFactor", "_ShadeColor"),
        ("shadeMultiplyTexture", "_ShadeTex"),
        ("shadingShiftFactor", "_ShadingShiftFactor"),
        ("shadingShiftTexture", "_ShadingShiftTex"),
        ("shadingToonyFactor", "_ShadingToonyFactor"),
        ("giEqualizationFactor", "_GiEqualization"),
        ("matcapFactor", "_MatcapColor"),
        ("matcapTexture", "_MatcapTex"),
        ("parametricRimColorFactor", "_RimColor"),
        ("rimMultiplyTexture", "_RimTex"),
        ("rimLightingMixFactor", "_RimLightingMix"),
        ("parametricRimFresnelPowerFactor", "_RimFresnelPower"),
        ("parametricRimLiftFactor", "_RimLift"),
        ("outlineWidthMode", "_OutlineWidthMode"),
        ("outlineWidthFactor", "_OutlineWidth"),
        ("outlineWidthMultiplyTexture", "_OutlineWidthTex"),
        ("outlineColorFactor", "_OutlineColor"),
        ("outlineLightingMixFactor", "_OutlineLightingMix"),
        ("uvAnimationMaskTexture", "_UvAnimMaskTex"),
        ("uvAnimationScrollXSpeedFactor", "_UvAnimScrollXSpeed"),
        ("uvAnimationScrollYSpeedFactor", "_UvAnimScrollYSpeed"),
        ("uvAnimationRotationSpeedFactor", "_UvAnimRotationSpeed"),
        ("specVersion", ""),
    ];
    let mut checked = 0;
    for name in ["crimsom-1.0", "twist-1.0"] {
        let avatar = import(name);
        let (src, _) = source(name);
        for (i, m) in src["materials"].as_array().unwrap().iter().enumerate() {
            let Some(Value::Object(x)) = m.pointer("/extensions/VRMC_materials_mtoon") else { continue };
            let record = &avatar.model.materials[i];
            assert_eq!(record.shader, ShaderFamily::Mtoon);
            for (key, value) in x {
                let property = fields.iter().find(|f| f.0 == key).unwrap_or_else(|| panic!("{name} material {i}: MToon field {key} has no property")).1;
                if property.is_empty() {
                    continue;
                }
                let got = record.properties.get(property).unwrap_or_else(|| panic!("{name} material {i}: no {property}"));
                checked += 1;
                match (value, got) {
                    (Value::Number(n), MaterialProperty::Float(f)) => assert_eq!(*f, n.as_f64().unwrap() as f32, "{name} {i} {key}"),
                    (Value::Bool(b), MaterialProperty::Float(f)) => assert_eq!(*f, *b as u8 as f32, "{name} {i} {key}"),
                    (Value::String(_), MaterialProperty::Float(_)) => {}
                    (Value::Array(a), MaterialProperty::Color(c)) => {
                        for k in 0..3 {
                            assert!((c[k] - srgb(a[k].as_f64().unwrap() as f32)).abs() < 1e-6, "{name} {i} {key}");
                        }
                    }
                    (Value::Object(t), MaterialProperty::Texture(p)) => assert_eq!(p.texture.map(|t| t.0 as u64), t["index"].as_u64(), "{name} {i} {key}"),
                    other => panic!("{name} material {i}: {key} became {other:?}"),
                }
            }
            for (key, property) in [("normalTexture", "_BumpMap"), ("emissiveTexture", "_EmissionMap")] {
                if let Some(t) = m.get(key) {
                    let MaterialProperty::Texture(p) = &record.properties[property] else { panic!() };
                    assert_eq!(p.texture.map(|t| t.0 as u64), t["index"].as_u64());
                    checked += 1;
                }
            }
            let emissive = m.get("emissiveFactor").and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap() as f32).collect::<Vec<_>>()).unwrap_or(vec![0.0; 3]);
            let strength = m.pointer("/extensions/KHR_materials_emissive_strength/emissiveStrength").and_then(Value::as_f64).unwrap_or(1.0) as f32;
            let MaterialProperty::Color(e) = record.properties["_EmissionColor"] else { panic!() };
            assert_eq!(&e[..3], &emissive.iter().map(|x| x * strength).collect::<Vec<_>>()[..], "{name} {i} emission");
            let mode = m.get("alphaMode").and_then(Value::as_str).unwrap_or("OPAQUE");
            assert_eq!(record.properties["_AlphaMode"], MaterialProperty::Float(["OPAQUE", "MASK", "BLEND"].iter().position(|x| *x == mode).unwrap() as f32));
            assert_eq!(record.blend.is_some(), mode == "BLEND", "{name} {i}");
            let sided = m.get("doubleSided").and_then(Value::as_bool) == Some(true);
            assert_eq!(record.cull == avatar_core::model::Cull::Off, sided, "{name} {i}");
            checked += 3;
        }
    }
    for name in ["crimsom-0.x", "godette-0.x"] {
        let avatar = import(name);
        let (src, _) = source(name);
        for entry in src["extensions"]["VRM"]["materialProperties"].as_array().unwrap() {
            if entry["shader"] != "VRM/MToon" {
                continue;
            }
            let i = src["materials"].as_array().unwrap().iter().position(|m| m["name"] == entry["name"]).unwrap();
            let record = &avatar.model.materials[i];
            assert_eq!((record.shader, record.shader_name.as_str()), (ShaderFamily::Mtoon, "VRM/MToon"));
            for (k, v) in entry["floatProperties"].as_object().unwrap() {
                assert_eq!(record.properties[k], MaterialProperty::Float(v.as_f64().unwrap() as f32), "{name} {k}");
                checked += 1;
            }
            let textures = entry["textureProperties"].as_object().unwrap();
            for (k, v) in textures {
                let MaterialProperty::Texture(t) = &record.properties[k] else { panic!("{name} {k}") };
                assert_eq!(t.texture.map(|t| t.0 as u64), v.as_u64(), "{name} {k}");
                checked += 1;
            }
            for (k, v) in entry["vectorProperties"].as_object().unwrap() {
                let v: Vec<f32> = v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect();
                match &record.properties.get(k) {
                    Some(MaterialProperty::Color(c) | MaterialProperty::Vector(c)) => assert_eq!(&c[..], &v[..], "{name} {k}"),
                    Some(MaterialProperty::Texture(t)) => assert_eq!([t.tiling, t.offset].concat(), v, "{name} {k}"),
                    None if k == "_MainTex" => {}
                    other => panic!("{name}: {k} became {other:?}"),
                }
                checked += 1;
            }
            for (k, on) in entry["keywordMap"].as_object().unwrap() {
                assert_eq!(record.keywords.contains(k), on.as_bool().unwrap(), "{name} keyword {k}");
            }
        }
    }
    assert!(checked > 100, "only {checked} properties checked");
}

#[test]
fn faces_map_to_the_descriptor_and_clips() {
    for name in FIXTURES {
        let avatar = import(name);
        let d = &avatar.model.descriptor;
        let LipSync::VisemeBlendShape { visemes, .. } = &d.lip_sync else { panic!("{name}: {:?}", d.lip_sync) };
        assert!([&visemes.aa, &visemes.e, &visemes.ih, &visemes.oh, &visemes.ou].iter().all(|v| !v.is_empty()), "{name}");
        let head = avatar.model.humanoid.bones[&HumanBodyBone::Head].node.0 as usize;
        let world = { let n = Nodes::read(&avatar).unwrap(); n.world(&n.rest) };
        let view = Vec3::from(d.view_position);
        assert!((view - world[head].translation).length() < 0.3, "{name}: view {view} far from the head");
        assert!(view.y > 0.5, "{name}: view {view}");
        let names: Vec<&str> = avatar.model.clips.iter().map(|c| c.name.as_str()).collect();
        for preset in ["aa", "ih", "ou", "ee", "oh"] {
            assert!(names.contains(&preset), "{name} has no {preset} clip: {names:?}");
        }
        let blinks = if name.starts_with("crimsom-0") { ["blinkLeft", "blinkRight"].as_slice() } else { ["blink"].as_slice() };
        for b in blinks {
            assert!(names.contains(b), "{name} has no {b} clip: {names:?}");
        }
        let lids = matches!(&d.eyes.eyelids, avatar_core::model::Eyelids::BlendShapes { blink: Some(_), .. });
        assert_eq!(lids, name == "twist-1.0", "{name}: {:?}", d.eyes.eyelids);
        let look = d.eyes.look.is_some();
        assert_eq!(look, name == "twist-1.0", "{name}: eye look, which needs eye bones");
    }
}

/// The model faces +Z, so an avatar's left is +X, for VRM 0.x and 1.0 alike.
#[test]
fn every_fixture_faces_forward() {
    for name in FIXTURES {
        let avatar = import(name);
        let world = { let n = Nodes::read(&avatar).unwrap(); n.world(&n.rest) };
        let bones = &avatar.model.humanoid.bones;
        let at = |b: HumanBodyBone| world[bones[&b].node.0 as usize].translation;
        let left = at(HumanBodyBone::LeftUpperArm).x;
        let right = at(HumanBodyBone::RightUpperArm).x;
        assert!(left > right, "{name}: its left arm is not at +X");
    }
}

#[test]
fn unsupported_vrm_features_are_reported() {
    let count = |a: &Avatar, kind: &str| a.model.unsupported.iter().find(|u| u.kind == kind).map_or(0, |u| u.count);
    let twist = import("twist-1.0");
    let (src, _) = source("twist-1.0");
    for kind in ["aim", "roll", "rotation"] {
        let n = src["nodes"].as_array().unwrap().iter().filter(|n| n.pointer(&format!("/extensions/VRMC_node_constraint/constraint/{kind}")).is_some()).count() as u32;
        assert_eq!(count(&twist, &format!("vrm node constraint: {kind}")), n, "{kind}");
    }
    assert!(count(&twist, "vrm node constraint: aim") > 0);
    let godette = import("godette-0.x");
    assert_eq!(count(&godette, "vrm look-at by expression"), 1);
}

fn glb_of(gltf: &serde_json::Map<String, Value>, bin: &[u8]) -> Vec<u8> {
    let mut json = serde_json::to_vec(gltf).unwrap();
    json.resize(json.len().next_multiple_of(4), b' ');
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(&json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}

/// Players supply these files: each malformed variant is refused with a reason, none panics, and an unchanged
/// copy written the same way imports.
#[test]
fn malformed_files_are_refused() {
    let (src, bin) = source("twist-1.0");
    let mut bin = bin;
    bin.resize(bin.len().next_multiple_of(4), 0);
    assert!(importer::import(&glb_of(&src, &bin)).is_ok());
    let edits: Vec<(&str, Box<dyn Fn(&mut Value)>)> = vec![
        ("huge stiffness", Box::new(|j| j["extensions"]["VRMC_springBone"]["springs"][0]["joints"][0]["stiffness"] = serde_json::json!(1e39))),
        ("huge collider radius", Box::new(|j| j["extensions"]["VRMC_springBone"]["colliders"][0]["shape"]["sphere"]["radius"] = serde_json::json!(1e39))),
        ("huge translation", Box::new(|j| j["nodes"][3]["translation"] = serde_json::json!([1e39, 0, 0]))),
        ("no humanoid", Box::new(|j| _ = j["extensions"]["VRMC_vrm"].as_object_mut().unwrap().remove("humanoid"))),
        ("bone on a missing node", Box::new(|j| j["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"]["hips"]["node"] = serde_json::json!(100000))),
        ("unknown bone", Box::new(|j| j["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"][""] = serde_json::json!({ "node": 1 }))),
        ("missing collider group", Box::new(|j| j["extensions"]["VRMC_springBone"]["springs"][0]["colliderGroups"] = serde_json::json!([999]))),
        ("missing collider", Box::new(|j| j["extensions"]["VRMC_springBone"]["colliderGroups"][0]["colliders"] = serde_json::json!([999]))),
        ("missing morph target", Box::new(|j| j["extensions"]["VRMC_vrm"]["expressions"]["preset"]["aa"]["morphTargetBinds"][0]["index"] = serde_json::json!(9999))),
        ("missing texture", Box::new(|j| j["materials"][0]["extensions"]["VRMC_materials_mtoon"]["shadeMultiplyTexture"] = serde_json::json!({ "index": 9999 }))),
        ("node cycle", Box::new(|j| {
            let parent = j["nodes"].as_array().unwrap().iter().position(|n| n["children"].as_array().is_some_and(|c| !c.is_empty())).unwrap();
            let child = j["nodes"][parent]["children"][0].as_u64().unwrap() as usize;
            j["nodes"][child]["children"] = serde_json::json!([parent]);
        })),
    ];
    for (what, edit) in edits {
        let mut json = Value::Object(src.clone());
        edit(&mut json);
        let Value::Object(gltf) = json else { unreachable!() };
        let result = std::panic::catch_unwind(|| importer::import(&glb_of(&gltf, &bin)));
        match result {
            Ok(Err(e)) => assert!(!e.to_string().is_empty(), "{what}"),
            Ok(Ok(_)) => panic!("{what} imported"),
            Err(_) => panic!("{what} panicked"),
        }
    }
}

#[test]
fn bones_slots_and_target_names_follow_the_file() {
    for name in ["crimsom-1.0", "twist-1.0"] {
        let avatar = import(name);
        let (src, _) = source(name);
        let human = &src["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"];
        for (side, s) in [(HumanBodyBone::LeftThumbProximal, "leftThumbMetacarpal"), (HumanBodyBone::LeftThumbIntermediate, "leftThumbProximal"), (HumanBodyBone::LeftThumbDistal, "leftThumbDistal"), (HumanBodyBone::RightThumbProximal, "rightThumbMetacarpal"), (HumanBodyBone::RightThumbIntermediate, "rightThumbProximal"), (HumanBodyBone::RightThumbDistal, "rightThumbDistal")] {
            let Some(node) = human[s]["node"].as_u64() else { continue };
            assert_eq!(avatar.model.humanoid.bones[&side].node.0 as u64, node, "{name} {s}");
        }
    }
    for name in FIXTURES {
        let avatar = import(name);
        let (src, _) = source(name);
        for r in &avatar.model.renderers {
            let mesh = src["nodes"][r.node.0 as usize]["mesh"].as_u64().unwrap() as usize;
            let slots: Vec<u64> = src["meshes"][mesh]["primitives"].as_array().unwrap().iter().map(|p| p["material"].as_u64().unwrap()).collect();
            assert_eq!(r.materials.iter().map(|m| m.0 as u64).collect::<Vec<_>>(), slots, "{name} renderer {}", r.node.0);
        }
        for (m, mesh) in src["meshes"].as_array().unwrap().iter().enumerate() {
            let given = mesh["extras"].get("targetNames").or(mesh.pointer("/primitives/0/extras/targetNames"));
            if let Some(given) = given {
                assert_eq!(&avatar.gltf["meshes"][m]["extras"]["targetNames"], given, "{name} mesh {m}");
            }
        }
    }
}

#[test]
fn texture_transforms_flip_to_unity_v() {
    let (mut src, bin) = source("twist-1.0");
    let mut bin = bin;
    bin.resize(bin.len().next_multiple_of(4), 0);
    let texture = src["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["index"].clone();
    src["materials"].as_array_mut().unwrap()[0]["pbrMetallicRoughness"]["baseColorTexture"] =
        serde_json::json!({ "index": texture, "extensions": { "KHR_texture_transform": { "offset": [0.25, 0.125], "scale": [0.5, 0.25] } } });
    let avatar = importer::import(&glb_of(&src, &bin)).unwrap();
    let MaterialProperty::Texture(t) = &avatar.model.materials[0].properties["_MainTex"] else { panic!() };
    assert_eq!((t.tiling, t.offset), ([0.5, 0.25], [0.25, 0.625]));
}

/// A bone without stretch that a collider pushes keeps its length: every simulated bone's local offset stays
/// its rest length through twist's colliding hair.
#[test]
fn collided_bones_keep_their_length() {
    use avatar_core::physbones::{Frame, PhysBones, STEP};
    let avatar = import("twist-1.0");
    let recording = vrm_harness::parse(&std::fs::read_to_string(fixture("twist-1.0.springs.tsv")).unwrap());
    let nodes = Nodes::read(&avatar).unwrap();
    let head = vrm_harness::head(&avatar).unwrap();
    let mut pb = PhysBones::new(&avatar).unwrap();
    let mut worst = 0.0f32;
    for (f, root) in recording.roots.iter().enumerate() {
        let mut locals = nodes.rest.clone();
        locals[head].rotation = recording.heads[f];
        let frame = Frame { root: *root, nodes: nodes.world(&locals), locals: locals.clone(), active: vec![true; locals.len()], enabled: vec![true; avatar.model.physbones.len()] };
        pb.advance(if f == 0 { 0.0 } else { STEP }, &frame);
        for p in 0..avatar.model.physbones.len() {
            for (n, t) in pb.bones(p).collect::<Vec<_>>().into_iter().zip(pb.local(p)).skip(1) {
                worst = worst.max((t.translation.length() - nodes.rest[n].translation.length()).abs());
            }
        }
    }
    assert!(worst < 1e-5, "a simulated bone's length moved {worst} m from its rest");
}
