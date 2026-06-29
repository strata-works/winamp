import WidgetKit
import SwiftUI
import os

private let log = Logger(subsystem: "com.carapace.spike", category: "provider")

struct CarapaceEntry: TimelineEntry {
    let date: Date
    let state: Int
    let image: UIImage?
}

struct Provider: TimelineProvider {
    func placeholder(in context: Context) -> CarapaceEntry {
        CarapaceEntry(date: Date(), state: 0, image: nil)
    }
    func getSnapshot(in context: Context, completion: @escaping (CarapaceEntry) -> Void) {
        completion(entry())
    }
    func getTimeline(in context: Context, completion: @escaping (Timeline<CarapaceEntry>) -> Void) {
        completion(Timeline(entries: [entry()], policy: .never))
    }
    private func entry() -> CarapaceEntry {
        let s = AppGroup.currentState()
        #if WIDGET_RENDER_PROBE
        // Task 6 stretch: try a live render INSIDE the extension. Enable with
        // `WIDGET_RENDER_PROBE=1 ruby project.rb` (links carapace into the widget target).
        // Finding: in the Simulator this returns false (Vello needs INDIRECT_EXECUTION),
        // and the extension survives gracefully — it does not crash or get jetsam-killed.
        if let skin = Bundle.main.url(forResource: "skin", withExtension: nil)?.path {
            let tmp = NSTemporaryDirectory() + "ext-render.png"
            let level = Double(s) / Double(AppGroup.stateCount - 1)
            let ok = carapace_render_png(skin, 240, 80, level, tmp)
            log.log("Provider PROBE: in-extension render ok=\(ok)")
            if ok, let img = UIImage(contentsOfFile: tmp) {
                return CarapaceEntry(date: Date(), state: s, image: img)
            }
        }
        #endif
        // Load the app-rendered PNG from the App Group.
        let path = AppGroup.pngURL(state: s).path
        let img = UIImage(contentsOfFile: path)
        log.log("Provider.entry: state=\(s) png=\(path) loaded=\(img != nil)")
        return CarapaceEntry(date: Date(), state: s, image: img)
    }
}

struct CarapaceWidgetView: View {
    var entry: CarapaceEntry
    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            if let img = entry.image {
                Image(uiImage: img).resizable().scaledToFit()
            } else {
                Text("no render")
            }
            Button(intent: BumpIntent()) {
                Text("state \(entry.state) ▸")
                    .font(.caption2).padding(4)
            }
            .buttonStyle(.plain)
            .background(.white.opacity(0.15))
        }
        .containerBackground(.black, for: .widget)
    }
}

@main
struct CarapaceWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "CarapaceWidget", provider: Provider()) { entry in
            CarapaceWidgetView(entry: entry)
        }
        .configurationDisplayName("Carapace")
        .description("A carapace skin rendered to a widget.")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}
