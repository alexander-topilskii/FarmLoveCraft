use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use crate::terrain::{TERRAIN_STEP, WorldTerrain};

/// Максимальный уровень воды (блок-источник по правилам Minecraft)
pub const WATER_MAX_LEVEL: u8 = 8;
/// Интервал обновления клеточного автомата воды в секундах
pub const WATER_TICK_INTERVAL: f32 = 0.25;

/// Клетка воды в дискретной сетке мира
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterCell {
    /// Уровень воды: 8 = постоянный блок-источник; 7..1 = текущая вода; 0 = сухо
    pub level: u8,
    /// Является ли клетка созданным игроком постоянным источником (Колодец, Шлюз)
    pub is_source: bool,
    /// Флаг падающей воды (водопад при перепаде высоты рельефа)
    pub falling: bool,
    /// Высота дна рельефа в данной клетке
    pub ground_y: f32,
    /// Вектор поверхностного течения в плоскости XZ
    pub flow_dir: Vec2,
    /// Передаваемый оккультный бафф (если Сыщик бросил бафф в источник)
    pub buff_active: bool,
    pub buff_timer: f32,
}

impl Default for WaterCell {
    fn default() -> Self {
        Self {
            level: 0,
            is_source: false,
            falling: false,
            ground_y: 0.0,
            flow_dir: Vec2::ZERO,
            buff_active: false,
            buff_timer: 0.0,
        }
    }
}

/// Таймер шага клеточного автомата воды
#[derive(Resource)]
pub struct WaterSimulationTimer(pub Timer);

impl Default for WaterSimulationTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(WATER_TICK_INTERVAL, TimerMode::Repeating))
    }
}

/// Маркер 3D-сущности процедурной водной глади
#[derive(Component)]
pub struct WaterSurfaceMeshEntity;

/// Главный ресурс гидродинамического мира (клеточный автомат по правилам Minecraft)
#[derive(Resource, Default)]
pub struct WaterWorld {
    /// Дискретная сетка воды по узлам (gx, gz) с шагом `TERRAIN_STEP` (2.0 м)
    pub cells: HashMap<IVec2, WaterCell>,
    /// Список координат постоянных источников воды
    pub sources: HashSet<IVec2>,
    /// Флаг изменения сетки воды для пересчета 3D-меша
    pub is_dirty: bool,
    /// Кэшированный материал полупрозрачной биолюминесцентной воды Бездны
    pub material_handle: Option<Handle<StandardMaterial>>,
}

impl WaterWorld {
    /// Переводит мировые координаты XZ в дискретный узел сетки воды
    #[inline]
    pub fn world_to_grid(x: f32, z: f32) -> IVec2 {
        IVec2::new(
            (x / TERRAIN_STEP).round() as i32,
            (z / TERRAIN_STEP).round() as i32,
        )
    }

    /// Переводит координаты узла сетки в мировую позицию XZ
    #[inline]
    pub fn grid_to_world(grid: IVec2) -> Vec2 {
        Vec2::new(grid.x as f32 * TERRAIN_STEP, grid.y as f32 * TERRAIN_STEP)
    }

    /// Добавляет постоянный блок-источник воды (Колодец Бездны, шлюз, родник)
    pub fn add_source(&mut self, pos: Vec2, terrain: &WorldTerrain) {
        let grid = Self::world_to_grid(pos.x, pos.y);
        let ground_y = terrain.sample_height(pos.x, pos.y);
        self.sources.insert(grid);
        self.cells.insert(
            grid,
            WaterCell {
                level: WATER_MAX_LEVEL,
                is_source: true,
                falling: false,
                ground_y,
                flow_dir: Vec2::ZERO,
                buff_active: false,
                buff_timer: 0.0,
            },
        );
        self.is_dirty = true;
    }

    /// Удаляет источник воды и запускает естественное осушение русла
    pub fn remove_source(&mut self, pos: Vec2) {
        let grid = Self::world_to_grid(pos.x, pos.y);
        self.sources.remove(&grid);
        if let Some(cell) = self.cells.get_mut(&grid) {
            cell.is_source = false;
        }
        self.is_dirty = true;
    }

