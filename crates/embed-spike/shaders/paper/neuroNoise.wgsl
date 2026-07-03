struct gl_DefaultUniformBlock {
    u_time: f32,
    u_resolution: vec2<f32>,
    u_pixelRatio: f32,
    u_colorFront: vec4<f32>,
    u_colorMid: vec4<f32>,
    u_colorBack: vec4<f32>,
    u_brightness: f32,
    u_contrast: f32,
}

var<private> v_patternUV_1: vec2<f32>;
@group(0) @binding(0) 
var<uniform> unnamed: gl_DefaultUniformBlock;
var<private> gl_FragCoord_1: vec4<f32>;
var<private> fragColor: vec4<f32>;

fn rotate_u0028_vf2_u003b_f1_u003b(uv: ptr<function, vec2<f32>>, th: ptr<function, f32>) -> vec2<f32> {
    let _e34 = (*th);
    let _e36 = (*th);
    let _e38 = (*th);
    let _e41 = (*th);
    let _e46 = (*uv);
    return (mat2x2<f32>(vec2<f32>(cos(_e34), sin(_e36)), vec2<f32>(-(sin(_e38)), cos(_e41))) * _e46);
}

fn neuroShape_u0028_vf2_u003b_f1_u003b(uv_1: ptr<function, vec2<f32>>, t: ptr<function, f32>) -> f32 {
    var sine_acc: vec2<f32>;
    var res: vec2<f32>;
    var scale: f32;
    var j: i32;
    var param: vec2<f32>;
    var param_1: f32;
    var param_2: vec2<f32>;
    var param_3: f32;
    var layer: vec2<f32>;

    sine_acc = vec2<f32>(0f, 0f);
    res = vec2<f32>(0f, 0f);
    scale = 8f;
    j = 0i;
    loop {
        let _e43 = j;
        if (_e43 < 15i) {
            let _e45 = (*uv_1);
            param = _e45;
            param_1 = 1f;
            let _e46 = rotate_u0028_vf2_u003b_f1_u003b((&param), (&param_1));
            (*uv_1) = _e46;
            let _e47 = sine_acc;
            param_2 = _e47;
            param_3 = 1f;
            let _e48 = rotate_u0028_vf2_u003b_f1_u003b((&param_2), (&param_3));
            sine_acc = _e48;
            let _e49 = (*uv_1);
            let _e50 = scale;
            let _e52 = j;
            let _e56 = sine_acc;
            let _e58 = (*t);
            layer = ((((_e49 * _e50) + vec2(f32(_e52))) + _e56) - vec2(_e58));
            let _e61 = layer;
            let _e63 = sine_acc;
            sine_acc = (_e63 + sin(_e61));
            let _e65 = layer;
            let _e70 = scale;
            let _e73 = res;
            res = (_e73 + ((vec2(0.5f) + (cos(_e65) * 0.5f)) / vec2(_e70)));
            let _e75 = scale;
            scale = (_e75 * 1.2f);
            continue;
        } else {
            break;
        }
        continuing {
            let _e77 = j;
            j = (_e77 + 1i);
        }
    }
    let _e80 = res[0u];
    let _e82 = res[1u];
    return (_e80 + _e82);
}

fn main_1() {
    var shape_uv: vec2<f32>;
    var t_1: f32;
    var noise: f32;
    var param_4: vec2<f32>;
    var param_5: f32;
    var blend: f32;
    var frontC: vec4<f32>;
    var midC: vec4<f32>;
    var blendFront: vec4<f32>;
    var safeNoise: f32;
    var color: vec3<f32>;
    var opacity: f32;
    var bgColor: vec3<f32>;

    let _e45 = v_patternUV_1;
    shape_uv = _e45;
    let _e46 = shape_uv;
    shape_uv = (_e46 * 0.13f);
    let _e49 = unnamed.u_time;
    t_1 = (0.5f * _e49);
    let _e51 = shape_uv;
    param_4 = _e51;
    let _e52 = t_1;
    param_5 = _e52;
    let _e53 = neuroShape_u0028_vf2_u003b_f1_u003b((&param_4), (&param_5));
    noise = _e53;
    let _e55 = unnamed.u_brightness;
    let _e57 = noise;
    let _e59 = noise;
    noise = (((1f + _e55) * _e57) * _e59);
    let _e61 = noise;
    let _e63 = unnamed.u_contrast;
    noise = pow(_e61, (0.7f + (6f * _e63)));
    let _e67 = noise;
    noise = min(1.4f, _e67);
    let _e69 = noise;
    blend = smoothstep(0.7f, 1.4f, _e69);
    let _e72 = unnamed.u_colorFront;
    frontC = _e72;
    let _e74 = frontC[3u];
    let _e75 = frontC;
    let _e77 = (_e75.xyz * _e74);
    frontC[0u] = _e77.x;
    frontC[1u] = _e77.y;
    frontC[2u] = _e77.z;
    let _e85 = unnamed.u_colorMid;
    midC = _e85;
    let _e87 = midC[3u];
    let _e88 = midC;
    let _e90 = (_e88.xyz * _e87);
    midC[0u] = _e90.x;
    midC[1u] = _e90.y;
    midC[2u] = _e90.z;
    let _e97 = midC;
    let _e98 = frontC;
    let _e99 = blend;
    blendFront = mix(_e97, _e98, vec4(_e99));
    let _e102 = noise;
    safeNoise = max(_e102, 0f);
    let _e104 = blendFront;
    let _e106 = safeNoise;
    color = (_e104.xyz * _e106);
    let _e109 = blendFront[3u];
    let _e110 = safeNoise;
    opacity = clamp((_e109 * _e110), 0f, 1f);
    let _e114 = unnamed.u_colorBack;
    let _e118 = unnamed.u_colorBack[3u];
    bgColor = (_e114.xyz * _e118);
    let _e120 = color;
    let _e121 = bgColor;
    let _e122 = opacity;
    color = (_e120 + (_e121 * (1f - _e122)));
    let _e126 = opacity;
    let _e129 = unnamed.u_colorBack[3u];
    let _e130 = opacity;
    opacity = (_e126 + (_e129 * (1f - _e130)));
    let _e134 = gl_FragCoord_1;
    let _e143 = color;
    color = (_e143 + vec3((0.00390625f * (fract((sin(dot((_e134.xy * 0.014f), vec2<f32>(12.9898f, 78.233f))) * 43758.547f)) - 0.5f))));
    let _e146 = color;
    let _e147 = opacity;
    fragColor = vec4<f32>(_e146.x, _e146.y, _e146.z, _e147);
    return;
}

@fragment 
fn main(@location(0) v_patternUV: vec2<f32>, @builtin(position) gl_FragCoord: vec4<f32>) -> @location(0) vec4<f32> {
    v_patternUV_1 = v_patternUV;
    gl_FragCoord_1 = gl_FragCoord;
    main_1();
    let _e5 = fragColor;
    return _e5;
}
