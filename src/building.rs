use std::collections::HashMap;
use bevy::prelude::*;
use crate::camera::CameraController;
use crate::navigation::{NavWorld, StairLink};
use crate::player::Player;

/// Уровень непрозрачности (альфа-канал) для «отрезанных» стен, закрывающих обзор камере
pub const CUTAWAY_ALPHA: f32 = 0.18;

/// Уникальный идентификатор здания/помещения
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BuildingId(pub u32);

/// Описание отдельного этажа здания
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct FloorDefinition {
    pub floor_index: i32,
    pub elevation: f32,
    pub wall_height: f32,
}

/// Описание многоэтажного здания для реестра помещений
#[derive(Clone, Debug)]
pub struct BuildingDefinition {
    pub id: BuildingId,
    pub name: &'static str,
    /// Прямоугольник (в плоскости XZ), внутри которого игрок считается вошедшим в здание
    pub bounds: Rect,
    /// Есть ли у здания внешняя монолитная 3D-оболочка (например `outdoor_inn.glb`),
    /// которая скрывается при входе внутрь в пользу модульного интерьера
    pub has_exterior_shell: bool,
    pub floors: Vec<FloorDefinition>,
}

impl BuildingDefinition {
    pub fn max_floor(&self) -> i32 {
        self.floors.iter().map(|f| f.floor_index).max().unwrap_or(0)
    }

    pub fn floor_elevation(&self, floor_index: i32) -> f32 {
        self.floors
            .iter()
            .find(|f| f.floor_index == floor_index)
            .map(|f| f.elevation)
            .unwrap_or(0.0)
    }
}

/// Реестр всех зданий сцены с поддержкой переиспользования
#[derive(Resource, Default)]
pub struct BuildingRegistry {
    pub buildings: Vec<BuildingDefinition>,
}

impl BuildingRegistry {
    pub fn register(&mut self, def: BuildingDefinition) -> BuildingId {
        let id = def.id;
        self.buildings.push(def);
        id
    }

    pub fn get(&self, id: BuildingId) -> Option<&BuildingDefinition> {
        self.buildings.iter().find(|b| b.id == id)
    }

    pub fn find_building_at(&self, xz: Vec2) -> Option<&BuildingDefinition> {
        self.buildings.iter().find(|b| b.bounds.contains(xz))
    }
}

/// Текущее состояние активного помещения и просматриваемого этажа
#[derive(Resource, Default, Debug)]
pub struct ActiveBuildingState {
    pub active_building: Option<BuildingId>,
    pub player_floor: i32,
    pub viewed_floor: i32,
    pub manual_floor_override: Option<i32>,
}

/// Кэш полупрозрачных копий материалов для мгновенного переключения прозрачности стен без аллокаций каждый кадр
#[derive(Resource, Default)]
pub struct CutawayMaterialCache {
    pub transparent_by_opaque: HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>>,
}

/// Компонент, автоматически сохраняющий исходный непрозрачный материал меша
#[derive(Component, Clone)]
pub struct OriginalMaterial(pub Handle<StandardMaterial>);

/// Правило видимости этажа, когда персонаж находится внутри здания
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[allow(dead_code)]
pub enum FloorVisibilityRule {
    /// Показывать ТОЛЬКО активный этаж (скрывать все неактивные этажи — и выше, и ниже)
    #[default]
    OnlyWhenActive,
    /// Показывать активный этаж и этажи ниже него, скрывая только верхние этажи
    ActiveOrBelow,
}

/// Корневой компонент этажа здания (управляет видимостью всего этажа)
#[derive(Component)]
pub struct BuildingFloorRoot {
    pub building_id: BuildingId,
    pub floor_index: i32,
    pub rule: FloorVisibilityRule,
}

/// Компонент для объектов, соединяющих несколько этажей (например, межэтажная лестница),
/// которые должны оставаться видимыми при нахождении на любом из связываемых этажей
#[derive(Component)]
pub struct BuildingSpanFloors {
    pub building_id: BuildingId,
    pub min_floor: i32,
    pub max_floor: i32,
}

/// Крыша здания — отображается снаружи и автоматически скрывается при входе игрока внутрь
#[derive(Component)]
pub struct BuildingRoof {
    pub building_id: BuildingId,
}

/// Внешняя оболочка дома (для готовых GLB зданий улицы), которая отображается снаружи
/// и скрывается при входе внутрь, открывая модульный многоэтажный интерьер
#[derive(Component)]
pub struct BuildingExteriorShell {
    pub building_id: BuildingId,
}

