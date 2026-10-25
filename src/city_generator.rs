use bevy::prelude::*;
use crate::destruction::{Destructible, ResourceType};
use crate::navigation::NavWorld;
use crate::terrain::{Biome, ChunkCoord, ChunkMember, WorldTerrain, CHUNK_SIZE};
use crate::water_cellular::WaterWorld;
use crate::world::dungeons::catacombs::{DefenseEnemy, EnemyKind};

/// Набор 3D-ассетов для процедурной генерации городов Иннсмута и Оккультных Деревень
#[derive(Resource, Clone)]
#[allow(dead_code)]
pub struct ProceduralCityAssets {
    // Городские ассеты Иннсмута / Аркхэма (Викторианский готический квартал)
    pub townhouse: Handle<WorldAsset>,
    pub shop: Handle<WorldAsset>,
    pub path: Handle<WorldAsset>,
    pub fence: Handle<WorldAsset>,
    pub gate: Handle<WorldAsset>,
    pub lamppost: Handle<WorldAsset>,
    pub bench: Handle<WorldAsset>,
    pub kiosk: Handle<WorldAsset>,
    pub phonebooth: Handle<WorldAsset>,
    pub cart: Handle<WorldAsset>,
    pub well: Handle<WorldAsset>,

    // Ассеты Оккультной Деревни Ведьм и Заброшенного Поселения (Haunted Kit)
    pub witch_cottage: Handle<WorldAsset>,
    pub witch_tower: Handle<WorldAsset>,
    pub canopy_shelter: Handle<WorldAsset>,
    pub doghouse: Handle<WorldAsset>,
    pub chimney: Handle<WorldAsset>,
    pub cauldron_large: Handle<WorldAsset>,
    pub cauldron_small: Handle<WorldAsset>,
    pub firewood_large: Handle<WorldAsset>,
    pub firewood_small: Handle<WorldAsset>,
    pub pumpkin_jack: Handle<WorldAsset>,
    pub pumpkin_plain: Handle<WorldAsset>,
    pub lantern_green: Handle<WorldAsset>,
}

/// Роль чанка в регулярной планировочной структуре города
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CityChunkRole {
    /// Городская площадь / центральный перекресток с монументом, колодцем или фонтаном
    TownSquare,
    /// Главный перекресток двух проспектов
    Intersection,
    /// Проспект Север-Юг (вдоль оси Z)
    AvenueNS,
    /// Проспект Восток-Запад (вдоль оси X)
    AvenueEW,
    /// Застраиваемый жилой квартал с особняками / хижинами и внутренними дворами
    BuildingDistrict,
}

/// Определяет архитектурную роль чанка по его мировым координатам (шаг сетки 3x3 чанка = 48x48 м)
pub fn determine_city_chunk_role(coord: ChunkCoord) -> CityChunkRole {
    let mx = coord.cx.rem_euclid(3);
    let mz = coord.cz.rem_euclid(3);

    if mx == 0 && mz == 0 {
        // Каждые 6 чанков — просторная городская площадь
        if (coord.cx.div_euclid(3) + coord.cz.div_euclid(3)).rem_euclid(2) == 0 {
            CityChunkRole::TownSquare
        } else {
            CityChunkRole::Intersection
        }
    } else if mx == 0 {
        CityChunkRole::AvenueNS
    } else if mz == 0 {
        CityChunkRole::AvenueEW
    } else {
        CityChunkRole::BuildingDistrict
    }
}

