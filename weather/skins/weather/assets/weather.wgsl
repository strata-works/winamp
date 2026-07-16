// ---- Noise / warp helpers ----
fn hash21(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}
fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i + vec2<f32>(0.0, 0.0));
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0; var amp = 0.5; var q = p;
    for (var k = 0; k < 5; k = k + 1) { v = v + amp * noise2(q); q = q * 2.0; amp = amp * 0.5; }
    return v;
}
fn rot(a: f32) -> mat2x2<f32> {
    let s = sin(a); let c = cos(a);
    return mat2x2<f32>(c, -s, s, c);
}
// ---- 3D value noise (pure ALU — shader{} has no texture bindings) ----
fn hash13(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q = q + dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
}
fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u3 = f * f * (3.0 - 2.0 * f);
    let n000 = hash13(i);
    let n100 = hash13(i + vec3<f32>(1.0, 0.0, 0.0));
    let n010 = hash13(i + vec3<f32>(0.0, 1.0, 0.0));
    let n110 = hash13(i + vec3<f32>(1.0, 1.0, 0.0));
    let n001 = hash13(i + vec3<f32>(0.0, 0.0, 1.0));
    let n101 = hash13(i + vec3<f32>(1.0, 0.0, 1.0));
    let n011 = hash13(i + vec3<f32>(0.0, 1.0, 1.0));
    let n111 = hash13(i + vec3<f32>(1.0, 1.0, 1.0));
    return mix(mix(mix(n000, n100, u3.x), mix(n010, n110, u3.x), u3.y),
               mix(mix(n001, n101, u3.x), mix(n011, n111, u3.x), u3.y), u3.z);
}
fn fbm3d(p: vec3<f32>) -> f32 {
    var v = 0.0; var amp = 0.5; var q = p;
    for (var k = 0; k < 3; k = k + 1) { v = v + amp * noise3(q); q = q * 2.15; amp = amp * 0.5; }
    return v;
}
// 2-octave variant for the per-step sun tap (half the cost, shadows don't need detail).
fn fbm3d2(p: vec3<f32>) -> f32 {
    return 0.5 * noise3(p) + 0.25 * noise3(p * 2.15);
}

// ---- View camera: each pixel is a ray into a sky dome. Camera near the ground, looking
// forward+up; azimuth spreads across the window width. uv.y=0 is the TOP of the canvas. ----
fn view_ray(uv: vec2<f32>) -> vec3<f32> {
    let el = mix(0.85, 0.02, uv.y);          // radians: top of window looks well up
    let az = (uv.x - 0.5) * 0.9;
    return normalize(vec3<f32>(cos(el) * sin(az), sin(el), cos(el) * cos(az)));
}
fn sun_dir(sun: f32) -> vec3<f32> {
    let el = clamp(sun, -1.0, 1.0) * 1.1;
    let az = 0.32;                            // fixed right-of-center, like the old light_pos
    return normalize(vec3<f32>(cos(el) * sin(az), sin(el), cos(el) * cos(az)));
}

// ---- Analytic scattering sky: Rayleigh-ish gradient + mie halo + sun disc.
// The base field for every condition (replaces the mesh-gradient wallpaper). ----
fn sky_dome(rd: vec3<f32>, sd: vec3<f32>, sun: f32) -> vec3<f32> {
    let day = smoothstep(-0.12, 0.35, sun);
    let up = clamp(rd.y, 0.0, 1.0);
    // Zenith -> horizon gradients, day and night.
    let day_col = mix(vec3<f32>(0.58, 0.72, 0.94), vec3<f32>(0.10, 0.34, 0.80), pow(up, 0.55));
    let ngt_col = mix(vec3<f32>(0.05, 0.05, 0.13), vec3<f32>(0.012, 0.018, 0.055), pow(up, 0.7));
    var col = mix(ngt_col, day_col, day);
    // Golden hour: warm band hugging the horizon, strongest when the sun is low.
    let golden = (1.0 - smoothstep(0.0, 0.4, abs(sun))) * exp(-max(rd.y, 0.0) * 5.0);
    col = mix(col, vec3<f32>(1.0, 0.52, 0.24), golden * 0.55);
    // Mie forward-scatter halo around the sun + the disc itself (tonemap shoulders it).
    let mu = clamp(dot(rd, sd), 0.0, 1.0);
    col = col + vec3<f32>(1.0, 0.82, 0.55) * pow(mu, 32.0) * mix(0.06, 0.30, day);
    col = col + vec3<f32>(1.0, 0.95, 0.85) * smoothstep(0.99930, 0.99985, mu) * day * 2.0;
    return col;
}
// The dome sun's position projected back to screen uv (for god-rays/flare anchoring).
// Inverse of view_ray: el = mix(0.85, 0.02, uv.y), az = (uv.x - 0.5) * 0.9.
fn sun_screen(sun: f32) -> vec2<f32> {
    let el = clamp(sun, -1.0, 1.0) * 1.1;
    return vec2<f32>(0.5 + 0.32 / 0.9, (0.85 - el) / 0.83);
}

