#version 300 es

precision highp float;

uniform sampler2D shadowMap;
in vec3 v_normal;
in vec4 shadowPos;
out vec4 outColor;
const vec3 grassColor = vec3(0, 1, 0);

void main() {
	// outColor = vec4(0, 1, depth, 1);

	vec3 normShadowPos = shadowPos.xyz / shadowPos.w;
	bool inRange = normShadowPos.x >= 0.0f &&
		normShadowPos.x <= 1.0f &&
		normShadowPos.y >= 0.0f &&
		normShadowPos.y <= 1.0f;

	// the 'r' channel has the depth values
	float currentDepth = normShadowPos.z - 0.001f;
	float projectedDepth = texture(shadowMap, normShadowPos.xy).r;
	float shadowLight = (inRange && projectedDepth <= currentDepth) ? 0.2f : 1.0f;
	outColor = vec4(grassColor * shadowLight, 1);
	// outColor = vec4(v_normal, 1);
	// outColor = inRange ? vec4(vec3(1.f - projectedDepth), 1) : vec4(0, normShadowPos.x, normShadowPos.y, 1.0f);
}
