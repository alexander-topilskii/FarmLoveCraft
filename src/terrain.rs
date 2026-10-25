use std::collections::{HashMap, HashSet};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use crate::destruction::Destructible;
use crate::navigation::NavWorld;
use crate::player::Player;

/// Размер одного чанка рельефа в метрах (16x16 м, как в Valheim-подобных сетках)
pub const CHUNK_SIZE: f32 = 16.0;
/// Шаг вершин внутри чанка (2.0 м -> 8x8 ячеек = 9x9 вершин на чанк)
pub const TERRAIN_STEP: f32 = 2.0;
pub const CHUNK_CELLS: i32 = 8;
/// Максимальная глубина раскопки и высота насыпи относительно базового процедурного рельефа
pub const MAX_TERRAFORM_DELTA: f32 = 3.5;
/// Радиус активной подгрузки чанков вокруг Сыщика
pub const CHUNK_STREAM_RADIUS: i32 = 5;
/// Радиус выгрузки далеких чанков (гистерезис в 2 чанка предотвращает частые перезагрузки)
pub const CHUNK_UNLOAD_RADIUS: i32 = CHUNK_STREAM_RADIUS + 2;

/// Маркер привязки процедурного объекта к чанку для выгрузки (OPT-03)
#[derive(Component, Clone, Copy, Debug)]
pub struct ChunkMember(pub ChunkCoord);

/// Координаты чанка в мировом пространстве
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChunkCoord {
    pub cx: i32,
    pub cz: i32,
}

impl ChunkCoord {
    pub fn new(cx: i32, cz: i32) -> Self {
        Self { cx, cz }
    }

    pub fn from_world(pos: Vec2) -> Self {
        Self {
            cx: (pos.x / CHUNK_SIZE).floor() as i32,
            cz: (pos.y / CHUNK_SIZE).floor() as i32,
        }
    }

    pub fn min_world_xz(self) -> Vec2 {
        Vec2::new(self.cx as f32 * CHUNK_SIZE, self.cz as f32 * CHUNK_SIZE)
    }
}

/// Тип поверхности в узле сетки рельефа (меняется при копке, вспашке или мощении)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SurfaceMod {
    #[default]
    Natural,
    /// Раскопанная/обнаженная земля и порода
    Excavated,
    /// Вскопанная болотная/фермерская почва под посадку Щупалец
    Cultivated,
    /// Мощеная камнем площадка
    Paved,
}

/// Процедурные биомы мира FarmLoveCraft
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Biome {
    /// Плато Замка Исследователя и кварталов Аркхэма/Иннсмута
    ArkhamCitadel,
    /// Топи и биолюминесцентные болота Иннсмута
    InnsmouthSwamp,
    /// Холмистый Проклятый Лес Аркхэма
    Darkwoods,
    /// Скалистые пустоши и циклопические руины Древних
    EldritchRuins,
}

impl Biome {
    pub fn display_name(self) -> &'static str {
        match self {
            Biome::ArkhamCitadel => "Поместье Аркхэма",
            Biome::InnsmouthSwamp => "Болота Иннсмута",
            Biome::Darkwoods => "Чащоба Черного Леса",
            Biome::EldritchRuins => "Пустоши Древних",
        }
    }
}

/// Операция изменения рельефа (терраформинг по принципу Valheim)
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum TerrainOp {
    /// Копать киркой/лопатой вниз
    Dig { depth: f32 },
    /// Насыпать землю вверх
    Raise { height: f32 },
    /// Выровнять землю мотыгой под заданную высоту `target_y`
    Flatten { target_y: f32 },
    /// Вскопать/удобрить почву под посадку
    Cultivate,
    /// Замостить брусчаткой
    Pave,
}

/// Компонент 3D-сущности чанка рельефа
#[derive(Component)]
#[allow(dead_code)]
pub struct TerrainChunkEntity {
    pub coord: ChunkCoord,
}

