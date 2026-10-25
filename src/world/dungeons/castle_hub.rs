use bevy::prelude::*;
use crate::building::{
    spawn_staircase, BuildingDefinition, BuildingFloorRoot, BuildingId, BuildingRegistry,
    BuildingRoof, CutawayAttachedProp, CutawayPillar, CutawayWall, FloorDefinition,
    FloorVisibilityRule,
};
use crate::destruction::{Destructible, ResourceType, StructuralKind, StructuralPiece};
use crate::navigation::{NavWorld, StairLink};
use crate::world::InteriorKitAssets;

/// Строит двухэтажный Замок Исследователя с отрезаемыми стенами, лестницей на 2-й этаж и крышей
pub fn spawn_investigator_castle(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    nav_world: &mut NavWorld,
    building_registry: &mut BuildingRegistry,
    kit: &InteriorKitAssets,
) {
    let castle_id = BuildingId(1);
    let castle_bounds = Rect::from_center_half_size(Vec2::ZERO, Vec2::splat(10.3));
    let floor0_y = 0.08;
    let floor1_base_y = 4.42;
    let floor1_walk_y = 4.50;

    building_registry.register(BuildingDefinition {
        id: castle_id,
        name: "Замок Исследователя",
        bounds: castle_bounds,
        has_exterior_shell: false,
        floors: vec![
            FloorDefinition {
                floor_index: 0,
                elevation: floor0_y,
                wall_height: 4.42,
            },
            FloorDefinition {
                floor_index: 1,
                elevation: floor1_walk_y,
                wall_height: 4.42,
            },
        ],
    });

    nav_world.add_floor_zone(
        Rect::from_center_half_size(Vec2::ZERO, Vec2::splat(9.8)),
        0,
        floor0_y,
    );
    nav_world.add_floor_zone(
        Rect::from_center_half_size(Vec2::ZERO, Vec2::splat(9.8)),
        1,
        floor1_walk_y,
    );

    let rot_90 = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let n_north = Vec2::new(0.0, -1.0);
    let n_south = Vec2::new(0.0, 1.0);
    let n_west = Vec2::new(-1.0, 0.0);
    let n_east = Vec2::new(1.0, 0.0);

    // =========================================================================
    // ПЕРВЫЙ ЭТАЖ ЗАМКА (floor_index = 0)
    // =========================================================================
    commands
        .spawn((
            BuildingFloorRoot {
                building_id: castle_id,
                floor_index: 0,
                rule: FloorVisibilityRule::OnlyWhenActive,
            },
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|floor0| {
            // Плиты пола 1 этажа (5x5 тайлов по 4x4м)
            for gx in -2..=2 {
                for gz in -2..=2 {
                    let pos = Vec3::new(gx as f32 * 4.0, 0.0, gz as f32 * 4.0);
                    floor0.spawn((
                        WorldAssetRoot(kit.floor.clone()),
                        Transform::from_translation(pos),
                        Destructible::floor_tile("Каменная плита 1 этажа"),
                        StructuralPiece::new(StructuralKind::Floor, pos, 0),
                    ));
                }
            }

            // Северная стена (Z = -10.0): 4 стены + центральная арка
            for wx in [-8.0, -4.0, 4.0, 8.0] {
                let col = nav_world.add_box_on_floor(wx, -10.0, 2.0, 0.38, 0);
                let pos = Vec3::new(wx, 0.0, -10.0);
                floor0.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos),
                    CutawayWall::exterior_glb(castle_id, 0, n_north, Vec2::new(wx, -10.0), 0.0),
                    Destructible::wall("Северная стена Замка (1 эт.)", vec![col]),
                    StructuralPiece::new(StructuralKind::Wall, pos, 0),
                ));
            }
            let arch_n_cols = nav_world.add_arch_on_floor(0.0, -10.0, false, 0);
            floor0.spawn((
                WorldAssetRoot(kit.arch.clone()),
                Transform::from_xyz(0.0, 0.0, -10.0),
                CutawayWall::exterior_glb(castle_id, 0, n_north, Vec2::new(0.0, -10.0), 0.0),
                Destructible::arch("Северная арка Замка", arch_n_cols),
                StructuralPiece::new(StructuralKind::Wall, Vec3::new(0.0, 0.0, -10.0), 0),
            ));

            // Южная стена (Z = +10.0): теперь полностью закрыта 4 стенами + парадная арка!
            // Когда персонаж внутри замка, южная стена автоматически «отрезается», открывая вид камере!
            for wx in [-8.0, -4.0, 4.0, 8.0] {
                let col = nav_world.add_box_on_floor(wx, 10.0, 2.0, 0.38, 0);
                let pos = Vec3::new(wx, 0.0, 10.0);
                floor0.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos),
                    CutawayWall::exterior_glb(castle_id, 0, n_south, Vec2::new(wx, 10.0), 0.0),
                    Destructible::wall("Южная стена Замка (1 эт.)", vec![col]),
                    StructuralPiece::new(StructuralKind::Wall, pos, 0),
                ));
            }
            let arch_s_cols = nav_world.add_arch_on_floor(0.0, 10.0, false, 0);
            floor0.spawn((
                WorldAssetRoot(kit.arch.clone()),
                Transform::from_xyz(0.0, 0.0, 10.0),
                CutawayWall::exterior_glb(castle_id, 0, n_south, Vec2::new(0.0, 10.0), 0.0),
                Destructible::arch("Парадная арка Замка", arch_s_cols),
                StructuralPiece::new(StructuralKind::Wall, Vec3::new(0.0, 0.0, 10.0), 0),
            ));

            // Западная стена (X = -10.0)
            for wz in [-8.0, -4.0, 0.0, 4.0, 8.0] {
                let col = nav_world.add_box_on_floor(-10.0, wz, 0.38, 2.0, 0);
                let pos = Vec3::new(-10.0, 0.0, wz);
                floor0.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos).with_rotation(rot_90),
                    CutawayWall::exterior_glb(castle_id, 0, n_west, Vec2::new(-10.0, wz), 0.0),
                    Destructible::wall("Западная стена Замка (1 эт.)", vec![col]),
                    StructuralPiece::new(StructuralKind::Wall, pos, 0),
                ));
            }

            // Восточная стена (X = +10.0) + Восточный портал-арка
            for wz in [-8.0, -4.0, 4.0, 8.0] {
                let col = nav_world.add_box_on_floor(10.0, wz, 0.38, 2.0, 0);
                let pos = Vec3::new(10.0, 0.0, wz);
                floor0.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos).with_rotation(rot_90),
                    CutawayWall::exterior_glb(castle_id, 0, n_east, Vec2::new(10.0, wz), 0.0),
                    Destructible::wall("Восточная стена Замка (1 эт.)", vec![col]),
                    StructuralPiece::new(StructuralKind::Wall, pos, 0),
                ));
            }
            let arch_e_cols = nav_world.add_arch_on_floor(10.0, 0.0, true, 0);
            floor0.spawn((
                WorldAssetRoot(kit.arch.clone()),
                Transform::from_xyz(10.0, 0.0, 0.0).with_rotation(rot_90),
                CutawayWall::exterior_glb(castle_id, 0, n_east, Vec2::new(10.0, 0.0), 0.0),
                Destructible::arch("Восточная арка Замка", arch_e_cols),
                StructuralPiece::new(StructuralKind::Wall, Vec3::new(10.0, 0.0, 0.0), 0),
            ));

            // Колонны периметра и центральные колонны 1 этажа
            let perimeter_pillars = [
                (-10.0, -10.0, vec![n_north, n_west]),
                (-2.2, -10.0, vec![n_north]),
                (2.2, -10.0, vec![n_north]),
                (10.0, -10.0, vec![n_north, n_east]),
                (-10.0, 10.0, vec![n_south, n_west]),
                (-2.2, 10.0, vec![n_south]),
                (2.2, 10.0, vec![n_south]),
                (10.0, 10.0, vec![n_south, n_east]),
            ];
            for (px, pz, normals) in perimeter_pillars {
                let col = nav_world.add_box_on_floor(px, pz, 0.58, 0.58, 0);
                let pos = Vec3::new(px, 0.0, pz);
                floor0.spawn((
                    WorldAssetRoot(kit.pillar.clone()),
                    Transform::from_translation(pos),
                    CutawayPillar::perimeter_glb(castle_id, 0, normals, Vec2::new(px, pz), 0.0),
                    Destructible::pillar("Несущая колонна периметра (1 эт.)", vec![col]),
                    StructuralPiece::new(StructuralKind::Pillar, pos, 0),
                ));
            }
            for (px, pz) in [(-5.0, -4.5), (5.5, -4.5), (-5.0, 4.5), (5.5, 4.5)] {
                let col = nav_world.add_box_on_floor(px, pz, 0.58, 0.58, 0);
                let pos = Vec3::new(px, 0.0, pz);
                floor0.spawn((
                    WorldAssetRoot(kit.pillar.clone()),
                    Transform::from_translation(pos),
                    CutawayPillar::interior_glb(castle_id, 0, Vec2::new(px, pz), 0.0),
                    Destructible::pillar("Колонна зала (1 эт.)", vec![col]),
                    StructuralPiece::new(StructuralKind::Pillar, pos, 0),
                ));
            }

            // Мебель и освещение 1 этажа
            floor0.spawn((
                WorldAssetRoot(kit.rug.clone()),
                Transform::from_xyz(0.0, floor0_y, -1.0),
            ));

            let tbl_col = nav_world.add_box_on_floor(-4.8, -1.5, 1.45, 0.80, 0);
            floor0.spawn((
                WorldAssetRoot(kit.table.clone()),
                Transform::from_xyz(-4.8, floor0_y, -1.5),
                Destructible::furniture(
                    "Стол исследователя с Некрономиконом",
                    vec![tbl_col],
                    vec![(ResourceType::Wood, 3), (ResourceType::Clues, 2)],
                ),
            ));

            // Старая оккультная книга и ручной топор из референсного листа Сыщика на столе исследователя
            floor0.spawn((
                WorldAssetRoot(kit.occult_book.clone()),
                Transform::from_xyz(-4.3, floor0_y + 0.96, -1.35)
                    .with_rotation(Quat::from_rotation_y(0.35)),
            ));
            floor0.spawn((
                WorldAssetRoot(kit.hand_axe.clone()),
                Transform::from_xyz(-5.4, floor0_y + 0.98, -1.25)
                    .with_rotation(Quat::from_euler(EulerRot::XYZ, std::f32::consts::FRAC_PI_2, 0.4, 0.0)),
            ));

            let ch1_col = nav_world.add_box_on_floor(-4.8, -3.0, 0.44, 0.44, 0);
            floor0.spawn((
                WorldAssetRoot(kit.chair.clone()),
                Transform::from_xyz(-4.8, floor0_y, -3.0),
                Destructible::furniture(
                    "Готическое кресло",
                    vec![ch1_col],
                    vec![(ResourceType::Wood, 2)],
                ),
            ));

            let ch2_col = nav_world.add_box_on_floor(-6.8, -1.5, 0.44, 0.44, 0);
            floor0.spawn((
                WorldAssetRoot(kit.chair.clone()),
                Transform::from_xyz(-6.8, floor0_y, -1.5)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                Destructible::furniture(
                    "Готическое кресло",
                    vec![ch2_col],
                    vec![(ResourceType::Wood, 2)],
                ),
            ));

            // Книжные шкафы вдоль северной стены
            for bx in [-6.5, 6.5] {
                let col = nav_world.add_box_on_floor(bx, -9.1, 1.28, 0.40, 0);
                floor0.spawn((
                    WorldAssetRoot(kit.bookcase.clone()),
                    Transform::from_xyz(bx, floor0_y, -9.1),
                    CutawayAttachedProp::new(castle_id, 0, n_north, floor0_y),
                    Destructible::furniture(
                        "Библиотечный шкаф с фолиантами",
                        vec![col],
                        vec![(ResourceType::Wood, 4), (ResourceType::Clues, 3)],
                    ),
                ));
            }

            // Камин у западной стены
            let fp_col = nav_world.add_box_on_floor(-9.0, -1.0, 0.75, 1.78, 0);
            floor0.spawn((
                WorldAssetRoot(kit.fireplace.clone()),
                Transform::from_xyz(-9.0, floor0_y, -1.0)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                CutawayAttachedProp::new(castle_id, 0, n_west, floor0_y),
                Destructible::stone_monolith("Каменный готический камин", vec![fp_col]),
            ));
            floor0.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.52, 0.18),
                    intensity: 85_000.0,
                    range: 14.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(-8.0, 0.9, -1.0),
            ));

            // Алтарь Ктулху в восточном крыле
            let alt_col = nav_world.add_box_on_floor(6.2, -2.0, 1.15, 1.15, 0);
            floor0.spawn((
                WorldAssetRoot(kit.altar.clone()),
                Transform::from_xyz(6.2, floor0_y, -2.0)
                    .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
                Destructible::occult_shrine("Жертвенный Алтарь Ктулху", vec![alt_col]),
            ));
            floor0.spawn((
                PointLight {
                    color: Color::srgb(0.25, 0.95, 0.55),
                    intensity: 65_000.0,
                    range: 12.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(6.2, 2.5, -2.0),
            ));

            // Алхимическая лаборатория в юго-восточном крыле
            let alch_col = nav_world.add_box_on_floor(5.8, 6.8, 1.36, 0.65, 0);
            floor0.spawn((
                WorldAssetRoot(kit.alchemy.clone()),
                Transform::from_xyz(5.8, floor0_y, 6.8),
                Destructible::furniture(
                    "Алхимический верстак с щупальцем",
                    vec![alch_col],
                    vec![
                        (ResourceType::Wood, 3),
                        (ResourceType::Fear, 2),
                        (ResourceType::Clues, 2),
                    ],
                ),
            ));

            // Напольные часы у северной стены
            let clk_col = nav_world.add_box_on_floor(3.4, -9.1, 0.50, 0.40, 0);
            floor0.spawn((
                WorldAssetRoot(kit.clock.clone()),
                Transform::from_xyz(3.4, floor0_y, -9.1),
                CutawayAttachedProp::new(castle_id, 0, n_north, floor0_y),
                Destructible::furniture(
                    "Напольные часы с лунным циферблатом",
                    vec![clk_col],
                    vec![(ResourceType::Wood, 2), (ResourceType::Stone, 1)],
                ),
            ));

            // Канделябры 1 этажа
            for (cx, cz) in [(-3.2, -8.2), (3.2, -8.2), (-3.5, 8.2), (8.0, 8.2)] {
                let col = nav_world.add_box_on_floor(cx, cz, 0.42, 0.42, 0);
                floor0.spawn((
                    WorldAssetRoot(kit.candelabra.clone()),
                    Transform::from_xyz(cx, floor0_y, cz),
                    Destructible::furniture(
                        "Пятирожковый канделябр",
                        vec![col],
                        vec![(ResourceType::Stone, 1)],
                    ),
                ));
                floor0.spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.72, 0.32),
                        intensity: 40_000.0,
                        range: 10.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(cx, 2.2, cz),
                ));
            }
        });

    // Парадная каменная лестница в юго-западном крыле Замка (ведет с 1 на 2 этаж)
    spawn_staircase(
        commands,
        meshes,
        materials,
        nav_world,
        castle_id,
        StairLink {
            lower_floor: 0,
            upper_floor: 1,
            lower_entry: Vec2::new(-7.6, 6.4),
            upper_entry: Vec2::new(-7.6, 1.4),
            lower_y: floor0_y,
            upper_y: floor1_walk_y,
            half_width: 0.95,
        },
        16,
    );

    // =========================================================================
    // ВТОРОЙ ЭТАЖ ЗАМКА (floor_index = 1 — Тайная Библиотека и Верхний Санктум)
    // =========================================================================
    let rail_mesh = meshes.add(Cuboid::new(0.18, 1.0, 4.2));
    let rail_short_mesh = meshes.add(Cuboid::new(2.8, 1.0, 0.18));
    let rail_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.20, 0.19),
        perceptual_roughness: 0.85,
        ..default()
    });

    commands
        .spawn((
            BuildingFloorRoot {
                building_id: castle_id,
                floor_index: 1,
                rule: FloorVisibilityRule::OnlyWhenActive,
            },
            Transform::default(),
            Visibility::Hidden,
        ))
        .with_children(|floor1| {
            // Плиты пола 2 этажа (оставляем лестничный проем в тайле gx = -2, gz = 1)
            for gx in -2..=2 {
                for gz in -2..=2 {
                    if gx == -2 && gz == 1 {
                        continue;
                    }
                    let pos = Vec3::new(gx as f32 * 4.0, floor1_base_y, gz as f32 * 4.0);
                    floor1.spawn((
                        WorldAssetRoot(kit.floor.clone()),
                        Transform::from_translation(pos),
                        Destructible::floor_tile("Плита перекрытия 2 этажа"),
                        StructuralPiece::new(StructuralKind::Floor, pos, 1),
                    ));
                }
            }

            // Каменная балюстрада вокруг лестничного проема на 2 этаже
            floor1.spawn((
                Mesh3d(rail_mesh),
                MeshMaterial3d(rail_mat.clone()),
                Transform::from_xyz(-6.0, floor1_walk_y + 0.5, 4.0),
            ));
            floor1.spawn((
                Mesh3d(rail_short_mesh),
                MeshMaterial3d(rail_mat),
                Transform::from_xyz(-7.4, floor1_walk_y + 0.5, 6.0),
            ));

            // Внешние стены 2 этажа (все 4 стороны замка на высоте floor1_base_y)
            for wx in [-8.0, -4.0, 0.0, 4.0, 8.0] {
                let col_n = nav_world.add_box_on_floor(wx, -10.0, 2.0, 0.38, 1);
                let pos_n = Vec3::new(wx, floor1_base_y, -10.0);
                floor1.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos_n),
                    CutawayWall::exterior_glb(
                        castle_id,
                        1,
                        n_north,
                        Vec2::new(wx, -10.0),
                        floor1_base_y,
                    ),
                    Destructible::wall("Северная стена Замка (2 эт.)", vec![col_n]),
                    StructuralPiece::new(StructuralKind::Wall, pos_n, 1),
                ));

                let col_s = nav_world.add_box_on_floor(wx, 10.0, 2.0, 0.38, 1);
                let pos_s = Vec3::new(wx, floor1_base_y, 10.0);
                floor1.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos_s),
                    CutawayWall::exterior_glb(
                        castle_id,
                        1,
                        n_south,
                        Vec2::new(wx, 10.0),
                        floor1_base_y,
                    ),
                    Destructible::wall("Южная стена Замка (2 эт.)", vec![col_s]),
                    StructuralPiece::new(StructuralKind::Wall, pos_s, 1),
                ));
            }

            for wz in [-8.0, -4.0, 0.0, 4.0, 8.0] {
                let col_w = nav_world.add_box_on_floor(-10.0, wz, 0.38, 2.0, 1);
                let pos_w = Vec3::new(-10.0, floor1_base_y, wz);
                floor1.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos_w).with_rotation(rot_90),
                    CutawayWall::exterior_glb(
                        castle_id,
                        1,
                        n_west,
                        Vec2::new(-10.0, wz),
                        floor1_base_y,
                    ),
                    Destructible::wall("Западная стена Замка (2 эт.)", vec![col_w]),
                    StructuralPiece::new(StructuralKind::Wall, pos_w, 1),
                ));

                let col_e = nav_world.add_box_on_floor(10.0, wz, 0.38, 2.0, 1);
                let pos_e = Vec3::new(10.0, floor1_base_y, wz);
                floor1.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos_e).with_rotation(rot_90),
                    CutawayWall::exterior_glb(
                        castle_id,
                        1,
                        n_east,
                        Vec2::new(10.0, wz),
                        floor1_base_y,
                    ),
                    Destructible::wall("Восточная стена Замка (2 эт.)", vec![col_e]),
                    StructuralPiece::new(StructuralKind::Wall, pos_e, 1),
                ));
            }

            // Внутренняя перегородка 2 этажа (делит 2 этаж на Астральную Библиотеку и Верхний Санктум)
            for wx in [-3.8, 3.8, 7.8] {
                let col = nav_world.add_box_on_floor(wx, 0.0, 1.95, 0.38, 1);
                let pos = Vec3::new(wx, floor1_base_y, 0.0);
                floor1.spawn((
                    WorldAssetRoot(kit.wall.clone()),
                    Transform::from_translation(pos),
                    CutawayWall::interior_glb(
                        castle_id,
                        1,
                        n_south,
                        Vec2::new(wx, 0.0),
                        floor1_base_y,
                    ),
                    Destructible::wall("Внутренняя стена (2 эт.)", vec![col]),
                    StructuralPiece::new(StructuralKind::Wall, pos, 1),
                ));
            }
            let arch_int_cols = nav_world.add_arch_on_floor(0.0, 0.0, false, 1);
            floor1.spawn((
                WorldAssetRoot(kit.arch.clone()),
                Transform::from_xyz(0.0, floor1_base_y, 0.0),
                CutawayWall::interior_glb(castle_id, 1, n_south, Vec2::new(0.0, 0.0), floor1_base_y),
                Destructible::arch("Внутренняя арка (2 эт.)", arch_int_cols),
                StructuralPiece::new(StructuralKind::Wall, Vec3::new(0.0, floor1_base_y, 0.0), 1),
            ));

            // Угловые колонны 2 этажа
            let floor1_pillars = [
                (-10.0, -10.0, vec![n_north, n_west]),
                (10.0, -10.0, vec![n_north, n_east]),
                (-10.0, 10.0, vec![n_south, n_west]),
                (10.0, 10.0, vec![n_south, n_east]),
            ];
            for (px, pz, normals) in floor1_pillars {
                let col = nav_world.add_box_on_floor(px, pz, 0.58, 0.58, 1);
                let pos = Vec3::new(px, floor1_base_y, pz);
                floor1.spawn((
                    WorldAssetRoot(kit.pillar.clone()),
                    Transform::from_translation(pos),
                    CutawayPillar::perimeter_glb(
                        castle_id,
                        1,
                        normals,
                        Vec2::new(px, pz),
                        floor1_base_y,
                    ),
                    Destructible::pillar("Угловая колонна (2 эт.)", vec![col]),
                    StructuralPiece::new(StructuralKind::Pillar, pos, 1),
                ));
            }

            // Интерьер 2 этажа: Северный зал — Тайная Библиотека и Кабинет Астронома
            for bx in [-6.5, -2.5, 2.5, 6.5] {
                let col = nav_world.add_box_on_floor(bx, -9.1, 1.28, 0.40, 1);
                floor1.spawn((
                    WorldAssetRoot(kit.bookcase.clone()),
                    Transform::from_xyz(bx, floor1_walk_y, -9.1),
                    CutawayAttachedProp::new(castle_id, 1, n_north, floor1_walk_y),
                    Destructible::furniture(
                        "Шкаф Тайной Библиотеки",
                        vec![col],
                        vec![(ResourceType::Wood, 4), (ResourceType::Clues, 3)],
                    ),
                ));
            }

            floor1.spawn((
                WorldAssetRoot(kit.rug.clone()),
                Transform::from_xyz(0.0, floor1_walk_y, -5.0),
            ));

            let tbl2_col = nav_world.add_box_on_floor(-3.5, -5.0, 1.45, 0.80, 1);
            floor1.spawn((
                WorldAssetRoot(kit.table.clone()),
                Transform::from_xyz(-3.5, floor1_walk_y, -5.0),
                Destructible::furniture(
                    "Стол астронома",
                    vec![tbl2_col],
                    vec![(ResourceType::Wood, 3), (ResourceType::Clues, 2)],
                ),
            ));

            let ch3_col = nav_world.add_box_on_floor(-3.5, -6.5, 0.44, 0.44, 1);
            floor1.spawn((
                WorldAssetRoot(kit.chair.clone()),
                Transform::from_xyz(-3.5, floor1_walk_y, -6.5),
                Destructible::furniture(
                    "Кресло астронома",
                    vec![ch3_col],
                    vec![(ResourceType::Wood, 2)],
                ),
            ));

            let alch2_col = nav_world.add_box_on_floor(5.2, -5.0, 1.36, 0.65, 1);
            floor1.spawn((
                WorldAssetRoot(kit.alchemy.clone()),
                Transform::from_xyz(5.2, floor1_walk_y, -5.0),
                Destructible::furniture(
                    "Алхимический стол (2 эт.)",
                    vec![alch2_col],
                    vec![(ResourceType::Wood, 3), (ResourceType::Fear, 2)],
                ),
            ));

            // Южный зал 2 этажа — Верхнее Святилище Звездной Мудрости
            floor1.spawn((
                WorldAssetRoot(kit.rug.clone()),
                Transform::from_xyz(2.5, floor1_walk_y, 5.0),
            ));
            let alt2_col = nav_world.add_box_on_floor(2.5, 5.0, 1.15, 1.15, 1);
            floor1.spawn((
                WorldAssetRoot(kit.altar.clone()),
                Transform::from_xyz(2.5, floor1_walk_y, 5.0),
                Destructible::occult_shrine("Алтарь Звездной Мудрости", vec![alt2_col]),
            ));

            let clk2_col = nav_world.add_box_on_floor(9.0, 5.0, 0.40, 0.50, 1);
            floor1.spawn((
                WorldAssetRoot(kit.clock.clone()),
                Transform::from_xyz(9.0, floor1_walk_y, 5.0)
                    .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
                CutawayAttachedProp::new(castle_id, 1, n_east, floor1_walk_y),
                Destructible::furniture(
                    "Астральные часы",
                    vec![clk2_col],
                    vec![(ResourceType::Wood, 2), (ResourceType::Stone, 1)],
                ),
            ));

            // Освещение и канделябры 2 этажа
            for (cx, cz, col) in [
                (-7.5, -7.5, Color::srgb(1.0, 0.76, 0.36)),
                (7.5, -7.5, Color::srgb(1.0, 0.76, 0.36)),
                (7.5, 7.5, Color::srgb(0.35, 0.95, 0.65)),
                (-2.5, 7.5, Color::srgb(1.0, 0.76, 0.36)),
            ] {
                let c_col = nav_world.add_box_on_floor(cx, cz, 0.42, 0.42, 1);
                floor1.spawn((
                    WorldAssetRoot(kit.candelabra.clone()),
                    Transform::from_xyz(cx, floor1_walk_y, cz),
                    Destructible::furniture(
                        "Канделябр 2 этажа",
                        vec![c_col],
                        vec![(ResourceType::Stone, 1)],
                    ),
                ));
                floor1.spawn((
                    PointLight {
                        color: col,
                        intensity: 55_000.0,
                        range: 12.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(cx, floor1_walk_y + 2.2, cz),
                ));
            }
        });

    // =========================================================================
    // КРЫША ЗАМКА (видна снаружи, автоматически скрывается при входе в Замок)
    // =========================================================================
    let roof_y = 8.84;
    let roof_slab_mesh = meshes.add(Cuboid::new(21.2, 0.45, 21.2));
    let roof_crest_mesh = meshes.add(Cuboid::new(14.0, 1.4, 14.0));
    let roof_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.14, 0.15, 0.18),
        perceptual_roughness: 0.9,
        ..default()
    });
    commands
        .spawn((
            BuildingRoof {
                building_id: castle_id,
            },
            Transform::from_xyz(0.0, roof_y, 0.0),
            Visibility::default(),
        ))
        .with_children(|roof| {
            roof.spawn((
                Mesh3d(roof_slab_mesh),
                MeshMaterial3d(roof_mat.clone()),
                Transform::from_xyz(0.0, 0.22, 0.0),
            ));
            roof.spawn((
                Mesh3d(roof_crest_mesh),
                MeshMaterial3d(roof_mat),
                Transform::from_xyz(0.0, 0.95, 0.0),
            ));
        });
}
