use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use crate::building::{ActiveBuildingState, BuildingRegistry};
use crate::player::Player;

#[derive(Component)]
pub struct CameraController {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub current_focus: Option<Vec3>,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 42.0_f32.to_radians(),
            distance: 24.0,
            current_focus: None,
        }
    }
}

pub fn camera_controller_system(
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
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

    // Вращение камеры при зажатии СКМ, перетаскивании ПКМ или клавишами Q / E
    if mouse_button.pressed(MouseButton::Middle)
        || (mouse_button.pressed(MouseButton::Right) && mouse_motion.delta.length_squared() > 1.5)
    {
        let delta = mouse_motion.delta;
        let sensitivity = 0.005;

        controller.yaw -= delta.x * sensitivity;
        controller.pitch += delta.y * sensitivity;
        controller.pitch = controller.pitch.clamp(20.0_f32.to_radians(), 75.0_f32.to_radians());
    }

    if keyboard_input.pressed(KeyCode::KeyQ) {
        controller.yaw += 1.6 * time.delta_secs();
    }
    if keyboard_input.pressed(KeyCode::KeyE) {
        controller.yaw -= 1.6 * time.delta_secs();
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

    let dt = time.delta_secs().clamp(0.0, 0.1);
    let focus = match controller.current_focus {
        Some(prev) => prev.lerp(target, (10.0 * dt).min(1.0)),
        None => target,
    };
    controller.current_focus = Some(focus);

    let cos_pitch = controller.pitch.cos();
    let sin_pitch = controller.pitch.sin();

    let offset = Vec3::new(
        controller.yaw.sin() * cos_pitch * controller.distance,
        sin_pitch * controller.distance,
        controller.yaw.cos() * cos_pitch * controller.distance,
    );

    cam_transform.translation = focus + offset;
    cam_transform.look_at(focus + Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_focus_smooth_following_and_stable_orientation() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<AccumulatedMouseMotion>();
        app.init_resource::<AccumulatedMouseScroll>();
        app.init_resource::<ActiveBuildingState>();
        app.init_resource::<BuildingRegistry>();

        let player = app
            .world_mut()
            .spawn((
                Player {
                    speed: 9.0,
                    current_floor: 0,
                    path: std::collections::VecDeque::new(),
                    keyboard_vel: Vec3::ZERO,
                    is_moving: true,
                },
                Transform::from_xyz(0.0, 0.0, 0.0),
            ))
            .id();

        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                CameraController::default(),
                Transform::default(),
            ))
            .id();

        app.add_systems(Update, camera_controller_system);

        // Первый кадр
        app.update();

        let cam_tf_1 = *app.world().get::<Transform>(camera).unwrap();
        let controller_1 = app.world().get::<CameraController>(camera).unwrap();
        assert!(controller_1.current_focus.is_some());

        // Персонаж перемещается на скорости
        app.world_mut()
            .get_mut::<Transform>(player)
            .unwrap()
            .translation = Vec3::new(5.0, 0.0, 5.0);
        app.update();

        let cam_tf_2 = *app.world().get::<Transform>(camera).unwrap();
        // Вектор поворота камеры должен оставаться строго выровненным (без рывков rotation)
        let forward_1 = cam_tf_1.forward();
        let forward_2 = cam_tf_2.forward();
        assert!(
            (forward_1.dot(*forward_2) - 1.0).abs() < 1e-4,
            "Направление обзора камеры должно быть стабильным без угловых рывков"
        );
    }
}
