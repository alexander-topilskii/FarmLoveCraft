use std::collections::VecDeque;
use bevy::prelude::*;
use crate::building::{
    spawn_staircase, BuildingDefinition, BuildingExteriorShell, BuildingFloorRoot, BuildingHudText,
    BuildingId, BuildingRegistry, BuildingRoof, CutawayAttachedProp, CutawayPillar, CutawayWall,
    FloorDefinition, FloorVisibilityRule,
};
use crate::camera::CameraController;
use crate::destruction::{
    Destructible, ResourceType, StructuralIntegrityDirty, StructuralKind, StructuralPiece,
};
use crate::navigation::{NavWorld, StairLink};
use crate::player::{on_character_scene_ready, CharacterAnimations, Player};
use crate::rts_command::{BuildableKitAssets, InventoryHudText};
use crate::terrain::{ChunkCoord, ProceduralBiomeAssets, TerrainChunkEntity, WorldTerrain};

/// Набор переиспользуемых ассетов для сборки многоэтажных интерьеров
#[derive(Clone)]
struct InteriorKitAssets {
    floor: Handle<WorldAsset>,
    wall: Handle<WorldAsset>,
    pillar: Handle<WorldAsset>,
    arch: Handle<WorldAsset>,
    table: Handle<WorldAsset>,
    chair: Handle<WorldAsset>,
    bookcase: Handle<WorldAsset>,
    rug: Handle<WorldAsset>,
    candelabra: Handle<WorldAsset>,
    altar: Handle<WorldAsset>,
    fireplace: Handle<WorldAsset>,
    alchemy: Handle<WorldAsset>,
    clock: Handle<WorldAsset>,
    hand_axe: Handle<WorldAsset>,
    occult_book: Handle<WorldAsset>,
}

