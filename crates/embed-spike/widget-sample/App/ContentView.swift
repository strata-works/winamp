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
            case .some(let m): Text("\(AppGroup.stateCount) states ready ✅\n\(m.rawValue)")
                                .multilineTextAlignment(.center)
            }
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(width: 240, height: 80)
                    .border(.gray)
            }
            Text("App Group: \(AppGroup.id)")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .padding()
        .onAppear {
            mode = CarapaceBridge.populateStates()
            image = UIImage(contentsOfFile: AppGroup.pngURL(state: AppGroup.stateCount - 1).path)
            // Reload the widget so it picks up freshly-populated PNGs.
            WidgetCenter.shared.reloadAllTimelines()
        }
    }
}
