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

@Test func add_tracks_appends_and_preserves_state() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    h.play(index: 1)
    h.setVolume(0.5)
    h.addTracks([Track(title: "Four", artist: "Delta",
                       url: URL(fileURLWithPath: "/tmp/4.wav"), duration: 44)])
    #expect(h.rowCount() == 4)
    #expect(h.rowString(3, field: "title") == "Four")
    #expect(h.current == 1)      // selection unchanged
    #expect(h.playing == true)   // playback unchanged
    #expect(h.volume == 0.5)     // volume unchanged
}

@Test func add_tracks_empty_is_noop() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    h.addTracks([])
    #expect(h.rowCount() == 3)
}

@Test func clock_key_is_elapsed_only() {
    let fake = FakeAudioPlayer()
    let h = MusicHost(playlist: demoPlaylist(), player: fake)
    fake.currentTime = 63
    #expect(h.str("clock") == "1:03")
    #expect(h.str("time")?.contains("/") == true)  // full "time" key still elapsed/total
}

@Test func long_title_is_ellipsized_for_lcd_and_rows() {
    let long = "Ameno Amapiano (You Wanna Bamba) (David Guetta Remix)"
    let t = Track(title: long, artist: "A Very Long Artist Name That Overflows Too",
                  url: URL(fileURLWithPath: "/tmp/a.mp3"), duration: 60)
    let h = MusicHost(playlist: [t], player: FakeAudioPlayer())
    let lcdTitle = h.str("track_title")!
    #expect(lcdTitle.count <= 24)
    #expect(lcdTitle.hasSuffix("…"))
    let lcdArtist = h.str("artist")!
    #expect(lcdArtist.count <= 24)
    #expect(lcdArtist.hasSuffix("…"))
    let rowTitle = h.rowString(0, field: "title")!
    #expect(rowTitle.count <= 20)
    #expect(rowTitle.hasSuffix("…"))
    // The ellipsized string is a prefix of the original (plus the ellipsis).
    #expect(long.hasPrefix(String(lcdTitle.dropLast())))
}

@Test func short_title_is_not_truncated() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    #expect(h.str("track_title") == "One")           // unchanged: no ellipsis
    #expect(h.str("artist") == "Alpha")
    #expect(h.rowString(2, field: "title") == "Three")
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
