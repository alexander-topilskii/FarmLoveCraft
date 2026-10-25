use bevy::prelude::*;
use crate::navigation::{ColliderId, NavWorld};
use crate::player::Player;
use crate::terrain::WorldTerrain;

/// Игровые ресурсы из `GameDoc.md`
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ResourceType {
    /// Дерево — используется для строительства, добывается из деревьев, мебели и деревянных построек
    Wood,
    /// Камень / Металл — используется для строительства готических стен, колонн, арок и полов
    Stone,
    /// Земля — добывается при копке рельефа, используется для насыпи и высадки растений
    Soil,
    /// Споры / Страхи — основная мистическая валюта (с алтарей, щупалец и споровых узлов)
    Fear,
    /// Зацепки (Улики) — исследовательская валюта (из книжных шкафов, столов и фолиантов)
    Clues,
    /// Листва / Ветки — органические остатки при рубке крон деревьев и расчистке кустов
    Foliage,
}

impl ResourceType {
    pub fn name(self) -> &'static str {
        match self {
            ResourceType::Wood => "Дерево",
            ResourceType::Stone => "Камень/Металл",
            ResourceType::Soil => "Земля",
            ResourceType::Fear => "Споры (Страх)",
            ResourceType::Clues => "Зацепки",
            ResourceType::Foliage => "Листва",
        }
    }

    pub fn color(self) -> Color {
        match self {
            ResourceType::Wood => Color::srgb(0.68, 0.46, 0.24),
            ResourceType::Stone => Color::srgb(0.62, 0.66, 0.72),
            ResourceType::Soil => Color::srgb(0.45, 0.34, 0.22),
            ResourceType::Fear => Color::srgb(0.25, 0.95, 0.58),
            ResourceType::Clues => Color::srgb(0.95, 0.80, 0.32),
            ResourceType::Foliage => Color::srgb(0.18, 0.45, 0.22),
        }
    }
}

/// 3D-модели выпадающих ресурсов, лежащих на земле
#[derive(Resource, Clone, Default)]
pub struct ResourceModelAssets {
    pub stone: Handle<WorldAsset>,
    pub wood: Handle<WorldAsset>,
    pub foliage: Handle<WorldAsset>,
    pub spores: Handle<WorldAsset>,
}

/// Кэшированные меши и материалы для эффектов осколков и выпадающего лута (OPT-06)
#[derive(Resource, Clone)]
pub struct SharedVfxAssets {
    pub debris_mesh: Handle<Mesh>,
    pub fallback_loot_mesh: Handle<Mesh>,
    pub wood_debris_mat: Handle<StandardMaterial>,
    pub stone_debris_mat: Handle<StandardMaterial>,
    pub foliage_debris_mat: Handle<StandardMaterial>,
    pub occult_debris_mat: Handle<StandardMaterial>,
    pub default_debris_mat: Handle<StandardMaterial>,
    pub stone_loot_mat: Handle<StandardMaterial>,
    pub wood_loot_mat: Handle<StandardMaterial>,
    pub foliage_loot_mat: Handle<StandardMaterial>,
    pub spores_loot_mat: Handle<StandardMaterial>,
    pub clues_loot_mat: Handle<StandardMaterial>,
}

impl SharedVfxAssets {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let debris_mesh = meshes.add(Cuboid::new(0.22, 0.22, 0.22));
        let fallback_loot_mesh = meshes.add(Cuboid::new(0.32, 0.32, 0.32));

