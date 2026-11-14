//! Compares an exported `.glb` with the census the Unity exporter writes beside it (`-awCensus`):
//! `census <export.glb> <census.json> [--names]`. Prints one line per category, `ok` or `FAIL`, and exits
//! non-zero on any failure. A failing category prints its fault count, and with `--names` its faults, which name
//! the avatar's objects, materials and fields.

#[path = "../tests/reference/names.rs"]
mod names;

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

use avatar_core::model::{
    AwAvatar, BlendAlgorithm, Binding, ControlKind, LayerContent, Menu, Motion, StateMachine,
    TransformProperty,
};
use avatar_core::Avatar;
use serde_json::{json, Value};

fn main() -> ExitCode {
    let names = names::names();
    let args: Vec<String> = names::args().into_iter().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: census <export.glb> <census.json> [--names]");
        return ExitCode::from(2);
    }
    let avatar = match std::fs::read(&args[0])
        .map_err(|e| e.to_string())
        .and_then(|b| avatar_core::read(&b).map_err(|e| e.to_string()))
    {
        Ok(a) => a,
        Err(e) => {
            eprintln!("export: {}", if names { e } else { "unreadable: pass --names for the error".into() });
            return ExitCode::FAILURE;
        }
    };
    let census: Value = match std::fs::read(&args[1])
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("census: {}", if names { e } else { "unreadable: pass --names for the error".into() });
            return ExitCode::FAILURE;
        }
    };
    let streams = match std::fs::read(format!("{}.bin", args[1])) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("census streams: {}", if names { e.to_string() } else { "unreadable: pass --names for the error".into() });
            return ExitCode::FAILURE;
        }
    };
    let mut report = Report::default();
    Check { avatar: &avatar, census: &census, streams: &streams, report: &mut report }.run();
    report.print(names);
    if report.failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[derive(Default)]
struct Report {
    lines: Vec<(String, String, String, bool, Vec<String>)>,
    failed: bool,
}

impl Report {
    fn line(&mut self, category: &str, unity: impl ToString, model: impl ToString, faults: Vec<String>) {
        let ok = faults.is_empty();
        self.failed |= !ok;
        self.lines.push((category.into(), unity.to_string(), model.to_string(), ok, faults));
    }

    fn count(&mut self, category: &str, unity: usize, model: usize, accounted: usize, why: &str, mut faults: Vec<String>) {
        if unity != model + accounted {
            faults.push(format!("Unity {unity}, model {model}, reported {accounted} ({why})"));
        }
        let shown = if accounted > 0 { format!("{model} + {accounted} reported") } else { model.to_string() };
        self.line(category, unity, shown, faults);
    }

    fn print(&self, names: bool) {
        println!("category\tunity\tmodel\tverdict");
        for (category, unity, model, ok, faults) in &self.lines {
            println!("{category}\t{unity}\t{model}\t{}", if *ok { "ok" } else { "FAIL" });
            if !names {
                if !faults.is_empty() {
                    println!("  faults: {}", faults.len());
                }
                continue;
            }
            for f in faults.iter().take(20) {
                println!("  {f}");
            }
            if faults.len() > 20 {
                println!("  … {} more", faults.len() - 20);
            }
        }
    }
}

struct Check<'a> {
    avatar: &'a Avatar,
    census: &'a Value,
    streams: &'a [u8],
    report: &'a mut Report,
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

fn num(v: &Value) -> f32 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN) as f32,
        Value::String(s) if s == "Infinity" => f32::INFINITY,
        Value::String(s) if s == "-Infinity" => f32::NEG_INFINITY,
        _ => f32::NAN,
    }
}

fn same(a: f32, b: f32) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

