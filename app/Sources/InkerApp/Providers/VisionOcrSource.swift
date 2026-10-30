import CoreGraphics
import Foundation
import ImageIO
import InkerCore
import InkerKit
import PDFKit
import Vision

/// OCR through Vision (stack-decision 3.3): `RecognizeDocumentsRequest` on
/// macOS 26, `VNRecognizeTextRequest` before that. A PDF is rendered one page
/// at a time at 200 dpi through PDFKit; an image (PNG, JPEG, HEIC) is decoded
/// by ImageIO with its EXIF orientation applied.
///
/// Called from a Rust background thread, never the main thread, so blocking
/// on Vision's async API here is safe.
struct VisionOcrSource {
    /// Longest side an image is decoded at. Phone photos are ~4000 px; more
    /// only costs memory.
    static let maxImagePixels = 4096

    func ocr(path: String) throws -> ExtractedFfi {
        let url = URL(fileURLWithPath: path)
        var pages: [[String]] = []
        // VERIFY: PDFDocument(url:) returns nil for a PNG, JPEG or HEIC, so
        // images fall through to ImageIO. The core has already sniffed the
        // file as a PDF or an image before calling.
        if let document = PDFDocument(url: url) {
            if document.isLocked {
                throw ProviderError.Failed(reason: "the PDF is password-protected")
            }
            for index in 0..<document.pageCount {
                let lines: [String] = try autoreleasepool {
                    guard let page = document.page(at: index), let image = Self.render(page) else {
                        return []
                    }
                    return try Self.recognize(image)
                }
                pages.append(lines)
            }
        } else {
            guard let image = Self.decodeImage(url) else {
                throw ProviderError.Failed(reason: "ImageIO could not decode the image")
            }
            pages.append(try Self.recognize(image))
        }
        return ProviderText.ocrResult(pageLines: pages)
    }

    // MARK: Images

    /// One PDF page at 200 dpi on white.
    static func render(_ page: PDFPage) -> CGImage? {
        let box = page.bounds(for: .mediaBox)
        let quarterTurned = (page.rotation % 180) != 0
        let widthPoints = Double(quarterTurned ? box.height : box.width)
        let heightPoints = Double(quarterTurned ? box.width : box.height)
        let size = ProviderText.renderSize(widthPoints: widthPoints, heightPoints: heightPoints)
        guard let context = CGContext(data: nil, width: size.width, height: size.height, bitsPerComponent: 8,
                                      bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                      bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else {
            return nil
        }
        context.setFillColor(CGColor(gray: 1, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: size.width, height: size.height))
        context.scaleBy(x: CGFloat(size.width) / CGFloat(widthPoints), y: CGFloat(size.height) / CGFloat(heightPoints))
        // VERIFY: draw(with:to:) applies the page's /Rotate and crop origin
        // (it uses transform(for:) internally); a rotated scan should come out
        // upright. If not, concat page.transform(for: .mediaBox) first.
        page.draw(with: .mediaBox, to: context)
        return context.makeImage()
    }

    /// PNG, JPEG or HEIC, upright, at most `maxImagePixels` on its long side.
    static func decodeImage(_ url: URL) -> CGImage? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxImagePixels,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }

    // MARK: Recognition

    static func recognize(_ image: CGImage) throws -> [String] {
        if #available(macOS 26, *) {
            return try recognizeDocument(image)
        }
        return try recognizeText(image)
    }

    @available(macOS 26, *)
    static func recognizeDocument(_ image: CGImage) throws -> [String] {
        try blocking {
            let request = RecognizeDocumentsRequest()
            // VERIFY: the macOS 26 Swift Vision API as shown at WWDC25:
            // perform(on: CGImage) -> [DocumentObservation], and
            // observation.document.text.lines, each with a `transcript`.
            // If `lines` is not there, split `document.text.transcript` on
            // newlines instead.
            let observations = try await request.perform(on: image)
            guard let document = observations.first?.document else { return [] }
            return document.text.lines.map(\.transcript)
        }
    }

    static func recognizeText(_ image: CGImage) throws -> [String] {
        ProviderText.readingOrder(try textBoxes(image))
    }

    /// The lines `recognize` returns, each with its box, for the attention
    /// card's page crop. Same recognizer per OS as `recognize`, so the line
    /// texts match what the core indexed.
    static func recognizeLines(_ image: CGImage) throws -> [PageSnippet.Line] {
        if #available(macOS 26, *) {
            return try recognizeDocumentLines(image)
        }
        return ProviderText.readingOrderRows(try textBoxes(image)).map { row in
            let union = row.dropFirst().reduce(CGRect(x: row[0].minX, y: row[0].minY, width: row[0].maxX - row[0].minX,
                                                      height: row[0].maxY - row[0].minY)) { acc, b in
                acc.union(CGRect(x: b.minX, y: b.minY, width: b.maxX - b.minX, height: b.maxY - b.minY))
            }
            return PageSnippet.Line(text: ProviderText.lineText(row), box: union)
        }
    }

    @available(macOS 26, *)
    static func recognizeDocumentLines(_ image: CGImage) throws -> [PageSnippet.Line] {
        try blocking {
            let request = RecognizeDocumentsRequest()
            let observations = try await request.perform(on: image)
            guard let document = observations.first?.document else { return [] }
            // VERIFY: a text line's region is `boundingRegion` (NormalizedRegion);
            // its `boundingBox` is a NormalizedRect with `cgRect`, origin bottom left.
            return document.text.lines.map {
                PageSnippet.Line(text: $0.transcript, box: $0.boundingRegion.boundingBox.cgRect)
            }
        }
    }

    static func textBoxes(_ image: CGImage) throws -> [ProviderText.TextBox] {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        // Language correction "fixes" account numbers and IDs into words.
        request.usesLanguageCorrection = false
        let handler = VNImageRequestHandler(cgImage: image, options: [:])
        do {
            try handler.perform([request])
        } catch {
            throw ProviderError.Failed(reason: "Vision text recognition failed: \((error as NSError).code)")
        }
        return (request.results ?? []).compactMap { observation in
            guard let text = observation.topCandidates(1).first?.string else { return nil }
            let b = observation.boundingBox
            return ProviderText.TextBox(text: text, minX: Double(b.minX), minY: Double(b.minY),
                                        maxX: Double(b.maxX), maxY: Double(b.maxY))
        }
    }

    /// Run async work to completion from a synchronous caller. Only safe on a
    /// thread outside Swift's cooperative pool, which a Rust thread is.
    private static func blocking<T>(_ work: @escaping @Sendable () async throws -> T) throws -> T {
        let done = DispatchSemaphore(value: 0)
        let box = ResultBox<T>()
        Task.detached {
            do {
                box.result = .success(try await work())
            } catch {
                box.result = .failure(error)
            }
            done.signal()
        }
        done.wait()
        guard let result = box.result else {
            throw ProviderError.Failed(reason: "Vision returned nothing")
        }
        do {
            return try result.get()
        } catch let error as ProviderError {
            throw error
        } catch {
            throw ProviderError.Failed(reason: "Vision document recognition failed: \((error as NSError).code)")
        }
    }
}

/// Carries a result out of a detached task. The semaphore orders the write
/// before the read.
private final class ResultBox<T>: @unchecked Sendable {
    var result: Result<T, Error>?
}
