uniform vec4 base_color = vec4(1.0);
uniform sampler2D base_texture : source_color, hint_default_white, filter_linear_mipmap, repeat_enable;
uniform vec4 uv_transform = vec4(1.0, 1.0, 0.0, 0.0);
uniform float alpha_cutoff = 0.5;
uniform vec3 shade_color = vec3(0.0);
uniform sampler2D shade_texture : source_color, hint_default_white, filter_linear_mipmap, repeat_enable;
uniform float shading_shift = 0.0;
uniform float shading_toony = 0.9;
uniform sampler2D shift_texture : hint_default_black, filter_linear_mipmap, repeat_enable;
uniform float shift_texture_scale = 1.0;
uniform sampler2D normal_texture : hint_normal, filter_linear_mipmap, repeat_enable;
uniform float normal_scale = 1.0;
uniform vec3 emissive = vec3(0.0);
uniform sampler2D emissive_texture : source_color, hint_default_white, filter_linear_mipmap, repeat_enable;
uniform vec3 rim_color = vec3(0.0);
uniform float rim_lighting_mix = 1.0;
uniform float rim_fresnel_power = 5.0;
uniform float rim_lift = 0.0;
uniform sampler2D rim_texture : source_color, hint_default_white, filter_linear_mipmap, repeat_enable;
uniform vec3 matcap_color = vec3(1.0);
uniform sampler2D matcap_texture : source_color, hint_default_black, filter_linear_mipmap, repeat_disable;
uniform vec3 outline_color = vec3(0.0);
uniform float outline_width = 0.0;
uniform float outline_lighting_mix = 1.0;
uniform sampler2D outline_width_texture : hint_default_white, filter_linear, repeat_enable;
uniform sampler2D uv_animation_mask : hint_default_white, filter_linear, repeat_enable;
// Scroll x and y in UV per second, rotation in radians per second.
uniform vec3 uv_animation_speed = vec3(0.0);

varying vec3 shade;
varying float shift;
varying vec3 lit_rim;
varying vec3 normal;

void vertex() {
#ifdef OUTLINE
	float width = outline_width;
#ifdef OUTLINE_TEXTURE
	width *= textureLod(outline_width_texture, UV * uv_transform.xy + uv_transform.zw, 0.0).g;
#endif
	// A world width keeps its length however the model is scaled; a screen width, its share of the view's height.
	width /= length(mat3(MODEL_MATRIX) * NORMAL);
#ifdef OUTLINE_SCREEN
	width *= -(MODELVIEW_MATRIX * vec4(VERTEX, 1.0)).z / abs(PROJECTION_MATRIX[1][1]);
#endif
	VERTEX += NORMAL * width;
#endif
}

void fragment() {
	vec2 uv = UV;
#ifdef UV_ANIMATION
	float mask = texture(uv_animation_mask, uv).b;
	float turn = uv_animation_speed.z * TIME * mask;
	uv = mat2(vec2(cos(turn), -sin(turn)), vec2(sin(turn), cos(turn))) * (uv - 0.5) + 0.5;
	uv += uv_animation_speed.xy * TIME * mask;
#endif
	uv = uv * uv_transform.xy + uv_transform.zw;
#ifdef OUTLINE
	vec3 tint = outline_color * outline_lighting_mix;
#else
	vec3 tint = vec3(1.0);
#endif
	vec4 base = base_color * texture(base_texture, uv);
	shade = shade_color * texture(shade_texture, uv).rgb * tint;
	shift = shading_shift;
#ifdef SHIFT_TEXTURE
	shift += texture(shift_texture, uv).r * shift_texture_scale;
#endif
	normal = NORMAL;
#ifdef NORMAL_TEXTURE
	vec3 m = texture(normal_texture, uv).xyz * 2.0 - 1.0;
	m.xy *= normal_scale;
	normal = normalize(TANGENT * m.x + BINORMAL * m.y + NORMAL * m.z);
#endif
#ifdef OUTLINE
	normal = -normal;
#endif
	vec3 rim = rim_color * pow(clamp(1.0 - dot(normal, VIEW) + rim_lift, 0.0, 1.0), rim_fresnel_power);
#ifdef MATCAP
	vec3 x = normalize(vec3(VIEW.z, 0.0, -VIEW.x));
	vec3 y = cross(VIEW, x);
	rim += matcap_color * texture(matcap_texture, 0.5 + 0.5 * vec2(dot(x, normal), -dot(y, normal))).rgb;
#endif
#ifdef RIM_TEXTURE
	rim *= texture(rim_texture, uv).rgb;
#endif
	lit_rim = rim * rim_lighting_mix * tint;
	ALBEDO = base.rgb * tint;
	EMISSION = (emissive * texture(emissive_texture, uv).rgb + rim * (1.0 - rim_lighting_mix)) * tint;
#ifdef OUTLINE
	EMISSION += outline_color * (1.0 - outline_lighting_mix);
#endif
	NORMAL = normal;
	ROUGHNESS = 1.0;
	METALLIC = 0.0;
	SPECULAR = 0.0;
#if defined(BLEND)
	ALPHA = base.a;
#elif defined(CUTOUT)
	ALPHA = base.a;
	ALPHA_SCISSOR_THRESHOLD = alpha_cutoff;
#endif
}

void light() {
	vec3 light = LIGHT_COLOR / PI;
	float shading = clamp((dot(normal, LIGHT) + shift + 1.0 - shading_toony) / max(2.0 - 2.0 * shading_toony, 1e-5), 0.0, 1.0);
	if (LIGHT_IS_DIRECTIONAL) {
		shading *= ATTENUATION;
	} else {
		light *= ATTENUATION;
	}
	// Godot multiplies DIFFUSE_LIGHT by ALBEDO, the lit colour; the shade colour's share goes past it.
	DIFFUSE_LIGHT += light * shading;
	SPECULAR_LIGHT += light * ((1.0 - shading) * shade + lit_rim);
}
