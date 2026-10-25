use bevy::prelude::*;
use crate::building::{
    spawn_staircase, BuildingDefinition, BuildingFloorRoot, BuildingId, BuildingRegistry,
    CutawayPillar, CutawayWall, FloorDefinition, FloorVisibilityRule,
};
use crate::destruction::{Destructible, ResourceType, StructuralKind, StructuralPiece};
use crate::navigation::{NavWorld, StairLink};
use crate::water_cellular::WaterWorld;
use crate::world::InteriorKitAssets;

/// Древний Алтарь Призыва (центр обороны Замка/Катакомб)
#[derive(Component, Debug, Clone)]
pub struct AltarOfSummoning {
    pub ritual_active: bool,
    pub current_wave: u32,
    pub total_waves: u32,
    pub wave_countdown: f32,
    pub enemies_remaining: u32,
    pub altar_hp: f32,
    pub max_hp: f32,
}

impl Default for AltarOfSummoning {
    fn default() -> Self {
        Self {
            ritual_active: false,
            current_wave: 0,
            total_waves: 3,
            wave_countdown: 0.0,
            enemies_remaining: 0,
            altar_hp: 500.0,
            max_hp: 500.0,
        }
    }
}

/// Разлом Бездны — портал появления волн врагов в Катакомбах
#[derive(Component, Debug, Clone)]
pub struct VoidRiftPortal {
    pub portal_id: u32,
    pub spawn_timer: f32,
    pub spawn_interval: f32,
    pub pending_spawns: u32,
}

/// Виды порождений Бездны в оборонных волнах
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyKind {
    DeepOne,
    Cultist,
    VoidBeast,
}

/// Враг, идущий по коридору Катакомб к Алтарю Призыва
#[derive(Component, Debug, Clone)]
pub struct DefenseEnemy {
    pub kind: EnemyKind,
    pub hp: f32,
    pub max_hp: f32,
    pub speed: f32,
    pub damage: f32,
    pub attack_cooldown: f32,
    pub water_push_force: Vec2,
}