    /// Возвращает уровень воды в заданной мировой точке (0 = сухо, 1..=8 = вода)
    #[allow(dead_code)]
    pub fn sample_water_level(&self, x: f32, z: f32) -> u8 {
        let grid = Self::world_to_grid(x, z);
        self.cells.get(&grid).map(|c| c.level).unwrap_or(0)
    }

    /// Проверяет, орошается ли позиция текущей или стоячей водой (для фермы Щупалец и боевых растений)
    pub fn is_watered(&self, x: f32, z: f32) -> bool {
        let grid = Self::world_to_grid(x, z);
        if let Some(c) = self.cells.get(&grid) {
            if c.level >= 1 {
                return true;
            }
        }
        // Проверяем 4 соседние клетки (орошение прибрежной полосы)
        let neighbors = [
            IVec2::new(grid.x + 1, grid.y),
            IVec2::new(grid.x - 1, grid.y),
            IVec2::new(grid.x, grid.y + 1),
            IVec2::new(grid.x, grid.y - 1),
        ];
        neighbors
            .iter()
            .any(|n| self.cells.get(n).map(|c| c.level >= 1).unwrap_or(false))
    }

    /// Возвращает вектор физического сноса течением в заданной мировой точке
    pub fn sample_flow_velocity(&self, x: f32, z: f32) -> Vec3 {
        let grid = Self::world_to_grid(x, z);
        if let Some(cell) = self.cells.get(&grid) {
            if cell.level >= 1 && cell.flow_dir.length_squared() > 0.01 {
                let speed = 1.2 + (cell.level as f32) * 0.35;
                return Vec3::new(cell.flow_dir.x * speed, 0.0, cell.flow_dir.y * speed);
            }
        }
        Vec3::ZERO
    }

    /// Накладывает алхимический бафф на источник воды, который передается вниз по течению
    #[allow(dead_code)]
    pub fn infuse_source_buff(&mut self, source_pos: Vec2, duration: f32) {
        let grid = Self::world_to_grid(source_pos.x, source_pos.y);
        if let Some(cell) = self.cells.get_mut(&grid) {
            cell.buff_active = true;
            cell.buff_timer = duration;
            self.is_dirty = true;
        }
    }

