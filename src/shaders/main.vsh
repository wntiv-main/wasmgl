#version 300 es

uniform mat4 projection;
uniform mat4 view;
uniform mat4 shadowView;
uniform vec3 lightPos;
in vec3 pos;
out vec4 shadowPos;
in vec3 normal;
out vec3 v_normal;
out vec3 surfaceToView;
out vec3 surfaceToLight;

void main() {
	vec4 modelPos = vec4(pos + vec3(float((gl_InstanceID % 100) - 50) / 10.f, 0, -float(gl_InstanceID / 100) / 10.f), 1);

	// orient the normals and pass to the fragment shader
	v_normal = mat3(view) * normal;

	// compute the world position of the surface
	vec3 surfaceWorldPosition = (view * vec4(pos, 1)).xyz;

	// compute the vector of the surface to the light
	// and pass it to the fragment shader
	surfaceToLight = lightPos - surfaceWorldPosition;

	// compute the vector of the surface to the view/camera
	// and pass it to the fragment shader
	surfaceToView = -view[3].xyz - surfaceWorldPosition;

	shadowPos = (shadowView * modelPos);
	gl_Position = projection * view * modelPos;
}
