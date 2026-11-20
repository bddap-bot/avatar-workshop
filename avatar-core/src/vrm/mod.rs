//! VRM 0.x and 1.0 into the model. Every VRM 0.x difference is resolved here: it faces -Z, numbers its thumb
//! bones from the metacarpal as Unity does, names its expression presets differently, binds expressions by mesh,
//! stores MToon as Unity material properties and its collider offsets with Z reversed.

mod materials;
mod springs;

use std::collections::BTreeMap;
use std::fmt;

use glam::{Quat, Vec3};
use serde_json::{Map, Value};

use crate::glb::{self, Avatar};
use crate::model::{
    AnimationCurve, AwAvatar, Binding, Clip, Contacts, Curve, Descriptor, DescriptorCollider, DescriptorColliderState,
    DescriptorColliders, EyeLook, EyeMovement, EyeRotations, Eyelids, Eyes, HumanBodyBone, HumanBone, Humanoid,
    Keyframe, LipSync, MaterialIndex, Menu, MuscleLimit, Node, NodeIndex, Renderer, SkeletonBone, Source, SourceFormat,
    Unsupported, Visemes, WeightedMode,
};
use crate::physbones::{Nodes, Transform};

pub use springs::rest_world;

#[derive(Debug)]
pub enum Error {
    Glb(glb::Error),
    NotVrm,
    Malformed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Glb(e) => write!(f, "{e}"),
            Error::NotVrm => write!(f, "no VRM or VRMC_vrm extension on the glTF root"),
            Error::Malformed(what) => write!(f, "VRM: {what}"),
        }
    }
}

impl std::error::Error for Error {}

fn malformed(what: impl Into<String>) -> Error {
    Error::Malformed(what.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Version {
    V0,
    V1,
}

/// What the file holds that the model cannot, by kind.
#[derive(Default)]
struct Report(BTreeMap<String, u32>);

impl Report {
    fn add(&mut self, kind: impl Into<String>) {
        *self.0.entry(kind.into()).or_default() += 1;
    }
}

/// The source document while it is converted: its glTF, the node tree at rest and what is reported.
struct Doc<'a> {
    gltf: &'a Map<String, Value>,
    vrm: &'a Value,
    version: Version,
    nodes: Nodes,
    world: Vec<Transform>,
    report: Report,
}

impl Doc<'_> {
    fn array(&self, key: &str) -> &[Value] {
        self.gltf.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
    }

    fn node(&self, value: Option<&Value>, what: &str) -> Result<NodeIndex, Error> {
        let i = value.and_then(Value::as_u64).ok_or_else(|| malformed(format!("{what} names no node")))?;
        if i as usize >= self.nodes.rest.len() {
            return Err(malformed(format!("{what} names node {i}, but there are {}", self.nodes.rest.len())));
        }
        Ok(NodeIndex(i as u32))
    }

    /// The nodes that draw mesh `mesh`.
    fn mesh_nodes(&self, mesh: u64) -> Vec<NodeIndex> {
        self.array("nodes")
            .iter()
            .enumerate()
            .filter(|(_, n)| n.get("mesh").and_then(Value::as_u64) == Some(mesh))
            .map(|(i, _)| NodeIndex(i as u32))
            .collect()
    }

    fn target_name(&self, node: NodeIndex, index: u64) -> Result<String, Error> {
        let mesh = self.array("nodes")[node.0 as usize].get("mesh").and_then(Value::as_u64);
        mesh.and_then(|m| self.array("meshes").get(m as usize))
            .and_then(|m| m.pointer(&format!("/extras/targetNames/{index}")))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| malformed(format!("node {} has no morph target {index}", node.0)))
    }
}

pub(crate) fn number(v: Option<&Value>, default: f32) -> f32 {
    v.and_then(Value::as_f64).map_or(default, |x| x as f32)
}

pub(crate) fn floats<const N: usize>(v: Option<&Value>) -> Option<[f32; N]> {
    let a = v?.as_array()?;
    if a.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (o, x) in out.iter_mut().zip(a) {
        *o = x.as_f64()? as f32;
    }
    Some(out)
}