/// Главный ресурс процедурного и деформируемого мира (Valheim-style Heightmap + Vertex Deltas)
#[derive(Resource)]
pub struct WorldTerrain {
    pub seed: u32,
    /// Смещения высоты в узлах глобальной сетки `(gx, gz)` с шагом `TERRAIN_STEP`
    pub height_deltas: HashMap<IVec2, f32>,
    /// Модификаторы типа поверхности в узлах сетки `(gx, gz)`
    pub surface_mods: HashMap<IVec2, SurfaceMod>,
    /// Уже созданные чанки мира `ChunkCoord -> Entity`
    pub spawned_chunks: HashMap<ChunkCoord, Entity>,
    /// Чанки, в которых уже были сгенерированы процедурные объекты природы
    pub populated_chunks: HashSet<ChunkCoord>,
    /// Чанки, требующие пересчета 3D-меша после раскопок/терраформинга
    pub dirty_chunks: HashSet<ChunkCoord>,
    /// Общий материал вершинного рельефа
    pub terrain_material: Option<Handle<StandardMaterial>>,
}

impl Default for WorldTerrain {
    fn default() -> Self {
        Self::new(1928_04_15)
    }
}

impl WorldTerrain {
    pub fn new(seed: u32) -> Self {
        Self {
            seed,
            height_deltas: HashMap::new(),
            surface_mods: HashMap::new(),
            spawned_chunks: HashMap::new(),
            populated_chunks: HashSet::new(),
            dirty_chunks: HashSet::new(),
            terrain_material: None,
        }
    }

    /// Быстрый детерминированный 2D-хэш в `[0.0 .. 1.0]`
    #[inline]
    pub fn hash2d(ix: i32, iz: i32, seed: u32) -> f32 {
        let mut n = (ix as u32)
            .wrapping_mul(374761393)
            .wrapping_add((iz as u32).wrapping_mul(668265263))
            .wrapping_add(seed.wrapping_mul(1442695041));
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        n = n ^ (n >> 16);
        (n & 0x00FF_FFFF) as f32 / 16_777_215.0
    }

    /// Гладкий 2D Value Noise в `[-1.0 .. 1.0]`
    pub fn smooth_noise(x: f32, z: f32, seed: u32) -> f32 {
        let ix = x.floor() as i32;
        let iz = z.floor() as i32;
        let fx = x - ix as f32;
        let fz = z - iz as f32;
        let ux = fx * fx * (3.0 - 2.0 * fx);
        let uz = fz * fz * (3.0 - 2.0 * fz);

        let v00 = Self::hash2d(ix, iz, seed);
        let v10 = Self::hash2d(ix + 1, iz, seed);
        let v01 = Self::hash2d(ix, iz + 1, seed);
        let v11 = Self::hash2d(ix + 1, iz + 1, seed);

        let a = v00 + (v10 - v00) * ux;
        let b = v01 + (v11 - v01) * ux;
        (a + (b - a) * uz) * 2.0 - 1.0
    }

    /// Многооктавный фрактальный шум (fBm)
    pub fn fbm(x: f32, z: f32, seed: u32, octaves: u32) -> f32 {
        let mut value = 0.0;
        let mut amp = 0.5;
        let mut freq = 1.0;
        for i in 0..octaves {
            value += amp * Self::smooth_noise(x * freq, z * freq, seed.wrapping_add(i * 1013));
            freq *= 2.03;
            amp *= 0.5;
        }
        value
    }

    /// Определяет биом в мировой точке `(wx, wz)`
    pub fn sample_biome(&self, wx: f32, wz: f32) -> Biome {
        if self.city_flatten_factor(wx, wz) > 0.65 {
            return Biome::ArkhamCitadel;
        }
        let moisture = Self::fbm(wx * 0.012, wz * 0.012, self.seed ^ 0xA5A5, 3);
        let elevation_bias = Self::fbm(wx * 0.010, wz * 0.010, self.seed ^ 0x5A5A, 3);

        if elevation_bias > 0.25 {
            Biome::EldritchRuins
        } else if moisture > 0.10 {
            Biome::InnsmouthSwamp
        } else {
            Biome::Darkwoods
        }
    }

