#version 300 es

precision highp float;

in float depth;
out vec4 outColor;

void main() {
	outColor = vec4(0, 1, depth, 1);
}