/// A VRM 0.x `{x, y, z}` object.
fn xyz(v: Option<&Value>) -> Option<Vec3> {
    let v = v?;
    Some(Vec3::new(number(v.get("x"), 0.0), number(v.get("y"), 0.0), number(v.get("z"), 0.0)))
}

/// Reads a VRM 0.x or 1.0 file into the model.
pub fn import(bytes: &[u8]) -> Result<Avatar, Error> {
    let (mut gltf, bin) = glb::parts(bytes).map_err(Error::Glb)?;
    let exts = gltf.get("extensions").cloned().unwrap_or(Value::Null);
    let (version, vrm) = match (exts.get("VRMC_vrm"), exts.get("VRM")) {
        (Some(v), _) => (Version::V1, v.clone()),
        (None, Some(v)) => (Version::V0, v.clone()),
        (None, None) => return Err(Error::NotVrm),
    };
    if version == Version::V0 {
        face_forward(&mut gltf);
    }
    let mut report = Report::default();
    name_targets(&mut gltf, &mut report);
    let primitives_without_material = default_material(&mut gltf);
    let nodes = Nodes::of(&gltf).map_err(|e| malformed(e.to_string()))?;
    let world = rest_world(&nodes);
    let mut doc = Doc { gltf: &gltf, vrm: &vrm, version, nodes, world, report };

    let humanoid = humanoid(&doc)?;
    let clips = expressions(&mut doc)?;
    let descriptor = descriptor(&mut doc, &humanoid, &clips)?;
    let materials = materials::materials(&mut doc)?;
    let (physbones, colliders) = springs::springs(&mut doc, &exts)?;
    first_person(&mut doc);
    let constraints: Vec<String> = doc
        .array("nodes")
        .iter()
        .filter_map(|n| n.pointer("/extensions/VRMC_node_constraint/constraint")?.as_object())
        .flat_map(|c| c.keys().cloned())
        .collect();
    for kind in constraints {
        doc.report.add(format!("vrm node constraint: {kind}"));
    }
    let renderers = doc
        .array("nodes")
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let mesh = doc.array("meshes").get(n.get("mesh")?.as_u64()? as usize)?;
            let slots = mesh.get("primitives")?.as_array()?;
            Some(Renderer {
                node: NodeIndex(i as u32),
                enabled: true,
                materials: slots.iter().map(|p| MaterialIndex(p.get("material").and_then(Value::as_u64).unwrap_or(0) as u32)).collect(),
            })
        })
        .collect();
    let node_count = doc.nodes.rest.len();
    let source = source(&doc);
    let mut unsupported: Vec<Unsupported> = doc.report.0.into_iter().map(|(kind, count)| Unsupported { kind, count }).collect();
    if primitives_without_material > 0 {
        unsupported.push(Unsupported { kind: "glTF primitive without a material, given glTF's default".into(), count: primitives_without_material });
    }
    let model = AwAvatar {
        source,
        humanoid,
        descriptor,
        renderers,
        nodes: vec![Node { active: true }; node_count],
        materials,
        physbones,
        colliders,
        contacts: Contacts { senders: vec![], receivers: vec![] },
        constraints: vec![],
        parameters: vec![],
        menu: Menu { controls: vec![] },
        layers: BTreeMap::new(),
        behaviours: vec![],
        clips,
        unsupported,
    };
    strip_vrm(&mut gltf);
    let avatar = Avatar { gltf, bin, model };
    crate::validate::validate(&avatar.gltf, &avatar.model).map_err(|e| malformed(format!("{}: {}", e.at, e.reason)))?;
    Ok(avatar)
}