impl Check<'_> {
    fn model(&self) -> &AwAvatar {
        &self.avatar.model
    }

    fn unsupported(&self, kind: &str) -> usize {
        self.model().unsupported.iter().filter(|u| u.kind == kind).map(|u| u.count as usize).sum()
    }

    fn run(&mut self) {
        self.nodes();
        self.physbones();
        self.counts();
        self.renderers();
        self.parameters_and_menu();
        self.layers();
        self.curves();
        self.clip_settings();
        self.human_rest();
        self.materials();
        self.inactive();
        self.components();
    }

    fn nodes(&mut self) {
        let unity: Vec<&str> = arr(&self.census["nodes"]).iter().filter_map(Value::as_str).collect();
        let gltf: Vec<&str> = arr(&self.avatar.gltf["nodes"]).iter().map(|n| n["name"].as_str().unwrap_or("")).collect();
        let faults = (0..unity.len().max(gltf.len()))
            .filter(|&i| unity.get(i) != gltf.get(i))
            .map(|i| format!("node {i}: Unity {:?}, glTF {:?}", unity.get(i), gltf.get(i)))
            .collect();
        self.report.line("nodes", unity.len(), gltf.len(), faults);
    }

    fn physbones(&mut self) {
        let unity = arr(&self.census["physbones"]);
        let model = &self.model().physbones;
        let colliders: Vec<&Value> = arr(&self.census["colliders"]).iter().collect();
        let non_finite = self.unsupported("non-finite value in physbones, written as 0 or the largest finite value") > 0;
        let mut faults = vec![];
        let mut unmapped = BTreeMap::<String, usize>::new();
        let (mut checked, mut outside_ignored, mut outside_colliders) = (0, 0, 0);
        if unity.len() != model.len() {
            faults.push(format!("Unity {} components, model {}", unity.len(), model.len()));
        }
        for (i, (u, m)) in unity.iter().zip(model).enumerate() {
            let m = serde_json::to_value(m).unwrap();
            let node = u["node"].as_u64().unwrap_or(u64::MAX);
            let f = &u["fields"];
            let found = std::cell::RefCell::new(vec![]);
            let eq = |field: &str, expected: Value, at: &str| {
                let actual = m.pointer(at).cloned().unwrap_or(json!("<absent>"));
                if !values_equal(&expected, &actual, non_finite) {
                    found.borrow_mut().push(format!("physbone {i} (node {node}): {field}: Unity {expected}, model {at} {actual}"));
                }
            };
            let reference = |v: &Value| -> Value { if v.is_null() { json!(node) } else { v["node"].clone() } };
            let limit = f["limitType"].as_str().unwrap_or("").to_lowercase();
            for (name, value) in f.as_object().into_iter().flatten() {
                let mut compared = true;
                match name.as_str() {
                    "m_Enabled" => eq(name, value.clone(), "/enabled"),
                    n if n.starts_with("m_") || n.starts_with("foldout_") || matches!(n, "showGizmos" | "boneOpacity" | "limitOpacity") => compared = false,
                    "version" => eq(name, json!(match value.as_str() { Some("Version_1_0") => "1.0", Some("Version_1_1") => "1.1", other => other.unwrap_or("") }), "/version"),
                    "integrationType" | "multiChildType" | "immobileType" => {
                        let s = value.as_str().unwrap_or("");
                        let camel = s.get(..1).unwrap_or("").to_lowercase() + s.get(1..).unwrap_or("");
                        eq(name, json!(camel), &format!("/{name}"));
                    }
                    "rootTransform" => eq(name, reference(value), "/root"),
                    "ignoreTransforms" => {
                        outside_ignored += arr(value).iter().filter(|v| v.get("outside").is_some()).count();
                        eq(name, Value::Array(arr(value).iter().filter(|v| v.get("node").is_some()).map(reference).collect()), "/ignoreTransforms");
                    }
                    "colliders" => {
                        outside_colliders += arr(value).iter().filter(|v| v.get("outside").is_some()).count();
                        let expected = arr(value)
                            .iter()
                            .filter(|v| v.get("node").is_some())
                            .map(|v| colliders.iter().position(|c| *c == v).map_or(json!(format!("unknown collider {v}")), |p| json!(p)))
                            .collect();
                        eq(name, Value::Array(expected), "/colliders");
                    }
                    "endpointPosition" => eq(name, mirror(value), "/endpointPosition"),
                    "pull" | "spring" | "stiffness" | "gravity" | "gravityFalloff" | "immobile" | "radius" | "maxStretch" | "stretchMotion" | "maxSquish" => {
                        eq(name, jf(num(value)), &format!("/{name}/value"))
                    }
                    "limitType" => eq(name, json!(limit), "/limit/type"),
                    "maxAngleX" | "maxAngleZ" | "maxAngleXCurve" | "maxAngleZCurve" => {
                        let base = name.trim_end_matches("Curve");
                        let applies = if base == "maxAngleX" { limit != "none" } else { limit == "polar" };
                        compared = applies;
                        if applies {
                            if name.ends_with("Curve") {
                                eq(name, keys(value), &format!("/limit/{base}/curve/keys"));
                            } else {
                                eq(name, jf(num(value)), &format!("/limit/{base}/value"));
                            }
                        }
                    }
                    "limitRotation" | "limitRotationXCurve" | "limitRotationYCurve" | "limitRotationZCurve" => {
                        compared = limit != "none";
                        if compared {
                            if name == "limitRotation" {
                                eq(name, mirror_euler(value), "/limit/rotation/value");
                            } else {
                                let axis = name["limitRotation".len()..="limitRotation".len()].to_lowercase();
                                eq(name, keys(value), &format!("/limit/rotation/{axis}Curve/keys"));
                            }
                        }
                    }
                    n if n.ends_with("Curve") => {
                        let base = &n[..n.len() - "Curve".len()];
                        eq(name, keys(value), &format!("/{base}/curve/keys"));
                    }
                    "allowCollision" | "allowGrabbing" | "allowPosing" => {
                        let (filter, at) = match name.as_str() {
                            "allowCollision" => ("collisionFilter", "/collision"),
                            "allowGrabbing" => ("grabFilter", "/grabbing"),
                            _ => ("poseFilter", "/posing"),
                        };
                        let expected = match value.as_str() {
                            Some("True") => json!({"allowSelf": true, "allowOthers": true}),
                            Some("False") => json!({"allowSelf": false, "allowOthers": false}),
                            _ => json!({"allowSelf": f[filter]["allowSelf"], "allowOthers": f[filter]["allowOthers"]}),
                        };
                        eq(name, expected, at);
                    }
                    "collisionFilter" | "grabFilter" | "poseFilter" => compared = false,
                    "grabMovement" => eq(name, jf(num(value)), "/grabMovement"),
                    "snapToHand" | "isAnimated" | "resetWhenDisabled" => eq(name, value.clone(), &format!("/{name}")),
                    "parameter" => eq(name, if value.as_str() == Some("") { json!("<absent>") } else { value.clone() }, "/parameter"),
                    "ignoreOtherPhysBones" => {
                        let actual = m.pointer("/ignoreOtherPhysBones").cloned().unwrap_or(json!(false));
                        if !values_equal(value, &actual, non_finite) {
                            found.borrow_mut().push(format!("physbone {i} (node {node}): {name}: Unity {value}, model {actual}"));
                        }
                    }
                    "staticFreezeAxis" => {
                        let default = value == &json!(false) || value == &json!([0.0, 0.0, 0.0]);
                        if !default && self.unsupported(&format!("physbone {name}")) == 0 {
                            found.borrow_mut().push(format!("physbone {i} (node {node}): {name} {value} is neither in the model nor reported"));
                        }
                    }
                    other => {
                        compared = false;
                        *unmapped.entry(other.to_string()).or_default() += 1;
                    }
                }
                checked += compared as usize;
            }
            faults.extend(found.into_inner());
        }
        for (field, n) in &unmapped {
            faults.push(format!("serialized field {field} on {n} components has no model field and no report"));
        }
        for (n, kind) in [(outside_ignored, "physbone ignored transform outside the avatar"), (outside_colliders, "physbone collider outside the avatar")] {
            if n != self.unsupported(kind) {
                faults.push(format!("{n} references outside the avatar, {} reported as {kind}", self.unsupported(kind)));
            }
        }
        self.report.line(&format!("physbones ({checked} fields compared)"), unity.len(), model.len(), faults);
    }

    fn counts(&mut self) {
        let m = &self.avatar.model;
        let census = self.census;
        self.report.count("colliders", arr(&census["colliders"]).len(), m.colliders.len(), 0, "", vec![]);
        let contacts = arr(&census["contacts"]);
        let boxes = contacts.iter().filter(|c| c["box"] == true).count();
        let enabled = |sender: bool| -> Vec<bool> { contacts.iter().filter(|c| c["box"] != true && c["sender"] == sender).map(|c| c["enabled"] == true).collect() };
        let mut faults = vec![];
        if boxes != self.unsupported("box contact") {
            faults.push(format!("{boxes} box contacts, {} reported", self.unsupported("box contact")));
        }
        if enabled(true) != m.contacts.senders.iter().map(|c| c.enabled).collect::<Vec<_>>() {
            faults.push("senders or their enabled flags differ".into());
        }
        if enabled(false) != m.contacts.receivers.iter().map(|c| c.enabled).collect::<Vec<_>>() {
            faults.push("receivers or their enabled flags differ".into());
        }
        self.report.line("contacts", contacts.len(), format!("{} + {boxes} box reported", m.contacts.senders.len() + m.contacts.receivers.len()), faults);

        let constraints = arr(&census["constraints"]);
        let mut by_type = BTreeMap::<&str, usize>::new();
        for c in constraints {
            *by_type.entry(c["type"].as_str().unwrap_or("")).or_default() += 1;
        }
        let dropped: usize = by_type.keys().map(|t| self.unsupported(t)).sum();
        let mut faults = vec![];
        let on = |v: &mut dyn Iterator<Item = bool>| v.filter(|e| *e).count();
        let (u_on, m_on) = (on(&mut constraints.iter().map(|c| c["enabled"] == true)), on(&mut m.constraints.iter().map(|c| c.enabled)));
        if dropped == 0 && u_on != m_on {
            faults.push(format!("{u_on} enabled in Unity, {m_on} in the model"));
        }
        if constraints.len() != m.constraints.len() + dropped {
            faults.push(format!("Unity {}, model {}, reported {dropped}", constraints.len(), m.constraints.len()));
        }
        self.report.line("constraints", constraints.len(), if dropped > 0 { format!("{} + {dropped} reported", m.constraints.len()) } else { m.constraints.len().to_string() }, faults);
    }

    fn accessor(&self, index: &Value) -> Result<(Vec<f32>, usize), String> {
        let a = &self.avatar.gltf["accessors"][index.as_u64().ok_or("no accessor")? as usize];
        let count = a["count"].as_u64().ok_or("accessor count")? as usize;
        let comps = match a["type"].as_str() {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            Some("MAT4") => 16,
            t => return Err(format!("accessor type {t:?}")),
        };
        let ctype = a["componentType"].as_u64().unwrap_or(0);
        let read = |view: &Value, at: usize, n: usize, ctype: u64| -> Result<Vec<f32>, String> {
            let v = &self.avatar.gltf["bufferViews"][view.as_u64().ok_or("bufferView")? as usize];
            let start = v["byteOffset"].as_u64().unwrap_or(0) as usize + at;
            let size = match ctype {
                5126 | 5125 => 4,
                5123 => 2,
                5121 => 1,
                c => return Err(format!("component type {c}")),
            };
            let bytes = self.avatar.bin.get(start..start + n * size).ok_or("bufferView overruns the buffer")?;
            Ok(bytes
                .chunks_exact(size)
                .map(|b| match ctype {
                    5126 => f32::from_le_bytes(b.try_into().unwrap()),
                    5125 => u32::from_le_bytes(b.try_into().unwrap()) as f32,
                    5123 => u16::from_le_bytes(b.try_into().unwrap()) as f32,
                    _ => b[0] as f32,
                })
                .collect())
        };
        let mut values = match a.get("bufferView") {
            Some(view) => read(view, a["byteOffset"].as_u64().unwrap_or(0) as usize, count * comps, ctype)?,
            None => vec![0.0; count * comps],
        };
        if let Some(sparse) = a.get("sparse") {
            let n = sparse["count"].as_u64().unwrap_or(0) as usize;
            let idx = read(&sparse["indices"]["bufferView"], 0, n, sparse["indices"]["componentType"].as_u64().unwrap_or(0))?;
            let vals = read(&sparse["values"]["bufferView"], 0, n * comps, ctype)?;
            for (k, &i) in idx.iter().enumerate() {
                let i = i as usize;
                values[i * comps..(i + 1) * comps].copy_from_slice(&vals[k * comps..(k + 1) * comps]);
            }
        }
        Ok((values, comps))
    }

    fn stream(&self, s: &Value) -> Vec<f32> {
        let start = s["offset"].as_u64().unwrap_or(0) as usize;
        let len = s["count"].as_u64().unwrap_or(0) as usize * s["components"].as_u64().unwrap_or(1) as usize;
        self.streams
            .get(start..start + len * 4)
            .unwrap_or(&[])
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect()
    }

    fn renderers(&mut self) {
        let unity = arr(&self.census["renderers"]);
        let gltf = &self.avatar.gltf;
        let mut slot_faults = vec![];
        let mut shape_faults = vec![];
        let mut mesh_faults = vec![];
        let (mut shapes_unity, mut shapes_model) = (0, 0);
        let (mut values, mut vertices) = (0usize, 0usize);
        let mut streams_checked = BTreeMap::<String, usize>::new();
        for u in unity {
            let node = u["node"].as_u64().unwrap_or(u64::MAX) as u32;
            let name = gltf["nodes"][node as usize]["name"].as_str().unwrap_or("?").to_string();
            let Some(r) = self.model().renderers.iter().find(|r| r.node.0 == node) else {
                slot_faults.push(format!("{name}: no model renderer"));
                continue;
            };
            let slots: Vec<Value> = r
                .materials
                .iter()
                .map(|m| match gltf["materials"][m.0 as usize]["name"].as_str() {
                    Some("missing") if self.model().materials[m.0 as usize].shader_name.is_empty() => Value::Null,
                    n => json!(n),
                })
                .collect();
            if u["enabled"] != r.enabled {
                slot_faults.push(format!("{name}: enabled {} in Unity, {} in the model", u["enabled"], r.enabled));
            }
            if slots != *arr(&u["materials"]) {
                slot_faults.push(format!("{name}: Unity slots {}, model {}", u["materials"], Value::Array(slots)));
            }
            if u.get("mesh").is_none() {
                continue;
            }
            let gnode = &gltf["nodes"][node as usize];
            let Some(mesh) = gnode["mesh"].as_u64().map(|m| &gltf["meshes"][m as usize]) else {
                mesh_faults.push(format!("{name}: no glTF mesh"));
                continue;
            };
            let prims = arr(&mesh["primitives"]);
            let want = arr(&u["materials"]).len().max(1);
            if prims.len() != want {
                slot_faults.push(format!("{name}: {} primitives for {want} slots", prims.len()));
            }
            let names: Vec<&Value> = arr(&mesh["extras"]["targetNames"]).iter().collect();
            let unames: Vec<&Value> = arr(&u["blendshapes"]).iter().collect();
            shapes_unity += unames.len();
            shapes_model += names.len();
            if names != unames {
                shape_faults.push(format!("{name}: Unity {} blendshapes, model {}", unames.len(), names.len()));
            }
            let Some(prim) = prims.first() else { continue };
            vertices += u["vertices"].as_u64().unwrap_or(0) as usize;
            let attributes = &prim["attributes"];
            let mut fault = |what: String| mesh_faults.push(format!("{name}: {what}"));
            let mut seen = BTreeSet::new();
            for s in arr(&u["streams"]) {
                let sname = s["name"].as_str().unwrap_or("");
                let expected = self.stream(s);
                let actual: Result<Vec<f32>, String> = if let Some(sub) = sname.strip_prefix("INDICES_") {
                    let sub: usize = sub.parse().unwrap_or(usize::MAX);
                    let subs = arr(&u["streams"]).iter().filter(|s| s["name"].as_str().is_some_and(|n| n.starts_with("INDICES_"))).count();
                    let slots: Vec<usize> = (0..prims.len()).filter(|&k| k.min(subs.saturating_sub(1)) == sub).collect();
                    if slots.is_empty() {
                        Ok(expected.clone())
                    } else {
                        slots.iter().map(|&k| match prims[k].get("indices") {
                            Some(a) => self.accessor(a).map(|v| v.0),
                            None => Err("primitive without indices".into()),
                        }).find(|r| r.as_ref().map_or(true, |v| *v != expected)).unwrap_or(Ok(expected.clone()))
                    }
                } else if sname == "WEIGHTS" {
                    seen.extend(attributes.as_object().into_iter().flatten().map(|(k, _)| k.clone()).filter(|k| k.starts_with("JOINTS_") || k.starts_with("WEIGHTS_")));
                    self.weights(gnode, attributes)
                } else if let Some(rest) = sname.strip_prefix("MORPH_") {
                    let (shape, attr) = rest.split_once('_').unwrap_or(("", ""));
                    match prim["targets"].get(shape.parse::<usize>().unwrap_or(usize::MAX)).and_then(|t| t.get(attr)) {
                        Some(a) => self.accessor(a).map(|v| v.0),
                        None => Err("no morph target attribute".into()),
                    }
                } else {
                    seen.insert(sname.to_string());
                    match attributes.get(sname) {
                        Some(a) => self.accessor(a).map(|v| v.0),
                        None => Err("absent".into()),
                    }
                };
                let kind = sname.split('_').filter(|p| !p.is_empty() && p.parse::<u32>().is_err()).next().unwrap_or(sname);
                *streams_checked.entry(if sname.starts_with("MORPH") { format!("morph {}", sname.rsplit('_').next().unwrap()) } else { kind.to_string() }).or_default() += 1;
                match actual {
                    Err(e) => fault(format!("{sname}: {e}")),
                    Ok(actual) => {
                        values += expected.len();
                        if actual.len() != expected.len() {
                            fault(format!("{sname}: Unity {} values, glTF {}", expected.len(), actual.len()));
                        } else if let Some(i) = (0..expected.len()).find(|&i| !same(expected[i], actual[i])) {
                            let differ = (0..expected.len()).filter(|&i| !same(expected[i], actual[i])).count();
                            fault(format!("{sname}: {differ} values differ, first at {i}: Unity {}, glTF {}", expected[i], actual[i]));
                        }
                    }
                }
            }
            for k in attributes.as_object().into_iter().flatten().map(|(k, _)| k) {
                if !seen.contains(k) {
                    fault(format!("glTF attribute {k} has no Unity stream"));
                }
            }
        }
        let nodes: BTreeSet<u64> = unity.iter().filter_map(|u| u["node"].as_u64()).collect();
        for r in &self.model().renderers {
            if !nodes.contains(&(r.node.0 as u64)) {
                slot_faults.push(format!("model renderer on node {} has no Unity renderer", r.node.0));
            }
        }
        if unity.len() != self.model().renderers.len() {
            slot_faults.push(format!("Unity {} renderers, model {}", unity.len(), self.model().renderers.len()));
        }
        self.report.line("renderers and slot order", unity.len(), self.model().renderers.len(), slot_faults);
        let frames: usize = unity.iter().flat_map(|u| arr(&u["blendshapeFrames"])).map(|f| f.as_u64().unwrap_or(1).saturating_sub(1) as usize).sum();
        if frames != self.unsupported("blendshape frames beyond the last") {
            shape_faults.push(format!("{frames} blendshape frames beyond the last, {} reported", self.unsupported("blendshape frames beyond the last")));
        }
        self.report.line("blendshapes", shapes_unity, shapes_model, shape_faults);
        let summary: Vec<String> = streams_checked.iter().map(|(k, n)| format!("{n} {k}")).collect();
        self.report.line(
            &format!("mesh data ({vertices} vertices, {values} values exact: {})", summary.join(", ")),
            "Unity's",
            "glTF",
            mesh_faults,
        );
    }

    fn weights(&self, node: &Value, attributes: &Value) -> Result<Vec<f32>, String> {
        let skin = &self.avatar.gltf["skins"][node["skin"].as_u64().ok_or("no skin")? as usize];
        let joints: Vec<f32> = arr(&skin["joints"]).iter().map(|j| j.as_f64().unwrap_or(-1.0) as f32).collect();
        let mut sets = vec![];
        for s in 0.. {
            let (Some(j), Some(w)) = (attributes.get(format!("JOINTS_{s}")), attributes.get(format!("WEIGHTS_{s}"))) else { break };
            sets.push((self.accessor(j)?.0, self.accessor(w)?.0));
        }
        let vertices = sets.first().map_or(0, |s| s.1.len() / 4);
        let mut out = vec![];
        for v in 0..vertices {
            for (j, w) in &sets {
                for k in 0..4 {
                    let weight = w[v * 4 + k];
                    if weight != 0.0 {
                        out.extend([v as f32, *joints.get(j[v * 4 + k] as usize).ok_or("joint index past the skin")?, weight]);
                    }
                }
            }
        }
        Ok(out)
    }

    fn parameters_and_menu(&mut self) {
        let unity: Vec<&str> = arr(&self.census["parameters"]).iter().filter_map(Value::as_str).filter(|n| !n.is_empty()).collect();
        let model: Vec<&str> = self.model().parameters.iter().map(|p| p.name.as_str()).collect();
        let faults = if unity == model { vec![] } else { vec![format!("names differ: Unity {unity:?}, model {model:?}")] };
        self.report.line("parameters", unity.len(), model.len(), faults);

        fn flatten<'a>(menu: &'a Menu, out: &mut Vec<(String, &'static str, String)>) {
            for c in &menu.controls {
                let (kind, parameter, sub) = match &c.kind {
                    ControlKind::Button { parameter, .. } => ("Button", parameter, None),
                    ControlKind::Toggle { parameter, .. } => ("Toggle", parameter, None),
                    ControlKind::SubMenu { parameter, sub_menu, .. } => ("SubMenu", parameter, sub_menu.as_ref()),
                    ControlKind::TwoAxisPuppet { parameter, .. } => ("TwoAxisPuppet", parameter, None),
                    ControlKind::FourAxisPuppet { parameter, .. } => ("FourAxisPuppet", parameter, None),
                    ControlKind::RadialPuppet { parameter, .. } => ("RadialPuppet", parameter, None),
                };
                out.push((c.name.clone(), kind, parameter.clone().unwrap_or_default()));
                if let Some(sub) = sub {
                    flatten(sub, out);
                }
            }
        }
        let mut model = vec![];
        flatten(&self.model().menu, &mut model);
        let unity: Vec<(String, String, String)> = arr(&self.census["menuControls"])
            .iter()
            .map(|c| (c["name"].as_str().unwrap_or("").into(), c["type"].as_str().unwrap_or("").into(), c["parameter"].as_str().unwrap_or("").into()))
            .collect();
        let faults: Vec<String> = (0..unity.len().max(model.len()))
            .filter(|&i| unity.get(i).map(|u| (u.0.as_str(), u.1.as_str(), u.2.as_str())) != model.get(i).map(|m| (m.0.as_str(), m.1, m.2.as_str())))
            .map(|i| format!("control {i}: Unity {:?}, model {:?}", unity.get(i), model.get(i)))
            .collect();
        self.report.line("menu controls", unity.len(), model.len(), faults);
    }

    fn layers(&mut self) {
        #[derive(Default, Debug, PartialEq, Clone, Copy)]
        struct Counts {
            layers: usize,
            machines: usize,
            states: usize,
            transitions: usize,
            trees: usize,
            behaviours: usize,
        }
        fn motion(m: &Motion, c: &mut Counts) {
            if let Motion::BlendTree(t) = m {
                c.trees += 1;
                let children: Vec<Option<&Motion>> = match &t.algorithm {
                    BlendAlgorithm::Simple1D { children, .. } => children.iter().map(|c| c.motion.as_ref()).collect(),
                    BlendAlgorithm::SimpleDirectional2D { children, .. }
                    | BlendAlgorithm::FreeformDirectional2D { children, .. }
                    | BlendAlgorithm::FreeformCartesian2D { children, .. } => children.iter().map(|c| c.motion.as_ref()).collect(),
                    BlendAlgorithm::Direct { children, .. } => children.iter().map(|c| c.motion.as_ref()).collect(),
                };
                for m in children.into_iter().flatten() {
                    motion(m, c);
                }
            }
        }
        fn machine(m: &StateMachine, c: &mut Counts) {
            c.machines += 1;
            c.behaviours += m.behaviours.len();
            c.transitions += m.any_state_transitions.len() + m.entry_transitions.len() + m.state_machine_transitions.len();
            for s in &m.states {
                c.states += 1;
                c.behaviours += s.behaviours.len();
                c.transitions += s.transitions.len();
                if let Some(m) = &s.motion {
                    motion(m, c);
                }
            }
            for sub in &m.state_machines {
                machine(sub, c);
            }
        }
        let key = |k: &str| match k {
            "FX" => "fx".to_string(),
            "TPose" => "tPose".into(),
            "IKPose" => "ikPose".into(),
            k => k[..1].to_lowercase() + &k[1..],
        };
        let unity = self.census["layers"].as_object().cloned().unwrap_or_default();
        let model: BTreeMap<String, Counts> = self
            .model()
            .layers
            .iter()
            .filter_map(|(t, l)| {
                let controller = l.controller.as_ref()?;
                let mut c = Counts { layers: controller.layers.len(), ..Default::default() };
                for layer in &controller.layers {
                    match &layer.content {
                        LayerContent::StateMachine { state_machine } => machine(state_machine, &mut c),
                        LayerContent::Synced { states, .. } => {
                            for s in states {
                                c.behaviours += s.behaviours.len();
                                if let Some(m) = &s.motion {
                                    motion(m, &mut c);
                                }
                            }
                        }
                    }
                }
                Some((serde_json::to_value(t).unwrap().as_str().unwrap().to_string(), c))
            })
            .collect();
        let represented = ["VRCAvatarParameterDriver", "VRCAnimatorTrackingControl", "VRCAnimatorLocomotionControl", "VRCPlayableLayerControl", "VRCAnimatorLayerControl", "VRCAnimatorTemporaryPoseSpace"];
        let mut dropped_behaviours = 0;
        let mut faults = vec![];
        for t in arr(&self.census["behaviourTypes"]) {
            let ty = t["type"].as_str().unwrap_or("");
            if represented.contains(&ty) {
                continue;
            }
            let n = t["count"].as_u64().unwrap_or(0) as usize;
            dropped_behaviours += n;
            if self.unsupported(ty) != n {
                faults.push(format!("{n} × behaviour {ty}, {} reported", self.unsupported(ty)));
            }
        }
        let mut totals = (Counts::default(), Counts::default());
        for (k, u) in &unity {
            let n = |f: &str| u[f].as_u64().unwrap_or(0) as usize;
            let uc = Counts { layers: n("layers"), machines: n("stateMachines"), states: n("states"), transitions: n("transitions"), trees: n("blendTrees"), behaviours: n("behaviours") };
            let mc = model.get(&key(k)).copied().unwrap_or_default();
            if !model.contains_key(&key(k)) {
                faults.push(format!("playable layer {k} has no controller in the model"));
            }
            for (t, c) in [(&mut totals.0, uc), (&mut totals.1, mc)] {
                t.layers += c.layers;
                t.machines += c.machines;
                t.states += c.states;
                t.transitions += c.transitions;
                t.trees += c.trees;
                t.behaviours += c.behaviours;
            }
            let ours = mc;
            let theirs = Counts { behaviours: mc.behaviours, transitions: mc.transitions, ..uc };
            if ours != theirs {
                faults.push(format!("{k}: Unity {uc:?}, model {mc:?}"));
            }
            let up: Vec<&Value> = arr(&u["parameters"]).iter().collect();
            let mp: Vec<Value> = self.model().layers.iter().find(|(t, _)| serde_json::to_value(t).unwrap() == json!(key(k))).and_then(|(_, l)| l.controller.as_ref()).map(|c| c.parameters.iter().map(|p| json!(p.name)).collect()).unwrap_or_default();
            if up.len() != mp.len() || up.iter().zip(&mp).any(|(a, b)| *a != b) {
                faults.push(format!("{k}: animator parameters differ"));
            }
        }
        for k in model.keys() {
            if !unity.keys().any(|u| key(u) == *k) {
                faults.push(format!("model playable layer {k} has no controller in Unity"));
            }
        }
        let (u, m) = totals;
        let other_layer: usize = unity.values().map(|u| u["otherLayerTransitions"].as_u64().unwrap_or(0) as usize).sum();
        let reported = self.unsupported("transition to another layer's state");
        let r = &mut *self.report;
        r.count("layers (playable: animator layers)", u.layers, m.layers, 0, "", faults);
        r.count("state machines", u.machines, m.machines, 0, "", vec![]);
        r.count("states", u.states, m.states, 0, "", vec![]);
        let cross = if other_layer == reported { vec![] } else { vec![format!("{other_layer} destinations in another layer, {reported} reported")] };
        r.count("transitions", u.transitions, m.transitions, other_layer, "destinations in another layer", cross);
        r.count("blend trees", u.trees, m.trees, 0, "", vec![]);
        r.count("behaviours", u.behaviours, m.behaviours, dropped_behaviours, "behaviour types not represented", vec![]);
        let clips = self.census["clips"].as_u64().unwrap_or(0) as usize;
        self.report.count("clips", clips, self.model().clips.len(), 0, "", vec![]);
    }

    fn curves(&mut self) {
        let mut model = BTreeMap::<String, usize>::new();
        for clip in &self.model().clips {
            for c in &clip.curves {
                let k = match &c.binding {
                    Binding::NodeActive { .. } => "nodeActive",
                    Binding::RendererEnabled { .. } => "rendererEnabled",
                    Binding::BlendShape { .. } => "blendShape",
                    Binding::MaterialProperty { .. } => "materialProperty",
                    Binding::Transform { property, .. } => match property {
                        TransformProperty::Translation => "translation",
                        TransformProperty::Rotation => "rotation",
                        TransformProperty::EulerRotation => "eulerRotation",
                        TransformProperty::Scale => "scale",
                    },
                    Binding::Muscle { .. } => "muscle",
                    Binding::AnimatorParameter { .. } => "animatorParameter",
                    Binding::PhysBone { .. } => "physBone",
                    Binding::Constraint { .. } => "constraint",
                    Binding::ContactSender { .. } => "contactSender",
                    Binding::ContactReceiver { .. } => "contactReceiver",
                };
                *model.entry(k.into()).or_default() += 1;
            }
            *model.entry("materialSlot".into()).or_default() += clip.slot_curves.len();
        }
        let unity: BTreeMap<String, usize> = self.census["curves"].as_object().into_iter().flatten().map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0) as usize)).collect();
        let mut faults = vec![];
        let (mut ut, mut mt, mut reported) = (0, 0, 0);
        for k in unity.keys().chain(model.keys()).collect::<BTreeSet<_>>() {
            let u = unity.get(k).copied().unwrap_or(0);
            let m = model.get(k).copied().unwrap_or(0);
            ut += u;
            mt += m;
            let accounted = if k == "slot past the renderer's slots" {
                self.unsupported("animated material slot past the renderer's slots")
            } else if k == "missing component" {
                self.unsupported("animation curve on a missing component")
            } else if k == "missing object" {
                self.unsupported("animation curve on a missing object")
            } else if let Some(rest) = k.strip_prefix("other reference ") {
                self.unsupported(&format!("animated object reference {rest}"))
            } else if let Some(rest) = k.strip_prefix("other ") {
                self.unsupported(&format!("animated {rest}"))
            } else {
                0
            };
            reported += accounted.min(u);
            if u != m + accounted {
                faults.push(format!("{k}: Unity {u}, model {m}, reported {accounted}"));
            }
        }
        let by: Vec<String> = model.iter().filter(|(_, n)| **n > 0).map(|(k, n)| format!("{n} {k}")).collect();
        self.report.line(&format!("curves by binding ({})", by.join(", ")), ut, format!("{mt} + {reported} reported"), faults);
    }

    fn clip_settings(&mut self) {
        use avatar_core::model::{HeightBasis, HumanoidClip, RotationBasis, XzBasis};
        const SETTINGS: [&str; 12] = [
            "loop", "cycleOffset", "additiveReferencePose", "mirror", "loopPose", "rotationBakeIntoPose", "rotationBasedUpon",
            "rotationOffsetDegrees", "heightBakeIntoPose", "heightBasedUpon", "heightOffset", "xzBakeIntoPose",
        ];
        const ALL: [&str; 13] = [
            SETTINGS[0], SETTINGS[1], SETTINGS[2], SETTINGS[3], SETTINGS[4], SETTINGS[5], SETTINGS[6], SETTINGS[7], SETTINGS[8],
            SETTINGS[9], SETTINGS[10], SETTINGS[11], "xzBasedUpon",
        ];
        type Settings = (String, BTreeMap<&'static str, Value>);
        let model = self.model();
        let humanoid_settings = |h: Option<&HumanoidClip>| -> [Value; 10] {
            let Some(h) = h else {
                return [json!(false), json!(false), json!(false), json!("bodyOrientation"), jf(0.0), json!(false), json!("centerOfMass"), jf(0.0), json!(false), json!("centerOfMass")];
            };
            let rotation = match h.rotation.based_upon {
                RotationBasis::Original => "original",
                RotationBasis::BodyOrientation => "bodyOrientation",
            };
            let height = match h.height.based_upon {
                HeightBasis::Original => "original",
                HeightBasis::CenterOfMass => "centerOfMass",
                HeightBasis::Feet => "feet",
            };
            let xz = match h.xz.based_upon {
                XzBasis::Original => "original",
                XzBasis::CenterOfMass => "centerOfMass",
            };
            [
                json!(h.mirror), json!(h.loop_pose), json!(h.rotation.bake_into_pose), json!(rotation), jf(h.rotation.offset_degrees),
                json!(h.height.bake_into_pose), json!(height), jf(h.height.offset), json!(h.xz.bake_into_pose), json!(xz),
            ]
        };
        let record = |name: &str, looping: Value, cycle: Value, reference: Value, humanoid: bool, h: [Value; 10]| -> Settings {
            let mut v: BTreeMap<&'static str, Value> = ALL[3..].iter().copied().zip(h).collect();
            v.extend([("loop", looping), ("cycleOffset", cycle), ("additiveReferencePose", reference), ("humanoid", json!(humanoid))]);
            (name.to_string(), v)
        };
        let model_settings: Vec<Settings> = model
            .clips
            .iter()
            .map(|c| {
                let reference = c.additive_reference_pose.as_ref().map_or(Value::Null, |r| json!({ "clip": model.clips[r.clip.0 as usize].name, "time": jf(r.time) }));
                record(&c.name, json!(c.looping), jf(c.cycle_offset), reference, c.humanoid.is_some(), humanoid_settings(c.humanoid.as_ref()))
            })
            .collect();
        let census = arr(&self.census["clipSettings"]);
        let lost_reference = census.iter().filter(|c| c["hasAdditiveReferencePose"] == json!(true) && c["additiveReferencePose"].is_null()).count();
        let unity_settings: Vec<Settings> = census
            .iter()
            .map(|c| {
                let reference = match &c["additiveReferencePose"] {
                    Value::Null => Value::Null,
                    r => json!({ "clip": r["clip"], "time": jf(num(&r["time"])) }),
                };
                let humanoid = c["humanMotion"] == json!(true);
                let flag = |k: &str| json!(humanoid && c[k] == json!(true));
                let h = if humanoid {
                    let t = |k: &str| c[k] == json!(true);
                    [
                        flag("mirror"), flag("loopBlend"), flag("loopBlendOrientation"),
                        json!(if t("keepOriginalOrientation") { "original" } else { "bodyOrientation" }),
                        jf(num(&c["orientationOffsetY"])), flag("loopBlendPositionY"),
                        json!(if t("keepOriginalPositionY") { "original" } else if t("heightFromFeet") { "feet" } else { "centerOfMass" }),
                        jf(num(&c["level"])), flag("loopBlendPositionXZ"),
                        json!(if t("keepOriginalPositionXZ") { "original" } else { "centerOfMass" }),
                    ]
                } else {
                    humanoid_settings(None)
                };
                record(c["name"].as_str().unwrap_or(""), c["loop"].clone(), jf(num(&c["cycleOffset"])), reference, humanoid, h)
            })
            .collect();
        let mut faults = vec![];
        let reported = self.unsupported("clip additive reference pose with no clip");
        if lost_reference != reported {
            faults.push(format!("{lost_reference} additive reference poses with no clip, {reported} reported"));
        }
        let humanoid = |s: &[Settings]| s.iter().filter(|(_, v)| v["humanoid"] == json!(true)).count();
        let (uh, mh) = (humanoid(&unity_settings), humanoid(&model_settings));
        if uh != mh {
            faults.push(format!("Unity {uh} humanoid clips, model {mh}"));
        }
        self.report.line("humanoid clips", uh, mh, faults);
        let neutral = humanoid_settings(None);
        for (i, k) in ALL.into_iter().enumerate() {
            let pairs = |s: &[Settings]| {
                let mut p: Vec<String> = s.iter().map(|(n, v)| format!("{n} = {}", v[k])).collect();
                p.sort();
                p
            };
            let (u, mut rest) = (pairs(&unity_settings), pairs(&model_settings));
            let mut faults = vec![];
            let mut equal = 0;
            for x in &u {
                match rest.iter().position(|y| y == x) {
                    Some(j) => {
                        rest.swap_remove(j);
                        equal += 1;
                    }
                    None => faults.push(format!("Unity clip {x} has no model clip equal to it")),
                }
            }
            faults.extend(rest.iter().map(|x| format!("model clip {x} is not Unity's")));
            let unset = match i {
                0 => json!(false),
                1 => jf(0.0),
                2 => Value::Null,
                _ => neutral[i - 3].clone(),
            };
            let set = unity_settings.iter().filter(|(_, v)| v[k] != unset).count();
            self.report.line(&format!("clip setting {k} ({set} not the default)"), u.len(), equal, faults);
        }
    }

    fn human_rest(&mut self) {
        let unity = &self.census["humanRest"];
        let mut faults = vec![];
        let reported = self.unsupported("humanoid rest pose Unity did not evaluate");
        let Some(rest) = &self.model().humanoid.rest else {
            if !unity.is_null() {
                faults.push("Unity's humanoid stream holds rest values the model lacks".into());
            } else if !self.model().humanoid.bones.is_empty() && reported != 1 {
                faults.push(format!("a humanoid avatar without rest values, {reported} reported"));
            }
            self.report.line("humanoid rest (body and IK goals)", 0, 0, faults);
            return;
        };
        let model = [("body", &rest.body), ("leftFoot", &rest.left_foot), ("rightFoot", &rest.right_foot), ("leftHand", &rest.left_hand), ("rightHand", &rest.right_hand)];
        let mut equal = 0;
        for (k, g) in model {
            let u: Vec<f32> = arr(&unity[k]).iter().map(num).collect();
            let m: Vec<f32> = g.position.iter().chain(&g.rotation).copied().collect();
            if u.len() == m.len() && u.iter().zip(&m).all(|(a, b)| same(*a, *b)) {
                equal += 1;
            } else {
                faults.push(format!("{k}: Unity {u:?}, model {m:?}"));
            }
        }
        self.report.line("humanoid rest (body and IK goals)", model.len(), equal, faults);
    }

    fn materials(&mut self) {
        let unity = arr(&self.census["materials"]);
        let model = &self.model().materials;
        let gltf = &self.avatar.gltf["materials"];
        let non_finite = self.unsupported("non-finite value in materials, written as 0 or the largest finite value") > 0;
        let mut faults = vec![];
        let (mut up, mut mp, mut dp, mut values) = (0, 0, 0, 0);
        let mut matched = vec![false; model.len()];
        let mut duplicated = 0;
        for u in unity {
            let Some(name) = u["name"].as_str() else { continue };
            let saved = arr(&u["properties"]);
            let props: BTreeSet<&str> = saved.iter().filter_map(Value::as_str).collect();
            duplicated += props.iter().filter(|p| saved.iter().filter(|s| s.as_str() == Some(p)).count() > 1).count();
            up += props.len();
            let keywords: Vec<&str> = arr(&u["keywords"]).iter().filter_map(Value::as_str).collect();
            let found = (0..model.len()).find(|&i| {
                !matched[i] && gltf[i]["name"].as_str() == Some(name) && model[i].shader_name == u["shader"].as_str().unwrap_or("")
                    && model[i].properties.keys().map(String::as_str).collect::<BTreeSet<_>>() == props
            });
            let Some(i) = found else {
                faults.push(format!("material {name} ({}): no model material with its shader and {} saved properties", u["shader"], props.len()));
                continue;
            };
            matched[i] = true;
            mp += model[i].properties.len();
            if model[i].keywords.iter().map(String::as_str).collect::<Vec<_>>() != keywords {
                faults.push(format!("material {name}: keywords {:?} in Unity (serialized {}), {:?} in the model", keywords, u["serializedKeywords"], model[i].keywords));
            }
            let declared: BTreeSet<&str> = u["values"].as_object().into_iter().flatten().map(|(k, _)| k.as_str()).collect();
            let modelled: BTreeSet<&str> = model[i].declared.keys().map(String::as_str).collect();
            if modelled != declared {
                faults.push(format!(
                    "material {name}: shader declares {:?} only in Unity, {:?} only in the model",
                    declared.difference(&modelled).collect::<Vec<_>>(),
                    modelled.difference(&declared).collect::<Vec<_>>()
                ));
            }
            dp += declared.len();
            let record = serde_json::to_value(&model[i].properties).unwrap();
            let resolved = serde_json::to_value(&model[i].declared).unwrap();
            for (prop, v) in u["values"].as_object().into_iter().flatten() {
                let v = arr(v);
                for (which, model) in [("saved", record.get(prop)), ("declared", resolved.get(prop))] {
                    let Some(model) = model else { continue };
                    values += 1;
                    let kind = v[0].as_str().unwrap_or("");
                    let expected = match kind {
                        "float" => json!({"float": jf(num(&v[1]))}),
                        "int" => json!({"int": v[1]}),
                        "color" | "vector" => json!({kind: [jf(num(&v[1])), jf(num(&v[2])), jf(num(&v[3])), jf(num(&v[4]))]}),
                        _ => {
                            let mut t = json!({"tiling": [jf(num(&v[2])), jf(num(&v[3]))], "offset": [jf(num(&v[4])), jf(num(&v[5]))]});
                            if v[1] == "Texture2D" {
                                t["texture"] = model["texture"].get("texture").cloned().unwrap_or(json!("<missing Texture2D>"));
                            }
                            json!({"texture": t})
                        }
                    };
                    if !values_equal(&expected, model, non_finite) {
                        faults.push(format!("material {name}.{prop} ({which}): Unity {expected}, model {model}"));
                    }
                }
            }
        }
        let kept = self.unsupported("material property saved under several types, kept the one the shader declares");
        if duplicated != kept {
            faults.push(format!("{duplicated} saved properties appear under several types, {kept} reported"));
        }
        let missing = unity.iter().any(|u| u["name"].is_null()) as usize;
        let named = unity.len() - unity.iter().filter(|u| u["name"].is_null()).count();
        if named + missing != model.len() {
            faults.push(format!("Unity {} materials, model {}", named + missing, model.len()));
        }
        self.report.line(&format!("materials ({up} Unity properties, {mp} model, {dp} shader-declared, {values} saved and declared values compared with Unity's)"), named + missing, model.len(), faults);
    }

    fn inactive(&mut self) {
        let unity: BTreeSet<u64> = arr(&self.census["authoredInactive"]).iter().filter_map(Value::as_u64).collect();
        let model: BTreeSet<u64> = self.model().nodes.iter().enumerate().filter(|(_, n)| !n.active).map(|(i, _)| i as u64).collect();
        let faults = unity
            .symmetric_difference(&model)
            .map(|i| format!("node {i}: {} in Unity, {} in the model", if unity.contains(i) { "inactive" } else { "active" }, if model.contains(i) { "inactive" } else { "active" }))
            .collect();
        self.report.line("initially inactive nodes", unity.len(), model.len(), faults);
    }

    fn components(&mut self) {
        let mut unrepresented = BTreeMap::<&str, usize>::new();
        for c in arr(&self.census["components"]) {
            let represented = match c["category"].as_str().unwrap_or("") {
                "animator" => c["onRoot"] == true,
                "other" => false,
                _ => true,
            };
            if !represented {
                *unrepresented.entry(c["type"].as_str().unwrap_or("")).or_default() += c["count"].as_u64().unwrap_or(0) as usize;
            }
        }
        let faults = unrepresented
            .iter()
            .filter(|(ty, n)| self.unsupported(ty) != **n)
            .map(|(ty, n)| format!("{n} × {ty}, {} reported", self.unsupported(ty)))
            .collect();
        let other: usize = unrepresented.values().sum();
        let kinds = self.model().unsupported.len();
        self.report.line(&format!("unsupported report ({kinds} kinds) covers every unrepresented component"), other, "reported", faults);
    }
}

