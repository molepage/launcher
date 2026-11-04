// Launcher for macOS: the native shell around the Rust core.

import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var controller: PanelController!
    private let clipboard = ClipboardWatcher()

    func applicationDidFinishLaunching(_ notification: Notification) {
        let fm = FileManager.default
        let support = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Launcher", isDirectory: true)
        let shots = UserDefaults(suiteName: "com.apple.screencapture")?.string(forKey: "location")
            .map { ($0 as NSString).expandingTildeInPath }

        if let err = Core.start(dataDir: support.path, home: NSHomeDirectory(), screenshotDir: shots) {
            fail("Launcher couldn't start", err)
        }
        installExampleAddons(into: support.appendingPathComponent("addons"))

        controller = PanelController()
        let s = Core.settings()
        let hotkey = s?.hotkey ?? "alt+space"
        if !HotKeys.register(hotkey, id: 1, { [weak self] in self?.controller.toggle() }) {
            alert("Couldn't set the hotkey “\(hotkey)”",
                  "Another app may be using it. Change `hotkey` in \(support.path)/config.toml and restart.")
        }
        if let clip = s?.clipboardHotkey, !clip.isEmpty {
            if !HotKeys.register(clip, id: 2, { [weak self] in self?.controller.toggle(prefill: "clip ") }) {
                NSLog("Launcher: couldn't register clipboard hotkey %@", clip)
            }
        }
        for e in s?.errors ?? [] { NSLog("Launcher: %@", e) }
        clipboard.start()

        let args = CommandLine.arguments
        if args.contains("--show") { controller.show() }
        if let i = args.firstIndex(of: "--snapshot"), i + 2 < args.count {
            let q = args[i + 2]
            controller.snapshot(query: q, to: args[i + 1], after: Double(ProcessInfo.processInfo.environment["SNAPSHOT_DELAY"] ?? "") ?? (q.hasPrefix("f ") ? 7 : 1.2)) { exit(0) }
        }
    }

    /// First run: copy the bundled example add-ons so there's something to try.
    private func installExampleAddons(into dir: URL) {
        let fm = FileManager.default
        let marker = dir.appendingPathComponent(".examples-installed")
        guard !fm.fileExists(atPath: marker.path),
              let bundled = Bundle.main.resourceURL?.appendingPathComponent("addons"),
              let names = try? fm.contentsOfDirectory(atPath: bundled.path) else { return }
        for name in names where !fm.fileExists(atPath: dir.appendingPathComponent(name).path) {
            try? fm.copyItem(at: bundled.appendingPathComponent(name), to: dir.appendingPathComponent(name))
        }
        fm.createFile(atPath: marker.path, contents: nil)
    }

    private func alert(_ title: String, _ text: String) {
        NSApp.activate(ignoringOtherApps: true)
        let a = NSAlert()
        a.messageText = title
        a.informativeText = text
        a.runModal()
    }

    private func fail(_ title: String, _ text: String) -> Never {
        alert(title, text)
        exit(1)
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