/// VRM 0.x faces -Z; the model faces +Z, so its scene's root nodes turn half a turn about Y.
fn face_forward(gltf: &mut Map<String, Value>) {
    let half = Quat::from_xyzw(0.0, 1.0, 0.0, 0.0);
    let Some(Value::Array(nodes)) = gltf.get_mut("nodes") else { return };
    let mut child = vec![false; nodes.len()];
    for n in nodes.iter() {
        for c in n.get("children").and_then(Value::as_array).into_iter().flatten() {
            if let Some(c) = c.as_u64().filter(|&c| (c as usize) < child.len()) {
                child[c as usize] = true;
            }
        }
    }
    for (n, is_child) in nodes.iter_mut().zip(child) {
        let Value::Object(n) = n else { continue };
        if is_child {
            continue;
        }
        if let Some(m) = floats::<16>(n.get("matrix")) {
            let turned = glam::Mat4::from_quat(half) * glam::Mat4::from_cols_slice(&m);
            n.insert("matrix".into(), Value::from(turned.to_cols_array().to_vec()));
            continue;
        }
        let t = floats::<3>(n.get("translation")).map_or(Vec3::ZERO, Vec3::from);
        let r = floats::<4>(n.get("rotation")).map_or(Quat::IDENTITY, Quat::from_array);
        n.insert("translation".into(), Value::from((half * t).to_array().to_vec()));
        n.insert("rotation".into(), Value::from((half * r).normalize().to_array().to_vec()));
    }
}

