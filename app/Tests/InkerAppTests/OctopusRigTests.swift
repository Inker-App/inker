import CoreGraphics
import Foundation
import Testing
@testable import InkerKit

/// `build(T)` and the rotation from AppOnboard2 / WorldOctopus. Golden values
/// at T = 1 from node (the board stores points in a Float32Array, hence 1e-6).
struct OctopusRigTests {
    @Test func eightTentaclesOfNinePoints() {
        let t = OctopusRig.build(time: 1)
        #expect(t.count == 8)
        #expect(t.allSatisfy { $0.points.count == 9 && $0.radii.count == 9 })
    }

    @Test func firstTentacleMatchesTheBoard() {
        let p = OctopusRig.build(time: 1)[0]
        let golden: [Double] = [0.3077748417854309, 0, 0.08760502189397812, 0.38615575432777405, 0.1935424506664276, 0.1099153608083725,
                                0.49074235558509827, 0.3731960654258728, 0.13968487083911896, 0.6296064257621765, 0.5256901383399963, 0.17921115458011627,
                                0.8037795424461365, 0.6320192217826843, 0.22878777980804443, 1.0024536848068237, 0.6698429584503174, 0.2853383719921112,
                                1.1987972259521484, 0.6205904483795166, 0.34122559428215027, 1.3502975702285767, 0.48170897364616394, 0.38434863090515137,
                                1.4088866710662842, 0.28073832392692566, 0.4010254442691803]
        for k in 0..<9 {
            #expect(close(p.points[k].x, golden[k * 3], 1e-6))
            #expect(close(p.points[k].y, golden[k * 3 + 1], 1e-6))
            #expect(close(p.points[k].z, golden[k * 3 + 2], 1e-6))
        }
        let radii: [Double] = [0.18, 0.1631578952074051, 0.1463157832622528, 0.12947368621826172, 0.11263158172369003,
                               0.09578947722911835, 0.07894736528396606, 0.06210526451468468, 0.04526315629482269]
        for (a, b) in zip(p.radii, radii) {
            #expect(close(a, b, 1e-6))
        }
    }

    @Test func boundsMatchTheBoardAndContainEveryCapsule() {
        let all = OctopusRig.build(time: 1)
        let t = all[3]
        #expect(close(t.boundsCenter.x, -0.6659461855888367, 1e-6))
        #expect(close(t.boundsCenter.y, 0.4452718926800622, 1e-6))
        #expect(close(t.boundsCenter.z, 0.5662166062328551, 1e-6))
        #expect(close(t.boundsRadius, 0.890855761478097, 1e-6))
        for tent in all {
            for (p, r) in zip(tent.points, tent.radii) {
                let d = Vec3(p.x - tent.boundsCenter.x, p.y - tent.boundsCenter.y, p.z - tent.boundsCenter.z).length
                #expect(d + r <= tent.boundsRadius + 1e-9)
            }
        }
    }

    @Test func rotationMatchesTheBoardAndIsOrthonormal() {
        let m = OctopusRig.rotation(time: 1)
        let golden = [0.9962533079151944, 0.037617217782244784, -0.0778735603035849, 0, 0.9004471023526769,
                      0.43496553411123023, 0.08648321494968204, -0.43333585218741244, 0.897073404321506]
        for (a, b) in zip(m, golden) {
            #expect(close(a, b))
        }
        for i in 0..<3 {
            for j in 0..<3 {
                let d = m[i * 3] * m[j * 3] + m[i * 3 + 1] * m[j * 3 + 1] + m[i * 3 + 2] * m[j * 3 + 2]
                #expect(close(d, i == j ? 1 : 0, 1e-12))
            }
        }
    }

    @Test func eyesAndConstants() {
        #expect(OctopusRig.eyes == [Vec3(-0.25, -0.40, -0.5), Vec3(0.25, -0.40, -0.5)])
        #expect(OctopusRig.marchSteps == 56)
        #expect(OctopusRig.boundingRadius == 2)
        #expect(OctopusRig.framesPerSecond == 12)
    }

    @Test func lookIsClampedAndEased() {
        // Onboarding: lx = cl((P.x - cx0) / 160), ly = cl((P.y - cy0 + 30) / 160); ease min(1, dt * 6).
        var look = OctopusLook(center: CGPoint(x: 240, y: 58), yOffset: 30, scale: 160)
        look.update(pointer: CGPoint(x: 2000, y: 58), dt: 1)
        #expect(look.x == 1)
        #expect(close(look.y, 30.0 / 160.0))
        look.update(pointer: nil, dt: 1.0 / 60.0)
        #expect(close(look.x, 1 - 0.1))
    }
}
