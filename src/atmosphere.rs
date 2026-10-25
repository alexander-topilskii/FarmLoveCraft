use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{GlobalAmbientLight, NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{DistanceFog, FogFalloff, Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin, ScreenSpaceAmbientOcclusion};
use bevy::post_process::bloom::{Bloom, BloomCompositeMode};
use bevy::post_process::dof::{DepthOfField, DepthOfFieldMode};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection, Msaa};
use bevy::shader::ShaderRef;

use crate::camera::CameraController;
use crate::defense::DefenseWaveState;
use crate::player::Player;
use crate::terrain::{Biome, WorldTerrain};

/// Часы игрового мира с суточным циклом (0.0 .. 24.0 ч)
#[derive(Resource, Debug, Clone)]
pub struct WorldClock {
    /// Текущее время суток в часах (0.0..24.0).
    /// По умолчанию: 17.5 (Золотой час / закат с максимальными God Rays)
    pub time_of_day: f32,
    /// Скорость течения времени (игровых часов за реальную секунду).
    /// 0.05 => 24 часа за 480 секунд (8 минут)
    pub time_speed: f32,
    pub is_paused: bool,
}

impl Default for WorldClock {
    fn default() -> Self {
        Self {
            time_of_day: 17.5,
            time_speed: 0.05,
            is_paused: false,
        }
    }
}

impl WorldClock {
    #[inline]
    pub fn is_night(&self) -> bool {
        self.time_of_day >= 20.5 || self.time_of_day < 5.0
    }

    pub fn phase_name(&self) -> &'static str {
        if self.time_of_day >= 5.0 && self.time_of_day < 8.0 {
            "Туманный Рассвет"
        } else if self.time_of_day >= 8.0 && self.time_of_day < 17.0 {
            "Пасмурный День"
        } else if self.time_of_day >= 17.0 && self.time_of_day < 20.5 {
            "Кровавый Закат"
        } else {
            "Ночь Древних"
        }
    }
}

/// Унифицированный шейдерный буфер скайбокса Лавкрафта
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct SkyUniforms {
    pub zenith_color: LinearRgba,
    pub horizon_color: LinearRgba,
    pub nadir_color: LinearRgba,
    pub sun_dir_and_intensity: Vec4,
    pub sun_color: LinearRgba,
    pub moon_dir_and_phase: Vec4,
    pub moon_color: LinearRgba,
    pub cloud_color: LinearRgba,
    pub params: Vec4, // x: time, y: stars_intensity, z: cloud_coverage, w: storm_intensity
}

