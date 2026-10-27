extends RefCounted

## MToon materials for Godot from VRM material data: VRM 1.0's `VRMC_materials_mtoon` on a glTF material, or a VRM
## 0.x `materialProperties` entry, which `from_vrm0` turns into the 1.0 form as three-vrm's v0 compatibility plugin
## does (LICENSE-three-vrm).

const INCLUDE := "res://addons/avatar_workshop/mtoon.gdshaderinc"
const OUTLINE_MODES := ["none", "worldCoordinates", "screenCoordinates"]

static var _shaders := {}


## Whether a glTF material dictionary carries MToon.
static func is_mtoon(material: Dictionary) -> bool:
	return _dict(material, "extensions").get("VRMC_materials_mtoon") is Dictionary


## A VRM 0.x material entry as the glTF material dictionary its 1.0 form would be, with `material`'s other fields;
## `material` unchanged where the entry is not MToon or VRM's unlit shaders. `queues` is every transparent render queue
## the file's entries use, by whether they write depth (`render_queues`).
static func from_vrm0(entry: Dictionary, material: Dictionary, queues: Dictionary) -> Dictionary:
	var shader := str(entry.get("shader", ""))
	if shader != "VRM/MToon" and not shader.begins_with("VRM/Unlit"):
		return material
	var f := _dict(entry, "floatProperties")
	var v := _dict(entry, "vectorProperties")
	var t := _dict(entry, "textureProperties")
	var keys := _dict(entry, "keywordMap")
	var transform := {}
	if v.has("_MainTex"):
		var st := _numbers(v, "_MainTex", [0, 0, 1, 1], 4)
		transform = {"KHR_texture_transform": {"offset": [st[0], 1.0 - st[3] - st[1]], "scale": [st[2], st[3]]}}
	var tex := func(name: String) -> Variant:
		return {"index": int(t[name]), "extensions": transform} if t.get(name) is int or t.get(name) is float else null
	var gamma := func(name: String, default: Array) -> Array:
		var c := _numbers(v, name, default, 4)
		return [pow(maxf(c[0], 0.0), 2.2), pow(maxf(c[1], 0.0), 2.2), pow(maxf(c[2], 0.0), 2.2)]
	var out := material.duplicate(true)
	var mtoon := {"specVersion": "1.0"}
	var zwrite: bool = shader == "VRM/UnlitTransparentZWrite" or (shader == "VRM/MToon" and _num(f, "_ZWrite", 0) == 1)
	var blend: bool = shader in ["VRM/UnlitTransparent", "VRM/UnlitTransparentZWrite"] or (shader == "VRM/MToon" and _true(keys, "_ALPHABLEND_ON"))
	var cutout: bool = shader == "VRM/UnlitCutout" or (shader == "VRM/MToon" and _true(keys, "_ALPHATEST_ON"))
	out.alphaMode = "BLEND" if blend else "MASK" if cutout else "OPAQUE"
	if cutout:
		out.alphaCutoff = _num(f, "_Cutoff", 0.5)
	if blend:
		mtoon.transparentWithZWrite = zwrite
		var used: Array = queues.get(zwrite, [])
		var queue := int(_num(entry, "renderQueue", -1))
		if used.has(queue):
			mtoon.renderQueueOffsetNumber = used.find(queue) - (0 if zwrite else used.size() - 1)
	var base: Array = gamma.call("_Color", [1, 1, 1, 1])
	base.append(_numbers(v, "_Color", [1, 1, 1, 1], 4)[3])
	out.pbrMetallicRoughness = {"baseColorFactor": base}
	if tex.call("_MainTex"):
		out.pbrMetallicRoughness.baseColorTexture = tex.call("_MainTex")
	if shader.begins_with("VRM/Unlit"):
		mtoon.shadeColorFactor = base.slice(0, 3)
		if tex.call("_MainTex"):
			mtoon.shadeMultiplyTexture = tex.call("_MainTex")
		out.extensions = {"VRMC_materials_mtoon": mtoon}
		return out
	out.doubleSided = _num(f, "_CullMode", 2) == 0
	if tex.call("_BumpMap"):
		out.normalTexture = tex.call("_BumpMap")
		out.normalTexture.scale = _num(f, "_BumpScale", 1.0)
	out.emissiveFactor = gamma.call("_EmissionColor", [0, 0, 0, 1])
	if tex.call("_EmissionMap"):
		out.emissiveTexture = tex.call("_EmissionMap")
	mtoon.shadeColorFactor = gamma.call("_ShadeColor", [0.97, 0.81, 0.86, 1])
	if tex.call("_ShadeTexture"):
		mtoon.shadeMultiplyTexture = tex.call("_ShadeTexture")
	var shift := _num(f, "_ShadeShift", 0.0)
	var toony := lerpf(_num(f, "_ShadeToony", 0.9), 1.0, 0.5 + 0.5 * shift)
	mtoon.shadingToonyFactor = toony
	mtoon.shadingShiftFactor = -shift - (1.0 - toony)
	mtoon.giEqualizationFactor = 1.0 - _num(f, "_IndirectLightIntensity", 0.1)
	if tex.call("_SphereAdd"):
		mtoon.matcapFactor = [1, 1, 1]
		mtoon.matcapTexture = {"index": tex.call("_SphereAdd").index}
	mtoon.rimLightingMixFactor = _num(f, "_RimLightingMix", 0.0)
	if tex.call("_RimTexture"):
		mtoon.rimMultiplyTexture = tex.call("_RimTexture")
	mtoon.parametricRimColorFactor = gamma.call("_RimColor", [0, 0, 0, 1])
	mtoon.parametricRimFresnelPowerFactor = _num(f, "_RimFresnelPower", 1.0)
	mtoon.parametricRimLiftFactor = _num(f, "_RimLift", 0.0)
	var mode := int(_num(f, "_OutlineWidthMode", 0))
	mtoon.outlineWidthMode = OUTLINE_MODES[mode] if mode >= 0 and mode < OUTLINE_MODES.size() else "none"
	mtoon.outlineWidthFactor = 0.01 * _num(f, "_OutlineWidth", 0.0)
	if tex.call("_OutlineWidthTexture"):
		mtoon.outlineWidthMultiplyTexture = tex.call("_OutlineWidthTexture")
	mtoon.outlineColorFactor = gamma.call("_OutlineColor", [0, 0, 0, 1])
	mtoon.outlineLightingMixFactor = _num(f, "_OutlineLightingMix", 1.0) if _num(f, "_OutlineColorMode", 0) == 1 else 0.0
	if tex.call("_UvAnimMaskTexture"):
		mtoon.uvAnimationMaskTexture = tex.call("_UvAnimMaskTexture")
	mtoon.uvAnimationScrollXSpeedFactor = _num(f, "_UvAnimScrollX", 0.0)
	mtoon.uvAnimationScrollYSpeedFactor = -_num(f, "_UvAnimScrollY", 0.0)
	mtoon.uvAnimationRotationSpeedFactor = _num(f, "_UvAnimRotation", 0.0)
	out.extensions = {"VRMC_materials_mtoon": mtoon}
	return out