    /// Коэффициент плавного выравнивания рельефа `[0.0 .. 1.0]` в зоне Замка и улиц Аркхэма,
    /// чтобы предсобранные дома и канал аккуратно стыковались с процедурными холмами вокруг
    pub fn city_flatten_factor(&self, wx: f32, wz: f32) -> f32 {
        // Прямоугольник исторического центра: X in [-38..38], Z in [-18..72]
        let dx = (wx.abs() - 38.0).max(0.0);
        let dz = if wz < -18.0 {
            -18.0 - wz
        } else if wz > 72.0 {
            wz - 72.0
        } else {
            0.0
        };
        let dist = (dx * dx + dz * dz).sqrt();
        let transition = 22.0;
        (1.0 - (dist / transition).clamp(0.0, 1.0)).powi(2)
    }

    /// Базовая процедурная высота рельефа до пользовательских раскопок
    pub fn base_height(&self, wx: f32, wz: f32) -> f32 {
        let flatten = self.city_flatten_factor(wx, wz);
        if flatten >= 0.999 {
            return 0.0;
        }

        let macro_hills = Self::fbm(wx * 0.018, wz * 0.018, self.seed, 4) * 4.2;
        let detail = Self::fbm(wx * 0.065, wz * 0.065, self.seed.wrapping_add(77), 2) * 0.85;
        let biome = self.sample_biome(wx, wz);

        let raw_h = match biome {
            Biome::ArkhamCitadel => 0.0,
            Biome::InnsmouthSwamp => (macro_hills * 0.45 + detail * 0.4 - 0.35).max(-0.8),
            Biome::Darkwoods => macro_hills * 0.85 + detail + 0.4,
            Biome::EldritchRuins => (macro_hills * 1.35 + detail.abs() * 1.4 + 1.1).max(0.2),
        };

        raw_h * (1.0 - flatten)
    }

    /// Высота в узле дискретной сетки `(gx, gz)` с учетом раскопок/насыпей
    pub fn grid_height(&self, gx: i32, gz: i32) -> f32 {
        let wx = gx as f32 * TERRAIN_STEP;
        let wz = gz as f32 * TERRAIN_STEP;
        let base = self.base_height(wx, wz);
        let delta = self
            .height_deltas
            .get(&IVec2::new(gx, gz))
            .copied()
            .unwrap_or(0.0);
        base + delta
    }

    /// Билинейная интерполяция высоты рельефа в произвольной мировой точке `(wx, wz)`
    pub fn sample_height(&self, wx: f32, wz: f32) -> f32 {
        let gx = (wx / TERRAIN_STEP).floor() as i32;
        let gz = (wz / TERRAIN_STEP).floor() as i32;
        let tx = (wx / TERRAIN_STEP) - gx as f32;
        let tz = (wz / TERRAIN_STEP) - gz as f32;

        let h00 = self.grid_height(gx, gz);
        let h10 = self.grid_height(gx + 1, gz);
        let h01 = self.grid_height(gx, gz + 1);
        let h11 = self.grid_height(gx + 1, gz + 1);

        let hx0 = h00 + (h10 - h00) * tx;
        let hx1 = h01 + (h11 - h01) * tx;
        hx0 + (hx1 - hx0) * tz
    }

