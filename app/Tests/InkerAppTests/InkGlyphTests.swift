import Foundation
import Testing
@testable import InkerKit

/// `bits`, `word`, `LET` and `MARK` from the boards.
struct InkGlyphTests {
    @Test func markRoundTripsAndMatchesTheBoard() {
        let b = InkBitmap(rows: InkGlyphs.mark)
        #expect(b.width == 7 && b.height == 7)
        #expect(b.rows == InkGlyphs.mark)
        #expect(b.cells.reduce(0, +) == 35)
        #expect(InkGlyphs.mark == [".#####.", "#######", "##.#.##", "#######", "#.#.#.#", "#.#.#.#", ".#.#.#."])
    }

    @Test func everyLetterRoundTrips() {
        for (letter, rows) in InkGlyphs.letters {
            #expect(rows.count == 7, "\(letter)")
            #expect(InkBitmap(rows: rows).rows == rows, "\(letter)")
        }
        #expect(Set(InkGlyphs.letters.keys) == Set("INKER"))
    }

    @Test func wordInkerMatchesTheBoard() {
        let w = InkGlyphs.word("INKER")
        #expect(w.width == 27 && w.height == 7)
        #expect(w.cells.reduce(0, +) == 79)
        // Letters are separated by one empty column.
        #expect(w.rows[0] == "###.#...#.#...#.#####.####.")
    }

    @Test func raggedRowsArePaddedWithEmptyCells() {
        let b = InkBitmap(rows: ["#", "###"])
        #expect(b.width == 3)
        #expect(b.rows == ["#..", "###"])
        #expect(b[x: 2, y: 0] == 0 && b[x: 2, y: 1] == 1)
    }
}
