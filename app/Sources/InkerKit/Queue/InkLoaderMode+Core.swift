import Foundation
import InkerCore

public extension InkLoaderMode {
    /// nil for `.idle` (no loader).
    init?(_ loader: Loader) {
        switch loader {
        case .read: self = .read
        case .ocr: self = .ocr
        case .queued: self = .queued
        case .idle: return nil
        }
    }
}
