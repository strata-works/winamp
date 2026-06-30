import SwiftUI
import WidgetKit

struct ContentView: View {
    @State private var mode: RenderMode?
    @State private var image: UIImage?
    @State private var probeLog: String = ""

    var body: some View {
        VStack(spacing: 16) {
            switch mode {
            case .none:    Text("Rendering…")
            case .failed:  Text("Render failed ❌")
            case .some(let m): Text("Now Playing rendered ✅\n\(m.rawValue)")
                                .multilineTextAlignment(.center)
            }
            if let image {
                Image(uiImage: image).resizable().scaledToFit()
                    .frame(maxWidth: 320)
            }
            Text("Live data rendered through a carapace skin")
                .font(.caption2).foregroundStyle(.secondary)
            // Last breadcrumb the widget EXTENSION wrote during its in-extension render probe.
            // Reflects the previous widget-timeline reload (this launch triggers the next one).
            Text("widget-ext probe: \(probeLog.isEmpty ? "— (extension hasn't run yet)" : probeLog)")
                .font(.caption2.monospaced()).foregroundStyle(.orange)
                .multilineTextAlignment(.center)
        }
        .padding()
        .onAppear {
            mode = CarapaceBridge.render()
            image = UIImage(contentsOfFile: AppGroup.renderURL.path)
            probeLog = (try? String(contentsOf: AppGroup.probeLogURL, encoding: .utf8)) ?? ""
            WidgetCenter.shared.reloadAllTimelines()
        }
    }
}
