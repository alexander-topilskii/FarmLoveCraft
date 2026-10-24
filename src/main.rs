use bevy::prelude::*;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::cmp::Ordering;

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

const PLAYER_RADIUS: f32 = 0.34;
const NAV_CELL_SIZE: f32 = 0.25;
const NAV_HALF_EXTENT: i32 = 220; // [-55m .. +55m]

/// Физический 2D-коллайдер (AABB в плоскости XZ) для твердых объектов сцены
#[derive(Clone, Copy, Debug)]
struct ColliderRect {
    center: Vec2,
    half_extents: Vec2,
}

impl ColliderRect {
    fn new(x: f32, z: f32, half_x: f32, half_z: f32) -> Self {
        Self {
            center: Vec2::new(x, z),
            half_extents: Vec2::new(half_x, half_z),
        }
    }

    /// Проверяет пересечение с кругом (игроком) заданного радиуса
    fn intersects_circle(&self, pos: Vec2, radius: f32) -> bool {
        let closest = pos.clamp(self.center - self.half_extents, self.center + self.half_extents);
        pos.distance_squared(closest) < radius * radius
    }

    /// Выталкивает круг из прямоугольного коллайдера (sliding collision resolution)
    fn resolve_circle(&self, pos: Vec2, radius: f32) -> Vec2 {
        let min = self.center - self.half_extents;
        let max = self.center + self.half_extents;
        let closest = pos.clamp(min, max);
        let diff = pos - closest;
        let dist_sq = diff.length_squared();

        if dist_sq >= radius * radius {
            return pos;
        }

        if dist_sq > 1e-6 {
            let dist = dist_sq.sqrt();
            return pos + (diff / dist) * (radius - dist);
        }

        // Центр круга попал внутрь AABB — выталкиваем к ближайшей грани
        let dx_left = (pos.x - min.x).abs();
        let dx_right = (max.x - pos.x).abs();
        let dz_top = (pos.y - min.y).abs();
        let dz_bottom = (max.y - pos.y).abs();
        let min_pen = dx_left.min(dx_right).min(dz_top).min(dz_bottom);

        if (min_pen - dx_left).abs() < 1e-4 {
            Vec2::new(min.x - radius, pos.y)
        } else if (min_pen - dx_right).abs() < 1e-4 {
            Vec2::new(max.x + radius, pos.y)
        } else if (min_pen - dz_top).abs() < 1e-4 {
            Vec2::new(pos.x, min.y - radius)
        } else {
            Vec2::new(pos.x, max.y + radius)
        }
    }
}

/// Ресурс физического мира и навигационной сетки для A* поиска пути
#[derive(Resource, Default)]
struct NavWorld {
    colliders: Vec<ColliderRect>,
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct AStarNode {
    cell: IVec2,
    f_cost: i32,
}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f_cost.cmp(&self.f_cost)
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl NavWorld {
    fn add_box(&mut self, x: f32, z: f32, half_x: f32, half_z: f32) {
        self.colliders.push(ColliderRect::new(x, z, half_x, half_z));
    }

    /// Добавляет готическую арку как два боковых физических пилона (оставляя проход в центре шириной 1.7м открытым!)
    fn add_arch(&mut self, x: f32, z: f32, rotated_90: bool) {
        if !rotated_90 {
            self.add_box(x - 1.42, z, 0.58, 0.30);
            self.add_box(x + 1.42, z, 0.58, 0.30);
        } else {
            self.add_box(x, z - 1.42, 0.30, 0.58);
            self.add_box(x, z + 1.42, 0.30, 0.58);
        }
    }

    fn is_walkable_pos(&self, pos: Vec2, radius: f32) -> bool {
        !self.colliders.iter().any(|c| c.intersects_circle(pos, radius))
    }

    fn resolve_collisions(&self, mut pos: Vec2, radius: f32) -> Vec2 {
        for _ in 0..3 {
            for col in &self.colliders {
                pos = col.resolve_circle(pos, radius);
            }
        }
        pos
    }

    fn world_to_cell(pos: Vec2) -> IVec2 {
        IVec2::new(
            (pos.x / NAV_CELL_SIZE).round() as i32,
            (pos.y / NAV_CELL_SIZE).round() as i32,
        )
    }

    fn cell_to_world(cell: IVec2) -> Vec2 {
        Vec2::new(cell.x as f32 * NAV_CELL_SIZE, cell.y as f32 * NAV_CELL_SIZE)
    }

    fn is_cell_walkable(&self, cell: IVec2) -> bool {
        if cell.x.abs() > NAV_HALF_EXTENT || cell.y.abs() > NAV_HALF_EXTENT {
            return false;
        }
        self.is_walkable_pos(Self::cell_to_world(cell), PLAYER_RADIUS + 0.03)
    }

    /// Если кликнули прямо внутрь твердого объекта, находим ближайшую свободную ячейку рядом с ним
    fn find_nearest_walkable(&self, origin: IVec2) -> Option<IVec2> {
        if self.is_cell_walkable(origin) {
            return Some(origin);
        }
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back(origin);
        visited.insert(origin);

        let dirs = [
            IVec2::new(1, 0), IVec2::new(-1, 0), IVec2::new(0, 1), IVec2::new(0, -1),
            IVec2::new(1, 1), IVec2::new(-1, 1), IVec2::new(1, -1), IVec2::new(-1, -1),
        ];

        while let Some(curr) = queue.pop_front() {
            if (curr - origin).length_squared() > 400 {
                break;
            }
            for d in dirs {
                let next = curr + d;
                if visited.insert(next) {
                    if self.is_cell_walkable(next) {
                        return Some(next);
                    }
                    queue.push_back(next);
                }
            }
        }
        None
    }

    /// Проверяет прямую видимость между двумя точками с учетом радиуса игрока (для сглаживания пути)
    fn has_line_of_sight(&self, a: Vec2, b: Vec2) -> bool {
        let dist = a.distance(b);
        let steps = ((dist / (NAV_CELL_SIZE * 0.45)).ceil() as i32).max(1);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let p = a.lerp(b, t);
            if !self.is_walkable_pos(p, PLAYER_RADIUS + 0.02) {
                return false;
            }
        }
        true
    }

