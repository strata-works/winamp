import Foundation
import AVFoundation
import UniformTypeIdentifiers

/// Turns user-picked URLs (files and/or folders) into playable `Track`s.
/// URL expansion is kept separate from metadata extraction so both are unit-testable.
enum TrackImporter {
    /// Expand `inputs` to a flat, input-ordered list of audio file URLs. Directories are
    /// enumerated recursively; only files whose content type conforms to `.audio` are kept.
    static func audioURLs(from inputs: [URL]) -> [URL] {
        var out: [URL] = []
        let fm = FileManager.default
        for url in inputs {
            let isDir = (try? url.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory ?? false
            if isDir {
                guard let en = fm.enumerator(at: url,
                                             includingPropertiesForKeys: [.contentTypeKey],
                                             options: [.skipsHiddenFiles]) else { continue }
                for case let f as URL in en where isAudio(f) { out.append(f) }
            } else if isAudio(url) {
                out.append(url)
            }
        }
        return out
    }

    private static func isAudio(_ url: URL) -> Bool {
        guard let t = (try? url.resourceValues(forKeys: [.contentTypeKey]))?.contentType else {
            return false
        }
        return t.conforms(to: .audio)
    }

    /// Build a `Track` from one audio file: common metadata title/artist with fallbacks
    /// (filename, "Unknown Artist"), and duration in seconds (0 if unreadable).
    static func track(for url: URL) async -> Track {
        let asset = AVURLAsset(url: url)
        let seconds = (try? await asset.load(.duration).seconds) ?? 0
        let duration = (seconds.isFinite && seconds > 0) ? seconds : 0
        let meta = (try? await asset.load(.commonMetadata)) ?? []
        let title = await stringValue(meta, .commonIdentifierTitle)
            ?? url.deletingPathExtension().lastPathComponent
        let artist = await stringValue(meta, .commonIdentifierArtist) ?? "Unknown Artist"
        return Track(title: title, artist: artist, url: url, duration: duration)
    }

    private static func stringValue(_ items: [AVMetadataItem],
                                    _ id: AVMetadataIdentifier) async -> String? {
        let matches = AVMetadataItem.metadataItems(from: items, filteredByIdentifier: id)
        guard let first = matches.first else { return nil }
        let loaded = (try? await first.load(.stringValue)) ?? nil
        let trimmed = loaded?.trimmingCharacters(in: .whitespacesAndNewlines)
        return (trimmed?.isEmpty == false) ? trimmed : nil
    }

    /// Full pipeline: expand `inputs` to audio files, then build `Track`s in input order.
    static func importTracks(from inputs: [URL]) async -> [Track] {
        var tracks: [Track] = []
        for url in audioURLs(from: inputs) {
            tracks.append(await track(for: url))
        }
        return tracks
    }
}