/// Процедурно заселяет чанк городского биома домами, улицами, фонарями, колодцами и жителями
#[allow(clippy::too_many_arguments)]
pub fn populate_city_chunk(
    commands: &mut Commands,
    terrain: &WorldTerrain,
    nav_world: &mut NavWorld,
    water_world: &mut Option<ResMut<WaterWorld>>,
    assets: &ProceduralCityAssets,
    coord: ChunkCoord,
    biome: Biome,
) {
    let min_xz = coord.min_world_xz();
    let center_xz = min_xz + Vec2::splat(CHUNK_SIZE * 0.5);
    let center_y = terrain.sample_height(center_xz.x, center_xz.y);
    let role = determine_city_chunk_role(coord);
    let seed = terrain.seed ^ (coord.cx as u32).wrapping_mul(73856093) ^ (coord.cz as u32).wrapping_mul(19349663);

    let is_witch_village = biome == Biome::WitchVillage;
    let path_handle = assets.path.clone();

    match role {
        CityChunkRole::TownSquare => {
            // Мощеная брусчаткой площадь 4x4 тайла в центре чанка
            for px in [-4.5, -1.5, 1.5, 4.5] {
                for pz in [-4.5, -1.5, 1.5, 4.5] {
                    let wx = center_xz.x + px;
                    let wz = center_xz.y + pz;
                    let wy = terrain.sample_height(wx, wz);
                    commands.spawn((
                        WorldAssetRoot(path_handle.clone()),
                        Transform::from_xyz(wx, wy, wz),
                        ChunkMember(coord),
                    ));
                }
            }

            // Центральный объект площади
            if is_witch_village {
                // Большой бурлящий оккультный котел с зеленым мистическим пламенем
                let col = nav_world.add_box(center_xz.x, center_xz.y, 1.4, 1.4);
                commands
                    .spawn((
                        WorldAssetRoot(assets.cauldron_large.clone()),
                        Transform::from_xyz(center_xz.x, center_y, center_xz.y),
                        Destructible::occult_shrine("Жертвенный Котёл Ведьм", vec![col]),
                        ChunkMember(coord),
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            PointLight {
                                color: Color::srgb(0.20, 0.98, 0.55),
                                intensity: 85_000.0,
                                range: 14.0,
                                shadow_maps_enabled: false,
                                ..default()
                            },
                            Transform::from_xyz(0.0, 1.8, 0.0),
                        ));
                    });

                // Светящиеся тыквы Джека вокруг котла
                for (dx, dz) in [(-2.4, -2.4), (2.4, -2.4), (-2.4, 2.4), (2.4, 2.4)] {
                    let tx = center_xz.x + dx;
                    let tz = center_xz.y + dz;
                    let ty = terrain.sample_height(tx, tz);
                    let p_col = nav_world.add_box(tx, tz, 0.5, 0.5);
                    commands
                        .spawn((
                            WorldAssetRoot(assets.pumpkin_jack.clone()),
                            Transform::from_xyz(tx, ty, tz)
                                .with_rotation(Quat::from_rotation_y(dx * 1.5)),
                            Destructible::furniture("Фонарь из тыквы", vec![p_col], vec![(ResourceType::Foliage, 2), (ResourceType::Fear, 1)]),
                            ChunkMember(coord),
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                PointLight {
                                    color: Color::srgb(1.0, 0.65, 0.15),
                                    intensity: 25_000.0,
                                    range: 6.0,
                                    shadow_maps_enabled: false,
                                    ..default()
                                },
                                Transform::from_xyz(0.0, 0.6, 0.0),
                            ));
                        });
                }
            } else {
                // Городской колодец Иннсмута — активный источник воды Minecraft-style!
                let col = nav_world.add_box(center_xz.x, center_xz.y, 1.35, 1.35);
                if let Some(ww) = water_world.as_deref_mut() {
                    ww.add_source(center_xz, terrain);
                }
                commands
                    .spawn((
                        WorldAssetRoot(assets.well.clone()),
                        Transform::from_xyz(center_xz.x, center_y, center_xz.y),
                        Destructible::occult_shrine("Городской Колодец Иннсмута", vec![col]),
                        ChunkMember(coord),
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            PointLight {
                                color: Color::srgb(0.35, 0.85, 0.95),
                                intensity: 45_000.0,
                                range: 10.0,
                                shadow_maps_enabled: false,
                                ..default()
                            },
                            Transform::from_xyz(0.0, 1.8, 0.0),
                        ));
                    });

                // Афишная тумба и телефонная будка на углах площади
                let k_col = nav_world.add_box(center_xz.x - 4.5, center_xz.y - 4.5, 0.9, 0.9);
                commands.spawn((
                    WorldAssetRoot(assets.kiosk.clone()),
                    Transform::from_xyz(center_xz.x - 4.5, center_y, center_xz.y - 4.5),
                    Destructible::furniture("Городская афишная тумба", vec![k_col], vec![(ResourceType::Wood, 2), (ResourceType::Clues, 2)]),
                    ChunkMember(coord),
                ));

                let pb_col = nav_world.add_box(center_xz.x + 4.5, center_xz.y - 4.5, 0.75, 0.75);
                commands.spawn((
                    WorldAssetRoot(assets.phonebooth.clone()),
                    Transform::from_xyz(center_xz.x + 4.5, center_y, center_xz.y - 4.5),
                    Destructible::street_prop("Телефонная будка 1920-х", vec![pb_col]),
                    ChunkMember(coord),
                ));
            }

            // Фонари на 4 углах площади
            spawn_corner_streetlights(commands, nav_world, assets, center_xz, terrain, coord, is_witch_village);
        }

        CityChunkRole::Intersection => {
            // Перекресток дорог: вымощенные крестом плиты
            for offset in [-6.0, -3.0, 0.0, 3.0, 6.0] {
                // Вдоль X
                let wx = center_xz.x + offset;
                let wz = center_xz.y;
                let wy = terrain.sample_height(wx, wz);
                commands.spawn((
                    WorldAssetRoot(path_handle.clone()),
                    Transform::from_xyz(wx, wy, wz),
                    ChunkMember(coord),
                ));
                // Вдоль Z
                let wx_z = center_xz.x;
                let wz_z = center_xz.y + offset;
                let wy_z = terrain.sample_height(wx_z, wz_z);
                commands.spawn((
                    WorldAssetRoot(path_handle.clone()),
                    Transform::from_xyz(wx_z, wy_z, wz_z),
                    ChunkMember(coord),
                ));
            }

            // Угловые фонари перекрестка
            spawn_corner_streetlights(commands, nav_world, assets, center_xz, terrain, coord, is_witch_village);
        }

        CityChunkRole::AvenueNS => {
            // Проспект Север-Юг (вдоль Z): 2 параллельных ряда брусчатки
            for pz in [-6.0, -3.0, 0.0, 3.0, 6.0] {
                for px in [-1.5, 1.5] {
                    let wx = center_xz.x + px;
                    let wz = center_xz.y + pz;
                    let wy = terrain.sample_height(wx, wz);
                    commands.spawn((
                        WorldAssetRoot(path_handle.clone()),
                        Transform::from_xyz(wx, wy, wz),
                        ChunkMember(coord),
                    ));
                }
            }

            // Уличные фонари и скамейки на обочине
            let lx = center_xz.x - 4.2;
            let lz = center_xz.y;
            let ly = terrain.sample_height(lx, lz);
            spawn_streetlamp(commands, nav_world, assets, lx, ly, lz, coord, is_witch_village);

            let bx = center_xz.x + 4.0;
            let bz = center_xz.y;
            let by = terrain.sample_height(bx, bz);
            if is_witch_village {
                let col = nav_world.add_box(bx, bz, 1.2, 0.6);
                commands.spawn((
                    WorldAssetRoot(assets.firewood_large.clone()),
                    Transform::from_xyz(bx, by, bz),
                    Destructible::furniture("Штабель дров", vec![col], vec![(ResourceType::Wood, 3)]),
                    ChunkMember(coord),
                ));
            } else {
                let col = nav_world.add_box(bx, bz, 1.2, 0.45);
                commands.spawn((
                    WorldAssetRoot(assets.bench.clone()),
                    Transform::from_xyz(bx, by, bz)
                        .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
                    Destructible::street_prop("Уличная скамейка", vec![col]),
                    ChunkMember(coord),
                ));
            }
        }

        CityChunkRole::AvenueEW => {
            // Проспект Восток-Запад (вдоль X)
            for px in [-6.0, -3.0, 0.0, 3.0, 6.0] {
                for pz in [-1.5, 1.5] {
                    let wx = center_xz.x + px;
                    let wz = center_xz.y + pz;
                    let wy = terrain.sample_height(wx, wz);
                    commands.spawn((
                        WorldAssetRoot(path_handle.clone()),
                        Transform::from_xyz(wx, wy, wz),
                        ChunkMember(coord),
                    ));
                }
            }

            // Фонарь на северной обочине
            let lx = center_xz.x;
            let lz = center_xz.y + 4.2;
            let ly = terrain.sample_height(lx, lz);
            spawn_streetlamp(commands, nav_world, assets, lx, ly, lz, coord, is_witch_village);

            // Телега или навес на южной обочине
            let cx = center_xz.x;
            let cz = center_xz.y - 4.0;
            let cy = terrain.sample_height(cx, cz);
            if is_witch_village {
                let col = nav_world.add_box(cx, cz, 1.8, 1.4);
                commands.spawn((
                    WorldAssetRoot(assets.canopy_shelter.clone()),
                    Transform::from_xyz(cx, cy, cz)
                        .with_rotation(Quat::from_rotation_y(0.35)),
                    Destructible::building_structure("Навес ведьмы", vec![col]),
                    ChunkMember(coord),
                ));
            } else {
                let col = nav_world.add_box(cx, cz, 1.4, 0.9);
                commands.spawn((
                    WorldAssetRoot(assets.cart.clone()),
                    Transform::from_xyz(cx, cy, cz)
                        .with_rotation(Quat::from_rotation_y(0.35)),
                    Destructible::furniture("Телега горожанина", vec![col], vec![(ResourceType::Wood, 3)]),
                    ChunkMember(coord),
                ));
            }
        }

        CityChunkRole::BuildingDistrict => {
            // Застраиваемый квартал с домами, заборами и внутренними дворами!
            let r_building = WorldTerrain::hash2d(coord.cx * 19, coord.cz * 37, seed);
            let b_pos = Vec3::new(center_xz.x, center_y, center_xz.y);

            // Ориентация здания: фасад обращен к ближайшей улице
            let yaw = if coord.cx.rem_euclid(3) == 1 {
                // Улица слева (на западе) -> фасад на запад (-PI/2)
                -std::f32::consts::FRAC_PI_2
            } else {
                // Улица справа (на востоке) -> фасад на восток (+PI/2)
                std::f32::consts::FRAC_PI_2
            };

            if is_witch_village {
                // Оккультная деревня: Хижина Ведьмы либо Башня Ведьмы
                if r_building < 0.65 {
                    let col = nav_world.add_box(b_pos.x, b_pos.z, 3.8, 3.4);
                    commands
                        .spawn((
                            WorldAssetRoot(assets.witch_cottage.clone()),
                            Transform::from_xyz(b_pos.x, b_pos.y, b_pos.z)
                                .with_rotation(Quat::from_rotation_y(yaw)),
                            Destructible::building_structure(
                                "Хижина Ведьмы",
                                vec![col],
                            ),
                            ChunkMember(coord),
                        ))
                        .with_children(|parent| {
                            // Зловещий изумрудный свет из окон
                            parent.spawn((
                                PointLight {
                                    color: Color::srgb(0.30, 0.98, 0.62),
                                    intensity: 65_000.0,
                                    range: 12.0,
                                    shadow_maps_enabled: false,
                                    ..default()
                                },
                                Transform::from_xyz(0.0, 2.2, 0.0),
                            ));
                            // Дымоход на крыше хижины
                            parent.spawn((
                                WorldAssetRoot(assets.chimney.clone()),
                                Transform::from_xyz(1.2, 2.5, 0.8),
                            ));
                        });
                } else {
                    let col = nav_world.add_box(b_pos.x, b_pos.z, 2.8, 2.8);
                    commands
                        .spawn((
                            WorldAssetRoot(assets.witch_tower.clone()),
                            Transform::from_xyz(b_pos.x, b_pos.y, b_pos.z)
                                .with_rotation(Quat::from_rotation_y(yaw)),
                            Destructible::building_structure(
                                "Сторожевая Башня Ведьм",
                                vec![col],
                            ),
                            ChunkMember(coord),
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                PointLight {
                                    color: Color::srgb(0.40, 0.98, 0.50),
                                    intensity: 85_000.0,
                                    range: 16.0,
                                    shadow_maps_enabled: false,
                                    ..default()
                                },
                                Transform::from_xyz(0.0, 6.0, 0.0),
                            ));
                        });
                }

                // Дворовые постройки деревни: будка оборотня, малый котел, тыквы
                let yard_x = center_xz.x + if yaw > 0.0 { -4.5 } else { 4.5 };
                let yard_z = center_xz.y + 4.0;
                let yard_y = terrain.sample_height(yard_x, yard_z);
                let dog_col = nav_world.add_box(yard_x, yard_z, 0.9, 0.9);
                commands.spawn((
                    WorldAssetRoot(assets.doghouse.clone()),
                    Transform::from_xyz(yard_x, yard_y, yard_z),
                    Destructible::furniture("Конура адской гончей", vec![dog_col], vec![(ResourceType::Wood, 2)]),
                    ChunkMember(coord),
                ));

                // Малый котел во дворе
                let c_col = nav_world.add_box(yard_x, yard_z - 3.5, 0.7, 0.7);
                commands.spawn((
                    WorldAssetRoot(assets.cauldron_small.clone()),
                    Transform::from_xyz(yard_x, terrain.sample_height(yard_x, yard_z - 3.5), yard_z - 3.5),
                    Destructible::furniture("Котелок с зельем", vec![c_col], vec![(ResourceType::Fear, 1)]),
                    ChunkMember(coord),
                ));

                // Поленница дров и спелая тыква во дворе ведьмы
                let firewood_model = if r_building < 0.5 {
                    assets.firewood_large.clone()
                } else {
                    assets.firewood_small.clone()
                };
                let f_col = nav_world.add_box(yard_x, yard_z - 5.5, 0.8, 0.8);
                commands.spawn((
                    WorldAssetRoot(firewood_model),
                    Transform::from_xyz(yard_x, terrain.sample_height(yard_x, yard_z - 5.5), yard_z - 5.5),
                    Destructible::furniture("Поленница дров", vec![f_col], vec![(ResourceType::Wood, 3)]),
                    ChunkMember(coord),
                ));

                let p_col = nav_world.add_box(yard_x + 1.2, yard_z, 0.45, 0.45);
                commands.spawn((
                    WorldAssetRoot(assets.pumpkin_plain.clone()),
                    Transform::from_xyz(yard_x + 1.2, terrain.sample_height(yard_x + 1.2, yard_z), yard_z),
                    Destructible::furniture("Спелая тыква", vec![p_col], vec![(ResourceType::Foliage, 2)]),
                    ChunkMember(coord),
                ));
            } else {
                // Городские кварталы Иннсмута: Двухэтажные таунхаусы и лавки Ордена
                let selected_model = if r_building < 0.65 {
                    assets.townhouse.clone()
                } else {
                    assets.shop.clone()
                };

                let col = nav_world.add_box(b_pos.x, b_pos.z, 3.8, 3.8);
                commands
                    .spawn((
                        WorldAssetRoot(selected_model),
                        Transform::from_xyz(b_pos.x, b_pos.y, b_pos.z)
                            .with_rotation(Quat::from_rotation_y(yaw)),
                        Destructible::building_structure(
                            "Городской Особняк Иннсмута",
                            vec![col],
                        ),
                        ChunkMember(coord),
                    ))
                    .with_children(|parent| {
                        // Теплый вечерний свет из окон
                        parent.spawn((
                            PointLight {
                                color: Color::srgb(1.0, 0.78, 0.42),
                                intensity: 55_000.0,
                                range: 11.0,
                                shadow_maps_enabled: false,
                                ..default()
                            },
                            Transform::from_xyz(0.0, 2.5, 0.0),
                        ));
                    });

                // Задний двор с кованой оградой, калиткой и колодцем
                let yard_x = center_xz.x + if yaw > 0.0 { -4.8 } else { 4.8 };
                let fence_col = nav_world.add_box(yard_x, center_xz.y - 1.5, 0.35, 2.5);
                commands.spawn((
                    WorldAssetRoot(assets.fence.clone()),
                    Transform::from_xyz(yard_x, center_y, center_xz.y - 1.5)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                    Destructible::stone_monolith("Чугунная ограда особняка", vec![fence_col]),
                    ChunkMember(coord),
                ));

                let gate_col = nav_world.add_box(yard_x, center_xz.y + 1.5, 0.35, 1.2);
                commands.spawn((
                    WorldAssetRoot(assets.gate.clone()),
                    Transform::from_xyz(yard_x, center_y, center_xz.y + 1.5)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                    Destructible::stone_monolith("Кованая калитка двора", vec![gate_col]),
                    ChunkMember(coord),
                ));

                // 35% шанс колодца во дворе дома с источником воды Minecraft!
                let r_well = WorldTerrain::hash2d(coord.cx * 31, coord.cz * 47, seed);
                if r_well > 0.65 {
                    let wx = yard_x;
                    let wz = center_xz.y + 4.2;
                    let wy = terrain.sample_height(wx, wz);
                    let w_col = nav_world.add_box(wx, wz, 1.25, 1.25);
                    if let Some(ww) = water_world.as_deref_mut() {
                        ww.add_source(Vec2::new(wx, wz), terrain);
                    }
                    commands.spawn((
                        WorldAssetRoot(assets.well.clone()),
                        Transform::from_xyz(wx, wy, wz),
                        Destructible::occult_shrine("Дворовый колодец Бездны", vec![w_col]),
                        ChunkMember(coord),
                    ));
                }
            }

            // Шанс появления патруля врагов на прилегающей улице (25%)
            let r_mob = WorldTerrain::hash2d(coord.cx * 43, coord.cz * 67, seed);
            if r_mob > 0.75 {
                let mob_pos = Vec3::new(center_xz.x, center_y + 0.75, center_xz.y - 5.5);
                commands.spawn((
                    DefenseEnemy {
                        kind: if is_witch_village { EnemyKind::Cultist } else { EnemyKind::DeepOne },
                        hp: 55.0,
                        max_hp: 55.0,
                        speed: 2.1,
                        damage: 10.0,
                        attack_cooldown: 1.0,
                        water_push_force: Vec2::ZERO,
                    },
                    Transform::from_translation(mob_pos),
                    ChunkMember(coord),
                ));
            }
        }
    }
}

