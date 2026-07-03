struct gl_DefaultUniformBlock {
    u_colorFront: vec4<f32>,
    u_colorBack: vec4<f32>,
    u_shape: f32,
    u_frequency: f32,
    u_amplitude: f32,
    u_spacing: f32,
    u_proportion: f32,
    u_softness: f32,
}

var<private> v_patternUV_1: vec2<f32>;
@group(0) @binding(0) 
var<uniform> unnamed: gl_DefaultUniformBlock;
var<private> fragColor: vec4<f32>;

fn main_1() {
    var shape_uv: vec2<f32>;
    var wave: f32;
    var zigzag: f32;
    var irregular: f32;
    var irregular2_: f32;
    var offset: f32;
    var spacing: f32;
    var shape: f32;
    var aa: f32;
    var dc: f32;
    var e0_: f32;
    var e1_: f32;
    var res: f32;
    var fgColor: vec3<f32>;
    var fgOpacity: f32;
    var bgColor: vec3<f32>;
    var bgOpacity: f32;
    var color: vec3<f32>;
    var opacity: f32;

    let _e45 = v_patternUV_1;
    shape_uv = _e45;
    let _e46 = shape_uv;
    shape_uv = (_e46 * 4f);
    let _e49 = shape_uv[0u];
    let _e51 = unnamed.u_frequency;
    wave = (0.5f * cos(((_e49 * _e51) * 6.2831855f)));
    let _e57 = shape_uv[0u];
    let _e59 = unnamed.u_frequency;
    zigzag = (2f * abs((fract((_e57 * _e59)) - 0.5f)));
    let _e66 = shape_uv[0u];
    let _e69 = unnamed.u_frequency;
    let _e74 = shape_uv[0u];
    let _e76 = unnamed.u_frequency;
    irregular = (sin((((_e66 * 0.25f) * _e69) * 6.2831855f)) * cos(((_e74 * _e76) * 6.2831855f)));
    let _e82 = shape_uv[0u];
    let _e84 = unnamed.u_frequency;
    let _e89 = shape_uv[0u];
    let _e92 = unnamed.u_frequency;
    irregular2_ = (0.75f * (sin(((_e82 * _e84) * 6.2831855f)) + (0.5f * cos((((_e89 * 0.5f) * _e92) * 6.2831855f)))));
    let _e99 = zigzag;
    let _e100 = wave;
    let _e102 = unnamed.u_shape;
    offset = mix(_e99, _e100, smoothstep(0f, 1f, _e102));
    let _e105 = offset;
    let _e106 = irregular;
    let _e108 = unnamed.u_shape;
    offset = mix(_e105, _e106, smoothstep(1f, 2f, _e108));
    let _e111 = offset;
    let _e112 = irregular2_;
    let _e114 = unnamed.u_shape;
    offset = mix(_e111, _e112, smoothstep(2f, 3f, _e114));
    let _e118 = unnamed.u_amplitude;
    let _e120 = offset;
    offset = (_e120 * (2f * _e118));
    let _e123 = unnamed.u_spacing;
    spacing = (0.001f + _e123);
    let _e126 = shape_uv[1u];
    let _e127 = offset;
    let _e130 = spacing;
    shape = (0.5f + (0.5f * sin((((_e126 + _e127) * 3.1415927f) / _e130))));
    let _e135 = shape;
    let _e136 = fwidth(_e135);
    aa = (0.0001f + _e136);
    let _e139 = unnamed.u_proportion;
    dc = (1f - clamp(_e139, 0f, 1f));
    let _e142 = dc;
    let _e144 = unnamed.u_softness;
    let _e146 = aa;
    e0_ = ((_e142 - _e144) - _e146);
    let _e148 = dc;
    let _e150 = unnamed.u_softness;
    let _e152 = aa;
    e1_ = ((_e148 + _e150) + _e152);
    let _e154 = e0_;
    let _e155 = e1_;
    let _e157 = e0_;
    let _e158 = e1_;
    let _e160 = shape;
    res = smoothstep(min(_e154, _e155), max(_e157, _e158), _e160);
    let _e163 = unnamed.u_colorFront;
    let _e167 = unnamed.u_colorFront[3u];
    fgColor = (_e163.xyz * _e167);
    let _e171 = unnamed.u_colorFront[3u];
    fgOpacity = _e171;
    let _e173 = unnamed.u_colorBack;
    let _e177 = unnamed.u_colorBack[3u];
    bgColor = (_e173.xyz * _e177);
    let _e181 = unnamed.u_colorBack[3u];
    bgOpacity = _e181;
    let _e182 = fgColor;
    let _e183 = res;
    color = (_e182 * _e183);
    let _e185 = fgOpacity;
    let _e186 = res;
    opacity = (_e185 * _e186);
    let _e188 = bgColor;
    let _e189 = opacity;
    let _e192 = color;
    color = (_e192 + (_e188 * (1f - _e189)));
    let _e194 = bgOpacity;
    let _e195 = opacity;
    let _e198 = opacity;
    opacity = (_e198 + (_e194 * (1f - _e195)));
    let _e200 = color;
    let _e201 = opacity;
    fragColor = vec4<f32>(_e200.x, _e200.y, _e200.z, _e201);
    return;
}

@fragment 
fn main(@location(0) v_patternUV: vec2<f32>) -> @location(0) vec4<f32> {
    v_patternUV_1 = v_patternUV;
    main_1();
    let _e3 = fragColor;
    return _e3;
}
