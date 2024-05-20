#version 300 es

uniform mat4 projectionView;
in vec3 pos;

void main() {
	gl_Position = projectionView * vec4(pos + vec3(float((gl_InstanceID % 100) - 50) / 10.f, -1, -float(gl_InstanceID / 100) / 10.f), 1);
}