/// Строит многоуровневые Древние Катакомбы под Замком/Погостом с Алтарем Призыва и Разломами Бездны
#[allow(clippy::too_many_arguments)]
pub fn spawn_catacombs(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    nav_world: &mut NavWorld,
    building_registry: &mut BuildingRegistry,
    water_world: &mut WaterWorld,
    kit: &InteriorKitAssets,
) {
    let catacombs_id = BuildingId(10);
    let center = Vec2::new(0.0, -32.0);
    let floor_neg1_y = -6.0;
    let bounds = Rect::from_center_half_size(center, Vec2::splat(14.0));

    building_registry.register(BuildingDefinition {
        id: catacombs_id,
        name: "Древние Катакомбы Бездны",
        bounds,
        has_exterior_shell: false,
        floors: vec![FloorDefinition {
            floor_index: -1,
            elevation: floor_neg1_y,
            wall_height: 5.5,
        }],
    });

    nav_world.add_floor_zone(
        Rect::from_center_half_size(center, Vec2::splat(13.2)),
        -1,
        floor_neg1_y,
    );

    // Спуск в Катакомбы (лестница из склепа или северного крыла)
    spawn_staircase(
        commands,
        meshes,
        materials,
        nav_world,
        catacombs_id,
        StairLink {
            lower_floor: -1,
            upper_floor: 0,
            lower_entry: Vec2::new(center.x, center.y + 11.2),
            upper_entry: Vec2::new(center.x, center.y + 16.8),
            lower_y: floor_neg1_y,
            upper_y: 0.08,
            half_width: 1.25,
        },
        20,
    );

    let rot_90 = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let n_north = Vec2::new(0.0, -1.0);
    let n_south = Vec2::new(0.0, 1.0);
    let n_west = Vec2::new(-1.0, 0.0);
    let n_east = Vec2::new(1.0, 0.0);

    commands
        .spawn((
            BuildingFloorRoot {
                building_id: catacombs_id,
                floor_index: -1,
                rule: FloorVisibilityRule::OnlyWhenActive,
            },
            Transform::default(),
            Visibility::Hidden,
        ))
        .with_children(|dungeon| {
            // 1. Каменный пол подземелья (сетка 7x7 тайлов по 4м)
            for gx in -3..=3 {
                for gz in -3..=3 {
                    let pos = Vec3::new(
                        center.x + gx as f32 * 4.0,
                        floor_neg1_y,
                        center.y + gz as f32 * 4.0,
                    );
                    dungeon.spawn((
                        WorldAssetRoot(kit.floor.clone()),
                        Transform::from_translation(pos),
                        Destructible::floor_tile("Древняя базальтовая плита"),
                        StructuralPiece::new(StructuralKind::Floor, pos, -1),
                    ));
                }
            }

            // 2. Внешние базальтовые стены периметра Катакомб
            for i in -3_i32..=3_i32 {
                let off = i as f32 * 4.0;
                // Северная стена
                let pos_n = Vec3::new(center.x + off, floor_neg1_y, center.y - 14.0);
                let col_n = nav_world.add_box_on_floor(pos_n.x, pos_n.z, 2.0, 0.45, -1);
                dungeon.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos_n),
                    CutawayWall::exterior_glb(catacombs_id, -1, n_north, Vec2::new(pos_n.x, pos_n.z), floor_neg1_y),
                    Destructible::wall("Стена катакомб", vec![col_n]),
                    StructuralPiece::new(StructuralKind::Wall, pos_n, -1),
                ));

                // Южная стена (за исключением проема лестницы)
                if i.abs() > 0 {
                    let pos_s = Vec3::new(center.x + off, floor_neg1_y, center.y + 14.0);
                    let col_s = nav_world.add_box_on_floor(pos_s.x, pos_s.z, 2.0, 0.45, -1);
                    dungeon.spawn((
                        WorldAssetRoot(kit.wall.clone()),
                        Transform::from_translation(pos_s),
                        CutawayWall::exterior_glb(catacombs_id, -1, n_south, Vec2::new(pos_s.x, pos_s.z), floor_neg1_y),
                        Destructible::wall("Стена катакомб", vec![col_s]),
                        StructuralPiece::new(StructuralKind::Wall, pos_s, -1),
                    ));
                }

                // Западная стена
                let pos_w = Vec3::new(center.x - 14.0, floor_neg1_y, center.y + off);
                let col_w = nav_world.add_box_on_floor(pos_w.x, pos_w.z, 0.45, 2.0, -1);
                dungeon.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos_w).with_rotation(rot_90),
                    CutawayWall::exterior_glb(catacombs_id, -1, n_west, Vec2::new(pos_w.x, pos_w.z), floor_neg1_y),
                    Destructible::wall("Стена катакомб", vec![col_w]),
                    StructuralPiece::new(StructuralKind::Wall, pos_w, -1),
                ));

                // Восточная стена
                let pos_e = Vec3::new(center.x + 14.0, floor_neg1_y, center.y + off);
                let col_e = nav_world.add_box_on_floor(pos_e.x, pos_e.z, 0.45, 2.0, -1);
                dungeon.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos_e).with_rotation(rot_90),
                    CutawayWall::exterior_glb(catacombs_id, -1, n_east, Vec2::new(pos_e.x, pos_e.z), floor_neg1_y),
                    Destructible::wall("Стена катакомб", vec![col_e]),
                    StructuralPiece::new(StructuralKind::Wall, pos_e, -1),
                ));
            }

            // 3. Несущие готические колонны зала
            for px in [-6.0_f32, 6.0_f32] {
                for pz in [-6.0_f32, 6.0_f32] {
                    let pos = Vec3::new(center.x + px, floor_neg1_y, center.y + pz);
                    let col = nav_world.add_box_on_floor(pos.x, pos.z, 0.65, 0.65, -1);
                    dungeon.spawn((
                        WorldAssetRoot(kit.pillar.clone()),
                        Transform::from_translation(pos),
                        CutawayPillar::interior_glb(catacombs_id, -1, Vec2::new(pos.x, pos.z), floor_neg1_y),
                        Destructible::pillar("Монолитная колонна подземелья", vec![col]),
                        StructuralPiece::new(StructuralKind::Pillar, pos, -1),
                    ));
                }
            }

            // 4. Древний Алтарь Призыва (Центральный объект обороны)
            let altar_pos = Vec3::new(center.x, floor_neg1_y, center.y);
            let altar_col = nav_world.add_box_on_floor(altar_pos.x, altar_pos.z, 1.45, 1.45, -1);
            dungeon
                .spawn((
                    WorldAssetRoot(kit.altar.clone()),
                    Transform::from_translation(altar_pos).with_scale(Vec3::splat(1.4)),
                    AltarOfSummoning::default(),
                    Destructible::occult_shrine("Древний Алтарь Призыва", vec![altar_col]),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        PointLight {
                            color: Color::srgb(0.20, 0.95, 0.65),
                            intensity: 95_000.0,
                            range: 16.0,
                            shadow_maps_enabled: false,
                            ..default()
                        },
                        Transform::from_xyz(0.0, 2.5, 0.0),
                    ));
                });

            // 5. Разломы Бездны (точки появления врагов волны на западе и востоке)
            let rift_west = Vec3::new(center.x - 10.5, floor_neg1_y + 0.1, center.y);
            dungeon.spawn((
                VoidRiftPortal {
                    portal_id: 1,
                    spawn_timer: 1.5,
                    spawn_interval: 2.4,
                    pending_spawns: 0,
                },
                Transform::from_translation(rift_west),
                PointLight {
                    color: Color::srgb(0.95, 0.15, 0.35),
                    intensity: 75_000.0,
                    range: 11.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
            ));

            let rift_east = Vec3::new(center.x + 10.5, floor_neg1_y + 0.1, center.y);
            dungeon.spawn((
                VoidRiftPortal {
                    portal_id: 2,
                    spawn_timer: 2.5,
                    spawn_interval: 2.4,
                    pending_spawns: 0,
                },
                Transform::from_translation(rift_east),
                PointLight {
                    color: Color::srgb(0.95, 0.15, 0.35),
                    intensity: 75_000.0,
                    range: 11.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
            ));

            // 6. Освещение и канделябры в Катакомбах
            for (cx, cz) in [
                (-10.0, -10.0),
                (10.0, -10.0),
                (-10.0, 10.0),
                (10.0, 10.0),
            ] {
                let pos = Vec3::new(center.x + cx, floor_neg1_y, center.y + cz);
                let col = nav_world.add_box_on_floor(pos.x, pos.z, 0.42, 0.42, -1);
                dungeon.spawn((
                    WorldAssetRoot(kit.candelabra.clone()),
                    Transform::from_translation(pos),
                    Destructible::furniture("Канделябр катакомб", vec![col], vec![(ResourceType::Stone, 1)]),
                ));
                dungeon.spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.65, 0.32),
                        intensity: 45_000.0,
                        range: 11.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_translation(pos + Vec3::Y * 2.2),
                ));
            }
        });

    // Добавляем подземный поток воды для питания ловушек и сноса врагов
    water_world.cells.insert(
        crate::water_cellular::WaterWorld::world_to_grid(center.x - 8.0, center.y),
        crate::water_cellular::WaterCell {
            level: 8,
            ground_y: floor_neg1_y,
            is_source: true,
            falling: false,
            flow_dir: Vec2::X,
            buff_active: false,
            buff_timer: 0.0,
        },
    );
    water_world.sources.insert(crate::water_cellular::WaterWorld::world_to_grid(center.x - 8.0, center.y));
}
