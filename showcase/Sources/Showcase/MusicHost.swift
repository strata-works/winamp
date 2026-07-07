import Foundation
import os

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

/// Single-line ellipsis truncation. The engine's `text{}` primitive wraps (no ellipsis) and its
/// list-row cells can't be width-bounded, so the host caps the dynamic strings it hands out before
/// the engine draws them. Without this, imported tracks with long titles run off the panel and over
/// the clock/duration. Proper skin-aware truncation is a planned engine feature.
private func ellipsize(_ s: String, _ max: Int) -> String {
    guard s.count > max else { return s }
    return s.prefix(max - 1).trimmingCharacters(in: .whitespaces) + "…"
}

/// Character caps for the dynamic strings, sized to the narrowest skin that shows each field.
/// Wider skins simply leave slack. Tuned against the three showcase skins.
private enum TitleCap {
    static let nowPlaying = 26  // LCD "now playing" title (Faceplate size-22 / Studio / Cassette)
    static let artist = 24      // LCD artist line (narrowest: Faceplate LCD)
    static let row = 20         // playlist / library row title (narrowest: Studio Library, w=168)
}

/// Swift-owned music host — the single source of truth exposed to the engine over the vtable.
/// Survives skin swaps (the engine never owns this state).
final class MusicHost {
    private var playlist: [Track]
    /// Guards every access to the `playlist` array. The engine's render thread reads the playlist
    /// via the row/str vtable callbacks every frame, while `addTracks` appends on the main actor;
    /// without this lock an append could reallocate the array's buffer mid-read. Non-recursive —
    /// never hold it across a call that also locks (see `loadCurrent` callers).
    private let playlistLock = OSAllocatedUnfairLock()
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
        guard let t = playlistLock.withLock({ playlist.indices.contains(current) ? playlist[current] : nil }) else { return }
        player.load(t.url, duration: t.duration)
        if autoplay { player.play(); playing = true }
    }
    func togglePlay() {
        if playing { player.pause(); playing = false }
        else { player.play(); playing = true }
    }
    func stop() { player.stop(); playing = false }
    func next() {
        let count = playlistLock.withLock { playlist.count }
        if current + 1 < count { current += 1; loadCurrent(autoplay: playing) }
    }
    func prev() { if current > 0 { current -= 1; loadCurrent(autoplay: playing) } }
    func play(index: Int) {
        let valid = playlistLock.withLock { playlist.indices.contains(index) }
        guard valid else { return }
        current = index; loadCurrent(autoplay: true)
    }
    /// Append imported tracks to the end of the playlist. Current selection, playback state,
    /// and volume are untouched; the engine picks up the new rows on its next pull of rowCount().
    func addTracks(_ tracks: [Track]) {
        guard !tracks.isEmpty else { return }
        playlistLock.withLock { playlist.append(contentsOf: tracks) }
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
    func rowCount() -> Int { playlistLock.withLock { playlist.count } }
    func rowString(_ i: Int, field: String) -> String? {
        guard let t = playlistLock.withLock({ playlist.indices.contains(i) ? playlist[i] : nil }) else { return nil }
        switch field {
        case "now": return i == current ? "▶" : ""
        case "title": return ellipsize(t.title, TitleCap.row)
        case "artist": return ellipsize(t.artist, TitleCap.row)
        case "duration": return fmtMMSS(t.duration)
        default: return nil
        }
    }

    // MARK: state keys
    func str(_ key: String) -> String? {
        switch key {
        case "track_title":
            let title = playlistLock.withLock { playlist.indices.contains(current) ? playlist[current].title : "" }
            return ellipsize(title, TitleCap.nowPlaying)
        case "artist":
            let artist = playlistLock.withLock { playlist.indices.contains(current) ? playlist[current].artist : "" }
            return ellipsize(artist, TitleCap.artist)
        case "time": return timeString()
        case "clock": return fmtMMSS(player.currentTime)  // elapsed-only, for the DSEG7 counter
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