/// The model names blendshapes by the mesh's `extras.targetNames`; VRM exporters put them there or on the
/// first primitive. A mesh with neither gets its targets' indices as names.
fn name_targets(gltf: &mut Map<String, Value>, report: &mut Report) {
    let Some(Value::Array(meshes)) = gltf.get_mut("meshes") else { return };
    for mesh in meshes {
        let Value::Object(mesh) = mesh else { continue };
        let targets = mesh
            .get("primitives")
            .and_then(Value::as_array)
            .and_then(|p| p.first())
            .and_then(|p| p.get("targets"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if targets == 0 || mesh.get("extras").and_then(|e| e.get("targetNames")).is_some() {
            continue;
        }
        let from_primitive = mesh.get("primitives").and_then(|p| p.pointer("/0/extras/targetNames")).filter(|n| n.as_array().is_some_and(|a| a.len() == targets)).cloned();
        let names = from_primitive.unwrap_or_else(|| {
            report.add("vrm mesh without morph target names, named by index");
            Value::from((0..targets).map(|i| i.to_string()).collect::<Vec<_>>())
        });
        let extras = mesh.entry("extras").or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(extras) = extras {
            extras.insert("targetNames".into(), names);
        }
    }
}

/// Gives a primitive without a material glTF's default one, so every renderer slot names a material.
fn default_material(gltf: &mut Map<String, Value>) -> u32 {
    let missing = gltf
        .get("meshes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|m| m.get("primitives").and_then(Value::as_array).into_iter().flatten())
        .filter(|p| p.get("material").is_none())
        .count() as u32;
    if missing == 0 {
        return 0;
    }
    let Value::Array(materials) = gltf.entry("materials").or_insert_with(|| Value::Array(vec![])) else { return 0 };
    let index = materials.len();
    materials.push(serde_json::json!({ "name": "glTF default", "pbrMetallicRoughness": { "metallicFactor": 1.0, "roughnessFactor": 1.0 } }));
    if let Some(Value::Array(meshes)) = gltf.get_mut("meshes") {
        for m in meshes {
            for p in m.get_mut("primitives").and_then(Value::as_array_mut).into_iter().flatten() {
                if let Value::Object(p) = p {
                    p.entry("material").or_insert(Value::from(index));
                }
            }
        }
    }
    missing
}

/// Drops every VRM extension, whose facts the model now holds.
fn strip_vrm(gltf: &mut Map<String, Value>) {
    let vrm = |name: &str| name == "VRM" || name.starts_with("VRMC_");
    for key in ["extensionsUsed", "extensionsRequired"] {
        if let Some(Value::Array(used)) = gltf.get_mut(key) {
            used.retain(|n| !n.as_str().is_some_and(vrm));
            if used.is_empty() {
                gltf.remove(key);
            }
        }
    }
    let strip = |v: &mut Value| {
        if let Some(Value::Object(e)) = v.get_mut("extensions") {
            e.retain(|k, _| !vrm(k));
            if e.is_empty() {
                v.as_object_mut().unwrap().remove("extensions");
            }
        }
    };
    for key in ["nodes", "materials"] {
        if let Some(Value::Array(items)) = gltf.get_mut(key) {
            items.iter_mut().for_each(strip);
        }
    }
    let mut root = Value::Object(std::mem::take(gltf));
    strip(&mut root);
    *gltf = match root {
        Value::Object(m) => m,
        _ => unreachable!(),
    };
}

fn source(doc: &Doc) -> Source {
    let meta = doc.vrm.get("meta");
    let text = |key: &str| meta.and_then(|m| m.get(key)).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
    let (licence, author) = match doc.version {
        Version::V1 => (
            text("licenseUrl"),
            meta.and_then(|m| m.get("authors")).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty()),
        ),
        Version::V0 => (if text("licenseName").as_deref() == Some("Other") { text("otherLicenseUrl").or(text("licenseName")) } else { text("licenseName") }, text("author")),
    };
    Source {
        importer: "avatar-core vrm".into(),
        importer_version: env!("CARGO_PKG_VERSION").into(),
        format: match doc.version {
            Version::V0 => SourceFormat::Vrm0,
            Version::V1 => SourceFormat::Vrm1,
        },
        licence,
        author,
    }
}

/// VRM bone names to Unity's. VRM 1.0 counts thumbs from the metacarpal, which Unity calls proximal.
fn human_body_bone(name: &str, version: Version) -> Option<HumanBodyBone> {
    let renamed = match (version, name) {
        (Version::V1, "leftThumbMetacarpal") => "leftThumbProximal",
        (Version::V1, "leftThumbProximal") => "leftThumbIntermediate",
        (Version::V1, "rightThumbMetacarpal") => "rightThumbProximal",
        (Version::V1, "rightThumbProximal") => "rightThumbIntermediate",
        (Version::V1, "leftThumbIntermediate" | "rightThumbIntermediate") => return None,
        (Version::V0, "leftThumbMetacarpal" | "rightThumbMetacarpal") => return None,
        _ => name,
    };
    let mut pascal = renamed.to_owned();
    pascal.get_mut(..1)?.make_ascii_uppercase();
    serde_json::from_value(Value::from(pascal)).ok()
}

fn humanoid(doc: &Doc) -> Result<Humanoid, Error> {
    let human = doc.vrm.get("humanoid").ok_or_else(|| malformed("no humanoid"))?;
    let mut bones = BTreeMap::new();
    let entries: Vec<(String, &Value)> = match doc.version {
        Version::V1 => human.get("humanBones").and_then(Value::as_object).ok_or_else(|| malformed("no humanBones"))?.iter().map(|(k, v)| (k.clone(), v)).collect(),
        Version::V0 => human
            .get("humanBones")
            .and_then(Value::as_array)
            .ok_or_else(|| malformed("no humanBones"))?
            .iter()
            .map(|b| (b.get("bone").and_then(Value::as_str).unwrap_or_default().to_owned(), b))
            .collect(),
    };
    for (name, entry) in entries {
        let Some(bone) = human_body_bone(&name, doc.version) else {
            return Err(malformed(format!("humanoid bone {name:?} is not a VRM bone")));
        };
        let node = doc.node(entry.get("node"), &format!("humanoid bone {name}"))?;
        let limit = (doc.version == Version::V0 && entry.get("useDefaultValues").and_then(Value::as_bool) == Some(false)).then(|| MuscleLimit {
            min: xyz(entry.get("min")).unwrap_or_default().to_array(),
            max: xyz(entry.get("max")).unwrap_or_default().to_array(),
            center: xyz(entry.get("center")).unwrap_or_default().to_array(),
            axis_length: number(entry.get("axisLength"), 0.0),
        });
        if bones.insert(bone, HumanBone { node, limit }).is_some() {
            return Err(malformed(format!("humanoid bone {name} is given twice")));
        }
    }
    for required in [HumanBodyBone::Hips, HumanBodyBone::Head] {
        if !bones.contains_key(&required) {
            return Err(malformed(format!("humanoid has no {required:?}")));
        }
    }
    let skeleton = doc
        .nodes
        .rest
        .iter()
        .enumerate()
        .map(|(i, t)| SkeletonBone { node: NodeIndex(i as u32), position: t.translation.to_array(), rotation: t.rotation.to_array(), scale: t.scale.to_array() })
        .collect();
    let given = |key: &str, default: f32| if doc.version == Version::V0 { number(human.get(key), default) } else { default };
    Ok(Humanoid {
        bones,
        skeleton,
        arm_stretch: given("armStretch", 0.05),
        leg_stretch: given("legStretch", 0.05),
        upper_arm_twist: given("upperArmTwist", 0.5),
        lower_arm_twist: given("lowerArmTwist", 0.5),
        upper_leg_twist: given("upperLegTwist", 0.5),
        lower_leg_twist: given("lowerLegTwist", 0.5),
        feet_spacing: given("feetSpacing", 0.0),
        has_translation_do_f: doc.version == Version::V0 && human.get("hasTranslationDoF").and_then(Value::as_bool) == Some(true),
        rest: None,
    })
}

/// VRM 0.x preset names to VRM 1.0's, which name the clips.
fn preset_v0(name: &str) -> Option<&'static str> {
    Some(match name {
        "neutral" => "neutral",
        "a" => "aa",
        "i" => "ih",
        "u" => "ou",
        "e" => "ee",
        "o" => "oh",
        "blink" => "blink",
        "blink_l" => "blinkLeft",
        "blink_r" => "blinkRight",
        "joy" => "happy",
        "angry" => "angry",
        "sorrow" => "sad",
        "fun" => "relaxed",
        "lookup" => "lookUp",
        "lookdown" => "lookDown",
        "lookleft" => "lookLeft",
        "lookright" => "lookRight",
        _ => return None,
    })
}

/// A constant curve: one key, held.
fn constant(value: f32) -> AnimationCurve {
    AnimationCurve {
        keys: vec![Keyframe { time: 0.0, value, in_tangent: 0.0, out_tangent: 0.0, in_weight: 1.0 / 3.0, out_weight: 1.0 / 3.0, weighted_mode: WeightedMode::None }],
    }
}

fn clip(name: String, curves: Vec<Curve>) -> Clip {
    Clip {
        name,
        length: 0.0,
        sample_rate: 60.0,
        high_quality_curve: true,
        looping: false,
        cycle_offset: 0.0,
        additive_reference_pose: None,
        humanoid: None,
        curves,
        slot_curves: vec![],
    }
}

/// Each expression as a clip that holds its full effect: blendshapes on Unity's 0 to 100 scale and material
/// properties at their target values.
fn expressions(doc: &mut Doc) -> Result<Vec<Clip>, Error> {
    let mut clips = vec![];
    match doc.version {
        Version::V1 => {
            let Some(groups) = doc.vrm.get("expressions") else { return Ok(clips) };
            let mut all = vec![];
            for kind in ["preset", "custom"] {
                for (name, e) in groups.get(kind).and_then(Value::as_object).into_iter().flatten() {
                    all.push((name.clone(), e.clone()));
                }
            }
            for (name, e) in all {
                let mut curves = vec![];
                for b in e.get("morphTargetBinds").and_then(Value::as_array).into_iter().flatten() {
                    let node = doc.node(b.get("node"), &format!("expression {name}"))?;
                    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| malformed(format!("expression {name} binds no morph target")))?;
                    curves.push(Curve { binding: Binding::BlendShape { node, name: doc.target_name(node, index)? }, curve: constant(100.0 * number(b.get("weight"), 0.0)) });
                }
                for b in e.get("materialColorBinds").and_then(Value::as_array).into_iter().flatten() {
                    let material = b.get("material").and_then(Value::as_u64).ok_or_else(|| malformed(format!("expression {name} binds no material")))?;
                    let ty = b.get("type").and_then(Value::as_str).unwrap_or_default();
                    let Some((property, srgb)) = materials::color_property(ty) else {
                        doc.report.add(format!("vrm expression material colour {ty:?}"));
                        continue;
                    };
                    let value = floats::<4>(b.get("targetValue")).unwrap_or([0.0; 4]);
                    let value = if srgb { materials::to_srgb(value) } else { value };
                    curves.extend(material_curves(doc, material, property, &value, "rgba"));
                }
                for b in e.get("textureTransformBinds").and_then(Value::as_array).into_iter().flatten() {
                    let material = b.get("material").and_then(Value::as_u64).ok_or_else(|| malformed(format!("expression {name} binds no material")))?;
                    let scale = floats::<2>(b.get("scale")).unwrap_or([1.0, 1.0]);
                    let offset = floats::<2>(b.get("offset")).unwrap_or([0.0, 0.0]);
                    let st = [scale[0], scale[1], offset[0], 1.0 - offset[1] - scale[1]];
                    curves.extend(material_curves(doc, material, "_MainTex_ST", &st, "xyzw"));
                }
                if e.get("isBinary").and_then(Value::as_bool) == Some(true) {
                    doc.report.add("vrm binary expression");
                }
                for o in ["overrideBlink", "overrideLookAt", "overrideMouth"] {
                    if e.get(o).and_then(Value::as_str).is_some_and(|v| v != "none") {
                        doc.report.add(format!("vrm expression {o}"));
                    }
                }
                clips.push(clip(name, curves));
            }
        }
        Version::V0 => {
            let groups = doc.vrm.pointer("/blendShapeMaster/blendShapeGroups").and_then(Value::as_array).cloned().unwrap_or_default();
            let material_names: Vec<Option<String>> = doc.array("materials").iter().map(|m| m.get("name").and_then(Value::as_str).map(str::to_owned)).collect();
            for g in groups {
                let preset = g.get("presetName").and_then(Value::as_str).unwrap_or("unknown");
                let given = g.get("name").and_then(Value::as_str).unwrap_or_default();
                let name = preset_v0(preset).map_or_else(|| given.to_owned(), str::to_owned);
                let mut curves = vec![];
                for b in g.get("binds").and_then(Value::as_array).into_iter().flatten() {
                    let mesh = b.get("mesh").and_then(Value::as_u64).ok_or_else(|| malformed(format!("blendshape group {name} binds no mesh")))?;
                    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| malformed(format!("blendshape group {name} binds no morph target")))?;
                    let weight = number(b.get("weight"), 0.0);
                    for node in doc.mesh_nodes(mesh) {
                        curves.push(Curve { binding: Binding::BlendShape { node, name: doc.target_name(node, index)? }, curve: constant(weight) });
                    }
                }
                for v in g.get("materialValues").and_then(Value::as_array).into_iter().flatten() {
                    let target = v.get("materialName").and_then(Value::as_str);
                    let Some(material) = material_names.iter().position(|n| n.as_deref() == target) else {
                        doc.report.add("vrm material value on a missing material");
                        continue;
                    };
                    let property = v.get("propertyName").and_then(Value::as_str).unwrap_or_default().to_owned();
                    let value: Vec<f32> = v.get("targetValue").and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
                    let (base, axes) = match property.strip_suffix("_ST") {
                        Some(_) => (property.clone(), "xyzw"),
                        None => (property.clone(), "rgba"),
                    };
                    curves.extend(material_curves(doc, material as u64, &base, &value, axes));
                }
                if g.get("isBinary").and_then(Value::as_bool) == Some(true) {
                    doc.report.add("vrm binary expression");
                }
                clips.push(clip(name, curves));
            }
        }
    }
    Ok(clips)
}

