import AppKit
import MetalKit

/// MTKView configured for dot renderers: transparent, top-left coordinates,
/// cursor tracked with an NSTrackingArea, clicks and drags pass through.
final class InkMTKView: MTKView {
    /// Cursor position in points, top-left origin; nil when outside.
    private(set) var pointer: CGPoint?
    /// Optional mirror so a SwiftUI layer underneath (the reaction-diffusion
    /// field) can react to the same cursor.
    var pointerBox: PointerBox?
    private var trackingArea: NSTrackingArea?

    override init(frame frameRect: CGRect, device: MTLDevice?) {
        super.init(frame: frameRect, device: device)
        configure()
    }

    required init(coder: NSCoder) {
        super.init(coder: coder)
        configure()
    }

    private func configure() {
        colorPixelFormat = .bgra8Unorm
        clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)
        framebufferOnly = true
        autoResizeDrawable = true
        wantsLayer = true
        layer?.isOpaque = false
        // VERIFY: the shaders output sRGB-encoded colours; tagging the layer
        // sRGB keeps them matching the SwiftUI paper/red.
        if let metal = layer as? CAMetalLayer {
            metal.colorspace = CGColorSpace(name: CGColorSpace.sRGB)
        }
    }

    override var isFlipped: Bool { true }

    /// Drawable pixels per point (2 on Retina).
    static func pixelsPerPoint(_ view: MTKView) -> Float {
        guard view.bounds.width > 0 else { return Float(view.window?.backingScaleFactor ?? 2) }
        return Float(view.drawableSize.width / view.bounds.width)
    }

    /// Let clicks and drops reach the SwiftUI views and the popover behind us.
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let trackingArea { removeTrackingArea(trackingArea) }
        let area = NSTrackingArea(rect: .zero,
                                  options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                                  owner: self, userInfo: nil)
        addTrackingArea(area)
        trackingArea = area
    }

    override func mouseMoved(with event: NSEvent) {
        setPointer(convert(event.locationInWindow, from: nil))
    }

    override func mouseEntered(with event: NSEvent) {
        setPointer(convert(event.locationInWindow, from: nil))
    }

    override func mouseExited(with event: NSEvent) {
        setPointer(nil)
    }

    private func setPointer(_ p: CGPoint?) {
        pointer = p
        pointerBox?.point = p
    }

    /// Run continuously, or draw one still frame and stop (Reduce Motion, or
    /// paused while hidden).
    func apply(running: Bool, reduceMotion: Bool, fps: Int) {
        preferredFramesPerSecond = fps
        if running {
            enableSetNeedsDisplay = false
            isPaused = false
        } else {
            isPaused = true
            enableSetNeedsDisplay = true
            if reduceMotion { needsDisplay = true }
        }
    }
}

/// A cursor position shared between an InkMTKView and a SwiftUI canvas.
final class PointerBox {
    var point: CGPoint?
}
