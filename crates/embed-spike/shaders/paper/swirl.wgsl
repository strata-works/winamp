struct gl_DefaultUniformBlock {
    u_time: f32,
    u_colorBack: vec4<f32>,
    u_colors: array<vec4<f32>, 10>,
    u_colorsCount: f32,
    u_bandCount: f32,
    u_twist: f32,
    u_center: f32,
    u_proportion: f32,
    u_softness: f32,
    u_noise: f32,
    u_noiseFrequency: f32,
}

var<private> v_objectUV_1: vec2<f32>;
@group(0) @binding(0) 
var<uniform> unnamed: gl_DefaultUniformBlock;
var<private> gl_FragCoord_1: vec4<f32>;
var<private> fragColor: vec4<f32>;

fn permute_u0028_vf3_u003b(x: ptr<function, vec3<f32>>) -> vec3<f32> {
    let _e56 = (*x);
    let _e60 = (*x);
    let _e61 = (((_e56 * 34f) + vec3(1f)) * _e60);
    let _e62 = vec3(289f);
    return (_e61 - (floor((_e61 / _e62)) * _e62));
}

fn snoise_u0028_vf2_u003b(v: ptr<function, vec2<f32>>) -> f32 {
    var i: vec2<f32>;
    var x0_: vec2<f32>;
    var i1_: vec2<f32>;
    var x12_: vec4<f32>;
    var p: vec3<f32>;
    var param: vec3<f32>;
    var param_1: vec3<f32>;
    var m: vec3<f32>;
    var x_1: vec3<f32>;
    var h: vec3<f32>;
    var ox: vec3<f32>;
    var a0_: vec3<f32>;
    var g: vec3<f32>;

    let _e69 = (*v);
    let _e70 = (*v);
    i = floor((_e69 + vec2(dot(_e70, vec2<f32>(0.36602542f, 0.36602542f)))));
    let _e75 = (*v);
    let _e76 = i;
    let _e78 = i;
    x0_ = ((_e75 - _e76) + vec2(dot(_e78, vec2<f32>(0.21132487f, 0.21132487f))));
    let _e83 = x0_[0u];
    let _e85 = x0_[1u];
    i1_ = select(vec2<f32>(0f, 1f), vec2<f32>(1f, 0f), vec2((_e83 > _e85)));
    let _e89 = x0_;
    x12_ = (_e89.xyxy + vec4<f32>(0.21132487f, 0.21132487f, -0.57735026f, -0.57735026f));
    let _e92 = i1_;
    let _e93 = x12_;
    let _e95 = (_e93.xy - _e92);
    x12_[0u] = _e95.x;
    x12_[1u] = _e95.y;
    let _e100 = i;
    let _e101 = vec2(289f);
    i = (_e100 - (floor((_e100 / _e101)) * _e101));
    let _e107 = i[1u];
    let _e109 = i1_[1u];
    param = (vec3(_e107) + vec3<f32>(0f, _e109, 1f));
    let _e113 = permute_u0028_vf3_u003b((&param));
    let _e115 = i[0u];
    let _e119 = i1_[0u];
    param_1 = ((_e113 + vec3(_e115)) + vec3<f32>(0f, _e119, 1f));
    let _e122 = permute_u0028_vf3_u003b((&param_1));
    p = _e122;
    let _e123 = x0_;
    let _e124 = x0_;
    let _e126 = x12_;
    let _e128 = x12_;
    let _e131 = x12_;
    let _e133 = x12_;
    m = max((vec3(0.5f) - vec3<f32>(dot(_e123, _e124), dot(_e126.xy, _e128.xy), dot(_e131.zw, _e133.zw))), vec3(0f));
    let _e141 = m;
    let _e142 = m;
    m = (_e141 * _e142);
    let _e144 = m;
    let _e145 = m;
    m = (_e144 * _e145);
    let _e147 = p;
    x_1 = ((fract((_e147 * vec3<f32>(0.024390243f, 0.024390243f, 0.024390243f))) * 2f) - vec3(1f));
    let _e153 = x_1;
    h = (abs(_e153) - vec3(0.5f));
    let _e157 = x_1;
    ox = floor((_e157 + vec3(0.5f)));
    let _e161 = x_1;
    let _e162 = ox;
    a0_ = (_e161 - _e162);
    let _e164 = a0_;
    let _e165 = a0_;
    let _e167 = h;
    let _e168 = h;
    let _e174 = m;
    m = (_e174 * (vec3(1.7928429f) - (((_e164 * _e165) + (_e167 * _e168)) * 0.85373473f)));
    let _e177 = a0_[0u];
    let _e179 = x0_[0u];
    let _e182 = h[0u];
    let _e184 = x0_[1u];
    g[0u] = ((_e177 * _e179) + (_e182 * _e184));
    let _e188 = a0_;
    let _e190 = x12_;
    let _e193 = h;
    let _e195 = x12_;
    let _e198 = ((_e188.yz * _e190.xz) + (_e193.yz * _e195.yw));
    g[1u] = _e198.x;
    g[2u] = _e198.y;
    let _e203 = m;
    let _e204 = g;
    return (130f * dot(_e203, _e204));
}

