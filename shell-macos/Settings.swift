// The settings window: appearance, hotkeys, extra app folders and deep
// links into System Settings. Every change is written straight to
// config.toml via the core and applied live.

import AppKit

/// Mirrors core/src/engine.rs SYSTEM_LINKS: (anchor, title, x-apple.systempreferences anchor, _).
private let SYSTEM_LINKS: [(String, String, String, [String])] = [
    ("accessibility", "Accessibility", "com.apple.preference.security?Privacy_Accessibility", []),
    ("firewall", "Firewall", "com.apple.preference.security?Firewall", []),
    ("notifications", "Notifications", "com.apple.preference.notifications", []),
    ("login_items", "Login Items", "com.apple.LoginItems-Settings.extension", []),
    ("privacy", "Privacy & Security", "com.apple.preference.security", []),
    ("network", "Network", "com.apple.preference.network", []),
    ("sound", "Sound", "com.apple.preference.sound", []),
    ("displays", "Displays", "com.apple.preference.displays", []),
    ("battery", "Battery", "com.apple.preference.battery", []),
    ("keyboard", "Keyboard", "com.apple.preference.keyboard", []),
    ("mouse", "Mouse", "com.apple.preference.mouse", []),
    ("trackpad", "Trackpad", "com.apple.preference.trackpad", []),
    ("printer", "Printers & Scanners", "com.apple.preference.printfax", []),
    ("bluetooth", "Bluetooth", "com.apple.preference.bluetooth", []),
    ("siri", "Siri & Spotlight", "com.apple.preference.siri", []),
    ("desktop", "Desktop & Dock", "com.apple.preference.dock", []),
]

/// Click to record a shortcut. Emits the same "cmd+shift+v" spec format
/// the config uses.
final class HotkeyRecorder: NSView {
    var spec: String { didSet { needsDisplay = true } }
    var onChange: ((String) -> Void)?
    private var recording = false { didSet { needsDisplay = true } }

    init(spec: String) {
        self.spec = spec
        super.init(frame: NSRect(x: 0, y: 0, width: 180, height: 26))
    }

    required init?(coder: NSCoder) { fatalError("unsupported") }

    override var acceptsFirstResponder: Bool { true }

    private func symbols(_ s: String) -> String {
        let parts = s.split(separator: "+").map(String.init)
        var out = ""
        var key = ""
        for p in parts {
            switch p.lowercased() {
            case "cmd", "command": out += "⌘"
            case "alt", "opt", "option": out += "⌥"
            case "ctrl", "control": out += "⌃"
            case "shift": out += "⇧"
            case "space": key = "Space"
            case "return": key = "↩"
            case "tab": key = "⇥"
            case "escape": key = "⎋"
            default: key = p.uppercased()
            }
        }
        return out + key
    }

    override func draw(_ dirtyRect: NSRect) {
        let r = bounds.insetBy(dx: 0.5, dy: 0.5)
        let path = NSBezierPath(roundedRect: r, xRadius: 5, yRadius: 5)
        NSColor.textBackgroundColor.setFill()
        path.fill()
        (recording ? NSColor.controlAccentColor : NSColor.quaternaryLabelColor).setStroke()
        path.lineWidth = recording ? 2 : 1
        path.stroke()
        let text = recording ? "Type shortcut…" : symbols(spec.isEmpty ? "none" : spec)
        let attrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 13, weight: recording ? .regular : .medium),
            .foregroundColor: recording ? NSColor.secondaryLabelColor : NSColor.labelColor,
        ]
        let size = (text as NSString).size(withAttributes: attrs)
        (text as NSString).draw(at: NSPoint(x: r.midX - size.width / 2, y: r.midY - size.height / 2), withAttributes: attrs)
    }

    override func mouseDown(with event: NSEvent) {
        // Recording starts on an explicit click only — the view can become
        // first responder on window open without the user asking for it.
        recording = true
        window?.makeFirstResponder(self)
    }

    override func resignFirstResponder() -> Bool {
        recording = false
        return true
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard recording else { return super.performKeyEquivalent(with: event) }
        handle(event)
        return true
    }

    override func keyDown(with event: NSEvent) {
        guard recording else { return super.keyDown(with: event) }
        handle(event)
    }

    private func handle(_ event: NSEvent) {
        if event.keyCode == 53 {  // esc cancels the recording
            window?.makeFirstResponder(nil)
            return
        }
        guard let name = HotKeys.specName(keyCode: event.keyCode) else { return NSSound.beep() }
        var parts: [String] = []
        let f = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if f.contains(.command) { parts.append("cmd") }
        if f.contains(.option) { parts.append("alt") }
        if f.contains(.control) { parts.append("ctrl") }
        if f.contains(.shift) { parts.append("shift") }
        // Bare letter/number shortcuts would swallow normal typing; function
        // keys are fine alone.
        if parts.isEmpty, !name.hasPrefix("f") { return NSSound.beep() }
        spec = (parts + [name]).joined(separator: "+")
        window?.makeFirstResponder(nil)
        onChange?(spec)
    }
}

