use bevy::prelude::*;
use crate::building::{
    ActiveBuildingState, BuildingId, BuildingRegistry, CutawayPillar, CutawayWall,
};
use crate::camera::CameraController;
use crate::destruction::{
    apply_damage_to_entity, spawn_debris_burst, Destructible, DestructibleMaterial,
    PlayerInventory, ResourceModelAssets, ResourceType, SharedVfxAssets,
    StructuralIntegrityDirty, StructuralKind, StructuralPiece, TentacleCrop,
};
use crate::navigation::NavWorld;
use crate::player::Player;
use crate::terrain::{TerrainOp, WorldTerrain};

/// Набор 3D-ассетов для модульного строительства игроком через RTS-меню
#[derive(Resource, Clone)]
pub struct BuildableKitAssets {
    pub wall: Handle<WorldAsset>,
    pub arch: Handle<WorldAsset>,
    pub pillar: Handle<WorldAsset>,
    pub floor: Handle<WorldAsset>,
    pub candelabra: Handle<WorldAsset>,
    pub bush_tentacle: Handle<WorldAsset>,
}

/// Чертежи модульных построек, доступных в RTS-меню
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildBlueprint {
    /// Готическая каменная стена (4м)
    CastleWall,
    /// Готическая стрельчатая арка с проходом (4м)
    CastleArch,
    /// Несущая восьмигранная колонна (дает максимум опоры верхним этажам)
    CastlePillar,
    /// Каменная плита пола / межэтажного перекрытия (4x4м)
    FloorSlab,
    /// Пятирожковый канделябр (освещает территорию)
    Candelabra,
    /// Фермерский саженец Лавкрафтовских Щупалец (производит Страхи)
    TentacleSprout,
}

impl BuildBlueprint {
    pub fn title(self) -> &'static str {
        match self {
            BuildBlueprint::CastleWall => "Готическая стена",
            BuildBlueprint::CastleArch => "Готическая арка",
            BuildBlueprint::CastlePillar => "Несущая колонна",
            BuildBlueprint::FloorSlab => "Каменная плита перекрытия",
            BuildBlueprint::Candelabra => "Кованый канделябр",
            BuildBlueprint::TentacleSprout => "Саженец Щупалец (Ферма)",
        }
    }

    pub fn costs(self) -> &'static [(ResourceType, u32)] {
        match self {
            BuildBlueprint::CastleWall => &[(ResourceType::Stone, 2)],
            BuildBlueprint::CastleArch => &[(ResourceType::Stone, 2)],
            BuildBlueprint::CastlePillar => &[(ResourceType::Stone, 2)],
            BuildBlueprint::FloorSlab => &[(ResourceType::Stone, 2)],
            BuildBlueprint::Candelabra => &[(ResourceType::Wood, 1), (ResourceType::Fear, 1)],
            BuildBlueprint::TentacleSprout => &[(ResourceType::Soil, 1)],
        }
    }
}

/// Тип RTS-приказа, который выполняет Сыщик
#[derive(Clone, Debug)]
pub enum RtsOrder {
    /// Подойти к объекту и разрушить его топором/киркой
    Demolish {
        target: Entity,
        target_pos: Vec3,
        floor: i32,
        swing_timer: f32,
    },
    /// Подойти к поврежденному объекту и починить его
    Repair {
        target: Entity,
        target_pos: Vec3,
        floor: i32,
    },
    /// Подойти к точке рельефа и выполнить терраформинг (копать / насыпать / выровнять)
    Terraform {
        center_xz: Vec2,
        target_y: f32,
        floor: i32,
        op: TerrainOp,
        work_timer: f32,
    },
    /// Подойти к точке и возвести модульную постройку / высадить растение
    Build {
        pos: Vec3,
        floor: i32,
        yaw: f32,
        blueprint: BuildBlueprint,
        work_timer: f32,
    },
}

/// Текущий активный приказ Сыщика и статус последнего действия для HUD
#[derive(Resource, Default)]
pub struct ActiveRtsOrder {
    pub current: Option<RtsOrder>,
    pub status_message: String,
}

/// Отслеживание клика ПКМ (отличаем короткий RTS-клик от зажатия ПКМ для вращения камеры)
#[derive(Resource, Default)]
pub struct RightClickTracker {
    pub press_pos: Option<Vec2>,
    pub max_drag_dist: f32,
}

/// Корневой UI-узел открытого контекстного меню приказов
#[derive(Component)]
pub struct RtsContextMenuRoot;

/// Действие кнопки внутри контекстного RTS-меню с экранным прямоугольником и горячей клавишей
#[derive(Component, Clone, Debug)]
pub struct RtsMenuOption {
    pub action: RtsMenuAction,
    pub screen_rect: Rect,
    pub hotkey_index: usize,
    pub base_color: Color,
}