        let mut make_mat = |color: Color, roughness: f32, emissive: Option<Color>| {
            materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: roughness,
                emissive: emissive.map(|e| e.into()).unwrap_or_default(),
                ..default()
            })
        };

        Self {
            debris_mesh,
            fallback_loot_mesh,
            wood_debris_mat: make_mat(DestructibleMaterial::Wood.debris_color(), 0.9, None),
            stone_debris_mat: make_mat(DestructibleMaterial::Stone.debris_color(), 0.9, None),
            foliage_debris_mat: make_mat(DestructibleMaterial::Foliage.debris_color(), 0.9, None),
            occult_debris_mat: make_mat(DestructibleMaterial::Occult.debris_color(), 0.9, None),
            default_debris_mat: make_mat(Color::srgb(0.5, 0.5, 0.5), 0.9, None),
            stone_loot_mat: make_mat(ResourceType::Stone.color(), 0.4, Some(ResourceType::Stone.color())),
            wood_loot_mat: make_mat(ResourceType::Wood.color(), 0.4, Some(ResourceType::Wood.color())),
            foliage_loot_mat: make_mat(ResourceType::Foliage.color(), 0.4, Some(ResourceType::Foliage.color())),
            spores_loot_mat: make_mat(ResourceType::Fear.color(), 0.4, Some(ResourceType::Fear.color())),
            clues_loot_mat: make_mat(ResourceType::Clues.color(), 0.4, Some(ResourceType::Clues.color())),
        }
    }

    pub fn debris_material_for(&self, mat: DestructibleMaterial) -> Handle<StandardMaterial> {
        match mat {
            DestructibleMaterial::Wood => self.wood_debris_mat.clone(),
            DestructibleMaterial::Stone => self.stone_debris_mat.clone(),
            DestructibleMaterial::Foliage => self.foliage_debris_mat.clone(),
            DestructibleMaterial::Occult => self.occult_debris_mat.clone(),
        }
    }

    pub fn loot_material_for(&self, res: ResourceType) -> Handle<StandardMaterial> {
        match res {
            ResourceType::Stone | ResourceType::Soil => self.stone_loot_mat.clone(),
            ResourceType::Wood => self.wood_loot_mat.clone(),
            ResourceType::Foliage => self.foliage_loot_mat.clone(),
            ResourceType::Fear => self.spores_loot_mat.clone(),
            ResourceType::Clues => self.clues_loot_mat.clone(),
        }
    }
}

/// Инвентарь ресурсов Сыщика
#[derive(Resource, Debug, Clone)]
pub struct PlayerInventory {
    pub wood: u32,
    pub stone: u32,
    pub soil: u32,
    pub fear: u32,
    pub clues: u32,
    pub foliage: u32,
}

impl Default for PlayerInventory {
    fn default() -> Self {
        // Стартовый запас ресурсов для немедленного тестирования постройки и фермы
        Self {
            wood: 10,
            stone: 12,
            soil: 6,
            fear: 5,
            clues: 3,
            foliage: 4,
        }
    }
}

impl PlayerInventory {
    pub fn add(&mut self, res: ResourceType, amount: u32) {
        match res {
            ResourceType::Wood => self.wood += amount,
            ResourceType::Stone => self.stone += amount,
            ResourceType::Soil => self.soil += amount,
            ResourceType::Fear => self.fear += amount,
            ResourceType::Clues => self.clues += amount,
            ResourceType::Foliage => self.foliage += amount,
        }
    }

    pub fn get(&self, res: ResourceType) -> u32 {
        match res {
            ResourceType::Wood => self.wood,
            ResourceType::Stone => self.stone,
            ResourceType::Soil => self.soil,
            ResourceType::Fear => self.fear,
            ResourceType::Clues => self.clues,
            ResourceType::Foliage => self.foliage,
        }
    }

    pub fn can_afford(&self, costs: &[(ResourceType, u32)]) -> bool {
        costs.iter().all(|&(r, amt)| self.get(r) >= amt)
    }

    pub fn spend(&mut self, costs: &[(ResourceType, u32)]) -> bool {
        if !self.can_afford(costs) {
            return false;
        }
        for &(r, amt) in costs {
            match r {
                ResourceType::Wood => self.wood -= amt,
                ResourceType::Stone => self.stone -= amt,
                ResourceType::Soil => self.soil -= amt,
                ResourceType::Fear => self.fear -= amt,
                ResourceType::Clues => self.clues -= amt,
                ResourceType::Foliage => self.foliage -= amt,
            }
        }
        true
    }
}

/// Маркер заражения флоры спорами/скверной Древних
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InfectedFlora(pub bool);

/// Тип материала разрушаемого объекта (влияет на цвет обломков и прочность)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DestructibleMaterial {
    Wood,
    Stone,
    Foliage,
    Occult,
}

impl DestructibleMaterial {
    pub fn debris_color(self) -> Color {
        match self {
            DestructibleMaterial::Wood => Color::srgb(0.48, 0.32, 0.18),
            DestructibleMaterial::Stone => Color::srgb(0.36, 0.38, 0.42),
            DestructibleMaterial::Foliage => Color::srgb(0.18, 0.42, 0.24),
            DestructibleMaterial::Occult => Color::srgb(0.22, 0.88, 0.54),
        }
    }
}

/// Компонент разрушаемого объекта мира (как для природных объектов, так и для всех стен/полов/мебели)
#[derive(Component, Clone, Debug)]
pub struct Destructible {
    pub title: &'static str,
    pub hp: f32,
    pub max_hp: f32,
    pub material: DestructibleMaterial,
    pub drops: Vec<(ResourceType, u32)>,
    /// Связанные физические коллайдеры в `NavWorld`, которые удаляются при сносе объекта
    pub colliders: Vec<ColliderId>,
    /// Горизонтальный радиус для выбора кликом ПКМ (RTS-выбор)
    pub pick_radius: f32,
    /// Высота объекта для 3D-рейкаста мыши
    pub pick_height: f32,
}

