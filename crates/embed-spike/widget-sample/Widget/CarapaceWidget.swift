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
        let path = AppGroup.faceplateURL.path
        let img = UIImage(contentsOfFile: path)
        log.log("Provider.entry: faceplate=\(path) loaded=\(img != nil)")
        return CarapaceEntry(date: Date(), image: img)
    }
}

struct CarapaceWidgetView: View {
    var entry: CarapaceEntry
    var body: some View {
        ZStack {
            if let img = entry.image {
                // The faceplate is shaped with a transparent background, so it floats over the
                // widget's container — no opaque box, just the skin.
                Image(uiImage: img).resizable().scaledToFit()
            } else {
                Text("no render").foregroundStyle(.secondary)
            }
        }
        .containerBackground(for: .widget) { Color.clear }
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
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}
