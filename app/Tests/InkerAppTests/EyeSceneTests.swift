import CoreGraphics
import Foundation
import Testing
@testable import InkerKit

/// The per-surface eye behaviours from each board's start function.
struct EyeSceneTests {
    @Test func emptyZoneHasFourDriftingEyesOnA3pxGrid() {
        var s = DriftingEyesScene.emptyZone()
        #expect(s.cell == 3)
        #expect(s.size == CGSize(width: 328, height: 384))
        let f = s.frames(time: 1, dt: 1.0 / 30.0, pointer: nil)
        #expect(f.map(\.radius) == [20, 24, 14, 12])
    }

    @Test func dropDialogAndOnboardingUseTheirBoardSetups() {
        var d = StaringEyesScene.dropDialog()
        #expect(d.cell == 3 && d.size == CGSize(width: 392, height: 76) && d.tuning == .dialog)
        #expect(d.frames(time: 0.5, dt: 1.0 / 30.0, pointer: nil).map(\.radius) == [18, 13, 11, 12, 16, 19])
        var o = StaringEyesScene.onboarding()
        #expect(o.cell == 4 && o.size == CGSize(width: 480, height: 120) && o.tuning == .popover)
        #expect(o.frames(time: 0.5, dt: 1.0 / 30.0, pointer: nil).count == 8)
    }

    @Test func peekerBlinksBetween4_5And4_7AndGlancesAwayAt5() {
        var p = PeekerScene()
        let open = p.frames(time: 6 + 1.0, dt: 0, pointer: nil)[0]
        #expect(open.lid < 0.56)
        let mid = p.frames(time: 6 + 4.6, dt: 0, pointer: nil)[0]
        #expect(mid.lid > 0.99)
        let away = p.frames(time: 6 + 5.3, dt: 0, pointer: nil)[0]
        #expect(close(away.gaze.x, sin(-0.7) * cos(0.05)))
        #expect(p.cell == 2 && p.size == CGSize(width: 24, height: 24))
    }

    @Test func peekSquintsBetweenPoint2AndPoint4WhenNotBlinking() {
        var p = PeekScene(valueLength: 9, characterWidth: 7.2)
        for k in 1..<200 {
            let t = 0.3 + Double(k) * 0.02
            let f = p.frames(time: t, dt: 0.02, pointer: nil)[0]
            if t.truncatingRemainder(dividingBy: 4.1) >= 0.2 {
                #expect(f.lid >= 0.2 - 1e-9 && f.lid <= 0.4 + 1e-9)
            }
            #expect(f.radius == 32)
        }
        #expect(p.outline == CGRect(x: 90, y: 41, width: 9 * 7.2 + 12, height: 22))
    }
}