// ---- Bounded volumetric cloud march. Slab y ∈ [1.5, 3.6] world units, camera at y=0.2.
// rgb returned premultiplied by opacity; caller composites: col = col * (1-a) + rgb. ----
struct CloudParams {
    coverage: f32,   // 0..1 how much of the field is cloud
    dark: f32,       // 0 = white cumulus, 1 = storm-black albedo
    speed: f32,      // wind drift
    scale: f32,      // noise domain scale
    steps: i32,      // march steps (perf knob #1)
}
fn cloud_density(p: vec3<f32>, t: f32, cp: CloudParams) -> f32 {
    let q = p * cp.scale + vec3<f32>(t * cp.speed, 0.0, t * cp.speed * 0.35);
    let base = fbm3d(q);
    // Vertical profile: densest mid-slab, feathered top/bottom.
    let hprof = smoothstep(1.5, 1.9, p.y) * smoothstep(3.6, 2.7, p.y);
    // Threshold spans the noise's real value range (~0.30..0.62), not [0,1].
    let thr = mix(0.62, 0.30, cp.coverage);
    return clamp((base - thr) * 4.0, 0.0, 1.0) * hprof;
}
// Cheap 2-octave density for the per-step sun tap (shadow term needs no fine detail).
fn cloud_density_lo(p: vec3<f32>, t: f32, cp: CloudParams) -> f32 {
    let q = p * cp.scale + vec3<f32>(t * cp.speed, 0.0, t * cp.speed * 0.35);
    let hprof = smoothstep(1.5, 1.9, p.y) * smoothstep(3.6, 2.7, p.y);
    let thr = mix(0.62, 0.30, cp.coverage);
    return clamp((fbm3d2(q) - thr) * 4.0, 0.0, 1.0) * hprof;
}
fn march_clouds(uv: vec2<f32>, rd: vec3<f32>, t: f32, sd: vec3<f32>,
                key_col: vec3<f32>, amb_col: vec3<f32>, cp: CloudParams,
                flash_pos: vec3<f32>, flash_amt: f32) -> vec4<f32> {
    if (rd.y < 0.03) { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }
    let ro = vec3<f32>(0.0, 0.2, 0.0);
    let t0 = (1.5 - ro.y) / rd.y;
    let t1 = (3.6 - ro.y) / rd.y;
    let n = cp.steps;
    let dt = (t1 - t0) / f32(n);
    // Per-pixel dithered start (grain pass hides the stepping).
    var tt = t0 + dt * 0.7 * hash21(uv * u.res);
    var trans = 1.0;
    var acc = vec3<f32>(0.0);
    let albedo = mix(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(0.22, 0.24, 0.30), cp.dark);
    let mu = clamp(dot(rd, sd), 0.0, 1.0);
    let phase = 0.55 + 1.1 * pow(mu, 8.0);   // HG-ish forward lobe -> silver linings
    for (var i = 0; i < n; i = i + 1) {
        if (trans < 0.10) { break; }
        let p = ro + rd * tt;
        // Two-tier sampling: a cheap 2-octave probe rejects empty space before paying
        // for the full-detail density. Only worth it in sparse fields — dense storm
        // cells never reject, so the probe would be pure overhead there.
        if (cp.coverage < 0.7 && cloud_density_lo(p, t, cp) < 0.015) { tt = tt + dt; continue; }
        let dens = cloud_density(p, t, cp);
        if (dens > 0.012) {
            // 1-tap sun shadow (Beer's law, 2-octave density) + powder darkening in cores.
            // Near-black storm cells skip the tap — their shading is ambient + flash.
            var shadow = 0.45;
            if (cp.dark <= 0.7) {
                shadow = exp(-cloud_density_lo(p + sd * 0.5, t, cp) * 2.4);
            }
            let powder = 1.0 - exp(-dens * 4.5);
            var lit = albedo * (key_col * (shadow * phase * powder + 0.15) + amb_col * 0.75);
            // Storm interior flash: point light inside the cell during a strike.
            if (flash_amt > 0.001) {
                let fd = p - flash_pos;
                lit = lit + vec3<f32>(0.85, 0.88, 1.0) * flash_amt / (1.0 + dot(fd, fd) * 1.6);
            }
            let a = 1.0 - exp(-dens * dt * 5.5);
            acc = acc + trans * a * lit;
            trans = trans * (1.0 - a);
        }
        tt = tt + dt;
    }
    // Atmospheric perspective: distant (near-horizon) cloud fades into the sky instead of
    // stacking into a muddy wall at the bottom of the dome.
    let dist_fade = exp(-max(t0 - 8.0, 0.0) * 0.10);
    return vec4<f32>(acc * dist_fade, (1.0 - trans) * dist_fade);
}

// 3-octave fbm for far/cheap layers (perf budget: far planes never use the 5-octave fbm).
fn fbm3(p: vec2<f32>) -> f32 {
    var v = 0.0; var amp = 0.5; var q = p;
    for (var k = 0; k < 3; k = k + 1) { v = v + amp * noise2(q); q = q * 2.0; amp = amp * 0.5; }
    return v;
}

// ---- Sky grade: one global light state from continuous sun elevation ----
// sun ∈ [-1,1]: 1 noon · 0 horizon (golden hour) · -1 deep night.
struct Sky {
    key: vec3<f32>,      // key-light color (disc, rims, rays)
    ambient: f32,        // scene ambient level
    horizon: vec3<f32>,  // horizon-band tint (gold at low sun)
    daylight: f32,       // soft 0 night .. 1 day (replaces the old binary is_day)
}
fn sky_grade(sun: f32) -> Sky {
    let daylight = smoothstep(-0.12, 0.35, sun);
    let gold  = vec3<f32>(1.0, 0.72, 0.42);
    let noonw = vec3<f32>(1.0, 0.96, 0.90);
    let moon  = vec3<f32>(0.72, 0.78, 0.92);
    let golden = 1.0 - smoothstep(0.0, 0.45, abs(sun));   // peaks at the horizon
    var key = mix(moon, noonw, daylight);
    key = mix(key, gold, golden * 0.75);
    let ambient = mix(0.16, 1.0, daylight);
    // Horizon: night purple -> pale day blue-white, overridden by gold at golden hour.
    var horizon = mix(vec3<f32>(0.20, 0.16, 0.24), vec3<f32>(0.82, 0.88, 0.97), daylight);
    horizon = mix(horizon, gold, golden * 0.85);
    return Sky(key, ambient, horizon, daylight);
}

// ---- Moment scheduler: irregular episodic events (generalizes storm_strike) ----
// Time is cut into slots of 1/rate seconds; each slot fires with probability `prob`
// (hash-gated, per `channel`). Returns (env, phase, seed, active): env is a smooth
// attack/decay envelope over the slot; phase ∈ [0,1) is slot progress; seed is stable
// per slot for randomizing the event's parameters.
fn moment(t: f32, rate: f32, prob: f32, channel: f32) -> vec4<f32> {
    let slot = floor(t * rate);
    let phase = fract(t * rate);
    let seed = hash21(vec2<f32>(slot, 17.0 + channel * 31.0));
    let act = step(1.0 - prob, seed);   // ("active" is a reserved WGSL keyword)
    let env = act * smoothstep(0.0, 0.15, phase) * smoothstep(1.0, 0.45, phase);
    return vec4<f32>(env, phase, seed, act);
}

// ---- Depth helpers ----
// Atmospheric perspective: fade layer content toward the sky color by depth (0 near, 1 far).
fn atmo(col: vec3<f32>, sky_col: vec3<f32>, depth: f32) -> vec3<f32> {
    return mix(col, sky_col, depth * 0.55);
}
// Vertical depth grade: gently darken + desaturate down the canvas so the field reads as space.
fn depth_grade(col: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    let d = smoothstep(0.15, 1.0, uv.y);
    let lum = dot(col, vec3<f32>(0.299, 0.587, 0.114));
    return mix(col, mix(col, vec3<f32>(lum), 0.18) * 0.88, d);
}