    /// Алгоритм A* с octile-эвристикой, защитой от срезания углов и сглаживанием пути (string pulling)
    fn find_path(&self, start_world: Vec2, goal_world: Vec2) -> Vec<Vec3> {
        let Some(start_cell) = self.find_nearest_walkable(Self::world_to_cell(start_world)) else {
            return Vec::new();
        };
        let Some(goal_cell) = self.find_nearest_walkable(Self::world_to_cell(goal_world)) else {
            return Vec::new();
        };

        if start_cell == goal_cell {
            let final_pt = Self::cell_to_world(goal_cell);
            return vec![Vec3::new(final_pt.x, 0.0, final_pt.y)];
        }

        let heuristic = |a: IVec2, b: IVec2| -> i32 {
            let dx = (a.x - b.x).abs();
            let dy = (a.y - b.y).abs();
            10 * (dx + dy) - 6 * dx.min(dy)
        };

        let mut open = BinaryHeap::new();
        let mut g_score: HashMap<IVec2, i32> = HashMap::new();
        let mut came_from: HashMap<IVec2, IVec2> = HashMap::new();

        g_score.insert(start_cell, 0);
        open.push(AStarNode {
            cell: start_cell,
            f_cost: heuristic(start_cell, goal_cell),
        });

        let neighbors = [
            (IVec2::new(1, 0), 10),
            (IVec2::new(-1, 0), 10),
            (IVec2::new(0, 1), 10),
            (IVec2::new(0, -1), 10),
            (IVec2::new(1, 1), 14),
            (IVec2::new(-1, 1), 14),
            (IVec2::new(1, -1), 14),
            (IVec2::new(-1, -1), 14),
        ];

        let mut found = false;
        let mut iterations = 0;

        while let Some(current) = open.pop() {
            if current.cell == goal_cell {
                found = true;
                break;
            }

            iterations += 1;
            if iterations > 16_000 {
                break;
            }

            let current_g = *g_score.get(&current.cell).unwrap_or(&i32::MAX);

            for (offset, move_cost) in neighbors {
                let next = current.cell + offset;
                if !self.is_cell_walkable(next) {
                    continue;
                }
                // Запрещаем срезать углы по диагонали сквозь стыки стен
                if offset.x != 0 && offset.y != 0 {
                    if !self.is_cell_walkable(IVec2::new(current.cell.x + offset.x, current.cell.y))
                        || !self.is_cell_walkable(IVec2::new(current.cell.x, current.cell.y + offset.y))
                    {
                        continue;
                    }
                }

                let tentative_g = current_g + move_cost;
                if tentative_g < *g_score.get(&next).unwrap_or(&i32::MAX) {
                    came_from.insert(next, current.cell);
                    g_score.insert(next, tentative_g);
                    open.push(AStarNode {
                        cell: next,
                        f_cost: tentative_g + heuristic(next, goal_cell),
                    });
                }
            }
        }

        if !found {
            return Vec::new();
        }

        // Восстанавливаем цепочку ячеек
        let mut raw_pts = Vec::new();
        let mut curr = goal_cell;
        while curr != start_cell {
            raw_pts.push(Self::cell_to_world(curr));
            curr = came_from[&curr];
        }
        raw_pts.push(start_world);
        raw_pts.reverse();

        // Если кликнули в заведомо свободную точку, последнюю точку ставим точно в место клика
        let exact_goal = if self.is_walkable_pos(goal_world, PLAYER_RADIUS + 0.03) {
            goal_world
        } else {
            Self::cell_to_world(goal_cell)
        };
        if let Some(last) = raw_pts.last_mut() {
            *last = exact_goal;
        }

        // Сглаживание пути (Line-of-Sight String Pulling), чтобы сыщик шел плавно, а не зигзагами по клеткам
        let mut smooth_path = Vec::new();
        let mut anchor_idx = 0;
        while anchor_idx < raw_pts.len() - 1 {
            let mut furthest_idx = anchor_idx + 1;
            for test_idx in (anchor_idx + 2)..raw_pts.len() {
                if self.has_line_of_sight(raw_pts[anchor_idx], raw_pts[test_idx]) {
                    furthest_idx = test_idx;
                }
            }
            let wp = raw_pts[furthest_idx];
            smooth_path.push(Vec3::new(wp.x, 0.0, wp.y));
            anchor_idx = furthest_idx;
        }

        smooth_path
    }
}

