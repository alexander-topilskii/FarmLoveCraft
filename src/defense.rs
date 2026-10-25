use bevy::prelude::*;
use crate::destruction::{
    spawn_debris_burst, spawn_loot_drops, Destructible, DestructibleMaterial, ResourceType,
    SharedVfxAssets, TentacleCrop,
};
use crate::navigation::NavWorld;
use crate::player::Player;
use crate::water_cellular::WaterWorld;
use crate::world::dungeons::catacombs::{AltarOfSummoning, DefenseEnemy, EnemyKind, VoidRiftPortal};

/// Ресурс энергии / концентрации Сыщика для использования умений Саппорта/Хилера (Lineage II style)
#[derive(Resource, Debug, Clone)]
pub struct DetectiveEnergy {
    pub current: f32,
    pub max: f32,
    pub regen_rate: f32,
}

impl Default for DetectiveEnergy {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
            regen_rate: 8.0,
        }
    }
}

/// Бафф ускорения и усиления на боевом растении или постройке
#[derive(Component, Debug, Clone)]
pub struct EmpowerBuff {
    pub timer: f32,
    pub attack_speed_mult: f32,
}

/// Дебафф паники/страха от фонаря Сыщика (заставляет врагов бежать назад)
#[derive(Component, Debug, Clone)]
pub struct FearedDebuff {
    pub timer: f32,
}

/// Состояние оборонного ритуала
#[derive(Resource, Debug, Clone, Default)]
pub struct DefenseWaveState {
    pub is_active: bool,
    pub wave_number: u32,
    pub status_text: String,
}

/// Маркер текстового UI панели способностей саппорта (хоткеи 1-5)
#[derive(Component)]
pub struct SupportSkillbarText;

/// Система управления Ритуалом Призыва на Алтаре Катакомб и спавном волн (Фаза 3)
pub fn altar_ritual_defense_system(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut wave_state: ResMut<DefenseWaveState>,
    player_query: Query<&Transform, With<Player>>,
    mut altar_query: Query<(&Transform, &mut AltarOfSummoning)>,
    mut portals_query: Query<&mut VoidRiftPortal>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    let player_pos = player_query.single().map(|tf| tf.translation).unwrap_or(Vec3::ZERO);

    let Ok((altar_tf, mut altar)) = altar_query.single_mut() else {
        return;
    };

    let dist_to_altar = player_pos.distance(altar_tf.translation);

    // Клавиша [E] около Алтаря (или [F]) запускает Ритуал Призыва волн
    if dist_to_altar < 4.5 && !altar.ritual_active {
        if keyboard.just_pressed(KeyCode::KeyE) || keyboard.just_pressed(KeyCode::KeyF) {
            altar.ritual_active = true;
            altar.current_wave = 1;
            altar.wave_countdown = 2.0;
            wave_state.is_active = true;
            wave_state.wave_number = 1;
            wave_state.status_text = "Ритуал Призыва начат! Враги приближаются по коридорам!".to_string();

            // Взрыв мистического света при начале ритуала
            commands.spawn((
                PointLight {
                    color: Color::srgb(0.95, 0.25, 0.35),
                    intensity: 120_000.0,
                    range: 18.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_translation(altar_tf.translation + Vec3::Y * 2.0),
            ));
        }
    }

    if !altar.ritual_active {
        return;
    }

    if altar.wave_countdown > 0.0 {
        altar.wave_countdown -= dt;
        if altar.wave_countdown <= 0.0 {
            // Заряжаем порталы врагами для текущей волны
            let count_per_portal = 4 + altar.current_wave * 2;
            for mut portal in &mut portals_query {
                portal.pending_spawns += count_per_portal;
                portal.spawn_timer = 0.5;
            }
            wave_state.status_text = format!(
                "Волна {}/{}! Защищайте Алтарь Бездны (HP: {:.0}/{:.0})",
                altar.current_wave, altar.total_waves, altar.altar_hp, altar.max_hp
            );
        }
    }
}

/// Спавнит врагов из активных Разломов Бездны (`VoidRiftPortal`)
pub fn void_rift_spawner_system(
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut portals_query: Query<(&Transform, &mut VoidRiftPortal)>,
) {
    let dt = time.delta_secs();

    let enemy_mesh = meshes.add(Cuboid::new(0.85, 1.45, 0.85));
    let deep_one_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.38, 0.32),
        perceptual_roughness: 0.65,
        ..default()
    });

    for (tf, mut portal) in &mut portals_query {
        if portal.pending_spawns == 0 {
            continue;
        }

        portal.spawn_timer -= dt;
        if portal.spawn_timer <= 0.0 {
            portal.spawn_timer = portal.spawn_interval;
            portal.pending_spawns -= 1;

            let spawn_pos = tf.translation + Vec3::new(0.0, 0.75, 0.0);
            commands.spawn((
                DefenseEnemy {
                    kind: EnemyKind::DeepOne,
                    hp: 60.0,
                    max_hp: 60.0,
                    speed: 2.4,
                    damage: 12.0,
                    attack_cooldown: 1.0,
                    water_push_force: Vec2::ZERO,
                },
                Mesh3d(enemy_mesh.clone()),
                MeshMaterial3d(deep_one_mat.clone()),
                Transform::from_translation(spawn_pos),
            ));
        }
    }
}

