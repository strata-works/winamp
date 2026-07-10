// Fragment stage only — the engine supplies `vs`/`VsOut` and generates `struct U`.
// Animated interference-wave field: motion comes from `u.time`, overall brightness from the
// host-bound `u.intensity` uniform, and the color ramp from the baked-literal `u.hue`.
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.time;

    // Three drifting sine fronts → a plasma-ish interference field in -1..1.
    let w1 = sin((uv.x + uv.y) * 10.0 + t * 2.0);
    let w2 = sin((uv.x - uv.y) * 8.0 - t * 1.5);
    let w3 = sin(uv.x * 6.0 + t * 1.1);
    let field = (w1 + w2 + w3) / 3.0;
    let v = 0.5 + 0.5 * field; // 0..1

    // Color ramp keyed off the baked `hue` literal + the field value.
    let phase = u.hue * 6.2831 + v * 3.1416;
    let col = vec3<f32>(
        0.5 + 0.5 * sin(phase),
        0.5 + 0.5 * sin(phase + 2.094),
        0.5 + 0.5 * sin(phase + 4.188),
    );

    // Host-bound intensity scales the whole thing (proves per-frame reactivity).
    return vec4<f32>(col * u.intensity, 1.0);
}