#[derive(Clone, Debug)]
pub enum RtsMenuAction {
    IssueOrder(RtsOrder),
    Cancel,
}

/// Маркер UI-панели инвентаря ресурсов и текущего приказа
#[derive(Component)]
pub struct InventoryHudText;

fn ray_plane_hit(ray: Ray3d, plane_y: f32) -> Option<Vec3> {
    if ray.direction.y.abs() <= 1e-5 {
        return None;
    }
    let t = (plane_y - ray.origin.y) / ray.direction.y;
    if t > 0.0 {
        Some(ray.origin + *ray.direction * t)
    } else {
        None
    }
}

/// Пересечение 3D-луча с вертикальным цилиндром объекта `(center_base, radius, height)`
fn ray_cylinder_distance(ray: Ray3d, base_pos: Vec3, radius: f32, height: f32) -> Option<f32> {
    let min_y = base_pos.y - 0.25;
    let max_y = base_pos.y + height + 0.25;

    // Проверяем несколько высотных срезов вдоль объекта для точного выбора стен, деревьев и колонн
    let mut best_t: Option<f32> = None;
    for step in 0..=8 {
        let frac = step as f32 / 8.0;
        let sample_y = min_y + (max_y - min_y) * frac;
        if let Some(hit) = ray_plane_hit(ray, sample_y) {
            let dxz = Vec2::new(hit.x - base_pos.x, hit.z - base_pos.z).length();
            if dxz <= radius {
                let t = (hit - ray.origin).length();
                if best_t.map_or(true, |b| t < b) {
                    best_t = Some(t);
                }
            }
        }
    }
    best_t
}

/// Округляет координаты постройки к ближайшему узлу модульной сетки `step` (например 2.0м)
fn snap_to_grid(val: f32, step: f32) -> f32 {
    (val / step).round() * step
}