    /// Один шаг детерминированной симуляции клеточного автомата (FixedUpdate / 0.25 сек)
    pub fn step_simulation(&mut self, terrain: &WorldTerrain) {
        let orthogonal = [
            IVec2::new(1, 0),
            IVec2::new(-1, 0),
            IVec2::new(0, 1),
            IVec2::new(0, -1),
        ];

        let mut next_cells = self.cells.clone();
        let active_coords: HashSet<IVec2> = self.cells.keys().copied().collect();

        // 1. Правило 2x2 бесконечного источника воды (как в Minecraft)
        // Если пустая/проточная клетка имеет >= 2 ортогональных соседей-источников на той же высоте,
        // она превращается в постоянный источник воды!
        for &coord in &active_coords {
            let curr = self.cells.get(&coord).copied().unwrap_or_default();
            if !curr.is_source {
                let ground_y = terrain.sample_height(
                    coord.x as f32 * TERRAIN_STEP,
                    coord.y as f32 * TERRAIN_STEP,
                );
                let adjacent_sources_count = orthogonal
                    .iter()
                    .filter(|&&offset| {
                        let n_coord = coord + offset;
                        if let Some(n_cell) = self.cells.get(&n_coord) {
                            n_cell.is_source && (n_cell.ground_y - ground_y).abs() <= 0.4
                        } else {
                            false
                        }
                    })
                    .count();

                if adjacent_sources_count >= 2 {
                    if let Some(c) = next_cells.get_mut(&coord) {
                        c.level = WATER_MAX_LEVEL;
                        c.is_source = true;
                        c.falling = false;
                        self.sources.insert(coord);
                    }
                }
            }
        }

        // 2. Распространение и затухание потока
        let mut candidates_to_expand = Vec::new();

        for (&coord, &cell) in &self.cells {
            if cell.level == 0 {
                continue;
            }

            let curr_ground = terrain.sample_height(
                coord.x as f32 * TERRAIN_STEP,
                coord.y as f32 * TERRAIN_STEP,
            );

            for offset in orthogonal {
                let n_coord = coord + offset;
                let n_world = Self::grid_to_world(n_coord);
                let n_ground = terrain.sample_height(n_world.x, n_world.y);

                // Течение невозможно сквозь высокую насыпь/стену
                if n_ground > curr_ground + 0.35 {
                    continue;
                }

                // Правило стекания вниз (водопад / углубление канала):
                // Если соседняя клетка глубже на >= 0.35м, напор воды обновляется до максимума (8)
                let is_waterfall = n_ground < curr_ground - 0.35;
                let target_level = if is_waterfall {
                    WATER_MAX_LEVEL
                } else {
                    cell.level.saturating_sub(1)
                };

                if target_level > 0 {
                    candidates_to_expand.push((
                        n_coord,
                        target_level,
                        is_waterfall,
                        n_ground,
                        cell.buff_active,
                        cell.buff_timer,
                    ));
                }
            }
        }

        // Применяем расширение потока
        for (coord, level, falling, ground_y, buff, buff_time) in candidates_to_expand {
            let entry = next_cells.entry(coord).or_insert(WaterCell {
                ground_y,
                ..default()
            });

            if !entry.is_source && level > entry.level {
                entry.level = level;
                entry.falling = falling;
                entry.ground_y = ground_y;
                if buff {
                    entry.buff_active = true;
                    entry.buff_timer = buff_time;
                }
            }
        }

        // 3. Правило осушения: клетки, потерявшие подпитку от более высокого уровня, убывают на -1
        for (&coord, next_cell) in next_cells.iter_mut() {
            if next_cell.is_source {
                next_cell.level = WATER_MAX_LEVEL;
                continue;
            }

            if next_cell.level > 0 {
                let curr_ground = next_cell.ground_y;
                let max_neighbor_level = orthogonal
                    .iter()
                    .filter_map(|&off| {
                        let n_coord = coord + off;
                        self.cells.get(&n_coord).and_then(|nc| {
                            let higher_or_equal = nc.ground_y >= curr_ground - 0.35;
                            if higher_or_equal && nc.level > 0 {
                                Some(nc.level)
                            } else {
                                None
                            }
                        })
                    })
                    .max()
                    .unwrap_or(0);

                if max_neighbor_level <= next_cell.level && !next_cell.falling {
                    next_cell.level = next_cell.level.saturating_sub(1);
                }
            }
        }

        // 4. Расчет векторов поверхностного течения flow_dir в каждой клетке
        let mut flow_updates = Vec::new();
        for (&coord, next_cell) in &next_cells {
            if next_cell.level == 0 {
                continue;
            }

            let mut dir = Vec2::ZERO;
            for offset in orthogonal {
                let n_coord = coord + offset;
                if let Some(nc) = next_cells.get(&n_coord) {
                    if nc.level < next_cell.level || nc.ground_y < next_cell.ground_y - 0.25 {
                        let off_v = Vec2::new(offset.x as f32, offset.y as f32);
                        let strength = (next_cell.level as f32) - (nc.level as f32);
                        dir += off_v * strength.max(0.5);
                    }
                } else {
                    let off_v = Vec2::new(offset.x as f32, offset.y as f32);
                    dir += off_v;
                }
            }
            flow_updates.push((coord, dir.normalize_or_zero()));
        }

        for (coord, dir) in flow_updates {
            if let Some(c) = next_cells.get_mut(&coord) {
                c.flow_dir = dir;
            }
        }

        // Удаляем полностью высохшие клетки из памяти
        next_cells.retain(|_, c| c.level > 0 || c.is_source);

        self.cells = next_cells;
        self.is_dirty = true;
    }

