// Performs the actions the core attaches to results.

import AppKit
import ApplicationServices

enum Actions {
    static func perform(_ a: Action) {
        let ws = NSWorkspace.shared
        switch a.type {
        case "open":
            if let p = a.path { ws.open(URL(fileURLWithPath: p)) }
        case "open_url":
            if let s = a.url, let u = URL(string: s) { ws.open(u) }
        case "copy":
            copy(a.text ?? "")
        case "paste":
            copy(a.text ?? "")
            paste()
        case "copy_file":
            if let p = a.path {
                let pb = NSPasteboard.general
                pb.clearContents()
                pb.writeObjects([URL(fileURLWithPath: p) as NSURL])
            }
        case "reveal":
            if let p = a.path { ws.activateFileViewerSelecting([URL(fileURLWithPath: p)]) }
        case "run":
            run(a.argv ?? [])
        case "internal":
            if a.name == "quit" { NSApp.terminate(nil) }
        default:
            NSLog("Launcher: unknown action %@", a.type)
        }
    }

    static func copy(_ text: String) {
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(text, forType: .string)
    }

    /// Send ⌘V to the app in front. Needs Accessibility; without it the text
    /// is left on the clipboard and macOS is asked to prompt for access.
    static func paste() {
        guard AXIsProcessTrusted() else {
            let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
            AXIsProcessTrustedWithOptions([key: true] as CFDictionary)
            return
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
            let src = CGEventSource(stateID: .combinedSessionState)
            for down in [true, false] {
                let e = CGEvent(keyboardEventSource: src, virtualKey: 9, keyDown: down)
                e?.flags = .maskCommand
                e?.post(tap: .cghidEventTap)
            }
        }
    }

    private static func run(_ argv: [String]) {
        guard let first = argv.first else { return }
        let p = Process()
        if first.hasPrefix("/") {
            p.executableURL = URL(fileURLWithPath: first)
            p.arguments = Array(argv.dropFirst())
        } else {
            p.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            p.arguments = argv
        }
        p.standardInput = FileHandle.nullDevice
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        do { try p.run() } catch { NSLog("Launcher: could not run %@: %@", first, "\(error)") }
    }
}