impl Default for SkyUniforms {
    fn default() -> Self {
        Self {
            zenith_color: LinearRgba::new(0.12, 0.18, 0.35, 1.0),
            horizon_color: LinearRgba::new(0.80, 0.50, 0.32, 1.0),
            nadir_color: LinearRgba::new(0.08, 0.10, 0.12, 1.0),
            sun_dir_and_intensity: Vec4::new(0.65, 0.35, 0.40, 1.0),
            sun_color: LinearRgba::new(1.0, 0.68, 0.38, 1.0),
            moon_dir_and_phase: Vec4::new(-0.65, 0.40, -0.40, 1.0),
            moon_color: LinearRgba::new(0.55, 0.75, 0.95, 1.0),
            cloud_color: LinearRgba::new(0.45, 0.38, 0.42, 0.75),
            params: Vec4::new(0.0, 0.2, 0.45, 0.0),
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SkyMaterial {
    #[uniform(0)]
    pub uniforms: SkyUniforms,
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/skybox_lovecraft.wgsl".into()
    }

    #[inline]
    fn enable_shadows() -> bool {
        false
    }

    #[inline]
    fn enable_prepass() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Маркер небесного купола
#[derive(Component)]
pub struct SkyDome;

/// Маркер главного направленного светила (Солнце / Луна)
#[derive(Component)]
pub struct CelestialSunLight;

/// Компонент живого мерцания пламени и газовых фонарей
#[derive(Component, Clone, Copy, Debug)]
pub struct FlickeringLight {
    pub base_intensity: f32,
    pub amplitude: f32,
    pub speed: f32,
    pub phase: f32,
    /// Если true — днем фонарь приглушается, в сумерках и ночью зажигается на полную
    pub is_street_lantern: bool,
}

impl Default for FlickeringLight {
    fn default() -> Self {
        Self {
            base_intensity: 65_000.0,
            amplitude: 0.18,
            speed: 3.2,
            phase: 0.0,
            is_street_lantern: false,
        }
    }
}

impl FlickeringLight {
    pub fn torch(base: f32) -> Self {
        Self {
            base_intensity: base,
            amplitude: 0.26,
            speed: 5.5,
            phase: 0.0,
            is_street_lantern: false,
        }
    }

    pub fn lantern(base: f32) -> Self {
        Self {
            base_intensity: base,
            amplitude: 0.14,
            speed: 2.8,
            phase: 0.0,
            is_street_lantern: true,
        }
    }
}

/// Компонент покачивания крон деревьев и кустов на ветру
#[derive(Component, Clone, Copy, Debug)]
pub struct FoliageWindSway {
    pub base_rotation: Quat,
    pub phase: f32,
    pub sway_speed: f32,
    pub sway_amount: f32,
}

/// Маркер частицы атмосферной погоды вокруг камеры
#[derive(Component, Clone, Copy, Debug)]
pub struct WeatherParticle {
    pub offset: Vec3,
    pub fall_speed: f32,
    pub sway_phase: f32,
}

/// Пул материалов и мешей для легковесных атмосферных частиц вокруг камеры
#[derive(Resource)]
#[allow(dead_code)]
pub struct WeatherParticleAssets {
    pub rain_mesh: Handle<Mesh>,
    pub rain_material: Handle<StandardMaterial>,
    pub spore_mesh: Handle<Mesh>,
    pub spore_material: Handle<StandardMaterial>,
}

/// Текущее интерполированное состояние атмосферы мира
#[derive(Resource, Debug, Clone)]
#[allow(dead_code)]
pub struct AtmosphereState {
    pub sun_color: Color,
    pub sun_illuminance: f32,
    pub sun_direction: Vec3,

    pub moon_color: Color,
    pub moon_direction: Vec3,

    pub ambient_color: Color,
    pub ambient_brightness: f32,

    pub fog_color: Color,
    pub fog_directional_color: Color,
    pub fog_directional_exponent: f32,
    pub fog_extinction: Vec3,
    pub fog_inscattering: Vec3,

    pub fog_volume_density: f32,
    pub fog_volume_color: Color,

    pub sky_zenith: Color,
    pub sky_horizon: Color,
    pub sky_nadir: Color,

    pub stars_intensity: f32,
    pub cloud_coverage: f32,
    pub storm_intensity: f32,

    pub wind_strength: f32,
    pub wind_direction: Vec2,

    pub rain_rate: f32,
    pub spore_rate: f32,

    pub active_biome: Biome,
}

impl Default for AtmosphereState {
    fn default() -> Self {
        Self {
            sun_color: Color::srgb(1.0, 0.68, 0.38),
            sun_illuminance: 8_000.0,
            sun_direction: Vec3::new(0.65, 0.35, 0.40).normalize(),

            moon_color: Color::srgb(0.55, 0.75, 0.95),
            moon_direction: Vec3::new(-0.65, 0.40, -0.40).normalize(),

            ambient_color: Color::srgb(0.60, 0.54, 0.64),
            ambient_brightness: 420.0,

            fog_color: Color::srgb(0.48, 0.52, 0.58),
            fog_directional_color: Color::srgba(1.0, 0.75, 0.45, 0.65),
            fog_directional_exponent: 14.0,
            fog_extinction: Vec3::splat(0.005),
            fog_inscattering: Vec3::splat(0.003),

            fog_volume_density: 0.0,
            fog_volume_color: Color::srgb(0.48, 0.52, 0.58),

            sky_zenith: Color::srgb(0.12, 0.18, 0.35),
            sky_horizon: Color::srgb(0.80, 0.50, 0.32),
            sky_nadir: Color::srgb(0.08, 0.10, 0.12),

            stars_intensity: 0.2,
            cloud_coverage: 0.45,
            storm_intensity: 0.0,

            wind_strength: 0.85,
            wind_direction: Vec2::new(1.0, 0.35).normalize(),

            rain_rate: 0.2,
            spore_rate: 0.4,

            active_biome: Biome::ArkhamCitadel,
        }
    }
}

/// Маркер текста состояния атмосферы и времени суток в Top HUD
#[derive(Component)]
pub struct AtmosphereHudText;

/// Обновление текстового статуса времени и биома в Top HUD
fn update_atmosphere_hud_system(
    clock: Res<WorldClock>,
    state: Res<AtmosphereState>,
    mut query: Query<&mut Text, With<AtmosphereHudText>>,
) {
    let hours = clock.time_of_day.floor() as u32;
    let mins = ((clock.time_of_day.fract()) * 60.0).floor() as u32;
    let phase = clock.phase_name();
    let biome_name = state.active_biome.display_name();

    for mut text in &mut query {
        **text = format!("Время: {:02}:{:02} ({}) | Биом: {}", hours, mins, phase, biome_name);
    }
}

/// Плагин атмосферы Valheim + Лавкрафта
pub struct AtmospherePlugin;

impl Plugin for AtmospherePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WorldClock::default())
            .insert_resource(AtmosphereState::default())
            .add_plugins(MaterialPlugin::<SkyMaterial>::default())
            .add_systems(Startup, setup_atmosphere_system)
            .add_systems(
                Update,
                (
                    update_world_clock_system,
                    update_atmosphere_interpolation_system,
                    update_celestial_lights_system,
                    update_distance_fog_system,
                    update_sky_dome_system,
                    update_lantern_and_fire_flicker_system,
                    update_foliage_wind_system,
                    update_camera_weather_particles_system,
                    update_camera_depth_of_field_system,
                    update_atmosphere_hud_system,
                )
                    .in_set(crate::GameSystemSet::Presentation),
            );
    }
}