    /// Применяет операцию терраформинга (копка, насыпь, выравнивание, вспашка, мощение)
    /// в радиусе `radius` вокруг мировой точки `center_xz` и помечает затронутые чанки для перестройки меша.
    pub fn apply_terraform(
        &mut self,
        center_xz: Vec2,
        radius: f32,
        op: TerrainOp,
        nav_world: &mut NavWorld,
    ) -> usize {
        let min_gx = ((center_xz.x - radius) / TERRAIN_STEP).floor() as i32;
        let max_gx = ((center_xz.x + radius) / TERRAIN_STEP).ceil() as i32;
        let min_gz = ((center_xz.y - radius) / TERRAIN_STEP).floor() as i32;
        let max_gz = ((center_xz.y + radius) / TERRAIN_STEP).ceil() as i32;

        let mut modified_nodes = 0;

        for gz in min_gz..=max_gz {
            for gx in min_gx..=max_gx {
                let node_xz = Vec2::new(gx as f32 * TERRAIN_STEP, gz as f32 * TERRAIN_STEP);
                let dist = node_xz.distance(center_xz);
                if dist > radius {
                    continue;
                }

                // Плавный колоколообразный спад к краям кисти терраформинга
                let falloff = (1.0 - (dist / radius).clamp(0.0, 1.0)).sqrt();
                let key = IVec2::new(gx, gz);
                let base_h = self.base_height(node_xz.x, node_xz.y);
                let current_delta = self.height_deltas.get(&key).copied().unwrap_or(0.0);

                match op {
                    TerrainOp::Dig { depth } => {
                        let new_delta = (current_delta - depth * falloff)
                            .clamp(-MAX_TERRAFORM_DELTA, MAX_TERRAFORM_DELTA);
                        self.height_deltas.insert(key, new_delta);
                        self.surface_mods.insert(key, SurfaceMod::Excavated);
                    }
                    TerrainOp::Raise { height } => {
                        let new_delta = (current_delta + height * falloff)
                            .clamp(-MAX_TERRAFORM_DELTA, MAX_TERRAFORM_DELTA);
                        self.height_deltas.insert(key, new_delta);
                        self.surface_mods.insert(key, SurfaceMod::Excavated);
                    }
                    TerrainOp::Flatten { target_y } => {
                        let desired_delta = (target_y - base_h)
                            .clamp(-MAX_TERRAFORM_DELTA, MAX_TERRAFORM_DELTA);
                        let new_delta =
                            current_delta + (desired_delta - current_delta) * (falloff * 0.85);
                        self.height_deltas.insert(key, new_delta);
                        self.surface_mods.insert(key, SurfaceMod::Excavated);
                    }
                    TerrainOp::Cultivate => {
                        self.surface_mods.insert(key, SurfaceMod::Cultivated);
                    }
                    TerrainOp::Pave => {
                        self.surface_mods.insert(key, SurfaceMod::Paved);
                    }
                }

                let updated_h = self.grid_height(gx, gz);
                nav_world.terrain_heights.insert(key, updated_h);
                modified_nodes += 1;

                // Помечаем чанк и соседние чанки (если узел на границе чанка) как dirty
                for dcx in [-1, 0, 1] {
                    for dcz in [-1, 0, 1] {
                        let c = ChunkCoord::from_world(
                            node_xz + Vec2::new(dcx as f32 * TERRAIN_STEP, dcz as f32 * TERRAIN_STEP),
                        );
                        self.dirty_chunks.insert(c);
                    }
                }
            }
        }

        modified_nodes
    }

    /// Синхронизирует высоты узлов чанка с `NavWorld`
    pub fn sync_chunk_to_nav(&self, coord: ChunkCoord, nav_world: &mut NavWorld) {
        let base_gx = coord.cx * CHUNK_CELLS;
        let base_gz = coord.cz * CHUNK_CELLS;
        for lz in 0..=CHUNK_CELLS {
            for lx in 0..=CHUNK_CELLS {
                let gx = base_gx + lx;
                let gz = base_gz + lz;
                let h = self.grid_height(gx, gz);
                nav_world.terrain_heights.insert(IVec2::new(gx, gz), h);
            }
        }
    }

