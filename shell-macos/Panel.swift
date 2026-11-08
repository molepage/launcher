// The launcher window: one borderless, non-activating panel created once
// and reused. Non-activating means the app you were in stays the active
// app, so pasting and "quit this app" act on it, and dismissing returns
// focus without a flicker.

import AppKit

final class LauncherPanel: NSPanel {
    weak var controller: PanelController?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func performKeyEquivalent(with e: NSEvent) -> Bool {
        let flags = e.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard flags.contains(.command), let c = controller else { return super.performKeyEquivalent(with: e) }
        if e.keyCode == 36 || e.keyCode == 76 {  // ⌘↩
            c.activateSelected(alt: true)
            return true
        }
        let chars = e.charactersIgnoringModifiers ?? ""
        if let n = Int(chars), (1...9).contains(n) {
            c.activateVisible(n - 1)
            return true
        }
        // There's no Edit menu in an accessory app, so wire up editing keys.
        let editing: [String: Selector] = [
            "x": #selector(NSText.cut(_:)), "c": #selector(NSText.copy(_:)), "v": #selector(NSText.paste(_:)),
            "a": #selector(NSResponder.selectAll(_:)), "z": Selector(("undo:")),
        ]
        if !flags.contains(.shift), let sel = editing[chars.lowercased()] {
            return NSApp.sendAction(sel, to: nil, from: self)
        }
        return super.performKeyEquivalent(with: e)
    }
}

// NSImageView plays animated GIFs on its own; this just keeps upscaled
// pixels crisp instead of blurry.
private final class PixelImageView: NSImageView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override func draw(_ dirtyRect: NSRect) {
        NSGraphicsContext.current?.imageInterpolation = .none
        super.draw(dirtyRect)
    }
}

private final class FlippedView: NSView {
    override var isFlipped: Bool { true }
    var onMouseMoved: ((NSPoint) -> Void)?
    private var moveTracking: NSTrackingArea?

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas where area.owner === self { removeTrackingArea(area) }
        moveTracking = NSTrackingArea(rect: bounds,
                                      options: [.mouseEnteredAndExited, .mouseMoved, .activeAlways, .inVisibleRect],
                                      owner: self)
        addTrackingArea(moveTracking!)
    }

    override func mouseEntered(with event: NSEvent) { onMouseMoved?(convert(event.locationInWindow, from: nil)) }
    override func mouseMoved(with event: NSEvent) { onMouseMoved?(convert(event.locationInWindow, from: nil)) }
    override func mouseExited(with event: NSEvent) { onMouseMoved?(NSPoint(x: -1, y: -1)) }

    // Drag by hand: isMovableByWindowBackground trips an AppKit tracking-rect
    // assertion on borderless, transparent panels (macOS 12).
    override func mouseDown(with event: NSEvent) {
        window?.performDrag(with: event)
    }
}

final class PanelController: NSObject, NSTextFieldDelegate, NSWindowDelegate, NSMenuDelegate {
    let panel: LauncherPanel
    private let field = NSTextField()
    private let results = ResultsView()
    private let separator = NSBox()
    private let gear = NSButton()
    private let content = FlippedView()
    private var menuOpen = false
    private var generation = 0
    private var slowWork: DispatchWorkItem?
    private var lastWasSlow = false
    private var hasPositioned = false
    private var screenObserver: NSObjectProtocol?
    private var catWindow: NSWindow!

    private let width: CGFloat = 700
    private let barHeight: CGFloat = 58

