// OctopusShader: the hero, raymarched and drawn as dots at 12 fps.
// A port of `makeOcto` / `sdf` / `render` in AppOnboard2 and WorldOctopus.
//
// Two passes so the raymarch runs once per grid cell, not once per pixel:
//   1. octo_cells_fragment renders into a GW x GH r32Float texture, one
//      fragment per cell, writing the cell brightness b.
//   2. octo_dots_fragment draws each cell as a paper dot of radius
//      cell/2 * min(1.05, sqrt(b)).
// The vertex shader is ink_fullscreen_vertex from EyeShader.metal; this file
// declares its own copy so it compiles as a standalone library.

#include <metal_stdlib>
using namespace metal;

struct OctoVertexOut {
    float4 position [[position]];
};

vertex OctoVertexOut octo_fullscreen_vertex(uint vid [[vertex_id]]) {
    float2 uv = float2(float((vid << 1) & 2), float(vid & 2));
    OctoVertexOut out;
    out.position = float4(uv * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

// Must match OctoHeaderGPU in OctopusRenderer.swift.
struct OctoHeader {
    float4 grid;   // x: GW, y: GH, z: cell (pt), w: pixels per point
    float4 frame;  // x: cx0, y: cy0, z: S (pt per model unit), w: unused
    float4 look;   // xy: eye look
    float4 m0;     // rotation M, row 0 (xyz)
    float4 m1;     // row 1
    float4 m2;     // row 2
    float4 L;      // light, model space
    float4 V;      // view
    float4 H;      // half vector
    float4 D;      // ray direction
    float4 paper;
};

// Tentacles: 8 x 9 points as float4(x, y, z, radius) at [0, 72), then
// 8 bounding spheres float4(cx, cy, cz, r) at [72, 80). Built on the CPU by
// OctopusRig.build(T) each frame.
constant int TN = 8;
constant int SEG = 8;
constant int BOUNDS = 72;

constant float3 EYE0 = float3(-0.25, -0.40, -0.5);
constant float3 EYE1 = float3(0.25, -0.40, -0.5);

static float3 octo_mul(constant OctoHeader &h, float3 v) {
    return float3(dot(h.m0.xyz, v), dot(h.m1.xyz, v), dot(h.m2.xyz, v));
}

// `sdf(x, y, z, out)`: material 1 = body, 2 = eye; eyeIndex 0 or 1.
static float octo_sdf(float3 p, constant float4 *T, thread int &material, thread int &eyeIndex) {
    const float hx = p.x / 0.62;
    const float hy = (p.y + 0.55) / 0.74;
    const float hz = p.z / 0.6;
    const float k0 = sqrt(hx * hx + hy * hy + hz * hz);
    const float k1 = max(1e-6f, sqrt(hx * hx / 0.3844 + hy * hy / 0.5476 + hz * hz / 0.36));
    float d = k0 * (k0 - 1.0) / k1;
    for (int i = 0; i < TN; i++) {
        const float4 bnd = T[BOUNDS + i];
        if (length(p - bnd.xyz) - bnd.w > d + 0.15) { continue; }
        float dm = 1e9f;
        for (int k = 0; k < SEG; k++) {
            const float4 A = T[i * (SEG + 1) + k];
            const float4 B = T[i * (SEG + 1) + k + 1];
            const float3 v = B.xyz - A.xyz;
            const float3 q = p - A.xyz;
            const float hcap = clamp(dot(q, v) / dot(v, v), 0.0, 1.0);
            const float3 e = q - v * hcap;
            const float dc = length(e) - (A.w + (B.w - A.w) * hcap);
            dm = min(dm, dc);
        }
        const float hh = max(0.14 - fabs(d - dm), 0.0) / 0.14;
        d = min(d, dm) - hh * hh * 0.035;
    }
    const float q0 = length(p - EYE0) - 0.12;
    const float q1 = length(p - EYE1) - 0.12;
    const float de = min(q0, q1);
    eyeIndex = q1 < q0 ? 1 : 0;
    material = de < d ? 2 : 1;
    return min(d, de);
}

fragment float4 octo_cells_fragment(OctoVertexOut in [[stage_in]],
                                    constant OctoHeader &h [[buffer(0)]],
                                    constant float4 *T [[buffer(1)]]) {
    const float C = h.grid.z;
    const float S = h.frame.z;
    const float i = floor(in.position.x);
    const float j = floor(in.position.y);
    const float X = ((i + 0.5) * C - h.frame.x) / S;
    const float Y = ((j + 0.5) * C - h.frame.y) / S;
    const float3 o = octo_mul(h, float3(X, Y, -4.0));
    const float3 D = h.D.xyz;
    float b = 0.0;
    // Bounding sphere radius 2 around (0, -0.1, 0).
    const float3 oc = float3(o.x, o.y + 0.1, o.z);
    const float bb = dot(oc, D);
    const float cc = dot(oc, oc) - 4.0;
    const float disc = bb * bb - cc;
    if (disc > 0.0) {
        const float sq = sqrt(disc);
        const float t1 = -bb + sq;
        float tt = max(0.0, -bb - sq);
        bool hit = false;
        float3 p = o;
        int material = 1;
        int eyeIndex = 0;
        for (int s = 0; s < 56 && tt < t1; s++) {
            p = o + D * tt;
            const float dd = octo_sdf(p, T, material, eyeIndex);
            if (dd < 0.004) { hit = true; break; }
            tt += dd * 0.9;
        }
        if (hit) {
            octo_sdf(p, T, material, eyeIndex);
            const float e = 0.005;
            int m2 = 0;
            int e2 = 0;
            const float a = octo_sdf(p + float3(e, -e, -e), T, m2, e2);
            const float b2 = octo_sdf(p + float3(-e, -e, e), T, m2, e2);
            const float c = octo_sdf(p + float3(-e, e, -e), T, m2, e2);
            const float d3 = octo_sdf(p + float3(e, e, e), T, m2, e2);
            float3 nrm = float3(a - b2 - c + d3, -a - b2 + c + d3, -a + b2 - c + d3);
            const float nl = length(nrm);
            nrm = nl > 0.0 ? nrm / nl : float3(0.0, 0.0, -1.0);
            const float lam = max(0.0, dot(nrm, h.L.xyz));
            const float nv = max(0.0, dot(nrm, h.V.xyz));
            const float rim = pow(1.0 - nv, 3.0);
            const float sp = pow(max(0.0, dot(nrm, h.H.xyz)), 36.0);
            if (material == 2) {
                const float3 E = eyeIndex == 1 ? EYE1 : EYE0;
                const float qx = p.x - E.x - h.look.x * 0.04;
                const float qy = p.y - E.y - h.look.y * 0.03;
                const float qz = p.z - E.z;
                b = 0.25 + 0.75 * lam;
                if (qz < -0.06 && fabs(qy) < 0.028 && fabs(qx) < 0.075) { b = 0.02; } // slit pupil
                b += sp * 0.35;
            } else {
                const float tex = 0.88 + 0.12 * sin(p.x * 23.0 + sin(p.z * 17.0)) * sin(p.y * 19.0);
                b = (0.05 + 0.75 * lam) * tex + 0.14 * rim + sp * 0.08;
            }
        }
    }
    return float4(b, 0.0, 0.0, 1.0);
}

fragment float4 octo_dots_fragment(OctoVertexOut in [[stage_in]],
                                   constant OctoHeader &h [[buffer(0)]],
                                   texture2d<float, access::read> cells [[texture(0)]]) {
    const float scale = h.grid.w;
    const float C = h.grid.z;
    const float2 p = in.position.xy / scale;
    const float2 ij = floor(p / C);
    if (ij.x < 0.0 || ij.y < 0.0 || ij.x > h.grid.x - 1.0 || ij.y > h.grid.y - 1.0) {
        return float4(0.0);
    }
    const float b = cells.read(uint2(ij)).r;
    if (b <= 0.015) { return float4(0.0); }
    const float r = C * 0.5 * min(1.05, sqrt(b));
    const float2 c = (ij + 0.5) * C;
    const float a = clamp((r - length(p - c)) * scale + 0.5, 0.0, 1.0);
    return float4(h.paper.rgb * a, a);
}
