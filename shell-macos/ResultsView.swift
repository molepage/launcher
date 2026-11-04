// The result list: one view that draws every visible row itself instead of
// a table with a view per cell.

import AppKit

final class ResultsView: NSView {
    static let rowHeight: CGFloat = 46
    static let maxVisible = 8

    var items: [Item] = [] {
        didSet {
            selected = 0
            offset = 0
            needsDisplay = true
        }
    }
    var selected = 0 {
        didSet {
            if selected < offset { offset = selected }
            if selected >= offset + Self.maxVisible { offset = selected - Self.maxVisible + 1 }
            needsDisplay = true
        }
    }
    private(set) var offset = 0
    private var scrollAccum: CGFloat = 0
    var onActivate: ((Int) -> Void)?

    override var isFlipped: Bool { true }
    var visibleCount: Int { min(items.count, Self.maxVisible) }

    func move(_ delta: Int) {
        guard !items.isEmpty else { return }
        selected = (selected + delta + items.count) % items.count
    }

    /// Index of the n-th visible row (for ⌘1…⌘9).
    func visibleIndex(_ n: Int) -> Int? {
        let i = offset + n
        return n < visibleCount && i < items.count ? i : nil
    }

    private static let titleFont = NSFont.systemFont(ofSize: 14.5, weight: .medium)
    private static let subFont = NSFont.systemFont(ofSize: 12)
    private static let hintFont = NSFont.systemFont(ofSize: 12, weight: .medium)
    private static let para: NSParagraphStyle = {
        let p = NSMutableParagraphStyle()
        p.lineBreakMode = .byTruncatingTail
        return p
    }()

    override func draw(_ dirty: NSRect) {
        let h = Self.rowHeight
        let titleAttrs: [NSAttributedString.Key: Any] = [
            .font: Self.titleFont, .foregroundColor: NSColor.labelColor, .paragraphStyle: Self.para,
        ]
        let subAttrs: [NSAttributedString.Key: Any] = [
            .font: Self.subFont, .foregroundColor: NSColor.secondaryLabelColor, .paragraphStyle: Self.para,
        ]
        let hintAttrs: [NSAttributedString.Key: Any] = [
            .font: Self.hintFont, .foregroundColor: NSColor.tertiaryLabelColor,
        ]
        for row in 0..<visibleCount {
            let i = offset + row
            let item = items[i]
            let r = NSRect(x: 8, y: CGFloat(row) * h, width: bounds.width - 16, height: h)
            if i == selected {
                NSColor.labelColor.withAlphaComponent(0.1).setFill()
                NSBezierPath(roundedRect: r, xRadius: 8, yRadius: 8).fill()
            }
            if let img = Icons.shared.image(for: item.icon) {
                drawIcon(img, in: NSRect(x: r.minX + 10, y: r.minY + (h - 30) / 2, width: 30, height: 30))
            }
            let textX = r.minX + 52
            let hintW: CGFloat = row < 9 ? 34 : 0
            let textW = r.maxX - textX - hintW - 8
            if item.subtitle.isEmpty {
                (item.title as NSString).draw(
                    with: NSRect(x: textX, y: r.minY + (h - 18) / 2, width: textW, height: 18),
                    options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine], attributes: titleAttrs)
            } else {
                (item.title as NSString).draw(
                    with: NSRect(x: textX, y: r.minY + 5, width: textW, height: 18),
                    options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine], attributes: titleAttrs)
                (item.subtitle as NSString).draw(
                    with: NSRect(x: textX, y: r.minY + 24, width: textW, height: 16),
                    options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine], attributes: subAttrs)
            }
            if row < 9 {
                ("⌘\(row + 1)" as NSString).draw(at: NSPoint(x: r.maxX - hintW, y: r.minY + (h - 15) / 2),
                                                  withAttributes: hintAttrs)
            }
        }
    }

    private func drawIcon(_ img: NSImage, in rect: NSRect) {
        var r = rect
        if img.size.width > 0, img.size.height > 0 {
            let s = min(rect.width / img.size.width, rect.height / img.size.height)
            let w = img.size.width * s, hh = img.size.height * s
            r = NSRect(x: rect.midX - w / 2, y: rect.midY - hh / 2, width: w, height: hh)
        }
        if img.isTemplate {
            // Symbols: shrink a little and tint with the text colour.
            let inset = r.insetBy(dx: 5, dy: 5)
            let tinted = NSImage(size: img.size, flipped: false) { b in
                img.draw(in: b)
                NSColor.labelColor.withAlphaComponent(0.75).set()
                b.fill(using: .sourceAtop)
                return true
            }
            tinted.draw(in: inset, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
        } else {
            img.draw(in: r, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
        }
    }

    override func mouseDown(with e: NSEvent) {
        let p = convert(e.locationInWindow, from: nil)
        let i = offset + Int(p.y / Self.rowHeight)
        guard i < items.count else { return }
        selected = i
        onActivate?(i)
    }

    override func scrollWheel(with e: NSEvent) {
        guard items.count > Self.maxVisible else { return }
        scrollAccum += e.scrollingDeltaY
        let step = Self.rowHeight / 2
        while abs(scrollAccum) >= step {
            let dir = scrollAccum > 0 ? -1 : 1
            scrollAccum -= CGFloat(-dir) * step
            let newOffset = max(0, min(items.count - Self.maxVisible, offset + dir))
            guard newOffset != offset else { scrollAccum = 0; break }
            offset = newOffset
            selected = max(offset, min(offset + Self.maxVisible - 1, selected))
            needsDisplay = true
        }
    }
}
