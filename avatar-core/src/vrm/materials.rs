//! VRM materials as the Unity materials UniVRM makes of them: VRM 1.0 MToon as `VRM10/MToon10` properties, VRM
//! 0.x MToon as the `VRM/MToon` properties it saved, other glTF materials as `Standard` or UniVRM's unlit shader.
//! Colours are as Unity stores them, sRGB-encoded, but for emission, which UniVRM keeps linear as HDR.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{floats, malformed, number, Doc, Error, Version};
use crate::model::{
    Blend, BlendFactor, BlendOp, CompareFunction, Cull, Material, MaterialProperty, ShaderFamily, TextureIndex,
    TextureProperty,
};

type Properties = BTreeMap<String, MaterialProperty>;

pub(super) fn to_srgb(c: [f32; 4]) -> [f32; 4] {
    let encode = |x: f32| if x <= 0.003_130_8 { 12.92 * x } else { 1.055 * x.max(0.0).powf(1.0 / 2.4) - 0.055 };
    [encode(c[0]), encode(c[1]), encode(c[2]), c[3]]
}

/// A VRM 1.0 expression's material colour type: the MToon10 property and whether it is stored sRGB-encoded.
pub(super) fn color_property(ty: &str) -> Option<(&'static str, bool)> {
    Some(match ty {
        "color" => ("_Color", true),
        "emissionColor" => ("_EmissionColor", false),
        "shadeColor" => ("_ShadeColor", true),
        "matcapColor" => ("_MatcapColor", true),
        "rimColor" => ("_RimColor", true),
        "outlineColor" => ("_OutlineColor", true),
        _ => return None,
    })
}

fn float(v: f32) -> MaterialProperty {
    MaterialProperty::Float(v)
}

fn color(v: [f32; 4]) -> MaterialProperty {
    MaterialProperty::Color(v)
}

fn no_texture() -> MaterialProperty {
    MaterialProperty::Texture(TextureProperty { texture: None, tiling: [1.0, 1.0], offset: [0.0, 0.0] })
}

impl Doc<'_> {
    /// A glTF texture reference as a Unity texture property, `KHR_texture_transform` as its tiling and offset on
    /// Unity's flipped V.
    fn texture(&mut self, info: Option<&Value>) -> Result<MaterialProperty, Error> {
        let Some(info) = info else { return Ok(no_texture()) };
        let index = info.get("index").and_then(Value::as_u64).ok_or_else(|| malformed("a material texture names no texture"))?;
        if index as usize >= self.array("textures").len() {
            return Err(malformed(format!("a material names texture {index}, but there are {}", self.array("textures").len())));
        }
        let transform = info.pointer("/extensions/KHR_texture_transform");
        let scale = floats::<2>(transform.and_then(|t| t.get("scale"))).unwrap_or([1.0, 1.0]);
        let offset = floats::<2>(transform.and_then(|t| t.get("offset"))).unwrap_or([0.0, 0.0]);
        if number(transform.and_then(|t| t.get("rotation")), 0.0) != 0.0 {
            self.report.add("vrm texture transform rotation");
        }
        let set = transform.and_then(|t| t.get("texCoord")).or(info.get("texCoord")).and_then(Value::as_u64).unwrap_or(0);
        if set != 0 {
            self.report.add("vrm material texture on a second UV set");
        }
        Ok(MaterialProperty::Texture(TextureProperty { texture: Some(TextureIndex(index as u32)), tiling: scale, offset: [offset[0], 1.0 - offset[1] - scale[1]] }))
    }
}

/// How a material blends and culls: opaque, cut-out or blended, its queue and its depth writes.
struct State {
    queue: i32,
    blend: Option<Blend>,
    depth_write: bool,
    cull: Cull,
}

fn alpha_blend() -> Blend {
    Blend {
        src_color: BlendFactor::SrcAlpha,
        dst_color: BlendFactor::OneMinusSrcAlpha,
        src_alpha: BlendFactor::One,
        dst_alpha: BlendFactor::OneMinusSrcAlpha,
        color_op: BlendOp::Add,
        alpha_op: BlendOp::Add,
    }
}

