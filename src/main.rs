use bevy::prelude::*;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "FarmLoveCraft - Prototype".into(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                player_input_system,
                player_movement_system,
                camera_controller_system,
            ),
        )
        .run();
}

#[derive(Component)]
struct Player {
    speed: f32,
    target_pos: Option<Vec3>,
}

#[derive(Component)]
struct CameraController {
    /// Угол поворота вокруг персонажа по горизонтали (yaw / азимут)
    yaw: f32,
    /// Угол наклона по вертикали (pitch)
    pitch: f32,
    /// Расстояние от цели до камеры (zoom)
    distance: f32,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Земля фермы с разметкой сетки/точек
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(80.0, 80.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.2, 0.14),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Несколько декоративных ориентиров/камней на поле, чтобы чувствовалось вращение и перемещение
    let pillar_mesh = meshes.add(Cuboid::new(0.6, 2.0, 0.6));
    let pillar_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.28, 0.3),
        perceptual_roughness: 0.7,
        ..default()
    });

    for (x, z) in [(-6.0, -6.0), (8.0, -5.0), (-7.0, 9.0), (10.0, 8.0)] {
        commands.spawn((
            Mesh3d(pillar_mesh.clone()),
            MeshMaterial3d(pillar_mat.clone()),
            Transform::from_xyz(x, 1.0, z),
        ));
    }

    // Игрок: овальная капсула
    commands.spawn((
        Player {
            speed: 9.0,
            target_pos: None,
        },
        Mesh3d(meshes.add(Capsule3d::new(0.45, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.75, 0.22, 0.32),
            metallic: 0.1,
            perceptual_roughness: 0.4,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.95, 0.0),
    ));

    // Направленный лунный свет
    commands.spawn((
        DirectionalLight {
            illuminance: 3_500.0,
            shadow_maps_enabled: true,
            color: Color::srgb(0.85, 0.9, 1.0),
            ..default()
        },
        Transform::from_xyz(20.0, 30.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Изометрическая камера (Lineage 2 style: yaw, pitch, zoom)
    commands.spawn((
        Camera3d::default(),
        CameraController {
            yaw: 0.0,
            pitch: 42.0_f32.to_radians(), // удобный изометрический угол наклона
            distance: 22.0,
        },
        Transform::from_xyz(0.0, 15.0, 17.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn player_input_system(
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform, &CameraController)>,
    mut player_query: Query<(&mut Player, &Transform)>,
) {
    let Ok((mut player, player_transform)) = player_query.single_mut() else {
        return;
    };
    let Ok((camera, camera_transform, cam_controller)) = camera_query.single() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    // 1. Управление клавиатурой (WASD / стрелки) с учетом поворота камеры
    let mut keyboard_dir = Vec3::ZERO;
    if keyboard_input.pressed(KeyCode::KeyW) || keyboard_input.pressed(KeyCode::ArrowUp) {
        keyboard_dir.z -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyS) || keyboard_input.pressed(KeyCode::ArrowDown) {
        keyboard_dir.z += 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyA) || keyboard_input.pressed(KeyCode::ArrowLeft) {
        keyboard_dir.x -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyD) || keyboard_input.pressed(KeyCode::ArrowRight) {
        keyboard_dir.x += 1.0;
    }

    if keyboard_dir.length_squared() > 0.0 {
        // Направление движения трансформируется относительно угла поворота камеры (yaw)
        let yaw_rot = Quat::from_rotation_y(cam_controller.yaw);
        let move_dir = yaw_rot * keyboard_dir.normalize();

        // Клавиатура переопределяет цель мыши и сразу смещает цель вперед
        player.target_pos = Some(player_transform.translation + move_dir * 3.0);
        return;
    }

    // 2. Клик левой кнопкой мыши (или зажатие) для перемещения к точке на земле (L2 style)
    if mouse_button.pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                // Пересечение луча с плоскостью земли (Y = 0)
                // ray.origin + t * ray.direction = (x, 0, z) => t = -origin.y / direction.y
                if ray.direction.y.abs() > f32::EPSILON {
                    let t = (0.95 - ray.origin.y) / ray.direction.y;
                    if t > 0.0 {
                        let ground_hit = ray.origin + *ray.direction * t;
                        player.target_pos = Some(Vec3::new(ground_hit.x, 0.95, ground_hit.z));
                    }
                }
            }
        }
    }
}

fn player_movement_system(
    time: Res<Time>,
    mut query: Query<(&mut Player, &mut Transform)>,
) {
    for (mut player, mut transform) in &mut query {
        if let Some(target) = player.target_pos {
            let to_target = target - transform.translation;
            let flat_to_target = Vec3::new(to_target.x, 0.0, to_target.z);
            let distance = flat_to_target.length();

            let stop_tolerance = 0.15;
            if distance > stop_tolerance {
                let direction = flat_to_target.normalize();
                let step = player.speed * time.delta_secs();
                let move_amount = step.min(distance);

                transform.translation += direction * move_amount;

                // Плавный поворот лицом к цели движения
                let target_rotation = Quat::from_rotation_y(direction.x.atan2(direction.z));
                transform.rotation = transform.rotation.slerp(target_rotation, 15.0 * time.delta_secs());
            } else {
                player.target_pos = None;
            }
        }
    }
}

fn camera_controller_system(
    mouse_button: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
    mut camera_query: Query<(&mut CameraController, &mut Transform), With<Camera3d>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok((mut controller, mut cam_transform)) = camera_query.single_mut() else {
        return;
    };

    // 1. Вращение камеры перетаскиванием правой кнопкой мыши (RMB Drag в стиле Lineage 2)
    if mouse_button.pressed(MouseButton::Right) {
        let delta = mouse_motion.delta;
        let sensitivity = 0.005;

        // Поворот по горизонтали (yaw)
        controller.yaw -= delta.x * sensitivity;

        // Наклон по вертикали (pitch) с ограничением угла
        controller.pitch += delta.y * sensitivity;
        controller.pitch = controller.pitch.clamp(20.0_f32.to_radians(), 75.0_f32.to_radians());
    }

    // 2. Зум колесиком мыши
    if mouse_scroll.delta.y.abs() > f32::EPSILON {
        controller.distance -= mouse_scroll.delta.y * 1.5;
        controller.distance = controller.distance.clamp(8.0, 45.0);
    }

    // 3. Вычисление сферических координат камеры относительно позиции игрока
    let target = player_transform.translation;
    let cos_pitch = controller.pitch.cos();
    let sin_pitch = controller.pitch.sin();

    let offset = Vec3::new(
        controller.yaw.sin() * cos_pitch * controller.distance,
        sin_pitch * controller.distance,
        controller.yaw.cos() * cos_pitch * controller.distance,
    );

    let desired_cam_pos = target + offset;

    // Плавное следование камеры
    cam_transform.translation = cam_transform
        .translation
        .lerp(desired_cam_pos, 10.0 * time.delta_secs());
    cam_transform.look_at(target + Vec3::new(0.0, 0.5, 0.0), Vec3::Y);
}