// ---- Final grade: tints + soft-shoulder tone curve + vignette + grain ----
fn tonemap(c: vec3<f32>) -> vec3<f32> {
    // Knee curve: identity below the knee (mids untouched), soft shoulder above that
    // asymptotes to 1.0 — stops additive glows blowing out to flat white.
    let knee = vec3<f32>(0.85);
    let over = max(c - knee, vec3<f32>(0.0));
    return min(c, knee) + (over / (vec3<f32>(1.0) + over)) * (vec3<f32>(1.0) - knee);
}
fn grade(col_in: vec3<f32>, uv: vec2<f32>, t: f32) -> vec3<f32> {
    var col = max(col_in, vec3<f32>(0.0));
    // Temperature warmth + season tint (moved here from fs()).
    let warmth = clamp((u.temp - 10.0) / 25.0, -0.3, 0.3);
    col = col + vec3<f32>(warmth, 0.0, -warmth) * 0.12;
    col = mix(col, col * season_tint(u.season), 0.08);
    col = tonemap(col);
    // Vignette (aspect-corrected).
    let asp = u.res.y / u.res.x;
    let p = (uv - vec2<f32>(0.5, 0.5)) * vec2<f32>(1.0, asp);
    col = col * (1.0 - 0.22 * smoothstep(0.35, 0.85, length(p)));
    // Animated hash grain (~±1.5/255) dithers mesh-gradient banding away.
    let g = hash21(uv * u.res + vec2<f32>(fract(t) * 61.7, 0.0)) - 0.5;
    col = col + vec3<f32>(g * 0.012);
    return clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));
}

// Shared directional light. Elevation maps to screen height: horizon -> low, noon/deep night -> high.
// (uv.y = 0 is the TOP of the canvas.)
fn light_pos(t: f32, sun: f32) -> vec2<f32> {
    let h = mix(0.34, 0.12, clamp(abs(sun), 0.0, 1.0));
    return vec2<f32>(0.72, h + 0.02 * sin(t * 0.3));
}

// Faint round twinkling stars for clear/less-obscured night skies.
fn stars(uv: vec2<f32>, t: f32) -> f32 {
    let sc = uv * 110.0;
    let cell = floor(sc);
    let f = fract(sc) - 0.5;
    let g = hash21(cell);
    let tw = 0.5 + 0.5 * sin(t * 3.0 + g * 40.0);
    let dot = smoothstep(0.35, 0.0, length(f));
    return step(0.982, g) * tw * dot;
}

// Radial god-rays: march from the pixel toward the light, accumulating brightness.
fn god_rays(uv: vec2<f32>, lp: vec2<f32>, t: f32) -> f32 {
    var s = uv;
    let stepv = (lp - uv) / 24.0;
    var decay = 1.0;
    var acc = 0.0;
    for (var i = 0; i < 24; i = i + 1) {
        s = s + stepv;
        acc = acc + smoothstep(0.45, 0.0, distance(s, lp)) * decay;
        decay = decay * 0.92;
    }
    return acc / 24.0;
}

// ---- Condition bases + signature motifs ----
// Shooting star: rare, brief streak with a fading tail across the upper sky (night only).
fn shooting_star(uv: vec2<f32>, t: f32) -> f32 {
    let m = moment(t, 0.12, 0.5, 7.0);           // a chance roughly every ~8s
    if (m.w < 0.5) { return 0.0; }
    let s0 = vec2<f32>(0.15 + 0.6 * hash21(vec2<f32>(m.z, 3.0)),
                       0.08 + 0.15 * hash21(vec2<f32>(m.z, 5.0)));
    let dir = normalize(vec2<f32>(0.8, 0.35));
    let head = s0 + dir * m.y * 0.5;
    let to_head = uv - head;
    let along = dot(to_head, dir);                       // negative behind the head
    let across = abs(to_head.x * dir.y - to_head.y * dir.x);
    var tail = 0.0;
    if (along < 0.0 && along > -0.18) { tail = exp(along * 20.0); }
    let width = smoothstep(0.004, 0.0, across);
    let head_glow = smoothstep(0.015, 0.0, length(to_head));
    let vis = smoothstep(0.0, 0.1, m.y) * smoothstep(1.0, 0.6, m.y);
    return (head_glow + width * tail * 0.8) * vis;
}

