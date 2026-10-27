extends SceneTree

const MToon = preload("res://addons/avatar_workshop/mtoon.gd")
var failures := 0
var count := 0


func check(ok: bool, label: String) -> void:
	count += 1
	if not ok:
		printerr("FAIL: " + label)
		failures += 1


func _init() -> void:
	var white := ImageTexture.create_from_image(Image.create(2, 2, false, Image.FORMAT_RGBA8))
	var texture := func(i: int) -> Texture2D: return white if i == 0 else null
	var v0 := {
		"shader": "VRM/MToon",
		"renderQueue": 3001,
		"keywordMap": {"_ALPHABLEND_ON": true},
		"floatProperties": {"_ShadeShift": -0.2, "_ShadeToony": 0.8, "_OutlineWidthMode": 1, "_OutlineWidth": 0.5, "_OutlineColorMode": 1, "_OutlineLightingMix": 0.5, "_CullMode": 0, "_ZWrite": 1, "_BumpScale": 2.0},
		"vectorProperties": {"_Color": [0.5, 0.5, 0.5, 0.25], "_ShadeColor": [0.5, 1, 0, 1], "_MainTex": [0, 0, 2, 1]},
		"textureProperties": {"_MainTex": 0, "_BumpMap": 0, "_SphereAdd": 0},
	}
	var queues := MToon.render_queues([v0, {"shader": "VRM/MToon", "renderQueue": 3005, "keywordMap": {"_ALPHABLEND_ON": true}, "floatProperties": {"_ZWrite": 1}}])
	var m := MToon.from_vrm0(v0, {"name": "skin"}, queues)
	var x: Dictionary = m.extensions.VRMC_materials_mtoon
	check(is_equal_approx(m.pbrMetallicRoughness.baseColorFactor[0], pow(0.5, 2.2)) and m.pbrMetallicRoughness.baseColorFactor[3] == 0.25, "0.x colours are gamma, alpha linear")
	check(is_equal_approx(x.shadingToonyFactor, lerpf(0.8, 1.0, 0.4)) and is_equal_approx(x.shadingShiftFactor, 0.2 - (1.0 - x.shadingToonyFactor)), "0.x shading shift and toony as three-vrm converts them")
	check(is_equal_approx(x.outlineWidthFactor, 0.005) and x.outlineWidthMode == "worldCoordinates" and x.outlineLightingMixFactor == 0.5, "0.x outline width in centimetres becomes metres")
	check(m.alphaMode == "BLEND" and m.doubleSided and x.transparentWithZWrite and x.renderQueueOffsetNumber == 0, "0.x blending, culling and render queue")
	check(m.name == "skin" and m.normalTexture.scale == 2.0 and m.pbrMetallicRoughness.baseColorTexture.extensions.KHR_texture_transform.scale == [2, 1], "0.x textures keep their scale and the glTF fields")
	var mat := MToon.material(m, texture)
	check(mat.shader.code.contains("cull_disabled") and mat.shader.code.contains("depth_draw_always") and mat.shader.code.contains("#define BLEND"), "render modes follow alpha and culling")
	check(mat.shader.code.contains("#define NORMAL_TEXTURE") and mat.shader.code.contains("#define MATCAP"), "optional textures turn their features on")
	check(mat.get_shader_parameter("base_color").is_equal_approx(Vector4(pow(0.5, 2.2), pow(0.5, 2.2), pow(0.5, 2.2), 0.25)), "colour factors reach the shader linear, as vectors: a Color is sRGB-decoded")
	check(mat.next_pass is ShaderMaterial and mat.next_pass.shader.code.contains("cull_front") and mat.next_pass.shader.code.contains("#define OUTLINE") and not mat.next_pass.shader.code.contains("cull_disabled"), "an outline is a front-culled next pass")
	check(mat.shader == MToon.material(m, texture).shader, "materials alike share one shader")
	var opaque := MToon.material({"extensions": {"VRMC_materials_mtoon": {"outlineWidthMode": "screenCoordinates", "outlineWidthFactor": 0.01}}}, texture)
	check(opaque.shader.code.contains("cull_back") and not opaque.shader.code.contains("#define NORMAL_TEXTURE") and opaque.next_pass.shader.code.contains("OUTLINE_SCREEN"), "defaults: one-sided, opaque, no optional textures")
	check(MToon.material({"extensions": {"VRMC_materials_mtoon": {"outlineWidthMode": "worldCoordinates", "outlineWidthFactor": 0.0}}}, texture).next_pass == null, "a zero-width outline draws nothing")
	var bad := {"name": 4, "alphaMode": [], "doubleSided": "yes", "pbrMetallicRoughness": {"baseColorFactor": ["a", 1, 1, 1], "baseColorTexture": {"index": "0"}}, "normalTexture": 7, "extensions": {"VRMC_materials_mtoon": {"shadeColorFactor": [INF, 0, 0], "outlineWidthMode": 3, "outlineWidthFactor": "wide", "shadingToonyFactor": NAN, "matcapTexture": {"index": 99}}}}
	var b := MToon.material(bad, texture)
	check(b.get_shader_parameter("base_color") == Vector4(1, 1, 1, 1) and b.get_shader_parameter("shade_color") == Vector3.ZERO and b.get_shader_parameter("shading_toony") == 0.9 and b.next_pass == null, "malformed values fall back to the defaults")
	check(MToon.from_vrm0({"shader": "VRM/MToon", "floatProperties": [], "vectorProperties": {"_Color": "red"}, "textureProperties": {"_MainTex": "x"}}, {}, MToon.render_queues([1, "x"])).has("extensions"), "malformed 0.x entries convert with defaults")
	check(MToon.from_vrm0({"shader": "Standard"}, {"name": "glass"}, {}) == {"name": "glass"}, "other 0.x shaders leave the glTF material")
	var off := MToon.from_vrm0({"shader": "VRM/MToon", "keywordMap": {"_ALPHABLEND_ON": false}, "floatProperties": {"_OutlineWidthMode": 3, "_OutlineWidth": 1.0}}, {}, {})
	check(off.alphaMode == "OPAQUE" and off.extensions.VRMC_materials_mtoon.outlineWidthMode == "none", "a keyword set false and an unknown outline mode turn nothing on")
	check(MToon.from_vrm0({"shader": "VRM/UnlitTexture"}, {"doubleSided": true}, {}).doubleSided, "a 0.x unlit material keeps the glTF material's sides")
	var queued := func(offset: float, zwrite: bool) -> int:
		return MToon.material({"alphaMode": "BLEND", "extensions": {"VRMC_materials_mtoon": {"renderQueueOffsetNumber": offset, "transparentWithZWrite": zwrite}}}, texture).render_priority
	check(queued.call(1000, false) == 0 and queued.call(-1000, false) == -9 and queued.call(1000, true) == -10 and queued.call(-3, true) == -19, "render queue offsets keep to MToon's ranges, depth-writing ones drawn first")
	print("mtoon: %s (%d checks)" % ["ok" if failures == 0 else "FAILED", count])
	quit(0 if failures == 0 else 1)
