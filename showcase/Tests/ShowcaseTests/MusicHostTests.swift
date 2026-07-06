import Testing
import Foundation
@testable import Showcase

private func demoPlaylist() -> [Track] {
    [
        Track(title: "One", artist: "Alpha", url: URL(fileURLWithPath: "/tmp/1.wav"), duration: 100),
        Track(title: "Two", artist: "Beta",  url: URL(fileURLWithPath: "/tmp/2.wav"), duration: 200),
        Track(title: "Three", artist: "Gamma", url: URL(fileURLWithPath: "/tmp/3.wav"), duration: 300),
    ]
}

@Test func next_prev_clamp_at_ends() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    #expect(h.current == 0)
    h.next(); #expect(h.current == 1)
    h.next(); #expect(h.current == 2)
    h.next(); #expect(h.current == 2)          // clamps at last
    h.prev(); #expect(h.current == 1)
    h.prev(); h.prev(); #expect(h.current == 0) // clamps at first
}

@Test func play_index_sets_current_and_starts() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    h.play(index: 2)
    #expect(h.current == 2)
    #expect(h.playing == true)
    #expect(h.play(index: 99) == ())          // out of range: no-op
    #expect(h.current == 2)
}

@Test func volume_and_seek_clamp_0_1() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    h.setVolume(1.5); #expect(h.volume == 1.0)
    h.setVolume(-1);  #expect(h.volume == 0.0)
    h.setVolume(0.3); #expect(abs(h.volume - 0.3) < 1e-9)
    h.seek(2.0)                                // clamps; no crash with fake player
    #expect(h.num("volume") == 0.3)
}

@Test func rows_expose_now_marker_and_fields() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    h.play(index: 1)
    #expect(h.rowCount() == 3)
    #expect(h.rowString(1, field: "now") == "▶")
    #expect(h.rowString(0, field: "now") == "")
    #expect(h.rowString(2, field: "title") == "Three")
    #expect(h.rowString(2, field: "artist") == "Gamma")
    #expect(h.rowString(1, field: "duration") == "3:20")
}

@Test func str_and_num_keys() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    #expect(h.str("track_title") == "One")
    #expect(h.str("artist") == "Alpha")
    #expect(h.num("playing") == 0.0)
    h.togglePlay(); #expect(h.num("playing") == 1.0)
    #expect(h.num("current_index") == 0.0)
    #expect(h.num("viz_0") != nil)             // some level
    #expect(h.str("nope") == nil)
    #expect(h.num("nope") == nil)
}

final class FakeAudioPlayer: AudioPlayer {
    var isPlaying = false
    var volume: Float = 1.0
    var currentTime: TimeInterval = 0
    var duration: TimeInterval = 0
    func load(_ url: URL, duration: TimeInterval) { self.duration = duration; currentTime = 0 }
    func play() { isPlaying = true }
    func pause() { isPlaying = false }
    func stop() { isPlaying = false; currentTime = 0 }
}