fn clear_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let rd = view_ray(uv);
    let sd = sun_dir(u.sun);
    var col = sky_dome(rd, sd, u.sun);
    // Two-layer starfield: far = dim + dense (offset/scaled grid), near = bright + sparse.
    let starvis = smoothstep(0.15, -0.25, u.sun);
    col = col + vec3<f32>(0.85, 0.88, 1.0) * stars(uv * 1.9 + vec2<f32>(3.7, 1.3), t * 0.7) * starvis * 0.35;
    col = col + vec3<f32>(0.92, 0.94, 1.0) * stars(uv, t) * starvis * 0.75;
    col = col + vec3<f32>(1.0, 1.0, 1.0) * shooting_star(uv, t) * starvis;
    // Moon disc (NIGHT only — the dome draws the day sun), crater noise, halo + glow.
    let lp = light_pos(t, u.sun);
    let asp = u.res.y / u.res.x;
    let pd = length((uv - lp) * vec2<f32>(1.0, asp));
    var disc = smoothstep(0.070, 0.045, pd);
    disc = disc * (0.92 + 0.14 * fbm3(uv * 30.0));
    let glow = smoothstep(0.40, 0.0, pd) * 0.16;
    let halo = smoothstep(0.16, 0.0, pd) * 0.10;
    col = col + sky.key * (disc + glow + halo) * (1.0 - day);
    // God-rays from the dome sun's screen position; flare-pulse moment surges them by day.
    let flare = moment(t, 0.08, 0.5, 8.0).x * day;
    col = col + sky.key * god_rays(uv, sun_screen(u.sun), t) * (0.10 + 0.20 * day) * (1.0 + flare);
    return col;
}
fn cloud_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let rd = view_ray(uv);
    let sd = sun_dir(u.sun);
    var col = sky_dome(rd, sd, u.sun);
    // Cloud-break moment: a real coverage gap sweeps through (drives coverage down),
    // letting a god-ray shaft cross the scene.
    let mb = moment(t, 0.05, 0.6, 9.0);
    let cover = clamp(0.25 + 0.35 * intensity - mb.x * 0.25, 0.05, 0.62);
    let cp = CloudParams(cover, 0.12, 0.05, 0.42, 14);
    let amb = mix(vec3<f32>(0.10, 0.11, 0.17), vec3<f32>(0.55, 0.62, 0.75), day);
    let cl = march_clouds(uv, rd, t, sd, sky.key, amb, cp, vec3<f32>(0.0, 0.0, 0.0), 0.0);
    col = col * (1.0 - cl.a) + cl.rgb;
    col = col + sky.key * god_rays(uv, vec2<f32>(0.2 + 0.6 * mb.y, 0.18), t) * mb.x * 0.28 * day;
    return col;
}
// Falling rain streaks, per-column randomized, broken into short segments so they read as
// rain rather than static pinstripes. slant = diagonal lean (gusts push it), speedm = fall-
// speed multiplier. -t so streaks scroll DOWN (uv.y=0 is the top of the canvas).
fn rain_streaks(uv: vec2<f32>, t: f32, intensity: f32, slant: f32, speedm: f32) -> f32 {
    let sl = uv + vec2<f32>(uv.y * slant, 0.0);
    let cols = 55.0;
    let x = sl.x * cols;
    let col = floor(x);
    let fx = fract(x) - 0.5;
    let speed = (0.8 + hash21(vec2<f32>(col, 1.0)) * 1.0) * speedm;
    let y = fract(uv.y * 3.4 - t * speed + hash21(vec2<f32>(col, 3.0)));
    let line = smoothstep(0.09, 0.0, abs(fx));
    let head = smoothstep(0.85, 0.30, y) * smoothstep(0.0, 0.10, y);
    return line * head * (0.4 + 0.7 * intensity);
}
// One parallax snow layer: soft round flakes with coherent sway and per-flake size jitter.
// -t*speed falls DOWN (uv.y = 0 is the top). `soft` widens the flake; `boost` lowers the
// density threshold (flurries).
fn snow_layer2(uv: vec2<f32>, t: f32, scale: f32, speed: f32, seed: f32, soft: f32, boost: f32) -> f32 {
    var p = uv * scale + vec2<f32>(0.0, -t * speed);
    p.x = p.x + sin(t * (0.4 + seed * 0.2) + p.y * 1.5) * 0.35;   // coherent sway
    let g = floor(p);
    let f = fract(p) - 0.5;
    let h = hash21(g + seed);
    let sz = mix(0.10, 0.10 + soft, hash21(g + seed + 7.0));       // per-flake size jitter
    return smoothstep(sz, 0.0, length(f)) * step(0.84 - boost, h);
}
// Legacy 5-arg form (kept for rain's "big drops" near layer).
fn snow_layer(uv: vec2<f32>, t: f32, scale: f32, speed: f32, seed: f32) -> f32 {
    return snow_layer2(uv, t, scale, speed, seed, 0.06, 0.0);
}
fn rain_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    // Wind gust: slant + speed + brightness surge together over the event.
    let g = moment(t, 0.18, 0.45, 10.0);
    let slant = 0.06 + g.x * 0.22;
    let speedm = 1.0 + g.x * 0.9;
    // Depth: far misty rain sheet behind the glass streaks.
    let far = rain_streaks(uv * vec2<f32>(1.7, 1.4), t * 0.55, intensity, slant * 0.7, speedm) * 0.5;
    let streak = rain_streaks(uv, t, intensity, slant, speedm);
    // Wet-glass refraction: sample the dome at a streak-perturbed ray.
    let ruv = uv + vec2<f32>(streak * 0.014, 0.0);
    let sd = sun_dir(u.sun);
    var col = sky_dome(view_ray(ruv), sd, u.sun);
    // Overcast wash: rain skies are grey and dim.
    col = col * mix(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(0.45, 0.52, 0.62), 0.75) + vec3<f32>(0.02, 0.03, 0.05);
    col = col + vec3<f32>(0.45, 0.54, 0.68) * far * 0.15;
    col = col + vec3<f32>(0.65, 0.74, 0.88) * streak * 0.3 * (1.0 + g.x * 0.6);
    // Near depth: occasional large soft drops streaking past.
    let big = snow_layer(uv, t * 3.2, 5.0, 0.9, 5.0);
    col = col + vec3<f32>(0.55, 0.64, 0.80) * big * 0.18;
    // Wet sheen + pooling ripples near the silhouette band.
    col = col + vec3<f32>(0.10, 0.13, 0.18) * smoothstep(0.4, 1.0, uv.y) * (0.4 + 0.4 * day);
    let pool = smoothstep(0.78, 0.9, uv.y) * (0.5 + 0.5 * sin(uv.x * 40.0 - t * 4.0 + fbm(uv * 8.0) * 6.0));
    col = col + vec3<f32>(0.5, 0.6, 0.75) * pool * 0.12 * (0.5 + intensity);
    return col;
}
fn snow_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    var col = sky_dome(view_ray(uv), sun_dir(u.sun), u.sun);
    // Bright snow overcast on top of the dome.
    col = mix(col, vec3<f32>(0.78, 0.82, 0.90), 0.55 * mix(0.4, 1.0, day));
    // Flurry moment: density surge + gentle swirl of the whole field.
    let f = moment(t, 0.14, 0.4, 11.0);
    let suv = (uv - vec2<f32>(0.5, 0.5)) * rot(f.x * 0.35 * sin(t * 0.8)) + vec2<f32>(0.5, 0.5);
    let boost = f.x * 0.06;
    // Far (small/sharp) -> near (big/soft/swaying) parallax layers.
    var flakes = 0.0;
    flakes = flakes + snow_layer2(suv, t, 22.0, 0.10, 1.0, 0.04, boost) * 0.6;
    flakes = flakes + snow_layer2(suv, t, 15.0, 0.16, 2.0, 0.06, boost) * 0.8;
    flakes = flakes + snow_layer2(suv, t,  6.0, 0.26, 3.0, 0.16, boost) * 1.0;
    let bloom = mix(0.75, 1.0, day);
    col = col + vec3<f32>(1.0) * flakes * (0.35 + 0.4 * intensity) * bloom;
    // Faint ground-glow where snow gathers near the silhouette band.
    col = col + vec3<f32>(0.9, 0.93, 1.0) * smoothstep(0.75, 0.98, uv.y) * 0.08 * (0.4 + 0.6 * intensity);
    return col;
}
// Shared lightning-strike state so the bolt, shockwave, and window-edge jolt fire together.
// Returns (flash_env, bolt_x, life, seed); env is 0 when no strike. Sharper attack than the
// generic moment envelope, so it reshapes env from the raw phase.
fn storm_strike(t: f32) -> vec4<f32> {
    let m = moment(t, 0.7, 0.28, 4.0);
    let env = m.w * smoothstep(0.0, 0.04, m.y) * smoothstep(0.55, 0.08, m.y);
    let slot = floor(t * 0.7);
    let bx = 0.32 + 0.4 * hash21(vec2<f32>(slot, 7.0));
    return vec4<f32>(env, bx, m.y, slot);
}
// Piecewise-linear jagged bolt path: y quantized into segments, joints displaced by hash —
// sharp kinks, not the smooth fbm squiggle of the first version.
fn bolt_path(y: f32, seed: f32, bx: f32) -> f32 {
    let segs = 14.0;
    let fy = y * segs;
    let i0 = floor(fy);
    let f = fract(fy);
    let o0 = (hash21(vec2<f32>(i0, seed)) - 0.5) * 0.12
           + (hash21(vec2<f32>(i0, seed + 50.0)) - 0.5) * 0.04;
    let o1 = (hash21(vec2<f32>(i0 + 1.0, seed)) - 0.5) * 0.12
           + (hash21(vec2<f32>(i0 + 1.0, seed + 50.0)) - 0.5) * 0.04;
    return bx + mix(o0, o1, f);
}
// Forked lightning: thin brilliant core + tight inner glow + faint corona, with 3 tapering
// hash-placed branches, gated by the strike env. Fast flicker for the electric feel.
fn lightning(uv: vec2<f32>, t: f32, st: vec4<f32>) -> f32 {
    if (st.x <= 0.0) { return 0.0; }
    let seed = st.w;
    let ground = 0.70;                    // strike terminus = the shockwave impact row
    if (uv.y > ground) { return 0.0; }
    let path = bolt_path(uv.y, seed, st.y);
    let d = abs(uv.x - path);
    let core = smoothstep(0.0035, 0.0008, d);
    let inner = exp(-d * 220.0) * 0.55;
    let corona = exp(-d * 60.0) * 0.18;
    var b = core + inner + corona;
    // Branches: short diagonal offshoots leaving the trunk at hash-picked joints.
    for (var k = 0; k < 3; k = k + 1) {
        let fk = f32(k);
        let jy = (floor(hash21(vec2<f32>(seed, 60.0 + fk)) * 8.0) + 2.0) / 14.0;
        let sgn = sign(hash21(vec2<f32>(seed, 70.0 + fk)) - 0.5);
        let dy = uv.y - jy;
        if (dy > 0.0 && dy < 0.07) {
            let slope = 0.9 + 0.6 * hash21(vec2<f32>(seed, 80.0 + fk));
            let bp = bolt_path(jy, seed, st.y) + sgn * dy * slope;
            let bd = abs(uv.x - bp);
            let taper = 1.0 - dy / 0.07;
            b = b + (smoothstep(0.0022, 0.0005, bd) * 0.65 + exp(-bd * 260.0) * 0.22) * taper;
        }
    }
    let flick = 0.75 + 0.25 * step(0.4, hash21(vec2<f32>(floor(t * 48.0), seed)));
    return st.x * b * flick;
}
// Second bolt of an occasional double-strike: same slot as the primary, delayed ~10% of the
// slot, offset x. Gated on the PRIMARY slot being active (m.w) so a double never fires alone.
fn storm_strike2(t: f32) -> vec4<f32> {
    let m = moment(t, 0.7, 0.28, 4.0);            // same channel as storm_strike's gate
    let slot = floor(t * 0.7);
    let dbl = m.w * step(0.6, hash21(vec2<f32>(slot, 23.0)));
    let phase2 = m.y - 0.10;                      // smoothstep(0, 0.04, x) is 0 for x < 0
    let env = dbl * smoothstep(0.0, 0.04, phase2) * smoothstep(0.45, 0.06, phase2);
    let bx = clamp(0.32 + 0.4 * hash21(vec2<f32>(slot, 7.0)) + 0.10, 0.05, 0.95);
    return vec4<f32>(env, bx, phase2, slot + 101.0);
}
// Three counter-scrolling banks: sharp/fast -> soft/slow, at distinct scales.
fn fog_banks(uv: vec2<f32>, t: f32) -> f32 {
    let n1 = fbm(uv * vec2<f32>(3.0, 1.6) + vec2<f32>(t * 0.06, 0.0));
    let n2 = fbm3(uv * vec2<f32>(1.8, 1.0) + vec2<f32>(-t * 0.04, 1.7));
    let n3 = fbm3(uv * vec2<f32>(1.1, 0.7) + vec2<f32>(t * 0.02, 3.9));
    return 0.42 * n1 + 0.33 * n2 + 0.25 * n3;
}
fn storm_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let st = storm_strike(t);
    let st2 = storm_strike2(t);
    let df = moment(t, 0.5, 0.2, 12.0);   // distant sheet flash (no bolt)
    let flash = st.x + st2.x;
    // Shockwave: an expanding ring from the primary strike's ground contact deforms the background.
    let asp = u.res.y / u.res.x;
    let impact = vec2<f32>(st.y, 0.70);
    let dvec = (uv - impact) * vec2<f32>(1.0, asp);
    let dist = length(dvec);
    let ring = smoothstep(0.09, 0.0, abs(dist - st.z * 1.1));
    let disp = normalize(dvec + vec2<f32>(0.0001, 0.0001)) * ring * st.x * 0.06;
    let duv = uv + disp;
    // Very dark dome base, sampled at the shockwave-rippled coordinate.
    let sd = sun_dir(u.sun);
    var col = sky_dome(view_ray(duv), sd, u.sun) * vec3<f32>(0.30, 0.32, 0.38);
    // Volumetric storm cell, lit from inside by strikes (bolt x mapped into slab space).
    let rd = view_ray(uv);
    let cp = CloudParams(0.85, 0.9, 0.14, 0.55, 22);
    let flash_pos = vec3<f32>((st.y - 0.5) * 5.0, 2.4, 7.0);
    let amb = mix(vec3<f32>(0.06, 0.07, 0.11), vec3<f32>(0.30, 0.33, 0.42), day);
    // Even boltless sheet flashes glow inside the cell (df at lower gain).
    let cl = march_clouds(uv, rd, t, sd, sky.key, amb, cp, flash_pos, flash * 3.0 + df.x * 0.8);
    col = col * (1.0 - cl.a) + cl.rgb;
    // Driving rain sheets in two depths (storm-locked slant/speed).
    col = col + vec3<f32>(0.35, 0.40, 0.52) * rain_streaks(uv, t, 1.0, 0.10, 1.3) * 0.18;
    col = col + vec3<f32>(0.30, 0.34, 0.46) * rain_streaks(uv * vec2<f32>(1.6, 1.3), t * 0.6, 1.0, 0.08, 1.3) * 0.10;
    // Shockwave highlight + whole-sky flash (primary + double + distant sheet flashes).
    col = col + vec3<f32>(0.55, 0.60, 0.75) * ring * st.x * 0.5;
    col = col + vec3<f32>(0.60, 0.63, 0.78) * (flash * 0.18 + df.x * 0.08);
    // Bolts: primary + occasional double, with a soft afterglow trailing the primary.
    col = col + vec3<f32>(0.90, 0.92, 1.0) * lightning(uv, t, st);
    col = col + vec3<f32>(0.85, 0.88, 1.0) * lightning(uv, t, st2) * 0.8;
    col = col + vec3<f32>(0.70, 0.74, 0.95) * lightning(uv, t, vec4<f32>(sqrt(st.x) * 0.25, st.y, st.z, st.w));
    return col;
}
fn fog_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    var col = sky_dome(view_ray(uv), sun_dir(u.sun), u.sun);
    let fogc = mix(vec3<f32>(0.55, 0.57, 0.60), vec3<f32>(0.86, 0.88, 0.90), day);
    let banks = fog_banks(uv, t);
    // Fog-roll moment: a dense bank drifts across; visibility drops then recovers.
    let mr = moment(t, 0.06, 0.5, 13.0);
    let roll = mr.x * smoothstep(0.35, 0.0, abs(uv.x - (mr.y * 1.4 - 0.2))) * 0.35;
    // Denser low + toward the horizon; distant content fades hardest.
    var dens = banks * (0.5 + 0.7 * intensity) + smoothstep(0.3, 1.0, uv.y) * 0.35 + roll;
    dens = clamp(dens, 0.0, 0.92);
    col = mix(col, fogc, dens);
    // Light-diffusion halo where the sun sits behind the fog (day only).
    let lp = light_pos(t, u.sun);
    let asp = u.res.y / u.res.x;
    let pd = length((uv - lp) * vec2<f32>(1.0, asp));
    col = col + sky.key * smoothstep(0.5, 0.0, pd) * 0.10 * day;
    return col;
}

