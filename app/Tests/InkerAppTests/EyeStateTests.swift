import CoreGraphics
import Foundation
import Testing
@testable import InkerKit

/// `mkEye` / `eyeStep` from AppPopEmpty. Golden values from running the board's
/// JS in node: two eyes sharing `rng(31)`, stepped at 1/30 s, with a target at
/// (200, 100) between steps 151 and 219.
struct EyeStateTests {
    @Test func initialisationConsumesRandomsInTheBoardsOrder() {
        var r = SeededRandom(seed: 31)
        let a = EyeState(x: 62, y: 74, radius: 20, random: &r)
        let b = EyeState(x: 268, y: 300, radius: 24, random: &r)
        let expected = [1.3445122321136296, 4.343784149736166, 4.710116626580013, 0.3073463796281722,
                        0.8334786631166935, 2.3825110914185643, 0.4728067902820185, 5.85286882194248]
        let got = [a.nextSaccade, a.nextBlink, a.phase, a.spin, b.nextSaccade, b.nextBlink, b.phase, b.spin]
        for (g, e) in zip(got, expected) {
            #expect(close(g, e))
        }
    }

    @Test func traceMatchesTheBoard() {
        // [step, a.yaw, a.pitch, a.lid, a.next, a.dart, a.nb, b.yaw, b.pitch, b.lid]
        let golden: [[Double]] = [
            [25, 0, 0, 0, 1.3445122321136296, 0, 4.343784149736166, 0, 0, 0],
            [50, -0.8596022161841687, 0.27809259942578063, 0, 3.141122913795213, 2.2420235224689047, 4.343784149736166, -0.3411945219931678, 0.2993102056102973, 0],
            [75, -0.8596923614619199, 0.27812176260631205, 0, 3.141122913795213, 2.2420235224689047, 4.343784149736166, -0.3411945220315829, 0.29931020564399663, 1],
            [100, 0.6269542413109616, 0.3537224666771695, 0, 5.593690574976305, 4.170437140638629, 4.343784149736166, 0.27824275764912365, 0.17839846614599228, 0],
            [150, 0.6330685901688412, 0.35403340074699374, 0, 5.593690574976305, 4.170437140638629, 6.7415151249306895, 0.29451697189360854, 0.17522180303931237, 0],
            [175, 0.7117063445142456, 0.12243737742136288, 0, 7.616895248182118, 0, 6.7415151249306895, -0.4201620256998584, -0.2893750071867637, 0],
            [225, 0.9170008125616009, 0.412926035972995, 0, 7.616895248182118, 0, 10.83663182623374, -0.3213535885576486, -0.15415990254953418, 0],
            [300, 0.8679985257564112, 0.29248980979900807, 0, 10.424834961568315, 9.153232801457246, 10.83663182623374, -0.4375627205474296, 0.38991803647950035, 0],
        ]
        var r = SeededRandom(seed: 31)
        var a = EyeState(x: 62, y: 74, radius: 20, random: &r)
        var b = EyeState(x: 268, y: 300, radius: 24, random: &r)
        let dt = 1.0 / 30.0
        var rows: [Int: [Double]] = [:]
        for k in 1...300 {
            let t = Double(k) * dt
            let target: CGPoint? = (k > 150 && k < 220) ? CGPoint(x: 200, y: 100) : nil
            a.step(dt: dt, time: t, target: target, tuning: .popover, random: &r)
            b.step(dt: dt, time: t, target: target, tuning: .popover, random: &r)
            rows[k] = [Double(k), a.yaw, a.pitch, a.lid, a.nextSaccade, a.dartUntil, a.nextBlink, b.yaw, b.pitch, b.lid]
        }
        for g in golden {
            let row = rows[Int(g[0])]!
            for (x, y) in zip(row, g) {
                #expect(close(x, y, 1e-9), "step \(Int(g[0]))")
            }
        }
    }

    @Test func saccadesAreScheduledBetween0_4And2_8Seconds() {
        var r = SeededRandom(seed: 7)
        var e = EyeState(x: 0, y: 0, radius: 10, random: &r)
        var t = 0.0
        var lastNext = e.nextSaccade
        var gaps: [Double] = []
        while t < 120 {
            t += 1.0 / 60.0
            e.step(dt: 1.0 / 60.0, time: t, target: nil, tuning: .popover, random: &r)
            if e.nextSaccade != lastNext {
                gaps.append(e.nextSaccade - t)
                lastNext = e.nextSaccade
            }
        }
        #expect(gaps.count > 30)
        #expect(gaps.allSatisfy { $0 >= 0.4 && $0 <= 2.8 })
    }

