struct gl_DefaultUniformBlock {
    u_resolution: vec2<f32>,
    u_pixelRatio: f32,
    u_imageAspectRatio: f32,
    u_originX: f32,
    u_originY: f32,
    u_worldWidth: f32,
    u_worldHeight: f32,
    u_fit: f32,
    u_scale: f32,
    u_rotation: f32,
    u_offsetX: f32,
    u_offsetY: f32,
}

struct gl_PerVertex {
    @builtin(position) gl_Position: vec4<f32>,
    gl_PointSize: f32,
}

struct VertexOutput {
    @builtin(position) gl_Position: vec4<f32>,
    @location(1) member: vec2<f32>,
    @location(0) member_1: vec2<f32>,
    @location(3) member_2: vec2<f32>,
    @location(2) member_3: vec2<f32>,
    @location(5) member_4: vec2<f32>,
    @location(4) member_5: vec2<f32>,
    @location(6) member_6: vec2<f32>,
}

@group(0) @binding(0) 
var<uniform> unnamed: gl_DefaultUniformBlock;
var<private> unnamed_1: gl_PerVertex = gl_PerVertex(vec4<f32>(0f, 0f, 0f, 1f), 1f);
var<private> a_position_1: vec4<f32>;
var<private> v_objectBoxSize: vec2<f32>;
var<private> v_objectUV: vec2<f32>;
var<private> v_responsiveBoxGivenSize: vec2<f32>;
var<private> v_responsiveUV: vec2<f32>;
var<private> v_patternBoxSize: vec2<f32>;
var<private> v_patternUV: vec2<f32>;
var<private> v_imageUV: vec2<f32>;
var<private> gl_VertexIndex_1: i32;
var<private> gl_InstanceIndex_1: i32;

fn getBoxSize_u0028_f1_u003b_vf2_u003b(boxRatio: ptr<function, f32>, givenBoxSize: ptr<function, vec2<f32>>) -> vec3<f32> {
    var box: vec2<f32>;
    var noFitBoxWidth: f32;

    box = vec2<f32>(0f, 0f);
    let _e41 = (*boxRatio);
    let _e43 = (*givenBoxSize)[0u];
    let _e44 = (*boxRatio);
    let _e47 = (*givenBoxSize)[1u];
    box[0u] = (_e41 * min((_e43 / _e44), _e47));
    let _e52 = box[0u];
    noFitBoxWidth = _e52;
    let _e54 = unnamed.u_fit;
    if (_e54 == 1f) {
        let _e56 = (*boxRatio);
        let _e59 = unnamed.u_resolution[0u];
        let _e60 = (*boxRatio);
        let _e64 = unnamed.u_resolution[1u];
        box[0u] = (_e56 * min((_e59 / _e60), _e64));
    } else {
        let _e69 = unnamed.u_fit;
        if (_e69 == 2f) {
            let _e71 = (*boxRatio);
            let _e74 = unnamed.u_resolution[0u];
            let _e75 = (*boxRatio);
            let _e79 = unnamed.u_resolution[1u];
            box[0u] = (_e71 * max((_e74 / _e75), _e79));
        }
    }
    let _e84 = box[0u];
    let _e85 = (*boxRatio);
    box[1u] = (_e84 / _e85);
    let _e88 = box;
    let _e89 = noFitBoxWidth;
    return vec3<f32>(_e88.x, _e88.y, _e89);
}