/// Система обработки ПКМ для открытия контекстного RTS-меню («Разрушить», «Копать», «Построить»)
#[allow(clippy::too_many_arguments)]
pub fn rts_right_click_menu_system(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    mut tracker: ResMut<RightClickTracker>,
    terrain: Res<WorldTerrain>,
    building_state: Res<ActiveBuildingState>,
    building_registry: Res<BuildingRegistry>,
    camera_query: Query<(&Camera, &GlobalTransform), With<CameraController>>,
    player_query: Query<(&Transform, &Player)>,
    destructibles_query: Query<(
        Entity,
        &GlobalTransform,
        &Destructible,
        Option<&StructuralPiece>,
        Option<&InheritedVisibility>,
    )>,
    existing_menus: Query<Entity, With<RtsContextMenuRoot>>,
) {
    // Закрытие меню по Escape
    if keyboard_input.just_pressed(KeyCode::Escape) {
        for menu in &existing_menus {
            commands.entity(menu).despawn();
        }
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    if mouse_button.just_pressed(MouseButton::Right) {
        tracker.press_pos = Some(cursor_pos);
        tracker.max_drag_dist = 0.0;
    }

    if mouse_button.pressed(MouseButton::Right) {
        if let Some(start) = tracker.press_pos {
            let dist = start.distance(cursor_pos);
            if dist > tracker.max_drag_dist {
                tracker.max_drag_dist = dist;
            }
        }
    }

    // Если игрок отпустил ПКМ без перетаскивания камеры (< 7 пикселей) — открываем RTS-меню!
    if mouse_button.just_released(MouseButton::Right) {
        let was_click = tracker.press_pos.is_some() && tracker.max_drag_dist < 7.0;
        tracker.press_pos = None;

        if !was_click {
            return;
        }

        // Удаляем старое контекстное меню, если было открыто
        for menu in &existing_menus {
            commands.entity(menu).despawn();
        }

        let Ok((camera, cam_transform)) = camera_query.single() else {
            return;
        };
        let Ok((player_tf, player)) = player_query.single() else {
            return;
        };
        let Ok(ray) = camera.viewport_to_world(cam_transform, cursor_pos) else {
            return;
        };

        // 1. Ищем ближайший видимый разрушаемый объект под лучом мыши
        let mut best_target: Option<(
            Entity,
            Vec3,
            Destructible,
            Option<StructuralPiece>,
            f32,
        )> = None;

        for (entity, gtf, dest, piece_opt, vis_opt) in &destructibles_query {
            if let Some(vis) = vis_opt {
                if !vis.get() {
                    continue;
                }
            }
            let pos = gtf.translation();
            if let Some(t) = ray_cylinder_distance(ray, pos, dest.pick_radius, dest.pick_height) {
                if best_target.as_ref().map_or(true, |b| t < b.4) {
                    best_target = Some((entity, pos, dest.clone(), piece_opt.cloned(), t));
                }
            }
        }

        // 2. Вычисляем мировую точку на активном этаже или на рельефе
        let mut target_floor = building_state.viewed_floor;
        let plane_y = if let Some(b_id) = building_state.active_building {
            building_registry
                .get(b_id)
                .map(|b| b.floor_elevation(target_floor))
                .unwrap_or(0.0)
        } else {
            target_floor = 0;
            player_tf.translation.y
        };

        let raw_ground_hit = ray_plane_hit(ray, plane_y)
            .or_else(|| ray_plane_hit(ray, 0.0))
            .unwrap_or(player_tf.translation);

        let ground_y = if target_floor == 0 {
            terrain.sample_height(raw_ground_hit.x, raw_ground_hit.z)
        } else {
            plane_y
        };
        let world_click_pos = Vec3::new(raw_ground_hit.x, ground_y, raw_ground_hit.z);

        // Определяем ориентацию постройки (перпендикулярно взгляду от игрока к точке клика)
        let to_click = Vec2::new(
            world_click_pos.x - player_tf.translation.x,
            world_click_pos.z - player_tf.translation.z,
        );
        let build_yaw = if to_click.x.abs() > to_click.y.abs() {
            std::f32::consts::FRAC_PI_2
        } else {
            0.0
        };

        let snapped_pos = Vec3::new(
            snap_to_grid(world_click_pos.x, 2.0),
            ground_y,
            snap_to_grid(world_click_pos.z, 2.0),
        );

        spawn_rts_context_menu(
            &mut commands,
            cursor_pos,
            Vec2::new(window.width(), window.height()),
            best_target,
            world_click_pos,
            snapped_pos,
            target_floor,
            build_yaw,
            player_tf.translation.y,
            player.current_floor,
            &terrain,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_rts_context_menu(
    commands: &mut Commands,
    cursor_pos: Vec2,
    window_size: Vec2,
    hit_target: Option<(Entity, Vec3, Destructible, Option<StructuralPiece>, f32)>,
    world_click_pos: Vec3,
    snapped_pos: Vec3,
    target_floor: i32,
    build_yaw: f32,
    player_y: f32,
    player_floor: i32,
    terrain: &WorldTerrain,
) {
    let menu_width = 330.0_f32;
    let menu_left = cursor_pos.x.min((window_size.x - menu_width - 12.0).max(12.0));
    let menu_top = cursor_pos.y.min((window_size.y - 420.0).max(12.0));
    let row_height = 26.0_f32;
    let row_gap = 4.0_f32;

    let biome = terrain.sample_biome(world_click_pos.x, world_click_pos.z);

    let header_title = if let Some((_, _, ref dest, ref piece_opt, _)) = hit_target {
        if let Some(piece) = piece_opt {
            format!(
                "{} (HP {:.0}/{:.0} | Опора {:.0}%)",
                dest.title,
                dest.hp,
                dest.max_hp,
                piece.support_ratio() * 100.0
            )
        } else {
            format!("{} (HP {:.0}/{:.0})", dest.title, dest.hp, dest.max_hp)
        }
    } else {
        format!(
            "{} (X:{:.1}, Z:{:.1}, Y:{:.1}м)",
            biome.display_name(),
            world_click_pos.x,
            world_click_pos.z,
            world_click_pos.y
        )
    };

    commands
        .spawn((
            RtsContextMenuRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(menu_left),
                top: Val::Px(menu_top),
                width: Val::Px(menu_width),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(row_gap),
                border: UiRect::all(Val::Px(1.5)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.07, 0.09, 0.95)),
            BorderColor::all(Color::srgb(0.38, 0.56, 0.44)),
        ))
        .with_children(|menu| {
            // Заголовок контекстного меню
            menu.spawn((
                Node {
                    height: Val::Px(22.0),
                    ..default()
                },
                Text::new(header_title),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.85, 0.55)),
            ));

            let mut item_idx = 0_usize;
            let next_rect = |idx: usize| -> Rect {
                let top_y = menu_top + 8.0 + 22.0 + row_gap + (idx as f32) * (row_height + row_gap);
                Rect::from_corners(
                    Vec2::new(menu_left + 8.0, top_y),
                    Vec2::new(menu_left + menu_width - 8.0, top_y + row_height),
                )
            };

            // 1. Действия над выбранным разрушаемым объектом
            if let Some((target_entity, target_pos, dest, _, _)) = hit_target {
                let obj_floor = if target_pos.y > 2.5 { 1 } else { player_floor };
                let rect = next_rect(item_idx);
                item_idx += 1;
                spawn_menu_button(
                    menu,
                    item_idx,
                    rect,
                    format!("Разрушить: {}", dest.title),
                    Color::srgba(0.38, 0.14, 0.14, 0.92),
                    RtsMenuAction::IssueOrder(RtsOrder::Demolish {
                        target: target_entity,
                        target_pos,
                        floor: obj_floor,
                        swing_timer: 0.50,
                    }),
                );

                if dest.hp < dest.max_hp {
                    let rect = next_rect(item_idx);
                    item_idx += 1;
                    spawn_menu_button(
                        menu,
                        item_idx,
                        rect,
                        format!("Починить: {} (-1 Дерево)", dest.title),
                        Color::srgba(0.16, 0.32, 0.22, 0.92),
                        RtsMenuAction::IssueOrder(RtsOrder::Repair {
                            target: target_entity,
                            target_pos,
                            floor: obj_floor,
                        }),
                    );
                }
            }

            // 2. Действия терраформинга рельефа (на 0 этаже)
            if target_floor == 0 {
                let xz = Vec2::new(world_click_pos.x, world_click_pos.z);

                let rect = next_rect(item_idx);
                item_idx += 1;
                spawn_menu_button(
                    menu,
                    item_idx,
                    rect,
                    "Копать землю (+1 Земля, понизить)".to_string(),
                    Color::srgba(0.22, 0.18, 0.12, 0.92),
                    RtsMenuAction::IssueOrder(RtsOrder::Terraform {
                        center_xz: xz,
                        target_y: world_click_pos.y,
                        floor: 0,
                        op: TerrainOp::Dig { depth: 0.95 },
                        work_timer: 0.35,
                    }),
                );

                let rect = next_rect(item_idx);
                item_idx += 1;
                spawn_menu_button(
                    menu,
                    item_idx,
                    rect,
                    "Насыпать холм (-1 Земля, поднять)".to_string(),
                    Color::srgba(0.22, 0.18, 0.12, 0.92),
                    RtsMenuAction::IssueOrder(RtsOrder::Terraform {
                        center_xz: xz,
                        target_y: world_click_pos.y,
                        floor: 0,
                        op: TerrainOp::Raise { height: 0.95 },
                        work_timer: 0.35,
                    }),
                );

                let rect = next_rect(item_idx);
                item_idx += 1;
                spawn_menu_button(
                    menu,
                    item_idx,
                    rect,
                    "Выровнять площадку под уровень ног".to_string(),
                    Color::srgba(0.18, 0.22, 0.26, 0.92),
                    RtsMenuAction::IssueOrder(RtsOrder::Terraform {
                        center_xz: xz,
                        target_y: world_click_pos.y,
                        floor: 0,
                        op: TerrainOp::Flatten { target_y: player_y },
                        work_timer: 0.35,
                    }),
                );

                let rect = next_rect(item_idx);
                item_idx += 1;
                spawn_menu_button(
                    menu,
                    item_idx,
                    rect,
                    "Посадить Щупальца (-1 Земля -> Страхи)".to_string(),
                    Color::srgba(0.12, 0.28, 0.22, 0.92),
                    RtsMenuAction::IssueOrder(RtsOrder::Build {
                        pos: world_click_pos,
                        floor: 0,
                        yaw: 0.0,
                        blueprint: BuildBlueprint::TentacleSprout,
                        work_timer: 0.35,
                    }),
                );
            }

            // 3. Модульное строительство (по принципу Valheim)
            for bp in [
                BuildBlueprint::CastleWall,
                BuildBlueprint::CastleArch,
                BuildBlueprint::CastlePillar,
                BuildBlueprint::FloorSlab,
                BuildBlueprint::Candelabra,
            ] {
                let cost_str = bp
                    .costs()
                    .iter()
                    .map(|(r, c)| format!("-{} {}", c, r.name()))
                    .collect::<Vec<_>>()
                    .join(", ");

                let build_pos = if bp == BuildBlueprint::FloorSlab && target_floor == 0 {
                    // Позволяем перекрывать 2-й этаж над головой или мостить пол
                    Vec3::new(snapped_pos.x, snapped_pos.y + 4.42, snapped_pos.z)
                } else {
                    snapped_pos
                };

                let rect = next_rect(item_idx);
                item_idx += 1;
                spawn_menu_button(
                    menu,
                    item_idx,
                    rect,
                    format!("Строить: {} ({})", bp.title(), cost_str),
                    Color::srgba(0.14, 0.20, 0.28, 0.92),
                    RtsMenuAction::IssueOrder(RtsOrder::Build {
                        pos: build_pos,
                        floor: target_floor,
                        yaw: build_yaw,
                        blueprint: bp,
                        work_timer: 0.40,
                    }),
                );
            }

            // Кнопка отмены
            let rect = next_rect(item_idx);
            item_idx += 1;
            spawn_menu_button(
                menu,
                item_idx,
                rect,
                "Отмена [Esc]".to_string(),
                Color::srgba(0.15, 0.15, 0.18, 0.90),
                RtsMenuAction::Cancel,
            );
        });
}

fn spawn_menu_button(
    parent: &mut ChildSpawnerCommands,
    hotkey_index: usize,
    screen_rect: Rect,
    label: String,
    bg_color: Color,
    action: RtsMenuAction,
) {
    let numbered = if hotkey_index <= 9 {
        format!("[{}] {}", hotkey_index, label)
    } else {
        label
    };
    parent
        .spawn((
            RtsMenuOption {
                action,
                screen_rect,
                hotkey_index,
                base_color: bg_color,
            },
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(26.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(bg_color),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(numbered),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.92, 0.88)),
            ));
        });
}

