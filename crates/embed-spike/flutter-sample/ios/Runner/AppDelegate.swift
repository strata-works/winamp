import Flutter
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  private var carapace: CarapaceBridge?

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    // Defer ALL carapace setup well past Flutter's startup. Creating a wgpu/Metal device (in
    // carapace_create) synchronously here races Flutter's own engine/VSyncClient init and
    // intermittently segfaults it. Dart retries `textureId` until this completes.
    let registry = engineBridge.pluginRegistry
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
      self?.setUpCarapace(registry)
    }
  }

  private func setUpCarapace(_ registry: FlutterPluginRegistry) {
    guard let registrar = registry.registrar(forPlugin: "Carapace") else {
      NSLog("[carapace] no registrar"); return
    }
    let textures = registrar.textures()
    let messenger = registrar.messenger()

    guard let skinDir = Bundle.main.path(forResource: "skin-frame", ofType: nil) else {
      NSLog("[carapace] skin not in bundle"); return
    }
    // Render at 2x the 360x640 door canvas for crispness on the Retina texture.
    guard let bridge = CarapaceBridge(width: 720, height: 1280, skinDir: skinDir, registry: textures) else {
      NSLog("[carapace] bridge init failed"); return
    }
    let texId = textures.register(bridge)
    bridge.start(textureId: texId)   // safe: whole setup already deferred past Flutter startup
    carapace = bridge

    let channel = FlutterMethodChannel(name: "carapace", binaryMessenger: messenger)
    channel.setMethodCallHandler { call, result in
      switch call.method {
      case "textureId":
        result(Int(texId))
      case "setLit":
        if let a = call.arguments as? [String: Any], let v = a["v"] as? Double {
          bridge.setLit(v)   // Flutter drives the torch flame in time with the music
        }
        result(nil)
      case "tap":
        if let a = call.arguments as? [String: Any],
           let x = a["x"] as? Double, let y = a["y"] as? Double {
          bridge.pointer(x: x, y: y)   // torch toggle fires via the skin's region hit-test
        }
        result(bridge.lit)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }
}
