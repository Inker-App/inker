import InkerKit
import MetalKit
import SwiftUI

/// An eye scene drawn by EyeShader.metal. Under `INKER --snapshots` it draws
/// nothing: ImageRenderer cannot capture a Metal view.
struct EyeLayer: View {
    let makeScene: () -> any EyeScene
    var pointerBox: PointerBox?
    @Environment(\.snapshotMode) private var snapshotMode

    var body: some View {
        if snapshotMode {
            Color.clear
        } else {
            EyeMetalView(makeScene: makeScene, pointerBox: pointerBox)
        }
    }
}

/// The octopus (OctopusShader.metal). Blank under `INKER --snapshots`.
struct OctopusLayer: View {
    var placement: OctopusRenderer.Placement = .onboarding
    @Environment(\.snapshotMode) private var snapshotMode

    var body: some View {
        if snapshotMode {
            Color.clear
        } else {
            OctopusMetalView(placement: placement)
        }
    }
}

/// SwiftUI wrapper for an eye scene drawn by EyeShader.metal.
struct EyeMetalView: NSViewRepresentable {
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
struct OctopusMetalView: NSViewRepresentable {
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
