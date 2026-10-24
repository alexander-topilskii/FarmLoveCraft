mod building;
mod camera;
mod navigation;
mod player;
mod world;

use bevy::prelude::*;
use building::{
    update_active_building_system, update_building_floor_visibility_system,
    update_building_hud_system, update_wall_cutaway_system, ActiveBuildingState, BuildingRegistry,
};
use camera::camera_controller_system;
use navigation::NavWorld;
use player::{player_input_system, player_movement_system};
use world::setup_world;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "FarmLoveCraft - Prototype".into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(NavWorld::default())
        .insert_resource(BuildingRegistry::default())
        .insert_resource(ActiveBuildingState::default())
        .add_systems(Startup, setup_world)
        .add_systems(
            Update,
            (
                player_input_system,
                player_movement_system,
                update_active_building_system,
                update_building_floor_visibility_system,
                update_wall_cutaway_system,
                update_building_hud_system,
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
                player_input_system,
                player_movement_system,
                update_active_building_system,
                update_building_floor_visibility_system,
                update_wall_cutaway_system,
                update_building_hud_system,
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

