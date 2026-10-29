import CoreText
import SwiftUI

/// Archivo (variable: wdth 62-125, wght 100-900) and JetBrains Mono (wght
/// 100-800), registered from the bundle by `FontRegistry`. Sizes, widths and
/// weights are the boards' CSS (`font-stretch` percent maps to `wdth`).
enum Typography {
    private static let wdth = 0x7764_7468 // 'wdth'
    private static let wght = 0x7767_6874 // 'wght'

    /// Archivo at a given CSS weight and `font-stretch` width.
    static func archivo(_ size: CGFloat, weight: CGFloat = 400, width: CGFloat = 100) -> Font {
        variable(family: "Archivo", size: size, axes: [wdth: width, wght: weight]) ?? .system(size: size, weight: swiftUIWeight(weight))
    }

    static func mono(_ size: CGFloat, weight: CGFloat = 400) -> Font {
        variable(family: "JetBrains Mono", size: size, axes: [wght: weight])
            ?? .system(size: size, weight: swiftUIWeight(weight), design: .monospaced)
    }

    /// Labels: width 125, weight 600, tracked 0.2em, uppercase (apply `.tracking(labelTracking)`).
    static let label = archivo(11, weight: 600, width: 125)
    static let labelTracking: CGFloat = 11 * 0.2
    static let body = archivo(13)
    static let small = archivo(12)
    static let caption = archivo(11)
    /// "Drop files or folders", dialog titles: 15 / 600 / 112%.
    static let title = archivo(15, weight: 600, width: 112)
    /// Onboarding headings on the boards: 22 / 400 / 112%, tracking 0.01em.
    static let heading = archivo(22, weight: 400, width: 112)
    /// The same heading sized for the popover's 328-point column.
    static let popoverHeading = archivo(18, weight: 400, width: 112)

    // VERIFY: CoreText variation axes keyed by the axis tag as an integer is
    // the documented form of kCTFontVariationAttribute; check that the width
    // axis visibly changes on a Mac (labels should look wide).
    private static func variable(family: String, size: CGFloat, axes: [Int: CGFloat]) -> Font? {
        var variation: [NSNumber: NSNumber] = [:]
        for (tag, value) in axes {
            variation[NSNumber(value: tag)] = NSNumber(value: Double(value))
        }
        let attributes: [CFString: Any] = [
            kCTFontFamilyNameAttribute: family,
            kCTFontVariationAttribute: variation,
        ]
        let descriptor = CTFontDescriptorCreateWithAttributes(attributes as CFDictionary)
        let font = CTFontCreateWithFontDescriptor(descriptor, size, nil)
        // If the family is missing, CoreText substitutes another font; fall back
        // to the system font instead so the miss is obvious and consistent.
        // The variable Archivo's legacy family name is "Archivo SemiBold" (its
        // default instance); the typographic family is "Archivo". Accept both.
        let resolved = CTFontCopyFamilyName(font) as String
        guard resolved.hasPrefix(family) else { return nil }
        return Font(font)
    }

    private static func swiftUIWeight(_ w: CGFloat) -> Font.Weight {
        switch w {
        case ..<350: return .light
        case ..<450: return .regular
        case ..<550: return .medium
        case ..<650: return .semibold
        default: return .bold
        }
    }
}
