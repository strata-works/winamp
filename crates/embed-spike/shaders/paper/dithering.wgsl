struct gl_DefaultUniformBlock {
    u_time: f32,
    u_resolution: vec2<f32>,
    u_pixelRatio: f32,
    u_originX: f32,
    u_originY: f32,
    u_worldWidth: f32,
    u_worldHeight: f32,
    u_fit: f32,
    u_scale: f32,
    u_rotation: f32,
    u_offsetX: f32,
    u_offsetY: f32,
    u_pxSize: f32,
    u_colorBack: vec4<f32>,
    u_colorFront: vec4<f32>,
    u_shape: f32,
    u_type: f32,
}

@group(0) @binding(0) 
var<uniform> unnamed: gl_DefaultUniformBlock;
var<private> gl_FragCoord_1: vec4<f32>;
var<private> fragColor: vec4<f32>;

fn hash21_u0028_vf2_u003b(p: ptr<function, vec2<f32>>) -> f32 {
    let _e139 = (*p);
    (*p) = (fract((_e139 * vec2<f32>(0.3183099f, 0.3678794f))) + vec2(0.1f));
    let _e144 = (*p);
    let _e145 = (*p);
    let _e149 = (*p);
    (*p) = (_e149 + vec2(dot(_e144, (_e145 + vec2(19.19f)))));
    let _e153 = (*p)[0u];
    let _e155 = (*p)[1u];
    return fract((_e153 * _e155));
}

fn getBayerValue_u0028_vf2_u003b_i1_u003b(uv: ptr<function, vec2<f32>>, size: ptr<function, i32>) -> f32 {
    var pos: vec2<i32>;
    var index: i32;
    var indexable: array<i32, 4>;
    var indexable_1: array<i32, 16>;
    var indexable_2: array<i32, 64>;

    let _e145 = (*uv);
    let _e146 = (*size);
    let _e151 = (*size);
    pos = vec2<i32>((fract((_e145 / vec2(f32(_e146)))) * f32(_e151)));
    let _e156 = pos[1u];
    let _e157 = (*size);
    let _e160 = pos[0u];
    index = ((_e156 * _e157) + _e160);
    let _e162 = (*size);
    if (_e162 == 2i) {
        let _e164 = index;
        indexable = array<i32, 4>(0i, 2i, 3i, 1i);
        let _e166 = indexable[_e164];
        return (f32(_e166) / 4f);
    } else {
        let _e169 = (*size);
        if (_e169 == 4i) {
            let _e171 = index;
            indexable_1 = array<i32, 16>(0i, 8i, 2i, 10i, 12i, 4i, 14i, 6i, 3i, 11i, 1i, 9i, 15i, 7i, 13i, 5i);
            let _e173 = indexable_1[_e171];
            return (f32(_e173) / 16f);
        } else {
            let _e176 = (*size);
            if (_e176 == 8i) {
                let _e178 = index;
                indexable_2 = array<i32, 64>(0i, 32i, 8i, 40i, 2i, 34i, 10i, 42i, 48i, 16i, 56i, 24i, 50i, 18i, 58i, 26i, 12i, 44i, 4i, 36i, 14i, 46i, 6i, 38i, 60i, 28i, 52i, 20i, 62i, 30i, 54i, 22i, 3i, 35i, 11i, 43i, 1i, 33i, 9i, 41i, 51i, 19i, 59i, 27i, 49i, 17i, 57i, 25i, 15i, 47i, 7i, 39i, 13i, 45i, 5i, 37i, 63i, 31i, 55i, 23i, 61i, 29i, 53i, 21i);
                let _e180 = indexable_2[_e178];
                return (f32(_e180) / 64f);
            }
        }
    }
    return 0f;
}

fn hash11_u0028_f1_u003b(p_1: ptr<function, f32>) -> f32 {
    let _e139 = (*p_1);
    (*p_1) = (fract((_e139 * 0.3183099f)) + 0.1f);
    let _e143 = (*p_1);
    let _e145 = (*p_1);
    (*p_1) = (_e145 * (_e143 + 19.19f));
    let _e147 = (*p_1);
    let _e148 = (*p_1);
    return fract((_e147 * _e148));
}