fn record(shader: ShaderFamily, name: &str, version: Option<&str>, properties: Properties, declared: Properties, keywords: Vec<String>, state: State) -> Material {
    Material {
        shader,
        shader_name: name.into(),
        version: version.map(str::to_owned),
        properties,
        declared,
        keywords,
        render_queue: state.queue,
        cull: state.cull,
        blend: state.blend,
        depth_write: state.depth_write,
        depth_test: CompareFunction::LessEqual,
    }
}

pub(super) fn materials(doc: &mut Doc) -> Result<Vec<Material>, Error> {
    let entries: Vec<Value> = doc.array("materials").to_vec();
    let v0: Vec<Value> = doc.vrm.get("materialProperties").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out = vec![];
    for (i, m) in entries.iter().enumerate() {
        let name = m.get("name").and_then(Value::as_str);
        let saved = match doc.version {
            Version::V0 => v0
                .get(i)
                .filter(|p| p.get("name").and_then(Value::as_str) == name)
                .or_else(|| v0.iter().find(|p| p.get("name").and_then(Value::as_str) == name))
                .or(v0.get(i))
                .filter(|p| p.get("shader").and_then(Value::as_str) != Some("VRM_USE_GLTFSHADER")),
            Version::V1 => None,
        };
        out.push(match (saved, m.pointer("/extensions/VRMC_materials_mtoon")) {
            (Some(p), _) => v0_material(doc, p)?,
            (None, Some(mtoon)) => mtoon10(doc, m, mtoon)?,
            (None, None) => gltf_material(doc, m)?,
        });
    }
    Ok(out)
}

fn gltf_state(m: &Value, offset: i32) -> (f32, State) {
    let mode = m.get("alphaMode").and_then(Value::as_str).unwrap_or("OPAQUE");
    let cull = if m.get("doubleSided").and_then(Value::as_bool) == Some(true) { Cull::Off } else { Cull::Back };
    match mode {
        "MASK" => (1.0, State { queue: 2450, blend: None, depth_write: true, cull }),
        "BLEND" => (2.0, State { queue: 3000 + offset.clamp(-9, 0), blend: Some(alpha_blend()), depth_write: false, cull }),
        _ => (0.0, State { queue: 2000, blend: None, depth_write: true, cull }),
    }
}

fn pbr_base(doc: &mut Doc, m: &Value, p: &mut Properties) -> Result<(), Error> {
    let pbr = m.get("pbrMetallicRoughness");
    p.insert("_Color".into(), color(to_srgb(floats::<4>(pbr.and_then(|x| x.get("baseColorFactor"))).unwrap_or([1.0; 4]))));
    p.insert("_MainTex".into(), doc.texture(pbr.and_then(|x| x.get("baseColorTexture")))?);
    p.insert("_Cutoff".into(), float(number(m.get("alphaCutoff"), 0.5)));
    Ok(())
}

fn gltf_material(doc: &mut Doc, m: &Value) -> Result<Material, Error> {
    let (_, state) = gltf_state(m, 0);
    let mut p = Properties::new();
    pbr_base(doc, m, &mut p)?;
    if m.pointer("/extensions/KHR_materials_unlit").is_some() {
        return Ok(record(ShaderFamily::Unknown, "UniGLTF/UniUnlit", None, p.clone(), p, vec![], state));
    }
    let pbr = m.get("pbrMetallicRoughness");
    p.insert("_Metallic".into(), float(number(pbr.and_then(|x| x.get("metallicFactor")), 1.0)));
    p.insert("_Glossiness".into(), float(1.0 - number(pbr.and_then(|x| x.get("roughnessFactor")), 1.0)));
    p.insert("_MetallicGlossMap".into(), doc.texture(pbr.and_then(|x| x.get("metallicRoughnessTexture")))?);
    p.insert("_BumpMap".into(), doc.texture(m.get("normalTexture"))?);
    p.insert("_BumpScale".into(), float(number(m.pointer("/normalTexture/scale"), 1.0)));
    p.insert("_OcclusionMap".into(), doc.texture(m.get("occlusionTexture"))?);
    p.insert("_OcclusionStrength".into(), float(number(m.pointer("/occlusionTexture/strength"), 1.0)));
    let [r, g, b] = floats::<3>(m.get("emissiveFactor")).unwrap_or([0.0; 3]);
    let strength = number(m.pointer("/extensions/KHR_materials_emissive_strength/emissiveStrength"), 1.0);
    p.insert("_EmissionColor".into(), color([r * strength, g * strength, b * strength, 1.0]));
    p.insert("_EmissionMap".into(), doc.texture(m.get("emissiveTexture"))?);
    Ok(record(ShaderFamily::Standard, "Standard", None, p.clone(), p, vec![], state))
}

