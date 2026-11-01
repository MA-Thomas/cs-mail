#include <metal_stdlib>
using namespace metal;

// One interleaved material, never independent drawable fleck/binder layers.
// Channels: pigment response, neutral response, neutral alpha contribution,
// fragment alpha. Sampling wraps in window coordinates, including interpolation.
static float4 mineralSample(device const uchar4 *material, int2 pixel) {
    int2 wrapped = (pixel % 1536 + 1536) % 1536;
    return float4(material[wrapped.y * 1536 + wrapped.x]) / 255.0;
}

[[ stitchable ]] half4 interfaceBTerrazzo(float2 position, float2 origin,
    float3 pigment, float opacity, device const void *bytes, int byteCount) {
    if (byteCount != 1536 * 1536 * 4) return half4(0);
    device const uchar4 *material = static_cast<device const uchar4 *>(bytes);
    float2 point = position + origin - 0.5;
    int2 pixel = int2(floor(point));
    float2 fraction = fract(point);
    float4 sample = mix(mix(mineralSample(material, pixel),
                            mineralSample(material, pixel + int2(1, 0)), fraction.x),
                        mix(mineralSample(material, pixel + int2(0, 1)),
                            mineralSample(material, pixel + int2(1, 1)), fraction.x), fraction.y);
    return half4(half3(opacity * (sample.x * pigment + sample.y * (1 - sample.w * opacity))), half(opacity * (sample.w + sample.z * (1 - sample.w * opacity))));
}
