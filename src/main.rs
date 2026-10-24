use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::time::Duration;
use bevy::prelude::*;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::world_serialization::WorldInstanceReady;

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

const PLAYER_RADIUS: f32 = 0.36;
const NAV_CELL_SIZE: f32 = 0.30;
const NAV_HALF_EXTENT: i32 = 260; // [-78m .. +78m]

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

    fn intersects_circle(&self, pos: Vec2, radius: f32) -> bool {
        let closest = pos.clamp(self.center - self.half_extents, self.center + self.half_extents);
        pos.distance_squared(closest) < radius * radius
    }

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

    /// Добавляет готическую арку как два боковых физических пилона (оставляя проход в центре открытым)
    fn add_arch(&mut self, x: f32, z: f32, rotated_90: bool) {
        if !rotated_90 {
            self.add_box(x - 1.45, z, 0.58, 0.42);
            self.add_box(x + 1.45, z, 0.58, 0.42);
        } else {
            self.add_box(x, z - 1.45, 0.42, 0.58);
            self.add_box(x, z + 1.45, 0.42, 0.58);
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
            if (curr - origin).length_squared() > 625 {
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
            if iterations > 24_000 {
                break;
            }

            let current_g = *g_score.get(&current.cell).unwrap_or(&i32::MAX);

            for (offset, move_cost) in neighbors {
                let next = current.cell + offset;
                if !self.is_cell_walkable(next) {
                    continue;
                }
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

        let mut raw_pts = Vec::new();
        let mut curr = goal_cell;
        while curr != start_cell {
            raw_pts.push(Self::cell_to_world(curr));
            curr = came_from[&curr];
        }
        raw_pts.push(start_world);
        raw_pts.reverse();

        let exact_goal = if self.is_walkable_pos(goal_world, PLAYER_RADIUS + 0.03) {
            goal_world
        } else {
            Self::cell_to_world(goal_cell)
        };
        if let Some(last) = raw_pts.last_mut() {
            *last = exact_goal;
        }

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

#[derive(Resource)]
struct CharacterAnimations {
    graph_handle: Handle<AnimationGraph>,
    idle_node: AnimationNodeIndex,
    walk_node: AnimationNodeIndex,
}

#[derive(Component)]
struct Player {
    speed: f32,
    path: VecDeque<Vec3>,
    keyboard_vel: Vec3,
    is_moving: bool,
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
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut nav_world: ResMut<NavWorld>,
) {
    // 1. Создаем граф анимаций со скелетными клипами Idle и Walk из нашего GLB
    let idle_clip: Handle<AnimationClip> =
        asset_server.load(GltfAssetLabel::Animation(0).from_asset("models/detective.glb"));
    let walk_clip: Handle<AnimationClip> =
        asset_server.load(GltfAssetLabel::Animation(1).from_asset("models/detective.glb"));

    let mut graph = AnimationGraph::new();
    let idle_node = graph.add_clip(idle_clip, 1.0, graph.root);
    let walk_node = graph.add_clip(walk_clip, 1.0, graph.root);
    let graph_handle = graphs.add(graph);

    commands.insert_resource(CharacterAnimations {
        graph_handle,
        idle_node,
        walk_node,
    });

    // 2. Земля фермы и города Аркхэма (мрачно-зеленая лавкрафтовская почва 180x180м)
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(180.0, 180.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.2, 0.14),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // =========================================================================
    // ЗАМОК ИССЛЕДОВАТЕЛЯ (Просторная цитадель 20x20 метров, сетка пола 5x5 тайлов)
    // =========================================================================
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

    // Каменный пол замкового зала (сетка 5x5 тайлов по 4x4 метра -> зал 20x20м: X, Z в [-10..+10])
    for gx in -2..=2 {
        for gz in -2..=2 {
            commands.spawn((
                WorldAssetRoot(floor_scene.clone()),
                Transform::from_xyz(gx as f32 * 4.0, 0.0, gz as f32 * 4.0),
            ));
        }
    }

    // Северная стена (Z = -10.0): 4 глухие стены и центральная арка-портал
    for wx in [-8.0, -4.0, 4.0, 8.0] {
        commands.spawn((
            WorldAssetRoot(wall_scene.clone()),
            Transform::from_xyz(wx, 0.0, -10.0),
        ));
        nav_world.add_box(wx, -10.0, 2.0, 0.38);
    }
    commands.spawn((
        WorldAssetRoot(arch_scene.clone()),
        Transform::from_xyz(0.0, 0.0, -10.0),
    ));
    nav_world.add_arch(0.0, -10.0, false);

    // Западная и Восточная боковые стены (X = -10.0 и +10.0)
    let rot_90 = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    for wz in [-8.0, -4.0, 0.0, 4.0, 8.0] {
        commands.spawn((
            WorldAssetRoot(wall_scene.clone()),
            Transform::from_xyz(-10.0, 0.0, wz).with_rotation(rot_90),
        ));
        nav_world.add_box(-10.0, wz, 0.38, 2.0);
    }
    for wz in [-8.0, -4.0, 4.0, 8.0] {
        commands.spawn((
            WorldAssetRoot(wall_scene.clone()),
            Transform::from_xyz(10.0, 0.0, wz).with_rotation(rot_90),
        ));
        nav_world.add_box(10.0, wz, 0.38, 2.0);
    }
    // Восточный портал-арка (выход к проклятому колодцу)
    commands.spawn((
        WorldAssetRoot(arch_scene.clone()),
        Transform::from_xyz(10.0, 0.0, 0.0).with_rotation(rot_90),
    ));
    nav_world.add_arch(10.0, 0.0, true);

    // Южный фасад замка (Z = 10.0, полуоткрытый для изометрического обзора + парадная арка)
    commands.spawn((
        WorldAssetRoot(arch_scene),
        Transform::from_xyz(0.0, 0.0, 10.0),
    ));
    nav_world.add_arch(0.0, 10.0, false);

    // Рунические колонны (высота 4.6м) по периметру и в центре тронного зала
    for (px, pz) in [
        (-10.0, -10.0), (-2.2, -10.0), (2.2, -10.0), (10.0, -10.0),
        (-10.0,  10.0), (-2.2,  10.0), (2.2,  10.0), (10.0,  10.0),
        (-5.5, -4.5), (5.5, -4.5), (-5.5, 4.5), (5.5, 4.5),
    ] {
        commands.spawn((
            WorldAssetRoot(pillar_scene.clone()),
            Transform::from_xyz(px, 0.0, pz),
        ));
        nav_world.add_box(px, pz, 0.58, 0.58);
    }

    // Большой ритуальный ковер в центре зала (4.67 x 3.02м, проходимый)
    commands.spawn((
        WorldAssetRoot(rug_scene),
        Transform::from_xyz(0.0, 0.08, -1.0),
    ));

    // Массивный стол исследователя (2.87 x 1.55м) + два готических кресла-трона
    commands.spawn((
        WorldAssetRoot(table_scene),
        Transform::from_xyz(-4.8, 0.08, -1.5),
    ));
    nav_world.add_box(-4.8, -1.5, 1.45, 0.80);

    commands.spawn((
        WorldAssetRoot(chair_scene.clone()),
        Transform::from_xyz(-4.8, 0.08, -3.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
    ));
    nav_world.add_box(-4.8, -3.0, 0.44, 0.44);

    commands.spawn((
        WorldAssetRoot(chair_scene),
        Transform::from_xyz(-6.8, 0.08, -1.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    nav_world.add_box(-6.8, -1.5, 0.44, 0.44);

    // Высокие книжные шкафы (2.52 x 3.64м) вдоль северной стены
    for bx in [-6.5, 6.5] {
        commands.spawn((
            WorldAssetRoot(bookcase_scene.clone()),
            Transform::from_xyz(bx, 0.08, -9.1)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        ));
        nav_world.add_box(bx, -9.1, 1.28, 0.40);
    }
    commands.spawn((
        WorldAssetRoot(bookcase_scene),
        Transform::from_xyz(-9.1, 0.08, 5.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    nav_world.add_box(-9.1, 5.5, 0.40, 1.28);

    // Монументальный каменный камин (3.53м ширина, 4.07м высота) у западной стены
    commands.spawn((
        WorldAssetRoot(fireplace_scene),
        Transform::from_xyz(-9.0, 0.08, -1.0)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    nav_world.add_box(-9.0, -1.0, 0.75, 1.78);
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.52, 0.18),
            intensity: 85_000.0,
            range: 14.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-8.0, 0.9, -1.0),
    ));

    // Оккультный алтарь Ктулху (2.38 x 2.82м) в восточном крыле зала
    commands.spawn((
        WorldAssetRoot(altar_scene),
        Transform::from_xyz(6.2, 0.08, -2.0)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    nav_world.add_box(6.2, -2.0, 1.15, 1.15);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.25, 0.95, 0.55),
            intensity: 65_000.0,
            range: 12.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(6.2, 2.5, -2.0),
    ));

    // Алхимическая лаборатория (2.69 x 1.23м) в юго-восточном крыле
    commands.spawn((
        WorldAssetRoot(alchemy_scene),
        Transform::from_xyz(5.8, 0.08, 6.8),
    ));
    nav_world.add_box(5.8, 6.8, 1.36, 0.65);

    // Напольные часы с маятником (3.54м высота) у северной стены
    commands.spawn((
        WorldAssetRoot(clock_scene),
        Transform::from_xyz(3.4, 0.08, -9.1)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
    ));
    nav_world.add_box(3.4, -9.1, 0.50, 0.40);

    // Напольные канделябры (2.31м высота)
    for (cx, cz) in [(-3.2, -8.2), (3.2, -8.2), (-8.0, 8.2), (8.0, 8.2)] {
        commands.spawn((
            WorldAssetRoot(candelabra_scene.clone()),
            Transform::from_xyz(cx, 0.08, cz),
        ));
        nav_world.add_box(cx, cz, 0.42, 0.42);
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.72, 0.32),
                intensity: 40_000.0,
                range: 10.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(cx, 2.2, cz),
        ));
    }

    // =========================================================================
    // УЛИЦА / ПОМЕСТЬЕ, КАНАЛ ЩУПАЛЕЦ И АУТЕНТИЧНЫЙ ГОРОД АРКХЭМ / ИННСМУТ
    // (Все здания в реалистичном жилом масштабе: высота домов 9.5–17.5м, ширина 8–12м!)
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

    // Брусчатые проспекты и площади (тайл 3.0 x 3.0м, проходимые)
    for pz in [12.5, 15.8, 19.0, 27.5, 30.8, 39.2, 42.5, 45.8, 49.0, 52.2, 58.0, 61.5] {
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(0.0, 0.0, pz),
        ));
    }
    // Первая поперечная улица (Купеческий Проспект, Z = 35.0, X: -33..+33)
    for i in -11..=11 {
        if i == 0 { continue; }
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(i as f32 * 3.0, 0.0, 35.0),
        ));
    }
    // Вторая поперечная улица (Университетский Бульвар, Z = 55.0, X: -30..+30)
    for i in -10..=10 {
        if i == 0 { continue; }
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(i as f32 * 3.0, 0.0, 55.0),
        ));
    }
    // Восточная тропа к проклятому колодцу (X: 12.5..19.0, Z = 0)
    for px in [12.5, 15.8, 19.0] {
        commands.spawn((
            WorldAssetRoot(path_scene.clone()),
            Transform::from_xyz(px, 0.0, 0.0).with_rotation(rot_90),
        ));
    }

    // Монументальные кованые ворота поместья (4.8м ширина, 4.82м высота) и ограда кладбища (Z = 10.0)
    for gx in [-6.2, 6.2] {
        commands.spawn((
            WorldAssetRoot(gate_scene.clone()),
            Transform::from_xyz(gx, 0.0, 10.0),
        ));
        nav_world.add_box(gx - 1.85, 10.0, 0.55, 0.55);
        nav_world.add_box(gx + 1.85, 10.0, 0.55, 0.55);
    }
    for fx in [-19.0, -15.0, -11.0, 11.0, 15.0, 19.0] {
        commands.spawn((
            WorldAssetRoot(fence_scene.clone()),
            Transform::from_xyz(fx, 0.0, 10.0),
        ));
        nav_world.add_box(fx, 10.0, 2.05, 0.35);
    }
    for fz in [12.0, 16.0, 20.0] {
        commands.spawn((
            WorldAssetRoot(fence_scene.clone()),
            Transform::from_xyz(-21.0, 0.0, fz).with_rotation(rot_90),
        ));
        nav_world.add_box(-21.0, fz, 0.35, 2.05);
    }

    // Каменный готический склеп (4.56 x 5.21м, высота 5.88м) и надгробия (2.48м) на западном погосте
    commands.spawn((
        WorldAssetRoot(crypt_scene),
        Transform::from_xyz(-16.5, 0.0, 15.0).with_rotation(rot_90),
    ));
    nav_world.add_box(-16.5, 15.0, 2.65, 2.30);

    for (gx, gz, gyaw) in [
        (-11.5, 13.5, 0.15),
        (-11.8, 17.0, -0.20),
        (-15.5, 19.8, 0.05),
        (-11.5, 20.2, 0.25),
    ] {
        commands.spawn((
            WorldAssetRoot(gravestone_scene.clone()),
            Transform::from_xyz(gx, 0.0, gz).with_rotation(Quat::from_rotation_y(gyaw)),
        ));
        nav_world.add_box(gx, gz, 0.78, 0.55);
    }

    // Проклятый колодец со щупальцами (2.73 x 2.73м, высота 3.84м) на восточном дворе
    commands.spawn((
        WorldAssetRoot(well_scene),
        Transform::from_xyz(22.5, 0.0, 0.0).with_rotation(Quat::from_rotation_y(0.4)),
    ));
    nav_world.add_box(22.5, 0.0, 1.35, 1.35);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.25, 0.95, 0.55),
            intensity: 70_000.0,
            range: 14.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(22.5, 2.0, 0.0),
    ));

    // Затопленная циклопическая ротонда Древних (6.11 x 6.11м, высота 5.48м) в восточном парке
    commands.spawn((
        WorldAssetRoot(gazebo_scene),
        Transform::from_xyz(16.5, 0.0, 16.0).with_rotation(Quat::from_rotation_y(-0.35)),
    ));
    nav_world.add_box(16.5, 16.0, 2.60, 2.60);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.25, 0.95, 0.58),
            intensity: 75_000.0,
            range: 14.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(16.5, 3.0, 16.0),
    ));

    // Проточный каменный канал (Z = 23.5, секции 5.0 x 4.03м) и Два Горбатых Моста (3.57 x 5.10м, на X = 0.0 и X = 28.0)
    for bx in [0.0, 28.0] {
        commands.spawn((
            WorldAssetRoot(bridge_scene.clone()),
            Transform::from_xyz(bx, 0.0, 23.5),
        ));
        nav_world.add_box(bx - 1.58, 23.5, 0.30, 2.45);
        nav_world.add_box(bx + 1.58, 23.5, 0.30, 2.45);
    }
    for cx in [-26.5, -21.5, -16.5, -11.5, -6.5, 6.5, 11.5, 16.5, 21.5, 34.5] {
        commands.spawn((
            WorldAssetRoot(canal_scene.clone()),
            Transform::from_xyz(cx, 0.0, 23.5),
        ));
        nav_world.add_box(cx, 23.5, 2.45, 1.95);
    }

    // Доковые склады «Рыбопромышленной компании Марша» (9.10 x 7.52м, высота 10.60м!) на берегах канала
    commands.spawn((
        WorldAssetRoot(warehouse_scene.clone()),
        Transform::from_xyz(35.0, 0.0, 29.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));
    nav_world.add_box(35.0, 29.5, 3.70, 4.50);

    commands.spawn((
        WorldAssetRoot(warehouse_scene),
        Transform::from_xyz(-35.0, 0.0, 29.5).with_rotation(rot_90),
    ));
    nav_world.add_box(-35.0, 29.5, 3.70, 4.50);

    // Первая городская площадь: Большой Оккультный Фонтан (5.31 x 5.31м, высота 4.49м) на перекрестке (X = 0, Z = 35.0)
    commands.spawn((
        WorldAssetRoot(fountain_scene),
        Transform::from_xyz(0.0, 0.0, 35.0),
    ));
    nav_world.add_box(0.0, 35.0, 2.55, 2.55);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.22, 0.95, 0.62),
            intensity: 80_000.0,
            range: 16.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 3.8, 35.0),
    ));

    // Северный ряд зданий Первой улицы (Z = 28.5, фасадами на юг к улице Z = 35.0 -> поворот PI)
    // Особняки (8.68 x 8.19 x 11.80м) и Лавки (7.91 x 7.93 x 9.43м)
    for hx in [-23.5, 23.5] {
        commands.spawn((
            WorldAssetRoot(townhouse_scene.clone()),
            Transform::from_xyz(hx, 0.0, 28.5)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        ));
        nav_world.add_box(hx, 28.5, 4.25, 3.90);
    }
    for sx in [-13.5, 13.5] {
        commands.spawn((
            WorldAssetRoot(shop_scene.clone()),
            Transform::from_xyz(sx, 0.0, 28.5)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        ));
        nav_world.add_box(sx, 28.5, 3.90, 3.80);
    }

    // Южный ряд зданий Первой улицы (Z = 44.5, фасадами на север к улице Z = 35.0 -> поворот 0.0)
    // Гостиницы «Gilman House Inn» (9.29 x 8.83 x 10.68м), Лавки и Особняки
    for ix in [-10.0, 10.0] {
        commands.spawn((
            WorldAssetRoot(inn_scene.clone()),
            Transform::from_xyz(ix, 0.0, 44.5),
        ));
        nav_world.add_box(ix, 44.5, 4.55, 4.20);
    }
    for sx in [-20.0, 20.0] {
        commands.spawn((
            WorldAssetRoot(shop_scene.clone()),
            Transform::from_xyz(sx, 0.0, 44.5),
        ));
        nav_world.add_box(sx, 44.5, 3.90, 3.80);
    }
    for hx in [-30.0, 30.0] {
        commands.spawn((
            WorldAssetRoot(townhouse_scene.clone()),
            Transform::from_xyz(hx, 0.0, 44.5),
        ));
        nav_world.add_box(hx, 44.5, 4.25, 3.90);
    }

    // Монументальный Южный Проспект и Университетский Бульвар (Z = 55.0..68.5):
    // Городская Часовая Башня Аркхэма (4.76 x 4.76м, высота 15.41м!) на южном перекрестке (X = 0.0, Z = 55.0)
    commands.spawn((
        WorldAssetRoot(clocktower_scene),
        Transform::from_xyz(0.0, 0.0, 55.0),
    ));
    nav_world.add_box(0.0, 55.0, 2.38, 2.38);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.95, 0.82, 0.42),
            intensity: 90_000.0,
            range: 18.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 10.5, 55.0),
    ));

    // Готический Собор «Тайного Ордена Дагона» (10.55 x 12.73м, высота 17.46м!) замыкает главную ось на юге (X = 0.0, Z = 68.5)
    commands.spawn((
        WorldAssetRoot(church_scene),
        Transform::from_xyz(0.0, 0.0, 68.5),
    ));
    nav_world.add_box(0.0, 68.5, 5.10, 5.80);
    commands.spawn((
        PointLight {
            color: Color::srgb(0.65, 0.25, 0.85),
            intensity: 95_000.0,
            range: 20.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 6.8, 61.5),
    ));

    // Корпуса Библиотеки и Обсерватории Мискатоникского Университета (11.52 x 9.59м, высота 12.71м!) по бокам от Собора
    for ux in [-14.5, 14.5] {
        commands.spawn((
            WorldAssetRoot(university_scene.clone()),
            Transform::from_xyz(ux, 0.0, 66.5),
        ));
        nav_world.add_box(ux, 66.5, 5.65, 4.40);
    }

    // Боковые особняки на Университетском бульваре (X = -26.5 и +26.5, Z = 65.5)
    for hx in [-26.5, 26.5] {
        commands.spawn((
            WorldAssetRoot(townhouse_scene.clone()),
            Transform::from_xyz(hx, 0.0, 65.5),
        ));
        nav_world.add_box(hx, 65.5, 4.25, 3.90);
    }

    // Винтажные автомобили-катафалки 1920-х годов (2.76 x 5.14м, высота 2.96м)
    let carriage_positions = [
        (-8.5, 33.2, std::f32::consts::FRAC_PI_2, 2.55, 1.35),
        (12.5, 53.2, -std::f32::consts::FRAC_PI_2, 2.55, 1.35),
        (-3.8, 60.5, 0.15, 1.35, 2.55),
    ];
    for (cx, cz, cyaw, hx, hz) in carriage_positions {
        commands.spawn((
            WorldAssetRoot(carriage_scene.clone()),
            Transform::from_xyz(cx, 0.0, cz).with_rotation(Quat::from_rotation_y(cyaw)),
        ));
        nav_world.add_box(cx, cz, hx, hz);
    }

    // Фермерские телеги с урожаем (2.33 x 4.11м)
    commands.spawn((
        WorldAssetRoot(cart_scene.clone()),
        Transform::from_xyz(14.5, 0.0, -5.5).with_rotation(Quat::from_rotation_y(0.55)),
    ));
    nav_world.add_box(14.5, -5.5, 1.25, 1.95);

    commands.spawn((
        WorldAssetRoot(cart_scene),
        Transform::from_xyz(-24.5, 0.0, 38.2).with_rotation(Quat::from_rotation_y(1.25)),
    ));
    nav_world.add_box(-24.5, 38.2, 1.95, 1.25);

    // Афишные тумбы «Arkham Gazette» (1.99м диаметр, 4.20м высота) и Телефонные будки (1.51м ширина, 3.24м высота)
    for (kx, kz) in [(-6.5, 31.2), (6.5, 38.8), (-6.2, 58.5)] {
        commands.spawn((
            WorldAssetRoot(kiosk_scene.clone()),
            Transform::from_xyz(kx, 0.0, kz),
        ));
        nav_world.add_box(kx, kz, 0.95, 0.95);
    }
    for (px, pz) in [(6.5, 31.2), (-6.2, 51.2), (6.2, 58.5)] {
        commands.spawn((
            WorldAssetRoot(phonebooth_scene.clone()),
            Transform::from_xyz(px, 0.0, pz),
        ));
        nav_world.add_box(px, pz, 0.78, 0.78);
    }

    // Высокие уличные газовые фонари (4.65м высота) вдоль аллеи и проспектов + теплый свет
    let lamp_positions = [
        (-3.6, 13.5, 0.0),
        ( 3.6, 13.5, std::f32::consts::PI),
        (-3.6, 19.5, 0.0),
        ( 3.6, 19.5, std::f32::consts::PI),
        (-9.5, 31.8, std::f32::consts::FRAC_PI_2),
        ( 9.5, 31.8, std::f32::consts::FRAC_PI_2),
        (-19.5, 38.2, -std::f32::consts::FRAC_PI_2),
        ( 19.5, 38.2, -std::f32::consts::FRAC_PI_2),
        (-9.5, 51.8, std::f32::consts::FRAC_PI_2),
        ( 9.5, 51.8, std::f32::consts::FRAC_PI_2),
        (-19.5, 58.2, -std::f32::consts::FRAC_PI_2),
        ( 19.5, 58.2, -std::f32::consts::FRAC_PI_2),
        ( 17.5, -3.5, -std::f32::consts::FRAC_PI_2),
    ];
    for (lx, lz, lyaw) in lamp_positions {
        commands.spawn((
            WorldAssetRoot(lamppost_scene.clone()),
            Transform::from_xyz(lx, 0.0, lz).with_rotation(Quat::from_rotation_y(lyaw)),
        ));
        nav_world.add_box(lx, lz, 0.36, 0.36);
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.80, 0.38),
                intensity: 65_000.0,
                range: 14.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(lx, 3.8, lz),
        ));
    }

    // Увеличенные уличные скамейки (2.47м ширина, 1.61м высота)
    let bench_positions = [
        (-4.5, 16.5, rot_90, 0.48, 1.24),
        ( 4.5, 16.5, Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2), 0.48, 1.24),
        (-5.2, 35.0, rot_90, 0.48, 1.24),
        ( 5.2, 35.0, Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2), 0.48, 1.24),
        (-14.5, 51.5, Quat::IDENTITY, 1.24, 0.48),
        ( 14.5, 51.5, Quat::IDENTITY, 1.24, 0.48),
        ( 22.5, -4.5, Quat::IDENTITY, 1.24, 0.48),
    ];
    for (bx, bz, brot, hx, hz) in bench_positions {
        commands.spawn((
            WorldAssetRoot(bench_scene.clone()),
            Transform::from_xyz(bx, 0.0, bz).with_rotation(brot),
        ));
        nav_world.add_box(bx, bz, hx, hz);
    }

    // Вековые болотные деревья (высота 8.94м, крона 6.0м)
    let tree_positions = [
        (-16.5,   4.5, 0.0, 1.0),
        (-25.5,  19.0, 1.2, 1.1),
        ( 24.5,  18.5, 1.9, 1.0),
        ( 28.5,   3.5, 3.1, 1.15),
        ( 26.0,  -8.5, 0.5, 0.95),
        (-16.5, -14.5, 2.1, 1.05),
        ( 15.5, -14.0, 1.4, 1.0),
        (-34.5,  35.0, 0.8, 1.1),
        ( 34.5,  35.0, 2.4, 1.1),
        (-23.5,  55.0, 1.5, 1.05),
        ( 23.5,  55.0, 0.4, 1.05),
    ];
    for (tx, tz, tyaw, tscale) in tree_positions {
        commands.spawn((
            WorldAssetRoot(tree_scene.clone()),
            Transform::from_xyz(tx, 0.0, tz)
                .with_rotation(Quat::from_rotation_y(tyaw))
                .with_scale(Vec3::splat(tscale)),
        ));
        nav_world.add_box(tx, tz, 0.75 * tscale, 0.75 * tscale);
    }

    // Колючие болотные кусты со светящимися спорами (2.33 x 2.08м, высота 1.62м)
    let bush_positions = [
        (-6.2, 12.2, 0.3),
        ( 6.2, 12.2, 1.7),
        (-6.5, 19.5, 2.2),
        ( 6.5, 19.5, 0.9),
        (14.5,  4.2, 0.4),
        (26.5, -3.5, 1.5),
        (-8.5, 30.8, 1.1),
        ( 8.5, 30.8, 2.5),
        (-16.5, 39.5, 0.7),
        ( 16.5, 39.5, 1.9),
    ];
    for (bx, bz, byaw) in bush_positions {
        commands.spawn((
            WorldAssetRoot(bush_scene.clone()),
            Transform::from_xyz(bx, 0.0, bz).with_rotation(Quat::from_rotation_y(byaw)),
        ));
        nav_world.add_box(bx, bz, 0.82, 0.82);
    }

    // 3. Игрок: Загружаем 3D-модель Сыщика с наблюдателем инициализации скелетных анимаций
    let detective_scene =
        asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/detective.glb"));

    commands
        .spawn((
            Player {
                speed: 9.0,
                path: VecDeque::new(),
                keyboard_vel: Vec3::ZERO,
                is_moving: false,
            },
            Transform::from_xyz(0.0, 0.08, 4.0),
            Visibility::default(),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    WorldAssetRoot(detective_scene),
                    Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                ))
                .observe(on_character_scene_ready);

            parent.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.85, 0.45),
                    intensity: 80_000.0,
                    range: 14.0,
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
        Transform::from_xyz(25.0, 40.0, 25.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Изометрическая камера (Lineage 2 style)
    commands.spawn((
        Camera3d::default(),
        CameraController {
            yaw: 0.0,
            pitch: 42.0_f32.to_radians(),
            distance: 24.0,
        },
        Transform::from_xyz(0.0, 18.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Наблюдатель (Observer), срабатывающий когда иерархия сцены glTF спавнится в мире
fn on_character_scene_ready(
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
    if x.abs() <= 10.0 && z.abs() <= 10.0 {
        0.08
    } else if (x.abs() <= 1.55 || (x - 28.0).abs() <= 1.55) && (z - 23.5).abs() <= 2.55 {
        let t = 1.0 - ((z - 23.5).abs() / 2.55).clamp(0.0, 1.0);
        0.08 + 0.38 * t
    } else {
        0.0
    }
}

fn player_movement_system(
    time: Res<Time>,
    nav_world: Res<NavWorld>,
    animations: Option<Res<CharacterAnimations>>,
    mut gizmos: Gizmos,
    mut query: Query<(&mut Player, &mut Transform)>,
    mut animation_players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let Some(animations) = animations else { return };

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
                target.x - transform.translation.x,
                0.0,
                target.z - transform.translation.z,
            );
            let distance = to_target.length();
            let stop_tolerance = 0.14;

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

        let moving_now = move_dir.length_squared() > 0.0;

        if moving_now {
            let proposed = transform.translation + move_dir * step_dist;
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
            transform.translation.y = ground_elevation(resolved_xz.x, resolved_xz.y);

            let face_dir = if actual_delta.length_squared() > 1e-5 {
                actual_delta.normalize()
            } else {
                move_dir
            };
            let target_rotation = Quat::from_rotation_y(face_dir.x.atan2(face_dir.z));
            transform.rotation = transform.rotation.slerp(target_rotation, 15.0 * time.delta_secs());
        } else {
            transform.translation.y = ground_elevation(transform.translation.x, transform.translation.z);
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
                    .play(
                        &mut anim_player,
                        target_node,
                        Duration::from_millis(200),
                    )
                    .repeat();
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
