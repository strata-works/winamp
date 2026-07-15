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

// Shared directional light (sun by day / moon by night), drifting slightly.
fn light_pos(t: f32) -> vec2<f32> { return vec2<f32>(0.72, 0.26 + 0.02 * sin(t * 0.3)); }

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
fn clear_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.04, 0.05, 0.16), vec3<f32>(0.26, 0.52, 0.92), day);
    let c1 = mix(vec3<f32>(0.06, 0.08, 0.22), vec3<f32>(0.40, 0.66, 0.97), day);
    let c2 = mix(vec3<f32>(0.10, 0.09, 0.22), vec3<f32>(0.70, 0.83, 0.98), day);
    let c3 = mix(vec3<f32>(0.16, 0.11, 0.20), vec3<f32>(0.98, 0.86, 0.68), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    // Night stars.
    col = col + vec3<f32>(0.9, 0.92, 1.0) * stars(uv, t) * (1.0 - day) * 0.7;
    // Sun (day) / moon (night), aspect-corrected so the disc is round on the tall canvas.
    let lp = light_pos(t);
    let asp = u.res.y / u.res.x;
    let pd = length((uv - lp) * vec2<f32>(1.0, asp));
    let disc = smoothstep(0.070, 0.045, pd);
    let glow = smoothstep(0.40, 0.0, pd) * mix(0.16, 0.24, day);
    let discCol = mix(vec3<f32>(0.82, 0.87, 1.0), vec3<f32>(1.0, 0.94, 0.74), day);
    col = col + discCol * (disc + glow);
    // Volumetric god-rays from the light (subtle; mostly a daytime effect).
    col = col + discCol * god_rays(uv, lp, t) * (0.10 + 0.20 * day);
    return col;
}
fn cloud_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.10, 0.11, 0.16), vec3<f32>(0.55, 0.62, 0.74), day);
    let c1 = mix(vec3<f32>(0.13, 0.14, 0.20), vec3<f32>(0.68, 0.73, 0.82), day);
    let c2 = mix(vec3<f32>(0.16, 0.17, 0.23), vec3<f32>(0.80, 0.83, 0.89), day);
    let c3 = mix(vec3<f32>(0.12, 0.13, 0.18), vec3<f32>(0.60, 0.66, 0.78), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    let lp = light_pos(t);
    let litd = mix(vec3<f32>(0.20, 0.22, 0.28), vec3<f32>(0.92, 0.94, 0.98), day);
    let shad = mix(vec3<f32>(0.10, 0.11, 0.15), vec3<f32>(0.52, 0.56, 0.64), day);
    // Three parallax planes at increasing scale/speed/coverage.
    for (var k = 0; k < 3; k = k + 1) {
        let fk = f32(k);
        let sc = 2.0 + fk * 1.6;
        let sp = 0.04 + fk * 0.03;
        let n = fbm(uv * vec2<f32>(sc, sc * 0.7) + vec2<f32>(t * sp, fk * 3.1));
        let cover = smoothstep(0.55, 0.85, n) * (0.35 + 0.25 * fk) * (0.6 + 0.5 * intensity);
        // Fake lighting: brighter toward the light side.
        let lit = mix(shad, litd, clamp(0.5 + (lp.x - uv.x) * 0.8, 0.0, 1.0));
        col = mix(col, lit, cover);
    }
    return col;
}
// Falling rain: thin, slightly-diagonal streaks scrolling down, per-column randomized,
// broken into short segments so they read as rain rather than static pinstripes.
fn rain_streaks(uv: vec2<f32>, t: f32, intensity: f32) -> f32 {
    let slant = uv + vec2<f32>(uv.y * 0.06, 0.0);
    let cols = 55.0;
    let x = slant.x * cols;
    let col = floor(x);
    let fx = fract(x) - 0.5;
    let speed = 0.8 + hash21(vec2<f32>(col, 1.0)) * 1.0;
    // -t so streaks scroll DOWN (uv.y=0 is the top of the canvas).
    let y = fract(uv.y * 3.4 - t * speed + hash21(vec2<f32>(col, 3.0)));
    let line = smoothstep(0.09, 0.0, abs(fx));
    let head = smoothstep(0.85, 0.30, y) * smoothstep(0.0, 0.10, y);
    return line * head * (0.4 + 0.7 * intensity);
}
// One parallax snow layer: soft round flakes on a drifting grid.
// -t*speed so flakes fall DOWN (uv.y=0 is the top of the canvas).
fn snow_layer(uv: vec2<f32>, t: f32, scale: f32, speed: f32, seed: f32) -> f32 {
    let p = uv * scale + vec2<f32>(sin(t * 0.3 + seed) * 0.5, -t * speed);
    let g = floor(p);
    let f = fract(p) - 0.5;
    let h = hash21(g + seed);
    return smoothstep(0.14, 0.0, length(f)) * step(0.86, h);
}
fn rain_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.06, 0.09, 0.14), vec3<f32>(0.30, 0.40, 0.52), day);
    let c1 = mix(vec3<f32>(0.08, 0.11, 0.17), vec3<f32>(0.38, 0.48, 0.60), day);
    let c2 = mix(vec3<f32>(0.10, 0.13, 0.19), vec3<f32>(0.46, 0.56, 0.68), day);
    let c3 = mix(vec3<f32>(0.05, 0.08, 0.13), vec3<f32>(0.28, 0.38, 0.50), day);
    // Slight refraction: sample the mesh at a streak-perturbed coord for a wet-glass warp.
    let streak = rain_streaks(uv, t, intensity);
    let ruv = uv + vec2<f32>(streak * 0.01, 0.0);
    var col = mesh_gradient(ruv, t, c0, c1, c2, c3);
    // Bright streak highlight.
    col = col + vec3<f32>(0.65, 0.74, 0.88) * streak * 0.3;
    // Overall wet sheen toward the bottom.
    col = col + vec3<f32>(0.10, 0.13, 0.18) * smoothstep(0.4, 1.0, uv.y) * (0.4 + 0.4 * day);
    // Ripples pooling near the silhouette band.
    let pool = smoothstep(0.78, 0.9, uv.y) * (0.5 + 0.5 * sin(uv.x * 40.0 - t * 4.0 + fbm(uv * 8.0) * 6.0));
    col = col + vec3<f32>(0.5, 0.6, 0.75) * pool * 0.12 * (0.5 + intensity);
    return col;
}
fn snow_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.16, 0.19, 0.28), vec3<f32>(0.74, 0.80, 0.90), day);
    let c1 = mix(vec3<f32>(0.20, 0.23, 0.32), vec3<f32>(0.82, 0.87, 0.95), day);
    let c2 = mix(vec3<f32>(0.24, 0.27, 0.36), vec3<f32>(0.90, 0.93, 0.99), day);
    let c3 = mix(vec3<f32>(0.18, 0.21, 0.30), vec3<f32>(0.78, 0.84, 0.93), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    // Far (small/sharp) -> near (big/soft) parallax flake layers.
    var flakes = 0.0;
    flakes = flakes + snow_layer(uv, t, 22.0, 0.10, 1.0) * 0.6;
    flakes = flakes + snow_layer(uv, t, 15.0, 0.16, 2.0) * 0.8;
    flakes = flakes + snow_layer(uv, t,  9.0, 0.24, 3.0) * 1.0;
    let bloom = mix(0.75, 1.0, day);
    col = col + vec3<f32>(1.0) * flakes * (0.35 + 0.4 * intensity) * bloom;
    return col;
}
// Shared lightning-strike state so the bolt, the background shockwave, and the window-edge
// jolt all fire together. Returns (flash_env, bolt_x, life, seed). env is 0 when no strike.
fn storm_strike(t: f32) -> vec4<f32> {
    let rate = 0.7;
    let seed = floor(t * rate);
    let life = fract(t * rate);
    let strike_on = step(0.72, hash21(vec2<f32>(seed, 11.0)));
    let env = strike_on * smoothstep(0.0, 0.04, life) * smoothstep(0.55, 0.08, life);
    let bx = 0.32 + 0.4 * hash21(vec2<f32>(seed, 7.0));
    return vec4<f32>(env, bx, life, seed);
}
// Forked lightning: a noise-perturbed vertical bolt + a branch fork, gated by the strike env.
fn lightning(uv: vec2<f32>, t: f32, st: vec4<f32>) -> f32 {
    let seed = st.w;
    let path = st.y + (fbm(vec2<f32>(uv.y * 4.5, seed)) - 0.5) * 0.28;
    let main = smoothstep(0.018, 0.0, abs(uv.x - path)) * step(uv.y, 0.72);
    let fork = smoothstep(0.013, 0.0, abs(uv.x - (path + (uv.y - 0.4) * 0.6)))
             * step(0.4, uv.y) * step(uv.y, 0.66);
    return st.x * (main + fork * 0.7);
}
// Rolling volumetric fog bands (two counter-scrolling fbm layers).
fn fog_banks(uv: vec2<f32>, t: f32) -> f32 {
    let n1 = fbm(uv * vec2<f32>(3.0, 1.6) + vec2<f32>(t * 0.06, 0.0));
    let n2 = fbm(uv * vec2<f32>(2.0, 1.1) + vec2<f32>(-t * 0.04, 1.7));
    return 0.5 * (n1 + n2);
}
fn storm_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let st = storm_strike(t);
    // Shockwave: an expanding ring from the strike's ground contact deforms the background.
    let asp = u.res.y / u.res.x;
    let impact = vec2<f32>(st.y, 0.70);
    let dvec = (uv - impact) * vec2<f32>(1.0, asp);
    let dist = length(dvec);
    let ringR = st.z * 1.1;
    let ring = smoothstep(0.09, 0.0, abs(dist - ringR));
    let disp = normalize(dvec + vec2<f32>(0.0001, 0.0001)) * ring * st.x * 0.06;
    let duv = uv + disp;
    // Faster, higher-contrast churn (extra warp) + darker palette, sampled at the rippled coord.
    let w2 = warp(duv * 2.2 + vec2<f32>(t * 0.08, 0.0), t);
    let c0 = mix(vec3<f32>(0.04, 0.05, 0.09), vec3<f32>(0.20, 0.24, 0.32), day);
    let c1 = mix(vec3<f32>(0.07, 0.08, 0.13), vec3<f32>(0.30, 0.34, 0.42), day);
    let c2 = mix(vec3<f32>(0.05, 0.06, 0.11), vec3<f32>(0.24, 0.28, 0.36), day);
    let c3 = mix(vec3<f32>(0.02, 0.03, 0.07), vec3<f32>(0.14, 0.17, 0.24), day);
    var col = mesh_gradient(w2, t, c0, c1, c2, c3);
    // Driving rain sheets.
    col = col + vec3<f32>(0.35, 0.40, 0.52) * rain_streaks(uv, t, 1.0) * 0.18;
    // Shockwave ring highlight + whole-sky ambient flash on strike.
    col = col + vec3<f32>(0.55, 0.60, 0.75) * ring * st.x * 0.5;
    col = col + vec3<f32>(0.60, 0.63, 0.78) * st.x * 0.18;
    // Forked lightning bolt.
    col = col + vec3<f32>(0.90, 0.92, 1.0) * lightning(uv, t, st);
    return col;
}
fn fog_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.16, 0.17, 0.19), vec3<f32>(0.66, 0.68, 0.71), day);
    let c1 = mix(vec3<f32>(0.19, 0.20, 0.22), vec3<f32>(0.74, 0.76, 0.79), day);
    let c2 = mix(vec3<f32>(0.17, 0.18, 0.20), vec3<f32>(0.70, 0.72, 0.75), day);
    let c3 = mix(vec3<f32>(0.14, 0.15, 0.17), vec3<f32>(0.62, 0.64, 0.67), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    let fogc = mix(vec3<f32>(0.55, 0.57, 0.60), vec3<f32>(0.86, 0.88, 0.90), day);
    let banks = fog_banks(uv, t);
    // Denser fog low and rolling; reduces visibility toward the horizon.
    let dens = clamp(banks * (0.5 + 0.7 * intensity) + smoothstep(0.3, 1.0, uv.y) * 0.35, 0.0, 0.85);
    col = mix(col, fogc, dens);
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

// Softly darkens the shader behind the 2D UI so text stays legible. The engine has
// no text-shadow/scrim primitive, so legibility is baked here. Zones (canvas 400x680,
// uv normalized): hero top-left, hourly strip band, daily column. Returns a luminance
// multiplier in [~0.5, 1.0].
fn ui_scrim(uv: vec2<f32>) -> f32 {
    var s = 1.0;
    // Hero block (top-left ~ x<0.62, y<0.30): strongest darkening.
    s = s - 0.42 * smoothstep(0.62, 0.30, uv.x) * smoothstep(0.34, 0.02, uv.y);
    // Hourly strip band (~ y 0.33..0.40, full width): gentle.
    s = s - 0.22 * smoothstep(0.32, 0.36, uv.y) * smoothstep(0.42, 0.38, uv.y);
    // Daily column labels + temps (left third and right third, y 0.45..0.82): gentle.
    let band = smoothstep(0.44, 0.48, uv.y) * smoothstep(0.83, 0.79, uv.y);
    s = s - 0.20 * band * (smoothstep(0.32, 0.06, uv.x) + smoothstep(0.68, 0.95, uv.x));
    return clamp(s, 0.5, 1.0);
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
    // Temporary until Task 2 introduces sky_grade: soft day factor from continuous elevation.
    let day = smoothstep(-0.12, 0.3, u.sun);
    let intensity = clamp(u.intensity, 0.0, 1.0);
    let cond = i32(u.condition);
    var col: vec3<f32>;
    switch (cond) {
        case 0: { col = clear_c(uv, t, day, intensity); }
        case 1: { col = cloud_c(uv, t, day, intensity); }
        case 2: { col = rain_c(uv, t, day, intensity); }
        case 3: { col = snow_c(uv, t, day, intensity); }
        case 4: { col = storm_c(uv, t, day, intensity); }
        case 5: { col = fog_c(uv, t, day, intensity); }
        default: { col = clear_c(uv, t, day, intensity); }
    }
    // Warm/cool tint from temperature (raw °C).
    let warmth = clamp((u.temp - 10.0) / 25.0, -0.3, 0.3);
    col = col + vec3<f32>(warmth, 0.0, -warmth) * 0.12;
    // Subtle season tint.
    col = mix(col, col * season_tint(u.season), 0.08);
    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));
    col = col * ui_scrim(uv);
    let a = silhouette_alpha(uv, t, cond, intensity) * corner_alpha(uv);
    return vec4<f32>(col * a, a);
}
