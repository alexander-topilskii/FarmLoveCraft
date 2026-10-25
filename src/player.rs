use std::collections::VecDeque;
use std::time::Duration;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use crate::building::{ActiveBuildingState, BuildingRegistry};
use crate::camera::CameraController;
use crate::navigation::{NavWorld, PathWaypoint, PLAYER_RADIUS};
use crate::rts_command::{ActiveRtsOrder, RtsContextMenuRoot};

#[derive(Resource)]
pub struct CharacterAnimations {
    pub graph_handle: Handle<AnimationGraph>,
    pub idle_node: AnimationNodeIndex,
    pub walk_node: AnimationNodeIndex,
}

#[derive(Component)]
pub struct Player {
    pub speed: f32,
    pub current_floor: i32,
    pub path: VecDeque<PathWaypoint>,
    pub keyboard_vel: Vec3,
    pub is_moving: bool,
}

/// Наблюдатель (Observer), срабатывающий когда иерархия сцены glTF спавнится в мире
pub fn on_character_scene_ready(
    scene_ready: On<WorldInstanceReady>,
    mut commands: Commands,
    animations: Res<CharacterAnimations>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
) {
    for descendant in children.iter_descendants(scene_ready.entity) {
        if let Ok(mut player) = players.get_mut(descendant) {
            let mut transitions = AnimationTransitions::new();
            transitions
                .play(&mut player, animations.idle_node, Duration::from_millis(200))
                .repeat();

            commands
                .entity(descendant)
                .insert(AnimationGraphHandle(animations.graph_handle.clone()))
                .insert(transitions);
            break;
        }
    }
}

fn ray_plane_intersection_xz(ray: Ray3d, plane_y: f32) -> Option<Vec2> {
    if ray.direction.y.abs() <= f32::EPSILON {
        return None;
    }
    let t = (plane_y - ray.origin.y) / ray.direction.y;
    if t > 0.0 {
        let hit = ray.origin + *ray.direction * t;
        Some(Vec2::new(hit.x, hit.z))
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
pub fn player_input_system(
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    nav_world: Res<NavWorld>,
    building_state: Res<ActiveBuildingState>,
    building_registry: Res<BuildingRegistry>,
    mut active_order: Option<ResMut<ActiveRtsOrder>>,
    menu_roots: Query<Entity, With<RtsContextMenuRoot>>,
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
        player.keyboard_vel = yaw_rot * keyboard_dir.normalize();
        player.path.clear();
        return;
    } else {
        player.keyboard_vel = Vec3::ZERO;
    }

    // 2. Клик мыши (LMB) для многоэтажного поиска пути A*
    if mouse_button.just_pressed(MouseButton::Left) {
        // Если открыто контекстное RTS-меню — клик обрабатывается системой меню
        if !menu_roots.is_empty() {
            return;
        }

        if let Some(ref mut order_res) = active_order {
            order_res.current = None;
            order_res.status_message.clear();
        }
        let Some(cursor_pos) = window.cursor_position() else {
            return;
        };
        let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
            return;
        };

        let start_xz = Vec2::new(player_transform.translation.x, player_transform.translation.z);
        let start_floor = player.current_floor;

        // Проверяем, не кликнул ли игрок прямо по межэтажной лестнице в текущем здании,
        // чтобы автоматически подняться или спуститься на другой этаж одним кликом!
        let mut stair_click_target: Option<(Vec2, i32)> = None;
        if building_state.active_building.is_some() {
            for stair in &nav_world.stairs {
                for sample_step in 1..=5 {
                    let sample_t = sample_step as f32 / 6.0;
                    let sample_y = stair.elevation_at(sample_t);
                    if let Some(hit_xz) = ray_plane_intersection_xz(ray, sample_y) {
                        if let Some(proj_t) = stair.project_point(hit_xz, 0.20) {
                            if (proj_t - sample_t).abs() < 0.28 {
                                if building_state.viewed_floor == stair.lower_floor && proj_t >= 0.35
                                {
                                    stair_click_target =
                                        Some((stair.upper_entry, stair.upper_floor));
                                } else if building_state.viewed_floor == stair.upper_floor
                                    && proj_t <= 0.65
                                {
                                    stair_click_target =
                                        Some((stair.lower_entry, stair.lower_floor));
                                }
                                break;
                            }
                        }
                    }
                }
                if stair_click_target.is_some() {
                    break;
                }
            }
        }

        let (goal_xz, goal_floor) = if let Some(target) = stair_click_target {
            target
        } else {
            let mut resolved_target = None;
            if let Some(b_id) = building_state.active_building {
                if let Some(b_def) = building_registry.get(b_id) {
                    let viewed_floor = building_state.viewed_floor;
                    let plane_y = b_def.floor_elevation(viewed_floor);
                    if let Some(hit_xz) = ray_plane_intersection_xz(ray, plane_y) {
                        if b_def.bounds.contains(hit_xz) {
                            resolved_target = Some((hit_xz, viewed_floor));
                        }
                    }
                }
            }

            if let Some(target) = resolved_target {
                target
            } else if let Some(ground_xz) = ray_plane_intersection_xz(ray, 0.0) {
                (ground_xz, 0)
            } else {
                return;
            }
        };

        let waypoints = nav_world.find_path(start_xz, start_floor, goal_xz, goal_floor);
        player.path = waypoints.into();
    }
}