/// Начальная инициализация небесного купола, материалов и частиц погоды
fn setup_atmosphere_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut sky_materials: ResMut<Assets<SkyMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // 1. Создаем большую инвертированную сферу небесного купола (радиус 260м)
    let sky_mesh = create_inverted_sphere_mesh(260.0, 24, 36);
    let sky_mat = sky_materials.add(SkyMaterial {
        uniforms: SkyUniforms::default(),
    });

    commands.spawn((
        SkyDome,
        Mesh3d(meshes.add(sky_mesh)),
        MeshMaterial3d(sky_mat),
        NotShadowCaster,
        NotShadowReceiver,
        Transform::from_translation(Vec3::ZERO),
    ));

    // 2. Инициализируем ресурсы для частиц погоды (дождь + споры)
    let rain_mesh = meshes.add(Cuboid::new(0.04, 0.48, 0.04));
    let rain_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.70, 0.85, 1.0, 0.45),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.1,
        unlit: true,
        ..default()
    });

    let spore_mesh = meshes.add(Sphere::new(0.06).mesh().ico(1).unwrap());
    let spore_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 1.0, 0.7),
        emissive: LinearRgba::new(0.4, 2.5, 1.4, 1.0), // HDR > 2.0 для мягкого Bloom!
        unlit: true,
        ..default()
    });

    let weather_assets = WeatherParticleAssets {
        rain_mesh: rain_mesh.clone(),
        rain_material: rain_mat.clone(),
        spore_mesh: spore_mesh.clone(),
        spore_material: spore_mat.clone(),
    };

    // Спавним компактный пул из 160 частиц вокруг фокуса камеры
    let mut rng_val = 1337u32;
    let mut pseudo_rnd = || {
        rng_val = rng_val.wrapping_mul(1664525).wrapping_add(1013904223);
        (rng_val as f32) / (u32::MAX as f32)
    };

    for i in 0..160 {
        let ox = (pseudo_rnd() - 0.5) * 44.0;
        let oy = pseudo_rnd() * 18.0 + 1.0;
        let oz = (pseudo_rnd() - 0.5) * 44.0;
        let speed = 7.0 + pseudo_rnd() * 4.0;
        let phase = pseudo_rnd() * std::f32::consts::TAU;

        let is_spore = (i % 3) == 0;
        let mesh_h = if is_spore { spore_mesh.clone() } else { rain_mesh.clone() };
        let mat_h = if is_spore { spore_mat.clone() } else { rain_mat.clone() };

        commands.spawn((
            WeatherParticle {
                offset: Vec3::new(ox, oy, oz),
                fall_speed: if is_spore { 0.8 } else { speed },
                sway_phase: phase,
            },
            Mesh3d(mesh_h),
            MeshMaterial3d(mat_h),
            Transform::from_xyz(ox, oy, oz),
            Visibility::Inherited,
        ));
    }

    commands.insert_resource(weather_assets);
}

