import Foundation
import Testing
@testable import InkerKit

/// `startLoaders` in AppPopProcessing: 284x8 loaders, 8 px cells.
struct InkLoaderTests {
    @Test func cellCountForTheBoardWidth() {
        #expect(InkLoader.cellCount(width: 284) == 35)
    }

    @Test func queuedIsFlatAndFaint() {
        for j in 0..<35 {
            #expect(InkLoader.value(.queued, cell: j, count: 35, time: 3.3) == 0.1)
        }
    }

    @Test func readIsATravellingPulse() {
        // v = 0.18 + 0.45 * (0.5 + 0.5 sin(6t - 0.45 j))^3
        let t = 0.7
        for j in [0, 5, 17] {
            let s = 0.5 + 0.5 * sin(t * 6 - Double(j) * 0.45)
            #expect(close(InkLoader.value(.read, cell: j, count: 35, time: t), 0.18 + 0.45 * s * s * s))
        }
        let values = (0..<35).map { InkLoader.value(.read, cell: $0, count: 35, time: t) }
        #expect(values.min()! >= 0.18 && values.max()! <= 0.63 + 1e-12)
    }

    @Test func ocrIsAScanningHead() {
        // head = (14t) mod (w + 10) - 5; v = 0.16 + 0.5 exp(-((j - head)/3)^2)
        let t = 1.0
        let head = (14.0).truncatingRemainder(dividingBy: 45) - 5 // 9
        #expect(close(InkLoader.value(.ocr, cell: 9, count: 35, time: t), 0.66))
        #expect(close(InkLoader.value(.ocr, cell: 12, count: 35, time: t), 0.16 + 0.5 * exp(-1)))
        #expect(head == 9)
        #expect(InkLoader.value(.ocr, cell: 34, count: 35, time: t) < 0.17)
    }
}