// ---- Subtle season tint (multiplier, mixed at low strength) ----
fn season_tint(season: f32) -> vec3<f32> {
    let s = i32(round(clamp(season, 0.0, 3.0)));
    if (s == 0) { return vec3<f32>(0.86, 0.93, 1.06); }   // winter: cool
    if (s == 1) { return vec3<f32>(0.93, 1.05, 0.95); }   // spring: fresh green
    if (s == 2) { return vec3<f32>(1.08, 1.00, 0.90); }   // summer: warm
    return vec3<f32>(1.08, 0.95, 0.80);                    // autumn: amber
}

// Softly darkens the shader behind the 2D UI so text stays legible (the engine has no
// text-shadow/scrim primitive). Retuned for the graded palettes: shallower, wider falloffs
// so the scrim disappears into the scene. Zones (canvas 400x680, uv normalized).
fn ui_scrim(uv: vec2<f32>) -> f32 {
    var s = 1.0;
    // Hero block (top-left): strongest, with a long soft tail reaching the hi_lo/feels row.
    s = s - 0.44 * smoothstep(0.66, 0.24, uv.x) * smoothstep(0.42, 0.0, uv.y);
    // Hourly strip band: gentle.
    s = s - 0.24 * smoothstep(0.30, 0.355, uv.y) * smoothstep(0.43, 0.375, uv.y);
    // Daily columns (left + right thirds).
    let band = smoothstep(0.43, 0.49, uv.y) * smoothstep(0.84, 0.78, uv.y);
    s = s - 0.14 * band * (smoothstep(0.34, 0.04, uv.x) + smoothstep(0.66, 0.96, uv.x));
    return clamp(s, 0.55, 1.0);
}

