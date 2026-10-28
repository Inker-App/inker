import Foundation
import InkerCore

/// The Swift side of the UniFFI `HostProviders` callback interface: what the
/// Rust core needs from macOS (stack-decision 3.3). Rust calls these from its
/// ingest worker thread, never the main thread, and they may block.
///
/// Anything thrown becomes `Error::Extract` (or `Error::Vault` for the key)
/// in the core. Reasons must be safe to log: no text, values or file names.
final class MacProviders: HostProviders, @unchecked Sendable {
    private let keychain = KeychainKeyStore()
    private let pdfKit = PDFKitTextSource()
    private let vision = VisionOcrSource()

    func loadOrCreateKey() throws -> Data {
        try keychain.loadOrCreate()
    }

    func pdfText(path: String) throws -> ExtractedFfi? {
        try pdfKit.extract(path: path)
    }

    func ocr(path: String) throws -> ExtractedFfi {
        try vision.ocr(path: path)
    }
}