fn main_1() {
    var shape_uv: vec2<f32>;
    var l: f32;
    var t: f32;
    var angle: f32;
    var angle_norm: f32;
    var twist: f32;
    var offset: f32;
    var shape: f32;
    var param_2: vec2<f32>;
    var mid: f32;
    var proportion: f32;
    var exponent: f32;
    var mixer: f32;
    var gradient: vec4<f32>;
    var outerShape: f32;
    var i_1: i32;
    var m_1: f32;
    var aa: f32;
    var c: vec4<f32>;
    var midAA: f32;
    var outerMid: f32;
    var color: vec3<f32>;
    var opacity: f32;
    var bgColor: vec3<f32>;

    let _e79 = v_objectUV_1;
    shape_uv = _e79;
    let _e80 = shape_uv;
    l = length(_e80);
    let _e82 = l;
    l = max(0.0001f, _e82);
    let _e85 = unnamed.u_time;
    t = _e85;
    let _e87 = unnamed.u_bandCount;
    let _e90 = shape_uv[1u];
    let _e92 = shape_uv[0u];
    let _e95 = t;
    angle = ((ceil(_e87) * atan2(_e90, _e92)) + _e95);
    let _e97 = angle;
    angle_norm = (_e97 / 6.2831855f);
    let _e100 = unnamed.u_twist;
    twist = (3f * clamp(_e100, 0f, 1f));
    let _e103 = l;
    let _e104 = twist;
    let _e107 = angle_norm;
    offset = (pow(_e103, -(_e104)) + _e107);
    let _e109 = offset;
    shape = fract(_e109);
    let _e111 = shape;
    shape = (1f - abs(((2f * _e111) - 1f)));
    let _e117 = unnamed.u_noise;
    let _e119 = unnamed.u_noiseFrequency;
    let _e122 = shape_uv;
    param_2 = (_e122 * (15f * pow(_e119, 2f)));
    let _e124 = snoise_u0028_vf2_u003b((&param_2));
    let _e126 = shape;
    shape = (_e126 + (_e117 * _e124));
    let _e129 = unnamed.u_center;
    let _e132 = l;
    let _e133 = twist;
    mid = smoothstep(0.2f, (0.2f + (0.8f * _e129)), pow(_e132, _e133));
    let _e136 = shape;
    let _e137 = mid;
    shape = mix(0f, _e136, _e137);
    let _e140 = unnamed.u_proportion;
    proportion = clamp(_e140, 0f, 1f);
    let _e142 = proportion;
    exponent = mix(0.25f, 1f, (_e142 * 2f));
    let _e145 = exponent;
    let _e146 = proportion;
    exponent = mix(_e145, 10f, max(0f, ((_e146 * 2f) - 1f)));
    let _e151 = shape;
    let _e152 = exponent;
    shape = pow(_e151, _e152);
    let _e154 = shape;
    let _e156 = unnamed.u_colorsCount;
    mixer = (_e154 * _e156);
    let _e160 = unnamed.u_colors[0i];
    gradient = _e160;
    let _e162 = gradient[3u];
    let _e163 = gradient;
    let _e165 = (_e163.xyz * _e162);
    gradient[0u] = _e165.x;
    gradient[1u] = _e165.y;
    gradient[2u] = _e165.z;
    outerShape = 0f;
    i_1 = 1i;
    loop {
        let _e172 = i_1;
        if (_e172 < 11i) {
            let _e174 = i_1;
            let _e176 = unnamed.u_colorsCount;
            if (_e174 > i32(_e176)) {
                break;
            }
            let _e179 = mixer;
            let _e180 = i_1;
            m_1 = clamp((_e179 - f32((_e180 - 1i))), 0f, 1f);
            let _e185 = m_1;
            let _e186 = fwidth(_e185);
            aa = _e186;
            let _e188 = unnamed.u_softness;
            let _e191 = aa;
            let _e194 = unnamed.u_softness;
            let _e197 = aa;
            let _e199 = m_1;
            m_1 = smoothstep(((0.5f - (0.5f * _e188)) - _e191), ((0.5f + (0.5f * _e194)) + _e197), _e199);
            let _e201 = i_1;
            if (_e201 == 1i) {
                let _e203 = m_1;
                outerShape = _e203;
            }
            let _e204 = i_1;
            let _e208 = unnamed.u_colors[(_e204 - 1i)];
            c = _e208;
            let _e210 = c[3u];
            let _e211 = c;
            let _e213 = (_e211.xyz * _e210);
            c[0u] = _e213.x;
            c[1u] = _e213.y;
            c[2u] = _e213.z;
            let _e220 = gradient;
            let _e221 = c;
            let _e222 = m_1;
            gradient = mix(_e220, _e221, vec4(_e222));
            continue;
        } else {
            break;
        }
        continuing {
            let _e225 = i_1;
            i_1 = (_e225 + 1i);
        }
    }
    let _e227 = l;
    let _e228 = twist;
    let _e231 = fwidth(pow(_e227, -(_e228)));
    midAA = (0.1f * _e231);
    let _e233 = midAA;
    let _e235 = l;
    let _e236 = twist;
    outerMid = smoothstep(0.2f, (0.2f + _e233), pow(_e235, _e236));
    let _e239 = outerShape;
    let _e240 = outerMid;
    outerShape = mix(0f, _e239, _e240);
    let _e242 = gradient;
    let _e244 = outerShape;
    color = (_e242.xyz * _e244);
    let _e247 = gradient[3u];
    let _e248 = outerShape;
    opacity = (_e247 * _e248);
    let _e251 = unnamed.u_colorBack;
    let _e255 = unnamed.u_colorBack[3u];
    bgColor = (_e251.xyz * _e255);
    let _e257 = color;
    let _e258 = bgColor;
    let _e259 = opacity;
    color = (_e257 + (_e258 * (1f - _e259)));
    let _e263 = opacity;
    let _e266 = unnamed.u_colorBack[3u];
    let _e267 = opacity;
    opacity = (_e263 + (_e266 * (1f - _e267)));
    let _e271 = gl_FragCoord_1;
    let _e280 = color;
    color = (_e280 + vec3((0.00390625f * (fract((sin(dot((_e271.xy * 0.014f), vec2<f32>(12.9898f, 78.233f))) * 43758.547f)) - 0.5f))));
    let _e283 = color;
    let _e284 = opacity;
    fragColor = vec4<f32>(_e283.x, _e283.y, _e283.z, _e284);
    return;
}

@fragment 
fn main(@location(0) v_objectUV: vec2<f32>, @builtin(position) gl_FragCoord: vec4<f32>) -> @location(0) vec4<f32> {
    v_objectUV_1 = v_objectUV;
    gl_FragCoord_1 = gl_FragCoord;
    main_1();
    let _e5 = fragColor;
    return _e5;
}
