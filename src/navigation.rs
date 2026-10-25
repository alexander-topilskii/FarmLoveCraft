use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use bevy::prelude::*;

pub const PLAYER_RADIUS: f32 = 0.36;
pub const NAV_CELL_SIZE: f32 = 0.30;
pub const NAV_HALF_EXTENT: i32 = 260; // [-78m .. +78m]

/// Уникальный идентификатор физического коллайдера для динамического удаления при разрушении объекта
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ColliderId(pub u64);

/// Физический 2D-коллайдер (AABB в плоскости XZ) с привязкой к конкретному этажу
#[derive(Clone, Copy, Debug)]
pub struct ColliderRect {
    pub id: ColliderId,
    pub center: Vec2,
    pub half_extents: Vec2,
    /// Индекс этажа (`None` означает препятствие на всю высоту, например уличное дерево или колонна)
    pub floor: Option<i32>,
}

impl ColliderRect {
    pub fn new_on_floor(
        id: ColliderId,
        x: f32,
        z: f32,
        half_x: f32,
        half_z: f32,
        floor: Option<i32>,
    ) -> Self {
        Self {
            id,
            center: Vec2::new(x, z),
            half_extents: Vec2::new(half_x, half_z),
            floor,
        }
    }

    #[inline]
    pub fn applies_to_floor(&self, floor: i32) -> bool {
        match self.floor {
            Some(f) => f == floor,
            None => floor == 0,
        }
    }

    pub fn intersects_circle(&self, pos: Vec2, radius: f32) -> bool {
        let closest = pos.clamp(self.center - self.half_extents, self.center + self.half_extents);
        pos.distance_squared(closest) < radius * radius
    }