// Debris-impact event (winds): a moment channel picks an edge point; the scene draws a
// streak CONVERGING on it, then the window takes a dent with a damped spring-back.
// Returns (spring, edge_x, edge_y, converge) — spring > 0 only just after contact (phase
// 0.70 of the slot); converge ∈ (0,1] only during the approach (phase 0.55..0.70).
fn impact_event(t: f32) -> vec4<f32> {
    let m = moment(t, 0.08, 0.7, 21.0);
    if (m.w < 0.5) { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }
    let side = step(0.5, hash21(vec2<f32>(m.z, 2.0)));            // 0 = left edge, 1 = right
    let ey = 0.15 + 0.60 * hash21(vec2<f32>(m.z, 3.0));
    var spring = 0.0;
    if (m.y > 0.70) {
        let p = m.y - 0.70;
        spring = exp(-p * 9.0) * sin(p * 55.0);                    // damped wobble
    }
    let converge = smoothstep(0.55, 0.70, m.y) * step(m.y, 0.70);
    return vec4<f32>(spring, side, ey, converge);
}

// ---- High winds (demo condition 6): clear gale — racing shredded clouds + gusting debris. ----
fn wind_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let rd = view_ray(uv);
    let sd = sun_dir(u.sun);
    var col = sky_dome(rd, sd, u.sun);
    let g = moment(t, 0.15, 0.6, 20.0);        // gusts (shared with window_alpha case 6)
    // Shredded racing clouds: horizontally-elongated 2D noise scrolling FAST.
    let shred = fbm(vec2<f32>(uv.x * 1.4 - t * (0.9 + g.x * 0.6), uv.y * 7.0));
    let cover = smoothstep(0.55, 0.75, shred) * smoothstep(0.75, 0.15, uv.y);
    let cloudc = mix(vec3<f32>(0.35, 0.38, 0.45), vec3<f32>(0.95, 0.96, 0.99), day);
    col = mix(col, cloudc, cover * 0.75);
    // Debris: rain_streaks TRANSPOSED (columns -> rows, y-scroll -> x-scroll), ochre-tinted,
    // two depths, speed surging with gusts.
    let spd = 2.2 * (1.0 + g.x * 1.5);
    let d1 = rain_streaks(vec2<f32>(uv.y, uv.x), t, 0.8, 0.15, spd);
    let d2 = rain_streaks(vec2<f32>(uv.y * 1.6, uv.x * 1.4), t * 0.7, 0.8, 0.10, spd);
    col = col + vec3<f32>(0.55, 0.45, 0.28) * d1 * 0.30 * mix(0.5, 1.0, day);
    col = col + vec3<f32>(0.45, 0.38, 0.25) * d2 * 0.18 * mix(0.5, 1.0, day);
    // Converging impact streak: a bright dash flying toward the strike point.
    let ie = impact_event(t);
    if (ie.w > 0.001) {
        let tgt = vec2<f32>(ie.y, ie.z);   // ("target"/"from" are reserved WGSL keywords)
        let origin = vec2<f32>(1.0 - ie.y, ie.z - 0.25);          // enters from the far side, above
        let pos = mix(origin, tgt, ie.w);
        let asp = u.res.y / u.res.x;
        let dd = length((uv - pos) * vec2<f32>(1.0, asp));
        col = col + vec3<f32>(0.75, 0.62, 0.40) * smoothstep(0.020, 0.004, dd);
    }
    return col;
}

// SYNC: 32s period and the 0.60..0.80 engulf window mirror Tsunami.swift. Change together.
fn tsunami_phase() -> f32 { return fract(u.cond_age / 32.0); }