impl Destructible {
    pub fn wall(title: &'static str, colliders: Vec<ColliderId>) -> Self {
        Self {
            title,
            hp: 100.0,
            max_hp: 100.0,
            material: DestructibleMaterial::Stone,
            drops: vec![(ResourceType::Stone, 3), (ResourceType::Wood, 1)],
            colliders,
            pick_radius: 1.9,
            pick_height: 4.4,
        }
    }

    pub fn arch(title: &'static str, colliders: Vec<ColliderId>) -> Self {
        Self {
            title,
            hp: 110.0,
            max_hp: 110.0,
            material: DestructibleMaterial::Stone,
            drops: vec![(ResourceType::Stone, 3)],
            colliders,
            pick_radius: 1.9,
            pick_height: 4.4,
        }
    }

    pub fn pillar(title: &'static str, colliders: Vec<ColliderId>) -> Self {
        Self {
            title,
            hp: 140.0,
            max_hp: 140.0,
            material: DestructibleMaterial::Stone,
            drops: vec![(ResourceType::Stone, 4)],
            colliders,
            pick_radius: 0.95,
            pick_height: 4.5,
        }
    }

    pub fn floor_tile(title: &'static str) -> Self {
        Self {
            title,
            hp: 80.0,
            max_hp: 80.0,
            material: DestructibleMaterial::Stone,
            drops: vec![(ResourceType::Stone, 2)],
            colliders: Vec::new(),
            pick_radius: 1.8,
            pick_height: 0.4,
        }
    }

    #[allow(dead_code)]
    pub fn staircase(title: &'static str, colliders: Vec<ColliderId>) -> Self {
        Self {
            title,
            hp: 120.0,
            max_hp: 120.0,
            material: DestructibleMaterial::Stone,
            drops: vec![(ResourceType::Stone, 3), (ResourceType::Wood, 2)],
            colliders,
            pick_radius: 2.2,
            pick_height: 4.2,
        }
    }

    pub fn tree(infected: bool, colliders: Vec<ColliderId>) -> Self {
        let drops = if infected {
            vec![
                (ResourceType::Wood, 3),
                (ResourceType::Foliage, 3),
                (ResourceType::Fear, 2),
            ]
        } else {
            vec![
                (ResourceType::Wood, 3),
                (ResourceType::Foliage, 3),
            ]
        };
        Self {
            title: if infected { "Зараженное болотное дерево" } else { "Болотное дерево" },
            hp: 100.0,
            max_hp: 100.0,
            material: DestructibleMaterial::Wood,
            drops,
            colliders,
            pick_radius: 1.6,
            pick_height: 7.5,
        }
    }

    pub fn bush(infected: bool, colliders: Vec<ColliderId>) -> Self {
        let drops = if infected {
            vec![
                (ResourceType::Foliage, 2),
                (ResourceType::Wood, 1),
                (ResourceType::Fear, 1),
            ]
        } else {
            vec![
                (ResourceType::Foliage, 2),
                (ResourceType::Wood, 1),
            ]
        };
        Self {
            title: if infected { "Зараженный кустарник" } else { "Болотный кустарник" },
            hp: 30.0,
            max_hp: 30.0,
            material: DestructibleMaterial::Foliage,
            drops,
            colliders,
            pick_radius: 1.2,
            pick_height: 1.8,
        }
    }

    pub fn furniture(
        title: &'static str,
        colliders: Vec<ColliderId>,
        drops: Vec<(ResourceType, u32)>,
    ) -> Self {
        Self {
            title,
            hp: 50.0,
            max_hp: 50.0,
            material: DestructibleMaterial::Wood,
            drops,
            colliders,
            pick_radius: 1.4,
            pick_height: 2.6,
        }
    }

    pub fn occult_shrine(title: &'static str, colliders: Vec<ColliderId>) -> Self {
        Self {
            title,
            hp: 90.0,
            max_hp: 90.0,
            material: DestructibleMaterial::Occult,
            drops: vec![
                (ResourceType::Stone, 3),
                (ResourceType::Fear, 3),
                (ResourceType::Clues, 1),
            ],
            colliders,
            pick_radius: 2.0,
            pick_height: 3.8,
        }
    }