/// One curve per component of `property` on every renderer drawing `material`.
fn material_curves(doc: &Doc, material: u64, property: &str, value: &[f32], axes: &str) -> Vec<Curve> {
    let mut out = vec![];
    for (i, n) in doc.array("nodes").iter().enumerate() {
        let Some(mesh) = n.get("mesh").and_then(Value::as_u64).and_then(|m| doc.array("meshes").get(m as usize)) else { continue };
        let uses = mesh.get("primitives").and_then(Value::as_array).into_iter().flatten().any(|p| p.get("material").and_then(Value::as_u64) == Some(material));
        if !uses {
            continue;
        }
        for (c, v) in axes.chars().zip(value) {
            out.push(Curve { binding: Binding::MaterialProperty { node: NodeIndex(i as u32), property: format!("{property}.{c}") }, curve: constant(*v) });
        }
    }
    out
}

/// An expression that is one blendshape at full weight on one node, as VRChat's descriptor binds a viseme or
/// a blink.
fn single_shape(clips: &[Clip], name: &str) -> Option<(NodeIndex, String)> {
    let clip = clips.iter().find(|c| c.name == name)?;
    match clip.curves.as_slice() {
        [Curve { binding: Binding::BlendShape { node, name }, curve }] if (curve.keys[0].value - 100.0).abs() < 1e-3 => Some((*node, name.clone())),
        _ => None,
    }
}

