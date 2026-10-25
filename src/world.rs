use std::collections::VecDeque;
use bevy::prelude::*;
use crate::building::{BuildingHudText, BuildingId, BuildingRegistry};
use crate::camera::CameraController;
use crate::city_generator::ProceduralCityAssets;
use crate::destruction::{
    Destructible, InfectedFlora, ResourceModelAssets, ResourceType, SharedVfxAssets,
    StructuralIntegrityDirty,
};
use crate::navigation::NavWorld;
use crate::player::{on_character_scene_ready, CharacterAnimations, Player};
use crate::rts_command::{BuildableKitAssets, InventoryHudText};
use crate::terrain::{ChunkCoord, ProceduralBiomeAssets, TerrainChunkEntity, WorldTerrain};

pub mod dungeons;

/// Маркер фонаря Сыщика, чтобы дистанционный куллинг не отключал его свет (OPT-08)
#[derive(Component)]
pub struct PlayerLanternLight;

/// Набор переиспользуемых ассетов для сборки многоэтажных интерьеров
#[derive(Clone)]
pub struct InteriorKitAssets {
    pub floor: Handle<WorldAsset>,
    pub wall: Handle<WorldAsset>,
    pub pillar: Handle<WorldAsset>,
    pub arch: Handle<WorldAsset>,
    pub table: Handle<WorldAsset>,
    pub chair: Handle<WorldAsset>,
    pub bookcase: Handle<WorldAsset>,
    pub rug: Handle<WorldAsset>,
    pub candelabra: Handle<WorldAsset>,
    pub altar: Handle<WorldAsset>,
    pub fireplace: Handle<WorldAsset>,
    pub alchemy: Handle<WorldAsset>,
    pub clock: Handle<WorldAsset>,
    pub hand_axe: Handle<WorldAsset>,
    pub occult_book: Handle<WorldAsset>,
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
    mut water_world: ResMut<crate::water_cellular::WaterWorld>,
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

    let shared_vfx = SharedVfxAssets::new(&mut meshes, &mut materials);
    commands.insert_resource(shared_vfx);

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
    // 1. ДВУХЭТАЖНЫЙ ЗАМОК ИССЛЕДОВАТЕЛЯ И ДРЕВНИЕ КАТАКОМБЫ (Фазы 1 и 3)
    // =========================================================================
    dungeons::castle_hub::spawn_investigator_castle(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut nav_world,
        &mut building_registry,
        &kit,
    );
    dungeons::catacombs::spawn_catacombs(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut nav_world,
        &mut building_registry,
        &mut water_world,
        &kit,
    );

    // =========================================================================
    // 2. УЛИЦА / ПОМЕСТЬЕ, КАНАЛ ЩУПАЛЕЦ И ГОРОД АРКХЭМ / ИННСМУТ
    // =========================================================================
    let rot_90 = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let bench_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_bench.glb"));
    let lamppost_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_lamppost.glb"));
    let tree_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_tree.glb"));
    let tree_gnarled_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_tree_gnarled.glb"));
    let tree_snag_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_tree_snag.glb"));
    let tree_log_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_tree_log.glb"));
    let tree_willow_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_tree_willow.glb"));
    let tree_pine_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_tree_pine.glb"));
    let tree_corrupted_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_tree_corrupted.glb"));

    let all_trees = vec![
        tree_scene.clone(),
        tree_gnarled_scene.clone(),
        tree_snag_scene.clone(),
        tree_log_scene.clone(),
        tree_willow_scene.clone(),
        tree_pine_scene.clone(),
        tree_corrupted_scene.clone(),
    ];
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

    let res_stone_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/resources/resource_stone.glb"));
    let res_wood_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/resources/resource_wood.glb"));
    let res_foliage_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/resources/resource_foliage.glb"));
    let res_spores_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/resources/resource_spores.glb"));

    commands.insert_resource(ResourceModelAssets {
        stone: res_stone_scene,
        wood: res_wood_scene,
        foliage: res_foliage_scene,
        spores: res_spores_scene,
    });

    commands.insert_resource(ProceduralBiomeAssets {
        trees: all_trees,
        tree: tree_scene.clone(),
        bush: bush_scene.clone(),
        gravestone: gravestone_scene.clone(),
        gazebo: gazebo_scene.clone(),
        well: well_scene.clone(),
        crypt: crypt_scene.clone(),
    });