#[derive(Component)]
struct Player {
    speed: f32,
    path: VecDeque<Vec3>,
    keyboard_vel: Vec3,
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
    mut nav_world: ResMut<NavWorld>,
) {
    // Земля фермы и города Аркхэма (мрачно-зеленая лавкрафтовская почва)
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(140.0, 140.0))),
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
    let fireplace_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_fireplace.glb"));
    let alchemy_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_alchemy.glb"));
    let clock_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/castle/castle_clock.glb"));

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
        nav_world.add_box(wx, -6.0, 2.0, 0.28);
    }
    commands.spawn((
        WorldAssetRoot(arch_scene.clone()),
        Transform::from_xyz(0.0, 0.0, -6.0),
    ));
    nav_world.add_arch(0.0, -6.0, false);

    // Западная и Восточная боковые стены
    let rot_90 = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    for wz in [-4.0, 0.0, 4.0] {
        commands.spawn((
            WorldAssetRoot(wall_scene.clone()),
            Transform::from_xyz(-6.0, 0.0, wz).with_rotation(rot_90),
        ));
        nav_world.add_box(-6.0, wz, 0.28, 2.0);
    }
    for wz in [-4.0, 4.0] {
        commands.spawn((
            WorldAssetRoot(wall_scene.clone()),
            Transform::from_xyz(6.0, 0.0, wz).with_rotation(rot_90),
        ));
        nav_world.add_box(6.0, wz, 0.28, 2.0);
    }
    // Восточный портал-арка (выход к ферме/реке)
    commands.spawn((
        WorldAssetRoot(arch_scene.clone()),
        Transform::from_xyz(6.0, 0.0, 0.0).with_rotation(rot_90),
    ));
    nav_world.add_arch(6.0, 0.0, true);

    // Южный вход (две арки и колонны по углам)
    commands.spawn((
        WorldAssetRoot(arch_scene),
        Transform::from_xyz(0.0, 0.0, 6.0),
    ));
    nav_world.add_arch(0.0, 6.0, false);

    // 3. Рунические колонны по углам и вдоль проходов
    for (px, pz) in [
        (-6.0, -6.0), (-2.0, -6.0), (2.0, -6.0), (6.0, -6.0),
        (-6.0,  6.0), (-2.0,  6.0), (2.0,  6.0), (6.0,  6.0),
    ] {
        commands.spawn((
            WorldAssetRoot(pillar_scene.clone()),
            Transform::from_xyz(px, 0.0, pz),
        ));
        nav_world.add_box(px, pz, 0.38, 0.38);
    }

    // 4. Внутреннее убранство: Ритуальный ковер в центре зала (проходимый, без коллайдера)
    commands.spawn((
        WorldAssetRoot(rug_scene),
        Transform::from_xyz(0.0, 0.08, -0.5),
    ));

    // 5. Массивный стол исследователя с фолиантом и свечой + готические кресла
    commands.spawn((
        WorldAssetRoot(table_scene),
        Transform::from_xyz(-2.8, 0.08, -1.5),
    ));
    nav_world.add_box(-2.8, -1.5, 1.12, 0.58);

    commands.spawn((
        WorldAssetRoot(chair_scene.clone()),
        Transform::from_xyz(-2.8, 0.08, -2.5)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
    ));
    nav_world.add_box(-2.8, -2.5, 0.32, 0.32);

    commands.spawn((
        WorldAssetRoot(chair_scene),
        Transform::from_xyz(-4.2, 0.08, -1.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    nav_world.add_box(-4.2, -1.5, 0.32, 0.32);

    // 6. Книжные шкафы с запретными томами вдоль северной и западной стен
    for bx in [-4.0, 4.0] {
        commands.spawn((
            WorldAssetRoot(bookcase_scene.clone()),
            Transform::from_xyz(bx, 0.08, -5.4)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        ));
        nav_world.add_box(bx, -5.4, 0.92, 0.28);
    }
    commands.spawn((
        WorldAssetRoot(bookcase_scene),
        Transform::from_xyz(-5.4, 0.08, 2.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    nav_world.add_box(-5.4, 2.5, 0.28, 0.92);

    // 7. Оккультный алтарь Древних в восточном крыле зала + изумрудное свечение
    commands.spawn((
        WorldAssetRoot(altar_scene),
        Transform::from_xyz(3.5, 0.08, -1.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    nav_world.add_box(3.5, -1.5, 0.72, 0.72);
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
        nav_world.add_box(cx, cz, 0.28, 0.28);
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

    // 8b. Готический камин с тлеющими углями у западной стены + теплое свечение очага
    commands.spawn((
        WorldAssetRoot(fireplace_scene),
        Transform::from_xyz(-5.3, 0.08, -0.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    nav_world.add_box(-5.3, -0.5, 0.52, 1.25);
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.52, 0.18),
            intensity: 65_000.0,
            range: 10.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-4.6, 0.65, -0.5),
    ));

    // 8c. Алхимическая лаборатория с перегонным кубом и банкой со щупальцем в юго-восточном крыле
    commands.spawn((
        WorldAssetRoot(alchemy_scene),
        Transform::from_xyz(3.5, 0.08, 3.2),
    ));
    nav_world.add_box(3.5, 3.2, 1.05, 0.50);

    // 8d. Старинные напольные часы с маятником и лунным циферблатом у северной стены
    commands.spawn((
        WorldAssetRoot(clock_scene),
        Transform::from_xyz(1.9, 0.08, -5.3)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
    ));
    nav_world.add_box(1.9, -5.3, 0.38, 0.30);

    // =========================================================================
    // УЛИЦА / ПОМЕСТЬЕ, КАНАЛ ЩУПАЛЕЦ И АУТЕНТИЧНЫЙ ГОРОД АРКХЭМ / ИННСМУТ
    // =========================================================================
    let bench_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_bench.glb"));
    let lamppost_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_lamppost.glb"));
    let tree_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_tree.glb"));
    let bush_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_bush.glb"));
    let fence_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_fence.glb"));
    let gravestone_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_gravestone.glb"));
    let well_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_well.glb"));
    let path_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_path.glb"));
    let gazebo_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_gazebo.glb"));
    let cart_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_cart.glb"));
    let townhouse_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_townhouse.glb"));
    let shop_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_shop.glb"));
    let fountain_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_fountain.glb"));
    let gate_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_gate.glb"));
    let kiosk_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_kiosk.glb"));
    let crypt_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_crypt.glb"));
    let bridge_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_bridge.glb"));
    let canal_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_canal.glb"));
    let church_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_church.glb"));
    let university_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_university.glb"));
    let inn_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_inn.glb"));
    let clocktower_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_clocktower.glb"));
    let carriage_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_carriage.glb"));
    let warehouse_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_warehouse.glb"));
    let phonebooth_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/outdoor/outdoor_phonebooth.glb"));

    // 9. Брусчатые проспекты, перекрестки и площади города Аркхэма (проходимые, без коллайдеров)
    // Гранд-Авеню с севера на юг: от замка (Z=8) через каменный мост и площадь фонтана до Часовой Башни и Собора (Z=42)
    for pz in [8.0, 10.5, 13.0, 19.5, 22.0, 27.0, 29.5, 32.0, 34.5, 39.5, 42.0] {
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(0.0, 0.0, pz),
        ));
    }
    // Первая поперечная улица (Рыночная / Купеческая, Z = 24.5, X: -22.5..+22.5)
    for px in [-22.5, -20.0, -17.5, -15.0, -12.5, -10.0, -7.5, -5.0, -2.5, 2.5, 5.0, 7.5, 10.0, 12.5, 15.0, 17.5, 20.0, 22.5] {
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(px, 0.0, 24.5),
        ));
    }
    // Вторая поперечная улица (Университетский Бульвар / Церковный проспект, Z = 37.0, X: -20..+20)
    for px in [-20.0, -17.5, -15.0, -12.5, -10.0, -7.5, -5.0, -2.5, 2.5, 5.0, 7.5, 10.0, 12.5, 15.0, 17.5, 20.0] {
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(px, 0.0, 37.0),
        ));
    }
    // Боковые соединительные переулки (Западный и Восточный, X = -19.5 и +19.5, Z: 27..34.5)
    for sx in [-19.5, 19.5] {
        for pz in [27.0, 29.5, 32.0, 34.5] {
            commands.spawn((
                WorldAssetRoot(path_scene.clone()),
                Transform::from_xyz(sx, 0.0, pz),
            ));
        }
    }
    // Площади вокруг Фонтана (Z=24.5) и Часовой Башни (Z=37.0)
    for px in [-2.5, 2.5] {
        for pz in [22.0, 27.0, 34.5, 39.5] {
            commands.spawn((
                WorldAssetRoot(path_scene.clone()),
                Transform::from_xyz(px, 0.0, pz),
            ));
        }
    }
    // Восточная тропа к колодцу и ферме (X: 8..13, Z = 0)
    for px in [8.0, 10.5, 13.0] {
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(px, 0.0, 0.0).with_rotation(rot_90),
        ));
    }

    // 10. Монументальные кованые ворота поместья (Z = 6.0, X = -4.0 и +4.0 — с открытым проходом по центру)
    for gx in [-4.0, 4.0] {
        commands.spawn((
            WorldAssetRoot(gate_scene.clone()),
            Transform::from_xyz(gx, 0.0, 6.0),
        ));
        nav_world.add_box(gx - 1.48, 6.0, 0.45, 0.45);
        nav_world.add_box(gx + 1.48, 6.0, 0.45, 0.45);
    }

    // 11. Проточный каменный канал со светящимися щупальцами (Z = 16.2) и Два Горбатых Моста (X = 0.0 и X = 19.5)
    for bx in [0.0, 19.5] {
        commands.spawn((
            WorldAssetRoot(bridge_scene.clone()),
            Transform::from_xyz(bx, 0.0, 16.2),
        ));
        nav_world.add_box(bx - 1.08, 16.2, 0.22, 1.75);
        nav_world.add_box(bx + 1.08, 16.2, 0.22, 1.75);
    }

    for cx in [-17.2, -13.2, -9.2, -5.2, 5.2, 9.2, 13.2, 16.5, 23.0] {
        commands.spawn((
            WorldAssetRoot(canal_scene.clone()),
            Transform::from_xyz(cx, 0.0, 16.2),
        ));
        nav_world.add_box(cx, 16.2, 1.95, 1.32);
    }

    // 12. Доковых склады «Рыбопромышленной компании Марша» (Innsmouth Marsh Warehouse) на берегу канала
    commands.spawn((
        WorldAssetRoot(warehouse_scene.clone()),
        Transform::from_xyz(23.8, 0.0, 20.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    nav_world.add_box(23.8, 20.5, 1.95, 2.65);

    commands.spawn((
        WorldAssetRoot(warehouse_scene),
        Transform::from_xyz(-23.8, 0.0, 20.5).with_rotation(rot_90),
    ));
    nav_world.add_box(-23.8, 20.5, 1.95, 2.65);

    // 13. Первая городская площадь: Оккультный фонтан на перекрестке (X = 0, Z = 24.5)
    commands.spawn((
        WorldAssetRoot(fountain_scene),
        Transform::from_xyz(0.0, 0.0, 24.5),
    ));
    nav_world.add_box(0.0, 24.5, 1.62, 1.62);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.22, 0.95, 0.62),
            intensity: 60_000.0,
            range: 12.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 2.4, 24.5),
    ));

    // 14. Купеческий и Жилой Квартал вдоль Первой поперечной улицы (Z = 19.8 и Z = 30.8)
    // Северная сторона улицы (Z = 19.8, фасадами на юг -> поворот PI)
    for hx in [-14.5, 14.5] {
        commands.spawn((
            WorldAssetRoot(townhouse_scene.clone()),
            Transform::from_xyz(hx, 0.0, 19.8)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        ));
        nav_world.add_box(hx, 19.8, 2.38, 1.98);
    }
    for sx in [-9.2, 9.2] {
        commands.spawn((
            WorldAssetRoot(shop_scene.clone()),
            Transform::from_xyz(sx, 0.0, 19.8)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        ));
        nav_world.add_box(sx, 19.8, 2.25, 1.85);
    }

    // Южная сторона Первой улицы (Z = 30.8, фасадами на север -> поворот 0.0):
    // Гостиница «Gilman House Inn», Особняки и Оккультные Лавки
    commands.spawn((
        WorldAssetRoot(inn_scene.clone()),
        Transform::from_xyz(-6.2, 0.0, 30.8),
    ));
    nav_world.add_box(-6.2, 30.8, 2.55, 2.15);

    commands.spawn((
        WorldAssetRoot(inn_scene),
        Transform::from_xyz(6.2, 0.0, 30.8),
    ));
    nav_world.add_box(6.2, 30.8, 2.55, 2.15);

    for hx in [-15.2, 15.2] {
        commands.spawn((
            WorldAssetRoot(townhouse_scene.clone()),
            Transform::from_xyz(hx, 0.0, 30.8),
        ));
        nav_world.add_box(hx, 30.8, 2.38, 1.98);
    }
    for sx in [-10.8, 10.8] {
        commands.spawn((
            WorldAssetRoot(shop_scene.clone()),
            Transform::from_xyz(sx, 0.0, 30.8),
        ));
        nav_world.add_box(sx, 30.8, 2.25, 1.85);
    }

    // 15. Монументальный Южный Проспект (Z = 37.0..44.5):
    // Городская Часовая Башня Аркхэма в центре южного перекрестка (X = 0.0, Z = 37.0)
    commands.spawn((
        WorldAssetRoot(clocktower_scene),
        Transform::from_xyz(0.0, 0.0, 37.0),
    ));
    nav_world.add_box(0.0, 37.0, 1.42, 1.42);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.95, 0.82, 0.42),
            intensity: 65_000.0,
            range: 14.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 6.5, 37.0),
    ));

    // Готический Собор «Тайного Ордена Дагона» (замыкает перспективу главной оси на юге: X = 0.0, Z = 45.2)
    commands.spawn((
        WorldAssetRoot(church_scene),
        Transform::from_xyz(0.0, 0.0, 45.2),
    ));
    nav_world.add_box(0.0, 45.2, 2.75, 3.25);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.65, 0.25, 0.85),
            intensity: 75_000.0,
            range: 15.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 4.2, 41.5),
    ));

    // Корпус Библиотеки и Обсерватории Мискатоникского Университета (слева и справа от Собора: X = -9.5 и +9.5, Z = 44.0)
    for ux in [-9.5, 9.5] {
        commands.spawn((
            WorldAssetRoot(university_scene.clone()),
            Transform::from_xyz(ux, 0.0, 44.0),
        ));
        nav_world.add_box(ux, 44.0, 3.25, 2.35);
    }

    // Боковые особняки на Университетском бульваре (X = -17.0 и +17.0, Z = 43.5)
    for hx in [-17.0, 17.0] {
        commands.spawn((
            WorldAssetRoot(townhouse_scene.clone()),
            Transform::from_xyz(hx, 0.0, 43.5),
        ));
        nav_world.add_box(hx, 43.5, 2.38, 1.98);
    }

    // 16. Винтажные автомобили-катафалки 1920-х годов, припаркованные на улицах Аркхэма
    let carriage_positions = [
        (-5.8, 23.2, std::f32::consts::FRAC_PI_2, 1.75, 0.85),
        ( 8.2, 35.8, -std::f32::consts::FRAC_PI_2, 1.75, 0.85),
        (-2.2, 40.5, 0.15, 0.85, 1.75),
    ];
    for (cx, cz, cyaw, hx, hz) in carriage_positions {
        commands.spawn((
            WorldAssetRoot(carriage_scene.clone()),
            Transform::from_xyz(cx, 0.0, cz).with_rotation(Quat::from_rotation_y(cyaw)),
        ));
        nav_world.add_box(cx, cz, hx, hz);
    }

    // 17. Афишные тумбы «Arkham Gazette» и Телефонные будки 1920-х на углах кварталов
    for (kx, kz) in [(-4.8, 21.8), (4.8, 27.2), (-4.5, 39.5)] {
        commands.spawn((
            WorldAssetRoot(kiosk_scene.clone()),
            Transform::from_xyz(kx, 0.0, kz),
        ));
        nav_world.add_box(kx, kz, 0.68, 0.68);
    }
    for (px, pz) in [(4.6, 21.8), (-4.6, 34.2), (4.6, 39.5)] {
        commands.spawn((
            WorldAssetRoot(phonebooth_scene.clone()),
            Transform::from_xyz(px, 0.0, pz),
        ));
        nav_world.add_box(px, pz, 0.62, 0.62);
    }

    // 18. Уличные кованые фонари вдоль аллеи, мостов и двух бульваров + теплый свет
    let lamp_positions = [
        (-2.6,  8.5, 0.0),
        ( 2.6,  8.5, std::f32::consts::PI),
        (-2.6, 13.5, 0.0),
        ( 2.6, 13.5, std::f32::consts::PI),
        (-6.8, 22.2, std::f32::consts::FRAC_PI_2),
        ( 6.8, 22.2, std::f32::consts::FRAC_PI_2),
        (-13.5, 27.0, -std::f32::consts::FRAC_PI_2),
        ( 13.5, 27.0, -std::f32::consts::FRAC_PI_2),
        (-6.8, 34.8, std::f32::consts::FRAC_PI_2),
        ( 6.8, 34.8, std::f32::consts::FRAC_PI_2),
        (-13.5, 39.5, -std::f32::consts::FRAC_PI_2),
        ( 13.5, 39.5, -std::f32::consts::FRAC_PI_2),
        ( 11.5, -2.5, -std::f32::consts::FRAC_PI_2),
    ];
    for (lx, lz, lyaw) in lamp_positions {
        commands.spawn((
            WorldAssetRoot(lamppost_scene.clone()),
            Transform::from_xyz(lx, 0.0, lz).with_rotation(Quat::from_rotation_y(lyaw)),
        ));
        nav_world.add_box(lx, lz, 0.26, 0.26);
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.80, 0.38),
                intensity: 55_000.0,
                range: 11.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(lx, 2.7, lz),
        ));
    }

    // 19. Уличные скамейки вдоль аллеи, на площадях и перед университетом
    let bench_positions = [
        (-3.2, 11.2, rot_90, 0.32, 0.90),
        ( 3.2, 11.2, Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2), 0.32, 0.90),
        (-3.6, 24.5, rot_90, 0.32, 0.90),
        ( 3.6, 24.5, Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2), 0.32, 0.90),
        (-9.5, 34.5, Quat::IDENTITY, 0.90, 0.32),
        ( 9.5, 34.5, Quat::IDENTITY, 0.90, 0.32),
        (14.5, -3.2, Quat::IDENTITY, 0.90, 0.32),
    ];
    for (bx, bz, brot, hx, hz) in bench_positions {
        commands.spawn((
            WorldAssetRoot(bench_scene.clone()),
            Transform::from_xyz(bx, 0.0, bz).with_rotation(brot),
        ));
        nav_world.add_box(bx, bz, hx, hz);
    }

    // 17. Проклятый колодец со щупальцами на восточном дворе (X=14.5, Z=0.0) + изумрудное свечение
    commands.spawn((
        WorldAssetRoot(well_scene),
        Transform::from_xyz(14.5, 0.0, 0.0).with_rotation(Quat::from_rotation_y(0.4)),
    ));
    nav_world.add_box(14.5, 0.0, 0.88, 0.88);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.25, 0.95, 0.55),
            intensity: 50_000.0,
            range: 10.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(14.5, 1.4, 0.0),
    ));

    // 18. Кованая ограда поместья и старого кладбища + Фамильный Склеп на западном погосте
    for fx in [-12.0, -8.0, 8.0, 12.0] {
        commands.spawn((
            WorldAssetRoot(fence_scene.clone()),
            Transform::from_xyz(fx, 0.0, 6.0),
        ));
        nav_world.add_box(fx, 6.0, 2.0, 0.24);
    }
    for fz in [8.0, 12.0] {
        commands.spawn((
            WorldAssetRoot(fence_scene.clone()),
            Transform::from_xyz(-14.0, 0.0, fz).with_rotation(rot_90),
        ));
        nav_world.add_box(-14.0, fz, 0.24, 2.0);
    }

    // Каменный готический склеп (мавзолей) на западном кладбище
    commands.spawn((
        WorldAssetRoot(crypt_scene),
        Transform::from_xyz(-11.2, 0.0, 9.2).with_rotation(rot_90),
    ));
    nav_world.add_box(-11.2, 9.2, 1.65, 1.45);

    // 19. Покосившиеся надгробия и рунические обелиски на западном погосте
    let grave_positions = [
        (-8.2,  8.5,  0.15),
        (-8.5, 11.0, -0.20),
        (-11.0, 12.8, 0.05),
        (-8.2, 13.2,  0.25),
    ];
    for (gx, gz, gyaw) in grave_positions {
        commands.spawn((
            WorldAssetRoot(gravestone_scene.clone()),
            Transform::from_xyz(gx, 0.0, gz).with_rotation(Quat::from_rotation_y(gyaw)),
        ));
        nav_world.add_box(gx, gz, 0.55, 0.38);
    }

    // 20. Затопленная циклопическая ротонда-святилище Древних в восточном парке поместья
    commands.spawn((
        WorldAssetRoot(gazebo_scene),
        Transform::from_xyz(11.5, 0.0, 10.5).with_rotation(Quat::from_rotation_y(-0.35)),
    ));
    nav_world.add_box(11.5, 10.5, 1.55, 1.55);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.25, 0.95, 0.58),
            intensity: 55_000.0,
            range: 11.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(11.5, 2.2, 10.5),
    ));

    // 21. Брошенная фермерская телега с урожаем у дороги к колодцу и еще одна у лавки в квартале
    commands.spawn((
        WorldAssetRoot(cart_scene.clone()),
        Transform::from_xyz(9.2, 0.0, -3.6).with_rotation(Quat::from_rotation_y(0.55)),
    ));
    nav_world.add_box(9.2, -3.6, 0.85, 1.15);

    commands.spawn((
        WorldAssetRoot(cart_scene),
        Transform::from_xyz(-13.5, 0.0, 26.8).with_rotation(Quat::from_rotation_y(1.25)),
    ));
    nav_world.add_box(-13.5, 26.8, 1.15, 0.85);

    // 22. Искривленные болотные деревья вокруг замка, канала и вдоль бульвара Аркхэма
    let tree_positions = [
        (-11.0,  3.5, 0.0, 1.0),
        (-16.5, 13.5, 1.2, 1.15),
        ( 16.5, 12.5, 1.9, 1.05),
        ( 18.5,  2.5, 3.1, 1.2),
        ( 17.0, -5.5, 0.5, 0.9),
        (-10.5, -8.5, 2.1, 1.05),
        (  9.5, -9.0, 1.4, 1.0),
        (-19.5, 24.5, 0.8, 1.15),
        ( 19.5, 24.5, 2.4, 1.15),
        ( -1.8, 33.5, 1.1, 1.0),
    ];
    for (tx, tz, tyaw, tscale) in tree_positions {
        commands.spawn((
            WorldAssetRoot(tree_scene.clone()),
            Transform::from_xyz(tx, 0.0, tz)
                .with_rotation(Quat::from_rotation_y(tyaw))
                .with_scale(Vec3::splat(tscale)),
        ));
        nav_world.add_box(tx, tz, 0.45 * tscale, 0.45 * tscale);
    }

    // 23. Колючие болотные кусты со светящимися спорами
    let bush_positions = [
        (-4.5,  7.5, 0.3),
        ( 4.5,  7.5, 1.7),
        (-4.8, 13.5, 2.2),
        ( 4.8, 13.5, 0.9),
        ( 9.5,  3.2, 0.4),
        (16.5, -2.5, 1.5),
        (-6.2, 20.5, 1.1),
        ( 6.2, 20.5, 2.5),
        (-12.2, 28.2, 0.7),
        ( 12.2, 28.2, 1.9),
    ];
    for (bx, bz, byaw) in bush_positions {
        commands.spawn((
            WorldAssetRoot(bush_scene.clone()),
            Transform::from_xyz(bx, 0.0, bz).with_rotation(Quat::from_rotation_y(byaw)),
        ));
        nav_world.add_box(bx, bz, 0.52, 0.52);
    }

    // Игрок: Загружаем 3D-модель Сыщика из Blender (detective.glb)
    let detective_scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/detective.glb"));
    
    commands
        .spawn((
            Player {
                speed: 8.5,
                path: VecDeque::new(),
                keyboard_vel: Vec3::ZERO,
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
    nav_world: Res<NavWorld>,
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

    // 1. Управление клавиатурой (WASD / стрелки) относительно угла камеры (с физическими коллизиями)
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

    // 2. Клик мыши (LMB) для поиска пути A* в обход твердых препятствий
    if mouse_button.just_pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                if ray.direction.y.abs() > f32::EPSILON {
                    let t = -ray.origin.y / ray.direction.y;
                    if t > 0.0 {
                        let ground_hit = ray.origin + *ray.direction * t;
                        let start_xz = Vec2::new(player_transform.translation.x, player_transform.translation.z);
                        let goal_xz = Vec2::new(ground_hit.x, ground_hit.z);
                        let waypoints = nav_world.find_path(start_xz, goal_xz);
                        player.path = waypoints.into();
                    }
                }
            }
        }
    }
}

