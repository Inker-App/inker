import Foundation
import Testing
@testable import InkerKit

/// CPU reference of `eyeDraw`'s per-cell shading. EyeShader.metal must match it.
struct EyeShadingTests {
    private let ahead = Vec3(0, 0, 1)

    @Test func lightAndHalfVectorMatchTheBoards() {
        let l = EyeShading.light
        #expect(close(l.x, -0.45 / sqrt(0.45 * 0.45 + 0.55 * 0.55 + 0.49)))
        let h = EyeShading.halfVector
        #expect(close(h.length, 1))
    }

    @Test func outsideTheDiskIsNothing() {
        // Optional tuples are not Equatable; compare a mapped field.
        #expect(EyeShading.shade(dx: 0.8, dy: 0.7, gaze: ahead, lid: 0, spin: 0).map { $0.brightness } == nil)
    }

    @Test func centreOfAForwardGazeIsThePupil() {
        let s = EyeShading.shade(dx: 0, dy: 0, gaze: ahead, lid: 0, spin: 0)!
        #expect(!s.isIris)
        // b = sp * 0.35 with sp = pow(hv.z, 48)
        #expect(close(s.brightness, pow(EyeShading.halfVector.z, 48) * 0.35))
    }

    @Test func ringAroundThePupilIsRedIris() {
        // cos(angle) between 0.83 and 0.955: the normal at dx = 0.4 has z ~ 0.917.
        let s = EyeShading.shade(dx: 0.4, dy: 0, gaze: ahead, lid: 0, spin: 0)!
        #expect(s.isIris)
    }

    @Test func outerSphereIsSclera() {
        let s = EyeShading.shade(dx: -0.7, dy: 0, gaze: ahead, lid: 0, spin: 0)!
        #expect(!s.isIris)
        #expect(s.brightness > 0.1)
    }

    @Test func lidSquashesTheVisibleBand() {
        // |dy| > 1 - lid is lid: dim, never iris.
        let s = EyeShading.shade(dx: 0, dy: 0.4, gaze: Vec3(0, 0.4, 0.916), lid: 0.7, spin: 0)!
        #expect(!s.isIris)
        let lam = max(0, 0 * EyeShading.light.x + 0.4 * EyeShading.light.y + sqrt(1 - 0.16) * EyeShading.light.z)
        #expect(close(s.brightness, 0.05 + 0.3 * lam))
        // A full blink closes everything.
        #expect(EyeShading.shade(dx: 0, dy: 0, gaze: ahead, lid: 1, spin: 0)!.isIris == false)
    }

    @Test func dotRadiusIsHalfCellTimesSqrtBrightnessCapped() {
        #expect(EyeShading.dotRadius(cell: 4, brightness: 0.25) == 1)
        #expect(EyeShading.dotRadius(cell: 4, brightness: 4) == 2 * 1.05)
        #expect(EyeShading.dotRadius(cell: 4, brightness: 0.01) == nil, "below 0.015 no dot")
    }
}
