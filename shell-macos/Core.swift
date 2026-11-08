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
    // Optional-with-default so a core/shell version mismatch degrades to
    // defaults instead of failing the whole decode.
    var appDirs: [String] { appDirs_ ?? [] }
    var appearance: String { appearance_ ?? "system" }
    var systemLinks: [String] { systemLinks_ ?? [] }
    var fileSearch: Bool { fileSearch_ ?? false }
    var searchDirs: [String] { searchDirs_ ?? [] }
    var cat: Bool { cat_ ?? true }
    private let appDirs_: [String]?
    private let appearance_: String?
    private let systemLinks_: [String]?
    private let fileSearch_: Bool?
    private let searchDirs_: [String]?
    private let cat_: Bool?

    enum CodingKeys: String, CodingKey {
        case hotkey, clipboardHotkey, errors
        case appDirs_ = "appDirs"
        case appearance_ = "appearance"
        case systemLinks_ = "systemLinks"
        case fileSearch_ = "fileSearch"
        case searchDirs_ = "searchDirs"
        case cat_ = "cat"
    }
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
        var opts: [String: Any] = ["data_dir": dataDir, "home": home]
        if let s = screenshotDir { opts["screenshot_dir"] = s }
        // Gates System Settings entries that only exist in some macOS versions.
        opts["os_major"] = NSNumber(value: ProcessInfo.processInfo.operatingSystemVersion.majorVersion)
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

    /// Anchors of the built-in System Settings entries visible on this OS,
    /// regardless of the user's selection. The enable-all toggle saves this
    /// list. Nil if the core is unreachable.
    static func systemLinkAnchors() -> [String]? {
        guard let d = take(lc_system_links()),
              let links = try? decoder().decode([SystemLink].self, from: d) else { return nil }
        return links.map(\.anchor)
    }

    private struct SystemLink: Decodable {
        let anchor: String
        let title: String
    }

    /// Merge fields into config.toml and reload. Returns the fresh settings
    /// and, on failure, an error message.
    static func writeConfig(_ fields: [String: Any]) -> (settings: Settings?, error: String?) {
        guard let json = try? JSONSerialization.data(withJSONObject: fields),
              let str = String(data: json, encoding: .utf8) else {
            return (nil, "bad config json")
        }
        guard let d = take(lc_write_config(str)), let data = String(data: d, encoding: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: Data(data.utf8)) as? [String: Any] else {
            return (nil, "core unreachable")
        }
        if let err = obj["error"] as? String { return (nil, err) }
        guard let s = try? decoder().decode(Settings.self, from: Data(data.utf8)) else {
            return (nil, "unreadable settings response")
        }
        return (s, nil)
    }
}
