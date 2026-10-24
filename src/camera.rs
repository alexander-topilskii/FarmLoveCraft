use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use crate::building::{ActiveBuildingState, BuildingRegistry};
use crate::player::Player;

#[derive(Component)]
pub struct CameraController {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

pub fn camera_controller_system(
    mouse_button: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    building_state: Res<ActiveBuildingState>,
    building_registry: Res<BuildingRegistry>,
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
    mut camera_query: Query<(&mut CameraController, &mut Transform), With<Camera3d>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok((mut controller, mut cam_transform)) = camera_query.single_mut() else {
        return;
    };

    if mouse_button.pressed(MouseButton::Right) {
        let delta = mouse_motion.delta;
        let sensitivity = 0.005;

        controller.yaw -= delta.x * sensitivity;
        controller.pitch += delta.y * sensitivity;
        controller.pitch = controller.pitch.clamp(20.0_f32.to_radians(), 75.0_f32.to_radians());
    }

    if mouse_scroll.delta.y.abs() > f32::EPSILON {
        controller.distance -= mouse_scroll.delta.y * 1.5;
        controller.distance = controller.distance.clamp(8.0, 65.0);
    }

    let mut target = player_transform.translation;
    // Если игрок переключил обзор на другой этаж здания (Tab), плавно поднимаем/опускаем фокус камеры на этот этаж
    if let Some(b_id) = building_state.active_building {
        if building_state.viewed_floor != building_state.player_floor {
            if let Some(b_def) = building_registry.get(b_id) {
                target.y = b_def.floor_elevation(building_state.viewed_floor);
            }
        }
    }

    let cos_pitch = controller.pitch.cos();
    let sin_pitch = controller.pitch.sin();

    let offset = Vec3::new(
        controller.yaw.sin() * cos_pitch * controller.distance,
        sin_pitch * controller.distance,
        controller.yaw.cos() * cos_pitch * controller.distance,
    );

    let desired_cam_pos = target + offset;

    cam_transform.translation = cam_transform
        .translation
        .lerp(desired_cam_pos, 10.0 * time.delta_secs());
    cam_transform.look_at(target + Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
}