    /// Вычисляет нормаль поверхности в узле сетки `(gx, gz)` методом центральных разностей
    fn compute_node_normal(&self, gx: i32, gz: i32) -> Vec3 {
        let h_l = self.grid_height(gx - 1, gz);
        let h_r = self.grid_height(gx + 1, gz);
        let h_d = self.grid_height(gx, gz - 1);
        let h_u = self.grid_height(gx, gz + 1);

        Vec3::new(h_l - h_r, 2.0 * TERRAIN_STEP, h_d - h_u).normalize_or_zero()
    }

    /// Вычисляет вершинный цвет рельефа в узле `(gx, gz)` по биому, высоте, уклону и модификации почвы
    fn compute_node_color(&self, gx: i32, gz: i32, normal: Vec3) -> [f32; 4] {
        let wx = gx as f32 * TERRAIN_STEP;
        let wz = gz as f32 * TERRAIN_STEP;
        let h = self.grid_height(gx, gz);
        let surface_mod = self
            .surface_mods
            .get(&IVec2::new(gx, gz))
            .copied()
            .unwrap_or(SurfaceMod::Natural);

        // На крутых склонах (раскопки, обрывы, скалы) проступает серо-сланцевая каменная порода
        let slope = 1.0 - normal.y.clamp(0.0, 1.0);
        if slope > 0.34 {
            return [0.22, 0.23, 0.25, 1.0];
        }

        match surface_mod {
            SurfaceMod::Excavated => [0.24, 0.18, 0.13, 1.0],
            SurfaceMod::Cultivated => [0.16, 0.28, 0.20, 1.0],
            SurfaceMod::Paved => [0.27, 0.28, 0.30, 1.0],
            SurfaceMod::Natural => {
                let jitter = Self::hash2d(gx, gz, self.seed ^ 0x99) * 0.04 - 0.02;
                let biome = self.sample_biome(wx, wz);
                let base_rgb = match biome {
                    Biome::ArkhamCitadel => [0.12, 0.19, 0.14],
                    Biome::InnsmouthSwamp => {
                        if h < -0.15 {
                            // Биолюминесцентная болотная тина в низинах
                            [0.10, 0.26, 0.21]
                        } else {
                            [0.11, 0.22, 0.16]
                        }
                    }
                    Biome::Darkwoods => [0.10, 0.17, 0.12],
                    Biome::EldritchRuins => [0.19, 0.21, 0.23],
                };
                [
                    (base_rgb[0] + jitter).clamp(0.02, 1.0),
                    (base_rgb[1] + jitter).clamp(0.02, 1.0),
                    (base_rgb[2] + jitter).clamp(0.02, 1.0),
                    1.0,
                ]
            }
        }
    }