    /// Генерация 3D-меша водной глади для отображения каналов и потоков
    pub fn build_water_mesh(&self, time: f32) -> Mesh {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut colors = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();

        let half_step = TERRAIN_STEP * 0.5;

        for (&coord, cell) in &self.cells {
            if cell.level == 0 {
                continue;
            }

            let center = Self::grid_to_world(coord);

            // Проверяем соседей на наличие берега / сухости для расчета прибрежной пены
            let neighbors = [
                IVec2::new(coord.x + 1, coord.y),
                IVec2::new(coord.x - 1, coord.y),
                IVec2::new(coord.x, coord.y + 1),
                IVec2::new(coord.x, coord.y - 1),
            ];
            let is_edge = neighbors.iter().any(|n| self.cells.get(n).map(|c| c.level == 0).unwrap_or(true));

            // Волны Герстнера / двойной синус
            let wave_y = ((center.x * 0.85 + time * 2.4).sin() * (center.y * 0.75 - time * 1.9).cos()) * 0.035;
            let water_height = cell.ground_y + 0.05 + (cell.level as f32 / 8.0) * 0.20 + wave_y;

            let base_idx = positions.len() as u32;

            // 4 вершины квада водной глади ячейки
            positions.push([center.x - half_step, water_height, center.y - half_step]);
            positions.push([center.x + half_step, water_height, center.y - half_step]);
            positions.push([center.x + half_step, water_height, center.y + half_step]);
            positions.push([center.x - half_step, water_height, center.y + half_step]);

            // Нормали с легким уклоном по волне
            let norm_x = -((center.x * 0.85 + time * 2.4).cos() * 0.12);
            let norm_z = -((center.y * 0.75 - time * 1.9).sin() * 0.12);
            let wave_norm = Vec3::new(norm_x, 1.0, norm_z).normalize().to_array();

            for _ in 0..4 {
                normals.push(wave_norm);
            }

            // Цвет воды: лавкрафтовская глубина, френелевский блеск, пена у берегов и бафф
            let (r, g, b, alpha) = if cell.buff_active {
                // Пурпурное биолюминесцентное свечение алхимического баффа (HDR > 1.5)
                (1.20, 0.35, 1.45, 0.90)
            } else if is_edge || cell.falling {
                // Бело-бирюзовая пена у кромки берегов и на водопадах
                (0.72, 0.95, 0.92, 0.85)
            } else if cell.is_source {
                // Яркий источник Бездны
                (0.12, 0.92, 0.72, 0.82)
            } else {
                // Проточная вода канала Valheim (глубокий зеркальный бирюзово-изумрудный)
                (0.08, 0.58, 0.68, 0.72)
            };

            for _ in 0..4 {
                colors.push([r, g, b, alpha]);
            }

            uvs.push([0.0, 0.0]);
            uvs.push([1.0, 0.0]);
            uvs.push([1.0, 1.0]);
            uvs.push([0.0, 1.0]);

            indices.extend_from_slice(&[
                base_idx,
                base_idx + 1,
                base_idx + 2,
                base_idx,
                base_idx + 2,
                base_idx + 3,
            ]);
        }

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }
}

/// Система обновления клеточного автомата воды по таймеру (FixedUpdate)
pub fn water_simulation_system(
    time: Res<Time>,
    mut timer: ResMut<WaterSimulationTimer>,
    mut water_world: ResMut<WaterWorld>,
    terrain: Res<WorldTerrain>,
) {
    if timer.0.tick(time.delta()).just_finished() {
        water_world.step_simulation(&terrain);
    }
}

/// Система перестройки 3D-меша водной глади при изменении уровней воды
pub fn sync_water_mesh_system(
    mut commands: Commands,
    time: Res<Time>,
    mut water_world: ResMut<WaterWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing_query: Query<(Entity, &Mesh3d), With<WaterSurfaceMeshEntity>>,
) {
    if !water_world.is_dirty {
        return;
    }
    water_world.is_dirty = false;

    let mat_handle = water_world
        .material_handle
        .clone()
        .unwrap_or_else(|| {
            let h = materials.add(StandardMaterial {
                base_color: Color::srgba(0.08, 0.60, 0.72, 0.72),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.08, // зеркальные блики фонарей и луны в стиле Valheim
                metallic: 0.04,
                reflectance: 0.85,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            water_world.material_handle = Some(h.clone());
            h
        });

    let new_mesh = water_world.build_water_mesh(time.elapsed_secs());

    if let Ok((entity, mesh_handle)) = existing_query.single() {
        if water_world.cells.is_empty() {
            commands.entity(entity).despawn();
        } else {
            let _ = meshes.insert(&mesh_handle.0, new_mesh);
        }
    } else if !water_world.cells.is_empty() {
        commands.spawn((
            WaterSurfaceMeshEntity,
            Mesh3d(meshes.add(new_mesh)),
            MeshMaterial3d(mat_handle),
            Transform::default(),
        ));
    }
}

/// Плавная анимация вершинных волн Герстнера по времени
pub fn animate_water_waves_system(
    time: Res<Time>,
    water_world: Res<WaterWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    water_query: Query<&Mesh3d, With<WaterSurfaceMeshEntity>>,
) {
    if water_world.cells.is_empty() {
        return;
    }
    let Ok(mesh_handle) = water_query.single() else {
        return;
    };

    if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
        let t = time.elapsed_secs();
        let half_step = TERRAIN_STEP * 0.5;
        let mut new_positions = Vec::with_capacity(water_world.cells.len() * 4);

        for (&coord, cell) in &water_world.cells {
            if cell.level == 0 {
                continue;
            }
            let center = WaterWorld::grid_to_world(coord);
            let wave_y = ((center.x * 0.85 + t * 2.4).sin() * (center.y * 0.75 - t * 1.9).cos()) * 0.035;
            let water_height = cell.ground_y + 0.05 + (cell.level as f32 / 8.0) * 0.20 + wave_y;

            new_positions.push([center.x - half_step, water_height, center.y - half_step]);
            new_positions.push([center.x + half_step, water_height, center.y - half_step]);
            new_positions.push([center.x + half_step, water_height, center.y + half_step]);
            new_positions.push([center.x - half_step, water_height, center.y + half_step]);
        }

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, new_positions);
    }
}