fn mirror(v: &Value) -> Value {
    let a = arr(v);
    json!([jf(-num(&a[0])), jf(num(&a[1])), jf(num(&a[2]))])
}

fn mirror_euler(v: &Value) -> Value {
    let a = arr(v);
    json!([jf(num(&a[0])), jf(-num(&a[1])), jf(-num(&a[2]))])
}

fn keys(v: &Value) -> Value {
    let k: Vec<Value> = arr(v)
        .iter()
        .map(|k| {
            let k = arr(k);
            let mode = ["none", "in", "out", "both"][k[6].as_u64().unwrap_or(0).min(3) as usize];
            json!({"time": jf(num(&k[0])), "value": jf(num(&k[1])), "inTangent": jf(num(&k[2])), "outTangent": jf(num(&k[3])), "inWeight": jf(num(&k[4])), "outWeight": jf(num(&k[5])), "weightedMode": mode})
        })
        .collect();
    if k.is_empty() {
        json!("<absent>")
    } else {
        Value::Array(k)
    }
}

fn slope(t: f32) -> Value {
    if t == f32::INFINITY {
        json!("Infinity")
    } else if t == f32::NEG_INFINITY {
        json!("-Infinity")
    } else {
        json!(t)
    }
}

/// A non-finite Unity value matches the model's written stand-in (0, or the largest finite value
/// of its sign) when the export reports writing one.
fn values_equal(a: &Value, b: &Value, non_finite: bool) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => same(x.as_f64().unwrap() as f32, y.as_f64().unwrap() as f32),
        (Value::String(_), Value::Number(y)) if non_finite => {
            let (x, y) = (num(a), y.as_f64().unwrap() as f32);
            (x.is_nan() && y == 0.0) || (x == f32::INFINITY && y == f32::MAX) || (x == f32::NEG_INFINITY && y == f32::MIN)
        }
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| values_equal(a, b, non_finite)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| values_equal(v, w, non_finite))),
        _ => a == b,
    }
}

fn jf(f: f32) -> Value {
    if f.is_nan() {
        json!("NaN")
    } else {
        slope(f)
    }
}