## The transparent render queues of a file's VRM 0.x material entries, each list sorted, keyed by whether they write
## depth.
static func render_queues(entries: Array) -> Dictionary:
	var out := {true: [], false: []}
	for e: Variant in entries:
		if not e is Dictionary:
			continue
		var shader := str(e.get("shader", ""))
		var zwrite: bool = shader == "VRM/UnlitTransparentZWrite" or _num(_dict(e, "floatProperties"), "_ZWrite", 0) == 1
		var blend: bool = shader in ["VRM/UnlitTransparent", "VRM/UnlitTransparentZWrite"] or _dict(e, "keywordMap").has("_ALPHABLEND_ON")
		var queue := int(_num(e, "renderQueue", -1))
		if blend and queue >= 0 and not out[zwrite].has(queue):
			out[zwrite].append(queue)
	for k: bool in out:
		out[k].sort()
	return out


## The ShaderMaterial for a glTF material dictionary carrying MToon, its outline the next pass. `texture` answers a
## glTF texture index with its Texture2D, or null.
static func material(m: Dictionary, texture: Callable) -> ShaderMaterial:
	var x := _dict(_dict(m, "extensions"), "VRMC_materials_mtoon")
	var pbr := _dict(m, "pbrMetallicRoughness")
	var defines: Array[String] = []
	var modes: Array[String] = ["cull_disabled" if _true(m, "doubleSided") else "cull_back"]
	var alpha := str(m.get("alphaMode", "OPAQUE"))
	if alpha == "BLEND":
		defines.append("BLEND")
		if _true(x, "transparentWithZWrite"):
			modes.append("depth_draw_always")
	elif alpha == "MASK":
		defines.append("CUTOUT")
	var p := {}
	var base := _numbers(pbr, "baseColorFactor", [1, 1, 1, 1], 4)
	p.base_color = Vector4(base[0], base[1], base[2], base[3])
	var st := _dict(_dict(_dict(pbr, "baseColorTexture"), "extensions"), "KHR_texture_transform")
	var scale := _numbers(st, "scale", [1, 1], 2)
	var offset := _numbers(st, "offset", [0, 0], 2)
	p.uv_transform = Vector4(scale[0], scale[1], offset[0], offset[1])
	p.alpha_cutoff = _num(m, "alphaCutoff", 0.5)
	p.shade_color = _rgb(x, "shadeColorFactor", [0, 0, 0])
	p.shading_shift = _num(x, "shadingShiftFactor", 0.0)
	p.shading_toony = _num(x, "shadingToonyFactor", 0.9)
	p.emissive = _rgb(m, "emissiveFactor", [0, 0, 0]) * _emissive_strength(m)
	p.rim_color = _rgb(x, "parametricRimColorFactor", [0, 0, 0])
	p.rim_lighting_mix = _num(x, "rimLightingMixFactor", 1.0)
	p.rim_fresnel_power = _num(x, "parametricRimFresnelPowerFactor", 5.0)
	p.rim_lift = _num(x, "parametricRimLiftFactor", 0.0)
	p.matcap_color = _rgb(x, "matcapFactor", [1, 1, 1])
	p.outline_color = _rgb(x, "outlineColorFactor", [0, 0, 0])
	p.outline_lighting_mix = _num(x, "outlineLightingMixFactor", 1.0)
	p.uv_animation_speed = Vector3(_num(x, "uvAnimationScrollXSpeedFactor", 0.0), _num(x, "uvAnimationScrollYSpeedFactor", 0.0), _num(x, "uvAnimationRotationSpeedFactor", 0.0))
	var textures := [
		["base_texture", pbr.get("baseColorTexture"), ""],
		["shade_texture", x.get("shadeMultiplyTexture"), ""],
		["shift_texture", x.get("shadingShiftTexture"), "SHIFT_TEXTURE"],
		["normal_texture", m.get("normalTexture"), "NORMAL_TEXTURE"],
		["emissive_texture", m.get("emissiveTexture"), ""],
		["rim_texture", x.get("rimMultiplyTexture"), "RIM_TEXTURE"],
		["matcap_texture", x.get("matcapTexture"), "MATCAP"],
		["uv_animation_mask", x.get("uvAnimationMaskTexture"), ""],
	]
	for t: Array in textures:
		var tex := _texture(t[1], texture)
		if tex == null:
			continue
		p[t[0]] = tex
		if t[2] != "":
			defines.append(t[2])
	p.shift_texture_scale = _num(_dict(x, "shadingShiftTexture"), "scale", 1.0)
	p.normal_scale = _num(_dict(m, "normalTexture"), "scale", 1.0)
	if p.uv_animation_speed != Vector3.ZERO:
		defines.append("UV_ANIMATION")
	var out := _material(modes, defines, p)
	out.resource_name = str(m.get("name", ""))
	if alpha == "BLEND":
		var queue := int(_num(x, "renderQueueOffsetNumber", 0))
		out.render_priority = clampi(queue, 0, 9) - 19 if _true(x, "transparentWithZWrite") else clampi(queue, -9, 0)
	var mode := str(x.get("outlineWidthMode", "none"))
	p.outline_width = _num(x, "outlineWidthFactor", 0.0)
	if mode != "none" and p.outline_width > 0.0:
		var outline: Array[String] = defines.duplicate()
		outline.append("OUTLINE")
		if mode == "screenCoordinates":
			outline.append("OUTLINE_SCREEN")
		var tex := _texture(x.get("outlineWidthMultiplyTexture"), texture)
		if tex != null:
			p.outline_width_texture = tex
			outline.append("OUTLINE_TEXTURE")
		out.next_pass = _material(["cull_front"] + modes.slice(1), outline, p)
		out.next_pass.render_priority = out.render_priority
	return out


