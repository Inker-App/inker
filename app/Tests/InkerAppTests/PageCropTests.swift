import Foundation
import Testing
@testable import InkerKit

/// The attention card's page crop: zoomed on the held value, outlined,
/// kept inside the page; and finding a byte span on an OCR'd page.
struct PageCropTests {
    private func near(_ a: CGFloat, _ b: CGFloat) -> Bool { abs(a - b) < 0.01 }

    @Test func theCropCentresOnTheSpanAndScalesItToSixteenPoints() {
        let f = PageCrop.frame(span: CGRect(x: 200, y: 300, width: 100, height: 10), pageSize: CGSize(width: 612, height: 792))
        #expect(near(f.scale, 1.6))
        #expect(near(f.crop.width, 302 / 1.6) && near(f.crop.height, 84 / 1.6))
        #expect(near(f.crop.midX, 250) && near(f.crop.midY, 305))
        #expect(near(f.outline.width, 160) && near(f.outline.height, 16))
        #expect(near(f.outline.midX, 151) && near(f.outline.midY, 42), "the value sits in the middle of the box")
    }

    @Test func theScaleIsClamped() {
        let tiny = PageCrop.frame(span: CGRect(x: 0, y: 0, width: 10, height: 2), pageSize: CGSize(width: 612, height: 792))
        #expect(tiny.scale == 3)
        let huge = PageCrop.frame(span: CGRect(x: 0, y: 0, width: 300, height: 40), pageSize: CGSize(width: 612, height: 792))
        #expect(huge.scale == 1.25)
    }

    @Test func theCropStaysInsideThePage() {
        let page = CGSize(width: 612, height: 792)
        let corner = PageCrop.frame(span: CGRect(x: 2, y: 3, width: 50, height: 10), pageSize: page)
        #expect(corner.crop.origin == .zero)
        #expect(near(corner.outline.minX, 2 * corner.scale) && near(corner.outline.minY, 3 * corner.scale))
        let far = PageCrop.frame(span: CGRect(x: 600, y: 780, width: 10, height: 10), pageSize: page)
        #expect(near(far.crop.maxX, 612) && near(far.crop.maxY, 792))
    }

    @Test func aByteRangeLandsOnItsLineWithFractions() {
        let lines = ["Wire confirmation  ", "Account 123456789", "", "Thanks"]
        // "Wire confirmation" is 17 bytes, then the newline: line 2 starts at 18.
        let hit = PageCrop.locate(byteRange: 26..<35, inLines: lines)
        #expect(hit == PageCrop.LineHit(line: 1, start: 8.0 / 17, end: 1))
        #expect(PageCrop.locate(byteRange: 0..<4, inLines: lines) == PageCrop.LineHit(line: 0, start: 0, end: 4.0 / 17))
        #expect(PageCrop.locate(byteRange: 37..<43, inLines: lines)?.line == 3, "past the empty line")
        #expect(PageCrop.locate(byteRange: 36..<37, inLines: lines) == PageCrop.LineHit(line: 2, start: 0, end: 1), "the empty line itself")
        #expect(PageCrop.locate(byteRange: 90..<95, inLines: lines) == nil)
    }

    @Test func anOutlineIsEstimatedAlongTheLine() {
        let hit = PageCrop.LineHit(line: 0, start: 0.5, end: 1)
        let rect = PageCrop.estimate(in: CGRect(x: 10, y: 20, width: 100, height: 12), hit: hit)
        #expect(rect == CGRect(x: 60, y: 20, width: 50, height: 12))
    }
}
