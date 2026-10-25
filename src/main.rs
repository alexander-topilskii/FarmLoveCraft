mod building;
mod camera;
mod destruction;
mod navigation;
mod player;
mod rts_command;
mod terrain;
mod water_cellular;
mod world;

use bevy::asset::AssetId;
use bevy::prelude::*;
use bevy::text::Font;
use building::{
    update_active_building_system, update_building_floor_visibility_system,
    update_building_hud_system, update_wall_cutaway_system, ActiveBuildingState, BuildingRegistry,
    CutawayMaterialCache,
};
use camera::{camera_controller_system, cull_distant_lights_system};
use destruction::{
    recompute_structural_integrity_system, update_destruction_effects_system, update_farm_crops_system,
    PlayerInventory, StructuralIntegrityDirty,
};
use navigation::NavWorld;
use player::{player_input_system, player_movement_system};
use rts_command::{
    execute_rts_order_system, rts_menu_interaction_system, rts_right_click_menu_system,
    update_inventory_hud_system, ActiveRtsOrder, RightClickTracker,
};
use terrain::{rebuild_dirty_chunks_system, stream_terrain_chunks_system, WorldTerrain};
use water_cellular::{
    sync_water_mesh_system, water_flow_physics_system, water_simulation_system,
    WaterSimulationTimer, WaterWorld,
};
use world::setup_world;

/// Наборы систем для многопоточного выполнения в кадре (OPT-09)
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameSystemSet {
    Input,
    Simulation,
    WorldSync,
    Presentation,
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "FarmLoveCraft — Разрушаемый мир (Valheim + RTS)".into(),
                ..default()
            }),
            ..default()
        }));

    // Заменяем встроенный в Bevy дефолтный шрифт (FiraMono-subset только с ASCII символами)
    // на полный шрифт FiraMono-Medium с поддержкой кириллицы (русских букв).
    {
        let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
        let cyrillic_font =
            Font::from_bytes(include_bytes!("../assets/fonts/FiraMono-Medium.ttf").to_vec());
        fonts
            .insert(AssetId::default(), cyrillic_font)
            .expect("Не удалось инициализировать шрифт с поддержкой кириллицы");
    }

    app.insert_resource(NavWorld::default())
        .insert_resource(WorldTerrain::default())
        .insert_resource(PlayerInventory::default())
        .insert_resource(StructuralIntegrityDirty::default())
        .insert_resource(ActiveRtsOrder::default())
        .insert_resource(RightClickTracker::default())
        .insert_resource(BuildingRegistry::default())
        .insert_resource(ActiveBuildingState::default())
        .insert_resource(CutawayMaterialCache::default())
        .insert_resource(WaterWorld::default())
        .insert_resource(WaterSimulationTimer::default())
        .add_systems(Startup, setup_world)
        .configure_sets(
            Update,
            (
                GameSystemSet::Input,
                GameSystemSet::Simulation,
                GameSystemSet::WorldSync,
                GameSystemSet::Presentation,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                rts_right_click_menu_system,
                rts_menu_interaction_system,
                player_input_system,
            )
                .in_set(GameSystemSet::Input),
        )
        .add_systems(
            Update,
            (
                execute_rts_order_system,
                player_movement_system,
                stream_terrain_chunks_system,
                recompute_structural_integrity_system,
                update_farm_crops_system,
                water_simulation_system,
                water_flow_physics_system,
            )
                .in_set(GameSystemSet::Simulation),
        )
        .add_systems(
            Update,
            (
                rebuild_dirty_chunks_system,
                update_destruction_effects_system,
                update_active_building_system,
                update_building_floor_visibility_system,
                update_wall_cutaway_system,
                sync_water_mesh_system,
            )
                .in_set(GameSystemSet::WorldSync),
        )
        .add_systems(
            Update,
            (
                camera_controller_system,
                update_building_hud_system,
                update_inventory_hud_system,
                cull_distant_lights_system,
            )
                .in_set(GameSystemSet::Presentation),
        )
        .run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecs_systems_have_no_query_conflicts() {
        let mut schedule = Schedule::new(Update);
        schedule.configure_sets(
            (
                GameSystemSet::Input,
                GameSystemSet::Simulation,
                GameSystemSet::WorldSync,
                GameSystemSet::Presentation,
            )
                .chain(),
        );
        schedule.add_systems(
            (
                rts_right_click_menu_system,
                rts_menu_interaction_system,
                player_input_system,
            )
                .in_set(GameSystemSet::Input),
        );
        schedule.add_systems(
            (
                execute_rts_order_system,
                player_movement_system,
                stream_terrain_chunks_system,
                recompute_structural_integrity_system,
                update_farm_crops_system,
                water_simulation_system,
                water_flow_physics_system,
            )
                .in_set(GameSystemSet::Simulation),
        );
        schedule.add_systems(
            (
                rebuild_dirty_chunks_system,
                update_destruction_effects_system,
                update_active_building_system,
                update_building_floor_visibility_system,
                update_wall_cutaway_system,
                sync_water_mesh_system,
            )
                .in_set(GameSystemSet::WorldSync),
        );
        schedule.add_systems(
            (
                camera_controller_system,
                update_building_hud_system,
                update_inventory_hud_system,
                cull_distant_lights_system,
            )
                .in_set(GameSystemSet::Presentation),
        );
        let mut world = World::new();
        schedule
            .initialize(&mut world)
            .expect("Все ECS-системы должны инициализироваться без конфликтов запросов (B0001)");

        let mut startup_schedule = Schedule::new(Startup);
        startup_schedule.add_systems(setup_world);
        startup_schedule
            .initialize(&mut world)
            .expect("Startup-система должна инициализироваться без конфликтов");
    }

    #[test]
    fn test_cyrillic_font_embed_and_default_asset() {
        let font_bytes = include_bytes!("../assets/fonts/FiraMono-Medium.ttf");
        assert!(
            font_bytes.len() > 100_000,
            "Шрифт должен быть полным (включая кириллицу), а не урезанным subset"
        );
        let font = Font::from_bytes(font_bytes.to_vec());
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<Assets<Font>>();
        app.world_mut()
            .resource_mut::<Assets<Font>>()
            .insert(AssetId::default(), font)
            .expect("Дефолтный шрифт должен успешно заменяться на шрифт с кириллицей");

        let fonts = app.world().resource::<Assets<Font>>();
        let default_font = fonts
            .get(AssetId::default())
            .expect("Дефолтный шрифт должен существовать");
        assert_eq!(default_font.data.len(), font_bytes.len());
    }
}
