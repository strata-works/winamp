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
// fbm domain warp — bends the sample coordinate so gradients flow organically.
fn warp(p: vec2<f32>, t: f32) -> vec2<f32> {
    let q = vec2<f32>(fbm(p + vec2<f32>(0.0, t * 0.05)),
                      fbm(p + vec2<f32>(5.2, 1.3 - t * 0.04)));
    return p + 0.6 * q;
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
    return clamp((base - (1.0 - cp.coverage)) * 2.4, 0.0, 1.0) * hprof;
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
    var tt = t0 + dt * hash21(uv * u.res);
    var trans = 1.0;
    var acc = vec3<f32>(0.0);
    let albedo = mix(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(0.22, 0.24, 0.30), cp.dark);
    let mu = clamp(dot(rd, sd), 0.0, 1.0);
    let phase = 0.55 + 1.1 * pow(mu, 8.0);   // HG-ish forward lobe -> silver linings
    for (var i = 0; i < n; i = i + 1) {
        if (trans < 0.05) { break; }
        let p = ro + rd * tt;
        let dens = cloud_density(p, t, cp);
        if (dens > 0.012) {
            // 1-tap sun shadow (Beer's law) + powder darkening in thick cores.
            let ldens = cloud_density(p + sd * 0.5, t, cp);
            let shadow = exp(-ldens * 2.4);
            let powder = 1.0 - exp(-dens * 4.5);
            var lit = albedo * (key_col * shadow * phase * powder + amb_col * 0.4);
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
    return vec4<f32>(acc, 1.0 - trans);
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

// ---- paper.design-style flowing 4-point mesh gradient ----
// Four color anchors near the corners, each drifting on a slow path; blended by
// inverse-distance-power weights over a domain-warped coordinate.
fn mesh_gradient(uv: vec2<f32>, t: f32, c0: vec3<f32>, c1: vec3<f32>, c2: vec3<f32>, c3: vec3<f32>) -> vec3<f32> {
    let w = warp(uv * 1.5, t);
    let p0 = vec2<f32>(0.20 + 0.10 * sin(t * 0.11),        0.20 + 0.10 * cos(t * 0.13));
    let p1 = vec2<f32>(0.80 + 0.10 * sin(t * 0.10 + 1.7),  0.25 + 0.10 * cos(t * 0.09 + 2.1));
    let p2 = vec2<f32>(0.25 + 0.10 * sin(t * 0.08 + 3.1),  0.80 + 0.10 * cos(t * 0.12 + 0.7));
    let p3 = vec2<f32>(0.80 + 0.10 * sin(t * 0.09 + 4.2),  0.80 + 0.10 * cos(t * 0.10 + 1.1));
    let e = 2.0;
    let d0 = 1.0 / (pow(distance(w, p0), e) + 0.03);
    let d1 = 1.0 / (pow(distance(w, p1), e) + 0.03);
    let d2 = 1.0 / (pow(distance(w, p2), e) + 0.03);
    let d3 = 1.0 / (pow(distance(w, p3), e) + 0.03);
    let acc = c0 * d0 + c1 * d1 + c2 * d2 + c3 * d3;
    return acc / (d0 + d1 + d2 + d3);
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
    let golden = 1.0 - smoothstep(0.0, 0.45, abs(u.sun));
    // Palette anchors; the horizon anchor comes from the sky grade (golden hour lives there).
    let c0 = mix(vec3<f32>(0.03, 0.04, 0.14), vec3<f32>(0.15, 0.42, 0.88), day);
    let c1 = mix(vec3<f32>(0.05, 0.07, 0.20), vec3<f32>(0.28, 0.55, 0.93), day);
    let c2 = mix(vec3<f32>(0.09, 0.08, 0.20), vec3<f32>(0.42, 0.64, 0.94), day);
    // Bottom-right anchor: saturated blue at noon, giving way to the sky-grade horizon
    // (pale/gold) only near the horizon hours.
    let day_c3 = mix(vec3<f32>(0.50, 0.70, 0.96), sky.horizon, 0.35 + 0.65 * golden);
    let c3 = mix(vec3<f32>(0.15, 0.10, 0.19), day_c3, max(day, 0.35));
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    // Horizon glow band — a golden-hour feature; nearly invisible at noon/deep night.
    col = col + sky.horizon * smoothstep(0.45, 0.95, uv.y) * (0.06 + 0.22 * golden);
    // Two-layer starfield: far = dim + dense (offset/scaled grid), near = bright + sparse.
    let starvis = smoothstep(0.15, -0.25, u.sun);
    col = col + vec3<f32>(0.85, 0.88, 1.0) * stars(uv * 1.9 + vec2<f32>(3.7, 1.3), t * 0.7) * starvis * 0.35;
    col = col + vec3<f32>(0.92, 0.94, 1.0) * stars(uv, t) * starvis * 0.75;
    col = col + vec3<f32>(1.0, 1.0, 1.0) * shooting_star(uv, t) * starvis;
    // Sun/moon disc, elevation-tracking, aspect-corrected so the disc is round.
    let lp = light_pos(t, u.sun);
    let asp = u.res.y / u.res.x;
    let pd = length((uv - lp) * vec2<f32>(1.0, asp));
    var disc = smoothstep(0.070, 0.045, pd);
    // Moon surface: faint crater noise, night only.
    disc = disc * mix(0.92 + 0.14 * fbm3(uv * 30.0), 1.0, day);
    // Halo ring + broad glow; sun-flare pulse moment surges both by day.
    let flare = moment(t, 0.08, 0.5, 8.0).x * day;
    let glow = smoothstep(0.40, 0.0, pd) * mix(0.16, 0.19, day) * (1.0 + flare * 0.8);
    let halo = smoothstep(0.16, 0.0, pd) * 0.10;
    col = col + sky.key * (disc + glow + halo);
    // God-rays (subtle; surge with the flare pulse).
    col = col + sky.key * god_rays(uv, lp, t) * (0.10 + 0.20 * day) * (1.0 + flare);
    return col;
}
fn cloud_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let c0 = mix(vec3<f32>(0.10, 0.11, 0.16), vec3<f32>(0.55, 0.62, 0.74), day);
    let c1 = mix(vec3<f32>(0.13, 0.14, 0.20), vec3<f32>(0.68, 0.73, 0.82), day);
    let c2 = mix(vec3<f32>(0.16, 0.17, 0.23), vec3<f32>(0.80, 0.83, 0.89), day);
    let c3 = mix(vec3<f32>(0.12, 0.13, 0.18), vec3<f32>(0.60, 0.66, 0.78), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    let skybg = col;
    let lp = light_pos(t, u.sun);
    let to_light = normalize(lp - uv + vec2<f32>(0.0001, 0.0001));
    let litd = mix(vec3<f32>(0.20, 0.22, 0.28), vec3<f32>(0.92, 0.94, 0.98), day);
    let shad = mix(vec3<f32>(0.10, 0.11, 0.15), vec3<f32>(0.52, 0.56, 0.64), day);
    // Three parallax planes, far -> near. Far plane: 3-octave fbm + atmospheric fade.
    for (var k = 0; k < 3; k = k + 1) {
        let fk = f32(k);
        let far = 1.0 - fk / 2.0;                 // 1 far .. 0 near
        let sc = 2.0 + fk * 1.6;
        let sp = 0.04 + fk * 0.03;
        let q = uv * vec2<f32>(sc, sc * 0.7) + vec2<f32>(t * sp, fk * 3.1);
        // Billow: light second-octave warp of the sample coordinate.
        let bw = vec2<f32>(fbm3(q * 1.7 + vec2<f32>(t * 0.02, 0.0)), fbm3(q * 1.7 + vec2<f32>(2.7, 1.1)));
        var n = 0.0;
        if (k == 0) { n = fbm3(q + 0.35 * bw); } else { n = fbm(q + 0.35 * bw); }
        let cover = smoothstep(0.55, 0.85, n) * (0.35 + 0.25 * fk) * (0.6 + 0.5 * intensity);
        // Directional lighting + sun-side rim (gradient of the field toward the light).
        let nlit = fbm3(q + 0.35 * bw + to_light * 0.10);
        let rim = clamp(n - nlit, 0.0, 1.0) * 2.2;
        var plane = mix(shad, litd, clamp(0.5 + (lp.x - uv.x) * 0.8, 0.0, 1.0));
        plane = atmo(plane, skybg, far);
        col = mix(col, plane, cover);
        col = col + sky.key * rim * cover * 0.35;
    }
    // Cloud-break moment: a god-ray shaft sweeps across during the event (day only).
    // Gain kept modest — at noon the scene is already bright and a hot shaft washes it out.
    let mb = moment(t, 0.05, 0.6, 9.0);
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
    let c0 = mix(vec3<f32>(0.06, 0.09, 0.14), vec3<f32>(0.30, 0.40, 0.52), day);
    let c1 = mix(vec3<f32>(0.08, 0.11, 0.17), vec3<f32>(0.38, 0.48, 0.60), day);
    let c2 = mix(vec3<f32>(0.10, 0.13, 0.19), vec3<f32>(0.46, 0.56, 0.68), day);
    let c3 = mix(vec3<f32>(0.05, 0.08, 0.13), vec3<f32>(0.28, 0.38, 0.50), day);
    // Depth: far misty rain sheet behind the glass streaks.
    let far = rain_streaks(uv * vec2<f32>(1.7, 1.4), t * 0.55, intensity, slant * 0.7, speedm) * 0.5;
    let streak = rain_streaks(uv, t, intensity, slant, speedm);
    // Wet-glass refraction, slightly stronger than before.
    let ruv = uv + vec2<f32>(streak * 0.014, 0.0);
    var col = mesh_gradient(ruv, t, c0, c1, c2, c3);
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
    let c0 = mix(vec3<f32>(0.16, 0.19, 0.28), vec3<f32>(0.74, 0.80, 0.90), day);
    let c1 = mix(vec3<f32>(0.20, 0.23, 0.32), vec3<f32>(0.82, 0.87, 0.95), day);
    let c2 = mix(vec3<f32>(0.24, 0.27, 0.36), vec3<f32>(0.90, 0.93, 0.99), day);
    let c3 = mix(vec3<f32>(0.18, 0.21, 0.30), vec3<f32>(0.78, 0.84, 0.93), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
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
    let flash = st.x + st2.x;
    // Shockwave: an expanding ring from the primary strike's ground contact deforms the background.
    let asp = u.res.y / u.res.x;
    let impact = vec2<f32>(st.y, 0.70);
    let dvec = (uv - impact) * vec2<f32>(1.0, asp);
    let dist = length(dvec);
    let ring = smoothstep(0.09, 0.0, abs(dist - st.z * 1.1));
    let disp = normalize(dvec + vec2<f32>(0.0001, 0.0001)) * ring * st.x * 0.06;
    let duv = uv + disp;
    // Faster, higher-contrast churn (extra warp) + darker palette, sampled at the rippled coord.
    let w2 = warp(duv * 2.2 + vec2<f32>(t * 0.08, 0.0), t);
    let c0 = mix(vec3<f32>(0.04, 0.05, 0.09), vec3<f32>(0.20, 0.24, 0.32), day);
    let c1 = mix(vec3<f32>(0.07, 0.08, 0.13), vec3<f32>(0.30, 0.34, 0.42), day);
    let c2 = mix(vec3<f32>(0.05, 0.06, 0.11), vec3<f32>(0.24, 0.28, 0.36), day);
    let c3 = mix(vec3<f32>(0.02, 0.03, 0.07), vec3<f32>(0.14, 0.17, 0.24), day);
    var col = mesh_gradient(w2, t, c0, c1, c2, c3);
    // Volumetric storm cell, lit from inside by strikes (bolt x mapped into slab space).
    let rd = view_ray(uv);
    let sd = sun_dir(u.sun);
    let cp = CloudParams(0.85, 0.9, 0.14, 0.55, 22);
    let flash_pos = vec3<f32>((st.y - 0.5) * 5.0, 2.4, 7.0);
    let amb = mix(vec3<f32>(0.06, 0.07, 0.11), vec3<f32>(0.30, 0.33, 0.42), day);
    let cl = march_clouds(uv, rd, t, sd, sky.key, amb, cp, flash_pos, flash * 3.0);
    col = col * (1.0 - cl.a) + cl.rgb;
    // Driving rain sheets in two depths (storm-locked slant/speed).
    col = col + vec3<f32>(0.35, 0.40, 0.52) * rain_streaks(uv, t, 1.0, 0.10, 1.3) * 0.18;
    col = col + vec3<f32>(0.30, 0.34, 0.46) * rain_streaks(uv * vec2<f32>(1.6, 1.3), t * 0.6, 1.0, 0.08, 1.3) * 0.10;
    // Shockwave highlight + whole-sky flash (primary + double + distant sheet flashes).
    let df = moment(t, 0.5, 0.2, 12.0);
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
    let c0 = mix(vec3<f32>(0.16, 0.17, 0.19), vec3<f32>(0.66, 0.68, 0.71), day);
    let c1 = mix(vec3<f32>(0.19, 0.20, 0.22), vec3<f32>(0.74, 0.76, 0.79), day);
    let c2 = mix(vec3<f32>(0.17, 0.18, 0.20), vec3<f32>(0.70, 0.72, 0.75), day);
    let c3 = mix(vec3<f32>(0.14, 0.15, 0.17), vec3<f32>(0.62, 0.64, 0.67), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
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

// ---- Bottom-flowing silhouette (window alpha). Carried forward from M3. ----
fn silhouette_alpha(uv: vec2<f32>, t: f32, cond: i32, intensity: f32) -> f32 {
    let band_top = 0.82;
    if (uv.y < band_top) { return 1.0; }
    let x = uv.x;
    let b = (uv.y - band_top) / (1.0 - band_top);
    let amp = 0.10 + 0.10 * intensity;
    var edge = 0.4;
    var soft = 0.10;
    if (cond == 0) {
        edge = 0.42 + amp * sin(x * 8.0 + t * 0.8);
    } else if (cond == 1) {
        edge = 0.46 + amp * 0.7 * sin(x * 5.0 + t * 0.5);
    } else if (cond == 2) {
        let drip = fbm(vec2<f32>(x * 12.0, t * 0.6));
        edge = 0.30 + amp * 1.4 * drip;
        soft = 0.05;
    } else if (cond == 3) {
        edge = 0.42 + amp * 0.8 * abs(sin(x * 10.0 + t * 0.3));
        soft = 0.08;
    } else if (cond == 4) {
        // Broader, less needle-sharp churn for the storm edge.
        let j = fbm(vec2<f32>(x * 14.0 + t * 1.5, t));
        edge = 0.35 + amp * 1.2 * (j - 0.5) * 2.0;
        // Lightning strike jolts the window's bottom edge inward at the bolt's x — a soft,
        // rounded dip rather than a sharp notch.
        let st = storm_strike(t);
        let near = smoothstep(0.24, 0.0, abs(x - st.y));
        edge = edge - st.x * near * 0.42;
        soft = 0.06;
    } else {
        let n = fbm(vec2<f32>(x * 4.0 + t * 0.2, uv.y * 6.0));
        return clamp(1.0 - b * (0.7 + 0.6 * n), 0.0, 1.0);
    }
    return 1.0 - smoothstep(edge - soft, edge + soft, b);
}

// Rounded-window mask: softens the hard rectangle corners (a circle in uv is oval on the
// tall canvas, so distances are aspect-corrected to keep the radius round in pixels).
fn corner_alpha(uv: vec2<f32>) -> f32 {
    let asp = u.res.y / u.res.x;
    let p = (uv - vec2<f32>(0.5, 0.5)) * vec2<f32>(1.0, asp);
    let b = vec2<f32>(0.5, 0.5 * asp);
    let r = 0.065;   // corner radius (x-uv units ~ 26 px)
    let q = abs(p) - b + vec2<f32>(r, r);
    let sd = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
    return smoothstep(0.006, -0.006, sd);
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
        default: { col = clear_c(uv, t, sky, intensity); }
    }
    col = depth_grade(col, uv);
    col = grade(col, uv, t);   // tints + tone curve + vignette + grain
    col = col * ui_scrim(uv);
    let a = silhouette_alpha(uv, t, cond, intensity) * corner_alpha(uv);
    return vec4<f32>(col * a, a);
}
