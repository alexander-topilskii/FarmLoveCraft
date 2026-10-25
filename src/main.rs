mod building;
mod camera;
mod destruction;
mod navigation;
mod player;
mod rts_command;
mod terrain;
mod world;

use bevy::prelude::*;
use building::{
    update_active_building_system, update_building_floor_visibility_system,
    update_building_hud_system, update_wall_cutaway_system, ActiveBuildingState, BuildingRegistry,
    CutawayMaterialCache,
};
use camera::camera_controller_system;
use destruction::{
    recompute_structural_integrity_system, update_destruction_effects_system, PlayerInventory,
    StructuralIntegrityDirty,
};
use navigation::NavWorld;
use player::{player_input_system, player_movement_system};
use rts_command::{
    execute_rts_order_system, rts_menu_interaction_system, rts_right_click_menu_system,
    update_inventory_hud_system, ActiveRtsOrder, RightClickTracker,
};
use terrain::{rebuild_dirty_chunks_system, stream_terrain_chunks_system, WorldTerrain};
use world::setup_world;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "FarmLoveCraft — Разрушаемый мир (Valheim + RTS)".into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(NavWorld::default())
        .insert_resource(WorldTerrain::default())
        .insert_resource(PlayerInventory::default())
        .insert_resource(StructuralIntegrityDirty::default())
        .insert_resource(ActiveRtsOrder::default())
        .insert_resource(RightClickTracker::default())
        .insert_resource(BuildingRegistry::default())
        .insert_resource(ActiveBuildingState::default())
        .insert_resource(CutawayMaterialCache::default())
        .add_systems(Startup, setup_world)
        .add_systems(
            Update,
            (
                rts_right_click_menu_system,
                rts_menu_interaction_system,
                player_input_system,
                execute_rts_order_system,
                player_movement_system,
                stream_terrain_chunks_system,
                rebuild_dirty_chunks_system,
                recompute_structural_integrity_system,
                update_destruction_effects_system,
                update_active_building_system,
                update_building_floor_visibility_system,
                update_wall_cutaway_system,
                update_building_hud_system,
                update_inventory_hud_system,
                camera_controller_system,
            )
                .chain(),
        )
        .run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecs_systems_have_no_query_conflicts() {
        let mut schedule = Schedule::new(Update);
        schedule.add_systems(
            (
                rts_right_click_menu_system,
                rts_menu_interaction_system,
                player_input_system,
                execute_rts_order_system,
                player_movement_system,
                stream_terrain_chunks_system,
                rebuild_dirty_chunks_system,
                recompute_structural_integrity_system,
                update_destruction_effects_system,
                update_active_building_system,
                update_building_floor_visibility_system,
                update_wall_cutaway_system,
                update_building_hud_system,
                update_inventory_hud_system,
                camera_controller_system,
            )
                .chain(),
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
}
