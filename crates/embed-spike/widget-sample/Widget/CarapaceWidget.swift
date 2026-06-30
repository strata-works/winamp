import WidgetKit
import SwiftUI
import os

private let log = Logger(subsystem: "com.carapace.spike", category: "provider")

struct CarapaceEntry: TimelineEntry {
    let date: Date
    let image: UIImage?
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
        let path = AppGroup.renderURL.path
        let img = UIImage(contentsOfFile: path)
        log.log("Provider.entry: render=\(path) loaded=\(img != nil)")
        return CarapaceEntry(date: Date(), image: img)
    }
}

struct CarapaceWidgetView: View {
    var entry: CarapaceEntry
    var body: some View {
        // The skin fills the ENTIRE widget: rendered edge-to-edge by carapace and used as the
        // container background with scaledToFill, so it covers the whole tile (the system applies
        // the rounded-rect mask). No black margins.
        Color.clear.containerBackground(for: .widget) {
            if let img = entry.image {
                Image(uiImage: img).resizable().scaledToFill()
            } else {
                Color.black
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