/// Создает сетку сферы с инвертированными внутрь нормалями для скайбокса
fn create_inverted_sphere_mesh(radius: f32, stacks: usize, sectors: usize) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    for i in 0..=stacks {
        let v = i as f32 / stacks as f32;
        let phi = v * std::f32::consts::PI;

        for j in 0..=sectors {
            let u = j as f32 / sectors as f32;
            let theta = u * std::f32::consts::TAU;

            let x = phi.sin() * theta.cos();
            let y = phi.cos();
            let z = phi.sin() * theta.sin();

            positions.push([x * radius, y * radius, z * radius]);
            // Нормали смотрят внутрь купола
            normals.push([-x, -y, -z]);
            uvs.push([u, v]);
        }
    }

    for i in 0..stacks {
        for j in 0..sectors {
            let first = (i * (sectors + 1) + j) as u32;
            let second = first + sectors as u32 + 1;

            // Инвертированная ориентация треугольников
            indices.push(first);
            indices.push(first + 1);
            indices.push(second);

            indices.push(second);
            indices.push(first + 1);
            indices.push(second + 1);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Продвижение игровых суток
fn update_world_clock_system(
    time: Res<Time>,
    mut clock: ResMut<WorldClock>,
) {
    if clock.is_paused {
        return;
    }
    clock.time_of_day = (clock.time_of_day + clock.time_speed * time.delta_secs()) % 24.0;
}

/// Плавная интерполяция палитры атмосферы под текущий биом и время суток
fn update_atmosphere_interpolation_system(
    time: Res<Time>,
    clock: Res<WorldClock>,
    terrain: Res<WorldTerrain>,
    defense_wave: Res<DefenseWaveState>,
    player_query: Query<&Transform, With<Player>>,
    mut state: ResMut<AtmosphereState>,
) {
    let dt = time.delta_secs().clamp(0.0, 0.1);

    // Опрашиваем биом под Сыщиком
    let player_pos = player_query.single().map(|t| t.translation).unwrap_or(Vec3::ZERO);
    let sample_biome = terrain.sample_biome(player_pos.x, player_pos.z);
    state.active_biome = sample_biome;

    // Суточная дуга Солнца и Луны с оптимальным углом подъёма над горизонтом
    let tod_rad = (clock.time_of_day - 6.0) / 24.0 * std::f32::consts::TAU;
    let raw_sin = tod_rad.sin();
    let raw_cos = tod_rad.cos();

    let sun_y = (raw_sin * 0.75 + 0.35).clamp(0.22, 0.95);
    let sun_x = raw_cos * 0.85;
    let sun_z = 0.40;
    let sun_dir = Vec3::new(sun_x, sun_y, sun_z).normalize();

    let moon_y = (-raw_sin * 0.70 + 0.35).clamp(0.25, 0.90);
    let moon_x = -raw_cos * 0.85;
    let moon_z = -0.40;
    let moon_dir = Vec3::new(moon_x, moon_y, moon_z).normalize();

    state.sun_direction = sun_dir;
    state.moon_direction = moon_dir;

    // Расчет целевых параметров по суточному циклу:
    // Фазы: Рассвет (05-08), День (08-17), Закат (17-20.5), Ночь (20.5-05)
    let tod = clock.time_of_day;
    let (target_sun_col, target_sun_illum, target_ambient_col, target_ambient_bright, target_sky_zenith, target_sky_horiz, target_stars) =
        if tod >= 5.0 && tod < 8.0 {
            // Рассвет: розово-золотой горизонт, длинные мягкие тени
            let t = (tod - 5.0) / 3.0;
            (
                Color::srgb(1.0, 0.82, 0.68),
                7_000.0 * t + 3_000.0 * (1.0 - t),
                Color::srgb(0.58, 0.58, 0.66),
                380.0,
                Color::srgb(0.14, 0.22, 0.40),
                Color::srgb(0.70, 0.55, 0.48),
                (1.0 - t) * 0.4,
            )
        } else if tod >= 8.0 && tod < 17.0 {
            // День: естественный сбалансированный свет Valheim (мягкие тени, нет неонового пересвета)
            (
                Color::srgb(1.0, 0.96, 0.90),
                8_500.0,
                Color::srgb(0.62, 0.64, 0.70),
                480.0,
                Color::srgb(0.20, 0.35, 0.55),
                Color::srgb(0.58, 0.64, 0.72),
                0.0,
            )
        } else if tod >= 17.0 && tod < 20.5 {
            // Закат: багрово-янтарный золотой час с мягким заполнением теней
            let t = (tod - 17.0) / 3.5;
            (
                Color::srgb(1.0, 0.68, 0.38),
                8_000.0 * (1.0 - t) + 3_500.0 * t,
                Color::srgb(0.60, 0.54, 0.64),
                420.0 * (1.0 - t) + 260.0 * t,
                Color::srgb(0.12, 0.18, 0.35),
                Color::srgb(0.80, 0.50, 0.32),
                t * 0.45,
            )
        } else {
            // Ночь: читаемая лунная ночь без черных провалов
            (
                Color::srgb(0.55, 0.75, 0.95),
                2_400.0,
                Color::srgb(0.35, 0.42, 0.55),
                160.0,
                Color::srgb(0.02, 0.04, 0.08),
                Color::srgb(0.08, 0.12, 0.18),
                0.85,
            )
        };

    // Модификаторы по Биому: естественные нордические туманы
    let (biome_fog_col, biome_fog_dir_col, biome_fog_ext, biome_rain, biome_spores, biome_wind) = match sample_biome {
        Biome::ArkhamCitadel => (
            Color::srgb(0.50, 0.53, 0.58),
            Color::srgba(1.0, 0.75, 0.45, 0.65),
            Vec3::splat(0.005),
            0.15,
            0.10,
            0.6,
        ),
        Biome::InnsmouthCity => (
            Color::srgb(0.45, 0.52, 0.55), // прохладный морской туман
            Color::srgba(0.5, 0.85, 0.85, 0.70),
            Vec3::splat(0.007),
            0.45,
            0.15,
            1.1,
        ),
        Biome::InnsmouthSwamp => (
            Color::srgb(0.42, 0.48, 0.44), // приглушенный болотный туман
            Color::srgba(0.45, 0.85, 0.65, 0.85),
            Vec3::splat(0.008),
            0.65,
            0.90,
            0.7,
        ),
        Biome::WitchVillage => (
            Color::srgb(0.52, 0.46, 0.54), // дымчато-сумеречный вечер
            Color::srgba(0.85, 0.60, 0.80, 0.75),
            Vec3::splat(0.006),
            0.20,
            0.40,
            0.8,
        ),
        Biome::Darkwoods => (
            Color::srgb(0.44, 0.48, 0.45), // хвойная лесная дымка
            Color::srgba(0.95, 0.80, 0.50, 0.85),
            Vec3::splat(0.007),
            0.10,
            0.75,
            0.5,
        ),
        Biome::EldritchRuins => (
            Color::srgb(0.48, 0.44, 0.52), // древний серый сумрак
            Color::srgba(0.80, 0.45, 0.90, 0.75),
            Vec3::splat(0.008),
            0.05,
            0.85,
            1.4,
        ),
    };

    // Если активен Ритуал Защиты (DefenseWave) — атмосфера наполняется бурей Бездны
    let storm_active = defense_wave.is_active;
    let final_storm_intensity = if storm_active { 1.0 } else { 0.0 };

    // Скорость сглаживания перехода LERP (за ~3.0 секунды)
    let lerp_t = (dt * 1.8).min(1.0);

    state.sun_color = lerp_color(state.sun_color, target_sun_col, lerp_t);
    state.sun_illuminance = state.sun_illuminance + (target_sun_illum - state.sun_illuminance) * lerp_t;

    state.ambient_color = lerp_color(state.ambient_color, target_ambient_col, lerp_t);
    state.ambient_brightness = state.ambient_brightness + (target_ambient_bright - state.ambient_brightness) * lerp_t;

    state.sky_zenith = lerp_color(state.sky_zenith, target_sky_zenith, lerp_t);
    state.sky_horizon = lerp_color(state.sky_horizon, target_sky_horiz, lerp_t);

    state.fog_color = lerp_color(state.fog_color, biome_fog_col, lerp_t);
    state.fog_directional_color = lerp_color(state.fog_directional_color, biome_fog_dir_col, lerp_t);
    state.fog_extinction = state.fog_extinction.lerp(biome_fog_ext, lerp_t);
    state.fog_inscattering = state.fog_extinction * 0.75;

    state.stars_intensity = state.stars_intensity + (target_stars - state.stars_intensity) * lerp_t;
    state.storm_intensity = state.storm_intensity + (final_storm_intensity - state.storm_intensity) * lerp_t;

    state.rain_rate = state.rain_rate + (biome_rain - state.rain_rate) * lerp_t;
    state.spore_rate = state.spore_rate + (biome_spores - state.spore_rate) * lerp_t;
    state.wind_strength = state.wind_strength + (biome_wind - state.wind_strength) * lerp_t;
}

#[inline]
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let la = a.to_linear();
    let lb = b.to_linear();
    let r = la.red + (lb.red - la.red) * t;
    let g = la.green + (lb.green - la.green) * t;
    let bl = la.blue + (lb.blue - la.blue) * t;
    let al = la.alpha + (lb.alpha - la.alpha) * t;
    Color::LinearRgba(LinearRgba::new(r, g, bl, al))
}

/// Синхронизация направления, цвета и яркости главного DirectionalLight и GlobalAmbientLight
fn update_celestial_lights_system(
    state: Res<AtmosphereState>,
    player_query: Query<&Transform, (With<Player>, Without<CelestialSunLight>)>,
    mut sun_query: Query<(&mut Transform, &mut DirectionalLight), With<CelestialSunLight>>,
    ambient: Option<ResMut<GlobalAmbientLight>>,
) {
    let focus = player_query.single().map(|t| t.translation).unwrap_or(Vec3::ZERO);

    for (mut tf, mut light) in &mut sun_query {
        let light_dir = if state.sun_direction.y >= -0.08 {
            state.sun_direction
        } else {
            state.moon_direction
        };

        tf.translation = focus + light_dir * 50.0;
        tf.look_at(focus, Vec3::Y);

        light.color = state.sun_color;
        light.illuminance = state.sun_illuminance;
    }

    if let Some(mut amb) = ambient {
        amb.color = state.ambient_color;
        amb.brightness = state.ambient_brightness;
    }
}

/// Синхронизация DistanceFog на камере
fn update_distance_fog_system(
    state: Res<AtmosphereState>,
    mut cam_query: Query<&mut DistanceFog, With<Camera3d>>,
) {
    for mut dist_fog in &mut cam_query {
        dist_fog.color = state.fog_color;
        dist_fog.directional_light_color = state.fog_directional_color;
        dist_fog.directional_light_exponent = state.fog_directional_exponent;
        dist_fog.falloff = FogFalloff::Linear {
            start: 45.0,
            end: 130.0,
        };
    }
}

/// Синхронизация позиции небесного купола и параметров шейдера
fn update_sky_dome_system(
    time: Res<Time>,
    state: Res<AtmosphereState>,
    cam_query: Query<&Transform, With<Camera3d>>,
    mut dome_query: Query<(&mut Transform, &MeshMaterial3d<SkyMaterial>), (With<SkyDome>, Without<Camera3d>)>,
    mut sky_materials: ResMut<Assets<SkyMaterial>>,
) {
    let Ok(cam_tf) = cam_query.single() else {
        return;
    };

    for (mut dome_tf, mat_handle) in &mut dome_query {
        dome_tf.translation = cam_tf.translation;

        if let Some(mut mat) = sky_materials.get_mut(&mat_handle.0) {
            mat.uniforms.zenith_color = state.sky_zenith.to_linear();
            mat.uniforms.horizon_color = state.sky_horizon.to_linear();
            mat.uniforms.nadir_color = state.sky_nadir.to_linear();

            mat.uniforms.sun_dir_and_intensity = Vec4::new(
                state.sun_direction.x,
                state.sun_direction.y,
                state.sun_direction.z,
                (state.sun_illuminance / 10_000.0).clamp(0.2, 2.0),
            );
            mat.uniforms.sun_color = state.sun_color.to_linear();

            mat.uniforms.moon_dir_and_phase = Vec4::new(
                state.moon_direction.x,
                state.moon_direction.y,
                state.moon_direction.z,
                1.0,
            );
            mat.uniforms.moon_color = state.moon_color.to_linear();

            mat.uniforms.params = Vec4::new(
                time.elapsed_secs(),
                state.stars_intensity,
                state.cloud_coverage,
                state.storm_intensity,
            );
        }
    }
}

/// Живое мерцание газовых фонарей, свечей, каминов и фонаря Сыщика
fn update_lantern_and_fire_flicker_system(
    time: Res<Time>,
    clock: Res<WorldClock>,
    mut query: Query<(&FlickeringLight, &mut PointLight)>,
) {
    let t = time.elapsed_secs();
    let is_night = clock.is_night();

    for (flicker, mut light) in &mut query {
        // Многочастотная интерференция для естественного дыхания пламени
        let phase = flicker.phase;
        let s1 = ((t * flicker.speed + phase).sin() * 0.55).sin();
        let s2 = ((t * flicker.speed * 2.31 + phase * 1.7).sin() * 0.30).sin();
        let s3 = ((t * flicker.speed * 4.73 - phase * 0.9).cos() * 0.15).sin();
        let noise = s1 + s2 + s3;

        let mut multiplier = 1.0 + noise * flicker.amplitude;

        // Если это уличный фонарь: днем приглушается на 80% для реализма
        if flicker.is_street_lantern && !is_night {
            let day_factor = if clock.time_of_day >= 8.0 && clock.time_of_day < 17.0 {
                0.20
            } else {
                0.65
            };
            multiplier *= day_factor;
        }

        light.intensity = (flicker.base_intensity * multiplier).max(0.0);
    }
}

/// Качание крон деревьев и кустов на ветру
fn update_foliage_wind_system(
    time: Res<Time>,
    state: Res<AtmosphereState>,
    mut query: Query<(&FoliageWindSway, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    let wind = state.wind_strength;
    let wind_dir = state.wind_direction;

    for (sway, mut tf) in &mut query {
        let p = t * sway.sway_speed + sway.phase;
        let angle_x = (p.sin() * 0.7 + (p * 2.1).cos() * 0.3) * sway.sway_amount * wind;
        let angle_z = ((p * 1.3 + 1.2).cos() * 0.6) * sway.sway_amount * wind;

        let sway_rot = Quat::from_rotation_x(angle_x * wind_dir.x) * Quat::from_rotation_z(angle_z * wind_dir.y);
        tf.rotation = sway.base_rotation * sway_rot;
    }
}

/// Компактный пул частиц погоды (дождь + споры Бездны), следующий за камерой
fn update_camera_weather_particles_system(
    time: Res<Time>,
    state: Res<AtmosphereState>,
    player_query: Query<&Transform, (With<Player>, Without<WeatherParticle>)>,
    mut particles_query: Query<(&mut WeatherParticle, &mut Transform, &mut Visibility)>,
) {
    let dt = time.delta_secs();
    let focus = player_query.single().map(|t| t.translation).unwrap_or(Vec3::ZERO);

    let rain_active = state.rain_rate > 0.05 || state.storm_intensity > 0.05;
    let spores_active = state.spore_rate > 0.05;

    let wind_offset = Vec3::new(state.wind_direction.x, 0.0, state.wind_direction.y) * state.wind_strength * 2.5;

    for (mut particle, mut tf, mut vis) in &mut particles_query {
        // Опускаем частицу вниз со скоростью падения + снос ветром
        particle.offset.y -= particle.fall_speed * dt;
        particle.offset.x += wind_offset.x * dt;
        particle.offset.z += wind_offset.z * dt;

        // Зацикливание в локальном объеме вокруг фокуса камеры (±22м по XZ, 0..18м по Y)
        if particle.offset.y < 0.2 {
            particle.offset.y = 18.0;
        }
        if particle.offset.x > 22.0 {
            particle.offset.x -= 44.0;
        } else if particle.offset.x < -22.0 {
            particle.offset.x += 44.0;
        }
        if particle.offset.z > 22.0 {
            particle.offset.z -= 44.0;
        } else if particle.offset.z < -22.0 {
            particle.offset.z += 44.0;
        }

        tf.translation = focus + particle.offset;

        // Плавное покачивание
        particle.sway_phase += dt * 2.0;
        let sway_x = particle.sway_phase.sin() * 0.15;
        tf.translation.x += sway_x;

        let should_be_visible = rain_active || spores_active;
        let desired_vis = if should_be_visible { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != desired_vis {
            *vis = desired_vis;
        }
    }
}

/// Динамическая регулировка глубины резкости (DoF / Tilt-Shift) при зуме камеры
fn update_camera_depth_of_field_system(
    controller_query: Query<&CameraController, With<Camera3d>>,
    mut dof_query: Query<&mut DepthOfField, With<Camera3d>>,
) {
    let Ok(controller) = controller_query.single() else {
        return;
    };
    let Ok(mut dof) = dof_query.single_mut() else {
        return;
    };

    dof.focal_distance = controller.distance;

    // При приближении (< 15м) включаем мягкое кинематографическое размытие фона (F-stop 2.8)
    // При отдалении (> 22м) увеличиваем F-stop (диафрагму), чтобы весь тактический обзор был кристально четким
    if controller.distance < 15.0 {
        let t = ((15.0 - controller.distance) / 7.0).clamp(0.0, 1.0);
        dof.aperture_f_stops = 16.0 * (1.0 - t) + 2.8 * t;
    } else {
        dof.aperture_f_stops = 22.0;
    }
}

/// Хелпер для создания камеры с кинематографическим пайплайном (HDR, Bloom, SSAO, Tonemapping, DoF, Fog)
pub fn setup_cinematic_camera_bundle(
    cam_controller: CameraController,
    transform: Transform,
) -> impl Bundle {
    (
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.08, 0.12, 0.18)),
            ..default()
        },
        Tonemapping::TonyMcMapface,
        Bloom {
            intensity: 0.08,
            low_frequency_boost: 0.35,
            high_pass_frequency: 0.92,
            composite_mode: BloomCompositeMode::EnergyConserving,
            ..Bloom::NATURAL
        },
        ColorGrading {
            global: ColorGradingGlobal {
                exposure: -0.05,
                temperature: 0.01,
                tint: 0.0,
                post_saturation: 0.84,
                ..default()
            },
            shadows: ColorGradingSection {
                saturation: 0.80,
                contrast: 0.92,
                lift: 0.045, // Защита от черных провалов: тени мягкие, полупрозрачные и детализированные
                gain: 1.10,
                gamma: 0.95,
            },
            midtones: ColorGradingSection {
                saturation: 0.86,
                contrast: 1.00,
                ..default()
            },
            highlights: ColorGradingSection {
                saturation: 0.88,
                contrast: 0.98,
                ..default()
            },
        },
        Msaa::Off,
        ScreenSpaceAmbientOcclusion::default(),
        bevy::anti_alias::fxaa::Fxaa::default(),
        DistanceFog {
            color: Color::srgb(0.50, 0.53, 0.58),
            directional_light_color: Color::srgba(1.0, 0.75, 0.45, 0.65),
            directional_light_exponent: 14.0,
            falloff: FogFalloff::Linear {
                start: 45.0,
                end: 130.0,
            },
        },
        DepthOfField {
            mode: DepthOfFieldMode::Gaussian,
            focal_distance: 24.0,
            sensor_height: 0.01866,
            aperture_f_stops: 32.0,
            max_circle_of_confusion_diameter: 8.0,
            ..default()
        },
        cam_controller,
        transform,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_clock_phases_and_day_night_cycle() {
        let mut clock = WorldClock {
            time_of_day: 6.0,
            time_speed: 1.0,
            is_paused: false,
        };
        assert_eq!(clock.phase_name(), "Туманный Рассвет");
        assert!(!clock.is_night());

        clock.time_of_day = 12.0;
        assert_eq!(clock.phase_name(), "Пасмурный День");
        assert!(!clock.is_night());

        clock.time_of_day = 18.0;
        assert_eq!(clock.phase_name(), "Кровавый Закат");
        assert!(!clock.is_night());

        clock.time_of_day = 23.0;
        assert_eq!(clock.phase_name(), "Ночь Древних");
        assert!(clock.is_night());

        clock.time_of_day = 2.0;
        assert_eq!(clock.phase_name(), "Ночь Древних");
        assert!(clock.is_night());
    }

    #[test]
    fn test_inverted_sphere_mesh_generation() {
        let mesh = create_inverted_sphere_mesh(100.0, 8, 12);
        assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
        assert!(mesh.indices().is_some());
    }

    #[test]
    fn test_flickering_light_intensity_modulation() {
        let flicker = FlickeringLight::torch(10_000.0);
        assert_eq!(flicker.base_intensity, 10_000.0);
        assert!(flicker.amplitude > 0.0);
        assert!(flicker.speed > 0.0);
    }
}