fn descriptor(doc: &mut Doc, humanoid: &Humanoid, clips: &[Clip]) -> Result<Descriptor, Error> {
    let head = humanoid.bones[&HumanBodyBone::Head].node;
    let (eye_node, offset) = match doc.version {
        Version::V1 => (head, floats::<3>(doc.vrm.pointer("/lookAt/offsetFromHeadBone")).map_or(Vec3::ZERO, Vec3::from)),
        Version::V0 => {
            let first = doc.vrm.get("firstPerson");
            let node = match first.and_then(|f| f.get("firstPersonBone")).and_then(Value::as_i64) {
                Some(n) if n >= 0 => doc.node(Some(&Value::from(n)), "first-person bone")?,
                _ => head,
            };
            let o = xyz(first.and_then(|f| f.get("firstPersonBoneOffset"))).unwrap_or_default();
            (node, Vec3::new(o.x, o.y, -o.z))
        }
    };
    let view_position = doc.world[eye_node.0 as usize].point(offset).to_array();

    const VOWELS: [&str; 5] = ["aa", "ee", "ih", "oh", "ou"];
    let vowels: Vec<Option<(NodeIndex, String)>> = VOWELS.iter().map(|v| single_shape(clips, v)).collect();
    let any_vowel = VOWELS.iter().any(|v| clips.iter().any(|c| c.name == *v && !c.curves.is_empty()));
    let one_mesh = vowels.iter().flatten().map(|(n, _)| *n).collect::<std::collections::BTreeSet<_>>();
    let lip_sync = if vowels.iter().all(Option::is_some) && one_mesh.len() == 1 {
        let shape = |i: usize| vowels[i].as_ref().unwrap().1.clone();
        let e = String::new;
        LipSync::VisemeBlendShape {
            mesh: *one_mesh.first().unwrap(),
            visemes: Visemes { sil: e(), pp: e(), ff: e(), th: e(), dd: e(), kk: e(), ch: e(), ss: e(), nn: e(), rr: e(), aa: shape(0), e: shape(1), ih: shape(2), oh: shape(3), ou: shape(4) },
        }
    } else if any_vowel {
        doc.report.add("vrm vowel expressions that are not each one full blendshape on one mesh");
        LipSync::VisemeParameterOnly {}
    } else {
        LipSync::Default {}
    };
    let eyelids = match single_shape(clips, "blink") {
        Some((mesh, blink)) => Eyelids::BlendShapes { mesh, blink: Some(blink), looking_up: None, looking_down: None },
        None => {
            if clips.iter().any(|c| c.name == "blink" && !c.curves.is_empty()) {
                doc.report.add("vrm blink that is not one full blendshape");
            }
            Eyelids::None {}
        }
    };
    let look = look(doc, humanoid);
    let collider = DescriptorCollider { state: DescriptorColliderState::Disabled, bone: None, radius: 0.0, height: 0.0, position: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0] };
    let c = || collider.clone();
    Ok(Descriptor {
        view_position,
        lip_sync,
        eyes: Eyes { look, eyelids, eye_movement: EyeMovement { confidence: 0.5, excitement: 0.5 } },
        colliders: DescriptorColliders {
            head: c(),
            torso: c(),
            hand_left: c(),
            hand_right: c(),
            foot_left: c(),
            foot_right: c(),
            finger_index_left: c(),
            finger_middle_left: c(),
            finger_ring_left: c(),
            finger_little_left: c(),
            finger_index_right: c(),
            finger_middle_right: c(),
            finger_ring_right: c(),
            finger_little_right: c(),
        },
    })
}

