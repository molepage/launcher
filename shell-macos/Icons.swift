// Icon cache: file icons from the system, SF Symbols, and Quick Look
// thumbnails for screenshots (loaded in the background).

import AppKit
import QuickLookThumbnailing

final class Icons {
    static let shared = Icons()
    var onUpdate: (() -> Void)?

    private let cache = NSCache<NSString, NSImage>()
    private var pending = Set<String>()
    private let size = NSSize(width: 32, height: 32)

    init() { cache.countLimit = 500 }

    func image(for icon: Icon?) -> NSImage? {
        guard let icon else { return nil }
        let key = "\(icon.kind):\(icon.value)" as NSString
        if let img = cache.object(forKey: key) { return img }
        switch icon.kind {
        case "file":
            let img = NSWorkspace.shared.icon(forFile: icon.value)
            img.size = size
            cache.setObject(img, forKey: key)
            return img
        case "symbol":
            let cfg = NSImage.SymbolConfiguration(pointSize: 17, weight: .regular)
            let img = NSImage(systemSymbolName: icon.value, accessibilityDescription: nil)?.withSymbolConfiguration(cfg)
            if let img { cache.setObject(img, forKey: key) }
            return img
        case "thumb":
            loadThumbnail(icon.value, key: key)
            let placeholder = NSWorkspace.shared.icon(forFile: icon.value)
            placeholder.size = size
            return placeholder
        default:
            return nil
        }
    }

    private func loadThumbnail(_ path: String, key: NSString) {
        guard pending.insert(path).inserted else { return }
        let scale = NSScreen.main?.backingScaleFactor ?? 2
        let req = QLThumbnailGenerator.Request(fileAt: URL(fileURLWithPath: path), size: CGSize(width: 64, height: 64),
                                               scale: scale, representationTypes: .thumbnail)
        QLThumbnailGenerator.shared.generateBestRepresentation(for: req) { rep, _ in
            DispatchQueue.main.async {
                self.pending.remove(path)
                if let rep {
                    self.cache.setObject(rep.nsImage, forKey: key)
                    self.onUpdate?()
                }
            }
        }
    }
}