    commands.insert_resource(ProceduralCityAssets {
        townhouse: townhouse_scene.clone(),
        shop: shop_scene.clone(),
        path: path_scene.clone(),
        fence: fence_scene.clone(),
        gate: gate_scene.clone(),
        lamppost: lamppost_scene.clone(),
        bench: bench_scene.clone(),
        kiosk: kiosk_scene.clone(),
        phonebooth: phonebooth_scene.clone(),
        cart: cart_scene.clone(),
        well: well_scene.clone(),

        witch_cottage: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_witchcottage.glb")),
        witch_tower: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_witchtower.glb")),
        canopy_shelter: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_canopyshelter.glb")),
        doghouse: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_doghouse.glb")),
        chimney: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_chimney.glb")),
        cauldron_large: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_cauldron_large.glb")),
        cauldron_small: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_cauldron_small.glb")),
        firewood_large: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_firewood_large.glb")),
        firewood_small: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_firewood_small.glb")),
        pumpkin_jack: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_pumpkin_jack.glb")),
        pumpkin_plain: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_pumpkin_plain.glb")),
        lantern_green: asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/haunted/haunted_lantern_green.glb")),
    });

    commands.insert_resource(BuildableKitAssets {
        wall: kit.wall.clone(),
        arch: kit.arch.clone(),
        pillar: kit.pillar.clone(),
        floor: kit.floor.clone(),
        candelabra: kit.candelabra.clone(),
        bush_tentacle: bush_scene.clone(),
        well: well_scene.clone(),
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

    // Проклятый колодец со щупальцами на восточном дворе (источник воды Minecraft-style)
    let well_col = nav_world.add_box(22.5, 0.0, 1.35, 1.35);
    water_world.add_source(Vec2::new(22.5, 0.0), &terrain);
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
    water_world.add_source(Vec2::new(-26.5, 23.5), &terrain);
    water_world.add_source(Vec2::new(34.5, 23.5), &terrain);

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
    dungeons::town_innsmouth::spawn_enterable_town_house(
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
    dungeons::town_innsmouth::spawn_enterable_town_house(
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
    dungeons::town_innsmouth::spawn_enterable_town_house(
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
    dungeons::town_innsmouth::spawn_enterable_town_house(
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
            crate::atmosphere::FlickeringLight::lantern(65_000.0),
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

    // Болотные деревья и коряги (различные варианты по референсам стиля)
    let tree_placements: [(Handle<WorldAsset>, f32, f32, f32, f32, bool); 11] = [
        (tree_gnarled_scene.clone(), -16.5, 4.5, 0.0, 1.0, false),
        (tree_willow_scene.clone(), -25.5, 19.0, 1.2, 1.05, false),
        (tree_pine_scene.clone(), 24.5, 18.5, 1.9, 1.0, false),
        (tree_snag_scene.clone(), 28.5, 3.5, 3.1, 1.0, false),
        (tree_log_scene.clone(), 26.0, -8.5, 0.5, 1.0, false),
        (tree_scene.clone(), -16.5, -14.5, 2.1, 1.05, false),
        (tree_corrupted_scene.clone(), 15.5, -14.0, 1.4, 1.0, true), // Искаженное древо со спорами!
        (tree_gnarled_scene.clone(), -34.5, 35.0, 0.8, 1.1, false),
        (tree_pine_scene.clone(), 34.5, 35.0, 2.4, 1.05, false),
        (tree_willow_scene.clone(), -23.5, 55.0, 1.5, 1.05, true), // Болотная ива со спорами!
        (tree_snag_scene.clone(), 23.5, 55.0, 0.4, 1.05, true),   // Расколотый пень со спорами!
    ];
    for (handle, tx, tz, tyaw, tscale, infected) in tree_placements {
        let col = nav_world.add_box(tx, tz, 0.75 * tscale, 0.75 * tscale);
        commands.spawn((
            WorldAssetRoot(handle),
            Transform::from_xyz(tx, 0.0, tz)
                .with_rotation(Quat::from_rotation_y(tyaw))
                .with_scale(Vec3::splat(tscale)),
            Destructible::tree(infected, vec![col]),
            InfectedFlora(infected),
        ));
    }

    // Болотные кусты
    let bush_positions: [(f32, f32, f32, bool); 10] = [
        (-6.2, 12.2, 0.3, false),
        (6.2, 12.2, 1.7, false),
        (-6.5, 19.5, 2.2, true),  // У канала щупалец — заражен спорами!
        (6.5, 19.5, 0.9, true),   // У канала щупалец — заражен спорами!
        (14.5, 4.2, 0.4, false),
        (26.5, -3.5, 1.5, true),  // Болотная окраина — заражен спорами!
        (-8.5, 30.8, 1.1, false),
        (8.5, 30.8, 2.5, false),
        (-16.5, 39.5, 0.7, false),
        (16.5, 39.5, 1.9, false),
    ];
    for (bx, bz, byaw, infected) in bush_positions {
        let col = nav_world.add_box(bx, bz, 0.82, 0.82);
        commands.spawn((
            WorldAssetRoot(bush_scene.clone()),
            Transform::from_xyz(bx, 0.0, bz).with_rotation(Quat::from_rotation_y(byaw)),
            Destructible::bush(infected, vec![col]),
            InfectedFlora(infected),
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
                PlayerLanternLight,
                crate::atmosphere::FlickeringLight::lantern(80_000.0),
            ));
        });

    // Направленное небесное светило (Солнце / Луна) с объемными лучами Valheim (God Rays) и 4 каскадами теней
    commands.spawn((
        DirectionalLight {
            illuminance: 6_500.0,
            shadow_maps_enabled: true,
            color: Color::srgb(1.0, 0.55, 0.28),
            ..default()
        },
        Transform::from_xyz(25.0, 40.0, 25.0).looking_at(Vec3::ZERO, Vec3::Y),
        bevy::light::VolumetricLight,
        bevy::light::CascadeShadowConfigBuilder {
            num_cascades: 4,
            minimum_distance: 0.1,
            maximum_distance: 85.0,
            first_cascade_far_bound: 8.0,
            overlap_proportion: 0.2,
        }
        .build(),
        crate::atmosphere::CelestialSunLight,
    ));

    // Локальный объем тумана для объемного рассеивания лучей (God Rays) вокруг фокуса камеры
    commands.spawn((
        bevy::light::FogVolume {
            fog_color: Color::srgb(0.20, 0.18, 0.24),
            density_factor: 0.045,
            scattering_asymmetry: 0.65,
            absorption: 0.25,
            scattering: 0.35,
            ..default()
        },
        Transform::from_scale(Vec3::splat(70.0)),
        crate::atmosphere::AtmosphericFogVolume,
    ));

    // Глобальный рассеянный свет окружения (динамически управляется суточным циклом)
    commands.insert_resource(bevy::light::GlobalAmbientLight {
        color: Color::srgb(0.40, 0.48, 0.60),
        brightness: 75.0,
        ..default()
    });

    // Изометрическая кинематографическая камера (HDR + Bloom + SSAO + Tonemapping + DoF + AtmosphericFog)
    commands.spawn(crate::atmosphere::setup_cinematic_camera_bundle(
        CameraController {
            yaw: 0.0,
            pitch: 42.0_f32.to_radians(),
            distance: 24.0,
            current_focus: None,
        },
        Transform::from_xyz(0.0, 18.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Информационная плашка в левом верхнем углу экрана (скрыта по умолчанию, открывается/закрывается по Tab)
    commands
        .spawn((
            crate::game_menu::TopHudRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(44.0),
                left: Val::Px(16.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.06, 0.08, 0.10, 0.88)),
            BorderColor::all(Color::srgb(0.24, 0.36, 0.28)),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                BuildingHudText,
                Text::new("Замок Исследователя — Этаж 1/2 | [PgUp/PgDn] Обзор этажа"),
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
            parent.spawn((
                crate::defense::SupportSkillbarText,
                Text::new(
                    "Саппорт-Инженер: [1] Живительный Ихор | [2] Починка | [3] Ярость Бездны | [4] Вспышка Фонаря | [E] Ритуал Катакомб",
                ),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.85, 0.45)),
            ));
            parent.spawn((
                crate::atmosphere::AtmosphereHudText,
                Text::new("Время: 17:30 (Кровавый Закат) | Биом: Поместье Аркхэма"),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(0.72, 0.85, 0.98)),
            ));
        });
}

