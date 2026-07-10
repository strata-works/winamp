// Hash/noise helpers (value noise).
fn hash21(p: vec2<f32>) -> f32 {
    var h = dot(p, vec2<f32>(127.1, 311.7));
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
    for (var k = 0; k < 4; k = k + 1) { v = v + amp * noise2(q); q = q * 2.0; amp = amp * 0.5; }
    return v;
}

// Sky gradient tinted by day/night.
fn sky(uv: vec2<f32>, day: f32) -> vec3<f32> {
    let top_day = vec3<f32>(0.30, 0.55, 0.9);
    let bot_day = vec3<f32>(0.75, 0.85, 0.95);
    let top_night = vec3<f32>(0.03, 0.04, 0.12);
    let bot_night = vec3<f32>(0.08, 0.10, 0.20);
    let top = mix(top_night, top_day, day);
    let bot = mix(bot_night, bot_day, day);
    return mix(top, bot, uv.y);
}

fn clear_c(uv: vec2<f32>, t: f32, day: f32) -> vec3<f32> {
    var col = sky(uv, day);
    // A soft sun/moon disc drifting slightly.
    let c = vec2<f32>(0.72, 0.24 + 0.02 * sin(t * 0.3));
    let d = distance(uv, c);
    let disc = smoothstep(0.14, 0.10, d);
    let glow = smoothstep(0.5, 0.0, d) * 0.35;
    let sun = mix(vec3<f32>(1.0, 0.95, 0.75), vec3<f32>(0.85, 0.9, 1.0), 1.0 - day);
    col = col + sun * (disc + glow);
    return col;
}

fn cloud_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = sky(uv, day) * 0.9;
    let n = fbm(uv * vec2<f32>(3.0, 2.0) + vec2<f32>(t * 0.05, 0.0));
    let cover = smoothstep(0.4, 0.8, n) * (0.5 + 0.5 * intensity);
    let cloud = mix(vec3<f32>(0.6, 0.62, 0.68), vec3<f32>(0.85, 0.87, 0.9), day);
    return mix(col, cloud, cover);
}

fn rain_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = cloud_c(uv, t, day, 0.8) * 0.8;
    // Streaks: repeated diagonal lines scrolling down.
    let sc = uv * vec2<f32>(60.0, 30.0) + vec2<f32>(uv.y * 8.0, -t * 12.0);
    let line = fract(sc.x + floor(sc.y) * 0.5);
    let streak = smoothstep(0.96, 1.0, 1.0 - abs(line - 0.5) * 2.0) * (0.3 + intensity);
    col = col + vec3<f32>(0.6, 0.7, 0.85) * streak * 0.25;
    return col;
}

fn snow_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = cloud_c(uv, t, day, 0.5) * vec3<f32>(0.9, 0.93, 1.0);
    var flakes = 0.0;
    for (var k = 0; k < 3; k = k + 1) {
        let fk = f32(k);
        let p = uv * (10.0 + fk * 6.0) + vec2<f32>(sin(t * 0.5 + fk) * 0.5, t * (0.15 + fk * 0.05));
        let g = hash21(floor(p));
        let f = fract(p) - 0.5;
        flakes = flakes + smoothstep(0.08, 0.0, length(f)) * step(0.85, g);
    }
    return col + vec3<f32>(1.0) * flakes * (0.4 + intensity);
}

fn storm_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = rain_c(uv, t, day, 1.0) * 0.5;
    // Occasional lightning flash: a fast pulse gating on a time hash.
    let strike = step(0.985, hash21(vec2<f32>(floor(t * 2.0), 3.0)));
    let flash = strike * (0.5 + 0.5 * sin(t * 40.0)) * 0.6;
    col = col + vec3<f32>(0.9, 0.9, 1.0) * flash;
    return col;
}

fn fog_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let base = mix(vec3<f32>(0.5, 0.52, 0.55), vec3<f32>(0.8, 0.82, 0.85), day);
    let n = fbm(uv * 2.0 + vec2<f32>(t * 0.03, t * 0.01));
    return mix(base * 0.9, base, n);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.time;
    let day = clamp(u.is_day, 0.0, 1.0);
    let intensity = clamp(u.intensity, 0.0, 1.0);
    var col: vec3<f32>;
    switch (i32(u.condition)) {
        case 0: { col = clear_c(uv, t, day); }
        case 1: { col = cloud_c(uv, t, day, intensity); }
        case 2: { col = rain_c(uv, t, day, intensity); }
        case 3: { col = snow_c(uv, t, day, intensity); }
        case 4: { col = storm_c(uv, t, day, intensity); }
        case 5: { col = fog_c(uv, t, day, intensity); }
        default: { col = clear_c(uv, t, day); }
    }
    // Warm/cool tint from temperature (raw °C): cold → blue, hot → amber.
    let warmth = clamp((u.temp - 10.0) / 25.0, -0.3, 0.3);
    col = col + vec3<f32>(warmth, 0.0, -warmth) * 0.15;
    // Opaque background (premultiplied by alpha = 1).
    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
