struct gl_DefaultUniformBlock {
    u_time: f32,
    u_colors: array<vec4<f32>, 10>,
    u_colorsCount: f32,
    u_distortion: f32,
    u_swirl: f32,
    u_grainMixer: f32,
    u_grainOverlay: f32,
}

var<private> v_objectUV_1: vec2<f32>;
@group(0) @binding(0) 
var<uniform> unnamed: gl_DefaultUniformBlock;
var<private> fragColor: vec4<f32>;

fn hash21_u0028_vf2_u003b(p: ptr<function, vec2<f32>>) -> f32 {
    let _e51 = (*p);
    (*p) = (fract((_e51 * vec2<f32>(0.3183099f, 0.3678794f))) + vec2(0.1f));
    let _e56 = (*p);
    let _e57 = (*p);
    let _e61 = (*p);
    (*p) = (_e61 + vec2(dot(_e56, (_e57 + vec2(19.19f)))));
    let _e65 = (*p)[0u];
    let _e67 = (*p)[1u];
    return fract((_e65 * _e67));
}

fn valueNoise_u0028_vf2_u003b(st: ptr<function, vec2<f32>>) -> f32 {
    var i: vec2<f32>;
    var f: vec2<f32>;
    var a: f32;
    var param: vec2<f32>;
    var b: f32;
    var param_1: vec2<f32>;
    var c: f32;
    var param_2: vec2<f32>;
    var d: f32;
    var param_3: vec2<f32>;
    var u: vec2<f32>;
    var x1_: f32;
    var x2_: f32;

    let _e64 = (*st);
    i = floor(_e64);
    let _e66 = (*st);
    f = fract(_e66);
    let _e68 = i;
    param = _e68;
    let _e69 = hash21_u0028_vf2_u003b((&param));
    a = _e69;
    let _e70 = i;
    param_1 = (_e70 + vec2<f32>(1f, 0f));
    let _e72 = hash21_u0028_vf2_u003b((&param_1));
    b = _e72;
    let _e73 = i;
    param_2 = (_e73 + vec2<f32>(0f, 1f));
    let _e75 = hash21_u0028_vf2_u003b((&param_2));
    c = _e75;
    let _e76 = i;
    param_3 = (_e76 + vec2<f32>(1f, 1f));
    let _e78 = hash21_u0028_vf2_u003b((&param_3));
    d = _e78;
    let _e79 = f;
    let _e80 = f;
    let _e82 = f;
    u = ((_e79 * _e80) * (vec2(3f) - (_e82 * 2f)));
    let _e87 = a;
    let _e88 = b;
    let _e90 = u[0u];
    x1_ = mix(_e87, _e88, _e90);
    let _e92 = c;
    let _e93 = d;
    let _e95 = u[0u];
    x2_ = mix(_e92, _e93, _e95);
    let _e97 = x1_;
    let _e98 = x2_;
    let _e100 = u[1u];
    return mix(_e97, _e98, _e100);
}

fn getPosition_u0028_i1_u003b_f1_u003b(i_1: ptr<function, i32>, t: ptr<function, f32>) -> vec2<f32> {
    var a_1: f32;
    var b_1: f32;
    var c_1: f32;
    var x: f32;
    var y: f32;

    let _e57 = (*i_1);
    a_1 = (f32(_e57) * 0.37f);
    let _e60 = (*i_1);
    b_1 = (0.6f + (fract((f32(_e60) / 3f)) * 0.9f));
    let _e66 = (*i_1);
    c_1 = (0.8f + fract((f32((_e66 + 1i)) / 4f)));
    let _e72 = (*t);
    let _e73 = b_1;
    let _e75 = a_1;
    x = sin(((_e72 * _e73) + _e75));
    let _e78 = (*t);
    let _e79 = c_1;
    let _e81 = a_1;
    y = cos(((_e78 * _e79) + (_e81 * 1.5f)));
    let _e85 = x;
    let _e86 = y;
    return (vec2(0.5f) + (vec2<f32>(_e85, _e86) * 0.5f));
}

fn rotate_u0028_vf2_u003b_f1_u003b(uv: ptr<function, vec2<f32>>, th: ptr<function, f32>) -> vec2<f32> {
    let _e52 = (*th);
    let _e54 = (*th);
    let _e56 = (*th);
    let _e59 = (*th);
    let _e64 = (*uv);
    return (mat2x2<f32>(vec2<f32>(cos(_e52), sin(_e54)), vec2<f32>(-(sin(_e56)), cos(_e59))) * _e64);
}

