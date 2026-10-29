import Foundation
import InkerCore

/// The expanded row (AppFindHandle): label and value pairs, then Show in Finder.
public struct DetailLine: Equatable, Sendable, Identifiable {
    public var label: String
    public var value: String
    public var id: String { label }
}

public enum DocDetailsModel {
    public static let showInFinder = "Show in Finder"

    public static func lines(_ d: DocDetails, privateFolderDisplay: String, now: Date,
                             calendar: Calendar = .current, timeZone: TimeZone = .current) -> [DetailLine] {
        var out = [DetailLine(label: "In", value: "\(privateFolderDisplay)/\(d.location)")]
        var dropped = dateText(d.droppedAt, now: now, calendar: calendar, timeZone: timeZone)
        if d.droppedWith > 0 {
            dropped += d.droppedWith == 1 ? ", with 1 other" : ", with \(d.droppedWith) others"
        }
        out.append(DetailLine(label: "Dropped", value: dropped))
        if let read = d.lastRead {
            let ago = RelativeTime.short(from: read.at, to: now)
            let when = ago == "now" ? "just now" : "\(ago) ago"
            out.append(DetailLine(label: "Last read", value: "\(read.agent), \(when) · saw only ink"))
        }
        return out
    }

    /// "Jan 14", with the year when it is not this year.
    static func dateText(_ date: Date, now: Date, calendar: Calendar, timeZone: TimeZone) -> String {
        var cal = calendar
        cal.timeZone = timeZone
        let f = DateFormatter()
        f.calendar = cal
        f.timeZone = timeZone
        f.locale = Locale(identifier: "en_US_POSIX")
        let sameYear = cal.component(.year, from: date) == cal.component(.year, from: now)
        f.dateFormat = sameYear ? "MMM d" : "MMM d, yyyy"
        return f.string(from: date)
    }
}
