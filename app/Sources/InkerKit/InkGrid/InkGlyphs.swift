import Foundation

/// `LET`, `MARK` and `word` from the boards, verbatim.
public enum InkGlyphs {
    public static let letters: [Character: [String]] = [
        "I": ["###", ".#.", ".#.", ".#.", ".#.", ".#.", "###"],
        "N": ["#...#", "##..#", "#.#.#", "#.#.#", "#..##", "#...#", "#...#"],
        "K": ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"],
        "E": ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
        "R": ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
    ]

    /// The 7-cell octopus mark.
    public static let mark: [String] = [
        ".#####.",
        "#######",
        "##.#.##",
        "#######",
        "#.#.#.#",
        "#.#.#.#",
        ".#.#.#.",
    ]

    /// Letters joined by one empty column. Characters without a glyph are skipped.
    public static func word(_ text: String) -> InkBitmap {
        let glyphs = text.compactMap { letters[$0] }
        var rows: [String] = []
        for y in 0..<7 {
            var r = ""
            for (j, g) in glyphs.enumerated() {
                if j > 0 { r += "." }
                r += g[y]
            }
            rows.append(r)
        }
        return InkBitmap(rows: rows)
    }
}