fn ground_elevation(x: f32, z: f32) -> f32 {
    if x.abs() <= 6.0 && z.abs() <= 6.0 {
        0.08
    } else if (x.abs() <= 1.1 || (x - 19.5).abs() <= 1.1) && (z - 16.2).abs() <= 1.8 {
        let t = 1.0 - ((z - 16.2).abs() / 1.8).clamp(0.0, 1.0);
        0.06 + 0.28 * t
    } else {
        0.0
    }
}

fn player_movement_system(
    time: Res<Time>,
    nav_world: Res<NavWorld>,
    mut gizmos: Gizmos,
    mut query: Query<(&mut Player, &mut Transform)>,
) {
    for (mut player, mut transform) in &mut query {
        // Отрисовка найденного пути тонкими золотистыми линиями на земле
        if !player.path.is_empty() {
            let mut prev = Vec3::new(
                transform.translation.x,
                ground_elevation(transform.translation.x, transform.translation.z) + 0.12,
                transform.translation.z,
            );
            for wp in &player.path {
                let next = Vec3::new(wp.x, ground_elevation(wp.x, wp.z) + 0.12, wp.z);
                gizmos.line(prev, next, Color::srgba(0.95, 0.78, 0.30, 0.65));
                prev = next;
            }
            if let Some(final_wp) = player.path.back() {
                gizmos.circle(
                    Isometry3d::new(
                        Vec3::new(final_wp.x, ground_elevation(final_wp.x, final_wp.z) + 0.12, final_wp.z),
                        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                    ),
                    0.25,
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
                target.x - transform.translation.x,
                0.0,
                target.z - transform.translation.z,
            );
            let distance = to_target.length();
            let stop_tolerance = 0.12;

            if distance <= stop_tolerance {
                player.path.pop_front();
                if let Some(&next_target) = player.path.front() {
                    let next_vec = Vec3::new(
                        next_target.x - transform.translation.x,
                        0.0,
                        next_target.z - transform.translation.z,
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

        if move_dir.length_squared() > 0.0 {
            let proposed = transform.translation + move_dir * step_dist;
            // Физическое выталкивание из твердых объектов (скольжение вдоль стен и мебели)
            let resolved_xz = nav_world.resolve_collisions(
                Vec2::new(proposed.x, proposed.z),
                PLAYER_RADIUS,
            );
            let actual_delta = Vec3::new(
                resolved_xz.x - transform.translation.x,
                0.0,
                resolved_xz.y - transform.translation.z,
            );

            transform.translation.x = resolved_xz.x;
            transform.translation.z = resolved_xz.y;

            // Плавное покачивание при ходьбе (шаги сыщика)
            player.walk_timer += time.delta_secs() * 12.0;
            let base_y = ground_elevation(resolved_xz.x, resolved_xz.y);
            transform.translation.y = base_y + (player.walk_timer.sin() * 0.06).abs();

            // Поворот лицом по направлению движения
            let face_dir = if actual_delta.length_squared() > 1e-5 {
                actual_delta.normalize()
            } else {
                move_dir
            };
            let target_rotation = Quat::from_rotation_y(face_dir.x.atan2(face_dir.z));
            transform.rotation = transform.rotation.slerp(target_rotation, 15.0 * time.delta_secs());
        } else {
            player.walk_timer = 0.0;
            transform.translation.y = ground_elevation(transform.translation.x, transform.translation.z);
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