fn permute_u0028_vf3_u003b(x: ptr<function, vec3<f32>>) -> vec3<f32> {
    let _e139 = (*x);
    let _e143 = (*x);
    let _e144 = (((_e139 * 34f) + vec3(1f)) * _e143);
    let _e145 = vec3(289f);
    return (_e144 - (floor((_e144 / _e145)) * _e145));
}

fn snoise_u0028_vf2_u003b(v: ptr<function, vec2<f32>>) -> f32 {
    var i: vec2<f32>;
    var x0_: vec2<f32>;
    var i1_: vec2<f32>;
    var x12_: vec4<f32>;
    var p_2: vec3<f32>;
    var param: vec3<f32>;
    var param_1: vec3<f32>;
    var m: vec3<f32>;
    var x_1: vec3<f32>;
    var h: vec3<f32>;
    var ox: vec3<f32>;
    var a0_: vec3<f32>;
    var g: vec3<f32>;

    let _e152 = (*v);
    let _e153 = (*v);
    i = floor((_e152 + vec2(dot(_e153, vec2<f32>(0.36602542f, 0.36602542f)))));
    let _e158 = (*v);
    let _e159 = i;
    let _e161 = i;
    x0_ = ((_e158 - _e159) + vec2(dot(_e161, vec2<f32>(0.21132487f, 0.21132487f))));
    let _e166 = x0_[0u];
    let _e168 = x0_[1u];
    i1_ = select(vec2<f32>(0f, 1f), vec2<f32>(1f, 0f), vec2((_e166 > _e168)));
    let _e172 = x0_;
    x12_ = (_e172.xyxy + vec4<f32>(0.21132487f, 0.21132487f, -0.57735026f, -0.57735026f));
    let _e175 = i1_;
    let _e176 = x12_;
    let _e178 = (_e176.xy - _e175);
    x12_[0u] = _e178.x;
    x12_[1u] = _e178.y;
    let _e183 = i;
    let _e184 = vec2(289f);
    i = (_e183 - (floor((_e183 / _e184)) * _e184));
    let _e190 = i[1u];
    let _e192 = i1_[1u];
    param = (vec3(_e190) + vec3<f32>(0f, _e192, 1f));
    let _e196 = permute_u0028_vf3_u003b((&param));
    let _e198 = i[0u];
    let _e202 = i1_[0u];
    param_1 = ((_e196 + vec3(_e198)) + vec3<f32>(0f, _e202, 1f));
    let _e205 = permute_u0028_vf3_u003b((&param_1));
    p_2 = _e205;
    let _e206 = x0_;
    let _e207 = x0_;
    let _e209 = x12_;
    let _e211 = x12_;
    let _e214 = x12_;
    let _e216 = x12_;
    m = max((vec3(0.5f) - vec3<f32>(dot(_e206, _e207), dot(_e209.xy, _e211.xy), dot(_e214.zw, _e216.zw))), vec3(0f));
    let _e224 = m;
    let _e225 = m;
    m = (_e224 * _e225);
    let _e227 = m;
    let _e228 = m;
    m = (_e227 * _e228);
    let _e230 = p_2;
    x_1 = ((fract((_e230 * vec3<f32>(0.024390243f, 0.024390243f, 0.024390243f))) * 2f) - vec3(1f));
    let _e236 = x_1;
    h = (abs(_e236) - vec3(0.5f));
    let _e240 = x_1;
    ox = floor((_e240 + vec3(0.5f)));
    let _e244 = x_1;
    let _e245 = ox;
    a0_ = (_e244 - _e245);
    let _e247 = a0_;
    let _e248 = a0_;
    let _e250 = h;
    let _e251 = h;
    let _e257 = m;
    m = (_e257 * (vec3(1.7928429f) - (((_e247 * _e248) + (_e250 * _e251)) * 0.85373473f)));
    let _e260 = a0_[0u];
    let _e262 = x0_[0u];
    let _e265 = h[0u];
    let _e267 = x0_[1u];
    g[0u] = ((_e260 * _e262) + (_e265 * _e267));
    let _e271 = a0_;
    let _e273 = x12_;
    let _e276 = h;
    let _e278 = x12_;
    let _e281 = ((_e271.yz * _e273.xz) + (_e276.yz * _e278.yw));
    g[1u] = _e281.x;
    g[2u] = _e281.y;
    let _e286 = m;
    let _e287 = g;
    return (130f * dot(_e286, _e287));
}