/// Система воздействия физического течения воды на Сыщика (снос по каналу)
pub fn water_flow_physics_system(
    water_world: Res<WaterWorld>,
    time: Res<Time>,
    mut player_query: Query<(&mut Transform, &crate::player::Player)>,
) {
    let Ok((mut player_tf, player)) = player_query.single_mut() else {
        return;
    };

    let flow = water_world.sample_flow_velocity(player_tf.translation.x, player_tf.translation.z);
    if flow.length_squared() > 0.01 {
        // Если игрок идет против течения, его слегка тормозит; по течению — ускоряет
        let dt = time.delta_secs();
        let push_factor = if player.is_moving { 0.45 } else { 0.95 };
        player_tf.translation += flow * dt * push_factor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minecraft_water_flow_and_infinite_source_rule() {
        let terrain = WorldTerrain::default();
        let mut water = WaterWorld::default();

        // Ставим один источник в (0.0, 0.0)
        water.add_source(Vec2::new(0.0, 0.0), &terrain);
        assert_eq!(water.sample_water_level(0.0, 0.0), 8);

        // Тик 1: вода должна растечься на 1 клетку в стороны с уровнем 7
        water.step_simulation(&terrain);
        assert_eq!(water.sample_water_level(2.0, 0.0), 7);
        assert_eq!(water.sample_water_level(-2.0, 0.0), 7);
        assert_eq!(water.sample_water_level(0.0, 2.0), 7);
        assert_eq!(water.sample_water_level(0.0, -2.0), 7);

        // Тик 2: растекается дальше с уровнем 6
        water.step_simulation(&terrain);
        assert_eq!(water.sample_water_level(4.0, 0.0), 6);

        // Тестируем правило бесконечного источника 2x2:
        // Ставим второй источник в (2.0, 2.0)
        water.add_source(Vec2::new(2.0, 2.0), &terrain);
        // Клетка (2.0, 0.0) имеет соседа-источника (0.0, 0.0) и (2.0, 2.0)!
        water.step_simulation(&terrain);
        let cell_2_0 = water.cells.get(&WaterWorld::world_to_grid(2.0, 0.0)).unwrap();
        assert!(
            cell_2_0.is_source && cell_2_0.level == 8,
            "Клетка между двумя источниками должна сама стать бесконечным источником (Minecraft 2x2 rule)"
        );
    }

    #[test]
    fn test_water_drain_when_source_removed() {
        let terrain = WorldTerrain::default();
        let mut water = WaterWorld::default();

        water.add_source(Vec2::new(10.0, 10.0), &terrain);
        water.step_simulation(&terrain);
        assert_eq!(water.sample_water_level(12.0, 10.0), 7);

        // Убираем источник
        water.remove_source(Vec2::new(10.0, 10.0));
        // За несколько тиков вода должна полностью высохнуть
        for _ in 0..10 {
            water.step_simulation(&terrain);
        }
        assert_eq!(water.sample_water_level(10.0, 10.0), 0);
        assert_eq!(water.sample_water_level(12.0, 10.0), 0);
    }
}