    override init() {
        panel = LauncherPanel(contentRect: NSRect(x: 0, y: 0, width: width, height: barHeight),
                              styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        super.init()
        panel.controller = self
        panel.delegate = self
        panel.isFloatingPanel = true
        panel.level = NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.statusWindow)) + 2)
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient, .ignoresCycle]
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = true
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false

        let bg = NSVisualEffectView()
        bg.material = .popover
        bg.state = .active
        bg.blendingMode = .behindWindow
        // The vibrancy material is composited by the window server, so a layer
        // cornerRadius doesn't clip it (leaves square corners). maskImage is the
        // supported way to round a visual effect view; it stretches as a
        // nine-part image, so it survives resizes.
        let r: CGFloat = 14
        bg.maskImage = {
            let img = NSImage(size: NSSize(width: r * 2, height: r * 2), flipped: false) { rect in
                NSColor.black.setFill()
                NSBezierPath(roundedRect: rect, xRadius: r, yRadius: r).fill()
                return true
            }
            img.capInsets = NSEdgeInsets(top: r, left: r, bottom: r, right: r)
            return img
        }()
        panel.contentView = bg

        let content = self.content
        content.frame = bg.bounds
        content.autoresizingMask = [.width, .height]
        bg.addSubview(content)

        let glass = NSImageView(frame: NSRect(x: 20, y: (barHeight - 22) / 2, width: 22, height: 22))
        glass.image = NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 18, weight: .regular))
        glass.contentTintColor = .secondaryLabelColor
        content.addSubview(glass)

        // The cat sleeps on the roof: a separate transparent child window
        // hovering on the panel's top edge, since a window can't draw above
        // its own bounds. Child windows follow the parent when dragged.
        catWindow = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 64, height: 47),
                             styleMask: [.borderless], backing: .buffered, defer: false)
        catWindow.isOpaque = false
        catWindow.backgroundColor = .clear
        catWindow.hasShadow = false
        catWindow.ignoresMouseEvents = true
        catWindow.level = panel.level
        catWindow.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        if let catURL = Bundle.main.url(forResource: "sleeping-cat", withExtension: "gif", subdirectory: "Assets"),
           let catImage = NSImage(contentsOf: catURL) {
            let cat = PixelImageView(frame: NSRect(x: 0, y: 0, width: 64, height: 47))
            cat.image = catImage
            catWindow.contentView = cat
            panel.addChildWindow(catWindow, ordered: .above)
        }

        field.frame = NSRect(x: 52, y: (barHeight - 30) / 2, width: width - 100, height: 30)
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = .systemFont(ofSize: 22, weight: .regular)
        field.placeholderString = "Search apps, files, clipboard…"
        field.cell?.usesSingleLineMode = true
        field.cell?.lineBreakMode = .byTruncatingHead
        field.delegate = self
        content.addSubview(field)

        gear.frame = NSRect(x: width - 42, y: (barHeight - 22) / 2, width: 22, height: 22)
        gear.isBordered = false
        gear.image = NSImage(systemSymbolName: "gearshape", accessibilityDescription: "Settings")?
            .withSymbolConfiguration(.init(pointSize: 16, weight: .regular))
        gear.contentTintColor = .secondaryLabelColor
        gear.alphaValue = 0
        gear.target = self
        gear.action = #selector(showGearMenu)
        gear.setButtonType(.momentaryChange)
        content.addSubview(gear)
        content.onMouseMoved = { [weak self] p in
            guard let self, !self.menuOpen else { return }
            self.gear.alphaValue = self.gear.frame.insetBy(dx: -8, dy: -8).contains(p) ? 1 : 0
        }

        separator.boxType = .separator
        separator.frame = NSRect(x: 0, y: barHeight, width: width, height: 1)
        content.addSubview(separator)

        results.frame = NSRect(x: 0, y: barHeight + 6, width: width, height: 0)
        results.onActivate = { [weak self] i in self?.activate(i, alt: false) }
        content.addSubview(results)

        Icons.shared.onUpdate = { [weak self] in self?.results.needsDisplay = true }

        // Displays can come and go; never let the panel end up on a dead one.
        screenObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification,
            object: nil, queue: .main
        ) { [weak self] _ in self?.ensureOnScreen() }
    }

    deinit {
        if let observer = screenObserver { NotificationCenter.default.removeObserver(observer) }
    }

    /// Keep the panel on a live screen: reposition entirely if its frame no
    /// longer intersects any display (e.g. the monitor it sat on was
    /// unplugged), otherwise clamp it back inside the visible frame.
    private func ensureOnScreen() {
        guard let screen = NSScreen.screens.first(where: { $0.frame.intersects(panel.frame) })
                ?? NSScreen.screens.first(where: { NSMouseInRect(NSEvent.mouseLocation, $0.frame, false) })
                ?? NSScreen.main else { return }
        let vf = screen.visibleFrame
        let f = panel.frame
        if NSScreen.screens.contains(where: { $0.frame.intersects(f) }) {
            let x = min(max(f.minX, vf.minX + 8), max(vf.minX + 8, vf.maxX - f.width - 8))
            let y = min(max(f.minY, vf.minY + 8), max(vf.minY + 8, vf.maxY - f.height - 8))
            if x != f.minX || y != f.minY {
                panel.setFrameOrigin(NSPoint(x: x, y: y))
                positionCat()
            }
        } else {
            let top = vf.maxY - vf.height * 0.2
            panel.setFrameOrigin(NSPoint(x: vf.midX - f.width / 2, y: top - f.height))
            positionCat()
        }
    }

    // MARK: gear menu

    @objc private func showGearMenu() {
        let menu = NSMenu()
        menu.delegate = self
        let settingsItem = NSMenuItem(title: "Settings…", action: #selector(openSettings), keyEquivalent: ",")
        settingsItem.target = self
        menu.addItem(settingsItem)
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Quit", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        menuOpen = true
        gear.alphaValue = 1
        menu.popUp(positioning: nil, at: NSPoint(x: gear.frame.maxX, y: gear.frame.minY - 4), in: content)
    }

    @objc private func openSettings() {
        let sc = SettingsWindowController.shared
        sc.reapplyAppearance = { [weak self] name in self?.applyAppearance(name) }
        sc.reapplyCat = { [weak self] on in self?.applyCat(enabled: on) }
        sc.reapplyHotkeys = { [weak self] in self?.reapplyHotkeys() }
        sc.show(panelFrame: panel.frame)
    }

    /// Re-read the config hotkeys and re-register them.
    private func reapplyHotkeys() {
        guard let s = Core.settings() else { return }
        if !HotKeys.register(s.hotkey, id: 1, { [weak self] in self?.toggle() }) {
            NSLog("Launcher: couldn't register hotkey %@", s.hotkey)
        }
        if !s.clipboardHotkey.isEmpty {
            if !HotKeys.register(s.clipboardHotkey, id: 2, { [weak self] in self?.toggle(prefill: "clip ") }) {
                NSLog("Launcher: couldn't register clipboard hotkey %@", s.clipboardHotkey)
            }
        }
    }

    func applyAppearance(_ name: String) {
        // App-wide so the settings window follows the panel.
        NSApp.appearance = name == "light" ? NSAppearance(named: .aqua)
            : name == "dark" ? NSAppearance(named: .darkAqua) : nil
    }

    func menuDidClose(_ menu: NSMenu) {
        menuOpen = false
        gear.alphaValue = 0
    }

    // MARK: showing and hiding

    var isVisible: Bool { panel.isVisible }

    func toggle(prefill: String = "") {
        if isVisible && field.stringValue == prefill { hide() } else { show(prefill: prefill) }
    }

    /// Put the cat window on the panel's roof, centered at 4/5 of the width.
    private func positionCat() {
        let f = panel.frame
        catWindow.setFrameOrigin(NSPoint(x: f.minX + width * 0.8 - 32, y: f.maxY - 5))
    }

    /// Show or hide the cat (settings toggle).
    func applyCat(enabled: Bool) {
        if enabled {
            positionCat()
            if catWindow.parent == nil { panel.addChildWindow(catWindow, ordered: .above) }
        } else if catWindow.parent != nil {
            panel.removeChildWindow(catWindow)
            catWindow.orderOut(nil)
        }
    }

    func show(prefill: String = "") {
        DispatchQueue.global(qos: .utility).async { Core.refresh() }
        let s = Core.settings()  // one FFI round trip feeds everything below
        applyAppearance(s?.appearance ?? "system")
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
        // Only auto-position on the first show; afterwards keep wherever the
        // user dragged the panel to.
        if !hasPositioned, let vf = screen?.visibleFrame {
            hasPositioned = true
            let top = vf.maxY - vf.height * 0.2
            panel.setFrame(NSRect(x: vf.midX - width / 2, y: top - barHeight, width: width, height: barHeight),
                           display: false)
        }
        setQuery(prefill)
        ensureOnScreen()
        applyCat(enabled: s?.cat ?? true)
        positionCat()
        panel.makeKeyAndOrderFront(nil)
        panel.makeFirstResponder(field)
        moveCursorToEnd()
    }

    /// Debug aid (`--snapshot <png> <query>`): render the window to a PNG.
    /// Renders off-screen and never takes keyboard focus.
    func snapshot(query: String, to path: String, after delay: Double, done: @escaping () -> Void) {
        snapshotting = true
        panel.setFrame(NSRect(x: -20000, y: -20000, width: width, height: barHeight), display: false)
        setQuery(query)
        panel.orderBack(nil)
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
            guard let v = self.panel.contentView, let rep = v.bitmapImageRepForCachingDisplay(in: v.bounds) else {
                return done()
            }
            v.cacheDisplay(in: v.bounds, to: rep)
            let img = NSImage(size: v.bounds.size, flipped: false) { r in
                NSColor.windowBackgroundColor.setFill()
                r.fill()
                rep.draw(in: r)
                return true
            }
            if let tiff = img.tiffRepresentation, let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) {
                try? png.write(to: URL(fileURLWithPath: path))
            }
            done()
        }
    }

    func hide() {
        guard panel.isVisible else { return }
        generation += 1
        slowWork?.cancel()
        if lastWasSlow { Core.cancelSlow() }
        panel.orderOut(nil)
    }

    private var snapshotting = false

    func windowDidResignKey(_ notification: Notification) {
        // Stay visible while the settings window has focus — but hide when
        // the whole app is backgrounded, settings window or not.
        if NSApp.isActive, SettingsWindowController.shared.window?.isVisible == true { return }
        if !snapshotting { hide() }
    }

    private func resize() {
        let rows = results.visibleCount
        let height = barHeight + (rows > 0 ? 6 + CGFloat(rows) * ResultsView.rowHeight + 8 : 0)
        var f = panel.frame
        let top = f.maxY
        f.size.height = height
        f.origin.y = top - height
        separator.isHidden = rows == 0
        results.frame = NSRect(x: 0, y: barHeight + 6, width: width, height: CGFloat(rows) * ResultsView.rowHeight)
        panel.setFrame(f, display: true)
        positionCat()
    }

    // MARK: querying

    func controlTextDidChange(_ obj: Notification) { run(field.stringValue) }

    private func setQuery(_ text: String) {
        field.stringValue = text
        moveCursorToEnd()
        run(text)
    }

    private func moveCursorToEnd() {
        field.currentEditor()?.selectedRange = NSRange(location: (field.stringValue as NSString).length, length: 0)
    }

    private func run(_ query: String) {
        generation += 1
        let gen = generation
        slowWork?.cancel()
        let r = Core.query(query)
        if r.slow {
            // Keep what's showing until the slow results land, to avoid flicker.
            if !r.items.isEmpty { results.items = r.items }
            let work = DispatchWorkItem { [weak self] in
                let items = Core.querySlow(query)
                DispatchQueue.main.async {
                    guard let self, gen == self.generation else { return }
                    self.results.items = items
                    self.resize()
                }
            }
            slowWork = work
            DispatchQueue.global(qos: .userInitiated).asyncAfter(deadline: .now() + 0.04, execute: work)
        } else {
            if lastWasSlow { Core.cancelSlow() }
            results.items = r.items
        }
        lastWasSlow = r.slow
        resize()
    }

    // MARK: keys and activation

    func control(_ control: NSControl, textView: NSTextView, doCommandBy sel: Selector) -> Bool {
        switch sel {
        case #selector(NSResponder.moveDown(_:)):
            results.move(1)
        case #selector(NSResponder.moveUp(_:)):
            results.move(-1)
        case #selector(NSResponder.insertNewline(_:)):
            activateSelected(alt: NSEvent.modifierFlags.contains(.command))
        case #selector(NSResponder.insertTab(_:)):
            if let item = results.items[safe: results.selected], item.action.type == "complete" {
                setQuery(item.action.text ?? "")
            }
        case #selector(NSResponder.cancelOperation(_:)):
            if field.stringValue.isEmpty { hide() } else { setQuery("") }
        default:
            return false
        }
        return true
    }

    func activateSelected(alt: Bool) { activate(results.selected, alt: alt) }

    func activateVisible(_ n: Int) {
        if let i = results.visibleIndex(n) { activate(i, alt: false) }
    }

    func activate(_ index: Int, alt: Bool) {
        guard let item = results.items[safe: index] else { return }
        let action = alt ? (item.altAction ?? item.action) : item.action
        switch action.type {
        case "none":
            return
        case "complete":
            setQuery(action.text ?? "")
            return
        default:
            break
        }
        let query = field.stringValue
        DispatchQueue.global(qos: .utility).async { Core.picked(query: query, uid: item.uid) }
        hide()
        Actions.perform(action)
    }
}

extension Array {
    subscript(safe i: Int) -> Element? { indices.contains(i) ? self[i] : nil }
}
