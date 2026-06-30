import WidgetKit
import SwiftUI
import os

private let log = Logger(subsystem: "com.carapace.spike", category: "provider")

struct CarapaceEntry: TimelineEntry {
    let date: Date
    let image: UIImage?
    var note: String = ""
}

struct Provider: TimelineProvider {
    func placeholder(in context: Context) -> CarapaceEntry {
        CarapaceEntry(date: Date(), image: nil)
    }
    func getSnapshot(in context: Context, completion: @escaping (CarapaceEntry) -> Void) {
        completion(entry())
    }
    func getTimeline(in context: Context, completion: @escaping (Timeline<CarapaceEntry>) -> Void) {
        completion(Timeline(entries: [entry()], policy: .never))
    }
    private func entry() -> CarapaceEntry {
        #if WIDGET_RENDER_PROBE
        // Task 6 stretch: render the skin LIVE *inside the widget extension process* (not the app).
        // Enable with `DEVICE=1 DEV_TEAM=… WIDGET_RENDER_PROBE=1 ruby project.rb` (links carapace
        // into the widget target). Uses DISTINCT data from the app so the tile visibly proves which
        // process rendered it; the verdict is overlaid as text so a single screenshot is conclusive.
        let probe = probeInExtensionRender()
        if let extImg = probe.image {
            return CarapaceEntry(date: Date(), image: extImg, note: probe.note)
        }
        // Render failed (no crash) — show the app PNG plus the verdict text so we can see why.
        let appImg = UIImage(contentsOfFile: AppGroup.renderURL.path)
        return CarapaceEntry(date: Date(), image: appImg, note: probe.note)
        #else
        let path = AppGroup.renderURL.path
        let img = UIImage(contentsOfFile: path)
        log.log("Provider.entry: render=\(path) loaded=\(img != nil)")
        return CarapaceEntry(date: Date(), image: img)
        #endif
    }

    #if WIDGET_RENDER_PROBE
    private func probeInExtensionRender() -> (image: UIImage?, note: String) {
        // Overwrite a breadcrumb at each stage. If the extension is jetsam-killed mid-render, the
        // LAST surviving breadcrumb shows exactly where it died; the app reads this back.
        func crumb(_ s: String) { try? s.write(to: AppGroup.probeLogURL, atomically: true, encoding: .utf8) }
        func availMB() -> Int { Int(os_proc_available_memory() / 1_048_576) }
        crumb("1-enter avail=\(availMB())MB")
        guard let skin = Bundle.main.url(forResource: "skin-nowplaying", withExtension: nil)?.path
        else { crumb("X-skin-missing"); return (nil, "ext: skin missing") }
        // Deliberately different from the app's data, so the tile proves the EXTENSION rendered it.
        let info = ["track": "IN-EXTENSION", "artist": "widget process render",
                    "time": "EXT / PROC", "position": "0.85"]
        let keys = Array(info.keys), vals = keys.map { info[$0]! }
        let out = NSTemporaryDirectory() + "ext-render.png"
        let kc = keys.map { strdup($0) }, vc = vals.map { strdup($0) }
        defer { kc.forEach { free($0) }; vc.forEach { free($0) } }
        let kp = kc.map { UnsafePointer($0) }, vp = vc.map { UnsafePointer($0) }
        crumb("2-pre-render avail=\(availMB())MB")
        let ok = kp.withUnsafeBufferPointer { kb in
            vp.withUnsafeBufferPointer { vb in
                carapace_render_info(skin, 640, 280, UInt32(keys.count),
                                     kb.baseAddress!, vb.baseAddress!, out)
            }
        }
        crumb("3-post-render ok=\(ok) avail=\(availMB())MB")
        let note = "ext render_info ok=\(ok)"
        log.log("Provider PROBE: \(note)")
        return (ok ? UIImage(contentsOfFile: out) : nil, note)
    }
    #endif
}

struct CarapaceWidgetView: View {
    var entry: CarapaceEntry
    var body: some View {
        // The skin fills the ENTIRE widget: rendered edge-to-edge by carapace and used as the
        // container background with scaledToFill, so it covers the whole tile (the system applies
        // the rounded-rect mask). No black margins.
        Color.clear.containerBackground(for: .widget) {
            ZStack(alignment: .bottom) {
                if let img = entry.image {
                    Image(uiImage: img).resizable().scaledToFill()
                } else {
                    Color.black
                }
                if !entry.note.isEmpty {
                    Text(entry.note)
                        .font(.system(size: 9, weight: .semibold, design: .monospaced))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(.black.opacity(0.6))
                        .padding(.bottom, 2)
                }
            }
        }
    }
}

@main
struct CarapaceWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "CarapaceWidget", provider: Provider()) { entry in
            CarapaceWidgetView(entry: entry)
        }
        .configurationDisplayName("Carapace")
        .description("A carapace skin rendering live data in a widget.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}
