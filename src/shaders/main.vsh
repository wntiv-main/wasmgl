#version 300 es

uniform mat4 projectionView;
uniform mat4 shadowView;
in vec3 pos;
out vec4 shadowPos;
in vec3 normal;
out vec3 v_normal;

void main() {
	vec4 modelPos = vec4(pos + vec3(float((gl_InstanceID % 100) - 50) / 10.f, 0, -float(gl_InstanceID / 100) / 10.f), 1);

	// orient the normals and pass to the fragment shader
	v_normal = mat3(view) * normal;

	// compute the world position of the surface
	vec3 surfaceWorldPosition = (view * pos).xyz;

	// compute the vector of the surface to the light
	// and pass it to the fragment shader
	v_surfaceToLight = -shadowView[3].xyz - surfaceWorldPosition;

	// compute the vector of the surface to the view/camera
	// and pass it to the fragment shader
	v_surfaceToView = -projectionView[3].xyz - surfaceWorldPosition;

	shadowPos = (shadowView * modelPos);
	gl_Position = projectionView * modelPos;
}