/// Интеллект врагов: следование по Flow Field к Алтарю, снос водой и атака ловушек/Алтаря
#[allow(clippy::too_many_arguments)]
pub fn defense_enemy_ai_system(
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    vfx: Option<Res<SharedVfxAssets>>,
    water_world: Res<WaterWorld>,
    nav_world: Res<NavWorld>,
    mut altar_query: Query<(&Transform, &mut AltarOfSummoning), Without<DefenseEnemy>>,
    mut enemies_query: Query<(
        Entity,
        &mut DefenseEnemy,
        &mut Transform,
        Option<&FearedDebuff>,
    ), Without<TentacleCrop>>,
    mut crops_query: Query<(&Transform, &mut Destructible, Option<&EmpowerBuff>), (With<TentacleCrop>, Without<DefenseEnemy>)>,
) {
    let dt = time.delta_secs();

    let Ok((altar_tf, mut altar)) = altar_query.single_mut() else {
        return;
    };
    let altar_pos = altar_tf.translation;
    let altar_xz = Vec2::new(altar_pos.x, altar_pos.z);

    for (entity, mut enemy, mut tf, feared) in &mut enemies_query {
        let enemy_xz = Vec2::new(tf.translation.x, tf.translation.z);

        // 1. Влияние течения воды Minecraft на физику перемещения монстров (Фаза 3, Шаг 3.3)
        let flow_vel = water_world.sample_flow_velocity(enemy_xz.x, enemy_xz.y);
        if flow_vel.length_squared() > 0.01 {
            enemy.water_push_force = Vec2::new(flow_vel.x, flow_vel.z);
        } else {
            enemy.water_push_force = enemy.water_push_force.lerp(Vec2::ZERO, dt * 5.0);
        }

        // 2. Вектор направления движения: Flow Field к Алтарю либо паническое бегство от фонаря
        let to_altar = (altar_xz - enemy_xz).normalize_or_zero();
        let move_dir = if feared.is_some() {
            -to_altar // Бегут назад при ослеплении фонарем!
        } else {
            to_altar
        };

        // Движение с учетом сопротивления/увлечения потоком воды
        let combined_velocity = move_dir * enemy.speed + enemy.water_push_force;
        let next_pos = enemy_xz + combined_velocity * dt;

        // Проверяем проходимость клетки
        if nav_world.is_walkable_pos_on_floor(next_pos, -1, 0.45)
            || nav_world.is_walkable_pos_on_floor(next_pos, 0, 0.45)
        {
            tf.translation.x = next_pos.x;
            tf.translation.z = next_pos.y;
        }

        if combined_velocity.length_squared() > 0.01 {
            tf.rotation = Quat::from_rotation_y(combined_velocity.x.atan2(combined_velocity.y));
        }

        // 3. Атака боевых растений (Щупалец фермы) по врагу
        for (crop_tf, mut crop_dest, empower) in &mut crops_query {
            let crop_xz = Vec2::new(crop_tf.translation.x, crop_tf.translation.z);
            let dist = enemy_xz.distance(crop_xz);
            if dist <= 2.8 {
                let speed_mult = empower.map(|e| e.attack_speed_mult).unwrap_or(1.0);
                let damage = 22.0 * dt * speed_mult;
                enemy.hp -= damage;

                // Враг также слегка повреждает растение
                crop_dest.hp = (crop_dest.hp - 4.0 * dt).max(0.0);
            }
        }

        // 4. Достижение Алтаря и нанесение урона
        let dist_to_altar = enemy_xz.distance(altar_xz);
        if dist_to_altar <= 2.2 && feared.is_none() {
            enemy.attack_cooldown -= dt;
            if enemy.attack_cooldown <= 0.0 {
                enemy.attack_cooldown = 1.0;
                altar.altar_hp = (altar.altar_hp - enemy.damage).max(0.0);
            }
        }

        // 5. Гибель врага -> спавн осколков и дроп Спор Страха
        if enemy.hp <= 0.0 {
            spawn_debris_burst(
                &mut commands,
                &mut meshes,
                &mut materials,
                vfx.as_deref(),
                tf.translation,
                Color::srgb(0.18, 0.72, 0.55),
                Some(DestructibleMaterial::Occult),
                8,
            );
            spawn_loot_drops(
                &mut commands,
                &mut meshes,
                &mut materials,
                None,
                vfx.as_deref(),
                tf.translation,
                &[(ResourceType::Fear, 2)],
            );
            commands.entity(entity).despawn();
        }
    }
}