pub fn setup_world(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut nav_world: ResMut<NavWorld>,
    mut building_registry: ResMut<BuildingRegistry>,
    mut terrain: ResMut<WorldTerrain>,
    mut integrity_dirty: ResMut<StructuralIntegrityDirty>,
) {
    // 1. Создаем граф анимаций со скелетными клипами Idle и Walk из нашего GLB
    let idle_clip: Handle<AnimationClip> =
        asset_server.load(GltfAssetLabel::Animation(0).from_asset("models/detective.glb"));
    let walk_clip: Handle<AnimationClip> =
        asset_server.load(GltfAssetLabel::Animation(1).from_asset("models/detective.glb"));

    let mut graph = AnimationGraph::new();
    let idle_node = graph.add_clip(idle_clip, 1.0, graph.root);
    let walk_node = graph.add_clip(walk_clip, 1.0, graph.root);
    let graph_handle = graphs.add(graph);

    commands.insert_resource(CharacterAnimations {
        graph_handle,
        idle_node,
        walk_node,
    });

    // 2. Генерируемый чанковый рельеф мира (по принципу Valheim — с поддержкой копки и насыпи)
    let terrain_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.92,
        metallic: 0.02,
        ..default()
    });
    terrain.terrain_material = Some(terrain_mat.clone());

    for cz in -3..=5 {
        for cx in -4..=4 {
            let coord = ChunkCoord::new(cx, cz);
            terrain.sync_chunk_to_nav(coord, &mut nav_world);
            let chunk_mesh = terrain.build_chunk_mesh(coord);
            let chunk_entity = commands
                .spawn((
                    TerrainChunkEntity { coord },
                    Mesh3d(meshes.add(chunk_mesh)),
                    MeshMaterial3d(terrain_mat.clone()),
                    Transform::default(),
                ))
                .id();
            terrain.spawned_chunks.insert(coord, chunk_entity);
        }
    }

    let kit = InteriorKitAssets {
        floor: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_floor.glb")),
        wall: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_wall.glb")),
        pillar: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_pillar.glb")),
        arch: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_arch.glb")),
        table: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_table.glb")),
        chair: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_chair.glb")),
        bookcase: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_bookcase.glb")),
        rug: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_rug.glb")),
        candelabra: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_candelabra.glb")),
        altar: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_altar.glb")),
        fireplace: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_fireplace.glb")),
        alchemy: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_alchemy.glb")),
        clock: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_clock.glb")),
        hand_axe: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/prop_hand_axe.glb")),
        occult_book: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/prop_occult_book.glb")),
    };

    // =========================================================================
    // 1. ДВУХЭТАЖНЫЙ ЗАМОК ИССЛЕДОВАТЕЛЯ (20x20 метров, 1 и 2 этажи + крыша)
    // =========================================================================
    spawn_investigator_castle(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut nav_world,
        &mut building_registry,
        &kit,
    );

    // =========================================================================
    // 2. УЛИЦА / ПОМЕСТЬЕ, КАНАЛ ЩУПАЛЕЦ И ГОРОД АРКХЭМ / ИННСМУТ
    // =========================================================================
    let rot_90 = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let bench_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_bench.glb"));
    let lamppost_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_lamppost.glb"));
    let tree_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_tree.glb"));
    let bush_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_bush.glb"));
    let fence_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_fence.glb"));
    let gravestone_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_gravestone.glb"));
    let well_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_well.glb"));
    let path_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_path.glb"));
    let gazebo_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_gazebo.glb"));
    let cart_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_cart.glb"));
    let townhouse_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_townhouse.glb"));
    let shop_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_shop.glb"));
    let fountain_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_fountain.glb"));
    let gate_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_gate.glb"));
    let kiosk_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_kiosk.glb"));
    let crypt_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_crypt.glb"));
    let bridge_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_bridge.glb"));
    let canal_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_canal.glb"));
    let church_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_church.glb"));
    let university_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_university.glb"));
    let inn_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_inn.glb"));
    let clocktower_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_clocktower.glb"));
    let carriage_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_carriage.glb"));
    let warehouse_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_warehouse.glb"));
    let phonebooth_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_phonebooth.glb"));
    let trawler_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_trawler.glb"));
    let dock_cargo_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_dock_cargo.glb"));
    let cthulhu_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_cthulhu.glb"));

    commands.insert_resource(ProceduralBiomeAssets {
        tree: tree_scene.clone(),
        bush: bush_scene.clone(),
        gravestone: gravestone_scene.clone(),
        gazebo: gazebo_scene.clone(),
        well: well_scene.clone(),
        crypt: crypt_scene.clone(),
    });

    commands.insert_resource(BuildableKitAssets {
        wall: kit.wall.clone(),
        arch: kit.arch.clone(),
        pillar: kit.pillar.clone(),
        floor: kit.floor.clone(),
        candelabra: kit.candelabra.clone(),
        bush_tentacle: bush_scene.clone(),
    });

    // Брусчатые проспекты и площади (тайл 3.0 x 3.0м, проходимые)
    for pz in [12.5, 15.8, 19.0, 27.5, 30.8, 39.2, 42.5, 45.8, 49.0, 52.2, 58.0, 61.5] {
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(0.0, 0.0, pz),
        ));
    }
    for i in -11..=11 {
        if i == 0 {
            continue;
        }
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(i as f32 * 3.0, 0.0, 35.0),
        ));
    }
    for i in -10..=10 {
        if i == 0 {
            continue;
        }
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(i as f32 * 3.0, 0.0, 55.0),
        ));
    }
    for px in [12.5, 15.8, 19.0] {
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(px, 0.0, 0.0).with_rotation(rot_90),
        ));
    }

    // Кованые ворота поместья и ограда кладбища (Z = 10.0)
    for gx in [-6.2, 6.2] {
        let c1 = nav_world.add_box(gx - 1.85, 10.0, 0.55, 0.55);
        let c2 = nav_world.add_box(gx + 1.85, 10.0, 0.55, 0.55);
        commands.spawn((
            WorldAssetRoot(gate_scene.clone()),
            Transform::from_xyz(gx, 0.0, 10.0),
            Destructible::stone_monolith("Кованые ворота поместья", vec![c1, c2]),
        ));
    }
    for fx in [-19.0, -15.0, -11.0, 11.0, 15.0, 19.0] {
        let col = nav_world.add_box(fx, 10.0, 2.05, 0.35);
        commands.spawn((
            WorldAssetRoot(fence_scene.clone()),
            Transform::from_xyz(fx, 0.0, 10.0),
            Destructible::stone_monolith("Кованая ограда погоста", vec![col]),
        ));
    }
    for fz in [12.0, 16.0, 20.0] {
        let col = nav_world.add_box(-21.0, fz, 0.35, 2.05);
        commands.spawn((
            WorldAssetRoot(fence_scene.clone()),
            Transform::from_xyz(-21.0, 0.0, fz).with_rotation(rot_90),
            Destructible::stone_monolith("Кованая ограда погоста", vec![col]),
        ));
    }

    // Каменный готический склеп и надгробия на западном погосте
    let crypt_col = nav_world.add_box(-16.5, 15.0, 2.65, 2.30);
    commands.spawn((
        WorldAssetRoot(crypt_scene),
        Transform::from_xyz(-16.5, 0.0, 15.0).with_rotation(rot_90),
        Destructible::occult_shrine("Готический склеп предков", vec![crypt_col]),
    ));

    for (gx, gz, gyaw) in [
        (-11.5, 13.5, 0.15),
        (-11.8, 17.0, -0.20),
        (-15.5, 19.8, 0.05),
        (-11.5, 20.2, 0.25),
    ] {
        let col = nav_world.add_box(gx, gz, 0.78, 0.55);
        commands.spawn((
            WorldAssetRoot(gravestone_scene.clone()),
            Transform::from_xyz(gx, 0.0, gz).with_rotation(Quat::from_rotation_y(gyaw)),
            Destructible::stone_monolith("Кельтское надгробие", vec![col]),
        ));
    }

    // Проклятый колодец со щупальцами на восточном дворе
    let well_col = nav_world.add_box(22.5, 0.0, 1.35, 1.35);
    commands.spawn((
        WorldAssetRoot(well_scene),
        Transform::from_xyz(22.5, 0.0, 0.0).with_rotation(Quat::from_rotation_y(0.4)),
        Destructible::occult_shrine("Колодец со щупальцами", vec![well_col]),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(0.25, 0.95, 0.55),
            intensity: 70_000.0,
            range: 14.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(22.5, 2.0, 0.0),
    ));

    // Затопленная циклопическая ротонда Древних в восточном парке
    let gazebo_col = nav_world.add_box(16.5, 16.0, 2.60, 2.60);
    commands.spawn((
        WorldAssetRoot(gazebo_scene),
        Transform::from_xyz(16.5, 0.0, 16.0).with_rotation(Quat::from_rotation_y(-0.35)),
        Destructible::occult_shrine("Циклопическая ротонда Древних", vec![gazebo_col]),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(0.25, 0.95, 0.58),
            intensity: 75_000.0,
            range: 14.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(16.5, 3.0, 16.0),
    ));

    // Проточный каменный канал и два горбатых моста
    for bx in [0.0, 28.0] {
        let c1 = nav_world.add_box(bx - 1.58, 23.5, 0.30, 2.45);
        let c2 = nav_world.add_box(bx + 1.58, 23.5, 0.30, 2.45);
        commands.spawn((
            WorldAssetRoot(bridge_scene.clone()),
            Transform::from_xyz(bx, 0.0, 23.5),
            Destructible::stone_monolith("Каменный горбатый мост", vec![c1, c2]),
        ));
    }
    for cx in [-26.5, -21.5, -16.5, -11.5, -6.5, 6.5, 11.5, 16.5, 21.5, 34.5] {
        let col = nav_world.add_box(cx, 23.5, 2.45, 1.95);
        commands.spawn((
            WorldAssetRoot(canal_scene.clone()),
            Transform::from_xyz(cx, 0.0, 23.5),
            Destructible::occult_shrine("Канал со всходами щупалец", vec![col]),
        ));
    }

    // Доковые склады «Рыбопромышленной компании Марша»
    let wh_col1 = nav_world.add_box(35.0, 29.5, 3.70, 4.50);
    commands.spawn((
        WorldAssetRoot(warehouse_scene.clone()),
        Transform::from_xyz(35.0, 0.0, 29.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
        Destructible::building_structure("Портовый склад Марша (Восток)", vec![wh_col1]),
    ));

    let wh_col2 = nav_world.add_box(-35.0, 29.5, 3.70, 4.50);
    commands.spawn((
        WorldAssetRoot(warehouse_scene),
        Transform::from_xyz(-35.0, 0.0, 29.5).with_rotation(rot_90),
        Destructible::building_structure("Портовый склад Марша (Запад)", vec![wh_col2]),
    ));

    // =========================================================================
    // ПОРТ ИННСМУТА ИЗ РЕФЕРЕНСА: Рыбацкие траулеры, замшелые бочки/ящики,
    // ручной топор, старая оккультная книга и колосс Ктулху над горизонтом!
    // =========================================================================
    // Два ржавых рыбацких траулера («Innsmouth Fishing Trawler»), пришвартованных в канале
    for (bx, bz, byaw) in [
        (-14.0_f32, 23.5_f32, std::f32::consts::FRAC_PI_2 - 0.12),
        (14.0_f32, 23.5_f32, -std::f32::consts::FRAC_PI_2 + 0.10),
    ] {
        commands.spawn((
            WorldAssetRoot(trawler_scene.clone()),
            Transform::from_xyz(bx, -0.15, bz).with_rotation(Quat::from_rotation_y(byaw)),
            Destructible::street_prop("Рыбацкий траулер Иннсмута", Vec::new()),
        ));
    }

    // Причальные грузы с морской тиной (бочки, ящики контрабандистов и кнехты)
    let cargo_spots = [
        (-4.8_f32, 20.2_f32, 0.25_f32),
        (4.8_f32, 26.8_f32, -0.45_f32),
        (29.5_f32, 28.0_f32, 1.10_f32),
        (-29.5_f32, 28.0_f32, -0.85_f32),
    ];
    for (cx, cz, cyaw) in cargo_spots {
        let col = nav_world.add_box(cx, cz, 1.55, 1.15);
        commands.spawn((
            WorldAssetRoot(dock_cargo_scene.clone()),
            Transform::from_xyz(cx, 0.0, cz).with_rotation(Quat::from_rotation_y(cyaw)),
            Destructible::furniture(
                "Ящики контрабандистов в тине",
                vec![col],
                vec![(ResourceType::Wood, 3), (ResourceType::Clues, 1)],
            ),
        ));
    }

    // Ручной топор («Prop_HandAxe») из референса, воткнутый в ящик на причале у моста
    commands.spawn((
        WorldAssetRoot(kit.hand_axe.clone()),
        Transform::from_xyz(-3.8, 0.58, 20.1)
            .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.15, 0.65, -0.35)),
    ));

    // Исполинский крылатый силуэт Ктулху («Outdoor_CthulhuHorizon») в тумане на дальнем горизонте
    commands.spawn((
        WorldAssetRoot(cthulhu_scene),
        Transform::from_xyz(0.0, -0.5, -68.0),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(0.22, 0.92, 0.58),
            intensity: 180_000.0,
            range: 38.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 18.0, -62.0),
    ));

    // Большой Оккультный Фонтан на перекрестке (X = 0, Z = 35.0)
    let fount_col = nav_world.add_box(0.0, 35.0, 2.55, 2.55);
    commands.spawn((
        WorldAssetRoot(fountain_scene),
        Transform::from_xyz(0.0, 0.0, 35.0),
        Destructible::occult_shrine("Оккультный Фонтан Гаргулий", vec![fount_col]),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(0.22, 0.95, 0.62),
            intensity: 80_000.0,
            range: 16.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 3.8, 35.0),
    ));

    // =========================================================================
    // ВХОДИМЫЕ ДВУХЭТАЖНЫЕ ДОМА И ГОСТИНИЦЫ НА КУПЕЧЕСКОМ ПРОСПЕКТЕ
    // =========================================================================
    // Два двухэтажных особняка (северный ряд, Z = 28.5, вход с юга со стороны улицы Z = 35.0)
    spawn_enterable_town_house(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut nav_world,
        &mut building_registry,
        &kit,
        BuildingId(2),
        "Особняк Уэйтли",
        Vec2::new(-23.5, 28.5),
        true,
        townhouse_scene.clone(),
    );
    spawn_enterable_town_house(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut nav_world,
        &mut building_registry,
        &kit,
        BuildingId(3),
        "Особняк Марша",
        Vec2::new(23.5, 28.5),
        true,
        townhouse_scene.clone(),
    );

    // Две лавки на северной стороне Первой улицы
    for sx in [-13.5, 13.5] {
        let col = nav_world.add_box(sx, 28.5, 3.90, 3.80);
        commands.spawn((
            WorldAssetRoot(shop_scene.clone()),
            Transform::from_xyz(sx, 0.0, 28.5)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            Destructible::building_structure("Лавка Ордена Дагона", vec![col]),
        ));
    }

    // Две двухэтажные гостиницы «Gilman House Inn» (южный ряд, Z = 44.5, вход с севера со стороны улицы Z = 35.0)
    spawn_enterable_town_house(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut nav_world,
        &mut building_registry,
        &kit,
        BuildingId(4),
        "Гостиница «Gilman House» (Запад)",
        Vec2::new(-10.0, 44.5),
        false,
        inn_scene.clone(),
    );
    spawn_enterable_town_house(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut nav_world,
        &mut building_registry,
        &kit,
        BuildingId(5),
        "Гостиница «Gilman House» (Восток)",
        Vec2::new(10.0, 44.5),
        false,
        inn_scene,
    );

    for sx in [-20.0, 20.0] {
        let col = nav_world.add_box(sx, 44.5, 3.90, 3.80);
        commands.spawn((
            WorldAssetRoot(shop_scene.clone()),
            Transform::from_xyz(sx, 0.0, 44.5),
            Destructible::building_structure("Портовая лавка Иннсмута", vec![col]),
        ));
    }
    for hx in [-30.0, 30.0] {
        let col = nav_world.add_box(hx, 44.5, 4.25, 3.90);
        commands.spawn((
            WorldAssetRoot(townhouse_scene.clone()),
            Transform::from_xyz(hx, 0.0, 44.5),
            Destructible::building_structure("Дом Иннсмута", vec![col]),
        ));
    }

    // Городская Часовая Башня Аркхэма
    let tower_col = nav_world.add_box(0.0, 55.0, 2.38, 2.38);
    commands.spawn((
        WorldAssetRoot(clocktower_scene),
        Transform::from_xyz(0.0, 0.0, 55.0),
        Destructible::building_structure("Часовая Башня Аркхэма", vec![tower_col]),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(0.95, 0.82, 0.42),
            intensity: 90_000.0,
            range: 18.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 10.5, 55.0),
    ));

    // Готический Собор «Тайного Ордена Дагона»
    let church_col = nav_world.add_box(0.0, 68.5, 5.10, 5.80);
    commands.spawn((
        WorldAssetRoot(church_scene),
        Transform::from_xyz(0.0, 0.0, 68.5),
        Destructible::building_structure("Собор Тайного Ордена Дагона", vec![church_col]),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(0.65, 0.25, 0.85),
            intensity: 95_000.0,
            range: 20.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 6.8, 61.5),
    ));

    // Корпуса Библиотеки и Обсерватории Мискатоникского Университета
    for ux in [-14.5, 14.5] {
        let col = nav_world.add_box(ux, 66.5, 5.65, 4.40);
        commands.spawn((
            WorldAssetRoot(university_scene.clone()),
            Transform::from_xyz(ux, 0.0, 66.5),
            Destructible::building_structure("Корпус Мискатоникского Университета", vec![col]),
        ));
    }

    for hx in [-26.5, 26.5] {
        let col = nav_world.add_box(hx, 65.5, 4.25, 3.90);
        commands.spawn((
            WorldAssetRoot(townhouse_scene.clone()),
            Transform::from_xyz(hx, 0.0, 65.5),
            Destructible::building_structure("Старый особняк Иннсмута", vec![col]),
        ));
    }

    // Винтажные автомобили-катафалки 1920-х годов
    let carriage_positions = [
        (-8.5, 33.2, std::f32::consts::FRAC_PI_2, 2.55, 1.35),
        (12.5, 53.2, -std::f32::consts::FRAC_PI_2, 2.55, 1.35),
        (-3.8, 60.5, 0.15, 1.35, 2.55),
    ];
    for (cx, cz, cyaw, hx, hz) in carriage_positions {
        let col = nav_world.add_box(cx, cz, hx, hz);
        commands.spawn((
            WorldAssetRoot(carriage_scene.clone()),
            Transform::from_xyz(cx, 0.0, cz).with_rotation(Quat::from_rotation_y(cyaw)),
            Destructible::street_prop("Автомобиль-катафалк 1920-х", vec![col]),
        ));
    }

    // Фермерские телеги с урожаем
    let cart_col1 = nav_world.add_box(14.5, -5.5, 1.25, 1.95);
    commands.spawn((
        WorldAssetRoot(cart_scene.clone()),
        Transform::from_xyz(14.5, 0.0, -5.5).with_rotation(Quat::from_rotation_y(0.55)),
        Destructible::furniture(
            "Телега с урожаем спор",
            vec![cart_col1],
            vec![(ResourceType::Wood, 3), (ResourceType::Fear, 2)],
        ),
    ));

    let cart_col2 = nav_world.add_box(-24.5, 38.2, 1.95, 1.25);
    commands.spawn((
        WorldAssetRoot(cart_scene),
        Transform::from_xyz(-24.5, 0.0, 38.2).with_rotation(Quat::from_rotation_y(1.25)),
        Destructible::furniture(
            "Телега с урожаем спор",
            vec![cart_col2],
            vec![(ResourceType::Wood, 3), (ResourceType::Fear, 2)],
        ),
    ));

    // Афишные тумбы и телефонные будки
    for (kx, kz) in [(-6.5, 31.2), (6.5, 38.8), (-6.2, 58.5)] {
        let col = nav_world.add_box(kx, kz, 0.95, 0.95);
        commands.spawn((
            WorldAssetRoot(kiosk_scene.clone()),
            Transform::from_xyz(kx, 0.0, kz),
            Destructible::furniture(
                "Афишная тумба «Arkham Gazette»",
                vec![col],
                vec![(ResourceType::Wood, 2), (ResourceType::Clues, 2)],
            ),
        ));
    }
    for (px, pz) in [(6.5, 31.2), (-6.2, 51.2), (6.2, 58.5)] {
        let col = nav_world.add_box(px, pz, 0.78, 0.78);
        commands.spawn((
            WorldAssetRoot(phonebooth_scene.clone()),
            Transform::from_xyz(px, 0.0, pz),
            Destructible::street_prop("Телефонная будка 1920-х", vec![col]),
        ));
    }

    // Уличные газовые фонари
    let lamp_positions = [
        (-3.6, 13.5, 0.0),
        (3.6, 13.5, std::f32::consts::PI),
        (-3.6, 19.5, 0.0),
        (3.6, 19.5, std::f32::consts::PI),
        (-9.5, 31.8, std::f32::consts::FRAC_PI_2),
        (9.5, 31.8, std::f32::consts::FRAC_PI_2),
        (-19.5, 38.2, -std::f32::consts::FRAC_PI_2),
        (19.5, 38.2, -std::f32::consts::FRAC_PI_2),
        (-9.5, 51.8, std::f32::consts::FRAC_PI_2),
        (9.5, 51.8, std::f32::consts::FRAC_PI_2),
        (-19.5, 58.2, -std::f32::consts::FRAC_PI_2),
        (19.5, 58.2, -std::f32::consts::FRAC_PI_2),
        (17.5, -3.5, -std::f32::consts::FRAC_PI_2),
    ];
    for (lx, lz, lyaw) in lamp_positions {
        let col = nav_world.add_box(lx, lz, 0.36, 0.36);
        commands.spawn((
            WorldAssetRoot(lamppost_scene.clone()),
            Transform::from_xyz(lx, 0.0, lz).with_rotation(Quat::from_rotation_y(lyaw)),
            Destructible::street_prop("Чугунный газовый фонарь", vec![col]),
        ));
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.80, 0.38),
                intensity: 65_000.0,
                range: 14.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(lx, 3.8, lz),
        ));
    }

    // Уличные скамейки
    let bench_positions = [
        (-4.5, 16.5, rot_90, 0.48, 1.24),
        (4.5, 16.5, Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2), 0.48, 1.24),
        (-5.2, 35.0, rot_90, 0.48, 1.24),
        (5.2, 35.0, Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2), 0.48, 1.24),
        (-14.5, 51.5, Quat::IDENTITY, 1.24, 0.48),
        (14.5, 51.5, Quat::IDENTITY, 1.24, 0.48),
        (22.5, -4.5, Quat::IDENTITY, 1.24, 0.48),
    ];
    for (bx, bz, brot, hx, hz) in bench_positions {
        let col = nav_world.add_box(bx, bz, hx, hz);
        commands.spawn((
            WorldAssetRoot(bench_scene.clone()),
            Transform::from_xyz(bx, 0.0, bz).with_rotation(brot),
            Destructible::street_prop("Викторианская скамейка", vec![col]),
        ));
    }

    // Болотные деревья
    let tree_positions = [
        (-16.5, 4.5, 0.0, 1.0),
        (-25.5, 19.0, 1.2, 1.1),
        (24.5, 18.5, 1.9, 1.0),
        (28.5, 3.5, 3.1, 1.15),
        (26.0, -8.5, 0.5, 0.95),
        (-16.5, -14.5, 2.1, 1.05),
        (15.5, -14.0, 1.4, 1.0),
        (-34.5, 35.0, 0.8, 1.1),
        (34.5, 35.0, 2.4, 1.1),
        (-23.5, 55.0, 1.5, 1.05),
        (23.5, 55.0, 0.4, 1.05),
    ];
    for (tx, tz, tyaw, tscale) in tree_positions {
        let col = nav_world.add_box(tx, tz, 0.75 * tscale, 0.75 * tscale);
        commands.spawn((
            WorldAssetRoot(tree_scene.clone()),
            Transform::from_xyz(tx, 0.0, tz)
                .with_rotation(Quat::from_rotation_y(tyaw))
                .with_scale(Vec3::splat(tscale)),
            Destructible::tree(vec![col]),
        ));
    }

    // Болотные кусты
    let bush_positions = [
        (-6.2, 12.2, 0.3),
        (6.2, 12.2, 1.7),
        (-6.5, 19.5, 2.2),
        (6.5, 19.5, 0.9),
        (14.5, 4.2, 0.4),
        (26.5, -3.5, 1.5),
        (-8.5, 30.8, 1.1),
        (8.5, 30.8, 2.5),
        (-16.5, 39.5, 0.7),
        (16.5, 39.5, 1.9),
    ];
    for (bx, bz, byaw) in bush_positions {
        let col = nav_world.add_box(bx, bz, 0.82, 0.82);
        commands.spawn((
            WorldAssetRoot(bush_scene.clone()),
            Transform::from_xyz(bx, 0.0, bz).with_rotation(Quat::from_rotation_y(byaw)),
            Destructible::bush(vec![col]),
        ));
    }

    integrity_dirty.0 = true;

    // 3. Игрок: Загружаем 3D-модель Сыщика
    let detective_scene =
        asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/detective.glb"));

    commands
        .spawn((
            Player {
                speed: 9.0,
                current_floor: 0,
                path: VecDeque::new(),
                keyboard_vel: Vec3::ZERO,
                is_moving: false,
            },
            Transform::from_xyz(0.0, 0.08, 4.0)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            Visibility::default(),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    WorldAssetRoot(detective_scene),
                    Transform::default(),
                ))
                .observe(on_character_scene_ready);

            parent.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.85, 0.45),
                    intensity: 80_000.0,
                    range: 14.0,
                    shadow_maps_enabled: true,
                    ..default()
                },
                Transform::from_xyz(0.35, 1.0, 0.35),
            ));
        });

    // Направленный холодный лунный свет
    commands.spawn((
        DirectionalLight {
            illuminance: 3_000.0,
            shadow_maps_enabled: true,
            color: Color::srgb(0.82, 0.88, 1.0),
            ..default()
        },
        Transform::from_xyz(25.0, 40.0, 25.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Изометрическая камера
    commands.spawn((
        Camera3d::default(),
        CameraController {
            yaw: 0.0,
            pitch: 42.0_f32.to_radians(),
            distance: 24.0,
            current_focus: None,
        },
        Transform::from_xyz(0.0, 18.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Информационная плашка в левом верхнем углу экрана
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(14.0),
            left: Val::Px(16.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            ..default()
        })
        .insert(BackgroundColor(Color::srgba(0.06, 0.08, 0.10, 0.85)))
        .with_children(|parent| {
            parent.spawn((
                BuildingHudText,
                Text::new("Замок Исследователя — Этаж 1/2 | [Tab] Обзор другого этажа"),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(0.86, 0.82, 0.68)),
            ));
            parent.spawn((
                InventoryHudText,
                Text::new(
                    "ПКМ по объекту или земле — меню приказов (Разрушить / Копать / Строить)",
                ),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgb(0.58, 0.90, 0.72)),
            ));
        });
}

/// Строит двухэтажный Замок Исследователя с отрезаемыми стенами, лестницей на 2-й этаж и крышей
fn spawn_investigator_castle(
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

/// Переиспользуемый билдер входимого двухэтажного городского дома (Особняк или Гостиница)
/// Снаружи отображается оригинальная 3D-модель здания, а при входе через парадную дверь
/// открывается двухэтажный интерьер с отрезаемыми стенами и лестницей на 2-й этаж!
#[allow(clippy::too_many_arguments)]
fn spawn_enterable_town_house(
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