/// VRM 1.0's MToon, every property of `VRMC_materials_mtoon` and the glTF material it extends, defaults filled.
fn mtoon10(doc: &mut Doc, m: &Value, x: &Value) -> Result<Material, Error> {
    let offset = x.get("renderQueueOffsetNumber").and_then(Value::as_i64).unwrap_or(0) as i32;
    let z_write = x.get("transparentWithZWrite").and_then(Value::as_bool) == Some(true);
    let (alpha_mode, mut state) = gltf_state(m, offset);
    if state.blend.is_some() && z_write {
        state = State { queue: 2501 + offset.clamp(0, 9), depth_write: true, ..state };
    }
    let mut p = Properties::new();
    pbr_base(doc, m, &mut p)?;
    let f = |k: &str, d: f32| float(number(x.get(k), d));
    let c3 = |v: Option<&Value>, d: [f32; 3], srgb: bool| {
        let [r, g, b] = floats::<3>(v).unwrap_or(d);
        let c = [r, g, b, 1.0];
        color(if srgb { to_srgb(c) } else { c })
    };
    p.insert("_AlphaMode".into(), float(alpha_mode));
    p.insert("_TransparentWithZWrite".into(), float(z_write as u8 as f32));
    p.insert("_RenderQueueOffset".into(), float(offset as f32));
    p.insert("_DoubleSided".into(), float((m.get("doubleSided").and_then(Value::as_bool) == Some(true)) as u8 as f32));
    p.insert("_ShadeColor".into(), c3(x.get("shadeColorFactor"), [0.0; 3], true));
    p.insert("_ShadeTex".into(), doc.texture(x.get("shadeMultiplyTexture"))?);
    p.insert("_BumpMap".into(), doc.texture(m.get("normalTexture"))?);
    p.insert("_BumpScale".into(), float(number(m.pointer("/normalTexture/scale"), 1.0)));
    p.insert("_ShadingShiftFactor".into(), f("shadingShiftFactor", 0.0));
    p.insert("_ShadingShiftTex".into(), doc.texture(x.get("shadingShiftTexture"))?);
    p.insert("_ShadingShiftTexScale".into(), float(number(x.pointer("/shadingShiftTexture/scale"), 1.0)));
    p.insert("_ShadingToonyFactor".into(), f("shadingToonyFactor", 0.9));
    p.insert("_GiEqualization".into(), f("giEqualizationFactor", 0.9));
    let strength = number(m.pointer("/extensions/KHR_materials_emissive_strength/emissiveStrength"), 1.0)
        * number(m.pointer("/extensions/VRMC_materials_hdr_emissiveMultiplier/emissiveMultiplier"), 1.0);
    let [r, g, b] = floats::<3>(m.get("emissiveFactor")).unwrap_or([0.0; 3]);
    p.insert("_EmissionColor".into(), color([r * strength, g * strength, b * strength, 1.0]));
    p.insert("_EmissionMap".into(), doc.texture(m.get("emissiveTexture"))?);
    p.insert("_MatcapColor".into(), c3(x.get("matcapFactor"), [1.0; 3], true));
    p.insert("_MatcapTex".into(), doc.texture(x.get("matcapTexture"))?);
    p.insert("_RimColor".into(), c3(x.get("parametricRimColorFactor"), [0.0; 3], true));
    p.insert("_RimFresnelPower".into(), f("parametricRimFresnelPowerFactor", 5.0));
    p.insert("_RimLift".into(), f("parametricRimLiftFactor", 0.0));
    p.insert("_RimTex".into(), doc.texture(x.get("rimMultiplyTexture"))?);
    p.insert("_RimLightingMix".into(), f("rimLightingMixFactor", 1.0));
    let outline = match x.get("outlineWidthMode").and_then(Value::as_str) {
        Some("worldCoordinates") => 1.0,
        Some("screenCoordinates") => 2.0,
        _ => 0.0,
    };
    p.insert("_OutlineWidthMode".into(), float(outline));
    p.insert("_OutlineWidth".into(), f("outlineWidthFactor", 0.0));
    p.insert("_OutlineWidthTex".into(), doc.texture(x.get("outlineWidthMultiplyTexture"))?);
    p.insert("_OutlineColor".into(), c3(x.get("outlineColorFactor"), [0.0; 3], true));
    p.insert("_OutlineLightingMix".into(), f("outlineLightingMixFactor", 1.0));
    p.insert("_UvAnimMaskTex".into(), doc.texture(x.get("uvAnimationMaskTexture"))?);
    p.insert("_UvAnimScrollXSpeed".into(), f("uvAnimationScrollXSpeedFactor", 0.0));
    p.insert("_UvAnimScrollYSpeed".into(), f("uvAnimationScrollYSpeedFactor", 0.0));
    p.insert("_UvAnimRotationSpeed".into(), f("uvAnimationRotationSpeedFactor", 0.0));
    Ok(record(ShaderFamily::Mtoon, "VRM10/MToon10", Some("1.0"), p.clone(), p, vec![], state))
}

