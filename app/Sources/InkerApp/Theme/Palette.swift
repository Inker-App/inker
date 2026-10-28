import AppKit
import SwiftUI

/// The colours from the brief. All dark (decision 5.1); red is only ever the
/// iris (decision 5.2).
enum Palette {
    static let backgroundHex: UInt32 = 0x0D0F10
    static let panelHex: UInt32 = 0x08090A
    static let paperHex: UInt32 = 0xE6E4DE
    static let mutedHex: UInt32 = 0x8A8C8E
    static let rimHex: UInt32 = 0x3A3D40
    static let redHex: UInt32 = 0xE0473E
    /// Disabled button text on the Processing board.
    static let disabledHex: UInt32 = 0x5C5F61
    /// Checkbox label text on the dialog and onboarding boards.
    static let checkLabelHex: UInt32 = 0xC9C7C1

    static let background = color(backgroundHex)
    static let panel = color(panelHex)
    static let paper = color(paperHex)
    static let muted = color(mutedHex)
    static let rim = color(rimHex)
    static let red = color(redHex)
    static let disabled = color(disabledHex)
    static let checkLabel = color(checkLabelHex)

    /// `rgba(230,228,222,0.14)`.
    static let hairline = paper.opacity(0.14)
    /// Row separators.
    static let hairlineFaint = paper.opacity(0.08)
    /// Outline buttons and the drop strip.
    static let outline = paper.opacity(0.3)
    /// Inputs and onboarding outline buttons.
    static let field = paper.opacity(0.2)
    /// The attention card border.
    static let cardBorder = paper.opacity(0.55)
    /// The empty zone's dashed inset border.
    static let dashed = paper.opacity(0.35)

    static var nsBackground: NSColor { nsColor(backgroundHex) }

    static func color(_ hex: UInt32, opacity: Double = 1) -> Color {
        Color(.sRGB,
              red: Double((hex >> 16) & 0xFF) / 255,
              green: Double((hex >> 8) & 0xFF) / 255,
              blue: Double(hex & 0xFF) / 255,
              opacity: opacity)
    }

    static func nsColor(_ hex: UInt32, alpha: CGFloat = 1) -> NSColor {
        NSColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
                green: CGFloat((hex >> 8) & 0xFF) / 255,
                blue: CGFloat(hex & 0xFF) / 255,
                alpha: alpha)
    }

    /// sRGB components for the Metal shaders (straight alpha).
    static func simd(_ hex: UInt32) -> SIMD4<Float> {
        SIMD4<Float>(Float((hex >> 16) & 0xFF) / 255, Float((hex >> 8) & 0xFF) / 255, Float(hex & 0xFF) / 255, 1)
    }
}
