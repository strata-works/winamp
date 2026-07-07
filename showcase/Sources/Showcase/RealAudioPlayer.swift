import Foundation
import AVFoundation

/// Map an AVAudioPlayer average-power reading (dBFS, ~-60...0) to 0...1, clamped.
func normalizeDB(_ db: Float) -> Float {
    let floorDB: Float = -60
    return max(0, min(1, (db - floorDB) / (0 - floorDB)))
}

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
    private var smoothedLevel: Float = 0
    var level: Float {
        guard let p = player, p.isPlaying else { smoothedLevel *= 0.85; return smoothedLevel }
        p.updateMeters()
        let target = normalizeDB(p.averagePower(forChannel: 0))
        smoothedLevel += (target - smoothedLevel) * 0.35   // EMA toward target
        return smoothedLevel
    }
    func load(_ url: URL, duration: TimeInterval) {
        player = try? AVAudioPlayer(contentsOf: url)
        player?.prepareToPlay()
        player?.isMeteringEnabled = true
        player?.volume = volume
    }
    func play() { player?.play() }
    func pause() { player?.pause() }
    func stop() { player?.stop(); player?.currentTime = 0 }
}
