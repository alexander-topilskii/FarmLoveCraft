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
    walk_timer: f32,
}

#[derive(Component)]
struct CameraController {
    yaw: f32,
    pitch: f32,
    distance: f32,
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Земля фермы (мрачно-зеленая лавкрафтовская почва)
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(80.0, 80.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.2, 0.14),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Загрузка модульных PS1-объектов Лавкрафтовского Замка из Blender
    let floor_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_floor.glb"));
    let wall_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_wall.glb"));
    let pillar_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_pillar.glb"));
    let arch_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_arch.glb"));
    let table_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_table.glb"));
    let chair_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_chair.glb"));
    let bookcase_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_bookcase.glb"));
    let rug_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_rug.glb"));
    let candelabra_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_candelabra.glb"));
    let altar_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_altar.glb"));

    // 1. Каменный пол замкового зала (сетка 3x3 тайла по 4x4 метра -> зал 12x12м)
    for gx in -1..=1 {
        for gz in -1..=1 {
            commands.spawn((
                WorldAssetRoot(floor_scene.clone()),
                Transform::from_xyz(gx as f32 * 4.0, 0.0, gz as f32 * 4.0),
            ));
        }
    }

    // 2. Северная и Боковые стены замка (оставляем юг открытым для изометрического обзора)
    // Северная стена: две глухие стены по краям и готическая арка-портал по центру
    for wx in [-4.0, 4.0] {
        commands.spawn((
            WorldAssetRoot(wall_scene.clone()),
            Transform::from_xyz(wx, 0.0, -6.0),
        ));
    }
    commands.spawn((
        WorldAssetRoot(arch_scene.clone()),
        Transform::from_xyz(0.0, 0.0, -6.0),
    ));

    // Западная и Восточная боковые стены
    let rot_90 = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    for wz in [-4.0, 0.0, 4.0] {
        commands.spawn((
            WorldAssetRoot(wall_scene.clone()),
            Transform::from_xyz(-6.0, 0.0, wz).with_rotation(rot_90),
        ));
    }
    for wz in [-4.0, 4.0] {
        commands.spawn((
            WorldAssetRoot(wall_scene.clone()),
            Transform::from_xyz(6.0, 0.0, wz).with_rotation(rot_90),
        ));
    }
    // Восточный портал-арка (выход к ферме/реке)
    commands.spawn((
        WorldAssetRoot(arch_scene.clone()),
        Transform::from_xyz(6.0, 0.0, 0.0).with_rotation(rot_90),
    ));

    // Южный вход (две арки и колонны по углам)
    commands.spawn((
        WorldAssetRoot(arch_scene),
        Transform::from_xyz(0.0, 0.0, 6.0),
    ));

    // 3. Рунические колонны по углам и вдоль проходов
    for (px, pz) in [
        (-6.0, -6.0), (-2.0, -6.0), (2.0, -6.0), (6.0, -6.0),
        (-6.0,  6.0), (-2.0,  6.0), (2.0,  6.0), (6.0,  6.0),
    ] {
        commands.spawn((
            WorldAssetRoot(pillar_scene.clone()),
            Transform::from_xyz(px, 0.0, pz),
        ));
    }

    // 4. Внутреннее убранство: Ритуальный ковер в центре зала
    commands.spawn((
        WorldAssetRoot(rug_scene),
        Transform::from_xyz(0.0, 0.08, -0.5),
    ));