    @Test func blinksLastPoint2SecondsAndRecurEvery2To7Seconds() {
        var r = SeededRandom(seed: 9)
        var e = EyeState(x: 0, y: 0, radius: 10, random: &r)
        let dt = 1.0 / 240.0
        var t = 0.0
        var closedSince: Double?
        var durations: [Double] = []
        var intervals: [Double] = []
        var lastStart: Double?
        var peak = 0.0
        while t < 200 {
            t += dt
            e.step(dt: dt, time: t, target: nil, tuning: .popover, random: &r)
            peak = max(peak, e.lid)
            if e.lid > 0, closedSince == nil {
                closedSince = t
                if let s = lastStart { intervals.append(t - s) }
                lastStart = t
            } else if e.lid == 0, let s = closedSince {
                durations.append(t - s)
                closedSince = nil
            }
        }
        #expect(durations.count > 20)
        #expect(durations.allSatisfy { $0 > 0.18 && $0 < 0.21 })
        // Interval = 0.2 s blink + 2..7 s wait (plus step quantisation).
        #expect(intervals.allSatisfy { $0 >= 2.2 - 0.01 && $0 <= 7.2 + 0.01 })
        #expect(peak > 0.99)
    }

    @Test func lidIsAHalfSineOverTheBlink() {
        #expect(EyeState.lid(blinkProgress: 0) == 0)
        #expect(close(EyeState.lid(blinkProgress: 0.5), 1))
        #expect(close(EyeState.lid(blinkProgress: 0.25), sin(Double.pi / 4)))
    }

    @Test func gazeTowardsATargetPoint() {
        // n3(tx - x, ty - y, depth); yaw = atan2(gx, gz), pitch = asin(gy)
        let g = EyeState.gaze(from: CGPoint(x: 0, y: 0), to: CGPoint(x: 160, y: 0), depth: 160)
        #expect(close(g.yaw, Double.pi / 4))
        #expect(close(g.pitch, 0))
        let up = EyeState.gaze(from: CGPoint(x: 10, y: 10), to: CGPoint(x: 10, y: -150), depth: 160)
        #expect(close(up.yaw, 0))
        #expect(close(up.pitch, -Double.pi / 4))
    }

    @Test func dartingDelaysLookingAtTheCursor() {
        var r = SeededRandom(seed: 31)
        var e = EyeState(x: 0, y: 0, radius: 10, random: &r)
        e.restYaw = 0.3
        e.dartUntil = 10
        e.nextSaccade = 100
        e.step(dt: 1, time: 5, target: CGPoint(x: 500, y: 0), tuning: .popover, random: &r)
        #expect(e.targetYaw == 0.3, "still darting: keeps the random gaze")
        e.step(dt: 1, time: 11, target: CGPoint(x: 500, y: 0), tuning: .popover, random: &r)
        #expect(e.targetYaw > 1, "dart over: turns to the cursor")
    }

    @Test func frameAddsTheWobbleAndConvertsYawPitchToAGazeVector() {
        var r = SeededRandom(seed: 1)
        var e = EyeState(x: 50, y: 60, radius: 12, random: &r)
        e.yaw = 0.4
        e.pitch = -0.2
        e.lid = 0.1
        let f = e.frame(time: 2, squint: 0.3)
        #expect(close(f.centerX, 50 + sin(2 * 0.7 + e.phase) * 2))
        #expect(close(f.centerY, 60 + cos(2 * 0.9 + e.phase) * 1.5))
        #expect(close(f.gaze.x, sin(0.4) * cos(-0.2)))
        #expect(close(f.gaze.y, sin(-0.2)))
        #expect(close(f.gaze.z, cos(0.4) * cos(-0.2)))
        #expect(f.lid == 0.3, "squint wins over a smaller lid")
        #expect(f.radius == 12)
    }

    @Test func tuningsMatchTheBoards() {
        #expect(EyeTuning.popover.dartChance == 0.45 && EyeTuning.popover.dartSpan == 0.8 && EyeTuning.popover.gazeDepth == 160)
        #expect(EyeTuning.dialog.dartChance == 0.35 && EyeTuning.dialog.dartSpan == 0.6 && EyeTuning.dialog.gazeDepth == 120)
    }
}
