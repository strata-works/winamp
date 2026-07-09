/// Serializes skin swaps so rapid Tab presses can't overlap the crossfade + window-resize
/// animation. Each swap owns the ~250 ms transition window; a tap that arrives mid-swap doesn't
/// start a second (overlapping) swap. Instead it is *coalesced*: the caller still advances the skin
/// index, and exactly ONE follow-up swap — straight to the latest index — runs when the in-flight
/// swap completes. A lone tap starts instantly; mashing Tab lands on the final skin with one clean
/// dissolve instead of a pile-up of restarted animations.
///
/// Pure value type (no AppKit) so the coalescing logic is unit-tested independently of the GUI.
struct SwapGate {
    /// True while a swap's transition is in flight (from `requestSwap` returning true, or a
    /// coalesced follow-up, until the matching `swapCompleted`).
    private(set) var isSwapping = false
    private var pending = false

    /// Call when a swap is requested (e.g. a Tab press). Returns `true` if the caller should START a
    /// swap now; `false` if one is already in flight (the request is remembered and coalesced into a
    /// single follow-up).
    mutating func requestSwap() -> Bool {
        if isSwapping {
            pending = true
            return false
        }
        isSwapping = true
        return true
    }

    /// Call when the in-flight swap's transition finishes. Returns `true` if the caller should
    /// immediately start ONE more swap (to the latest target) to honor taps received mid-swap; the
    /// gate stays `isSwapping` for that follow-up. Returns `false` when nothing is queued and the
    /// gate settles.
    mutating func swapCompleted() -> Bool {
        if pending {
            pending = false
            return true // stay isSwapping = true: the follow-up is now the in-flight swap
        }
        isSwapping = false
        return false
    }
}
