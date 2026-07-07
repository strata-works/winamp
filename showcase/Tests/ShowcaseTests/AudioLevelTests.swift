import Testing
import Foundation
@testable import Showcase

@Test func normalize_db_maps_range_and_clamps() {
    #expect(normalizeDB(0) == 1.0)
    #expect(normalizeDB(-60) == 0.0)
    #expect(abs(normalizeDB(-30) - 0.5) < 1e-6)
    #expect(normalizeDB(10) == 1.0)     // clamps above 0 dB
    #expect(normalizeDB(-120) == 0.0)   // clamps below floor
}

@Test func fake_player_level_defaults_to_zero() {
    // FakeAudioPlayer (test double) must satisfy the new protocol requirement.
    let p = FakeAudioPlayer()
    #expect(p.level == 0.0)
}
