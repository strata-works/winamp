import Foundation
import AVFoundation

/// AVFoundation-backed AudioPlayer. Nil-safe: if a file can't load, playback is a no-op.
final class RealAudioPlayer: AudioPlayer {
    private var player: AVAudioPlayer?
    var isPlaying: Bool { player?.isPlaying ?? false }
    var volume: Float = 0.8 { didSet { player?.volume = volume } }
    var currentTime: TimeInterval {
        get { player?.currentTime ?? 0 }
        set { player?.currentTime = newValue }
    }
    var duration: TimeInterval { player?.duration ?? 0 }
    func load(_ url: URL, duration: TimeInterval) {
        player = try? AVAudioPlayer(contentsOf: url)
        player?.prepareToPlay()
        player?.volume = volume
    }
    func play() { player?.play() }
    func pause() { player?.pause() }
    func stop() { player?.stop(); player?.currentTime = 0 }
}