fn getSimplexNoise_u0028_vf2_u003b_f1_u003b(uv_1: ptr<function, vec2<f32>>, t: ptr<function, f32>) -> f32 {
    var noise: f32;
    var param_2: vec2<f32>;
    var param_3: vec2<f32>;

    let _e143 = (*uv_1);
    let _e144 = (*t);
    param_2 = (_e143 - vec2<f32>(0f, (0.3f * _e144)));
    let _e148 = snoise_u0028_vf2_u003b((&param_2));
    noise = (0.5f * _e148);
    let _e150 = (*uv_1);
    let _e152 = (*t);
    param_3 = ((_e150 * 2f) + vec2<f32>(0f, (0.32f * _e152)));
    let _e156 = snoise_u0028_vf2_u003b((&param_3));
    let _e158 = noise;
    noise = (_e158 + (0.5f * _e156));
    let _e160 = noise;
    return _e160;
}

fn main_1() {
    var t_1: f32;
    var pxSize: f32;
    var pxSizeUV: vec2<f32>;
    var canvasPixelizedUV: vec2<f32>;
    var normalizedUV: vec2<f32>;
    var ditheringNoiseUV: vec2<f32>;
    var shapeUV: vec2<f32>;
    var boxOrigin: vec2<f32>;
    var givenBoxSize: vec2<f32>;
    var r: f32;
    var graphicRotation: mat2x2<f32>;
    var graphicOffset: vec2<f32>;
    var patternBoxRatio: f32;
    var boxSize: vec2<f32>;
    var local: f32;
    var local_1: f32;
    var objectBoxSize: vec2<f32>;
    var objectWorldScale: vec2<f32>;
    var patternBoxSize: vec2<f32>;
    var patternWorldNoFitBoxWidth: f32;
    var patternWorldScale: vec2<f32>;
    var shape: f32;
    var param_4: vec2<f32>;
    var param_5: f32;
    var i_1: f32;
    var stripeIdx: f32;
    var rand: f32;
    var param_6: f32;
    var wave: f32;
    var dist: f32;
    var waves: f32;
    var l: f32;
    var angle: f32;
    var twist: f32;
    var offset: f32;
    var mid: f32;
    var d: f32;
    var pos_1: vec3<f32>;
    var lightPos: vec3<f32>;
    var type_28: i32;
    var dithering: f32;
    var param_7: vec2<f32>;
    var param_8: vec2<f32>;
    var param_9: i32;
    var param_10: vec2<f32>;
    var param_11: i32;
    var param_12: vec2<f32>;
    var param_13: i32;
    var res: f32;
    var fgColor: vec3<f32>;
    var fgOpacity: f32;
    var bgColor: vec3<f32>;
    var bgOpacity: f32;
    var color: vec3<f32>;
    var opacity: f32;

    let _e194 = unnamed.u_time;
    t_1 = (0.5f * _e194);
    let _e197 = unnamed.u_pxSize;
    let _e199 = unnamed.u_pixelRatio;
    pxSize = (_e197 * _e199);
    let _e201 = gl_FragCoord_1;
    let _e204 = unnamed.u_resolution;
    pxSizeUV = (_e201.xy - (_e204 * 0.5f));
    let _e207 = pxSize;
    let _e208 = pxSizeUV;
    pxSizeUV = (_e208 / vec2(_e207));
    let _e211 = pxSizeUV;
    let _e215 = pxSize;
    canvasPixelizedUV = ((floor(_e211) + vec2(0.5f)) * _e215);
    let _e217 = canvasPixelizedUV;
    let _e219 = unnamed.u_resolution;
    normalizedUV = (_e217 / _e219);
    let _e221 = canvasPixelizedUV;
    ditheringNoiseUV = _e221;
    let _e222 = normalizedUV;
    shapeUV = _e222;
    let _e224 = unnamed.u_originX;
    let _e227 = unnamed.u_originY;
    boxOrigin = vec2<f32>((0.5f - _e224), (_e227 - 0.5f));
    let _e231 = unnamed.u_worldWidth;
    let _e233 = unnamed.u_worldHeight;
    givenBoxSize = vec2<f32>(_e231, _e233);
    let _e235 = givenBoxSize;
    let _e238 = unnamed.u_pixelRatio;
    givenBoxSize = (max(_e235, vec2<f32>(1f, 1f)) * _e238);
    let _e241 = unnamed.u_rotation;
    r = ((_e241 * 3.1415927f) / 180f);
    let _e244 = r;
    let _e246 = r;
    let _e248 = r;
    let _e251 = r;
    graphicRotation = mat2x2<f32>(vec2<f32>(cos(_e244), sin(_e246)), vec2<f32>(-(sin(_e248)), cos(_e251)));
    let _e257 = unnamed.u_offsetX;
    let _e260 = unnamed.u_offsetY;
    graphicOffset = vec2<f32>(-(_e257), _e260);
    let _e263 = givenBoxSize[0u];
    let _e265 = givenBoxSize[1u];
    patternBoxRatio = (_e263 / _e265);
    let _e268 = unnamed.u_worldWidth;
    if (_e268 == 0f) {
        let _e272 = unnamed.u_resolution[0u];
        local = _e272;
    } else {
        let _e274 = givenBoxSize[0u];
        local = _e274;
    }
    let _e275 = local;
    let _e277 = unnamed.u_worldHeight;
    if (_e277 == 0f) {
        let _e281 = unnamed.u_resolution[1u];
        local_1 = _e281;
    } else {
        let _e283 = givenBoxSize[1u];
        local_1 = _e283;
    }
    let _e284 = local_1;
    boxSize = vec2<f32>(_e275, _e284);
    let _e287 = unnamed.u_shape;
    if (_e287 > 3.5f) {
        objectBoxSize = vec2<f32>(0f, 0f);
        let _e290 = boxSize[0u];
        let _e292 = boxSize[1u];
        objectBoxSize[0u] = min(_e290, _e292);
        let _e296 = unnamed.u_fit;
        if (_e296 == 1f) {
            let _e300 = unnamed.u_resolution[0u];
            let _e303 = unnamed.u_resolution[1u];
            objectBoxSize[0u] = min(_e300, _e303);
        } else {
            let _e307 = unnamed.u_fit;
            if (_e307 == 2f) {
                let _e311 = unnamed.u_resolution[0u];
                let _e314 = unnamed.u_resolution[1u];
                objectBoxSize[0u] = max(_e311, _e314);
            }
        }
        let _e318 = objectBoxSize[0u];
        objectBoxSize[1u] = _e318;
        let _e321 = unnamed.u_resolution;
        let _e322 = objectBoxSize;
        objectWorldScale = (_e321 / _e322);
        let _e324 = objectWorldScale;
        let _e325 = shapeUV;
        shapeUV = (_e325 * _e324);
        let _e327 = boxOrigin;
        let _e328 = objectWorldScale;
        let _e332 = shapeUV;
        shapeUV = (_e332 + (_e327 * (_e328 - vec2(1f))));
        let _e335 = unnamed.u_offsetX;
        let _e338 = unnamed.u_offsetY;
        let _e340 = shapeUV;
        shapeUV = (_e340 + vec2<f32>(-(_e335), _e338));
        let _e343 = unnamed.u_scale;
        let _e344 = shapeUV;
        shapeUV = (_e344 / vec2(_e343));
        let _e347 = graphicRotation;
        let _e348 = shapeUV;
        shapeUV = (_e347 * _e348);
    } else {
        patternBoxSize = vec2<f32>(0f, 0f);
        let _e350 = patternBoxRatio;
        let _e352 = boxSize[0u];
        let _e353 = patternBoxRatio;
        let _e356 = boxSize[1u];
        patternBoxSize[0u] = (_e350 * min((_e352 / _e353), _e356));
        let _e361 = patternBoxSize[0u];
        patternWorldNoFitBoxWidth = _e361;
        let _e363 = unnamed.u_fit;
        if (_e363 == 1f) {
            let _e365 = patternBoxRatio;
            let _e368 = unnamed.u_resolution[0u];
            let _e369 = patternBoxRatio;
            let _e373 = unnamed.u_resolution[1u];
            patternBoxSize[0u] = (_e365 * min((_e368 / _e369), _e373));
        } else {
            let _e378 = unnamed.u_fit;
            if (_e378 == 2f) {
                let _e380 = patternBoxRatio;
                let _e383 = unnamed.u_resolution[0u];
                let _e384 = patternBoxRatio;
                let _e388 = unnamed.u_resolution[1u];
                patternBoxSize[0u] = (_e380 * max((_e383 / _e384), _e388));
            }
        }
        let _e393 = patternBoxSize[0u];
        let _e394 = patternBoxRatio;
        patternBoxSize[1u] = (_e393 / _e394);
        let _e398 = unnamed.u_resolution;
        let _e399 = patternBoxSize;
        patternWorldScale = (_e398 / _e399);
        let _e402 = unnamed.u_offsetX;
        let _e405 = unnamed.u_offsetY;
        let _e407 = patternWorldScale;
        let _e409 = shapeUV;
        shapeUV = (_e409 + (vec2<f32>(-(_e402), _e405) / _e407));
        let _e411 = boxOrigin;
        let _e412 = shapeUV;
        shapeUV = (_e412 + _e411);
        let _e414 = boxOrigin;
        let _e415 = patternWorldScale;
        let _e417 = shapeUV;
        shapeUV = (_e417 - (_e414 / _e415));
        let _e420 = unnamed.u_resolution;
        let _e421 = shapeUV;
        shapeUV = (_e421 * _e420);
        let _e424 = unnamed.u_pixelRatio;
        let _e425 = shapeUV;
        shapeUV = (_e425 / vec2(_e424));
        let _e429 = unnamed.u_fit;
        if (_e429 > 0f) {
            let _e431 = patternWorldNoFitBoxWidth;
            let _e433 = patternBoxSize[0u];
            let _e435 = shapeUV;
            shapeUV = (_e435 * (_e431 / _e433));
        }
        let _e438 = unnamed.u_scale;
        let _e439 = shapeUV;
        shapeUV = (_e439 / vec2(_e438));
        let _e442 = graphicRotation;
        let _e443 = shapeUV;
        shapeUV = (_e442 * _e443);
        let _e445 = boxOrigin;
        let _e446 = patternWorldScale;
        let _e448 = shapeUV;
        shapeUV = (_e448 + (_e445 / _e446));
        let _e450 = boxOrigin;
        let _e451 = shapeUV;
        shapeUV = (_e451 - _e450);
        let _e453 = shapeUV;
        shapeUV = (_e453 + vec2(0.5f));
    }
    shape = 0f;
    let _e457 = unnamed.u_shape;
    if (_e457 < 1.5f) {
        let _e459 = shapeUV;
        shapeUV = (_e459 * 0.001f);
        let _e461 = shapeUV;
        param_4 = _e461;
        let _e462 = t_1;
        param_5 = _e462;
        let _e463 = getSimplexNoise_u0028_vf2_u003b_f1_u003b((&param_4), (&param_5));
        shape = (0.5f + (0.5f * _e463));
        let _e466 = shape;
        shape = smoothstep(0.3f, 0.9f, _e466);
    } else {
        let _e469 = unnamed.u_shape;
        if (_e469 < 2.5f) {
            let _e471 = shapeUV;
            shapeUV = (_e471 * 0.003f);
            i_1 = 1f;
            loop {
                let _e473 = i_1;
                if (_e473 < 6f) {
                    let _e475 = i_1;
                    let _e477 = i_1;
                    let _e480 = shapeUV[1u];
                    let _e482 = t_1;
                    let _e487 = shapeUV[0u];
                    shapeUV[0u] = (_e487 + ((0.6f / _e475) * cos((((_e477 * 2.5f) * _e480) + _e482))));
                    let _e490 = i_1;
                    let _e492 = i_1;
                    let _e495 = shapeUV[0u];
                    let _e497 = t_1;
                    let _e502 = shapeUV[1u];
                    shapeUV[1u] = (_e502 + ((0.6f / _e490) * cos((((_e492 * 1.5f) * _e495) + _e497))));
                    continue;
                } else {
                    break;
                }
                continuing {
                    let _e505 = i_1;
                    i_1 = (_e505 + 1f);
                }
            }
            let _e507 = t_1;
            let _e509 = shapeUV[1u];
            let _e512 = shapeUV[0u];
            shape = (0.15f / max(0.001f, abs(sin(((_e507 - _e509) - _e512)))));
            let _e518 = shape;
            shape = smoothstep(0.02f, 1f, _e518);
        } else {
            let _e521 = unnamed.u_shape;
            if (_e521 < 3.5f) {
                let _e523 = shapeUV;
                shapeUV = (_e523 * 0.05f);
                let _e526 = shapeUV[0u];
                stripeIdx = floor(((2f * _e526) / 6.2831855f));
                let _e530 = stripeIdx;
                param_6 = (_e530 * 10f);
                let _e532 = hash11_u0028_f1_u003b((&param_6));
                rand = _e532;
                let _e533 = rand;
                let _e536 = rand;
                rand = (sign((_e533 - 0.5f)) * pow((0.1f + abs(_e536)), 0.4f));
                let _e542 = shapeUV[0u];
                let _e545 = shapeUV[1u];
                let _e546 = rand;
                let _e548 = t_1;
                shape = (sin(_e542) * cos((_e545 - ((5f * _e546) * _e548))));
                let _e553 = shape;
                shape = pow(abs(_e553), 6f);
            } else {
                let _e557 = unnamed.u_shape;
                if (_e557 < 4.5f) {
                    let _e559 = shapeUV;
                    shapeUV = (_e559 * 4f);
                    let _e562 = shapeUV[0u];
                    let _e564 = t_1;
                    let _e569 = shapeUV[0u];
                    let _e571 = t_1;
                    let _e575 = t_1;
                    wave = ((cos(((0.5f * _e562) - (2f * _e564))) * sin(((1.5f * _e569) + _e571))) * (0.75f + (0.25f * cos((3f * _e575)))));
                    let _e582 = shapeUV[1u];
                    let _e583 = wave;
                    shape = (1f - smoothstep(-1f, 1f, (_e582 + _e583)));
                } else {
                    let _e588 = unnamed.u_shape;
                    if (_e588 < 5.5f) {
                        let _e590 = shapeUV;
                        dist = length(_e590);
                        let _e592 = dist;
                        let _e595 = t_1;
                        waves = ((sin(((pow(_e592, 1.7f) * 7f) - (3f * _e595))) * 0.5f) + 0.5f);
                        let _e601 = waves;
                        shape = _e601;
                    } else {
                        let _e603 = unnamed.u_shape;
                        if (_e603 < 6.5f) {
                            let _e605 = shapeUV;
                            l = length(_e605);
                            let _e608 = shapeUV[1u];
                            let _e610 = shapeUV[0u];
                            let _e613 = t_1;
                            angle = ((6f * atan2(_e608, _e610)) + (4f * _e613));
                            twist = 1.2f;
                            let _e616 = l;
                            let _e618 = twist;
                            let _e621 = angle;
                            offset = ((1f / pow(max(_e616, 0.000001f), _e618)) + (_e621 / 6.2831855f));
                            let _e624 = l;
                            let _e625 = twist;
                            mid = smoothstep(0f, 1f, pow(_e624, _e625));
                            let _e628 = offset;
                            let _e630 = mid;
                            shape = mix(0f, fract(_e628), _e630);
                        } else {
                            let _e632 = shapeUV;
                            shapeUV = (_e632 * 2f);
                            let _e634 = shapeUV;
                            d = (1f - pow(length(_e634), 2f));
                            let _e638 = shapeUV;
                            let _e639 = d;
                            pos_1 = vec3<f32>(_e638.x, _e638.y, sqrt(max(0f, _e639)));
                            let _e645 = t_1;
                            let _e648 = t_1;
                            lightPos = normalize(vec3<f32>(cos((1.5f * _e645)), 0.8f, sin((1.25f * _e648))));
                            let _e653 = lightPos;
                            let _e654 = pos_1;
                            shape = (0.5f + (0.5f * dot(_e653, _e654)));
                            let _e658 = d;
                            let _e660 = shape;
                            shape = (_e660 * step(0f, _e658));
                        }
                    }
                }
            }
        }
    }
    let _e663 = unnamed.u_type;
    type_28 = i32(floor(_e663));
    dithering = 0f;
    let _e666 = type_28;
    switch _e666 {
        case 1: {
            let _e669 = ditheringNoiseUV;
            param_7 = _e669;
            let _e670 = hash21_u0028_vf2_u003b((&param_7));
            let _e671 = shape;
            dithering = step(_e670, _e671);
            break;
        }
        case 2: {
            let _e673 = pxSizeUV;
            param_8 = _e673;
            param_9 = 2i;
            let _e674 = getBayerValue_u0028_vf2_u003b_i1_u003b((&param_8), (&param_9));
            dithering = _e674;
            break;
        }
        case 3: {
            let _e675 = pxSizeUV;
            param_10 = _e675;
            param_11 = 4i;
            let _e676 = getBayerValue_u0028_vf2_u003b_i1_u003b((&param_10), (&param_11));
            dithering = _e676;
            break;
        }
        default: {
            let _e667 = pxSizeUV;
            param_12 = _e667;
            param_13 = 8i;
            let _e668 = getBayerValue_u0028_vf2_u003b_i1_u003b((&param_12), (&param_13));
            dithering = _e668;
            break;
        }
    }
    let _e677 = dithering;
    dithering = (_e677 - 0.5f);
    let _e679 = shape;
    let _e680 = dithering;
    res = step(0.5f, (_e679 + _e680));
    let _e684 = unnamed.u_colorFront;
    let _e688 = unnamed.u_colorFront[3u];
    fgColor = (_e684.xyz * _e688);
    let _e692 = unnamed.u_colorFront[3u];
    fgOpacity = _e692;
    let _e694 = unnamed.u_colorBack;
    let _e698 = unnamed.u_colorBack[3u];
    bgColor = (_e694.xyz * _e698);
    let _e702 = unnamed.u_colorBack[3u];
    bgOpacity = _e702;
    let _e703 = fgColor;
    let _e704 = res;
    color = (_e703 * _e704);
    let _e706 = fgOpacity;
    let _e707 = res;
    opacity = (_e706 * _e707);
    let _e709 = bgColor;
    let _e710 = opacity;
    let _e713 = color;
    color = (_e713 + (_e709 * (1f - _e710)));
    let _e715 = bgOpacity;
    let _e716 = opacity;
    let _e719 = opacity;
    opacity = (_e719 + (_e715 * (1f - _e716)));
    let _e721 = color;
    let _e722 = opacity;
    fragColor = vec4<f32>(_e721.x, _e721.y, _e721.z, _e722);
    return;
}

@fragment 
fn main(@builtin(position) gl_FragCoord: vec4<f32>) -> @location(0) vec4<f32> {
    gl_FragCoord_1 = gl_FragCoord;
    main_1();
    let _e3 = fragColor;
    return _e3;
}