/// Спавнит уличный фонарь с коллизией и источником света
fn spawn_streetlamp(
    commands: &mut Commands,
    nav_world: &mut NavWorld,
    assets: &ProceduralCityAssets,
    x: f32,
    y: f32,
    z: f32,
    coord: ChunkCoord,
    is_witch_village: bool,
) {
    let col = nav_world.add_box(x, z, 0.40, 0.40);
    let (model, light_col, intensity) = if is_witch_village {
        (assets.lantern_green.clone(), Color::srgb(0.25, 0.98, 0.55), 45_000.0)
    } else {
        (assets.lamppost.clone(), Color::srgb(1.0, 0.75, 0.35), 55_000.0)
    };

    commands
        .spawn((
            WorldAssetRoot(model),
            Transform::from_xyz(x, y, z),
            Destructible::street_prop("Городской фонарь", vec![col]),
            ChunkMember(coord),
        ))
        .with_children(|parent| {
            parent.spawn((
                PointLight {
                    color: light_col,
                    intensity,
                    range: 12.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(0.0, 3.2, 0.0),
            ));
        });
}

/// Спавнит 4 угловых фонаря вокруг перекрестка/площади
fn spawn_corner_streetlights(
    commands: &mut Commands,
    nav_world: &mut NavWorld,
    assets: &ProceduralCityAssets,
    center_xz: Vec2,
    terrain: &WorldTerrain,
    coord: ChunkCoord,
    is_witch_village: bool,
) {
    for (dx, dz) in [(-5.5, -5.5), (5.5, -5.5), (-5.5, 5.5), (5.5, 5.5)] {
        let lx = center_xz.x + dx;
        let lz = center_xz.y + dz;
        let ly = terrain.sample_height(lx, lz);
        spawn_streetlamp(commands, nav_world, assets, lx, ly, lz, coord, is_witch_village);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_city_chunk_role_determination() {
        // Проверяем регулярность сетки 3x3
        assert_eq!(determine_city_chunk_role(ChunkCoord::new(0, 0)), CityChunkRole::TownSquare);
        assert_eq!(determine_city_chunk_role(ChunkCoord::new(3, 0)), CityChunkRole::Intersection);
        assert_eq!(determine_city_chunk_role(ChunkCoord::new(3, 3)), CityChunkRole::TownSquare);
        assert_eq!(determine_city_chunk_role(ChunkCoord::new(0, 1)), CityChunkRole::AvenueNS);
        assert_eq!(determine_city_chunk_role(ChunkCoord::new(0, 2)), CityChunkRole::AvenueNS);
        assert_eq!(determine_city_chunk_role(ChunkCoord::new(1, 0)), CityChunkRole::AvenueEW);
        assert_eq!(determine_city_chunk_role(ChunkCoord::new(2, 0)), CityChunkRole::AvenueEW);
        assert_eq!(determine_city_chunk_role(ChunkCoord::new(1, 1)), CityChunkRole::BuildingDistrict);
        assert_eq!(determine_city_chunk_role(ChunkCoord::new(2, 2)), CityChunkRole::BuildingDistrict);
    }
}
