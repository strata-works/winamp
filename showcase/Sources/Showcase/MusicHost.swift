import Foundation

struct Track {
    let title: String
    let artist: String
    let url: URL
    let duration: TimeInterval
}

/// Playback backend abstraction so MusicHost logic is testable without real audio.
protocol AudioPlayer: AnyObject {
    var isPlaying: Bool { get }
    var volume: Float { get set }
    var currentTime: TimeInterval { get set }
    var duration: TimeInterval { get }
    func load(_ url: URL, duration: TimeInterval)
    func play()
    func pause()
    func stop()
}

private func fmtMMSS(_ t: TimeInterval) -> String {
    let s = Int(t.rounded(.down))
    return "\(s / 60):" + String(format: "%02d", s % 60)
}

/// Swift-owned music host — the single source of truth exposed to the engine over the vtable.
/// Survives skin swaps (the engine never owns this state).
final class MusicHost {
    private let playlist: [Track]
    private let player: AudioPlayer
    private(set) var current: Int = 0
    private(set) var playing: Bool = false
    private(set) var volume: Double = 0.8

    init(playlist: [Track], player: AudioPlayer) {
        self.playlist = playlist
        self.player = player
        self.player.volume = Float(volume)
        if let t = playlist.first { self.player.load(t.url, duration: t.duration) }
    }

    // MARK: actions
    private func loadCurrent(autoplay: Bool) {
        guard playlist.indices.contains(current) else { return }
        let t = playlist[current]
        player.load(t.url, duration: t.duration)
        if autoplay { player.play(); playing = true }
    }
    func togglePlay() {
        if playing { player.pause(); playing = false }
        else { player.play(); playing = true }
    }
    func stop() { player.stop(); playing = false }
    func next() { if current + 1 < playlist.count { current += 1; loadCurrent(autoplay: playing) } }
    func prev() { if current > 0 { current -= 1; loadCurrent(autoplay: playing) } }
    func play(index: Int) {
        guard playlist.indices.contains(index) else { return }
        current = index; loadCurrent(autoplay: true)
    }
    func seek(_ f: Double) {
        let frac = min(max(f, 0), 1)
        player.currentTime = frac * player.duration
    }
    func setVolume(_ f: Double) {
        volume = min(max(f, 0), 1)
        player.volume = Float(volume)
    }

    // MARK: readers
    func positionFraction() -> Double {
        let d = player.duration
        return d > 0 ? min(max(player.currentTime / d, 0), 1) : 0
    }
    func timeString() -> String { "\(fmtMMSS(player.currentTime)) / \(fmtMMSS(player.duration))" }
    func viz(_ i: Int) -> Double {
        guard playing else { return 0.05 }
        let t = player.currentTime
        let fi = Double(i)
        let base = (1 - fi / 16) * 0.45 + 0.18
        let wobble = 0.55 * sin(t * (4 + fi * 0.6) + fi) + 0.30 * sin(t * (9 + fi * 0.27))
        return min(max(base + wobble * 0.4, 0.05), 1)
    }

    // MARK: collection
    func rowCount() -> Int { playlist.count }
    func rowString(_ i: Int, field: String) -> String? {
        guard playlist.indices.contains(i) else { return nil }
        let t = playlist[i]
        switch field {
        case "now": return i == current ? "▶" : ""
        case "title": return t.title
        case "artist": return t.artist
        case "duration": return fmtMMSS(t.duration)
        default: return nil
        }
    }

    // MARK: state keys
    func str(_ key: String) -> String? {
        switch key {
        case "track_title": return playlist.indices.contains(current) ? playlist[current].title : ""
        case "artist": return playlist.indices.contains(current) ? playlist[current].artist : ""
        case "time": return timeString()
        default: return nil
        }
    }
    func num(_ key: String) -> Double? {
        switch key {
        case "position": return positionFraction()
        case "volume": return volume
        case "playing": return playing ? 1 : 0
        case "current_index": return Double(current)
        default:
            if key.hasPrefix("viz_"), let i = Int(key.dropFirst(4)) { return viz(i) }
            return nil
        }
    }
}