fn main_1() {
    var uv: vec2<f32>;
    var boxOrigin: vec2<f32>;
    var givenBoxSize_1: vec2<f32>;
    var r: f32;
    var graphicRotation: mat2x2<f32>;
    var graphicOffset: vec2<f32>;
    var fixedRatio: f32;
    var fixedRatioBoxGivenSize: vec2<f32>;
    var local: f32;
    var local_1: f32;
    var param: f32;
    var param_1: vec2<f32>;
    var objectWorldScale: vec2<f32>;
    var local_2: f32;
    var local_3: f32;
    var responsiveRatio: f32;
    var responsiveBoxSize: vec2<f32>;
    var param_2: f32;
    var param_3: vec2<f32>;
    var responsiveBoxScale: vec2<f32>;
    var patternBoxRatio: f32;
    var patternBoxGivenSize: vec2<f32>;
    var local_4: f32;
    var local_5: f32;
    var boxSizeData: vec3<f32>;
    var param_4: f32;
    var param_5: vec2<f32>;
    var patternBoxNoFitBoxWidth: f32;
    var patternBoxScale: vec2<f32>;
    var imageBoxSize: vec2<f32>;
    var imageBoxScale: vec2<f32>;

    let _e68 = a_position_1;
    unnamed_1.gl_Position = _e68;
    let _e71 = unnamed_1.gl_Position;
    uv = (_e71.xy * 0.5f);
    let _e75 = unnamed.u_originX;
    let _e78 = unnamed.u_originY;
    boxOrigin = vec2<f32>((0.5f - _e75), (_e78 - 0.5f));
    let _e82 = unnamed.u_worldWidth;
    let _e84 = unnamed.u_worldHeight;
    givenBoxSize_1 = vec2<f32>(_e82, _e84);
    let _e86 = givenBoxSize_1;
    let _e89 = unnamed.u_pixelRatio;
    givenBoxSize_1 = (max(_e86, vec2<f32>(1f, 1f)) * _e89);
    let _e92 = unnamed.u_rotation;
    r = ((_e92 * 3.1415927f) / 180f);
    let _e95 = r;
    let _e97 = r;
    let _e99 = r;
    let _e102 = r;
    graphicRotation = mat2x2<f32>(vec2<f32>(cos(_e95), sin(_e97)), vec2<f32>(-(sin(_e99)), cos(_e102)));
    let _e108 = unnamed.u_offsetX;
    let _e111 = unnamed.u_offsetY;
    graphicOffset = vec2<f32>(-(_e108), _e111);
    fixedRatio = 1f;
    let _e114 = unnamed.u_worldWidth;
    if (_e114 == 0f) {
        let _e118 = unnamed.u_resolution[0u];
        local = _e118;
    } else {
        let _e120 = givenBoxSize_1[0u];
        local = _e120;
    }
    let _e121 = local;
    let _e123 = unnamed.u_worldHeight;
    if (_e123 == 0f) {
        let _e127 = unnamed.u_resolution[1u];
        local_1 = _e127;
    } else {
        let _e129 = givenBoxSize_1[1u];
        local_1 = _e129;
    }
    let _e130 = local_1;
    fixedRatioBoxGivenSize = vec2<f32>(_e121, _e130);
    let _e132 = fixedRatio;
    param = _e132;
    let _e133 = fixedRatioBoxGivenSize;
    param_1 = _e133;
    let _e134 = getBoxSize_u0028_f1_u003b_vf2_u003b((&param), (&param_1));
    v_objectBoxSize = _e134.xy;
    let _e137 = unnamed.u_resolution;
    let _e138 = v_objectBoxSize;
    objectWorldScale = (_e137 / _e138);
    let _e140 = uv;
    v_objectUV = _e140;
    let _e141 = objectWorldScale;
    let _e142 = v_objectUV;
    v_objectUV = (_e142 * _e141);
    let _e144 = boxOrigin;
    let _e145 = objectWorldScale;
    let _e149 = v_objectUV;
    v_objectUV = (_e149 + (_e144 * (_e145 - vec2(1f))));
    let _e151 = graphicOffset;
    let _e152 = v_objectUV;
    v_objectUV = (_e152 + _e151);
    let _e155 = unnamed.u_scale;
    let _e156 = v_objectUV;
    v_objectUV = (_e156 / vec2(_e155));
    let _e159 = graphicRotation;
    let _e160 = v_objectUV;
    v_objectUV = (_e159 * _e160);
    let _e163 = unnamed.u_worldWidth;
    if (_e163 == 0f) {
        let _e167 = unnamed.u_resolution[0u];
        local_2 = _e167;
    } else {
        let _e169 = givenBoxSize_1[0u];
        local_2 = _e169;
    }
    let _e170 = local_2;
    let _e172 = unnamed.u_worldHeight;
    if (_e172 == 0f) {
        let _e176 = unnamed.u_resolution[1u];
        local_3 = _e176;
    } else {
        let _e178 = givenBoxSize_1[1u];
        local_3 = _e178;
    }
    let _e179 = local_3;
    v_responsiveBoxGivenSize = vec2<f32>(_e170, _e179);
    let _e182 = v_responsiveBoxGivenSize[0u];
    let _e184 = v_responsiveBoxGivenSize[1u];
    responsiveRatio = (_e182 / _e184);
    let _e186 = responsiveRatio;
    param_2 = _e186;
    let _e187 = v_responsiveBoxGivenSize;
    param_3 = _e187;
    let _e188 = getBoxSize_u0028_f1_u003b_vf2_u003b((&param_2), (&param_3));
    responsiveBoxSize = _e188.xy;
    let _e191 = unnamed.u_resolution;
    let _e192 = responsiveBoxSize;
    responsiveBoxScale = (_e191 / _e192);
    let _e194 = uv;
    v_responsiveUV = _e194;
    let _e195 = responsiveBoxScale;
    let _e196 = v_responsiveUV;
    v_responsiveUV = (_e196 * _e195);
    let _e198 = boxOrigin;
    let _e199 = responsiveBoxScale;
    let _e203 = v_responsiveUV;
    v_responsiveUV = (_e203 + (_e198 * (_e199 - vec2(1f))));
    let _e205 = graphicOffset;
    let _e206 = v_responsiveUV;
    v_responsiveUV = (_e206 + _e205);
    let _e209 = unnamed.u_scale;
    let _e210 = v_responsiveUV;
    v_responsiveUV = (_e210 / vec2(_e209));
    let _e213 = responsiveRatio;
    let _e215 = v_responsiveUV[0u];
    v_responsiveUV[0u] = (_e215 * _e213);
    let _e218 = graphicRotation;
    let _e219 = v_responsiveUV;
    v_responsiveUV = (_e218 * _e219);
    let _e221 = responsiveRatio;
    let _e223 = v_responsiveUV[0u];
    v_responsiveUV[0u] = (_e223 / _e221);
    let _e227 = givenBoxSize_1[0u];
    let _e229 = givenBoxSize_1[1u];
    patternBoxRatio = (_e227 / _e229);
    let _e232 = unnamed.u_worldWidth;
    if (_e232 == 0f) {
        let _e236 = unnamed.u_resolution[0u];
        local_4 = _e236;
    } else {
        let _e238 = givenBoxSize_1[0u];
        local_4 = _e238;
    }
    let _e239 = local_4;
    let _e241 = unnamed.u_worldHeight;
    if (_e241 == 0f) {
        let _e245 = unnamed.u_resolution[1u];
        local_5 = _e245;
    } else {
        let _e247 = givenBoxSize_1[1u];
        local_5 = _e247;
    }
    let _e248 = local_5;
    patternBoxGivenSize = vec2<f32>(_e239, _e248);
    let _e251 = patternBoxGivenSize[0u];
    let _e253 = patternBoxGivenSize[1u];
    patternBoxRatio = (_e251 / _e253);
    let _e255 = patternBoxRatio;
    param_4 = _e255;
    let _e256 = patternBoxGivenSize;
    param_5 = _e256;
    let _e257 = getBoxSize_u0028_f1_u003b_vf2_u003b((&param_4), (&param_5));
    boxSizeData = _e257;
    let _e258 = boxSizeData;
    v_patternBoxSize = _e258.xy;
    let _e261 = boxSizeData[2u];
    patternBoxNoFitBoxWidth = _e261;
    let _e263 = unnamed.u_resolution;
    let _e264 = v_patternBoxSize;
    patternBoxScale = (_e263 / _e264);
    let _e266 = uv;
    v_patternUV = _e266;
    let _e267 = graphicOffset;
    let _e268 = patternBoxScale;
    let _e270 = v_patternUV;
    v_patternUV = (_e270 + (_e267 / _e268));
    let _e272 = boxOrigin;
    let _e273 = v_patternUV;
    v_patternUV = (_e273 + _e272);
    let _e275 = boxOrigin;
    let _e276 = patternBoxScale;
    let _e278 = v_patternUV;
    v_patternUV = (_e278 - (_e275 / _e276));
    let _e281 = unnamed.u_resolution;
    let _e282 = v_patternUV;
    v_patternUV = (_e282 * _e281);
    let _e285 = unnamed.u_pixelRatio;
    let _e286 = v_patternUV;
    v_patternUV = (_e286 / vec2(_e285));
    let _e290 = unnamed.u_fit;
    if (_e290 > 0f) {
        let _e292 = patternBoxNoFitBoxWidth;
        let _e294 = v_patternBoxSize[0u];
        let _e296 = v_patternUV;
        v_patternUV = (_e296 * (_e292 / _e294));
    }
    let _e299 = unnamed.u_scale;
    let _e300 = v_patternUV;
    v_patternUV = (_e300 / vec2(_e299));
    let _e303 = graphicRotation;
    let _e304 = v_patternUV;
    v_patternUV = (_e303 * _e304);
    let _e306 = boxOrigin;
    let _e307 = patternBoxScale;
    let _e309 = v_patternUV;
    v_patternUV = (_e309 + (_e306 / _e307));
    let _e311 = boxOrigin;
    let _e312 = v_patternUV;
    v_patternUV = (_e312 - _e311);
    let _e314 = v_patternUV;
    v_patternUV = (_e314 * 0.01f);
    let _e317 = unnamed.u_fit;
    if (_e317 == 1f) {
        let _e321 = unnamed.u_resolution[0u];
        let _e323 = unnamed.u_imageAspectRatio;
        let _e327 = unnamed.u_resolution[1u];
        let _e330 = unnamed.u_imageAspectRatio;
        imageBoxSize[0u] = (min((_e321 / _e323), _e327) * _e330);
    } else {
        let _e334 = unnamed.u_fit;
        if (_e334 == 2f) {
            let _e338 = unnamed.u_resolution[0u];
            let _e340 = unnamed.u_imageAspectRatio;
            let _e344 = unnamed.u_resolution[1u];
            let _e347 = unnamed.u_imageAspectRatio;
            imageBoxSize[0u] = (max((_e338 / _e340), _e344) * _e347);
        } else {
            let _e351 = unnamed.u_imageAspectRatio;
            let _e354 = unnamed.u_imageAspectRatio;
            imageBoxSize[0u] = min(10f, ((10f / _e351) * _e354));
        }
    }
    let _e359 = imageBoxSize[0u];
    let _e361 = unnamed.u_imageAspectRatio;
    imageBoxSize[1u] = (_e359 / _e361);
    let _e365 = unnamed.u_resolution;
    let _e366 = imageBoxSize;
    imageBoxScale = (_e365 / _e366);
    let _e368 = uv;
    v_imageUV = _e368;
    let _e369 = imageBoxScale;
    let _e370 = v_imageUV;
    v_imageUV = (_e370 * _e369);
    let _e372 = boxOrigin;
    let _e373 = imageBoxScale;
    let _e377 = v_imageUV;
    v_imageUV = (_e377 + (_e372 * (_e373 - vec2(1f))));
    let _e379 = graphicOffset;
    let _e380 = v_imageUV;
    v_imageUV = (_e380 + _e379);
    let _e383 = unnamed.u_scale;
    let _e384 = v_imageUV;
    v_imageUV = (_e384 / vec2(_e383));
    let _e388 = unnamed.u_imageAspectRatio;
    let _e390 = v_imageUV[0u];
    v_imageUV[0u] = (_e390 * _e388);
    let _e393 = graphicRotation;
    let _e394 = v_imageUV;
    v_imageUV = (_e393 * _e394);
    let _e397 = unnamed.u_imageAspectRatio;
    let _e399 = v_imageUV[0u];
    v_imageUV[0u] = (_e399 / _e397);
    let _e402 = v_imageUV;
    v_imageUV = (_e402 + vec2(0.5f));
    let _e406 = v_imageUV[1u];
    v_imageUV[1u] = (1f - _e406);
    return;
}

@vertex 
fn main(@location(0) a_position: vec4<f32>, @builtin(vertex_index) gl_VertexIndex: u32, @builtin(instance_index) gl_InstanceIndex: u32) -> VertexOutput {
    a_position_1 = a_position;
    gl_VertexIndex_1 = i32(gl_VertexIndex);
    gl_InstanceIndex_1 = i32(gl_InstanceIndex);
    main_1();
    let _e18 = unnamed_1.gl_Position.y;
    unnamed_1.gl_Position.y = -(_e18);
    let _e20 = unnamed_1.gl_Position;
    let _e21 = v_objectBoxSize;
    let _e22 = v_objectUV;
    let _e23 = v_responsiveBoxGivenSize;
    let _e24 = v_responsiveUV;
    let _e25 = v_patternBoxSize;
    let _e26 = v_patternUV;
    let _e27 = v_imageUV;
    return VertexOutput(_e20, _e21, _e22, _e23, _e24, _e25, _e26, _e27);
}
