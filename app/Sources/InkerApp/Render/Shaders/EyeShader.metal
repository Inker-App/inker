// EyeShader: the Nosy Ones, shaded analytically per grid cell and drawn as dots.
// A line-by-line port of `eyeDraw` in the App v2 boards; the CPU reference is
// InkerKit/Eyes/EyeShading.swift (tested). Keep the two in step.
//
// One full-screen triangle; every pixel finds its cell, shades the cell centre
// for each eye that covers it, and keeps the largest paper dot and the largest
// red (iris) dot. Red is composited over paper, as the boards fill `acc` last.

#include <metal_stdlib>
using namespace metal;

struct InkVertexOut {
    float4 position [[position]];
};

// Full-screen triangle from the vertex id; no vertex buffer.
vertex InkVertexOut ink_fullscreen_vertex(uint vid [[vertex_id]]) {
    float2 uv = float2(float((vid << 1) & 2), float(vid & 2));
    InkVertexOut out;
    out.position = float4(uv * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

// Must match EyeHeaderGPU in EyeRenderer.swift (all float4, 16-byte aligned).
struct EyeHeader {
    float4 viewport; // x: width (pt), y: height (pt), z: pixels per point, w: cell (pt)
    float4 info;     // x: eye count
    float4 paper;    // sRGB, straight alpha
    float4 red;
};

// Must match EyeGPU in EyeRenderer.swift.
struct EyeData {
    float4 a; // x: centre x, y: centre y, z: radius, w: lid
    float4 b; // xyz: gaze, w: spin
};

static float ink_smoothstep(float a, float b, float x) {
    float t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Antialiased disc coverage at distance d (pt) for radius r (pt).
static float ink_coverage(float d, float r, float pxPerPt) {
    if (r <= 0.0) { return 0.0; }
    return clamp((r - d) * pxPerPt + 0.5, 0.0, 1.0);
}

fragment float4 eye_fragment(InkVertexOut in [[stage_in]],
                             constant EyeHeader &h [[buffer(0)]],
                             constant EyeData *eyes [[buffer(1)]]) {
    const float scale = h.viewport.z;
    const float C = h.viewport.w;
    const float2 p = in.position.xy / scale; // points, top-left origin
    const float gw = floor(h.viewport.x / C);
    const float gh = floor(h.viewport.y / C);
    const float2 ij = floor(p / C);
    if (ij.x < 0.0 || ij.y < 0.0 || ij.x > gw - 1.0 || ij.y > gh - 1.0) {
        return float4(0.0);
    }
    const float2 c = (ij + 0.5) * C;
    const float dist = length(p - c);

    // LGT = n3(-0.45, -0.55, 0.7); HV = n3(LGT.x, LGT.y, LGT.z + 1)
    const float3 L = normalize(float3(-0.45, -0.55, 0.7));
    const float3 HV = normalize(float3(L.x, L.y, L.z + 1.0));

    float rPaper = 0.0;
    float rRed = 0.0;
    const int n = int(h.info.x);
    for (int e = 0; e < n; e++) {
        const float cx = eyes[e].a.x;
        const float cy = eyes[e].a.y;
        const float R = eyes[e].a.z;
        const float lid = eyes[e].a.w;
        const float3 g = eyes[e].b.xyz;
        const float spin = eyes[e].b.w;

        const float dx = (c.x - cx) / R;
        const float dy = (c.y - cy) / R;
        const float d2 = dx * dx + dy * dy;
        if (d2 > 1.0) { continue; }
        const float z = sqrt(1.0 - d2);
        const float lam = max(0.0, dx * L.x + dy * L.y + z * L.z);
        const float sp = pow(max(0.0, dx * HV.x + dy * HV.y + z * HV.z), 48.0);
        float b;
        bool isIris = false;
        if (fabs(dy) > 1.0 - lid) {
            b = 0.05 + 0.3 * lam;
        } else {
            const float ca = dx * g.x + dy * g.y + z * g.z;
            if (ca > 0.955) {
                b = sp * 0.35;
            } else if (ca > 0.83) {
                const float ang = atan2(dy - g.y * ca, dx - g.x * ca);
                b = (0.32 + 0.48 * lam) * (0.72 + 0.28 * sin(ang * 14.0 + spin)) * ink_smoothstep(0.83, 0.85, ca) + sp * 0.3;
                isIris = true;
            } else {
                b = (0.08 + 0.72 * lam) * (0.55 + 0.45 * sqrt(z)) + sp * 0.3;
            }
        }
        if (b < 0.015) { continue; }
        const float r = C * 0.5 * min(1.05, sqrt(b));
        if (isIris) { rRed = max(rRed, r); } else { rPaper = max(rPaper, r); }
    }

    const float aPaper = ink_coverage(dist, rPaper, scale);
    const float aRed = ink_coverage(dist, rRed, scale);
    // Premultiplied: red over paper.
    const float3 rgb = h.paper.rgb * aPaper * (1.0 - aRed) + h.red.rgb * aRed;
    const float alpha = aPaper * (1.0 - aRed) + aRed;
    return float4(rgb, alpha);
}
