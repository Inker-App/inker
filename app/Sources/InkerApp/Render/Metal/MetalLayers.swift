import InkerKit
import MetalKit
import SwiftUI

/// SwiftUI wrapper for an eye scene drawn by EyeShader.metal.
struct EyeLayer: NSViewRepresentable {
    let makeScene: () -> any EyeScene
    var pointerBox: PointerBox?
    @EnvironmentObject private var clock: RenderClock

    final class Coordinator {
        // MTKView.delegate is weak; the coordinator owns the renderer.
        var renderer: EyeRenderer?
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> InkMTKView {
        let view = InkMTKView(frame: .zero, device: MTLCreateSystemDefaultDevice())
        view.pointerBox = pointerBox
        let renderer = EyeRenderer(view: view, scene: makeScene(), clock: clock)
        context.coordinator.renderer = renderer
        view.delegate = renderer
        view.apply(running: clock.isRunning, reduceMotion: clock.reduceMotion, fps: EyeRenderer.framesPerSecond)
        return view
    }

    func updateNSView(_ view: InkMTKView, context: Context) {
        view.apply(running: clock.isRunning, reduceMotion: clock.reduceMotion, fps: EyeRenderer.framesPerSecond)
    }
}

/// SwiftUI wrapper for the octopus.
struct OctopusLayer: NSViewRepresentable {
    var placement: OctopusRenderer.Placement = .onboarding
    @EnvironmentObject private var clock: RenderClock

    final class Coordinator {
        var renderer: OctopusRenderer?
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> InkMTKView {
        let view = InkMTKView(frame: .zero, device: MTLCreateSystemDefaultDevice())
        let renderer = OctopusRenderer(view: view, placement: placement, clock: clock)
        context.coordinator.renderer = renderer
        view.delegate = renderer
        view.apply(running: clock.isRunning, reduceMotion: clock.reduceMotion, fps: OctopusRig.framesPerSecond)
        return view
    }

    func updateNSView(_ view: InkMTKView, context: Context) {
        view.apply(running: clock.isRunning, reduceMotion: clock.reduceMotion, fps: OctopusRig.framesPerSecond)
    }
}
