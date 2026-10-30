import AppKit
import CoreGraphics
import InkerKit
import PDFKit

/// The attention card's picture of the page around a held value. Reads the
/// user's own file again, finds the span the core recorded, and draws the
/// page region around it with the value outlined. Nothing crosses the core
/// boundary but a byte range; the file is the user's to look at.
enum PageSnippet {
    struct Line: Sendable {
        let text: String
        /// Normalised, origin bottom left (Vision's convention).
        let box: CGRect
    }

    struct Rendered: @unchecked Sendable {
        let image: CGImage
        /// The value's rectangle in box points.
        let outline: CGRect
    }

    /// Backing pixels per point.
    static let scale: CGFloat = 2

    /// Nil when the page cannot be read or the span cannot be placed; the
    /// card then keeps the octopus.
    static func render(path: String, pageIndex: Int, span: Range<Int>) -> Rendered? {
        let url = URL(fileURLWithPath: path)
        if let document = PDFDocument(url: url) {
            guard !document.isLocked, let page = document.page(at: pageIndex) else { return nil }
            if let text = page.string, !ProviderText.isBlank(text) {
                return renderPDF(page: page, text: text, span: span)
            }
            // A scanned PDF: the core read it through OCR at 200 dpi.
            guard let image = VisionOcrSource.render(page) else { return nil }
            return renderImage(image, span: span)
        }
        guard pageIndex == 0, let image = VisionOcrSource.decodeImage(url) else { return nil }
        return renderImage(image, span: span)
    }

    // MARK: A PDF with a text layer

    private static func renderPDF(page: PDFPage, text: String, span: Range<Int>) -> Rendered? {
        // VERIFY: on a rotated page, selection bounds and draw(with:to:) may
        // disagree on axes. Until that is checked by eye, such pages keep
        // the octopus.
        guard page.rotation == 0, let range = nsRange(of: span, in: text),
              let selection = page.selection(for: range) else { return nil }
        let bounds = selection.bounds(for: page)
        guard bounds.width > 0, bounds.height > 0 else { return nil }
        let box = page.bounds(for: .mediaBox)
        // Page space has its origin at the bottom left; the crop maths use the top left.
        let value = CGRect(x: bounds.minX - box.minX, y: box.maxY - bounds.maxY,
                           width: bounds.width, height: bounds.height)
        let frame = PageCrop.frame(span: value, pageSize: box.size)
        return draw(frame: frame, pageHeight: box.height) { context in
            page.draw(with: .mediaBox, to: context)
        }
    }

    /// The UTF-16 range PDFKit wants, from the core's byte range into the
    /// same text. Nil when the range does not sit on character boundaries.
    static func nsRange(of span: Range<Int>, in text: String) -> NSRange? {
        let utf8 = text.utf8
        guard span.lowerBound >= 0, span.lowerBound < span.upperBound, span.upperBound <= utf8.count,
              let start = utf8.index(utf8.startIndex, offsetBy: span.lowerBound, limitedBy: utf8.endIndex),
              let end = utf8.index(utf8.startIndex, offsetBy: span.upperBound, limitedBy: utf8.endIndex),
              let lower = String.Index(start, within: text),
              let upper = String.Index(end, within: text)
        else { return nil }
        return NSRange(lower..<upper, in: text)
    }

    // MARK: A scan

    private static func renderImage(_ image: CGImage, span: Range<Int>) -> Rendered? {
        guard let lines = try? VisionOcrSource.recognizeLines(image),
              let hit = PageCrop.locate(byteRange: span, inLines: lines.map(\.text)) else { return nil }
        let size = CGSize(width: image.width, height: image.height)
        let line = lines[hit.line].box
        // Vision's boxes are normalised with the origin at the bottom left.
        let lineRect = CGRect(x: line.minX * size.width, y: (1 - line.maxY) * size.height,
                              width: line.width * size.width, height: line.height * size.height)
        let frame = PageCrop.frame(span: PageCrop.estimate(in: lineRect, hit: hit), pageSize: size)
        return draw(frame: frame, pageHeight: size.height) { context in
            context.draw(image, in: CGRect(origin: .zero, size: size))
        }
    }

    // MARK: Drawing

    /// A box-sized bitmap showing `frame.crop` of a page `pageHeight` tall,
    /// drawn by `body` in a bottom-left coordinate system.
    private static func draw(frame: PageCrop.Frame, pageHeight: CGFloat, _ body: (CGContext) -> Void) -> Rendered? {
        let box = PageCrop.box
        let width = Int(box.width * scale)
        let height = Int(box.height * scale)
        guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                      space: CGColorSpaceCreateDeviceRGB(),
                                      bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return nil }
        context.setFillColor(CGColor(gray: 1, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        context.interpolationQuality = .high
        context.scaleBy(x: scale * frame.scale, y: scale * frame.scale)
        // The crop's bottom-left corner, in the page's bottom-left space.
        context.translateBy(x: -frame.crop.minX, y: -(pageHeight - frame.crop.maxY))
        body(context)
        guard let image = context.makeImage() else { return nil }
        return Rendered(image: image, outline: frame.outline)
    }
}
