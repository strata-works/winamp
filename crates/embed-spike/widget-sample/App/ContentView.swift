import SwiftUI
import WidgetKit

struct ContentView: View {
    @State private var mode: RenderMode?
    @State private var image: UIImage?

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
        }
        .padding()
        .onAppear {
            mode = CarapaceBridge.render()
            image = UIImage(contentsOfFile: AppGroup.renderURL.path)
            WidgetCenter.shared.reloadAllTimelines()
        }
    }
}