/// Система обработки наведения, кликов ЛКМ и горячих клавиш [1-9] по пунктам RTS-меню
pub fn rts_menu_interaction_system(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    nav_world: Res<NavWorld>,
    mut active_order: ResMut<ActiveRtsOrder>,
    mut options_query: Query<(&RtsMenuOption, &mut BackgroundColor)>,
    menu_roots: Query<Entity, With<RtsContextMenuRoot>>,
    mut player_query: Query<(&mut Player, &Transform)>,
) {
    if menu_roots.is_empty() {
        return;
    }

    let cursor_pos = windows.single().ok().and_then(|w| w.cursor_position());
    let mut pressed_hotkey: Option<usize> = None;
    for (idx, key) in [
        (1, KeyCode::Digit1),
        (2, KeyCode::Digit2),
        (3, KeyCode::Digit3),
        (4, KeyCode::Digit4),
        (5, KeyCode::Digit5),
        (6, KeyCode::Digit6),
        (7, KeyCode::Digit7),
        (8, KeyCode::Digit8),
        (9, KeyCode::Digit9),
    ] {
        if keyboard_input.just_pressed(key) {
            pressed_hotkey = Some(idx);
            break;
        }
    }

    let lmb_clicked = mouse_button.just_pressed(MouseButton::Left);
    let mut chosen_action: Option<RtsMenuAction> = None;

    for (opt, mut bg) in &mut options_query {
        let hovered = cursor_pos.map_or(false, |cp| opt.screen_rect.contains(cp));
        if hovered {
            *bg = BackgroundColor(Color::srgba(0.32, 0.48, 0.38, 0.98));
        } else {
            *bg = BackgroundColor(opt.base_color);
        }

        if (lmb_clicked && hovered) || pressed_hotkey == Some(opt.hotkey_index) {
            chosen_action = Some(opt.action.clone());
        }
    }

    // Если кликнули ЛКМ мимо меню — просто закрываем меню
    if lmb_clicked && chosen_action.is_none() {
        for root in &menu_roots {
            commands.entity(root).despawn();
        }
        return;
    }

    let Some(action) = chosen_action else {
        return;
    };

    for root in &menu_roots {
        commands.entity(root).despawn();
    }

    match action {
        RtsMenuAction::Cancel => {}
        RtsMenuAction::IssueOrder(order) => {
            if let Ok((mut player, player_tf)) = player_query.single_mut() {
                let start_xz = Vec2::new(player_tf.translation.x, player_tf.translation.z);
                let (goal_xz, goal_floor) = match &order {
                    RtsOrder::Demolish {
                        target_pos, floor, ..
                    }
                    | RtsOrder::Repair {
                        target_pos, floor, ..
                    } => (Vec2::new(target_pos.x, target_pos.z), *floor),
                    RtsOrder::Terraform {
                        center_xz, floor, ..
                    } => (*center_xz, *floor),
                    RtsOrder::Build { pos, floor, .. } => (Vec2::new(pos.x, pos.z), *floor),
                };

                // Прокладываем маршрут к цели приказа (если цель заблокирована самим объектом,
                // `find_nearest_walkable` внутри `find_path` автоматически найдет ближайшую свободную точку рядом!)
                let path = nav_world.find_path(start_xz, player.current_floor, goal_xz, goal_floor);
                if !path.is_empty() {
                    player.path = path.into();
                }
                active_order.status_message = match &order {
                    RtsOrder::Demolish { .. } => "Приказ: Иду разрушать объект...".to_string(),
                    RtsOrder::Repair { .. } => "Приказ: Иду чинить конструкцию...".to_string(),
                    RtsOrder::Terraform { .. } => "Приказ: Иду проводить земляные работы...".to_string(),
                    RtsOrder::Build { blueprint, .. } => {
                        format!("Приказ: Иду строить «{}»...", blueprint.title())
                    }
                };
                active_order.current = Some(order);
            }
        }
    }
}

