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
            case .some(let m): Text("Faceplate ready ✅\n\(m.rawValue)")
                                .multilineTextAlignment(.center)
            }
            // Float the shaped faceplate over a gradient to show its transparency.
            LinearGradient(colors: [.indigo, .purple], startPoint: .top, endPoint: .bottom)
                .frame(width: 240, height: 276)
                .overlay {
                    if let image {
                        Image(uiImage: image).resizable().scaledToFit().padding(8)
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 16))
            Text("App Group: \(AppGroup.id)")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .padding()
        .onAppear {
            mode = CarapaceBridge.renderFaceplate()
            image = UIImage(contentsOfFile: AppGroup.faceplateURL.path)
            WidgetCenter.shared.reloadAllTimelines()
        }
    }
}
