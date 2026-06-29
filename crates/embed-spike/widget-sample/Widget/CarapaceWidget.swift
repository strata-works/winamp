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