/// Система выполнения текущего RTS-приказа Сыщиком при подходе к цели
#[allow(clippy::too_many_arguments)]
pub fn execute_rts_order_system(
    mut commands: Commands,
    time: Res<Time>,
    mut gizmos: Gizmos,
    mut active_order: ResMut<ActiveRtsOrder>,
    mut terrain: ResMut<WorldTerrain>,
    mut nav_world: ResMut<NavWorld>,
    mut inventory: ResMut<PlayerInventory>,
    mut integrity_dirty: ResMut<StructuralIntegrityDirty>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    kit_assets: Option<Res<BuildableKitAssets>>,
    loot_models: Option<Res<ResourceModelAssets>>,
    vfx: Option<Res<SharedVfxAssets>>,
    building_state: Res<ActiveBuildingState>,
    mut player_query: Query<(&mut Player, &mut Transform)>,
    mut destructibles_query: Query<(&GlobalTransform, &mut Destructible)>,
) {
    let Ok((mut player, mut player_tf)) = player_query.single_mut() else {
        return;
    };

    // Если игрок перехватил управление клавишами WASD — отменяем текущий автоматический приказ
    if player.keyboard_vel.length_squared() > 0.01 && active_order.current.is_some() {
        active_order.current = None;
        active_order.status_message = "Приказ отменен (ручное управление)".to_string();
        return;
    }

    let Some(mut order) = active_order.current.take() else {
        return;
    };

    let dt = time.delta_secs();
    let player_xz = Vec2::new(player_tf.translation.x, player_tf.translation.z);

    match &mut order {
        RtsOrder::Demolish {
            target,
            target_pos,
            swing_timer,
            ..
        } => {
            let Ok((target_gtf, mut dest)) = destructibles_query.get_mut(*target) else {
                active_order.status_message = "Цель разрушена!".to_string();
                return;
            };
            *target_pos = target_gtf.translation();
            let target_xz = Vec2::new(target_pos.x, target_pos.z);
            let dist = player_xz.distance(target_xz);
            let reach = dest.pick_radius + 1.85;

            // Рисуем красное боевое кольцо вокруг разрушаемой цели
            gizmos.circle(
                Isometry3d::new(
                    *target_pos + Vec3::Y * 0.15,
                    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                ),
                dest.pick_radius.max(0.8),
                Color::srgb(0.95, 0.25, 0.20),
            );

            if dist <= reach {
                player.path.clear();
                // Поворачиваем Сыщика лицом к разрушаемому объекту
                let dir = (target_xz - player_xz).normalize_or_zero();
                if dir.length_squared() > 0.01 {
                    player_tf.rotation = Quat::from_rotation_y(dir.x.atan2(dir.y));
                }

                *swing_timer -= dt;
                if *swing_timer <= 0.0 {
                    *swing_timer = 0.50; // Каждые 0.5с — взмах топором
                    let destroyed = apply_damage_to_entity(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        loot_models.as_deref(),
                        vfx.as_deref(),
                        &mut nav_world,
                        &mut integrity_dirty,
                        *target,
                        *target_pos,
                        &mut dest,
                        10.0, // 10 урона за удар: 10 ударов * 0.5с = ровно 5.0 секунд на дерево (100 HP)!
                    );
                    if destroyed {
                        active_order.status_message =
                            format!("«{}» срублено! Ресурсы выпали на землю.", dest.title);
                        return;
                    } else {
                        let remaining_secs = (dest.hp / 10.0) * 0.5;
                        let progress_pct = ((1.0 - dest.hp / dest.max_hp) * 100.0).clamp(0.0, 100.0);
                        active_order.status_message = format!(
                            "Рубим «{}» [{:.0}%] — осталось {:.1}с",
                            dest.title, progress_pct, remaining_secs
                        );
                    }
                }
            } else if player.path.is_empty() {
                // Если путь закончился чуть раньше — дошагиваем напрямую
                let step = (player.speed * dt).min(dist - reach + 0.2);
                let dir = (target_xz - player_xz).normalize_or_zero();
                player_tf.translation.x += dir.x * step;
                player_tf.translation.z += dir.y * step;
            }
            active_order.current = Some(order);
        }

        RtsOrder::Repair {
            target, target_pos, ..
        } => {
            let Ok((target_gtf, mut dest)) = destructibles_query.get_mut(*target) else {
                return;
            };
            *target_pos = target_gtf.translation();
            let target_xz = Vec2::new(target_pos.x, target_pos.z);
            let dist = player_xz.distance(target_xz);

            if dist <= dest.pick_radius + 1.85 {
                player.path.clear();
                if inventory.spend(&[(ResourceType::Wood, 1)]) {
                    dest.hp = dest.max_hp;
                    active_order.status_message = format!("«{}» полностью отремонтировано!", dest.title);
                } else {
                    active_order.status_message = "Недостаточно Дерева для ремонта!".to_string();
                }
                return;
            }
            active_order.current = Some(order);
        }

        RtsOrder::Terraform {
            center_xz,
            target_y,
            op,
            work_timer,
            ..
        } => {
            let dist = player_xz.distance(*center_xz);
            let marker_pos = Vec3::new(center_xz.x, *target_y + 0.15, center_xz.y);
            gizmos.circle(
                Isometry3d::new(
                    marker_pos,
                    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                ),
                2.8,
                Color::srgb(0.95, 0.75, 0.25),
            );

            if dist <= 3.0 {
                player.path.clear();
                *work_timer -= dt;
                if *work_timer <= 0.0 {
                    match *op {
                        TerrainOp::Dig { .. } => {
                            inventory.add(ResourceType::Soil, 1);
                            active_order.status_message =
                                "Земля раскопана (+1 Земля)! Рельеф обновлен.".to_string();
                        }
                        TerrainOp::Raise { .. } => {
                            if !inventory.spend(&[(ResourceType::Soil, 1)]) {
                                active_order.status_message =
                                    "Нужна 1 Земля для насыпи рельефа!".to_string();
                                return;
                            }
                            active_order.status_message =
                                "Насыпь возведена (-1 Земля)!".to_string();
                        }
                        TerrainOp::Flatten { .. } => {
                            active_order.status_message =
                                "Площадка выровнена под уровень Сыщика!".to_string();
                        }
                        _ => {}
                    }

                    terrain.apply_terraform(*center_xz, 3.6, *op, &mut nav_world);
                    // Если раскопали землю под стенами — пересчитываем структурную опору!
                    integrity_dirty.0 = true;
                    spawn_debris_burst(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        vfx.as_deref(),
                        marker_pos,
                        Color::srgb(0.32, 0.24, 0.16),
                        Some(DestructibleMaterial::Wood),
                        8,
                    );
                    return;
                }
            }
            active_order.current = Some(order);
        }

        RtsOrder::Build {
            pos,
            floor,
            yaw,
            blueprint,
            work_timer,
        } => {
            let build_xz = Vec2::new(pos.x, pos.z);
            let dist = player_xz.distance(build_xz);

            // Рисуем изумрудный голографический контур (Ghost Preview) будущей постройки
            gizmos.cube(
                Transform::from_translation(*pos + Vec3::Y * 1.5)
                    .with_rotation(Quat::from_rotation_y(*yaw))
                    .with_scale(Vec3::new(3.6, 3.0, 0.8)),
                Color::srgba(0.25, 0.95, 0.58, 0.85),
            );

            if dist <= 3.4 {
                player.path.clear();
                *work_timer -= dt;
                if *work_timer <= 0.0 {
                    if !inventory.spend(blueprint.costs()) {
                        active_order.status_message = format!(
                            "Не хватает ресурсов для постройки «{}»!",
                            blueprint.title()
                        );
                        return;
                    }

                    if let Some(ref kit) = kit_assets {
                        spawn_built_piece(
                            &mut commands,
                            &mut nav_world,
                            &mut integrity_dirty,
                            kit,
                            building_state.active_building.unwrap_or(BuildingId(1)),
                            *pos,
                            *floor,
                            *yaw,
                            *blueprint,
                        );
                        active_order.status_message =
                            format!("Построено: «{}»!", blueprint.title());
                    }
                    return;
                }
            }
            active_order.current = Some(order);
        }
    }
}

