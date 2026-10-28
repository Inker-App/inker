import Foundation
import Testing
@testable import InkerKit

struct InkMathTests {
    @Test func smoothstepClampsAndEases() {
        #expect(InkMath.smoothstep(0.55, 0.9, 0.2) == 0)
        #expect(InkMath.smoothstep(0.55, 0.9, 0.55) == 0)
        #expect(InkMath.smoothstep(0.55, 0.9, 0.9) == 1)
        #expect(InkMath.smoothstep(0.55, 0.9, 2) == 1)
        #expect(close(InkMath.smoothstep(0.55, 0.9, 0.725), 0.5))
    }

    @Test func normalizedVectorHasUnitLengthAndZeroStaysFinite() {
        let v = Vec3.normalized(-0.45, -0.55, 0.7)
        #expect(close(v.length, 1))
        let z = Vec3.normalized(0, 0, 0)
        #expect(z == Vec3(0, 0, 0)) // JS: hypot(...) || 1
    }

    @Test func dotProduct() {
        #expect(Vec3(1, 2, 3).dot(Vec3(4, -5, 6)) == 12)
    }
}