/// MToon 0.x's declared properties and defaults, which a saved material overrides.
fn mtoon0_declared() -> Properties {
    let mut p = Properties::new();
    for (k, v) in [
        ("_Cutoff", 0.5),
        ("_BumpScale", 1.0),
        ("_ReceiveShadowRate", 1.0),
        ("_ShadingGradeRate", 1.0),
        ("_ShadeShift", 0.0),
        ("_ShadeToony", 0.9),
        ("_LightColorAttenuation", 0.0),
        ("_IndirectLightIntensity", 0.1),
        ("_RimLightingMix", 0.0),
        ("_RimFresnelPower", 1.0),
        ("_RimLift", 0.0),
        ("_OutlineWidth", 0.5),
        ("_OutlineScaledMaxDistance", 1.0),
        ("_OutlineLightingMix", 1.0),
        ("_UvAnimScrollX", 0.0),
        ("_UvAnimScrollY", 0.0),
        ("_UvAnimRotation", 0.0),
        ("_MToonVersion", 39.0),
        ("_DebugMode", 0.0),
        ("_BlendMode", 0.0),
        ("_OutlineWidthMode", 0.0),
        ("_OutlineColorMode", 0.0),
        ("_CullMode", 2.0),
        ("_OutlineCullMode", 1.0),
        ("_SrcBlend", 1.0),
        ("_DstBlend", 0.0),
        ("_ZWrite", 1.0),
        ("_AlphaToMask", 0.0),
    ] {
        p.insert(k.into(), float(v));
    }
    for (k, v) in [
        ("_Color", [1.0, 1.0, 1.0, 1.0]),
        ("_ShadeColor", [0.97, 0.81, 0.86, 1.0]),
        ("_RimColor", [0.0, 0.0, 0.0, 1.0]),
        ("_EmissionColor", [0.0, 0.0, 0.0, 1.0]),
        ("_OutlineColor", [0.0, 0.0, 0.0, 1.0]),
    ] {
        p.insert(k.into(), color(v));
    }
    for k in [
        "_MainTex",
        "_ShadeTexture",
        "_BumpMap",
        "_ReceiveShadowTexture",
        "_ShadingGradeTexture",
        "_RimTexture",
        "_SphereAdd",
        "_EmissionMap",
        "_OutlineWidthTexture",
        "_UvAnimMaskTexture",
    ] {
        p.insert(k.into(), no_texture());
    }
    p
}

