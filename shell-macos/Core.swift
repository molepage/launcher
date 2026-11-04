// Thin Swift wrapper over the Rust core's C ABI.

import Foundation

struct Icon: Decodable {
    let kind: String
    let value: String
}

struct Action: Decodable {
    let type: String
    let path: String?
    let url: String?
    let text: String?
    let argv: [String]?
    let name: String?
}

struct Item: Decodable {
    let uid: String
    let title: String
    let subtitle: String
    let icon: Icon?
    let action: Action
    let altAction: Action?
}

struct Settings: Decodable {
    let hotkey: String
    let clipboardHotkey: String
    let errors: [String]
}

private struct QueryResult: Decodable {
    let items: [Item]
    let slow: Bool?
}

private struct SlowResult: Decodable {
    let items: [Item]
}

enum Core {
    private static func decoder() -> JSONDecoder {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }

    private static func take(_ p: UnsafeMutablePointer<CChar>?) -> Data? {
        guard let p else { return nil }
        defer { lc_free(p) }
        return Data(bytes: p, count: strlen(p))
    }

    /// Returns an error message, or nil on success.
    static func start(dataDir: String, home: String, screenshotDir: String?) -> String? {
        var opts = ["data_dir": dataDir, "home": home]
        if let s = screenshotDir { opts["screenshot_dir"] = s }
        let json = String(data: try! JSONSerialization.data(withJSONObject: opts), encoding: .utf8)!
        return take(lc_init(json)).flatMap { String(data: $0, encoding: .utf8) }
    }

    static func query(_ q: String) -> (items: [Item], slow: Bool) {
        guard let d = take(lc_query(q)), let r = try? decoder().decode(QueryResult.self, from: d) else {
            return ([], false)
        }
        return (r.items, r.slow ?? false)
    }

    /// Blocks. Call off the main thread.
    static func querySlow(_ q: String) -> [Item] {
        guard let d = take(lc_query_slow(q)), let r = try? decoder().decode(SlowResult.self, from: d) else {
            return []
        }
        return r.items
    }

    static func cancelSlow() { lc_cancel_slow() }
    static func picked(query: String, uid: String) { lc_picked(query, uid) }
    static func clipboardAdd(_ text: String, app: String) { lc_clipboard_add(text, app) }
    static func refresh() { lc_refresh() }

    static func settings() -> Settings? {
        guard let d = take(lc_settings()) else { return nil }
        return try? decoder().decode(Settings.self, from: d)
    }
}