    // 5. Массивный стол исследователя с фолиантом и свечой + готические кресла
    commands.spawn((
        WorldAssetRoot(table_scene),
        Transform::from_xyz(-2.8, 0.08, -1.5),
    ));
    commands.spawn((
        WorldAssetRoot(chair_scene.clone()),
        Transform::from_xyz(-2.8, 0.08, -2.5)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
    ));
    commands.spawn((
        WorldAssetRoot(chair_scene),
        Transform::from_xyz(-4.2, 0.08, -1.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));

    // 6. Книжные шкафы с запретными томами вдоль северной и западной стен
    for bx in [-4.0, 4.0] {
        commands.spawn((
            WorldAssetRoot(bookcase_scene.clone()),
            Transform::from_xyz(bx, 0.08, -5.4)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        ));
    }
    commands.spawn((
        WorldAssetRoot(bookcase_scene),
        Transform::from_xyz(-5.4, 0.08, 2.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));

    // 7. Оккультный алтарь Древних в восточном крыле зала + изумрудное свечение
    commands.spawn((
        WorldAssetRoot(altar_scene),
        Transform::from_xyz(3.5, 0.08, -1.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(0.25, 0.95, 0.55),
            intensity: 45_000.0,
            range: 9.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(3.5, 1.8, -1.5),
    ));

    // 8. Напольные канделябры с теплым светом свечей
    for (cx, cz) in [(-1.8, -4.8), (1.8, -4.8), (-4.8, 4.5), (4.8, 4.5)] {
        commands.spawn((
            WorldAssetRoot(candelabra_scene.clone()),
            Transform::from_xyz(cx, 0.08, cz),
        ));
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.72, 0.32),
                intensity: 30_000.0,
                range: 8.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(cx, 1.65, cz),
        ));
    }

    // Игрок: Загружаем 3D-модель Сыщика из Blender (detective.glb)
    let detective_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/detective.glb"));
    
    commands
        .spawn((
            Player {
                speed: 8.5,
                target_pos: None,
                walk_timer: 0.0,
            },
            Transform::from_xyz(0.0, 0.08, 2.0),
            Visibility::default(),
        ))
        .with_children(|parent| {
            // Вложенная 3D-модель сыщика
            parent.spawn((
                WorldAssetRoot(detective_scene),
                Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)), // разворот лицом вперед
            ));

            // Источник теплого света от керосинового фонаря в руке сыщика
            parent.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.85, 0.45),
                    intensity: 80_000.0,
                    range: 12.0,
                    shadow_maps_enabled: true,
                    ..default()
                },
                Transform::from_xyz(0.35, 1.0, -0.35),
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
        Transform::from_xyz(20.0, 30.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Изометрическая камера (Lineage 2 style)
    commands.spawn((
        Camera3d::default(),
        CameraController {
            yaw: 0.0,
            pitch: 42.0_f32.to_radians(),
            distance: 20.0,
        },
        Transform::from_xyz(0.0, 15.0, 16.0).looking_at(Vec3::ZERO, Vec3::Y),
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

    // 1. Управление клавиатурой (WASD / стрелки) относительно угла камеры
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
        let yaw_rot = Quat::from_rotation_y(cam_controller.yaw);
        let move_dir = yaw_rot * keyboard_dir.normalize();
        player.target_pos = Some(player_transform.translation + move_dir * 3.0);
        return;
    }

    // 2. Клик мыши (LMB) для перемещения к точке на земле
    if mouse_button.pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                if ray.direction.y.abs() > f32::EPSILON {
                    let t = -ray.origin.y / ray.direction.y;
                    if t > 0.0 {
                        let ground_hit = ray.origin + *ray.direction * t;
                        player.target_pos = Some(Vec3::new(ground_hit.x, 0.0, ground_hit.z));
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

                // Плавное покачивание при ходьбе (шаги сыщика)
                player.walk_timer += time.delta_secs() * 12.0;
                transform.translation.y = (player.walk_timer.sin() * 0.06).abs();

                // Поворот лицом к цели движения
                let target_rotation = Quat::from_rotation_y(direction.x.atan2(direction.z));
                transform.rotation = transform.rotation.slerp(target_rotation, 15.0 * time.delta_secs());
            } else {
                player.target_pos = None;
                player.walk_timer = 0.0;
                transform.translation.y = 0.0;
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

    // 1. Вращение камеры перетаскиванием правой кнопкой мыши (RMB Drag)
    if mouse_button.pressed(MouseButton::Right) {
        let delta = mouse_motion.delta;
        let sensitivity = 0.005;

        controller.yaw -= delta.x * sensitivity;
        controller.pitch += delta.y * sensitivity;
        controller.pitch = controller.pitch.clamp(20.0_f32.to_radians(), 75.0_f32.to_radians());
    }

    // 2. Зум колесиком мыши
    if mouse_scroll.delta.y.abs() > f32::EPSILON {
        controller.distance -= mouse_scroll.delta.y * 1.5;
        controller.distance = controller.distance.clamp(8.0, 45.0);
    }

    // 3. Вычисление позиции камеры
    let target = player_transform.translation;
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