/// A bone look-at as VRChat's eye rotations: each direction turns the eyes by the output of its range map, in the
/// avatar's axes (+Z forward, +X its left).
fn look(doc: &mut Doc, humanoid: &Humanoid) -> Option<EyeLook> {
    let (bone, ranges) = match doc.version {
        Version::V1 => {
            let l = doc.vrm.get("lookAt");
            let ty = l.and_then(|l| l.get("type")).and_then(Value::as_str);
            let out = |k: &str| number(l.and_then(|l| l.get(k)).and_then(|m| m.get("outputScale")), if ty == Some("bone") { 10.0 } else { 1.0 });
            (ty == Some("bone"), [out("rangeMapHorizontalInner"), out("rangeMapHorizontalOuter"), out("rangeMapVerticalDown"), out("rangeMapVerticalUp")])
        }
        Version::V0 => {
            let f = doc.vrm.get("firstPerson");
            let ty = f.and_then(|f| f.get("lookAtTypeName")).and_then(Value::as_str);
            let out = |k: &str| number(f.and_then(|f| f.get(k)).and_then(|m| m.get("yRange")), 10.0);
            (ty == Some("Bone"), [out("lookAtHorizontalInner"), out("lookAtHorizontalOuter"), out("lookAtVerticalDown"), out("lookAtVerticalUp")])
        }
    };
    if !bone {
        let has = match doc.version {
            Version::V1 => doc.vrm.get("lookAt").is_some(),
            Version::V0 => doc.vrm.pointer("/firstPerson/lookAtTypeName").is_some(),
        };
        if has {
            doc.report.add("vrm look-at by expression");
        }
        return None;
    }
    let left = humanoid.bones.get(&HumanBodyBone::LeftEye).map(|b| b.node);
    let right = humanoid.bones.get(&HumanBodyBone::RightEye).map(|b| b.node);
    if left.is_none() && right.is_none() {
        return None;
    }
    let [inner, outer, down, up] = ranges.map(f32::to_radians);
    let local = |eye: Option<NodeIndex>, turn: Quat| -> [f32; 4] {
        let Some(e) = eye else { return [0.0, 0.0, 0.0, 1.0] };
        let i = e.0 as usize;
        let parent = doc.nodes.parent[i].map_or(Transform::IDENTITY, |p| doc.world[p]);
        (parent.rotation.inverse() * turn * doc.world[i].rotation).normalize().to_array()
    };
    let rotations = |l: Quat, r: Quat| EyeRotations { linked: l == r, left: local(left, l), right: local(right, r) };
    let toward_left = |a: f32| Quat::from_rotation_y(a);
    Some(EyeLook {
        left_eye: left,
        right_eye: right,
        straight: rotations(Quat::IDENTITY, Quat::IDENTITY),
        up: rotations(Quat::from_rotation_x(-up), Quat::from_rotation_x(-up)),
        down: rotations(Quat::from_rotation_x(down), Quat::from_rotation_x(down)),
        left: rotations(toward_left(outer), toward_left(inner)),
        right: rotations(toward_left(-inner), toward_left(-outer)),
    })
}

/// VRChat shows every mesh to its wearer but the head; a mesh VRM shows to one viewpoint only is reported.
fn first_person(doc: &mut Doc) {
    let flags: Vec<String> = match doc.version {
        Version::V1 => doc.vrm.pointer("/firstPerson/meshAnnotations").and_then(Value::as_array).into_iter().flatten().filter_map(|a| a.get("type")?.as_str().map(str::to_owned)).collect(),
        Version::V0 => doc
            .vrm
            .pointer("/firstPerson/meshAnnotations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|a| a.get("firstPersonFlag")?.as_str().map(str::to_owned))
            .collect(),
    };
    for f in flags {
        match f.as_str() {
            "thirdPersonOnly" | "ThirdPersonOnly" => doc.report.add("vrm third-person-only mesh"),
            "firstPersonOnly" | "FirstPersonOnly" => doc.report.add("vrm first-person-only mesh"),
            _ => {}
        }
    }
}