    /// Строит 3D `Mesh` чанка с плавными нормалями и вершинными цветами биомов/раскопок
    pub fn build_chunk_mesh(&self, coord: ChunkCoord) -> Mesh {
        let verts_per_side = (CHUNK_CELLS + 1) as usize;
        let total_verts = verts_per_side * verts_per_side;
        let mut positions = Vec::with_capacity(total_verts);
        let mut normals = Vec::with_capacity(total_verts);
        let mut colors = Vec::with_capacity(total_verts);
        let mut indices = Vec::with_capacity((CHUNK_CELLS * CHUNK_CELLS * 6) as usize);

        let base_gx = coord.cx * CHUNK_CELLS;
        let base_gz = coord.cz * CHUNK_CELLS;

        for lz in 0..=CHUNK_CELLS {
            for lx in 0..=CHUNK_CELLS {
                let gx = base_gx + lx;
                let gz = base_gz + lz;
                let wx = gx as f32 * TERRAIN_STEP;
                let wz = gz as f32 * TERRAIN_STEP;
                let wy = self.grid_height(gx, gz);
                let n = self.compute_node_normal(gx, gz);
                let col = self.compute_node_color(gx, gz, n);

                positions.push([wx, wy, wz]);
                normals.push(n.to_array());
                colors.push(col);
            }
        }

        let stride = verts_per_side as u32;
        for lz in 0..(CHUNK_CELLS as u32) {
            for lx in 0..(CHUNK_CELLS as u32) {
                let i00 = lz * stride + lx;
                let i10 = i00 + 1;
                let i01 = (lz + 1) * stride + lx;
                let i11 = i01 + 1;

                indices.extend_from_slice(&[i00, i01, i10, i10, i01, i11]);
            }
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }
}

/// Набор ассетов для процедурного заселения новых чанков мира
#[derive(Resource, Clone)]
#[allow(dead_code)]
pub struct ProceduralBiomeAssets {
    pub trees: Vec<Handle<WorldAsset>>,
    pub tree: Handle<WorldAsset>,
    pub bush: Handle<WorldAsset>,
    pub gravestone: Handle<WorldAsset>,
    pub gazebo: Handle<WorldAsset>,
    pub well: Handle<WorldAsset>,
    pub crypt: Handle<WorldAsset>,
}

/// Система перестройки мешей чанков, помеченных как `dirty` после раскопок или терраформинга
pub fn rebuild_dirty_chunks_system(
    mut terrain: ResMut<WorldTerrain>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunk_query: Query<&Mesh3d, With<TerrainChunkEntity>>,
) {
    if terrain.dirty_chunks.is_empty() {
        return;
    }

    let dirty_list: Vec<ChunkCoord> = terrain.dirty_chunks.drain().collect();
    for coord in dirty_list {
        if let Some(&entity) = terrain.spawned_chunks.get(&coord) {
            if let Ok(mesh_handle) = chunk_query.get(entity) {
                let new_mesh = terrain.build_chunk_mesh(coord);
                let _ = meshes.insert(&mesh_handle.0, new_mesh);
            }
        }
    }
}

/// Система динамической генерации новых чанков мира и процедурного спавна разрушаемых объектов
/// по мере продвижения Сыщика в неизведанные земли (по принципу Valheim)
pub fn stream_terrain_chunks_system(
    mut commands: Commands,
    mut terrain: ResMut<WorldTerrain>,
    mut nav_world: ResMut<NavWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    biome_assets: Option<Res<ProceduralBiomeAssets>>,
    player_query: Query<&Transform, With<Player>>,
    mut wild_objects: Query<(Entity, &ChunkMember, Option<&Destructible>)>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let Some(mat_handle) = terrain.terrain_material.clone().or_else(|| {
        let h = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.92,
            metallic: 0.02,
            ..default()
        });
        terrain.terrain_material = Some(h.clone());
        Some(h)
    }) else {
        return;
    };

    let center_chunk =
        ChunkCoord::from_world(Vec2::new(player_tf.translation.x, player_tf.translation.z));

    // 1. Выгрузка далеких чанков (Chunk Unloading с гистерезисом, OPT-03)
    let mut to_unload = Vec::new();
    for (&coord, &entity) in &terrain.spawned_chunks {
        // Стартовый замок/город (в радиусе 2 чанков) держим всегда загруженным
        if coord.cx.abs() <= 2 && coord.cz.abs() <= 2 {
            continue;
        }

        let dist_x = (coord.cx - center_chunk.cx).abs();
        let dist_z = (coord.cz - center_chunk.cz).abs();
        if dist_x > CHUNK_UNLOAD_RADIUS || dist_z > CHUNK_UNLOAD_RADIUS {
            to_unload.push((coord, entity));
        }
    }

    for (coord, entity) in to_unload {
        terrain.spawned_chunks.remove(&coord);
        terrain.populated_chunks.remove(&coord);
        commands.entity(entity).despawn();
    }

    // Выгрузка процедурных объектов далеких чанков
    for (obj_entity, member, dest_opt) in &mut wild_objects {
        if member.0.cx.abs() <= 2 && member.0.cz.abs() <= 2 {
            continue;
        }
        let dist_x = (member.0.cx - center_chunk.cx).abs();
        let dist_z = (member.0.cz - center_chunk.cz).abs();
        if dist_x > CHUNK_UNLOAD_RADIUS || dist_z > CHUNK_UNLOAD_RADIUS {
            if let Some(dest) = dest_opt {
                nav_world.remove_colliders(&dest.colliders);
            }
            commands.entity(obj_entity).despawn();
        }
    }

    // 2. Подгрузка новых чанков в радиусе стриминга
    let mut spawned_this_frame = 0;

    for dz in -CHUNK_STREAM_RADIUS..=CHUNK_STREAM_RADIUS {
        for dx in -CHUNK_STREAM_RADIUS..=CHUNK_STREAM_RADIUS {
            let coord = ChunkCoord::new(center_chunk.cx + dx, center_chunk.cz + dz);
            if terrain.spawned_chunks.contains_key(&coord) {
                continue;
            }

            terrain.sync_chunk_to_nav(coord, &mut nav_world);
            let mesh = terrain.build_chunk_mesh(coord);
            let entity = commands
                .spawn((
                    TerrainChunkEntity { coord },
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(mat_handle.clone()),
                    Transform::default(),
                ))
                .id();

            terrain.spawned_chunks.insert(coord, entity);

            // Заселяем дикие чанки за пределами стартового города процедурными разрушаемыми объектами
            if !terrain.populated_chunks.contains(&coord) {
                terrain.populated_chunks.insert(coord);
                if let Some(ref assets) = biome_assets {
                    populate_wild_chunk(
                        &mut commands,
                        &terrain,
                        &mut nav_world,
                        assets,
                        coord,
                    );
                }
            }

            spawned_this_frame += 1;
            if spawned_this_frame >= 4 {
                return;
            }
        }
    }
}