    pub fn resolve_circle(&self, pos: Vec2, radius: f32) -> Vec2 {
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

/// Связь между двумя этажами (лестничный марш), позволяющая персонажу и A* переходить между этажами
#[derive(Clone, Debug)]
pub struct StairLink {
    pub lower_floor: i32,
    pub upper_floor: i32,
    /// Точка входа на лестницу на нижнем этаже (XZ)
    pub lower_entry: Vec2,
    /// Точка выхода с лестницы на верхнем этаже (XZ)
    pub upper_entry: Vec2,
    pub lower_y: f32,
    pub upper_y: f32,
    pub half_width: f32,
}

impl StairLink {
    /// Возвращает прогресс `t` в `[0.0 .. 1.0]`, если точка `pos` находится на лестничном марше
    pub fn project_point(&self, pos: Vec2, margin: f32) -> Option<f32> {
        let axis = self.upper_entry - self.lower_entry;
        let len = axis.length();
        if len <= 1e-4 {
            return None;
        }
        let dir = axis / len;
        let perp = Vec2::new(-dir.y, dir.x);
        let rel = pos - self.lower_entry;
        let along = rel.dot(dir);
        let lateral = rel.dot(perp).abs();

        if lateral <= self.half_width + margin && along >= -margin && along <= len + margin {
            Some((along / len).clamp(0.0, 1.0))
        } else {
            None
        }
    }

    pub fn elevation_at(&self, t: f32) -> f32 {
        self.lower_y + t.clamp(0.0, 1.0) * (self.upper_y - self.lower_y)
    }
}

/// Плоская зона повышенного пола для конкретного этажа здания
#[derive(Clone, Debug)]
pub struct FloorElevationZone {
    pub rect: Rect,
    pub floor: i32,
    pub elevation: f32,
}

/// Точка маршрута с указанием целевого этажа
#[derive(Clone, Copy, Debug)]
pub struct PathWaypoint {
    pub pos: Vec3,
    pub floor: i32,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
struct NavCell {
    x: i32,
    z: i32,
    floor: i32,
}

impl NavCell {
    fn new(xy: IVec2, floor: i32) -> Self {
        Self {
            x: xy.x,
            z: xy.y,
            floor,
        }
    }

    fn xz(self) -> IVec2 {
        IVec2::new(self.x, self.z)
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct AStarNode {
    cell: NavCell,
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

/// Ресурс физического мира и многоэтажной навигационной сетки для A* поиска пути
#[derive(Resource, Default)]
pub struct NavWorld {
    pub colliders: Vec<ColliderRect>,
    pub stairs: Vec<StairLink>,
    pub floor_zones: Vec<FloorElevationZone>,
    next_collider_id: u64,
    /// Кэш высот терраформированного/процедурного рельефа по узлам сетки 2x2м
    pub terrain_heights: HashMap<IVec2, f32>,
}

impl NavWorld {
    fn alloc_collider_id(&mut self) -> ColliderId {
        self.next_collider_id += 1;
        ColliderId(self.next_collider_id)
    }

    /// Добавляет коллайдер на первый этаж / улицу (`floor = 0`)
    pub fn add_box(&mut self, x: f32, z: f32, half_x: f32, half_z: f32) -> ColliderId {
        self.add_box_on_floor(x, z, half_x, half_z, 0)
    }

    /// Добавляет коллайдер на конкретный этаж и возвращает его `ColliderId`
    pub fn add_box_on_floor(
        &mut self,
        x: f32,
        z: f32,
        half_x: f32,
        half_z: f32,
        floor: i32,
    ) -> ColliderId {
        let id = self.alloc_collider_id();
        self.colliders.push(ColliderRect::new_on_floor(
            id,
            x,
            z,
            half_x,
            half_z,
            Some(floor),
        ));
        id
    }

    /// Удаляет физические коллайдеры разрушенного объекта по их идентификаторам
    pub fn remove_colliders(&mut self, ids: &[ColliderId]) {
        if ids.is_empty() {
            return;
        }
        self.colliders.retain(|c| !ids.contains(&c.id));
    }

    /// Добавляет готическую арку как два боковых физических пилона (оставляя проход в центре открытым)
    #[allow(dead_code)]
    pub fn add_arch(&mut self, x: f32, z: f32, rotated_90: bool) -> Vec<ColliderId> {
        self.add_arch_on_floor(x, z, rotated_90, 0)
    }

    pub fn add_arch_on_floor(
        &mut self,
        x: f32,
        z: f32,
        rotated_90: bool,
        floor: i32,
    ) -> Vec<ColliderId> {
        if !rotated_90 {
            vec![
                self.add_box_on_floor(x - 1.45, z, 0.58, 0.42, floor),
                self.add_box_on_floor(x + 1.45, z, 0.58, 0.42, floor),
            ]
        } else {
            vec![
                self.add_box_on_floor(x, z - 1.45, 0.42, 0.58, floor),
                self.add_box_on_floor(x, z + 1.45, 0.42, 0.58, floor),
            ]
        }
    }

    pub fn add_stair_link(&mut self, stair: StairLink) {
        self.stairs.push(stair);
    }

    pub fn add_floor_zone(&mut self, rect: Rect, floor: i32, elevation: f32) {
        self.floor_zones.push(FloorElevationZone {
            rect,
            floor,
            elevation,
        });
    }

    /// Проверяет, находится ли точка на лестнице, позволяющей сменить этаж или игнорировать бортик перекрытия
    pub fn is_on_stair_corridor(&self, pos: Vec2, floor: i32) -> bool {
        self.stairs.iter().any(|s| {
            (s.lower_floor == floor || s.upper_floor == floor)
                && s.project_point(pos, 0.08).is_some()
        })
    }

    pub fn is_walkable_pos_on_floor(&self, pos: Vec2, floor: i32, radius: f32) -> bool {
        // Если это верхний этаж (> 0), ходить можно только в пределах плит этого этажа или лестниц
        if floor > 0 {
            let in_floor_zone = self
                .floor_zones
                .iter()
                .any(|z| z.floor == floor && z.rect.contains(pos));
            let on_stairs = self.is_on_stair_corridor(pos, floor);
            if !in_floor_zone && !on_stairs {
                return false;
            }
        }

        !self
            .colliders
            .iter()
            .filter(|c| c.applies_to_floor(floor))
            .any(|c| c.intersects_circle(pos, radius))
    }

    pub fn resolve_collisions_on_floor(&self, mut pos: Vec2, floor: i32, radius: f32) -> Vec2 {
        for _ in 0..3 {
            for col in self.colliders.iter().filter(|c| c.applies_to_floor(floor)) {
                pos = col.resolve_circle(pos, radius);
            }
        }
        pos
    }

    /// Вычисляет высоту Y и актуальный этаж для заданной позиции XZ и текущего этажа игрока
    pub fn sample_elevation_and_floor(&self, pos: Vec2, current_floor: i32) -> (f32, i32) {
        // 1. Сначала проверяем лестничные марши
        for stair in &self.stairs {
            if stair.lower_floor == current_floor || stair.upper_floor == current_floor {
                if let Some(t) = stair.project_point(pos, 0.15) {
                    let y = stair.elevation_at(t);
                    let new_floor = if current_floor == stair.lower_floor && t >= 0.56 {
                        stair.upper_floor
                    } else if current_floor == stair.upper_floor && t <= 0.44 {
                        stair.lower_floor
                    } else {
                        current_floor
                    };
                    return (y, new_floor);
                }
            }
        }

        // 2. Проверяем зоны этажей зданий
        for zone in &self.floor_zones {
            if zone.floor == current_floor && zone.rect.contains(pos) {
                return (zone.elevation, current_floor);
            }
        }

        // Если игрок оказался вне зоны верхнего этажа — плавно сбрасываем на 0 этаж
        if current_floor > 0 {
            for zone in &self.floor_zones {
                if zone.floor == 0 && zone.rect.contains(pos) {
                    return (zone.elevation, 0);
                }
            }
            return (self.sample_outdoor_elevation(pos.x, pos.y), 0);
        }

        (self.sample_outdoor_elevation(pos.x, pos.y), 0)
    }

    /// Высота рельефа для любой точки на заданном этаже (для отрисовки пути)
    pub fn elevation_on_floor(&self, pos: Vec2, floor: i32) -> f32 {
        for stair in &self.stairs {
            if stair.lower_floor == floor || stair.upper_floor == floor {
                if let Some(t) = stair.project_point(pos, 0.10) {
                    return stair.elevation_at(t);
                }
            }
        }
        for zone in &self.floor_zones {
            if zone.floor == floor && zone.rect.contains(pos) {
                return zone.elevation;
            }
        }
        self.sample_outdoor_elevation(pos.x, pos.y)
    }

    /// Вычисляет высоту уличного рельефа с учетом процедурных чанков, терраформинга и мостов
    pub fn sample_outdoor_elevation(&self, x: f32, z: f32) -> f32 {
        // Горбатые мосты через канал
        if (x.abs() <= 1.55 || (x - 28.0).abs() <= 1.55) && (z - 23.5).abs() <= 2.55 {
            let t = 1.0 - ((z - 23.5).abs() / 2.55).clamp(0.0, 1.0);
            return 0.08 + 0.38 * t;
        }
        if !self.terrain_heights.is_empty() {
            let step = 2.0_f32;
            let gx = (x / step).floor() as i32;
            let gz = (z / step).floor() as i32;
            let tx = (x / step) - gx as f32;
            let tz = (z / step) - gz as f32;

            let h00 = self
                .terrain_heights
                .get(&IVec2::new(gx, gz))
                .copied()
                .unwrap_or(0.0);
            let h10 = self
                .terrain_heights
                .get(&IVec2::new(gx + 1, gz))
                .copied()
                .unwrap_or(h00);
            let h01 = self
                .terrain_heights
                .get(&IVec2::new(gx, gz + 1))
                .copied()
                .unwrap_or(h00);
            let h11 = self
                .terrain_heights
                .get(&IVec2::new(gx + 1, gz + 1))
                .copied()
                .unwrap_or(h00);

            let hx0 = h00 + (h10 - h00) * tx;
            let hx1 = h01 + (h11 - h01) * tx;
            return hx0 + (hx1 - hx0) * tz;
        }
        Self::outdoor_ground_elevation(x, z)
    }

    pub fn outdoor_ground_elevation(x: f32, z: f32) -> f32 {
        if x.abs() <= 10.0 && z.abs() <= 10.0 {
            0.08
        } else if (x.abs() <= 1.55 || (x - 28.0).abs() <= 1.55) && (z - 23.5).abs() <= 2.55 {
            let t = 1.0 - ((z - 23.5).abs() / 2.55).clamp(0.0, 1.0);
            0.08 + 0.38 * t
        } else {
            0.0
        }
    }

    pub fn world_to_cell(pos: Vec2) -> IVec2 {
        IVec2::new(
            (pos.x / NAV_CELL_SIZE).round() as i32,
            (pos.y / NAV_CELL_SIZE).round() as i32,
        )
    }

    pub fn cell_to_world(cell: IVec2) -> Vec2 {
        Vec2::new(cell.x as f32 * NAV_CELL_SIZE, cell.y as f32 * NAV_CELL_SIZE)
    }

    fn is_cell_walkable(&self, cell: NavCell) -> bool {
        if cell.x.abs() > NAV_HALF_EXTENT || cell.z.abs() > NAV_HALF_EXTENT {
            return false;
        }
        self.is_walkable_pos_on_floor(Self::cell_to_world(cell.xz()), cell.floor, PLAYER_RADIUS + 0.02)
    }

    fn find_nearest_walkable(&self, origin: NavCell) -> Option<NavCell> {
        if self.is_cell_walkable(origin) {
            return Some(origin);
        }
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back(origin);
        visited.insert(origin);

        let dirs = [
            IVec2::new(1, 0),
            IVec2::new(-1, 0),
            IVec2::new(0, 1),
            IVec2::new(0, -1),
            IVec2::new(1, 1),
            IVec2::new(-1, 1),
            IVec2::new(1, -1),
            IVec2::new(-1, -1),
        ];

        while let Some(curr) = queue.pop_front() {
            if (curr.xz() - origin.xz()).length_squared() > 625 {
                break;
            }
            for d in dirs {
                let next = NavCell::new(curr.xz() + d, origin.floor);
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

    fn has_line_of_sight_on_floor(&self, a: Vec2, b: Vec2, floor: i32) -> bool {
        // Не спрямляем путь сквозь лестничный марш, чтобы персонаж заходил на лестницу ровно со входа
        let dist = a.distance(b);
        let steps = ((dist / (NAV_CELL_SIZE * 0.45)).ceil() as i32).max(1);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let p = a.lerp(b, t);
            if !self.is_walkable_pos_on_floor(p, floor, PLAYER_RADIUS + 0.02) {
                return false;
            }
        }
        true
    }

    /// Многоэтажный поиск пути A* с поддержкой автоматического подъема и спуска по лестницам
    pub fn find_path(
        &self,
        start_world: Vec2,
        start_floor: i32,
        goal_world: Vec2,
        goal_floor: i32,
    ) -> Vec<PathWaypoint> {
        let Some(start_cell) =
            self.find_nearest_walkable(NavCell::new(Self::world_to_cell(start_world), start_floor))
        else {
            return Vec::new();
        };
        let Some(goal_cell) =
            self.find_nearest_walkable(NavCell::new(Self::world_to_cell(goal_world), goal_floor))
        else {
            return Vec::new();
        };

        if start_cell == goal_cell {
            let final_pt = Self::cell_to_world(goal_cell.xz());
            let y = self.elevation_on_floor(final_pt, goal_floor);
            return vec![PathWaypoint {
                pos: Vec3::new(final_pt.x, y, final_pt.y),
                floor: goal_floor,
            }];
        }

        // Предвычисляем клетки входов/выходов всех лестниц для межэтажных переходов
        let stair_portals: Vec<(NavCell, NavCell, Vec2, Vec2, i32)> = self
            .stairs
            .iter()
            .filter_map(|s| {
                let lower_c = self.find_nearest_walkable(NavCell::new(
                    Self::world_to_cell(s.lower_entry),
                    s.lower_floor,
                ))?;
                let upper_c = self.find_nearest_walkable(NavCell::new(
                    Self::world_to_cell(s.upper_entry),
                    s.upper_floor,
                ))?;
                let cost = ((s.upper_entry.distance(s.lower_entry) / NAV_CELL_SIZE) * 12.0) as i32;
                Some((lower_c, upper_c, s.lower_entry, s.upper_entry, cost.max(20)))
            })
            .collect();

        let heuristic = |a: NavCell, b: NavCell| -> i32 {
            let dx = (a.x - b.x).abs();
            let dy = (a.z - b.z).abs();
            let floor_penalty = (a.floor - b.floor).abs() * 60;
            10 * (dx + dy) - 6 * dx.min(dy) + floor_penalty
        };

        let mut open = BinaryHeap::new();
        let mut g_score: HashMap<NavCell, i32> = HashMap::new();
        let mut came_from: HashMap<NavCell, NavCell> = HashMap::new();

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
            if iterations > 32_000 {
                break;
            }

            let current_g = *g_score.get(&current.cell).unwrap_or(&i32::MAX);

            // 1. Обычные соседи на том же этаже
            for (offset, move_cost) in neighbors {
                let next = NavCell::new(current.cell.xz() + offset, current.cell.floor);
                if !self.is_cell_walkable(next) {
                    continue;
                }
                if offset.x != 0 && offset.y != 0 {
                    let side_a = NavCell::new(
                        IVec2::new(current.cell.x + offset.x, current.cell.z),
                        current.cell.floor,
                    );
                    let side_b = NavCell::new(
                        IVec2::new(current.cell.x, current.cell.z + offset.y),
                        current.cell.floor,
                    );
                    if !self.is_cell_walkable(side_a) || !self.is_cell_walkable(side_b) {
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

            // 2. Межэтажные переходы по лестницам
            for &(lower_c, upper_c, _, _, stair_cost) in &stair_portals {
                let portal_target = if current.cell == lower_c {
                    Some(upper_c)
                } else if current.cell == upper_c {
                    Some(lower_c)
                } else {
                    None
                };

                if let Some(next) = portal_target {
                    let tentative_g = current_g + stair_cost;
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
        }

        if !found {
            return Vec::new();
        }

        let mut raw_cells = Vec::new();
        let mut curr = goal_cell;
        while curr != start_cell {
            raw_cells.push(curr);
            curr = came_from[&curr];
        }
        raw_cells.push(start_cell);
        raw_cells.reverse();

        let mut raw_pts: Vec<(Vec2, i32)> = raw_cells
            .iter()
            .map(|&c| {
                for &(lower_c, upper_c, lower_pos, upper_pos, _) in &stair_portals {
                    if c == lower_c {
                        return (lower_pos, c.floor);
                    }
                    if c == upper_c {
                        return (upper_pos, c.floor);
                    }
                }
                (Self::cell_to_world(c.xz()), c.floor)
            })
            .collect();

        if let Some(first) = raw_pts.first_mut() {
            first.0 = start_world;
        }
        let exact_goal = if self.is_walkable_pos_on_floor(goal_world, goal_floor, PLAYER_RADIUS + 0.03) {
            goal_world
        } else {
            Self::cell_to_world(goal_cell.xz())
        };
        if let Some(last) = raw_pts.last_mut() {
            last.0 = exact_goal;
        }

        // Сглаживание пути внутри каждого этажа с сохранением точек входа/выхода лестниц
        let mut smooth_path = Vec::new();
        let mut anchor_idx = 0;
        while anchor_idx < raw_pts.len() - 1 {
            let anchor_floor = raw_pts[anchor_idx].1;
            let mut furthest_idx = anchor_idx + 1;

            // Если следующий шаг меняет этаж (лестничный марш), не пропускаем точку входа/выхода лестницы
            if raw_pts[furthest_idx].1 == anchor_floor {
                for test_idx in (anchor_idx + 2)..raw_pts.len() {
                    if raw_pts[test_idx].1 != anchor_floor {
                        break;
                    }
                    if self.has_line_of_sight_on_floor(
                        raw_pts[anchor_idx].0,
                        raw_pts[test_idx].0,
                        anchor_floor,
                    ) {
                        furthest_idx = test_idx;
                    }
                }
            }

            let (wp_xz, wp_floor) = raw_pts[furthest_idx];
            let y = self.elevation_on_floor(wp_xz, wp_floor);
            smooth_path.push(PathWaypoint {
                pos: Vec3::new(wp_xz.x, y, wp_xz.y),
                floor: wp_floor,
            });
            anchor_idx = furthest_idx;
        }

        smooth_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multifloor_pathfinding_and_stair_elevation() {
        let mut nav = NavWorld::default();
        nav.add_floor_zone(Rect::from_center_half_size(Vec2::ZERO, Vec2::splat(10.0)), 0, 0.08);
        nav.add_floor_zone(Rect::from_center_half_size(Vec2::ZERO, Vec2::splat(10.0)), 1, 4.50);

        nav.add_stair_link(StairLink {
            lower_floor: 0,
            upper_floor: 1,
            lower_entry: Vec2::new(-7.6, 6.4),
            upper_entry: Vec2::new(-7.6, 1.4),
            lower_y: 0.08,
            upper_y: 4.50,
            half_width: 0.95,
        });

        // 1. Поиск пути с 1-го этажа (0) на 2-й этаж (1) через лестницу
        let path_up = nav.find_path(Vec2::new(0.0, 4.0), 0, Vec2::new(2.0, -5.0), 1);
        assert!(!path_up.is_empty(), "Путь на 2-й этаж должен быть найден");
        assert_eq!(path_up.last().unwrap().floor, 1);

        // 2. Поиск пути со 2-го этажа (1) на улицу (0)
        let path_down = nav.find_path(Vec2::new(2.0, -5.0), 1, Vec2::new(0.0, 15.0), 0);
        assert!(!path_down.is_empty(), "Путь со 2-го этажа на улицу должен быть найден");
        assert_eq!(path_down.last().unwrap().floor, 0);

        // 3. Проверка плавной интерполяции высоты и смены этажа на лестнице
        let (mid_y, floor_going_up) = nav.sample_elevation_and_floor(Vec2::new(-7.6, 3.0), 0);
        assert!(mid_y > 2.5 && mid_y < 4.5);
        assert_eq!(floor_going_up, 1, "При подъеме выше середины лестницы этаж переключается на 1");
    }
}

