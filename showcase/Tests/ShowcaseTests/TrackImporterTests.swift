import Testing
import Foundation
@testable import Showcase

@Test func audioURLs_recurses_folders_and_filters_non_audio() throws {
    let fm = FileManager.default
    let root = fm.temporaryDirectory.appendingPathComponent("ti-\(UUID().uuidString)")
    let sub = root.appendingPathComponent("sub")
    try fm.createDirectory(at: sub, withIntermediateDirectories: true)
    let mp3 = root.appendingPathComponent("a.mp3")
    let txt = root.appendingPathComponent("notes.txt")
    let m4a = sub.appendingPathComponent("b.m4a")
    for u in [mp3, txt, m4a] { fm.createFile(atPath: u.path, contents: Data([0])) }
    defer { try? fm.removeItem(at: root) }

    let found = TrackImporter.audioURLs(from: [root]).map { $0.lastPathComponent }.sorted()
    #expect(found == ["a.mp3", "b.m4a"])
}

@Test func audioURLs_accepts_direct_files_skips_non_audio() throws {
    let fm = FileManager.default
    let dir = fm.temporaryDirectory.appendingPathComponent("ti-\(UUID().uuidString)")
    try fm.createDirectory(at: dir, withIntermediateDirectories: true)
    let wav = dir.appendingPathComponent("x.wav")
    let txt = dir.appendingPathComponent("y.txt")
    for u in [wav, txt] { fm.createFile(atPath: u.path, contents: Data([0])) }
    defer { try? fm.removeItem(at: dir) }

    let found = TrackImporter.audioURLs(from: [wav, txt]).map { $0.lastPathComponent }
    #expect(found == ["x.wav"])
}

@Test func track_falls_back_to_filename_and_unknown_artist() async throws {
    let fm = FileManager.default
    let dir = fm.temporaryDirectory.appendingPathComponent("ti-\(UUID().uuidString)")
    try fm.createDirectory(at: dir, withIntermediateDirectories: true)
    let file = dir.appendingPathComponent("My Song.mp3")
    fm.createFile(atPath: file.path, contents: Data([0]))  // not real audio → no metadata
    defer { try? fm.removeItem(at: dir) }

    let t = await TrackImporter.track(for: file)
    #expect(t.title == "My Song")
    #expect(t.artist == "Unknown Artist")
    #expect(t.duration == 0)
}