    pub fn stone_monolith(title: &'static str, colliders: Vec<ColliderId>) -> Self {
        Self {
            title,
            hp: 75.0,
            max_hp: 75.0,
            material: DestructibleMaterial::Stone,
            drops: vec![(ResourceType::Stone, 3), (ResourceType::Fear, 1)],
            colliders,
            pick_radius: 1.5,
            pick_height: 3.2,
        }
    }

    pub fn street_prop(title: &'static str, colliders: Vec<ColliderId>) -> Self {
        Self {
            title,
            hp: 55.0,
            max_hp: 55.0,
            material: DestructibleMaterial::Wood,
            drops: vec![(ResourceType::Wood, 2), (ResourceType::Stone, 1)],
            colliders,
            pick_radius: 1.5,
            pick_height: 2.8,
        }
    }

    pub fn building_structure(title: &'static str, colliders: Vec<ColliderId>) -> Self {
        Self {
            title,
            hp: 180.0,
            max_hp: 180.0,
            material: DestructibleMaterial::Stone,
            drops: vec![
                (ResourceType::Wood, 6),
                (ResourceType::Stone, 6),
                (ResourceType::Clues, 2),
            ],
            colliders,
            pick_radius: 4.5,
            pick_height: 9.0,
        }
    }
}

/// Категория строительного элемента для расчета структурной опоры (Valheim Structural Integrity)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
pub enum StructuralKind {
    Floor,
    Wall,
    Pillar,
    Roof,
}

/// Компонент структурной целостности элемента постройки (по принципу Valheim)
#[derive(Component, Clone, Debug)]
#[allow(dead_code)]
pub struct StructuralPiece {
    pub kind: StructuralKind,
    pub world_pos: Vec3,
    pub floor_index: i32,
    pub max_support: f32,
    pub current_support: f32,
    /// Потеря стабильности при передаче вверх (`0.12` = 12% за ярус)
    pub vertical_loss: f32,
    /// Потеря стабильности при передаче по горизонтали (`0.24` = 24% за пролет)
    pub horizontal_loss: f32,
}

impl StructuralPiece {
    pub fn new(kind: StructuralKind, world_pos: Vec3, floor_index: i32) -> Self {
        let (max_support, vertical_loss, horizontal_loss) = match kind {
            StructuralKind::Pillar => (140.0, 0.08, 0.20),
            StructuralKind::Wall => (120.0, 0.12, 0.22),
            StructuralKind::Floor => (100.0, 0.18, 0.25),
            StructuralKind::Roof => (90.0, 0.20, 0.28),
        };
        Self {
            kind,
            world_pos,
            floor_index,
            max_support,
            current_support: max_support,
            vertical_loss,
            horizontal_loss,
        }
    }

    /// Относительный запас прочности опоры в `[0.0 .. 1.0]`
    /// (1.0 = синий фундамент на земле, >0.5 = зеленый, >0.25 = желтый, <0.10 = красный/обрушение)
    pub fn support_ratio(&self) -> f32 {
        (self.current_support / self.max_support).clamp(0.0, 1.0)
    }

    #[allow(dead_code)]
    pub fn stability_label(&self) -> &'static str {
        let r = self.support_ratio();
        if r >= 0.95 {
            "Фундамент (100%)"
        } else if r >= 0.60 {
            "Надежная опора"
        } else if r >= 0.30 {
            "Напряженная конструкция"
        } else {
            "Критический предел!"
        }
    }
}

/// Ресурс-флаг, сигнализирующий о необходимости пересчитать граф опоры зданий
#[derive(Resource, Default)]
pub struct StructuralIntegrityDirty(pub bool);

/// Компонент элемента, потерявшего опору и находящегося в процессе каскадного обрушения
#[derive(Component)]
pub struct PendingStructuralCollapse {
    pub timer: f32,
}

/// Визуальная вибрация объекта при получении удара топором/киркой
#[derive(Component)]
pub struct HitShake {
    pub timer: f32,
    pub base_scale: Vec3,
}

/// Летящий осколок разрушенного объекта
#[derive(Component)]
pub struct DebrisParticle {
    pub velocity: Vec3,
    pub lifetime: f32,
}

/// Выпавший на землю ресурс (лут), который притягивается к Сыщику и пополняет `PlayerInventory`
#[derive(Component)]
pub struct DroppedLoot {
    pub resource: ResourceType,
    pub amount: u32,
    pub velocity: Vec3,
    pub age: f32,
}

/// Высаженный игроком росток Лавкрафтовских Щупалец (ферма из `GameDoc.md`), производящий Страхи
#[derive(Component)]
pub struct TentacleCrop {
    pub growth: f32,
    pub harvest_ready: bool,
}