// ---- Tsunami (demo condition 7): a 32s arc — calm ocean, swell, wall, CRASH (full
// engulf; the host blanks the UI in sync), recede. Layered 2D ocean, no ray-march. ----
fn tsunami_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let ph = tsunami_phase();
    let day = sky.daylight;
    var col = sky_dome(view_ray(uv), sun_dir(u.sun), u.sun);
    // Sea level (uv.y of the surface): calm 0.80 -> swell 0.70 -> wall to ~0.05 -> restore.
    var level = 0.80;
    level = level - smoothstep(0.0, 0.45, ph) * 0.10;
    level = level - smoothstep(0.45, 0.62, ph) * 0.75;
    level = level + smoothstep(0.74, 0.86, ph) * 0.85;   // fast drain — text returns as the hero zone clears
    // 4 parallax wave bands stacked below `level`, each fbm-displaced, nearer = darker + wilder.
    let chop = 1.0 + smoothstep(0.30, 0.60, ph) * 2.0;            // seas roughen as it builds
    for (var k = 0; k < 4; k = k + 1) {
        let fk = f32(k);
        let wob = (fbm(vec2<f32>(uv.x * (2.0 + fk * 1.3) + t * (0.25 + fk * 0.18), fk * 3.7)) - 0.5) * chop;
        let surf = level + fk * 0.030 + wob * (0.025 + fk * 0.012);
        let m = smoothstep(surf, surf + 0.012, uv.y);
        let water = mix(vec3<f32>(0.10, 0.34, 0.44), vec3<f32>(0.02, 0.13, 0.21), fk / 3.0);
        col = mix(col, water * mix(0.35, 1.0, day), m * 0.85);
        // Foam crest on each band, brightest when seas are rough.
        col = col + vec3<f32>(0.90, 0.95, 1.0)
                  * smoothstep(0.010, 0.0, abs(uv.y - surf)) * (0.25 + 0.5 * chop * abs(wob));
    }
    // Spray during rise/crash: fast upward-streaking particles above the surface.
    let spray_amt = smoothstep(0.45, 0.60, ph) * (1.0 - smoothstep(0.74, 0.80, ph));
    if (spray_amt > 0.0) {
        let sp = snow_layer2(vec2<f32>(uv.x, 1.0 - uv.y), t * 2.5, 16.0, 0.8, 6.0, 0.05, 0.06);
        col = col + vec3<f32>(0.85, 0.92, 0.97) * sp * spray_amt * 0.6;
    }
    // Engulf: full-screen underwater — deep teal, caustic shimmer, rising bubbles.
    let uw = smoothstep(0.58, 0.62, ph) * (1.0 - smoothstep(0.78, 0.82, ph));
    if (uw > 0.0) {
        let caust = fbm(uv * 6.0 + vec2<f32>(t * 0.8, -t * 0.5))
                  * fbm(uv * 9.0 - vec2<f32>(t * 0.6, t * 0.4));
        var deep = vec3<f32>(0.03, 0.17, 0.25) + vec3<f32>(0.18, 0.45, 0.50) * caust;
        let bub = snow_layer2(vec2<f32>(uv.x, 1.0 - uv.y), t, 14.0, 0.5, 9.0, 0.05, 0.0);
        deep = deep + vec3<f32>(0.7, 0.85, 0.9) * bub * 0.5;
        col = mix(col, deep * mix(0.5, 1.0, day), uw);
    }
    return col;
}

// Snow pile height in uv units at column x. Grows linearly to full size over 150s.
// SYNC: mean height (0.21 * growth) crosses the last daily row's bottom (uv 0.809,
// i.e. height 0.191) at age ≈ 135s == SnowPile.buryAgeLastRow in Swift. Change together.
fn pile_height(x: f32, age: f32) -> f32 {
    let growth = clamp(age / 150.0, 0.0, 1.0);
    return growth * (0.16 + 0.10 * fbm(vec2<f32>(x * 3.5, 7.7)));
}