/// Стена или арка, которая становится полупрозрачной, когда закрывает обзор камере
#[derive(Component)]
pub struct CutawayWall {
    pub building_id: BuildingId,
    pub floor_index: i32,
    /// Внешняя нормаль стены в плоскости XZ (смотрит наружу от помещения)
    pub outward_normal: Vec2,
    /// Мировая координата центра стены в XZ
    pub world_xz: Vec2,
    /// Является ли стена внутренней перегородкой внутри этажа
    pub is_interior_partition: bool,
}

impl CutawayWall {
    pub fn exterior_glb(
        building_id: BuildingId,
        floor_index: i32,
        outward_normal: Vec2,
        world_xz: Vec2,
        _base_y: f32,
    ) -> Self {
        Self {
            building_id,
            floor_index,
            outward_normal: outward_normal.normalize_or_zero(),
            world_xz,
            is_interior_partition: false,
        }
    }

    pub fn interior_glb(
        building_id: BuildingId,
        floor_index: i32,
        normal: Vec2,
        world_xz: Vec2,
        _base_y: f32,
    ) -> Self {
        Self {
            building_id,
            floor_index,
            outward_normal: normal.normalize_or_zero(),
            world_xz,
            is_interior_partition: true,
        }
    }
}

/// Колонна или пилон, который становится полупрозрачным вместе с примыкающими к нему стенами
#[derive(Component)]
pub struct CutawayPillar {
    pub building_id: BuildingId,
    pub floor_index: i32,
    /// Нормали примыкающих внешних стен (если пусто — внутренняя колонна зала)
    pub adjacent_normals: Vec<Vec2>,
    pub world_xz: Vec2,
}

impl CutawayPillar {
    pub fn perimeter_glb(
        building_id: BuildingId,
        floor_index: i32,
        adjacent_normals: Vec<Vec2>,
        world_xz: Vec2,
        _base_y: f32,
    ) -> Self {
        Self {
            building_id,
            floor_index,
            adjacent_normals,
            world_xz,
        }
    }

    pub fn interior_glb(
        building_id: BuildingId,
        floor_index: i32,
        world_xz: Vec2,
        _base_y: f32,
    ) -> Self {
        Self {
            building_id,
            floor_index,
            adjacent_normals: Vec::new(),
            world_xz,
        }
    }
}

/// Высокая пристенная мебель (шкафы, часы, камины), которая тоже становится полупрозрачной,
/// когда стена за ней прозрачна, чтобы не загораживать обзор
#[derive(Component)]
pub struct CutawayAttachedProp {
    pub building_id: BuildingId,
    pub floor_index: i32,
    pub wall_normal: Vec2,
}

impl CutawayAttachedProp {
    pub fn new(
        building_id: BuildingId,
        floor_index: i32,
        wall_normal: Vec2,
        _base_y: f32,
    ) -> Self {
        Self {
            building_id,
            floor_index,
            wall_normal: wall_normal.normalize_or_zero(),
        }
    }
}

/// Маркер UI-текста статуса помещения и этажа
#[derive(Component)]
pub struct BuildingHudText;

/// Система определения текущего здания и активного этажа (плюс ручное переключение обзора этажа по Tab / PageUp / PageDown)
pub fn update_active_building_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    registry: Res<BuildingRegistry>,
    mut state: ResMut<ActiveBuildingState>,
    player_query: Query<(&Transform, &Player)>,
) {
    let Ok((player_tf, player)) = player_query.single() else {
        return;
    };

    let player_xz = Vec2::new(player_tf.translation.x, player_tf.translation.z);
    let current_building = registry.find_building_at(player_xz).map(|b| b.id);

    // Если игрок сменил здание или физически перешел на другой этаж по лестнице — сбрасываем ручной оверрайд обзора
    if current_building != state.active_building || player.current_floor != state.player_floor {
        state.manual_floor_override = None;
    }

    state.active_building = current_building;
    state.player_floor = player.current_floor;

    // Позволяем игроку вручную переключить просматриваемый этаж внутри многоэтажного дома (Tab / PageUp / PageDown)
    if let Some(b_id) = state.active_building {
        if let Some(b_def) = registry.get(b_id) {
            let max_floor = b_def.max_floor();
            if max_floor > 0 {
                if keyboard_input.just_pressed(KeyCode::Tab) {
                    let next_floor = (state.viewed_floor + 1) % (max_floor + 1);
                    state.manual_floor_override = if next_floor == state.player_floor {
                        None
                    } else {
                        Some(next_floor)
                    };
                } else if keyboard_input.just_pressed(KeyCode::PageUp) {
                    let next_floor = (state.viewed_floor + 1).min(max_floor);
                    state.manual_floor_override = Some(next_floor);
                } else if keyboard_input.just_pressed(KeyCode::PageDown) {
                    let next_floor = (state.viewed_floor - 1).max(0);
                    state.manual_floor_override = Some(next_floor);
                }
            }
        }
    }

    state.viewed_floor = state.manual_floor_override.unwrap_or(state.player_floor);
}

