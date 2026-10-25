use bevy::prelude::*;
use crate::building::{
    spawn_staircase, BuildingDefinition, BuildingExteriorShell, BuildingFloorRoot, BuildingId,
    BuildingRegistry, CutawayAttachedProp, CutawayWall, FloorDefinition, FloorVisibilityRule,
};
use crate::destruction::{Destructible, ResourceType, StructuralKind, StructuralPiece};
use crate::navigation::{NavWorld, StairLink};
use crate::world::InteriorKitAssets;

/// Переиспользуемый билдер входимого двухэтажного городского дома (Особняк или Гостиница)
/// Снаружи отображается оригинальная 3D-модель здания, а при входе через парадную дверь
/// открывается двухэтажный интерьер с отрезаемыми стенами и лестницей на 2-й этаж!
#[allow(clippy::too_many_arguments)]
pub fn spawn_enterable_town_house(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    nav_world: &mut NavWorld,
    building_registry: &mut BuildingRegistry,
    kit: &InteriorKitAssets,
    building_id: BuildingId,
    name: &'static str,
    center: Vec2,
    door_on_south: bool,
    exterior_shell_scene: Handle<WorldAsset>,
) {
    let bounds = Rect::from_center_half_size(center, Vec2::splat(4.35));
    let floor0_y = 0.08;
    let floor1_base_y = 4.42;
    let floor1_walk_y = 4.50;

    building_registry.register(BuildingDefinition {
        id: building_id,
        name,
        bounds,
        has_exterior_shell: true,
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
        Rect::from_center_half_size(center, Vec2::splat(3.9)),
        0,
        floor0_y,
    );
    nav_world.add_floor_zone(
        Rect::from_center_half_size(center, Vec2::splat(3.9)),
        1,
        floor1_walk_y,
    );

    // 1. Внешняя оболочка дома (видна снаружи, скрывается при входе игрока внутрь)
    let shell_rot = if door_on_south {
        Quat::from_rotation_y(std::f32::consts::PI)
    } else {
        Quat::IDENTITY
    };
    commands.spawn((
        BuildingExteriorShell { building_id },
        WorldAssetRoot(exterior_shell_scene),
        Transform::from_xyz(center.x, 0.0, center.y).with_rotation(shell_rot),
        Visibility::default(),
    ));

    let rot_90 = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let n_north = Vec2::new(0.0, -1.0);
    let n_south = Vec2::new(0.0, 1.0);
    let n_west = Vec2::new(-1.0, 0.0);
    let n_east = Vec2::new(1.0, 0.0);

    // =========================================================================
    // 1 ЭТАЖ ГОРОДСКОГО ДОМА (8x8м, сетка 2x2 тайла)
    // =========================================================================
    commands
        .spawn((
            BuildingFloorRoot {
                building_id,
                floor_index: 0,
                rule: FloorVisibilityRule::OnlyWhenActive,
            },
            Transform::default(),
            Visibility::Hidden,
        ))
        .with_children(|floor0| {
            for gx in [-2.0_f32, 2.0_f32] {
                for gz in [-2.0_f32, 2.0_f32] {
                    let pos = Vec3::new(center.x + gx, 0.0, center.y + gz);
                    floor0.spawn((
                        WorldAssetRoot(kit.floor.clone()),
                        Transform::from_translation(pos),
                        Destructible::floor_tile("Плита пола особняка"),
                        StructuralPiece::new(StructuralKind::Floor, pos, 0),
                    ));
                }
            }

            // Северная и Южная стены 1 этажа (со входом со стороны улицы)
            for (z_off, normal, is_door_side) in [
                (-4.0_f32, n_north, !door_on_south),
                (4.0_f32, n_south, door_on_south),
            ] {
                let wz = center.y + z_off;
                if is_door_side {
                    // Входная арка по центру + боковые простенки
                    let arch_cols = nav_world.add_arch_on_floor(center.x, wz, false, 0);
                    let arch_pos = Vec3::new(center.x, 0.0, wz);
                    floor0.spawn((
                        WorldAssetRoot(kit.arch.clone()),
                        Transform::from_translation(arch_pos),
                        CutawayWall::exterior_glb(building_id, 0, normal, Vec2::new(center.x, wz), 0.0),
                        Destructible::arch("Входная арка особняка", arch_cols),
                        StructuralPiece::new(StructuralKind::Wall, arch_pos, 0),
                    ));

                    for side_x in [-3.0_f32, 3.0_f32] {
                        let col = nav_world.add_box_on_floor(center.x + side_x, wz, 1.0, 0.38, 0);
                        let pos = Vec3::new(center.x + side_x, 0.0, wz);
                        floor0.spawn((
                            WorldAssetRoot(kit.wall.clone()),
                            Transform::from_translation(pos)
                                .with_scale(Vec3::new(0.5, 1.0, 1.0)),
                            CutawayWall::exterior_glb(
                                building_id,
                                0,
                                normal,
                                Vec2::new(center.x + side_x, wz),
                                0.0,
                            ),
                            Destructible::wall("Стена фасада особняка", vec![col]),
                            StructuralPiece::new(StructuralKind::Wall, pos, 0),
                        ));
                    }
                } else {
                    for wx in [-2.0_f32, 2.0_f32] {
                        let col = nav_world.add_box_on_floor(center.x + wx, wz, 2.0, 0.38, 0);
                        let pos = Vec3::new(center.x + wx, 0.0, wz);
                        floor0.spawn((
                            WorldAssetRoot(kit.wall.clone()),
                            Transform::from_translation(pos),
                            CutawayWall::exterior_glb(
                                building_id,
                                0,
                                normal,
                                Vec2::new(center.x + wx, wz),
                                0.0,
                            ),
                            Destructible::wall("Стена особняка (1 эт.)", vec![col]),
                            StructuralPiece::new(StructuralKind::Wall, pos, 0),
                        ));
                    }
                }
            }

            // Западная и Восточная стены 1 этажа
            for (x_off, normal) in [(-4.0_f32, n_west), (4.0_f32, n_east)] {
                let wx = center.x + x_off;
                for wz in [-2.0_f32, 2.0_f32] {
                    let col = nav_world.add_box_on_floor(wx, center.y + wz, 0.38, 2.0, 0);
                    let pos = Vec3::new(wx, 0.0, center.y + wz);
                    floor0.spawn((
                        WorldAssetRoot(kit.wall.clone()),
                        Transform::from_translation(pos).with_rotation(rot_90),
                        CutawayWall::exterior_glb(
                            building_id,
                            0,
                            normal,
                            Vec2::new(wx, center.y + wz),
                            0.0,
                        ),
                        Destructible::wall("Боковая стена особняка (1 эт.)", vec![col]),
                        StructuralPiece::new(StructuralKind::Wall, pos, 0),
                    ));
                }
            }

            // Интерьер 1 этажа (стол, кресло, ковер, канделябр)
            floor0.spawn((
                WorldAssetRoot(kit.rug.clone()),
                Transform::from_xyz(center.x + 0.5, floor0_y, center.y),
            ));
            let tbl_col = nav_world.add_box_on_floor(center.x + 1.6, center.y, 1.35, 0.75, 0);
            floor0.spawn((
                WorldAssetRoot(kit.table.clone()),
                Transform::from_xyz(center.x + 1.6, floor0_y, center.y),
                Destructible::furniture(
                    "Стол особняка",
                    vec![tbl_col],
                    vec![(ResourceType::Wood, 3), (ResourceType::Clues, 1)],
                ),
            ));

            floor0.spawn((
                WorldAssetRoot(kit.occult_book.clone()),
                Transform::from_xyz(center.x + 1.4, floor0_y + 0.96, center.y)
                    .with_rotation(Quat::from_rotation_y(-0.4)),
            ));

            let back_z = if door_on_south {
                center.y - 3.1
            } else {
                center.y + 3.1
            };
            let back_normal = if door_on_south { n_north } else { n_south };
            let back_rot = if door_on_south {
                Quat::IDENTITY
            } else {
                Quat::from_rotation_y(std::f32::consts::PI)
            };
            let bk_col = nav_world.add_box_on_floor(center.x + 1.8, back_z, 1.25, 0.40, 0);
            floor0.spawn((
                WorldAssetRoot(kit.bookcase.clone()),
                Transform::from_xyz(center.x + 1.8, floor0_y, back_z).with_rotation(back_rot),
                CutawayAttachedProp::new(building_id, 0, back_normal, floor0_y),
                Destructible::furniture(
                    "Книжный шкаф особняка",
                    vec![bk_col],
                    vec![(ResourceType::Wood, 3), (ResourceType::Clues, 2)],
                ),
            ));

            floor0.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.76, 0.38),
                    intensity: 45_000.0,
                    range: 10.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(center.x + 0.8, 2.4, center.y),
            ));
        });

    // Межэтажная лестница вдоль западной стены дома (с 1 на 2 этаж)
    let (stair_lower_z, stair_upper_z) = if door_on_south {
        (center.y + 1.8, center.y - 1.8)
    } else {
        (center.y - 1.8, center.y + 1.8)
    };
    spawn_staircase(
        commands,
        meshes,
        materials,
        nav_world,
        building_id,
        StairLink {
            lower_floor: 0,
            upper_floor: 1,
            lower_entry: Vec2::new(center.x - 2.4, stair_lower_z),
            upper_entry: Vec2::new(center.x - 2.4, stair_upper_z),
            lower_y: floor0_y,
            upper_y: floor1_walk_y,
            half_width: 0.72,
        },
        12,
    );

    // =========================================================================
    // 2 ЭТАЖ ГОРОДСКОГО ДОМА (мансарда / тайная комната, floor_index = 1)
    // =========================================================================
    commands
        .spawn((
            BuildingFloorRoot {
                building_id,
                floor_index: 1,
                rule: FloorVisibilityRule::OnlyWhenActive,
            },
            Transform::default(),
            Visibility::Hidden,
        ))
        .with_children(|floor1| {
            // Плиты 2 этажа (оставляем лестничный проем над нижней половиной лестницы)
            for gx in [-2.0_f32, 2.0_f32] {
                for gz in [-2.0_f32, 2.0_f32] {
                    let is_stair_hole = gx < 0.0
                        && ((door_on_south && gz > 0.0) || (!door_on_south && gz < 0.0));
                    if is_stair_hole {
                        continue;
                    }
                    let pos = Vec3::new(center.x + gx, floor1_base_y, center.y + gz);
                    floor1.spawn((
                        WorldAssetRoot(kit.floor.clone()),
                        Transform::from_translation(pos),
                        Destructible::floor_tile("Плита 2 этажа особняка"),
                        StructuralPiece::new(StructuralKind::Floor, pos, 1),
                    ));
                }
            }

            // Стены периметра 2 этажа
            for (z_off, normal) in [(-4.0_f32, n_north), (4.0_f32, n_south)] {
                let wz = center.y + z_off;
                for wx in [-2.0_f32, 2.0_f32] {
                    let col = nav_world.add_box_on_floor(center.x + wx, wz, 2.0, 0.38, 1);
                    let pos = Vec3::new(center.x + wx, floor1_base_y, wz);
                    floor1.spawn((
                        WorldAssetRoot(kit.wall.clone()),
                        Transform::from_translation(pos),
                        CutawayWall::exterior_glb(
                            building_id,
                            1,
                            normal,
                            Vec2::new(center.x + wx, wz),
                            floor1_base_y,
                        ),
                        Destructible::wall("Стена 2 этажа особняка", vec![col]),
                        StructuralPiece::new(StructuralKind::Wall, pos, 1),
                    ));
                }
            }
            for (x_off, normal) in [(-4.0_f32, n_west), (4.0_f32, n_east)] {
                let wx = center.x + x_off;
                for wz in [-2.0_f32, 2.0_f32] {
                    let col = nav_world.add_box_on_floor(wx, center.y + wz, 0.38, 2.0, 1);
                    let pos = Vec3::new(wx, floor1_base_y, center.y + wz);
                    floor1.spawn((
                        WorldAssetRoot(kit.wall.clone()),
                        Transform::from_translation(pos).with_rotation(rot_90),
                        CutawayWall::exterior_glb(
                            building_id,
                            1,
                            normal,
                            Vec2::new(wx, center.y + wz),
                            floor1_base_y,
                        ),
                        Destructible::wall("Боковая стена 2 этажа особняка", vec![col]),
                        StructuralPiece::new(StructuralKind::Wall, pos, 1),
                    ));
                }
            }

            // Обстановка 2 этажа дома: оккультная лаборатория и часы
            let alch_col = nav_world.add_box_on_floor(center.x + 1.6, center.y - 1.5, 1.30, 0.62, 1);
            floor1.spawn((
                WorldAssetRoot(kit.alchemy.clone()),
                Transform::from_xyz(center.x + 1.6, floor1_walk_y, center.y - 1.5),
                Destructible::furniture(
                    "Лаборатория особняка",
                    vec![alch_col],
                    vec![(ResourceType::Wood, 2), (ResourceType::Fear, 2)],
                ),
            ));

            let cand_col = nav_world.add_box_on_floor(center.x + 2.4, center.y + 2.2, 0.42, 0.42, 1);
            floor1.spawn((
                WorldAssetRoot(kit.candelabra.clone()),
                Transform::from_xyz(center.x + 2.4, floor1_walk_y, center.y + 2.2),
                Destructible::furniture(
                    "Канделябр мансарды",
                    vec![cand_col],
                    vec![(ResourceType::Stone, 1)],
                ),
            ));

            floor1.spawn((
                PointLight {
                    color: Color::srgb(0.45, 0.95, 0.65),
                    intensity: 45_000.0,
                    range: 10.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(center.x + 1.2, floor1_walk_y + 2.3, center.y),
            ));
        });
}