/// Наносит урон разрушаемому объекту; если `hp <= 0`, сносит его, освобождает `NavWorld`,
/// спавнит осколки, лут и запускает проверку каскадного обрушения верхних этажей!
#[allow(clippy::too_many_arguments)]
pub fn apply_damage_to_entity(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    loot_models: Option<&ResourceModelAssets>,
    vfx: Option<&SharedVfxAssets>,
    nav_world: &mut NavWorld,
    integrity_dirty: &mut StructuralIntegrityDirty,
    target_entity: Entity,
    world_pos: Vec3,
    destructible: &mut Destructible,
    damage: f32,
) -> bool {
    destructible.hp -= damage;

    // Спавним немного щепок/осколков при каждом ударе
    spawn_debris_burst(
        commands,
        meshes,
        materials,
        vfx,
        world_pos + Vec3::Y * 0.9,
        destructible.material.debris_color(),
        Some(destructible.material),
        4,
    );

    if destructible.hp > 0.0 {
        commands.entity(target_entity).insert(HitShake {
            timer: 0.18,
            base_scale: Vec3::ONE,
        });
        return false;
    }

    // Объект полностью разрушен!
    nav_world.remove_colliders(&destructible.colliders);
    integrity_dirty.0 = true;

    // Крупный взрыв обломков
    spawn_debris_burst(
        commands,
        meshes,
        materials,
        vfx,
        world_pos + Vec3::Y * 1.1,
        destructible.material.debris_color(),
        Some(destructible.material),
        12,
    );

    // Спавним выпавшие ресурсы (3D-модели лута)
    spawn_loot_drops(
        commands,
        meshes,
        materials,
        loot_models,
        vfx,
        world_pos + Vec3::Y * 0.6,
        &destructible.drops,
    );

    commands.entity(target_entity).despawn();
    true
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_debris_burst(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    vfx: Option<&SharedVfxAssets>,
    origin: Vec3,
    color: Color,
    material_kind: Option<DestructibleMaterial>,
    count: usize,
) {
    let (cube_mesh, mat) = if let Some(vfx) = vfx {
        let mat = if let Some(kind) = material_kind {
            vfx.debris_material_for(kind)
        } else {
            vfx.default_debris_mat.clone()
        };
        (vfx.debris_mesh.clone(), mat)
    } else {
        (
            meshes.add(Cuboid::new(0.22, 0.22, 0.22)),
            materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.9,
                ..default()
            }),
        )
    };

    for i in 0..count {
        let angle = (i as f32 / count as f32) * std::f32::consts::TAU + (i as f32 * 1.37);
        let speed = 2.2 + (i % 4) as f32 * 0.9;
        let vel = Vec3::new(
            angle.cos() * speed,
            3.2 + (i % 3) as f32 * 1.1,
            angle.sin() * speed,
        );
        commands.spawn((
            DebrisParticle {
                velocity: vel,
                lifetime: 0.55 + (i % 3) as f32 * 0.15,
            },
            Mesh3d(cube_mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(origin)
                .with_rotation(Quat::from_euler(EulerRot::XYZ, angle, angle * 0.7, 0.3)),
        ));
    }
}

pub fn spawn_loot_drops(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    loot_models: Option<&ResourceModelAssets>,
    vfx: Option<&SharedVfxAssets>,
    origin: Vec3,
    drops: &[(ResourceType, u32)],
) {
    let fallback_mesh = if let Some(vfx) = vfx {
        vfx.fallback_loot_mesh.clone()
    } else {
        meshes.add(Cuboid::new(0.32, 0.32, 0.32))
    };
    let mut drop_idx = 0_usize;

    for &(res_type, total_amount) in drops {
        if total_amount == 0 {
            continue;
        }

        // Спавним каждый кусочек как отдельную физическую модель,
        // чтобы они разлетались веером и красиво лежали на земле
        let spawn_count = total_amount.min(6);
        let per_drop = total_amount / spawn_count;
        let mut remainder = total_amount % spawn_count;

        for _ in 0..spawn_count {
            let count = per_drop + if remainder > 0 { remainder -= 1; 1 } else { 0 };
            if count == 0 {
                continue;
            }

            let angle = (drop_idx as f32) * 1.35 + 0.45;
            let speed = 1.4 + (drop_idx % 3) as f32 * 0.45;
            let vel = Vec3::new(
                angle.cos() * speed,
                3.4 + (drop_idx % 2) as f32 * 0.5,
                angle.sin() * speed,
            );

            let mut entity_cmds = commands.spawn((
                DroppedLoot {
                    resource: res_type,
                    amount: count,
                    velocity: vel,
                    age: 0.0,
                },
                Transform::from_translation(origin).with_scale(Vec3::splat(1.15)),
            ));

            if let Some(models) = loot_models {
                let model_handle = match res_type {
                    ResourceType::Stone | ResourceType::Soil => models.stone.clone(),
                    ResourceType::Wood => models.wood.clone(),
                    ResourceType::Foliage => models.foliage.clone(),
                    ResourceType::Fear => models.spores.clone(),
                    ResourceType::Clues => models.wood.clone(),
                };
                entity_cmds.insert(WorldAssetRoot(model_handle));
            } else {
                let mat = if let Some(vfx) = vfx {
                    vfx.loot_material_for(res_type)
                } else {
                    materials.add(StandardMaterial {
                        base_color: res_type.color(),
                        emissive: res_type.color().into(),
                        perceptual_roughness: 0.4,
                        ..default()
                    })
                };
                entity_cmds.insert((
                    Mesh3d(fallback_mesh.clone()),
                    MeshMaterial3d(mat),
                ));
            }

            drop_idx += 1;
        }
    }
}

