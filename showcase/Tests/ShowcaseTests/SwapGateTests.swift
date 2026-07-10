import Testing
@testable import Showcase

// SwapGate serializes skin swaps so rapid Tab presses can't overlap the crossfade/resize
// animation. A tap that arrives mid-swap is coalesced: the index still advances, and exactly
// ONE follow-up swap (to the latest target) runs when the in-flight swap completes.

@Test func lone_swap_starts_immediately_and_settles() {
    var g = SwapGate()
    #expect(g.requestSwap() == true)      // free gate → start the swap now
    #expect(g.isSwapping == true)
    #expect(g.swapCompleted() == false)   // nothing queued → no follow-up
    #expect(g.isSwapping == false)        // settled, ready for the next
}

@Test func taps_during_swap_coalesce_into_one_followup() {
    var g = SwapGate()
    #expect(g.requestSwap() == true)      // first tap starts a swap
    #expect(g.requestSwap() == false)     // 2nd tap mid-swap → queued, no new swap
    #expect(g.requestSwap() == false)     // 3rd tap → still just queued (coalesced)
    #expect(g.isSwapping == true)
    #expect(g.swapCompleted() == true)    // on completion → run exactly ONE follow-up (to latest)
    #expect(g.isSwapping == true)         // the follow-up is now the in-flight swap
    #expect(g.swapCompleted() == false)   // follow-up done, nothing more queued
    #expect(g.isSwapping == false)
}

@Test func gate_is_reusable_after_settling() {
    var g = SwapGate()
    _ = g.requestSwap()
    _ = g.swapCompleted()                 // one full cycle
    #expect(g.requestSwap() == true)      // ready for the next lone swap
    #expect(g.swapCompleted() == false)
    #expect(g.isSwapping == false)
}

@Test func completion_without_a_start_is_a_noop() {
    // Defensive: a stray completion on a fresh/settled gate must not get stuck "swapping".
    var g = SwapGate()
    #expect(g.swapCompleted() == false)
    #expect(g.isSwapping == false)
}
