import Foundation

/// One line of the row menu as the panel draws it.
public struct RowMenuItem: Equatable, Sendable, Identifiable {
    public enum Action: Equatable, Hashable, Sendable {
        case copyOctopus, copyAscii, showInFinder, inkAgain, forget
    }

    public var action: Action
    public var title: String
    /// Drawn in red: Forget while it asks "Are you sure?".
    public var isDestructive: Bool
    /// A hairline above this item.
    public var separatorAbove: Bool
    public var id: Action { action }
}

/// The row menu is INKER's own panel, not an NSMenu: a native menu closes
/// on every click, and Forget has to stay up while it asks "Are you sure?".
/// The artboard (AppFindRow) draws it as a 200-wide dark panel under the row.
public enum RowMenu {
    public static let areYouSure = "Are you sure?"
    public static let width: CGFloat = 200
    public static let gap: CGFloat = 4
    /// Five 29-point items, the separator block and the padding; close
    /// enough to decide whether the panel fits below its row.
    public static let estimatedHeight: CGFloat = 168

    public static func items(_ menu: RowMenuModel, confirmingForget: Bool) -> [RowMenuItem] {
        [
            RowMenuItem(action: .copyOctopus, title: menu.copyOctopus, isDestructive: false, separatorAbove: false),
            RowMenuItem(action: .copyAscii, title: menu.copyAscii, isDestructive: false, separatorAbove: false),
            RowMenuItem(action: .showInFinder, title: menu.showInFinder, isDestructive: false, separatorAbove: false),
            RowMenuItem(action: .inkAgain, title: menu.inkAgain, isDestructive: false, separatorAbove: false),
            RowMenuItem(action: .forget, title: confirmingForget ? areYouSure : menu.forget,
                        isDestructive: confirmingForget, separatorAbove: true),
        ]
    }

    public enum Placement: Equatable, Sendable {
        case below, above
    }

    /// Below the row unless the panel would run past the bottom of the board.
    public static func placement(rowMaxY: CGFloat, panelHeight: CGFloat, boardHeight: CGFloat) -> Placement {
        rowMaxY + gap + panelHeight <= boardHeight ? .below : .above
    }
}

/// What a menu needs to know about its row.
public struct RowMenuTarget: Equatable, Sendable {
    public var menu: RowMenuModel
    public var path: String

    public init(menu: RowMenuModel, path: String) {
        self.menu = menu
        self.path = path
    }
}

public extension BoardModel {
    /// The menu and path of the row an id names, folders and their children included.
    func menuTarget(for id: String) -> RowMenuTarget? {
        for group in groups {
            for row in group.rows {
                switch row {
                case .doc(let d):
                    if d.id == id { return RowMenuTarget(menu: d.menu, path: d.path) }
                case .folder(let f):
                    if f.id == id { return RowMenuTarget(menu: f.menu, path: f.path) }
                    if let c = f.children.first(where: { $0.id == id }) { return RowMenuTarget(menu: c.menu, path: c.path) }
                }
            }
        }
        return nil
    }
}

public extension FindResultsModel {
    func menuTarget(for id: String) -> RowMenuTarget? {
        rows.first { $0.id == id }.map { RowMenuTarget(menu: $0.row.menu, path: $0.row.path) }
    }
}
