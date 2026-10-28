import Foundation

/// `rng(seed)` in the boards (mulberry32). Same seed, same sequence as the JS,
/// so the eyes and the reaction-diffusion seeds match the design canvas.
public struct SeededRandom: Sendable {
    private var state: UInt32

    public init(seed: UInt32) {
        state = seed
    }

    /// Next value in [0, 1).
    public mutating func next() -> Double {
        state = state &+ 0x6D2B_79F5
        var t = (state ^ (state >> 15)) &* (1 | state)
        t = (t &+ ((t ^ (t >> 7)) &* (61 | t))) ^ t
        return Double(t ^ (t >> 14)) / 4_294_967_296.0
    }
}