fn noise_u0028_vf2_u003b_vf2_u003b(n: ptr<function, vec2<f32>>, seedOffset: ptr<function, vec2<f32>>) -> f32 {
    var param_4: vec2<f32>;

    let _e53 = (*n);
    let _e54 = (*seedOffset);
    param_4 = (_e53 + _e54);
    let _e56 = valueNoise_u0028_vf2_u003b((&param_4));
    return _e56;
}

fn main_1() {
    var uv_1: vec2<f32>;
    var grainUV: vec2<f32>;
    var grain: f32;
    var param_5: vec2<f32>;
    var param_6: vec2<f32>;
    var mixerGrain: f32;
    var t_1: f32;
    var radius: f32;
    var center: f32;
    var i_2: f32;
    var uvRotated: vec2<f32>;
    var angle: f32;
    var param_7: vec2<f32>;
    var param_8: f32;
    var color: vec3<f32>;
    var opacity: f32;
    var totalWeight: f32;
    var i_3: i32;
    var pos: vec2<f32>;
    var param_9: i32;
    var param_10: f32;
    var colorFraction: vec3<f32>;
    var opacityFraction: f32;
    var dist: f32;
    var weight: f32;
    var grainOverlay: f32;
    var param_11: vec2<f32>;
    var param_12: f32;
    var param_13: vec2<f32>;
    var param_14: vec2<f32>;
    var param_15: f32;
    var param_16: vec2<f32>;
    var grainOverlayV: f32;
    var grainOverlayColor: vec3<f32>;
    var grainOverlayStrength: f32;

    let _e85 = v_objectUV_1;
    uv_1 = _e85;
    let _e86 = uv_1;
    uv_1 = (_e86 + vec2(0.5f));
    let _e89 = uv_1;
    grainUV = (_e89 * 1000f);
    let _e91 = grainUV;
    param_5 = _e91;
    param_6 = vec2<f32>(0f, 0f);
    let _e92 = noise_u0028_vf2_u003b_vf2_u003b((&param_5), (&param_6));
    grain = _e92;
    let _e94 = unnamed.u_grainMixer;
    let _e96 = grain;
    mixerGrain = ((0.4f * _e94) * (_e96 - 0.5f));
    let _e100 = unnamed.u_time;
    t_1 = (0.5f * (_e100 + 41.5f));
    let _e103 = uv_1;
    radius = smoothstep(0f, 1f, length((_e103 - vec2(0.5f))));
    let _e108 = radius;
    center = (1f - _e108);
    i_2 = 1f;
    loop {
        let _e110 = i_2;
        if (_e110 <= 2f) {
            let _e113 = unnamed.u_distortion;
            let _e114 = center;
            let _e116 = i_2;
            let _e118 = t_1;
            let _e119 = i_2;
            let _e122 = uv_1[1u];
            let _e128 = t_1;
            let _e130 = i_2;
            let _e133 = uv_1[1u];
            let _e140 = uv_1[0u];
            uv_1[0u] = (_e140 + ((((_e113 * _e114) / _e116) * sin((_e118 + ((_e119 * 0.4f) * smoothstep(0f, 1f, _e122))))) * cos(((0.2f * _e128) + ((_e130 * 2.4f) * smoothstep(0f, 1f, _e133))))));
            let _e144 = unnamed.u_distortion;
            let _e145 = center;
            let _e147 = i_2;
            let _e149 = t_1;
            let _e150 = i_2;
            let _e153 = uv_1[0u];
            let _e160 = uv_1[1u];
            uv_1[1u] = (_e160 + (((_e144 * _e145) / _e147) * cos((_e149 + ((_e150 * 2f) * smoothstep(0f, 1f, _e153))))));
            continue;
        } else {
            break;
        }
        continuing {
            let _e163 = i_2;
            i_2 = (_e163 + 1f);
        }
    }
    let _e165 = uv_1;
    uvRotated = _e165;
    let _e166 = uvRotated;
    uvRotated = (_e166 - vec2<f32>(0.5f, 0.5f));
    let _e169 = unnamed.u_swirl;
    let _e171 = radius;
    angle = ((3f * _e169) * _e171);
    let _e173 = angle;
    let _e175 = uvRotated;
    param_7 = _e175;
    param_8 = -(_e173);
    let _e176 = rotate_u0028_vf2_u003b_f1_u003b((&param_7), (&param_8));
    uvRotated = _e176;
    let _e177 = uvRotated;
    uvRotated = (_e177 + vec2<f32>(0.5f, 0.5f));
    color = vec3<f32>(0f, 0f, 0f);
    opacity = 0f;
    totalWeight = 0f;
    i_3 = 0i;
    loop {
        let _e179 = i_3;
        if (_e179 < 10i) {
            let _e181 = i_3;
            let _e183 = unnamed.u_colorsCount;
            if (_e181 >= i32(_e183)) {
                break;
            }
            let _e186 = i_3;
            param_9 = _e186;
            let _e187 = t_1;
            param_10 = _e187;
            let _e188 = getPosition_u0028_i1_u003b_f1_u003b((&param_9), (&param_10));
            let _e189 = mixerGrain;
            pos = (_e188 + vec2(_e189));
            let _e192 = i_3;
            let _e195 = unnamed.u_colors[_e192];
            let _e197 = i_3;
            let _e201 = unnamed.u_colors[_e197][3u];
            colorFraction = (_e195.xyz * _e201);
            let _e203 = i_3;
            let _e207 = unnamed.u_colors[_e203][3u];
            opacityFraction = _e207;
            let _e208 = uvRotated;
            let _e209 = pos;
            dist = length((_e208 - _e209));
            let _e212 = dist;
            dist = pow(_e212, 3.5f);
            let _e214 = dist;
            weight = (1f / (_e214 + 0.001f));
            let _e217 = colorFraction;
            let _e218 = weight;
            let _e220 = color;
            color = (_e220 + (_e217 * _e218));
            let _e222 = opacityFraction;
            let _e223 = weight;
            let _e225 = opacity;
            opacity = (_e225 + (_e222 * _e223));
            let _e227 = weight;
            let _e228 = totalWeight;
            totalWeight = (_e228 + _e227);
            continue;
        } else {
            break;
        }
        continuing {
            let _e230 = i_3;
            i_3 = (_e230 + 1i);
        }
    }
    let _e232 = totalWeight;
    let _e234 = color;
    color = (_e234 / vec3(max(0.0001f, _e232)));
    let _e237 = totalWeight;
    let _e239 = opacity;
    opacity = (_e239 / max(0.0001f, _e237));
    let _e241 = grainUV;
    param_11 = _e241;
    param_12 = 1f;
    let _e242 = rotate_u0028_vf2_u003b_f1_u003b((&param_11), (&param_12));
    param_13 = (_e242 + vec2<f32>(3f, 3f));
    let _e244 = valueNoise_u0028_vf2_u003b((&param_13));
    grainOverlay = _e244;
    let _e245 = grainOverlay;
    let _e246 = grainUV;
    param_14 = _e246;
    param_15 = 2f;
    let _e247 = rotate_u0028_vf2_u003b_f1_u003b((&param_14), (&param_15));
    param_16 = (_e247 + vec2<f32>(-1f, -1f));
    let _e249 = valueNoise_u0028_vf2_u003b((&param_16));
    grainOverlay = mix(_e245, _e249, 0.5f);
    let _e251 = grainOverlay;
    grainOverlay = pow(_e251, 1.3f);
    let _e253 = grainOverlay;
    grainOverlayV = ((_e253 * 2f) - 1f);
    let _e256 = grainOverlayV;
    grainOverlayColor = vec3(step(0f, _e256));
    let _e260 = unnamed.u_grainOverlay;
    let _e261 = grainOverlayV;
    grainOverlayStrength = (_e260 * abs(_e261));
    let _e264 = grainOverlayStrength;
    grainOverlayStrength = pow(_e264, 0.8f);
    let _e266 = color;
    let _e267 = grainOverlayColor;
    let _e268 = grainOverlayStrength;
    color = mix(_e266, _e267, vec3((0.35f * _e268)));
    let _e272 = grainOverlayStrength;
    let _e274 = opacity;
    opacity = (_e274 + (0.5f * _e272));
    let _e276 = opacity;
    opacity = clamp(_e276, 0f, 1f);
    let _e278 = color;
    let _e279 = opacity;
    fragColor = vec4<f32>(_e278.x, _e278.y, _e278.z, _e279);
    return;
}

@fragment 
fn main(@location(0) v_objectUV: vec2<f32>) -> @location(0) vec4<f32> {
    v_objectUV_1 = v_objectUV;
    main_1();
    let _e3 = fragColor;
    return _e3;
}
