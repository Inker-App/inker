import CoreGraphics
import Foundation

/// |a - b| <= tol. Golden values come from running the design boards' JS in node.
func close(_ a: Double, _ b: Double, _ tol: Double = 1e-9) -> Bool {
    abs(a - b) <= tol
}