static func _material(modes: Array, defines: Array, params: Dictionary) -> ShaderMaterial:
	var code := "shader_type spatial;\nrender_mode %s;\n%s#include \"%s\"\n" % [", ".join(modes), "".join(defines.map(func(d: String) -> String: return "#define %s\n" % d)), INCLUDE]
	if not _shaders.has(code):
		var s := Shader.new()
		s.code = code
		_shaders[code] = s
	var out := ShaderMaterial.new()
	out.shader = _shaders[code]
	for k: String in params:
		out.set_shader_parameter(k, params[k])
	return out


static func _rgb(d: Dictionary, key: String, default: Array) -> Vector3:
	var c := _numbers(d, key, default, 3)
	return Vector3(c[0], c[1], c[2])


static func _num(d: Dictionary, key: String, default: float) -> float:
	var v: Variant = d.get(key)
	return float(v) if (v is float or v is int) and is_finite(float(v)) else default


## `d[key]`'s first `n` numbers, or `default` unless it holds that many finite numbers.
static func _numbers(d: Dictionary, key: String, default: Array, n: int) -> Array:
	var v: Variant = d.get(key)
	if not v is Array or v.size() < n or not v.slice(0, n).all(func(e: Variant) -> bool: return (e is float or e is int) and is_finite(float(e))):
		return default
	return v.slice(0, n)


static func _dict(d: Dictionary, key: String) -> Dictionary:
	var v: Variant = d.get(key)
	return v if v is Dictionary else {}


static func _texture(info: Variant, texture: Callable) -> Texture2D:
	if not info is Dictionary or not (info.get("index") is float or info.get("index") is int):
		return null
	return texture.call(int(info.index))


static func _emissive_strength(m: Dictionary) -> float:
	var e := _dict(m, "extensions")
	if e.has("KHR_materials_emissive_strength"):
		return _num(_dict(e, "KHR_materials_emissive_strength"), "emissiveStrength", 1.0)
	return _num(_dict(e, "VRMC_materials_hdr_emissiveMultiplier"), "emissiveMultiplier", 1.0)


static func _true(d: Dictionary, key: String) -> bool:
	return d.get(key) is bool and d.get(key)
