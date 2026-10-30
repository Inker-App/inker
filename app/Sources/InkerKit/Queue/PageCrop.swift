import Foundation

/// The attention card's page crop: a window onto the page around the held
/// value, zoomed so the value is readable, with the value outlined. All
/// rectangles here use a top-left origin; the renderer converts from PDF
/// and Vision space.
public enum PageCrop {
    /// The card's box, the size PeekView draws.
    public static let box = CGSize(width: 302, height: 84)
    /// The value's text is drawn about this tall.
    public static let targetSpanHeight: CGFloat = 16
    public static let minScale: CGFloat = 1.25
    public static let maxScale: CGFloat = 3

    public struct Frame: Equatable, Sendable {
        /// The page region shown, in page units.
        public var crop: CGRect
        /// Page units to box points.
        public var scale: CGFloat
        /// The value's rectangle in box points.
        public var outline: CGRect
    }

    /// The region around `span` (page units) that fills `box`, centred on
    /// the value and kept inside the page where the page allows.
    public static func frame(span: CGRect, pageSize: CGSize, box: CGSize = box) -> Frame {
        let scale = min(maxScale, max(minScale, targetSpanHeight / max(span.height, 1)))
        let size = CGSize(width: box.width / scale, height: box.height / scale)
        var origin = CGPoint(x: span.midX - size.width / 2, y: span.midY - size.height / 2)
        origin.x = max(0, min(origin.x, pageSize.width - size.width))
        origin.y = max(0, min(origin.y, pageSize.height - size.height))
        let crop = CGRect(origin: origin, size: size)
        let outline = CGRect(x: (span.minX - crop.minX) * scale, y: (span.minY - crop.minY) * scale,
                             width: span.width * scale, height: span.height * scale)
        return Frame(crop: crop, scale: scale, outline: outline)
    }

    /// Which line a byte range of the page text starts on, and how far
    /// along that line it starts and ends, as fractions of the line's bytes.
    public struct LineHit: Equatable, Sendable {
        public var line: Int
        public var start: Double
        public var end: Double

        public init(line: Int, start: Double, end: Double) {
            self.line = line
            self.start = start
            self.end = end
        }
    }

    /// Where a byte range of a page's text falls, the text being `lines`
    /// joined with newlines as `ProviderText.joinLines` builds it.
    public static func locate(byteRange: Range<Int>, inLines lines: [String]) -> LineHit? {
        var offset = 0
        for (index, raw) in lines.enumerated() {
            let count = ProviderText.trimTrailing(raw).utf8.count
            let lineEnd = offset + count
            if byteRange.lowerBound >= offset, byteRange.lowerBound < max(lineEnd, offset + 1) {
                guard count > 0 else { return LineHit(line: index, start: 0, end: 1) }
                let start = Double(byteRange.lowerBound - offset) / Double(count)
                let end = Double(min(byteRange.upperBound, lineEnd) - offset) / Double(count)
                return LineHit(line: index, start: start, end: max(end, start))
            }
            offset = lineEnd + 1
        }
        return nil
    }

    /// Part of a line's box, by proportion along it: good enough for an
    /// outline when only the line's box is known.
    public static func estimate(in line: CGRect, hit: LineHit) -> CGRect {
        CGRect(x: line.minX + line.width * hit.start, y: line.minY,
               width: max(line.width * (hit.end - hit.start), 1), height: line.height)
    }
}