/// Спавнит возведенную игроком модульную конструкцию с коллайдерами, разрушаемостью и опорой Valheim
#[allow(clippy::too_many_arguments)]
fn spawn_built_piece(
    commands: &mut Commands,
    nav_world: &mut NavWorld,
    integrity_dirty: &mut StructuralIntegrityDirty,
    kit: &BuildableKitAssets,
    building_id: BuildingId,
    pos: Vec3,
    floor: i32,
    yaw: f32,
    blueprint: BuildBlueprint,
) {
    let rot = Quat::from_rotation_y(yaw);
    let rotated_90 = yaw.abs() > 0.5;
    let normal = if rotated_90 {
        Vec2::new(1.0, 0.0)
    } else {
        Vec2::new(0.0, 1.0)
    };
    let xz = Vec2::new(pos.x, pos.z);

    match blueprint {
        BuildBlueprint::CastleWall => {
            let (hx, hz) = if rotated_90 { (0.38, 2.0) } else { (2.0, 0.38) };
            let col = nav_world.add_box_on_floor(pos.x, pos.z, hx, hz, floor);
            commands.spawn((
                WorldAssetRoot(kit.wall.clone()),
                Transform::from_translation(pos).with_rotation(rot),
                CutawayWall::interior_glb(building_id, floor, normal, xz, pos.y),
                Destructible::wall("Построенная каменная стена", vec![col]),
                StructuralPiece::new(StructuralKind::Wall, pos, floor),
            ));
        }
        BuildBlueprint::CastleArch => {
            let cols = nav_world.add_arch_on_floor(pos.x, pos.z, rotated_90, floor);
            commands.spawn((
                WorldAssetRoot(kit.arch.clone()),
                Transform::from_translation(pos).with_rotation(rot),
                CutawayWall::interior_glb(building_id, floor, normal, xz, pos.y),
                Destructible::arch("Построенная готическая арка", cols),
                StructuralPiece::new(StructuralKind::Wall, pos, floor),
            ));
        }
        BuildBlueprint::CastlePillar => {
            let col = nav_world.add_box_on_floor(pos.x, pos.z, 0.58, 0.58, floor);
            commands.spawn((
                WorldAssetRoot(kit.pillar.clone()),
                Transform::from_translation(pos),
                CutawayPillar::interior_glb(building_id, floor, xz, pos.y),
                Destructible::pillar("Построенная несущая колонна", vec![col]),
                StructuralPiece::new(StructuralKind::Pillar, pos, floor),
            ));
        }
        BuildBlueprint::FloorSlab => {
            let slab_floor = if pos.y > 2.0 { 1 } else { floor };
            nav_world.add_floor_zone(
                Rect::from_center_half_size(xz, Vec2::splat(2.1)),
                slab_floor,
                pos.y + 0.08,
            );
            commands.spawn((
                WorldAssetRoot(kit.floor.clone()),
                Transform::from_translation(pos),
                Destructible::floor_tile("Каменная плита перекрытия"),
                StructuralPiece::new(StructuralKind::Floor, pos, slab_floor),
            ));
        }
        BuildBlueprint::Candelabra => {
            let col = nav_world.add_box_on_floor(pos.x, pos.z, 0.42, 0.42, floor);
            commands
                .spawn((
                    WorldAssetRoot(kit.candelabra.clone()),
                    Transform::from_translation(pos),
                    Destructible::furniture(
                        "Кованый канделябр",
                        vec![col],
                        vec![(ResourceType::Wood, 1), (ResourceType::Fear, 1)],
                    ),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        PointLight {
                            color: Color::srgb(1.0, 0.76, 0.35),
                            intensity: 50_000.0,
                            range: 12.0,
                            shadow_maps_enabled: false,
                            ..default()
                        },
                        Transform::from_xyz(0.0, 2.2, 0.0),
                    ));
                });
        }
        BuildBlueprint::TentacleSprout => {
            commands
                .spawn((
                    WorldAssetRoot(kit.bush_tentacle.clone()),
                    Transform::from_translation(pos).with_scale(Vec3::splat(0.35)),
                    TentacleCrop {
                        growth: 0.0,
                        harvest_ready: false,
                    },
                    Destructible {
                        title: "Выращенные Щупальца Бездны",
                        hp: 25.0,
                        max_hp: 25.0,
                        material: crate::destruction::DestructibleMaterial::Occult,
                        drops: vec![(ResourceType::Fear, 4), (ResourceType::Soil, 1)],
                        colliders: Vec::new(),
                        pick_radius: 1.3,
                        pick_height: 2.0,
                    },
                ))
                .with_children(|parent| {
                    parent.spawn((
                        PointLight {
                            color: Color::srgb(0.22, 0.95, 0.58),
                            intensity: 35_000.0,
                            range: 8.0,
                            shadow_maps_enabled: false,
                            ..default()
                        },
                        Transform::from_xyz(0.0, 1.2, 0.0),
                    ));
                });
        }
    }

    integrity_dirty.0 = true;
}

