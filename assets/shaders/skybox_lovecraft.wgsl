struct SkyUniforms {
    zenith_color: vec4<f32>,
    horizon_color: vec4<f32>,
    nadir_color: vec4<f32>,
    sun_dir_and_intensity: vec4<f32>,
    sun_color: vec4<f32>,
    moon_dir_and_phase: vec4<f32>,
    moon_color: vec4<f32>,
    cloud_color: vec4<f32>,
    params: vec4<f32>, // x: time, y: stars_intensity, z: cloud_coverage, w: storm_intensity
};

@group(2) @binding(0) var<uniform> sky: SkyUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// Псевдо-случайный шум для звезд и облаков
fn hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let d = dot(q, q + 45.32);
    return fract(d);
}

fn hash3(p: vec3<f32>) -> f32 {
    let q = fract(p * vec3<f32>(123.34, 456.21, 789.92));
    let d = dot(q, q + 45.32);
    return fract(d);
}

fn smooth_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let a = hash(i + vec2<f32>(0.0, 0.0));
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));

    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    for (var i = 0; i < 4; i = i + 1) {
        val = val + amp * smooth_noise(pos);
        pos = pos * 2.02 + vec2<f32>(1.7, 3.2);
        amp = amp * 0.5;
    }
    return val;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(mesh.world_position.xyz);
    let time = sky.params.x;
    let stars_intensity = sky.params.y;
    let cloud_coverage = sky.params.z;
    let storm_intensity = sky.params.w;

    // 1. Небесный градиент (Зенит - Горизонт - Надир)
    var sky_base: vec3<f32>;
    let h = dir.y;
    if (h >= 0.0) {
        let grad = pow(clamp(h, 0.0, 1.0), 0.58);
        sky_base = mix(sky.horizon_color.rgb, sky.zenith_color.rgb, grad);
    } else {
        let grad = clamp(-h * 4.0, 0.0, 1.0);
        sky_base = mix(sky.horizon_color.rgb, sky.nadir_color.rgb, grad);
    }

    // 2. Двухслойные процедурные облака (fBm Noise)
    if (h > 0.02) {
        let plane_uv = (dir.xz / (dir.y + 0.18)) * 1.8;
        let wind_scroll1 = vec2<f32>(time * 0.012, time * 0.007);
        let wind_scroll2 = vec2<f32>(-time * 0.008, time * 0.015);

        let cloud_dense1 = fbm(plane_uv + wind_scroll1);
        let cloud_dense2 = fbm(plane_uv * 1.6 + wind_scroll2);
        let cloud_total = smoothstep(0.48 - cloud_coverage * 0.35, 0.82, (cloud_dense1 + cloud_dense2 * 0.5) / 1.5);

        let cloud_fade = smoothstep(0.02, 0.22, h);
        let cloud_alpha = clamp(cloud_total * sky.cloud_color.a * cloud_fade, 0.0, 1.0);

        // Облака подсвечиваются солнцем/луной
        let sun_dot = max(dot(dir, normalize(sky.sun_dir_and_intensity.xyz)), 0.0);
        let moon_dot = max(dot(dir, normalize(sky.moon_dir_and_phase.xyz)), 0.0);
        let cloud_illum = sky.cloud_color.rgb + sky.sun_color.rgb * pow(sun_dot, 4.0) * 0.45 + sky.moon_color.rgb * pow(moon_dot, 3.0) * 0.25;

        sky_base = mix(sky_base, cloud_illum, cloud_alpha);
    }

    // 3. Звездное небо Лавкрафта и космические созвездия
    if (h > 0.08 && stars_intensity > 0.02) {
        let star_coord = dir.xz / (h + 0.12) * 90.0;
        let star_cell = floor(star_coord);
        let star_rnd = hash(star_cell);
        if (star_rnd > 0.94) {
            let star_pos = star_cell + vec2<f32>(hash(star_cell + 1.1), hash(star_cell + 2.3));
            let dist_to_star = length(star_coord - star_pos);
            let twinkle = sin(time * 3.5 + star_rnd * 25.0) * 0.35 + 0.65;
            let star_lum = smoothstep(0.18, 0.0, dist_to_star) * star_rnd * twinkle * stars_intensity;
            // Некоторые звезды мерцают бледно-изумрудным оккультным светом
            let star_color = mix(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(0.6, 1.0, 0.8), fract(star_rnd * 17.0));
            sky_base = sky_base + star_color * star_lum * 2.2;
        }

        // Космическая туманность Каркозы (слабое зеленовато-пурпурное сияние на севере)
        let eldritch_nebula = fbm(dir.xz * 3.0 + vec2<f32>(0.0, time * 0.003));
        let nebula_mask = smoothstep(0.55, 0.85, eldritch_nebula) * smoothstep(0.1, 0.7, h) * (1.0 - dir.z * 0.5);
        let nebula_color = vec3<f32>(0.08, 0.24, 0.18) * stars_intensity * nebula_mask * 1.5;
        sky_base = sky_base + nebula_color;
    }

    // 4. Диск Солнца (с ярким HDR-ореолом для Bloom)
    let sun_norm = normalize(sky.sun_dir_and_intensity.xyz);
    let sun_dot = dot(dir, sun_norm);
    if (sun_dot > 0.9985) {
        // Яркий солнечный диск (HDR > 2.5)
        sky_base = sky_base + sky.sun_color.rgb * 4.5;
    } else if (sun_dot > 0.96) {
        // Мягкий ореол рассеяния (Mie forward scatter)
        let halo = pow((sun_dot - 0.96) / 0.04, 2.5);
        sky_base = sky_base + sky.sun_color.rgb * halo * 1.8;
    }

    // 5. Огромная Луна Иннсмута
    let moon_norm = normalize(sky.moon_dir_and_phase.xyz);
    let moon_dot = dot(dir, moon_norm);
    if (moon_dot > 0.996) {
        // Лунный диск с кратерами
        let moon_noise = smooth_noise(dir.xy * 80.0);
        let crater_tint = mix(0.78, 1.15, moon_noise);
        sky_base = sky_base + sky.moon_color.rgb * 2.8 * crater_tint;
    } else if (moon_dot > 0.97) {
        let moon_halo = pow((moon_dot - 0.97) / 0.03, 2.0);
        sky_base = sky_base + sky.moon_color.rgb * moon_halo * 0.9;
    }

    // 6. Далекие зарницы / грозовые вспышки во время ритуала Бездны
    if (storm_intensity > 0.05) {
        let flash = sin(time * 8.0) * sin(time * 19.3);
        if (flash > 0.82) {
            sky_base = sky_base + vec3<f32>(0.25, 0.15, 0.40) * storm_intensity * (flash - 0.82) * 8.0;
        }
    }

    return vec4<f32>(sky_base, 1.0);
}