/// Система пересчета структурной опоры всех блоков зданий (Valheim Structural Integrity)
/// Если нижние стены/колонны снесены или земля под фундаментом глубоко раскопана,
/// верхние этажи и крыши теряют опору и каскадно обрушиваются!
pub fn recompute_structural_integrity_system(
    mut commands: Commands,
    mut dirty: ResMut<StructuralIntegrityDirty>,
    terrain: Res<WorldTerrain>,
    mut pieces_query: Query<(Entity, &mut StructuralPiece), Without<PendingStructuralCollapse>>,
) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;

    let mut nodes: Vec<(Entity, StructuralPiece)> = pieces_query
        .iter_mut()
        .map(|(e, p)| (e, p.clone()))
        .collect();

    if nodes.is_empty() {
        return;
    }

    // 1. Помещаем блоки в 3D-сетку и определяем элементы, опирающиеся на грунт (OPT-04)
    let mut grid: std::collections::HashMap<(i32, i32, i32), Vec<usize>> =
        std::collections::HashMap::new();
    for (idx, (_, piece)) in nodes.iter().enumerate() {
        let cx = (piece.world_pos.x / 4.6).floor() as i32;
        let cy = (piece.world_pos.y / 4.6).floor() as i32;
        let cz = (piece.world_pos.z / 4.6).floor() as i32;
        grid.entry((cx, cy, cz)).or_default().push(idx);
    }

    let mut queue = std::collections::BinaryHeap::new();
    for (idx, (_, piece)) in nodes.iter_mut().enumerate() {
        let ground_y = terrain.sample_height(piece.world_pos.x, piece.world_pos.z);
        let dist_to_ground = (piece.world_pos.y - ground_y).abs();
        if piece.floor_index == 0 && dist_to_ground <= 0.95 {
            piece.current_support = piece.max_support;
            queue.push(((piece.max_support * 100.0) as u32, idx));
        } else {
            piece.current_support = 0.0;
        }
    }

    // 2. Локальный волновой алгоритм распространения опоры (Max-Heap Dijkstra propagation)
    while let Some((supp_int, src_idx)) = queue.pop() {
        let src_supp = supp_int as f32 / 100.0;
        if src_supp < nodes[src_idx].1.current_support - 0.05 {
            continue;
        }

        let src_pos = nodes[src_idx].1.world_pos;
        let cx = (src_pos.x / 4.6).floor() as i32;
        let cy = (src_pos.y / 4.6).floor() as i32;
        let cz = (src_pos.z / 4.6).floor() as i32;

        for nx in (cx - 1)..=(cx + 1) {
            for ny in (cy - 1)..=(cy + 2) {
                for nz in (cz - 1)..=(cz + 1) {
                    if let Some(bucket) = grid.get(&(nx, ny, nz)) {
                        for &target_idx in bucket {
                            if target_idx == src_idx {
                                continue;
                            }
                            let target = &nodes[target_idx].1;
                            let dxz = Vec2::new(
                                target.world_pos.x - src_pos.x,
                                target.world_pos.z - src_pos.z,
                            )
                            .length();
                            let dy = target.world_pos.y - src_pos.y;

                            let transferred = if dy >= 0.5 && dy <= 5.2 && dxz <= 4.6 {
                                (nodes[src_idx].1.current_support * (1.0 - target.vertical_loss))
                                    .min(target.max_support)
                            } else if dy.abs() < 1.2 && dxz <= 4.6 {
                                (nodes[src_idx].1.current_support * (1.0 - target.horizontal_loss))
                                    .min(target.max_support)
                            } else {
                                0.0
                            };

                            if transferred > nodes[target_idx].1.current_support + 0.1 {
                                nodes[target_idx].1.current_support = transferred;
                                queue.push(((transferred * 100.0) as u32, target_idx));
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Записываем вычисленную стабильность обратно и помечаем элементы без опоры на обрушение
    for (entity, computed) in nodes {
        if let Ok((_, mut piece)) = pieces_query.get_mut(entity) {
            piece.current_support = computed.current_support;
            if computed.current_support < 12.0 {
                // Задержка зависит от высоты, чтобы обрушение шло красивым каскадом снизу вверх
                let delay = 0.12 + (computed.floor_index.max(0) as f32) * 0.18;
                commands
                    .entity(entity)
                    .insert(PendingStructuralCollapse { timer: delay });
            }
        }
    }
}

/// Система каскадного обрушения блоков, потерявших опору, а также анимации ударов, осколков и сбора лута
#[allow(clippy::too_many_arguments)]
pub fn update_destruction_effects_system(
    mut commands: Commands,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    loot_models: Option<Res<ResourceModelAssets>>,
    vfx: Option<Res<SharedVfxAssets>>,
    mut nav_world: ResMut<NavWorld>,
    mut integrity_dirty: ResMut<StructuralIntegrityDirty>,
    mut inventory: ResMut<PlayerInventory>,
    terrain: Res<WorldTerrain>,
    player_query: Query<&Transform, With<Player>>,
    mut collapsing_query: Query<
        (
            Entity,
            &mut PendingStructuralCollapse,
            &Transform,
            Option<&mut Destructible>,
        ),
        Without<Player>,
    >,
    mut shake_query: Query<
        (Entity, &mut HitShake, &mut Transform),
        (Without<Player>, Without<PendingStructuralCollapse>),
    >,
    mut debris_query: Query<
        (Entity, &mut DebrisParticle, &mut Transform),
        (
            Without<Player>,
            Without<PendingStructuralCollapse>,
            Without<HitShake>,
        ),
    >,
    mut loot_query: Query<
        (Entity, &mut DroppedLoot, &mut Transform),
        (
            Without<Player>,
            Without<PendingStructuralCollapse>,
            Without<HitShake>,
            Without<DebrisParticle>,
        ),
    >,
    mut crop_query: Query<
        (&mut TentacleCrop, &mut Transform),
        (
            Without<Player>,
            Without<PendingStructuralCollapse>,
            Without<HitShake>,
            Without<DebrisParticle>,
            Without<DroppedLoot>,
        ),
    >,
) {
    let dt = time.delta_secs();
    let player_pos = player_query
        .single()
        .map(|tf| tf.translation)
        .unwrap_or(Vec3::ZERO);

    // 1. Обработка каскадного обрушения конструкций без опоры
    for (entity, mut collapse, tf, dest_opt) in &mut collapsing_query {
        collapse.timer -= dt;
        if collapse.timer <= 0.0 {
            if let Some(mut dest) = dest_opt {
                let mut dummy_dirty = StructuralIntegrityDirty(false);
                apply_damage_to_entity(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    loot_models.as_deref(),
                    vfx.as_deref(),
                    &mut nav_world,
                    &mut dummy_dirty,
                    entity,
                    tf.translation,
                    &mut dest,
                    9999.0,
                );
                integrity_dirty.0 = true;
            } else {
                commands.entity(entity).despawn();
            }
        }
    }

    // 2. Вибрация объектов при ударе
    for (entity, mut shake, mut tf) in &mut shake_query {
        shake.timer -= dt;
        if shake.timer <= 0.0 {
            tf.scale = shake.base_scale;
            commands.entity(entity).remove::<HitShake>();
        } else {
            let wave = (shake.timer * 55.0).sin() * 0.06;
            tf.scale = shake.base_scale * Vec3::new(1.0 + wave, 1.0 - wave * 0.5, 1.0 + wave);
        }
    }

    // 3. Физика разлетающихся осколков
    for (entity, mut debris, mut tf) in &mut debris_query {
        debris.lifetime -= dt;
        if debris.lifetime <= 0.0 {
            commands.entity(entity).despawn();
        } else {
            debris.velocity.y -= 14.0 * dt;
            tf.translation += debris.velocity * dt;
            tf.rotate_local_x(4.0 * dt);
            tf.rotate_local_y(3.0 * dt);
        }
    }

    // 4. Физика выпавших ресурсов (DroppedLoot) и магнитное притяжение к Сыщику
    for (entity, mut loot, mut tf) in &mut loot_query {
        loot.age += dt;
        let ground_y = terrain.sample_height(tf.translation.x, tf.translation.z) + 0.25;

        if tf.translation.y > ground_y || loot.velocity.y > 0.0 {
            loot.velocity.y -= 12.0 * dt;
            tf.translation += loot.velocity * dt;
            if tf.translation.y < ground_y {
                tf.translation.y = ground_y;
                loot.velocity = Vec3::ZERO;
            }
        }

        tf.rotate_y(2.5 * dt);

        // Притяжение к игроку с расстояния 5.5м и автоматический подбор
        let to_player = player_pos + Vec3::Y * 0.8 - tf.translation;
        let dist = to_player.length();
        if loot.age > 0.25 && dist < 5.5 {
            let pull_speed = 9.5;
            tf.translation += to_player.normalize_or_zero() * pull_speed * dt;
            if dist < 0.85 {
                inventory.add(loot.resource, loot.amount);
                commands.entity(entity).despawn();
            }
        }
    }

    // 5. Рост высаженных Щупалец на ферме
    for (mut crop, mut tf) in &mut crop_query {
        if !crop.harvest_ready {
            crop.growth = (crop.growth + dt * 0.18).min(1.0);
            let s = 0.35 + crop.growth * 0.65;
            tf.scale = Vec3::splat(s);
            if crop.growth >= 1.0 {
                crop.harvest_ready = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inventory_and_structural_piece_support() {
        let mut inv = PlayerInventory::default();
        let initial_wood = inv.get(ResourceType::Wood);
        inv.add(ResourceType::Wood, 5);
        assert_eq!(inv.get(ResourceType::Wood), initial_wood + 5);
        assert!(inv.spend(&[(ResourceType::Wood, 3)]));
        assert_eq!(inv.get(ResourceType::Wood), initial_wood + 2);

        let mut roof = StructuralPiece::new(StructuralKind::Roof, Vec3::new(0.0, 8.8, 0.0), 2);
        assert_eq!(roof.support_ratio(), 1.0);
        roof.current_support = 8.0;
        assert!(roof.support_ratio() < 0.15);
    }

    #[test]
    fn test_demolition_clears_nav_collider_and_cascades_support() {
        let mut nav = NavWorld::default();
        // Ставим стену в (0.0, 10.0)
        let wall_col = nav.add_box_on_floor(0.0, 10.0, 2.0, 0.38, 0);
        assert!(
            !nav.is_walkable_pos_on_floor(Vec2::new(0.0, 10.0), 0, 0.36),
            "Стена должна блокировать проход до разрушения"
        );

        // Разрушаем стену -> удаляем её коллайдер
        nav.remove_colliders(&[wall_col]);
        assert!(
            nav.is_walkable_pos_on_floor(Vec2::new(0.0, 10.0), 0, 0.36),
            "После сноса стены через пролом можно свободно пройти"
        );
    }

    #[test]
    fn test_tree_and_bush_destruction_drops_and_duration() {
        // Обычное дерево: 100 HP (10 ударов * 0.5с = 5.0с рубки), дроп 3 дерева и 3 листвы
        let tree_normal = Destructible::tree(false, vec![]);
        assert_eq!(tree_normal.hp, 100.0);
        assert_eq!(
            tree_normal.drops,
            vec![(ResourceType::Wood, 3), (ResourceType::Foliage, 3)]
        );

        // Зараженное дерево: +2 споры
        let tree_infected = Destructible::tree(true, vec![]);
        assert_eq!(
            tree_infected.drops,
            vec![
                (ResourceType::Wood, 3),
                (ResourceType::Foliage, 3),
                (ResourceType::Fear, 2)
            ]
        );

        // Обычный кустарник: дроп 2 листвы и 1 дерево
        let bush_normal = Destructible::bush(false, vec![]);
        assert_eq!(bush_normal.hp, 30.0);
        assert_eq!(
            bush_normal.drops,
            vec![(ResourceType::Foliage, 2), (ResourceType::Wood, 1)]
        );

        // Зараженный кустарник: +1 спора
        let bush_infected = Destructible::bush(true, vec![]);
        assert_eq!(
            bush_infected.drops,
            vec![
                (ResourceType::Foliage, 2),
                (ResourceType::Wood, 1),
                (ResourceType::Fear, 1)
            ]
        );
    }
}
