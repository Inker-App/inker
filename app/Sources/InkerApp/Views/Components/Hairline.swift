import SwiftUI

/// A 1 pt rule in the hairline colour.
struct Hairline: View {
    var color: Color = Palette.hairline

    var body: some View {
        Rectangle().fill(color).frame(height: 1)
    }
}
