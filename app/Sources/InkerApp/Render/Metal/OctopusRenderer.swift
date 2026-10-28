import InkerKit
import MetalKit
import simd

/// Must match `OctoHeader` in OctopusShader.metal.
struct OctoHeaderGPU {
    var grid: SIMD4<Float>
    var frame: SIMD4<Float>
    var look: SIMD4<Float>
    var m0: SIMD4<Float>
    var m1: SIMD4<Float>
    var m2: SIMD4<Float>
    var light: SIMD4<Float>
    var view: SIMD4<Float>
    var half: SIMD4<Float>
    var direction: SIMD4<Float>
    var paper: SIMD4<Float>
}

/// The octopus: tentacles built on the CPU per frame (`OctopusRig.build`), the
/// raymarch on the GPU into a cell texture, then dots. 12 fps on purpose.
@MainActor
final class OctopusRenderer: NSObject, MTKViewDelegate {
    struct Placement {
        var size: CGSize
        var cell: Double
        /// Model units to points (`S`).
        var scale: Double
        var center: CGPoint
        var look: OctopusLook

        /// AppOnboard2: 480x120, 4 px cells, S = 34, centre (240, 58).
        static let onboarding = Placement(size: CGSize(width: 480, height: 120), cell: 4, scale: 34,
                                          center: CGPoint(x: 240, y: 58),
                                          look: OctopusLook(center: CGPoint(x: 240, y: 58), yOffset: 30, scale: 160))
    }

    private var placement: Placement
    private let clock: RenderClock
    private let queue: MTLCommandQueue
    private let cellsPipeline: MTLRenderPipelineState
    private let dotsPipeline: MTLRenderPipelineState
    private var cellTexture: MTLTexture?
    private var lastTime: Double?

    init?(view: InkMTKView, placement: Placement, clock: RenderClock) {
        guard let device = view.device,
              let queue = device.makeCommandQueue(),
              let cells = MetalLibraryLoader.pipeline(device: device, shader: "OctopusShader",
                                                      vertex: "octo_fullscreen_vertex", fragment: "octo_cells_fragment",
                                                      pixelFormat: .r32Float),
              let dots = MetalLibraryLoader.pipeline(device: device, shader: "OctopusShader",
                                                     vertex: "octo_fullscreen_vertex", fragment: "octo_dots_fragment",
                                                     pixelFormat: view.colorPixelFormat) else { return nil }
        self.placement = placement
        self.clock = clock
        self.queue = queue
        cellsPipeline = cells
        dotsPipeline = dots
        super.init()
    }

    nonisolated func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}

    nonisolated func draw(in view: MTKView) {
        MainActor.assumeIsolated {
            render(in: view)
        }
    }

    private func gridSize() -> (Int, Int) {
        (Int(placement.size.width / placement.cell), Int(placement.size.height / placement.cell))
    }

    private func texture(device: MTLDevice, width: Int, height: Int) -> MTLTexture? {
        if let t = cellTexture, t.width == width, t.height == height { return t }
        let desc = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .r32Float, width: width, height: height, mipmapped: false)
        desc.usage = [.renderTarget, .shaderRead]
        desc.storageMode = .private
        cellTexture = device.makeTexture(descriptor: desc)
        return cellTexture
    }

    private func render(in view: MTKView) {
        let t = clock.now
        let dt = lastTime.map { max(0, t - $0) } ?? 0
        lastTime = t
        placement.look.update(pointer: (view as? InkMTKView)?.pointer, dt: dt)

        let (gw, gh) = gridSize()
        guard gw > 0, gh > 0,
              let device = view.device,
              let cells = texture(device: device, width: gw, height: gh),
              let pass = view.currentRenderPassDescriptor,
              let drawable = view.currentDrawable,
              let buffer = queue.makeCommandBuffer() else { return }

        let m = OctopusRig.rotation(time: t)
        let v = OctopusRig.frameVectors(m)
        func f4(_ x: Vec3) -> SIMD4<Float> { SIMD4<Float>(Float(x.x), Float(x.y), Float(x.z), 0) }
        var header = OctoHeaderGPU(
            grid: SIMD4<Float>(Float(gw), Float(gh), Float(placement.cell), InkMTKView.pixelsPerPoint(view)),
            frame: SIMD4<Float>(Float(placement.center.x), Float(placement.center.y), Float(placement.scale), 0),
            look: SIMD4<Float>(Float(placement.look.x), Float(placement.look.y), 0, 0),
            m0: SIMD4<Float>(Float(m[0]), Float(m[1]), Float(m[2]), 0),
            m1: SIMD4<Float>(Float(m[3]), Float(m[4]), Float(m[5]), 0),
            m2: SIMD4<Float>(Float(m[6]), Float(m[7]), Float(m[8]), 0),
            light: f4(v.light), view: f4(v.view), half: f4(v.half), direction: f4(v.direction),
            paper: Palette.simd(Palette.paperHex))

        var tentacles: [SIMD4<Float>] = []
        let rig = OctopusRig.build(time: t)
        for tent in rig {
            for (p, r) in zip(tent.points, tent.radii) {
                tentacles.append(SIMD4<Float>(Float(p.x), Float(p.y), Float(p.z), Float(r)))
            }
        }
        for tent in rig {
            tentacles.append(SIMD4<Float>(Float(tent.boundsCenter.x), Float(tent.boundsCenter.y),
                                          Float(tent.boundsCenter.z), Float(tent.boundsRadius)))
        }

        // Pass 1: one fragment per cell.
        let cellPass = MTLRenderPassDescriptor()
        cellPass.colorAttachments[0].texture = cells
        cellPass.colorAttachments[0].loadAction = .dontCare
        cellPass.colorAttachments[0].storeAction = .store
        if let enc = buffer.makeRenderCommandEncoder(descriptor: cellPass) {
            enc.setRenderPipelineState(cellsPipeline)
            enc.setFragmentBytes(&header, length: MemoryLayout<OctoHeaderGPU>.stride, index: 0)
            tentacles.withUnsafeBytes { raw in
                if let base = raw.baseAddress { enc.setFragmentBytes(base, length: raw.count, index: 1) }
            }
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
            enc.endEncoding()
        }

        // Pass 2: dots.
        if let enc = buffer.makeRenderCommandEncoder(descriptor: pass) {
            enc.setRenderPipelineState(dotsPipeline)
            enc.setFragmentBytes(&header, length: MemoryLayout<OctoHeaderGPU>.stride, index: 0)
            enc.setFragmentTexture(cells, index: 0)
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
            enc.endEncoding()
        }
        buffer.present(drawable)
        buffer.commit()
    }
}
