import Foundation
import Metal
import os

/// Loads the shader libraries. Tries a compiled `default.metallib` in the
/// resource bundle first (what Xcode produces when it processes .metal
/// resources), then compiles the bundled .metal source at runtime, which works
/// with plain `swift build`, where .metal files are only copied.
@MainActor
enum MetalLibraryLoader {
    private static let log = Logger(subsystem: "sh.inker.app", category: "metal")
    private static var cache: [String: MTLLibrary] = [:]

    static func library(device: MTLDevice, shader file: String, requiring function: String) -> MTLLibrary? {
        if let cached = cache[file] { return cached }
        // VERIFY: makeDefaultLibrary(bundle:) throws when the bundle has no
        // default.metallib; the runtime-compile path below covers that.
        if let bundle = ResourceBundle.bundle,
           let lib = try? device.makeDefaultLibrary(bundle: bundle),
           lib.functionNames.contains(function) {
            cache[file] = lib
            return lib
        }
        guard let url = ResourceBundle.url(forResource: file, withExtension: "metal", subdirectory: "Shaders"),
              let source = try? String(contentsOf: url, encoding: .utf8) else {
            log.error("shader source \(file, privacy: .public).metal not found in the resource bundle")
            return nil
        }
        do {
            let lib = try device.makeLibrary(source: source, options: nil)
            cache[file] = lib
            return lib
        } catch {
            log.error("compiling \(file, privacy: .public).metal failed: \(error.localizedDescription, privacy: .public)")
            return nil
        }
    }

    static func pipeline(device: MTLDevice, shader file: String, vertex: String, fragment: String,
                         pixelFormat: MTLPixelFormat) -> MTLRenderPipelineState? {
        guard let lib = library(device: device, shader: file, requiring: fragment),
              let v = lib.makeFunction(name: vertex),
              let f = lib.makeFunction(name: fragment) else { return nil }
        let desc = MTLRenderPipelineDescriptor()
        desc.vertexFunction = v
        desc.fragmentFunction = f
        desc.colorAttachments[0].pixelFormat = pixelFormat
        do {
            return try device.makeRenderPipelineState(descriptor: desc)
        } catch {
            log.error("pipeline \(fragment, privacy: .public) failed: \(error.localizedDescription, privacy: .public)")
            return nil
        }
    }
}