/// A VRM 0.x `materialProperties` entry: Unity's own saved values. A texture's tiling and offset are saved as a
/// vector under its name.
fn v0_material(doc: &mut Doc, e: &Value) -> Result<Material, Error> {
    let shader = e.get("shader").and_then(Value::as_str).unwrap_or_default().to_owned();
    let mut p = Properties::new();
    let floats_of = |k: &str| e.get(k).and_then(Value::as_object).cloned().unwrap_or_default();
    let textures = floats_of("textureProperties");
    for (k, v) in floats_of("floatProperties") {
        p.insert(k, float(v.as_f64().unwrap_or(0.0) as f32));
    }
    let colors = ["_Color", "_ShadeColor", "_RimColor", "_EmissionColor", "_OutlineColor"];
    for (k, v) in floats_of("vectorProperties") {
        let Some(v) = floats::<4>(Some(&v)) else { continue };
        if textures.contains_key(&k) || k == "_MainTex" {
            continue;
        }
        p.insert(k.clone(), if colors.contains(&k.as_str()) { color(v) } else { MaterialProperty::Vector(v) });
    }
    let vectors = floats_of("vectorProperties");
    for (k, v) in &textures {
        let index = v.as_u64().ok_or_else(|| malformed(format!("material texture {k} names no texture")))?;
        if index as usize >= doc.array("textures").len() {
            return Err(malformed(format!("material texture {k} names texture {index}, but there are {}", doc.array("textures").len())));
        }
        let st = floats::<4>(vectors.get(k)).unwrap_or([1.0, 1.0, 0.0, 0.0]);
        p.insert(k.clone(), MaterialProperty::Texture(TextureProperty { texture: Some(TextureIndex(index as u32)), tiling: [st[0], st[1]], offset: [st[2], st[3]] }));
    }
    let keywords: Vec<String> = floats_of("keywordMap").into_iter().filter(|(_, on)| on.as_bool() == Some(true)).map(|(k, _)| k).collect();
    let queue = e.get("renderQueue").and_then(Value::as_i64).map(|q| q as i32);
    let get = |k: &str, d: f32| match p.get(k) {
        Some(MaterialProperty::Float(v)) => *v,
        _ => d,
    };
    let (family, declared, state) = match shader.as_str() {
        "VRM/MToon" => {
            let mut declared = mtoon0_declared();
            declared.extend(p.clone());
            let mode = get("_BlendMode", 0.0) as i32;
            let cull = match get("_CullMode", 2.0) as i32 {
                0 => Cull::Off,
                1 => Cull::Front,
                _ => Cull::Back,
            };
            let factor = |k: &str, d: f32| -> BlendFactor {
                let all = [
                    BlendFactor::Zero,
                    BlendFactor::One,
                    BlendFactor::DstColor,
                    BlendFactor::SrcColor,
                    BlendFactor::OneMinusDstColor,
                    BlendFactor::SrcAlpha,
                    BlendFactor::OneMinusSrcColor,
                    BlendFactor::DstAlpha,
                    BlendFactor::OneMinusDstAlpha,
                    BlendFactor::SrcAlphaSaturate,
                    BlendFactor::OneMinusSrcAlpha,
                ];
                all.get(get(k, d) as usize).copied().unwrap_or(BlendFactor::One)
            };
            let blended = mode >= 2;
            let blend = Blend { src_color: factor("_SrcBlend", 1.0), dst_color: factor("_DstBlend", 0.0), src_alpha: factor("_SrcBlend", 1.0), dst_alpha: factor("_DstBlend", 0.0), color_op: BlendOp::Add, alpha_op: BlendOp::Add };
            let default_queue = [2000, 2450, 3000, 2501][mode.clamp(0, 3) as usize];
            (ShaderFamily::Mtoon, declared, State { queue: queue.unwrap_or(default_queue), blend: blended.then_some(blend), depth_write: get("_ZWrite", 1.0) != 0.0, cull })
        }
        other => {
            doc.report.add(format!("vrm shader {other}"));
            let (blended, default_queue, z) = match other {
                "VRM/UnlitCutout" => (false, 2450, true),
                "VRM/UnlitTransparent" => (true, 3000, false),
                "VRM/UnlitTransparentZWrite" => (true, 2501, true),
                _ => (false, 2000, true),
            };
            (ShaderFamily::Unknown, p.clone(), State { queue: queue.unwrap_or(default_queue), blend: blended.then(alpha_blend), depth_write: z, cull: Cull::Back })
        }
    };
    Ok(record(family, &shader, (family == ShaderFamily::Mtoon).then_some("0.x"), p, declared, keywords, state))
}