/// Система управления видимостью этажей, крыш и внешних оболочек зданий
pub fn update_building_floor_visibility_system(
    registry: Res<BuildingRegistry>,
    state: Res<ActiveBuildingState>,
    mut floor_roots: Query<(&BuildingFloorRoot, &mut Visibility)>,
    mut span_floors: Query<
        (&BuildingSpanFloors, &mut Visibility),
        Without<BuildingFloorRoot>,
    >,
    mut roofs: Query<
        (&BuildingRoof, &mut Visibility),
        (Without<BuildingFloorRoot>, Without<BuildingSpanFloors>),
    >,
    mut shells: Query<
        (&BuildingExteriorShell, &mut Visibility),
        (
            Without<BuildingFloorRoot>,
            Without<BuildingSpanFloors>,
            Without<BuildingRoof>,
        ),
    >,
) {
    // 1. Крыши зданий: скрываем, когда персонаж внутри этого здания
    for (roof, mut vis) in &mut roofs {
        let is_inside = state.active_building == Some(roof.building_id);
        let desired = if is_inside {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *vis != desired {
            *vis = desired;
        }
    }

    // 2. Внешние монолитные оболочки домов улицы: видимы снаружи, скрыты внутри
    for (shell, mut vis) in &mut shells {
        let is_inside = state.active_building == Some(shell.building_id);
        let desired = if is_inside {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *vis != desired {
            *vis = desired;
        }
    }

    // 3. Этажи зданий: когда внутри — скрываем неактивные этажи!
    for (floor_root, mut vis) in &mut floor_roots {
        let is_inside = state.active_building == Some(floor_root.building_id);
        let has_shell = registry
            .get(floor_root.building_id)
            .map(|b| b.has_exterior_shell)
            .unwrap_or(false);

        let should_show = if is_inside {
            match floor_root.rule {
                FloorVisibilityRule::OnlyWhenActive => {
                    floor_root.floor_index == state.viewed_floor
                }
                FloorVisibilityRule::ActiveOrBelow => {
                    floor_root.floor_index <= state.viewed_floor
                }
            }
        } else {
            !has_shell
        };

        let desired = if should_show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != desired {
            *vis = desired;
        }
    }

    // 4. Межэтажные объекты (лестницы): видимы при нахождении на любом из связываемых этажей
    for (span, mut vis) in &mut span_floors {
        let is_inside = state.active_building == Some(span.building_id);
        let has_shell = registry
            .get(span.building_id)
            .map(|b| b.has_exterior_shell)
            .unwrap_or(false);

        let should_show = if is_inside {
            state.viewed_floor >= span.min_floor && state.viewed_floor <= span.max_floor
        } else {
            !has_shell && span.min_floor == 0
        };

        let desired = if should_show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != desired {
            *vis = desired;
        }
    }
}

/// Вспомогательная функция: мгновенно переключает все дочерние меши объекта между непрозрачным и полупрозрачным материалом
fn apply_entity_transparency(
    root_entity: Entity,
    transparent: bool,
    children_query: &Query<&Children>,
    mesh_materials: &mut Query<(&mut MeshMaterial3d<StandardMaterial>, Option<&OriginalMaterial>)>,
    materials: &mut Assets<StandardMaterial>,
    cache: &mut CutawayMaterialCache,
    commands: &mut Commands,
) {
    for descendant in children_query.iter_descendants(root_entity) {
        let Ok((mut mat_comp, orig_opt)) = mesh_materials.get_mut(descendant) else {
            continue;
        };

        let orig_handle = if let Some(orig) = orig_opt {
            orig.0.clone()
        } else {
            let h = mat_comp.0.clone();
            commands.entity(descendant).insert(OriginalMaterial(h.clone()));
            h
        };

        if transparent {
            let orig_id = orig_handle.id();
            let trans_handle = if let Some(cached) = cache.transparent_by_opaque.get(&orig_id) {
                cached.clone()
            } else if let Some(orig_mat) = materials.get(&orig_handle).cloned() {
                let mut trans_mat = orig_mat;
                trans_mat.base_color = trans_mat.base_color.with_alpha(CUTAWAY_ALPHA);
                trans_mat.alpha_mode = AlphaMode::Blend;
                trans_mat.double_sided = true;
                let handle = materials.add(trans_mat);
                cache.transparent_by_opaque.insert(orig_id, handle.clone());
                handle
            } else {
                continue;
            };

            if mat_comp.0 != trans_handle {
                mat_comp.0 = trans_handle;
            }
        } else if mat_comp.0 != orig_handle {
            mat_comp.0 = orig_handle;
        }
    }
}

/// Система мгновенного переключения прозрачности стен, колонн и высокой пристенной мебели,
/// перекрывающих обзор камере (без анимации опускания по высоте)
#[allow(clippy::too_many_arguments)]
pub fn update_wall_cutaway_system(
    mut commands: Commands,
    state: Res<ActiveBuildingState>,
    mut cache: ResMut<CutawayMaterialCache>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    player_query: Query<&Transform, With<Player>>,
    camera_query: Query<(&Transform, &CameraController), (With<Camera3d>, Without<Player>)>,
    walls: Query<(Entity, &CutawayWall)>,
    pillars: Query<(Entity, &CutawayPillar)>,
    attached_props: Query<(Entity, &CutawayAttachedProp)>,
    children_query: Query<&Children>,
    mut mesh_materials: Query<(&mut MeshMaterial3d<StandardMaterial>, Option<&OriginalMaterial>)>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let Ok((cam_tf, _cam_ctrl)) = camera_query.single() else {
        return;
    };
    let Some(materials) = materials.as_deref_mut() else {
        return;
    };

    let player_xz = Vec2::new(player_tf.translation.x, player_tf.translation.z);
    let cam_xz = Vec2::new(cam_tf.translation.x, cam_tf.translation.z);
    // Вектор в горизонтальной плоскости XZ от персонажа к камере
    let to_camera_xz = (cam_xz - player_xz).normalize_or_zero();

    // 1. Переключаем прозрачность стен (внешних и внутренних перегородок)
    for (entity, wall) in &walls {
        let is_active_floor = state.active_building == Some(wall.building_id)
            && state.viewed_floor == wall.floor_index;

        let should_cut = if is_active_floor {
            if !wall.is_interior_partition {
                wall.outward_normal.dot(to_camera_xz) > 0.22
            } else {
                let rel_to_player = wall.world_xz - player_xz;
                let in_front_of_player = rel_to_player.dot(to_camera_xz) > -0.6;
                let faces_camera = wall.outward_normal.dot(to_camera_xz).abs() > 0.25;
                in_front_of_player && faces_camera
            }
        } else {
            false
        };

        apply_entity_transparency(
            entity,
            should_cut,
            &children_query,
            &mut mesh_materials,
            materials,
            &mut cache,
            &mut commands,
        );
    }

    // 2. Переключаем прозрачность колонн
    for (entity, pillar) in &pillars {
        let is_active_floor = state.active_building == Some(pillar.building_id)
            && state.viewed_floor == pillar.floor_index;

        let should_cut = if is_active_floor {
            if !pillar.adjacent_normals.is_empty() {
                pillar
                    .adjacent_normals
                    .iter()
                    .any(|n| n.dot(to_camera_xz) > 0.22)
            } else {
                let rel = pillar.world_xz - player_xz;
                let along_cam = rel.dot(to_camera_xz);
                let lateral = (rel - to_camera_xz * along_cam).length();
                along_cam > 0.2 && lateral < 4.5
            }
        } else {
            false
        };

        apply_entity_transparency(
            entity,
            should_cut,
            &children_query,
            &mut mesh_materials,
            materials,
            &mut cache,
            &mut commands,
        );
    }

    // 3. Переключаем прозрачность высокой пристенной мебели у прозрачных стен
    for (entity, prop) in &attached_props {
        let is_active_floor = state.active_building == Some(prop.building_id)
            && state.viewed_floor == prop.floor_index;

        let should_cut = is_active_floor && prop.wall_normal.dot(to_camera_xz) > 0.22;

        apply_entity_transparency(
            entity,
            should_cut,
            &children_query,
            &mut mesh_materials,
            materials,
            &mut cache,
            &mut commands,
        );
    }
}

/// Обновление информационной панели в углу экрана (название здания и активный этаж)
pub fn update_building_hud_system(
    registry: Res<BuildingRegistry>,
    state: Res<ActiveBuildingState>,
    mut text_query: Query<&mut Text, With<BuildingHudText>>,
) {
    if !state.is_changed() {
        return;
    }
    let Ok(mut text) = text_query.single_mut() else {
        return;
    };

    if let Some(b_id) = state.active_building {
        if let Some(def) = registry.get(b_id) {
            let total_floors = def.max_floor() + 1;
            let current_display = state.viewed_floor + 1;
            if state.manual_floor_override.is_some() {
                **text = format!(
                    "{} — Обзор {}/{} эт. (Игрок на {} эт.) | [Tab] Сменить этаж",
                    def.name,
                    current_display,
                    total_floors,
                    state.player_floor + 1
                );
            } else if total_floors > 1 {
                **text = format!(
                    "{} — Этаж {}/{} | [Tab] Обзор другого этажа",
                    def.name, current_display, total_floors
                );
            } else {
                **text = format!("{} — Внутри помещения", def.name);
            }
            return;
        }
    }

    **text = "Улицы Аркхэма (Снаружи) | Зайдите в Замок, Гостиницу или Особняк".to_string();
}

/// Переиспользуемый конструктор межэтажной лестницы (создает 3D-ступени, перила, коллайдеры и навигационный линк)
pub fn spawn_staircase(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    nav_world: &mut NavWorld,
    building_id: BuildingId,
    stair: StairLink,
    step_count: usize,
) {
    let axis = stair.upper_entry - stair.lower_entry;
    let total_len = axis.length();
    let dir_2d = axis / total_len;
    let perp_2d = Vec2::new(-dir_2d.y, dir_2d.x);
    let yaw = (-dir_2d.x).atan2(-dir_2d.y);

    let step_depth = total_len / step_count as f32;
    let total_height = stair.upper_y - stair.lower_y;
    let step_height = total_height / step_count as f32;
    let width = stair.half_width * 2.0;

    let step_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.26, 0.24, 0.22),
        perceptual_roughness: 0.88,
        ..default()
    });
    let rail_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.14, 0.12),
        perceptual_roughness: 0.75,
        ..default()
    });

    let step_mesh = meshes.add(Cuboid::new(width, step_height * 1.05, step_depth * 1.08));
    let post_mesh = meshes.add(Cuboid::new(0.16, 0.95, 0.16));

    commands
        .spawn((
            BuildingSpanFloors {
                building_id,
                min_floor: stair.lower_floor,
                max_floor: stair.upper_floor,
            },
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|parent| {
            for i in 0..step_count {
                let t = (i as f32 + 0.5) / step_count as f32;
                let pos_2d = stair.lower_entry.lerp(stair.upper_entry, t);
                let y = stair.lower_y + (i as f32 + 0.5) * step_height;

                parent.spawn((
                    Mesh3d(step_mesh.clone()),
                    MeshMaterial3d(step_mat.clone()),
                    Transform::from_xyz(pos_2d.x, y, pos_2d.y)
                        .with_rotation(Quat::from_rotation_y(yaw)),
                ));

                // Стойки балюстрады вдоль лестницы
                if i % 3 == 0 || i == step_count - 1 {
                    for side in [-1.0_f32, 1.0_f32] {
                        let rail_xz = pos_2d + perp_2d * (stair.half_width - 0.08) * side;
                        parent.spawn((
                            Mesh3d(post_mesh.clone()),
                            MeshMaterial3d(rail_mat.clone()),
                            Transform::from_xyz(rail_xz.x, y + step_height * 0.5 + 0.45, rail_xz.y)
                                .with_rotation(Quat::from_rotation_y(yaw)),
                        ));
                    }
                }
            }
        });

    // Физические коллайдеры боковых перил вдоль марша (чуть отступая от входов, чтобы заход под углом был свободным)
    let rail_segments = 4;
    for i in 0..rail_segments {
        let t = 0.18 + (i as f32 / (rail_segments - 1) as f32) * 0.64;
        let center_2d = stair.lower_entry.lerp(stair.upper_entry, t);
        for side in [-1.0_f32, 1.0_f32] {
            let side_pt = center_2d + perp_2d * (stair.half_width + 0.22) * side;
            nav_world.add_box_on_floor(side_pt.x, side_pt.y, 0.18, 0.18, stair.lower_floor);
            nav_world.add_box_on_floor(side_pt.x, side_pt.y, 0.18, 0.18, stair.upper_floor);
        }
    }

    // На нижнем этаже блокируем проход под верхним концом лестницы (с обратной стороны)
    let back_lower = stair.upper_entry + dir_2d * 0.65;
    nav_world.add_box_on_floor(back_lower.x, back_lower.y, 0.28, 0.28, stair.lower_floor);

    // На верхнем этаже ограждаем проем над нижним концом лестницы (чтобы не упасть в проем)
    let back_upper = stair.lower_entry - dir_2d * 0.65;
    nav_world.add_box_on_floor(back_upper.x, back_upper.y, 0.28, 0.28, stair.upper_floor);

    nav_world.add_stair_link(stair);
}