/// Процедурно размещает деревья, кусты со спорами, надгробия и древние руины в диком чанке
fn populate_wild_chunk(
    commands: &mut Commands,
    terrain: &WorldTerrain,
    nav_world: &mut NavWorld,
    assets: &ProceduralBiomeAssets,
    coord: ChunkCoord,
) {
    let min_xz = coord.min_world_xz();
    let center_xz = min_xz + Vec2::splat(CHUNK_SIZE * 0.5);

    // Не спавним случайные деревья поверх центрального Замка и улиц Аркхэма
    if terrain.city_flatten_factor(center_xz.x, center_xz.y) > 0.25 {
        return;
    }

    let biome = terrain.sample_biome(center_xz.x, center_xz.y);
    let seed = terrain.seed;

    // До 4 кандидатов на чанк по детерминированной сетке
    for slot in 0..4 {
        let r1 = WorldTerrain::hash2d(coord.cx * 17 + slot, coord.cz * 31 + slot, seed);
        let r2 = WorldTerrain::hash2d(coord.cx * 29 + slot, coord.cz * 13 + slot, seed ^ 0x7F7F);
        let r3 = WorldTerrain::hash2d(coord.cx * 41 + slot, coord.cz * 53 + slot, seed ^ 0x3333);

        let wx = min_xz.x + 2.2 + r1 * (CHUNK_SIZE - 4.4);
        let wz = min_xz.y + 2.2 + r2 * (CHUNK_SIZE - 4.4);
        let wy = terrain.sample_height(wx, wz);
        let yaw = r3 * std::f32::consts::TAU;

        match biome {
            Biome::Darkwoods | Biome::InnsmouthSwamp => {
                if r3 < 0.42 {
                    let tree_idx = ((r2 * 100.0) as usize) % (if assets.trees.is_empty() { 1 } else { assets.trees.len() });
                    let selected_tree = assets.trees.get(tree_idx).unwrap_or(&assets.tree).clone();
                    let scale = 0.85 + r1 * 0.35;
                    let is_infected = r1 > 0.60;
                    let col_id = nav_world.add_box(wx, wz, 0.75 * scale, 0.75 * scale);
                    commands.spawn((
                        WorldAssetRoot(selected_tree),
                        Transform::from_xyz(wx, wy, wz)
                            .with_rotation(Quat::from_rotation_y(yaw))
                            .with_scale(Vec3::splat(scale)),
                        crate::destruction::Destructible::tree(is_infected, vec![col_id]),
                        crate::destruction::InfectedFlora(is_infected),
                        ChunkMember(coord),
                    ));
                } else if r3 < 0.68 {
                    let is_infected = r2 > 0.55;
                    let col_id = nav_world.add_box(wx, wz, 0.80, 0.80);
                    commands.spawn((
                        WorldAssetRoot(assets.bush.clone()),
                        Transform::from_xyz(wx, wy, wz)
                            .with_rotation(Quat::from_rotation_y(yaw)),
                        crate::destruction::Destructible::bush(is_infected, vec![col_id]),
                        crate::destruction::InfectedFlora(is_infected),
                        ChunkMember(coord),
                    ));
                } else if slot == 0 && r1 > 0.88 {
                    let col_id = nav_world.add_box(wx, wz, 1.35, 1.35);
                    commands.spawn((
                        WorldAssetRoot(assets.well.clone()),
                        Transform::from_xyz(wx, wy, wz)
                            .with_rotation(Quat::from_rotation_y(yaw)),
                        crate::destruction::Destructible::occult_shrine(
                            "Древний колодец Бездны",
                            vec![col_id],
                        ),
                        ChunkMember(coord),
                    ));
                }
            }
            Biome::EldritchRuins => {
                if r3 < 0.38 {
                    let col_id = nav_world.add_box(wx, wz, 0.78, 0.55);
                    commands.spawn((
                        WorldAssetRoot(assets.gravestone.clone()),
                        Transform::from_xyz(wx, wy, wz)
                            .with_rotation(Quat::from_rotation_y(yaw)),
                        crate::destruction::Destructible::stone_monolith(
                            "Рунический монолит",
                            vec![col_id],
                        ),
                        ChunkMember(coord),
                    ));
                } else if slot == 0 && r1 > 0.78 {
                    let col_id = nav_world.add_box(wx, wz, 2.50, 2.50);
                    commands.spawn((
                        WorldAssetRoot(assets.gazebo.clone()),
                        Transform::from_xyz(wx, wy, wz)
                            .with_rotation(Quat::from_rotation_y(yaw)),
                        crate::destruction::Destructible::occult_shrine(
                            "Циклопическое святилище",
                            vec![col_id],
                        ),
                        ChunkMember(coord),
                    ));
                }
            }
            Biome::ArkhamCitadel => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_procedural_terrain_and_terraforming() {
        let mut terrain = WorldTerrain::new(42);
        let mut nav = NavWorld::default();

        // В центре Замка (0, 0) высота выровнена в 0.0
        assert!((terrain.sample_height(0.0, 0.0) - 0.0).abs() < 1e-4);

        // Применяем раскопку (Dig) в точке (6.0, 6.0)
        let changed = terrain.apply_terraform(
            Vec2::new(6.0, 6.0),
            3.5,
            TerrainOp::Dig { depth: 1.5 },
            &mut nav,
        );
        assert!(changed > 0, "Должны обновиться узлы сетки рельефа");
        assert!(
            terrain.sample_height(6.0, 6.0) < -0.5,
            "После раскопки рельеф должен понизиться"
        );
        assert!(
            !terrain.dirty_chunks.is_empty(),
            "Затронутые чанки должны быть помечены как dirty"
        );

        // Проверяем выравнивание (Flatten) обратно к 0.0
        terrain.apply_terraform(
            Vec2::new(6.0, 6.0),
            3.5,
            TerrainOp::Flatten { target_y: 0.0 },
            &mut nav,
        );
        assert!(terrain.sample_height(6.0, 6.0) > -0.5);
    }
}
