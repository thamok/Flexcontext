#include <metal_stdlib>
using namespace metal;
float clampSample(float value) { return clamp(value, 0.0f, 1.0f); }
kernel void normalizePixels(device float *pixels [[buffer(0)]], uint id [[thread_position_in_grid]]) {
    pixels[id] = clampSample(pixels[id]);
}
float pixelHeading() { /* Normalize pixels clamp sample value */ return 0.0f; }