/// Система обновления верхней панели ресурсов и текущего RTS-статуса
pub fn update_inventory_hud_system(
    inventory: Res<PlayerInventory>,
    active_order: Res<ActiveRtsOrder>,
    terrain: Res<WorldTerrain>,
    player_query: Query<&Transform, With<Player>>,
    mut hud_query: Query<&mut Text, With<InventoryHudText>>,
    mut last_biome: Local<Option<crate::terrain::Biome>>,
    mut last_chunk_count: Local<usize>,
) {
    let current_biome = player_query
        .single()
        .map(|tf| terrain.sample_biome(tf.translation.x, tf.translation.z))
        .ok();

    if !inventory.is_changed()
        && !active_order.is_changed()
        && *last_biome == current_biome
        && *last_chunk_count == terrain.spawned_chunks.len()
    {
        return;
    }

    *last_biome = current_biome;
    *last_chunk_count = terrain.spawned_chunks.len();

    let Ok(mut text) = hud_query.single_mut() else {
        return;
    };

    let biome_name = current_biome
        .map(|b| b.display_name())
        .unwrap_or("Аркхэм");

    let status = if active_order.status_message.is_empty() {
        "ПКМ по объекту или земле — меню приказов (Разрушить / Копать / Строить)"
    } else {
        active_order.status_message.as_str()
    };

    **text = format!(
        "Биом: {} ({} чанков) | Дерево: {} | Камень: {} | Листва: {} | Споры: {} | Земля: {} | Зацепки: {}\n{}",
        biome_name,
        *last_chunk_count,
        inventory.wood,
        inventory.stone,
        inventory.foliage,
        inventory.fear,
        inventory.soil,
        inventory.clues,
        status
    );
}
