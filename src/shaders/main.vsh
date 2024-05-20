#version 300 es

uniform mat4 projectionView;
uniform mat4 shadowView;
in vec3 pos;
out float depth;

void main() {
	vec4 modelPos = vec4(pos + vec3(float((gl_InstanceID % 100) - 50) / 10.f, -1, -float(gl_InstanceID / 100) / 10.f), 1);
	depth = (shadowView * modelPos).z;
	gl_Position = projectionView * modelPos;
}