pub fn player_movement_system(
    time: Res<Time>,
    nav_world: Res<NavWorld>,
    animations: Option<Res<CharacterAnimations>>,
    mut gizmos: Gizmos,
    mut query: Query<(&mut Player, &mut Transform)>,
    mut animation_players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let Some(animations) = animations else { return };

    for (mut player, mut transform) in &mut query {
        // Отрисовка найденного многоэтажного пути тонкими золотистыми линиями
        if !player.path.is_empty() {
            let mut prev = Vec3::new(
                transform.translation.x,
                transform.translation.y + 0.12,
                transform.translation.z,
            );
            for wp in &player.path {
                let next = Vec3::new(wp.pos.x, wp.pos.y + 0.12, wp.pos.z);
                gizmos.line(prev, next, Color::srgba(0.95, 0.78, 0.30, 0.65));
                prev = next;
            }
            if let Some(final_wp) = player.path.back() {
                gizmos.circle(
                    Isometry3d::new(
                        Vec3::new(final_wp.pos.x, final_wp.pos.y + 0.12, final_wp.pos.z),
                        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                    ),
                    0.30,
                    Color::srgba(0.25, 0.95, 0.55, 0.85),
                );
            }
        }

        let mut move_dir = Vec3::ZERO;
        let mut step_dist = player.speed * time.delta_secs();

        if player.keyboard_vel.length_squared() > 0.0 {
            move_dir = player.keyboard_vel;
        } else if let Some(&target) = player.path.front() {
            let to_target = Vec3::new(
                target.pos.x - transform.translation.x,
                0.0,
                target.pos.z - transform.translation.z,
            );
            let distance = to_target.length();
            let stop_tolerance = 0.14;

            if distance <= stop_tolerance {
                if let Some(reached) = player.path.pop_front() {
                    player.current_floor = reached.floor;
                }
                if let Some(&next_target) = player.path.front() {
                    let next_vec = Vec3::new(
                        next_target.pos.x - transform.translation.x,
                        0.0,
                        next_target.pos.z - transform.translation.z,
                    );
                    if next_vec.length_squared() > 1e-4 {
                        move_dir = next_vec.normalize();
                        step_dist = step_dist.min(next_vec.length());
                    }
                }
            } else {
                move_dir = to_target.normalize();
                step_dist = step_dist.min(distance);
            }
        }

        let moving_now = move_dir.length_squared() > 0.0;

        if moving_now {
            let proposed = transform.translation + move_dir * step_dist;
            let resolved_xz = nav_world.resolve_collisions_on_floor(
                Vec2::new(proposed.x, proposed.z),
                player.current_floor,
                PLAYER_RADIUS,
            );
            let actual_delta = Vec3::new(
                resolved_xz.x - transform.translation.x,
                0.0,
                resolved_xz.y - transform.translation.z,
            );

            let (new_y, new_floor) =
                nav_world.sample_elevation_and_floor(resolved_xz, player.current_floor);

            transform.translation.x = resolved_xz.x;
            transform.translation.z = resolved_xz.y;
            transform.translation.y = new_y;
            player.current_floor = new_floor;

            let face_dir = if actual_delta.length_squared() > 1e-5 {
                actual_delta.normalize()
            } else {
                move_dir
            };
            let target_rotation = Quat::from_rotation_y(face_dir.x.atan2(face_dir.z));
            transform.rotation = transform
                .rotation
                .slerp(target_rotation, 15.0 * time.delta_secs());
        } else {
            let current_xz = Vec2::new(transform.translation.x, transform.translation.z);
            let (new_y, new_floor) =
                nav_world.sample_elevation_and_floor(current_xz, player.current_floor);
            transform.translation.y = new_y;
            player.current_floor = new_floor;
        }

        // Переключение скелетных анимаций при смене состояния Движение / Покой
        if moving_now != player.is_moving {
            player.is_moving = moving_now;
            for (mut anim_player, mut transitions) in &mut animation_players {
                let target_node = if moving_now {
                    animations.walk_node
                } else {
                    animations.idle_node
                };

                transitions
                    .play(&mut anim_player, target_node, Duration::from_millis(200))
                    .repeat();
            }
        }
    }
}