final class SettingsWindowController: NSWindowController, NSWindowDelegate, NSTableViewDataSource, NSTableViewDelegate {
    static let shared = SettingsWindowController()

    /// Re-register global hotkeys after a change (set by the app delegate).
    var reapplyHotkeys: (() -> Void)?
    /// Apply the configured appearance to the panel (set by the panel).
    var reapplyAppearance: ((String) -> Void)?

    private var settings: Settings?
    private var folders: [String] = []
    private var searchDirs: [String] = []
    private let table = NSTableView()
    private let searchTable = NSTableView()
    private var launchRecorder: HotkeyRecorder?
    private var clipRecorder: HotkeyRecorder?
    private var appearanceButtons: [NSButton] = []

    convenience init() {
        let win = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 470, height: 600),
                           styleMask: [.titled, .closable], backing: .buffered, defer: false)
        win.title = "Launcher Settings"
        win.isReleasedWhenClosed = false
        win.center()
        self.init(window: win)
        win.delegate = self
        rebuild()
    }

    func windowWillClose(_ notification: Notification) {
        // Drop the reference so a fresh copy of settings loads next time.
        settings = nil
    }

    /// Open next to the launcher panel (below it, or above when the space
    /// below is too tight) instead of wherever the window manager pleases.
    func show(panelFrame: NSRect? = nil) {
        if settings == nil { rebuild() }
        showWindow(nil)
        if let pf = panelFrame, let win = window,
           let screen = NSScreen.screens.first(where: { $0.frame.intersects(pf) }) ?? NSScreen.main {
            let vis = screen.visibleFrame
            let size = win.frame.size
            var x = min(max(pf.midX - size.width / 2, vis.minX + 8), vis.maxX - size.width - 8)
            var y = pf.minY - size.height - 12
            if y < vis.minY { y = pf.maxY + 12 }
            x = max(x, vis.minX + 8)
            win.setFrameOrigin(NSPoint(x: x, y: y))
        }
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    // MARK: building the UI

    private func rebuild() {
        settings = Core.settings()
        guard let s = settings else { return }
        folders = s.appDirs
        searchDirs = s.searchDirs
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 470, height: 600))

        var y: CGFloat = 600
        func section(_ title: String) {
            let l = NSTextField(labelWithString: title.uppercased())
            l.font = .systemFont(ofSize: 11, weight: .semibold)
            l.textColor = .secondaryLabelColor
            l.frame = NSRect(x: 20, y: y - 20, width: 430, height: 16)
            content.addSubview(l)
            y -= 28
        }
        func reserve(_ h: CGFloat) -> CGFloat { y -= h; return y }

        section("Appearance")
        let ap = NSStackView(views: radioGroup(["system", "light", "dark"]))
        ap.orientation = .horizontal
        ap.spacing = 16
        ap.frame = NSRect(x: 20, y: reserve(24), width: 430, height: 24)
        content.addSubview(ap)

        y -= 14
        section("Hotkeys")
        let row1 = reserve(28)
        launchRecorder = addHotkeyRow(in: content, spec: s.hotkey) { [weak self] spec in
            self?.save(["hotkey": spec], then: { self?.reapplyHotkeys?() })
        }
        launchRecorder?.frame = NSRect(x: 270, y: row1, width: 180, height: 26)
        hotkeyLabel("Open launcher", at: row1, in: content)
        let row2 = reserve(28)
        clipRecorder = addHotkeyRow(in: content, spec: s.clipboardHotkey) { [weak self] spec in
            self?.save(["clipboard_hotkey": spec], then: { self?.reapplyHotkeys?() })
        }
        clipRecorder?.frame = NSRect(x: 270, y: row2, width: 180, height: 26)
        hotkeyLabel("Clipboard history", at: row2, in: content)

        y -= 14
        section("Index Settings")
        let toggleRow = systemToggleRow()
        toggleRow.frame.origin.y = y - 56
        content.addSubview(toggleRow)
        y -= 56

        let appsTitle = NSTextField(labelWithString: "Extra App Folders")
        appsTitle.font = .systemFont(ofSize: 13, weight: .medium)
        appsTitle.frame = NSRect(x: 20, y: y - 20, width: 300, height: 18)
        let appsSub = NSTextField(labelWithString: "Folders beyond the standard ones to scan for apps")
        appsSub.font = .systemFont(ofSize: 11)
        appsSub.textColor = .secondaryLabelColor
        appsSub.frame = NSRect(x: 20, y: y - 36, width: 360, height: 16)
        content.addSubview(appsTitle)
        content.addSubview(appsSub)
        y -= 40
        let foldersHolder = folderList(height: 96, items: folders, tv: table)
        foldersHolder.frame.origin.y = y - 96
        content.addSubview(foldersHolder)
        y -= 96

        y -= 14
        section("File Search")
        let fsToggle = systemToggleRow(title: "File Search",
                                       subtitle: "Search file names by typing “f <name>”. Off = nothing on disk is read.",
                                       on: s.fileSearch,
                                       action: #selector(fileSearchToggled(_:)))
        fsToggle.frame.origin.y = y - 56
        content.addSubview(fsToggle)
        y -= 56
        let dirsHolder = folderList(height: 96, items: searchDirs, tv: searchTable)
        dirsHolder.frame.origin.y = y - 96
        content.addSubview(dirsHolder)
        y -= 96

        window?.contentView = content
    }

    private func hotkeyLabel(_ text: String, at y: CGFloat, in view: NSView) {
        let l = NSTextField(labelWithString: text)
        l.frame = NSRect(x: 20, y: y + 5, width: 240, height: 18)
        view.addSubview(l)
    }

    private func addHotkeyRow(in view: NSView, spec: String, commit: @escaping (String) -> Void) -> HotkeyRecorder {
        let rec = HotkeyRecorder(spec: spec)
        rec.onChange = { new in
            guard HotKeys.parse(new) != nil else { return NSSound.beep() }
            commit(new)
        }
        view.addSubview(rec)
        return rec
    }

    private func radioGroup(_ options: [String]) -> [NSButton] {
        appearanceButtons.removeAll()
        let current = settings?.appearance ?? "system"
        return options.map { name in
            let b = NSButton(title: name == "system" ? "System" : name.capitalized,
                             target: self, action: #selector(appearanceChanged(_:)))
            b.setButtonType(.radio)
            b.state = name == current ? .on : .off
            b.identifier = NSUserInterfaceItemIdentifier(name)
            appearanceButtons.append(b)
            return b
        }
    }

    @objc private func appearanceChanged(_ sender: NSButton) {
        guard sender.state == .on, let name = sender.identifier?.rawValue else { return }
        appearanceButtons.forEach { $0.state = $0 === sender ? .on : .off }
        save(["appearance": name]) { [weak self] in self?.reapplyAppearance?(name) }
    }

    private func folderList(height: CGFloat, items: [String], tv: NSTableView) -> NSView {
        let col = NSTableColumn(identifier: .init("path"))
        col.width = 380
        tv.addTableColumn(col)
        tv.headerView = nil
        tv.dataSource = self
        tv.delegate = self
        tv.rowHeight = 22
        tv.allowsEmptySelection = true
        let holder = NSView(frame: NSRect(x: 0, y: 0, width: 450, height: height))
        let scroll = NSScrollView(frame: NSRect(x: 20, y: 2, width: 386, height: height - 4))
        scroll.documentView = tv
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder

        let add = NSButton(title: "+", target: self, action: #selector(addFolder(_:)))
        add.bezelStyle = .regularSquare
        add.identifier = NSUserInterfaceItemIdentifier(tv === table ? "apps" : "search")
        add.frame = NSRect(x: 412, y: height - 28, width: 38, height: 26)
        let del = NSButton(title: "−", target: self, action: #selector(removeFolder(_:)))
        del.bezelStyle = .regularSquare
        del.identifier = add.identifier
        del.frame = NSRect(x: 412, y: height - 58, width: 38, height: 26)
        holder.addSubview(scroll)
        holder.addSubview(add)
        holder.addSubview(del)
        tv.reloadData()
        return holder
    }

    private func items(for tableView: NSTableView) -> [String] {
        tableView === table ? folders : searchDirs
    }

    /// One switch row: title, subtitle and an NSSwitch on the right.
    private func systemToggleRow(title text: String, subtitle: String, on: Bool, action: Selector) -> NSView {
        let holder = NSView(frame: NSRect(x: 0, y: 0, width: 450, height: 56))
        let title = NSTextField(labelWithString: text)
        title.font = .systemFont(ofSize: 13, weight: .medium)
        title.frame = NSRect(x: 20, y: 30, width: 300, height: 18)
        let sub = NSTextField(labelWithString: subtitle)
        sub.font = .systemFont(ofSize: 11)
        sub.textColor = .secondaryLabelColor
        sub.frame = NSRect(x: 20, y: 12, width: 360, height: 16)
        let toggle = NSSwitch()
        toggle.controlSize = .regular
        toggle.target = self
        toggle.action = action
        toggle.state = on ? .on : .off
        toggle.sizeToFit()
        toggle.frame.origin = NSPoint(x: 450 - 20 - toggle.frame.width, y: 20)
        holder.addSubview(title)
        holder.addSubview(sub)
        holder.addSubview(toggle)
        return holder
    }

    /// One switch: on = all System Settings panes appear as launcher entries.
    private func systemToggleRow() -> NSView {
        systemToggleRow(title: "System Settings",
                        subtitle: "Search panes like “firewall” straight from the launcher",
                        on: !(settings?.systemLinks.isEmpty ?? true),
                        action: #selector(systemToggleChanged(_:)))
    }

    @objc private func systemToggleChanged(_ sender: NSSwitch) {
        let links = sender.state == .on ? SYSTEM_LINKS.map(\.0) : []
        save(["system_links": links], then: { Core.refresh() })
    }

    @objc private func fileSearchToggled(_ sender: NSSwitch) {
        save(["file_search": sender.state == .on], then: { Core.refresh() })
    }

    @objc private func addFolder(_ sender: NSButton) {
        let isSearch = sender.identifier?.rawValue == "search"
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = false
        panel.prompt = "Add"
        panel.beginSheetModal(for: window!) { [weak self] resp in
            guard resp == .OK, let url = panel.url, let self else { return }
            let path = url.path
            if isSearch {
                guard !self.searchDirs.contains(path) else { return }
                self.searchDirs.append(path)
                self.searchTable.reloadData()
                self.save(["search_dirs": self.searchDirs], then: { Core.refresh() })
            } else {
                guard !self.folders.contains(path) else { return }
                self.folders.append(path)
                self.table.reloadData()
                self.save(["app_dirs": self.folders], then: { Core.refresh() })
            }
        }
    }

    @objc private func removeFolder(_ sender: NSButton) {
        let isSearch = sender.identifier?.rawValue == "search"
        let (tv, key) = isSearch ? (searchTable, "search_dirs") : (table, "app_dirs")
        var list = items(for: tv)
        let i = tv.selectedRow
        guard i >= 0, i < list.count else { return }
        list.remove(at: i)
        if isSearch { searchDirs = list } else { folders = list }
        tv.reloadData()
        save([key: list], then: { Core.refresh() })
    }

    private func save(_ fields: [String: Any], then after: (() -> Void)? = nil) {
        let (s, err) = Core.writeConfig(fields)
        if let s {
            settings = s
            after?()
        } else {
            let a = NSAlert()
            a.messageText = "Couldn't save settings"
            a.informativeText = err ?? "unknown error"
            a.runModal()
        }
    }

    // MARK: folder tables

    func numberOfRows(in tableView: NSTableView) -> Int { items(for: tableView).count }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let path = items(for: tableView)[row] as NSString
        let cell = NSTextField(labelWithString: path.expandingTildeInPath)
        cell.lineBreakMode = .byTruncatingHead
        cell.font = .systemFont(ofSize: 12)
        let id = NSUserInterfaceItemIdentifier("cell")
        let v = tableView.makeView(withIdentifier: id, owner: nil) as? NSTableCellView ?? {
            let c = NSTableCellView()
            c.identifier = id
            c.addSubview(cell)
            cell.translatesAutoresizingMaskIntoConstraints = false
            NSLayoutConstraint.activate([
                cell.leadingAnchor.constraint(equalTo: c.leadingAnchor, constant: 6),
                cell.trailingAnchor.constraint(equalTo: c.trailingAnchor, constant: -6),
                cell.centerYAnchor.constraint(equalTo: c.centerYAnchor),
            ])
            return c
        }()
        (v.subviews.first as? NSTextField)?.stringValue = path.expandingTildeInPath
        return v
    }
}