/// Управление умениями Сыщика в роли Чистого Саппорта / Хилера / Инженера (Lineage II style, Фаза 4)
#[allow(clippy::too_many_arguments)]
pub fn support_caster_system(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut detective_energy: ResMut<DetectiveEnergy>,
    mut water_world: ResMut<WaterWorld>,
    mut commands: Commands,
    player_query: Query<&Transform, With<Player>>,
    mut altar_query: Query<(&Transform, &mut AltarOfSummoning)>,
    mut crops_query: Query<(Entity, &Transform, &mut Destructible), With<TentacleCrop>>,
    mut walls_query: Query<(Entity, &Transform, &mut Destructible), (Without<Player>, Without<TentacleCrop>)>,
    mut enemies_query: Query<(Entity, &Transform), With<DefenseEnemy>>,
) {
    let dt = time.delta_secs();

    // Регенерация энергии концентрации Сыщика
    detective_energy.current =
        (detective_energy.current + dt * detective_energy.regen_rate).min(detective_energy.max);

    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let player_pos = player_tf.translation;
    let player_xz = Vec2::new(player_pos.x, player_pos.z);

    // [1] Живительный Ихор (Greater Heal): исцеляет растения и Алтарь (20 энергии)
    if keyboard.just_pressed(KeyCode::Digit1) && detective_energy.current >= 20.0 {
        detective_energy.current -= 20.0;
        // Исцеляем поврежденные щупальца в радиусе 12м
        for (_, crop_tf, mut dest) in &mut crops_query {
            if player_pos.distance(crop_tf.translation) <= 12.0 {
                dest.hp = (dest.hp + 45.0).min(dest.max_hp);
            }
        }
        // Исцеляем Алтарь, если рядом
        if let Ok((altar_tf, mut altar)) = altar_query.single_mut() {
            if player_pos.distance(altar_tf.translation) <= 12.0 {
                altar.altar_hp = (altar.altar_hp + 60.0).min(altar.max_hp);
            }
        }
        // Вспышка целебного изумрудного света
        commands.spawn((
            PointLight {
                color: Color::srgb(0.25, 0.95, 0.60),
                intensity: 60_000.0,
                range: 12.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(player_pos + Vec3::Y * 1.5),
        ));
    }

    // [2] Мастерская Починка (Warsmith Repair): восстанавливает прочность укреплений (15 энергии)
    if keyboard.just_pressed(KeyCode::Digit2) && detective_energy.current >= 15.0 {
        detective_energy.current -= 15.0;
        for (_, wall_tf, mut dest) in &mut walls_query {
            if player_pos.distance(wall_tf.translation) <= 8.0 {
                dest.hp = (dest.hp + 75.0).min(dest.max_hp);
            }
        }
    }

    // [3] Печать Ярости Бездны (Prophet Haste): баффает растения и передается по воде! (25 энергии)
    if keyboard.just_pressed(KeyCode::Digit3) && detective_energy.current >= 25.0 {
        detective_energy.current -= 25.0;
        // Накладываем бафф на ближайшие растения
        for (crop_ent, crop_tf, _) in &mut crops_query {
            if player_pos.distance(crop_tf.translation) <= 10.0 {
                commands.entity(crop_ent).insert(EmpowerBuff {
                    timer: 20.0,
                    attack_speed_mult: 1.65,
                });
            }
        }
        // Если стоим у источника воды — пускаем бафф по течению водного канала!
        let grid = WaterWorld::world_to_grid(player_xz.x, player_xz.y);
        if let Some(cell) = water_world.cells.get_mut(&grid) {
            cell.buff_active = true;
            cell.buff_timer = 25.0;
        }
    }

    // [4] Вспышка Керосинового Фонаря (Mass Fear / Turn Undead): пугает толпу (30 энергии)
    if keyboard.just_pressed(KeyCode::Digit4) && detective_energy.current >= 30.0 {
        detective_energy.current -= 30.0;
        for (enemy_ent, enemy_tf) in &mut enemies_query {
            if player_pos.distance(enemy_tf.translation) <= 9.0 {
                commands.entity(enemy_ent).insert(FearedDebuff { timer: 3.5 });
            }
        }
        // Яркая ослепляющая золотистая вспышка
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.90, 0.45),
                intensity: 140_000.0,
                range: 16.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(player_pos + Vec3::Y * 1.5),
        ));
    }
}

