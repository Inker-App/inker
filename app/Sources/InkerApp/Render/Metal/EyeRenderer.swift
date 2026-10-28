import InkerKit
import MetalKit
import simd

/// Must match `EyeHeader` in EyeShader.metal.
struct EyeHeaderGPU {
    var viewport: SIMD4<Float>
    var info: SIMD4<Float>
    var paper: SIMD4<Float>
    var red: SIMD4<Float>
}

/// Must match `EyeData` in EyeShader.metal.
struct EyeGPU {
    var a: SIMD4<Float>
    var b: SIMD4<Float>

    init(_ f: EyeFrame) {
        a = SIMD4<Float>(Float(f.centerX), Float(f.centerY), Float(f.radius), Float(f.lid))
        b = SIMD4<Float>(Float(f.gaze.x), Float(f.gaze.y), Float(f.gaze.z), Float(f.spin))
    }
}

/// Drives one eye scene: steps the behaviour on the CPU (InkerKit) and hands
/// the frames to EyeShader.metal.
@MainActor
final class EyeRenderer: NSObject, MTKViewDelegate {
    static let framesPerSecond = 30
    /// setFragmentBytes is limited to 4 KB; 32 bytes per eye.
    static let maxEyes = 64

    private var scene: any EyeScene
    private let clock: RenderClock
    private let queue: MTLCommandQueue
    private let pipeline: MTLRenderPipelineState
    private var lastTime: Double?

    init?(view: InkMTKView, scene: any EyeScene, clock: RenderClock) {
        guard let device = view.device,
              let queue = device.makeCommandQueue(),
              let pipeline = MetalLibraryLoader.pipeline(device: device, shader: "EyeShader",
                                                         vertex: "ink_fullscreen_vertex", fragment: "eye_fragment",
                                                         pixelFormat: view.colorPixelFormat) else { return nil }
        self.scene = scene
        self.clock = clock
        self.queue = queue
        self.pipeline = pipeline
        super.init()
    }

    nonisolated func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}

    // VERIFY: MTKView calls draw(in:) on the main thread by default; the
    // assumeIsolated hop documents that (and traps if it ever is not).
    nonisolated func draw(in view: MTKView) {
        MainActor.assumeIsolated {
            render(in: view)
        }
    }

    private func render(in view: MTKView) {
        let t = clock.now
        let dt = lastTime.map { min(0.033, max(0, t - $0)) } ?? 0
        lastTime = t
        let pointer = (view as? InkMTKView)?.pointer
        let frames = scene.frames(time: t, dt: dt, pointer: pointer)

        guard let pass = view.currentRenderPassDescriptor,
              let drawable = view.currentDrawable,
              let buffer = queue.makeCommandBuffer(),
              let encoder = buffer.makeRenderCommandEncoder(descriptor: pass) else { return }

        let scale = InkMTKView.pixelsPerPoint(view)
        var header = EyeHeaderGPU(
            viewport: SIMD4<Float>(Float(scene.size.width), Float(scene.size.height), scale, Float(scene.cell)),
            info: SIMD4<Float>(Float(min(frames.count, EyeRenderer.maxEyes)), 0, 0, 0),
            paper: Palette.simd(Palette.paperHex),
            red: Palette.simd(Palette.redHex))
        var eyes = frames.prefix(EyeRenderer.maxEyes).map { EyeGPU($0) }
        if eyes.isEmpty {
            // Never bind a zero-length buffer.
            eyes = [EyeGPU(EyeFrame(centerX: -1000, centerY: -1000, radius: 1, gaze: Vec3(0, 0, 1), lid: 0, spin: 0))]
        }

        encoder.setRenderPipelineState(pipeline)
        encoder.setFragmentBytes(&header, length: MemoryLayout<EyeHeaderGPU>.stride, index: 0)
        eyes.withUnsafeBytes { raw in
            if let base = raw.baseAddress {
                encoder.setFragmentBytes(base, length: raw.count, index: 1)
            }
        }
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        encoder.endEncoding()
        buffer.present(drawable)
        buffer.commit()
    }
}