// ---- Theatrical window mask: rounded-rect base, deformed per condition on ALL edges.
// Replaces the M3 bottom-band silhouette + corner mask. ----
fn base_mask(uv: vec2<f32>, inset: f32) -> f32 {
    let asp = u.res.y / u.res.x;
    let p = (uv - vec2<f32>(0.5, 0.5)) * vec2<f32>(1.0, asp);
    let b = vec2<f32>(0.5 - inset, (0.5 - inset) * asp);
    let r = 0.065;   // corner radius (x-uv units ~ 26 px)
    let q = abs(p) - b + vec2<f32>(r, r);
    let sd = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
    return smoothstep(0.006, -0.006, sd);
}
fn window_alpha(uv: vec2<f32>, t: f32, cond: i32, intensity: f32) -> f32 {
    var a = base_mask(uv, 0.0);
    let x = uv.x;
    if (cond == 0) {
        // Clear: gentle bottom wave (restraint is the contrast).
        let b = smoothstep(0.86, 1.0, uv.y);
        a = a * (1.0 - smoothstep(0.42 + 0.08 * sin(x * 8.0 + t * 0.8), 1.0, b));
    } else if (cond == 1) {
        // Cloud: TOP edge takes soft cumulus-profile bumps (silhouette cut by the clouds).
        let bump = fbm(vec2<f32>(x * 5.0 + t * 0.05, 3.3)) * 0.045;
        a = a * smoothstep(0.0, 0.012, uv.y - (0.012 + bump) * smoothstep(0.2, 0.0, uv.y));
    } else if (cond == 2) {
        // Rain: whole outline undulates (sheeting water), drips at the bottom,
        // moment-gated droplet detaching from the bottom-right corner.
        let g = moment(t, 0.18, 0.45, 10.0);
        let amp = (0.006 + 0.010 * intensity) * (1.0 + g.x);
        let wob = fbm(vec2<f32>(uv.y * 9.0 + t * 1.2, x * 9.0 - t)) - 0.5;
        a = a * base_mask(uv + vec2<f32>(wob, wob) * amp, -0.004);
        let drip = fbm(vec2<f32>(x * 12.0, t * 0.6));
        a = a * (1.0 - smoothstep(0.30 + 0.24 * drip, 1.0, smoothstep(0.85, 1.0, uv.y) * 1.4));
        let md = moment(t, 0.10, 0.5, 14.0);
        if (md.w > 0.5) {
            let c = vec2<f32>(0.88, 0.965 + md.y * 0.08);
            let asp2 = u.res.y / u.res.x;
            let dd = length((uv - c) * vec2<f32>(1.0, asp2));
            a = max(a, smoothstep(0.016, 0.010, dd) * (1.0 - md.y));   // detaching blob
        }
    } else if (cond == 3) {
        // Snow: soft scalloped bottom + the accumulating pile (full alpha inside the mound).
        let b = (uv.y - 0.86) / 0.14;
        a = a * (1.0 - smoothstep(0.35 + 0.12 * abs(sin(x * 10.0 + t * 0.3)), 1.2, max(b, 0.0)));
        let ph = pile_height(x, u.cond_age);
        if (uv.y > 1.0 - ph) { a = base_mask(uv, 0.0); }   // pile restores the full mask
    } else if (cond == 4) {
        // Storm: jagged churned bottom + strike edge-jolt + WINDOW CRACKS from the impact.
        let st = storm_strike(t);
        let b = (uv.y - 0.84) / 0.16;
        let j = fbm(vec2<f32>(x * 14.0 + t * 1.5, t));
        var edge = 0.38 + 0.22 * (j - 0.5) * 2.0;
        edge = edge - st.x * smoothstep(0.24, 0.0, abs(x - st.y)) * 0.42;
        a = a * (1.0 - smoothstep(edge, edge + 0.06, max(b, 0.0)));
        // Cracks: 4 jagged transparent fractures radiating from the impact, healing with
        // the strike envelope (~0.7s visible). Transient text crossing is accepted theater.
        let crack_env = st.x;
        if (crack_env > 0.02) {
            let asp2 = u.res.y / u.res.x;
            let impact = vec2<f32>(st.y, 0.70);
            for (var k = 0; k < 4; k = k + 1) {
                let fk = f32(k);
                let ang = (hash21(vec2<f32>(st.w, 30.0 + fk)) - 0.5) * 2.6 - 1.57;
                let dirv = normalize(vec2<f32>(cos(ang), sin(ang) / asp2));
                let rel = uv - impact;
                let along = dot(rel, dirv);
                let across = abs(rel.x * dirv.y - rel.y * dirv.x);
                let jag = (fbm(vec2<f32>(along * 18.0, st.w + fk)) - 0.5) * 0.02;
                let reach = 0.05 + 0.30 * hash21(vec2<f32>(st.w, 40.0 + fk));
                let on_line = smoothstep(0.0035, 0.0012, abs(across + jag))
                            * step(0.0, along) * step(along, reach) * (1.0 - along / reach);
                a = a * (1.0 - on_line * crack_env);
            }
        }
    } else if (cond == 5) {
        // Fog: erosion — edge noise eats inward on ALL edges; peak = ghost window.
        let mr = moment(t, 0.06, 0.5, 13.0);
        let breathe = 0.5 + 0.5 * sin(t * 0.15) + mr.x;
        let edge_d = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
        let eat = (0.02 + 0.030 * breathe) * fbm(vec2<f32>(uv.x * 6.0 + t * 0.1, uv.y * 6.0 - t * 0.07));
        a = a * smoothstep(0.0, 0.02, edge_d - eat);
    } else if (cond == 6) {
        // High winds: tremble + gust jolts + top-edge fabric flap + debris-impact dents.
        let g = moment(t, 0.15, 0.6, 20.0);
        let jit = (hash21(vec2<f32>(floor(t * 30.0), 1.0)) - 0.5) * 0.003;
        let jolt = g.x * 0.008;                                    // shove downwind (-x)
        let uvw = uv + vec2<f32>(jit - jolt, jit * 0.6);
        a = base_mask(uvw, 0.0);
        // Top edge luffs like fabric: a traveling ripple, gust-enveloped.
        let flap = (0.5 + 0.5 * sin(uv.x * 30.0 - t * 18.0)) * 0.010 * (0.3 + g.x);
        a = a * smoothstep(0.0, 0.010, uvw.y - flap * smoothstep(0.15, 0.0, uv.y));
        // Bottom: gentle clear-style wave.
        let b = smoothstep(0.86, 1.0, uv.y);
        a = a * (1.0 - smoothstep(0.45 + 0.06 * sin(x * 7.0 + t * 1.2), 1.0, b));
        // Impact dent with damped spring-back.
        let ie = impact_event(t);
        if (abs(ie.x) > 0.001) {
            let asp2 = u.res.y / u.res.x;
            let dd = length((uv - vec2<f32>(ie.y, ie.z)) * vec2<f32>(1.0, asp2));
            a = a * (1.0 - clamp(ie.x, 0.0, 1.0) * 0.9 * smoothstep(0.06, 0.0, dd));
        }
    } else if (cond == 7) {
        let ph = tsunami_phase();
        // Impact bulge: the window swells outward as the wall arrives, peaking at the crash.
        let bulge = smoothstep(0.45, 0.62, ph) * (1.0 - smoothstep(0.66, 0.74, ph)) * 0.012;
        a = base_mask(uv, -bulge);
        // Recede: water sheets off — heavy streams hanging below the bottom edge + side runs.
        let shed = smoothstep(0.74, 0.78, ph) * (1.0 - smoothstep(0.92, 1.0, ph));
        if (shed > 0.0) {
            let stream = fbm(vec2<f32>(x * 10.0, t * 1.4));
            let ext = smoothstep(0.9, 1.0, uv.y) * smoothstep(0.25, 0.75, stream) * shed;
            a = max(a, ext * base_mask(vec2<f32>(uv.x, 0.5), 0.0));   // streams hang below
            let side_d = min(uv.x, 1.0 - uv.x);
            a = max(a, shed * smoothstep(0.012, 0.0, side_d)
                        * smoothstep(0.35, 0.75, fbm(vec2<f32>(uv.y * 8.0, t + x))) * 0.8);
        }
    }
    return clamp(a, 0.0, 1.0);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.time;
    let sky = sky_grade(clamp(u.sun, -1.0, 1.0));
    let intensity = clamp(u.intensity, 0.0, 1.0);
    let cond = i32(u.condition);
    var col: vec3<f32>;
    switch (cond) {
        case 0: { col = clear_c(uv, t, sky, intensity); }
        case 1: { col = cloud_c(uv, t, sky, intensity); }
        case 2: { col = rain_c(uv, t, sky, intensity); }
        case 3: { col = snow_c(uv, t, sky, intensity); }
        case 4: { col = storm_c(uv, t, sky, intensity); }
        case 5: { col = fog_c(uv, t, sky, intensity); }
        case 6: { col = wind_c(uv, t, sky, intensity); }
        case 7: { col = tsunami_c(uv, t, sky, intensity); }
        default: { col = clear_c(uv, t, sky, intensity); }
    }
    // Snow pile is drawn scenery: bright mound with faint sparkle, graded with the scene.
    if (cond == 3) {
        let ph = pile_height(uv.x, u.cond_age);
        if (uv.y > 1.0 - ph) {
            let depth_in = (uv.y - (1.0 - ph)) / max(ph, 0.001);
            col = mix(vec3<f32>(0.92, 0.94, 0.99), vec3<f32>(0.70, 0.75, 0.86), depth_in);
            col = col + vec3<f32>(1.0, 1.0, 1.0) * step(0.985, hash21(floor(uv * u.res / 3.0))) * 0.25;
        }
    }
    col = depth_grade(col, uv);
    col = grade(col, uv, t);   // tints + tone curve + vignette + grain
    col = col * ui_scrim(uv);
    let a = window_alpha(uv, t, cond, intensity);
    return vec4<f32>(col * a, a);
}