/// Обновление таймеров баффов и дебаффов
pub fn update_buffs_and_debuffs_system(
    time: Res<Time>,
    mut commands: Commands,
    mut empower_query: Query<(Entity, &mut EmpowerBuff)>,
    mut feared_query: Query<(Entity, &mut FearedDebuff)>,
) {
    let dt = time.delta_secs();

    for (entity, mut buff) in &mut empower_query {
        buff.timer -= dt;
        if buff.timer <= 0.0 {
            commands.entity(entity).remove::<EmpowerBuff>();
        }
    }

    for (entity, mut debuff) in &mut feared_query {
        debuff.timer -= dt;
        if debuff.timer <= 0.0 {
            commands.entity(entity).remove::<FearedDebuff>();
        }
    }
}

/// Отображение панели умений Саппорта и энергии в HUD (хоткеи [1] Ихор, [2] Починка, [3] Ярость, [4] Фонарь)
pub fn update_support_skillbar_hud_system(
    detective_energy: Res<DetectiveEnergy>,
    wave_state: Res<DefenseWaveState>,
    mut text_query: Query<&mut Text, With<SupportSkillbarText>>,
) {
    let Ok(mut text) = text_query.single_mut() else {
        return;
    };

    let wave_str = if wave_state.is_active {
        format!(" | {}", wave_state.status_text)
    } else {
        " | [E] у Алтаря Катакомб — Начать Ритуал Обороны".to_string()
    };

    **text = format!(
        "Энергия Саппорта: {:.0}/{:.0} | [1] Живительный Ихор (-20) | [2] Починка (-15) | [3] Ярость Бездны (-25) | [4] Вспышка Фонаря (-30){}",
        detective_energy.current, detective_energy.max, wave_str
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_altar_and_detective_energy_defaults() {
        let altar = AltarOfSummoning::default();
        assert_eq!(altar.altar_hp, 500.0);
        assert!(!altar.ritual_active);

        let mut energy = DetectiveEnergy::default();
        assert_eq!(energy.current, 100.0);
        energy.current -= 30.0;
        assert_eq!(energy.current, 70.0);
    }
}
