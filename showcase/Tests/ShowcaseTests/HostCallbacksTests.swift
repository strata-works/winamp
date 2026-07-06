import Testing
import Foundation
@testable import Showcase

private func withHost(_ body: (MusicHost) -> Void) {
    let h = MusicHost(playlist: [
        Track(title: "One", artist: "Alpha", url: URL(fileURLWithPath: "/tmp/1.wav"), duration: 100),
        Track(title: "Two", artist: "Beta",  url: URL(fileURLWithPath: "/tmp/2.wav"), duration: 200),
    ], player: FakeAudioPlayer())
    hostBox.host = h
    defer { hostBox.host = nil }
    body(h)
}

// Serialized: these tests all drive the same shared global `hostBox`, so Swift Testing's
// default parallel execution across @Test functions races on it. Grouping them into one
// `.serialized` suite forces sequential execution without changing any test body/assertion.
@Suite(.serialized)
struct HostCallbacksTests {

    @Test func get_num_reads_volume() {
        withHost { h in
            h.setVolume(0.5)
            var out = 0.0
            let ok = "volume".withCString { hostGetNum(nil, $0, &out) }
            #expect(ok); #expect(out == 0.5)
            let miss = "nope".withCString { hostGetNum(nil, $0, &out) }
            #expect(!miss)
        }
    }

    @Test func get_str_writes_title_nul_terminated() {
        withHost { _ in
            var buf = [CChar](repeating: 0, count: 64)
            let ok = "track_title".withCString { hostGetStr(nil, $0, &buf, UInt(buf.count)) }
            #expect(ok)
            #expect(String(cString: buf) == "One")
        }
    }

    @Test func rows_via_callbacks() {
        withHost { h in
            h.play(index: 1)
            #expect("playlist".withCString { hostRowCount(nil, $0) } == 2)
            var buf = [CChar](repeating: 0, count: 32)
            let ok = "playlist".withCString { col in "now".withCString { f in
                hostGetRowStr(nil, col, 1, f, &buf, UInt(buf.count)) } }
            #expect(ok); #expect(String(cString: buf) == "▶")
        }
    }

    @Test func invoke_and_invoke_arg_route_to_host() {
        withHost { h in
            "toggle_play".withCString { hostInvoke(nil, $0) }
            #expect(h.playing == true)
            "set_volume".withCString { hostInvokeArg(nil, $0, 0.25) }
            #expect(h.volume == 0.25)
            "play_index".withCString { hostInvokeArg(nil, $0, 1) }
            #expect(h.current == 1)
        }
    }
}
