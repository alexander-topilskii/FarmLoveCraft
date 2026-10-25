use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use serde::{Deserialize, Serialize};

use crate::atmosphere::WorldClock;
use crate::camera::CameraController;
use crate::defense::{DefenseWaveState, DetectiveEnergy};
use crate::destruction::PlayerInventory;
use crate::player::Player;
use crate::rts_command::RtsContextMenuRoot;

pub const SAVE_FILE_NAME: &str = "savegame.json";

/// Состояние открытости и статуса главного меню
#[derive(Resource)]
pub struct GameMenuState {
    pub is_open: bool,
    pub status_message: String,
    pub status_timer: f32,
}

impl Default for GameMenuState {
    fn default() -> Self {
        Self {
            is_open: false,
            status_message: "Готово к сохранению".to_string(),
            status_timer: 0.0,
        }
    }
}

/// Сериализуемый прогресс игрока и состояния мира
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SaveData {
    pub version: u32,
    pub timestamp_sec: u64,
    pub player_pos: [f32; 3],
    pub player_floor: i32,
    pub inventory: SavedInventory,
    pub energy: SavedEnergy,
    pub defense_wave: SavedDefenseWave,
    pub camera: SavedCamera,
    #[serde(default)]
    pub time_of_day: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SavedInventory {
    pub wood: u32,
    pub stone: u32,
    pub soil: u32,
    pub fear: u32,
    pub clues: u32,
    pub foliage: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SavedEnergy {
    pub current: f32,
    pub max: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SavedDefenseWave {
    pub is_active: bool,
    pub wave_number: u32,
    pub status_text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SavedCamera {
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_degrees: f32,
}

/// Запись структуры сохранения в файл
pub fn save_game_to_file(path: &str, data: &SaveData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

/// Чтение структуры сохранения из файла
pub fn load_game_from_file(path: &str) -> Result<SaveData, String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let data = serde_json::from_str::<SaveData>(&json).map_err(|e| e.to_string())?;
    Ok(data)
}

/// Маркер контейнера верхней информационной панели (скрывается/открывается по Tab)
#[derive(Component)]
pub struct TopHudRoot;

/// Маркер кнопки-переключателя [Tab] Инфо в левом верхнем углу
#[derive(Component)]
pub struct TopHudTabButton;

/// Текст внутри кнопки-переключателя [Tab] Инфо
#[derive(Component)]
pub struct TopHudTabText;

/// Маркер корневого оверлея главного игрового меню
#[derive(Component)]
pub struct GameMenuRoot;

/// Маркер кнопки вызова меню [⚙ Меню / Esc] в правом верхнем углу
#[derive(Component)]
pub struct MenuToggleButton;

/// Действие, закреплённое за интерактивной кнопкой в UI меню
#[derive(Component, Clone, Debug, PartialEq)]
pub enum MenuButtonAction {
    SaveGame,
    LoadGame,
    CameraDistanceStep(f32),
    CameraDistanceSet(f32),
    CameraFovStep(f32),
    CameraFovSet(f32),
    SetTimeOfDay(f32),
    CloseMenu,
}

/// Компонент подсветки кнопки при наведении курсора
#[derive(Component, Clone, Copy)]
pub struct MenuHoverable {
    pub base_color: Color,
    pub hover_color: Color,
}

/// Метка текущего значения дистанции камеры в меню
#[derive(Component)]
pub struct MenuDistanceLabel;

/// Метка текущего значения FOV в меню
#[derive(Component)]
pub struct MenuFovLabel;

/// Метка текущего времени суток в меню
#[derive(Component)]
pub struct MenuTimeLabel;

/// Метка строки статуса сохранения/загрузки
#[derive(Component)]
pub struct MenuStatusLabel;

/// Спавн постоянных кнопок интерфейса: [Tab] Инфо (слева) и [⚙ Меню] (справа), а также модального меню
pub fn setup_game_menu_ui(mut commands: Commands) {
    // 1. Компактная кнопка-бейдж переключения Tab слева сверху
    commands
        .spawn((
            TopHudTabButton,
            MenuHoverable {
                base_color: Color::srgba(0.08, 0.12, 0.15, 0.92),
                hover_color: Color::srgba(0.18, 0.28, 0.24, 0.98),
            },
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(16.0),
                height: Val::Px(26.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.2)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.12, 0.15, 0.92)),
            BorderColor::all(Color::srgb(0.35, 0.52, 0.42)),
        ))
        .with_children(|btn| {
            btn.spawn((
                TopHudTabText,
                Text::new("[Tab] ℹ Инфо"),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(0.88, 0.92, 0.85)),
            ));
        });

    // 2. Кнопка вызова меню в правом верхнем углу [⚙ Меню (Esc)]
    commands
        .spawn((
            MenuToggleButton,
            MenuHoverable {
                base_color: Color::srgba(0.12, 0.14, 0.18, 0.92),
                hover_color: Color::srgba(0.24, 0.28, 0.36, 0.98),
            },
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                right: Val::Px(16.0),
                height: Val::Px(26.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.2)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.12, 0.14, 0.18, 0.92)),
            BorderColor::all(Color::srgb(0.68, 0.58, 0.35)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("⚙ Меню [Esc]"),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.88, 0.65)),
            ));
        });

    // 3. Модальное окно меню (по умолчанию скрыто)
    commands
        .spawn((
            GameMenuRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            Visibility::Hidden,
        ))
        .with_children(|backdrop| {
            backdrop
                .spawn((
                    Node {
                        width: Val::Px(540.0),
                        padding: UiRect::all(Val::Px(22.0)),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(14.0),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.06, 0.08, 0.11, 0.97)),
                    BorderColor::all(Color::srgb(0.65, 0.55, 0.32)),
                ))
                .with_children(|menu| {
                    // Заголовок
                    menu.spawn((
                        Text::new("МЕНЮ ИГРЫ И НАСТРОЙКИ"),
                        TextFont {
                            font_size: FontSize::Px(18.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.96, 0.88, 0.58)),
                    ));

                    // Статусная строка
                    menu.spawn((
                        MenuStatusLabel,
                        Text::new("✓ Готово к сохранению"),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.45, 0.90, 0.65)),
                    ));

                    // Разделитель
                    spawn_menu_divider(menu);

                    // Секция 1: Сохранение прогресса
                    menu.spawn((
                        Text::new("1. Сохранение прогресса:"),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.88, 0.84, 0.72)),
                    ));

                    menu.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(10.0),
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_action_button(
                            row,
                            "💾 Сохранить прогресс [F5]",
                            MenuButtonAction::SaveGame,
                            Color::srgba(0.16, 0.32, 0.22, 0.95),
                            Color::srgba(0.24, 0.48, 0.32, 1.0),
                            Val::Px(230.0),
                        );
                        spawn_action_button(
                            row,
                            "📂 Загрузить прогресс [F9]",
                            MenuButtonAction::LoadGame,
                            Color::srgba(0.18, 0.24, 0.30, 0.95),
                            Color::srgba(0.28, 0.38, 0.48, 1.0),
                            Val::Px(230.0),
                        );
                    });

                    // Разделитель
                    spawn_menu_divider(menu);

                    // Секция 2: Поле зрения и Дистанция камеры (делать дальше/ближе)
                    menu.spawn((
                        Text::new("2. Поле зрения и обзор (делать дальше/ближе):"),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.88, 0.84, 0.72)),
                    ));

                    // Ряд 1: Дистанция камеры
                    menu.spawn((
                        MenuDistanceLabel,
                        Text::new("Дистанция камеры: 24.0 м"),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.60, 0.88, 0.75)),
                    ));

                    menu.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_action_button(
                            row,
                            "◀ Ближе (-3м)",
                            MenuButtonAction::CameraDistanceStep(-3.0),
                            Color::srgba(0.18, 0.22, 0.26, 0.95),
                            Color::srgba(0.28, 0.34, 0.40, 1.0),
                            Val::Px(110.0),
                        );
                        spawn_action_button(
                            row,
                            "Дальше (+3м) ▶",
                            MenuButtonAction::CameraDistanceStep(3.0),
                            Color::srgba(0.18, 0.22, 0.26, 0.95),
                            Color::srgba(0.28, 0.34, 0.40, 1.0),
                            Val::Px(110.0),
                        );
                        spawn_action_button(
                            row,
                            "14м",
                            MenuButtonAction::CameraDistanceSet(14.0),
                            Color::srgba(0.14, 0.16, 0.20, 0.95),
                            Color::srgba(0.24, 0.28, 0.34, 1.0),
                            Val::Px(56.0),
                        );
                        spawn_action_button(
                            row,
                            "24м",
                            MenuButtonAction::CameraDistanceSet(24.0),
                            Color::srgba(0.14, 0.16, 0.20, 0.95),
                            Color::srgba(0.24, 0.28, 0.34, 1.0),
                            Val::Px(56.0),
                        );
                        spawn_action_button(
                            row,
                            "40м",
                            MenuButtonAction::CameraDistanceSet(40.0),
                            Color::srgba(0.14, 0.16, 0.20, 0.95),
                            Color::srgba(0.24, 0.28, 0.34, 1.0),
                            Val::Px(56.0),
                        );
                        spawn_action_button(
                            row,
                            "60м",
                            MenuButtonAction::CameraDistanceSet(60.0),
                            Color::srgba(0.14, 0.16, 0.20, 0.95),
                            Color::srgba(0.24, 0.28, 0.34, 1.0),
                            Val::Px(56.0),
                        );
                    });

                    // Ряд 2: Угол обзора FOV
                    menu.spawn((
                        MenuFovLabel,
                        Text::new("Угол обзора (FOV): 45°"),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.60, 0.88, 0.75)),
                    ));

                    menu.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_action_button(
                            row,
                            "◀ Уже (-5°)",
                            MenuButtonAction::CameraFovStep(-5.0),
                            Color::srgba(0.18, 0.22, 0.26, 0.95),
                            Color::srgba(0.28, 0.34, 0.40, 1.0),
                            Val::Px(110.0),
                        );
                        spawn_action_button(
                            row,
                            "Шире (+5°) ▶",
                            MenuButtonAction::CameraFovStep(5.0),
                            Color::srgba(0.18, 0.22, 0.26, 0.95),
                            Color::srgba(0.28, 0.34, 0.40, 1.0),
                            Val::Px(110.0),
                        );
                        spawn_action_button(
                            row,
                            "45°",
                            MenuButtonAction::CameraFovSet(45.0),
                            Color::srgba(0.14, 0.16, 0.20, 0.95),
                            Color::srgba(0.24, 0.28, 0.34, 1.0),
                            Val::Px(56.0),
                        );
                        spawn_action_button(
                            row,
                            "60°",
                            MenuButtonAction::CameraFovSet(60.0),
                            Color::srgba(0.14, 0.16, 0.20, 0.95),
                            Color::srgba(0.24, 0.28, 0.34, 1.0),
                            Val::Px(56.0),
                        );
                        spawn_action_button(
                            row,
                            "75°",
                            MenuButtonAction::CameraFovSet(75.0),
                            Color::srgba(0.14, 0.16, 0.20, 0.95),
                            Color::srgba(0.24, 0.28, 0.34, 1.0),
                            Val::Px(56.0),
                        );
                        spawn_action_button(
                            row,
                            "90°",
                            MenuButtonAction::CameraFovSet(90.0),
                            Color::srgba(0.14, 0.16, 0.20, 0.95),
                            Color::srgba(0.24, 0.28, 0.34, 1.0),
                            Val::Px(56.0),
                        );
                    });

                    // Разделитель
                    spawn_menu_divider(menu);

                    // Секция 3: Время суток в игре
                    menu.spawn((
                        Text::new("3. Время суток в игре:"),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.88, 0.84, 0.72)),
                    ));

                    // Метка текущего времени
                    menu.spawn((
                        MenuTimeLabel,
                        Text::new("Время суток: 17:30 (Кровавый Закат)"),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.96, 0.75, 0.40)),
                    ));

                    menu.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_action_button(
                            row,
                            "🌙 00:00 (12 ночи)",
                            MenuButtonAction::SetTimeOfDay(0.0),
                            Color::srgba(0.12, 0.14, 0.24, 0.95),
                            Color::srgba(0.20, 0.26, 0.44, 1.0),
                            Val::Px(118.0),
                        );
                        spawn_action_button(
                            row,
                            "🌅 06:00 (6 утра)",
                            MenuButtonAction::SetTimeOfDay(6.0),
                            Color::srgba(0.24, 0.20, 0.14, 0.95),
                            Color::srgba(0.38, 0.30, 0.20, 1.0),
                            Val::Px(118.0),
                        );
                        spawn_action_button(
                            row,
                            "☀️ 12:00 (12 дня)",
                            MenuButtonAction::SetTimeOfDay(12.0),
                            Color::srgba(0.14, 0.24, 0.28, 0.95),
                            Color::srgba(0.22, 0.38, 0.44, 1.0),
                            Val::Px(118.0),
                        );
                        spawn_action_button(
                            row,
                            "🌇 18:00 (6 вечера)",
                            MenuButtonAction::SetTimeOfDay(18.0),
                            Color::srgba(0.28, 0.16, 0.14, 0.95),
                            Color::srgba(0.44, 0.24, 0.20, 1.0),
                            Val::Px(118.0),
                        );
                    });

                    // Разделитель
                    spawn_menu_divider(menu);

                    // Кнопка закрытия
                    spawn_action_button(
                        menu,
                        "✖ Вернуться в игру [Esc]",
                        MenuButtonAction::CloseMenu,
                        Color::srgba(0.28, 0.18, 0.16, 0.95),
                        Color::srgba(0.44, 0.24, 0.20, 1.0),
                        Val::Percent(100.0),
                    );
                });
        });
}

fn spawn_menu_divider(parent: &mut ChildSpawnerCommands) {
    parent.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(1.0),
            margin: UiRect::axes(Val::Px(0.0), Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.45, 0.40, 0.30, 0.50)),
    ));
}

fn spawn_action_button(
    parent: &mut ChildSpawnerCommands,
    title: &str,
    action: MenuButtonAction,
    base_color: Color,
    hover_color: Color,
    width: Val,
) {
    parent
        .spawn((
            action,
            MenuHoverable {
                base_color,
                hover_color,
            },
            Node {
                width,
                height: Val::Px(28.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(base_color),
            BorderColor::all(Color::srgba(0.55, 0.50, 0.40, 0.65)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(title),
                TextFont {
                    font_size: FontSize::Px(12.5),
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.92, 0.90)),
            ));
        });
}

/// Проверка, попадает ли курсор мыши в пределы UI-элемента (учитывает физические и логические координаты)
pub fn is_node_hovered(
    c_node: &ComputedNode,
    ui_tf: &UiGlobalTransform,
    cursor_phys: Option<Vec2>,
    cursor_logical: Option<Vec2>,
    scale: f32,
) -> bool {
    if c_node.size().x <= 0.0 || c_node.size().y <= 0.0 {
        return false;
    }
    if let Some(phys) = cursor_phys {
        if c_node.contains_point(*ui_tf, phys) {
            return true;
        }
    }
    if let Some(logical) = cursor_logical {
        if c_node.contains_point(*ui_tf, logical * scale) || c_node.contains_point(*ui_tf, logical) {
            return true;
        }
    }
    false
}

/// Система переключения видимости верхней информационной панели по клавише Tab или клику мыши на кнопку
pub fn toggle_top_hud_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    tab_button_query: Query<
        (
            &UiGlobalTransform,
            &ComputedNode,
            &MenuHoverable,
            &mut BackgroundColor,
        ),
        With<TopHudTabButton>,
    >,
    mut hud_query: Query<&mut Visibility, With<TopHudRoot>>,
    mut text_query: Query<&mut Text, With<TopHudTabText>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let cursor_phys = window.physical_cursor_position();
    let cursor_logical = window.cursor_position();
    let scale = window.scale_factor();

    let mut button_clicked = false;
    for (ui_tf, c_node, hoverable, mut bg) in &mut tab_button_query.into_iter() {
        let hovered = is_node_hovered(c_node, ui_tf, cursor_phys, cursor_logical, scale);
        if hovered {
            *bg = BackgroundColor(hoverable.hover_color);
            if mouse_button.just_pressed(MouseButton::Left) {
                button_clicked = true;
            }
        } else {
            *bg = BackgroundColor(hoverable.base_color);
        }
    }

    if keyboard_input.just_pressed(KeyCode::Tab) || button_clicked {
        for mut vis in &mut hud_query {
            let new_vis = match *vis {
                Visibility::Hidden => Visibility::Inherited,
                _ => Visibility::Hidden,
            };
            *vis = new_vis;

            for mut text in &mut text_query {
                if new_vis == Visibility::Hidden {
                    **text = "[Tab] ℹ Инфо".to_string();
                } else {
                    **text = "[Tab] ✕ Скрыть инфо".to_string();
                }
            }
        }
    }
}

/// Система открытия/закрытия игрового меню по Escape, KeyM или кнопке в правом углу
pub fn toggle_game_menu_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    mut menu_state: ResMut<GameMenuState>,
    rts_menus: Query<Entity, With<RtsContextMenuRoot>>,
    toggle_btn_query: Query<
        (
            &UiGlobalTransform,
            &ComputedNode,
            &MenuHoverable,
            &mut BackgroundColor,
        ),
        With<MenuToggleButton>,
    >,
    mut menu_root_query: Query<&mut Visibility, With<GameMenuRoot>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let cursor_phys = window.physical_cursor_position();
    let cursor_logical = window.cursor_position();
    let scale = window.scale_factor();

    let mut toggle_clicked = false;
    for (ui_tf, c_node, hoverable, mut bg) in &mut toggle_btn_query.into_iter() {
        let hovered = is_node_hovered(c_node, ui_tf, cursor_phys, cursor_logical, scale);
        if hovered {
            *bg = BackgroundColor(hoverable.hover_color);
            if mouse_button.just_pressed(MouseButton::Left) {
                toggle_clicked = true;
            }
        } else {
            *bg = BackgroundColor(hoverable.base_color);
        }
    }

    // Escape закрывает контекстное RTS меню в первую очередь, если оно открыто
    let escape_pressed = keyboard_input.just_pressed(KeyCode::Escape) && rts_menus.is_empty();
    let m_pressed = keyboard_input.just_pressed(KeyCode::KeyM);

    if escape_pressed || m_pressed || toggle_clicked {
        menu_state.is_open = !menu_state.is_open;
        for mut vis in &mut menu_root_query {
            *vis = if menu_state.is_open {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Обработка кликов и наведений по кнопкам модального меню
#[allow(clippy::too_many_arguments)]
pub fn game_menu_interaction_system(
    time: Res<Time>,
    windows: Query<&Window>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut menu_state: ResMut<GameMenuState>,
    mut menu_root_query: Query<&mut Visibility, With<GameMenuRoot>>,
    buttons_query: Query<
        (
            &UiGlobalTransform,
            &ComputedNode,
            &MenuButtonAction,
            &MenuHoverable,
            &mut BackgroundColor,
        ),
        Without<TopHudTabButton>,
    >,
    mut camera_query: Query<(&mut CameraController, &mut Projection), With<Camera3d>>,
    mut player_query: Query<(&mut Transform, &mut Player)>,
    mut inventory: ResMut<PlayerInventory>,
    mut energy: ResMut<DetectiveEnergy>,
    mut wave_state: ResMut<DefenseWaveState>,
    mut clock: Option<ResMut<WorldClock>>,
) {
    if !menu_state.is_open {
        return;
    }

    // Таймер показа статусного сообщения
    if menu_state.status_timer > 0.0 {
        menu_state.status_timer -= time.delta_secs();
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let cursor_phys = window.physical_cursor_position();
    let cursor_logical = window.cursor_position();
    let scale = window.scale_factor();
    let lmb_pressed = mouse_button.just_pressed(MouseButton::Left);

    let mut chosen_action: Option<MenuButtonAction> = None;

    // Быстрые клавиши внутри меню: F5 — сохранить, F9 — загрузить
    if keyboard_input.just_pressed(KeyCode::F5) {
        chosen_action = Some(MenuButtonAction::SaveGame);
    } else if keyboard_input.just_pressed(KeyCode::F9) {
        chosen_action = Some(MenuButtonAction::LoadGame);
    } else if keyboard_input.just_pressed(KeyCode::Minus)
        || keyboard_input.just_pressed(KeyCode::NumpadSubtract)
    {
        chosen_action = Some(MenuButtonAction::CameraDistanceStep(-3.0));
    } else if keyboard_input.just_pressed(KeyCode::Equal)
        || keyboard_input.just_pressed(KeyCode::NumpadAdd)
    {
        chosen_action = Some(MenuButtonAction::CameraDistanceStep(3.0));
    }

    for (ui_tf, c_node, action, hoverable, mut bg) in &mut buttons_query.into_iter() {
        let hovered = is_node_hovered(c_node, ui_tf, cursor_phys, cursor_logical, scale);

        if hovered {
            *bg = BackgroundColor(hoverable.hover_color);
            if lmb_pressed {
                chosen_action = Some(action.clone());
            }
        } else {
            *bg = BackgroundColor(hoverable.base_color);
        }
    }

    let Some(action) = chosen_action else {
        return;
    };

    match action {
        MenuButtonAction::SaveGame => {
            let Ok((p_transform, player)) = player_query.single() else {
                menu_state.status_message = "Ошибка: игрок не найден в мире".to_string();
                menu_state.status_timer = 4.0;
                return;
            };
            let (cam_distance, cam_yaw, cam_pitch, fov_deg) =
                if let Ok((ctrl, proj)) = camera_query.single() {
                    let fov = match *proj {
                        Projection::Perspective(ref p) => p.fov.to_degrees(),
                        _ => 45.0,
                    };
                    (ctrl.distance, ctrl.yaw, ctrl.pitch, fov)
                } else {
                    (24.0, 0.0, 42.0_f32.to_radians(), 45.0)
                };

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let save_data = SaveData {
                version: 1,
                timestamp_sec: now,
                player_pos: [
                    p_transform.translation.x,
                    p_transform.translation.y,
                    p_transform.translation.z,
                ],
                player_floor: player.current_floor,
                inventory: SavedInventory {
                    wood: inventory.wood,
                    stone: inventory.stone,
                    soil: inventory.soil,
                    fear: inventory.fear,
                    clues: inventory.clues,
                    foliage: inventory.foliage,
                },
                energy: SavedEnergy {
                    current: energy.current,
                    max: energy.max,
                },
                defense_wave: SavedDefenseWave {
                    is_active: wave_state.is_active,
                    wave_number: wave_state.wave_number,
                    status_text: wave_state.status_text.clone(),
                },
                camera: SavedCamera {
                    distance: cam_distance,
                    yaw: cam_yaw,
                    pitch: cam_pitch,
                    fov_degrees: fov_deg,
                },
                time_of_day: clock.as_ref().map(|c| c.time_of_day),
            };

            match save_game_to_file(SAVE_FILE_NAME, &save_data) {
                Ok(_) => {
                    menu_state.status_message = format!(
                        "✓ Успешно сохранено в {}! (Дерево: {}, Камень: {}, Волна: {})",
                        SAVE_FILE_NAME, inventory.wood, inventory.stone, wave_state.wave_number
                    );
                    menu_state.status_timer = 5.0;
                }
                Err(err) => {
                    menu_state.status_message = format!("Ошибка сохранения: {}", err);
                    menu_state.status_timer = 5.0;
                }
            }
        }
        MenuButtonAction::LoadGame => {
            match load_game_from_file(SAVE_FILE_NAME) {
                Ok(data) => {
                    if let Ok((mut p_transform, mut player)) = player_query.single_mut() {
                        p_transform.translation = Vec3::new(
                            data.player_pos[0],
                            data.player_pos[1],
                            data.player_pos[2],
                        );
                        player.current_floor = data.player_floor;
                        player.path.clear();
                        player.keyboard_vel = Vec3::ZERO;
                    }
                    inventory.wood = data.inventory.wood;
                    inventory.stone = data.inventory.stone;
                    inventory.soil = data.inventory.soil;
                    inventory.fear = data.inventory.fear;
                    inventory.clues = data.inventory.clues;
                    inventory.foliage = data.inventory.foliage;

                    energy.current = data.energy.current;
                    energy.max = data.energy.max;

                    wave_state.is_active = data.defense_wave.is_active;
                    wave_state.wave_number = data.defense_wave.wave_number;
                    wave_state.status_text = data.defense_wave.status_text;

                    if let Ok((mut ctrl, mut proj)) = camera_query.single_mut() {
                        ctrl.distance = data.camera.distance;
                        ctrl.yaw = data.camera.yaw;
                        ctrl.pitch = data.camera.pitch;
                        ctrl.current_focus = None;
                        if let Projection::Perspective(ref mut persp) = *proj {
                            persp.fov = data.camera.fov_degrees.to_radians();
                        }
                    }

                    if let Some(saved_tod) = data.time_of_day {
                        if let Some(ref mut c) = clock {
                            c.time_of_day = saved_tod;
                        }
                    }

                    menu_state.status_message = format!(
                        "✓ Прогресс загружен из {}! (Волна: {}, Ресурсы восстановлены)",
                        SAVE_FILE_NAME, wave_state.wave_number
                    );
                    menu_state.status_timer = 5.0;
                }
                Err(err) => {
                    menu_state.status_message = format!("Ошибка загрузки: {}", err);
                    menu_state.status_timer = 5.0;
                }
            }
        }
        MenuButtonAction::CameraDistanceStep(delta) => {
            if let Ok((mut ctrl, _)) = camera_query.single_mut() {
                ctrl.distance = (ctrl.distance + delta).clamp(8.0, 70.0);
            }
        }
        MenuButtonAction::CameraDistanceSet(dist) => {
            if let Ok((mut ctrl, _)) = camera_query.single_mut() {
                ctrl.distance = dist.clamp(8.0, 70.0);
            }
        }
        MenuButtonAction::CameraFovStep(delta_deg) => {
            if let Ok((_, mut proj)) = camera_query.single_mut() {
                if let Projection::Perspective(ref mut persp) = *proj {
                    let deg = (persp.fov.to_degrees() + delta_deg).clamp(30.0, 105.0);
                    persp.fov = deg.to_radians();
                }
            }
        }
        MenuButtonAction::CameraFovSet(deg) => {
            if let Ok((_, mut proj)) = camera_query.single_mut() {
                if let Projection::Perspective(ref mut persp) = *proj {
                    persp.fov = deg.clamp(30.0, 105.0).to_radians();
                }
            }
        }
        MenuButtonAction::SetTimeOfDay(hour) => {
            if let Some(ref mut c) = clock {
                c.time_of_day = hour.clamp(0.0, 24.0) % 24.0;
                let phase = c.phase_name();
                let hour_int = c.time_of_day.floor() as u32;
                menu_state.status_message = format!("✓ Установлено время: {:02}:00 ({})", hour_int, phase);
                menu_state.status_timer = 4.0;
            }
        }
        MenuButtonAction::CloseMenu => {
            menu_state.is_open = false;
            for mut vis in &mut menu_root_query {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// Динамическое обновление меток дистанции, FOV, времени и статуса в открытом меню
pub fn update_game_menu_labels_system(
    menu_state: Res<GameMenuState>,
    camera_query: Query<(&CameraController, &Projection), With<Camera3d>>,
    clock: Option<Res<WorldClock>>,
    mut dist_labels: Query<&mut Text, (With<MenuDistanceLabel>, Without<MenuFovLabel>, Without<MenuStatusLabel>, Without<MenuTimeLabel>)>,
    mut fov_labels: Query<&mut Text, (With<MenuFovLabel>, Without<MenuDistanceLabel>, Without<MenuStatusLabel>, Without<MenuTimeLabel>)>,
    mut time_labels: Query<&mut Text, (With<MenuTimeLabel>, Without<MenuDistanceLabel>, Without<MenuFovLabel>, Without<MenuStatusLabel>)>,
    mut status_labels: Query<&mut Text, (With<MenuStatusLabel>, Without<MenuDistanceLabel>, Without<MenuFovLabel>, Without<MenuTimeLabel>)>,
) {
    if !menu_state.is_open {
        return;
    }

    if let Ok((ctrl, proj)) = camera_query.single() {
        let fov_deg = match *proj {
            Projection::Perspective(ref p) => p.fov.to_degrees(),
            _ => 45.0,
        };

        for mut text in &mut dist_labels {
            **text = format!("Дистанция камеры: {:.1} м", ctrl.distance);
        }

        for mut text in &mut fov_labels {
            **text = format!("Поле зрения (FOV): {:.0}°", fov_deg);
        }
    }

    if let Some(ref c) = clock {
        let hours = c.time_of_day.floor() as u32;
        let mins = ((c.time_of_day.fract()) * 60.0).floor() as u32;
        let phase = c.phase_name();
        for mut text in &mut time_labels {
            **text = format!("Время суток: {:02}:{:02} ({})", hours, mins, phase);
        }
    }

    for mut text in &mut status_labels {
        **text = menu_state.status_message.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_load_serialization_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let save_path = temp_dir.join("test_savegame.json");
        let path_str = save_path.to_str().unwrap();

        let original = SaveData {
            version: 1,
            timestamp_sec: 1700000000,
            player_pos: [12.5, 4.0, -8.0],
            player_floor: 1,
            inventory: SavedInventory {
                wood: 42,
                stone: 18,
                soil: 9,
                fear: 11,
                clues: 7,
                foliage: 5,
            },
            energy: SavedEnergy {
                current: 85.5,
                max: 100.0,
            },
            defense_wave: SavedDefenseWave {
                is_active: true,
                wave_number: 3,
                status_text: "Волна 3 активна".to_string(),
            },
            camera: SavedCamera {
                distance: 35.0,
                yaw: 1.25,
                pitch: 0.75,
                fov_degrees: 60.0,
            },
            time_of_day: Some(17.5),
        };

        save_game_to_file(path_str, &original).expect("Сохранение должно успешно записаться");
        let loaded = load_game_from_file(path_str).expect("Файл сохранения должен успешно прочитаться");

        assert_eq!(original, loaded);
        let _ = std::fs::remove_file(save_path);
    }

    #[test]
    fn test_menu_state_defaults() {
        let state = GameMenuState::default();
        assert!(!state.is_open);
        assert!(!state.status_message.is_empty());
    }

    #[test]
    fn test_camera_distance_and_fov_clamping() {
        // Проверка ограничений дистанции зума: [8.0, 70.0]
        let clamp_dist = |d: f32, delta: f32| (d + delta).clamp(8.0, 70.0);
        assert_eq!(clamp_dist(24.0, -3.0), 21.0);
        assert_eq!(clamp_dist(9.0, -3.0), 8.0);
        assert_eq!(clamp_dist(68.0, 5.0), 70.0);

        // Проверка ограничений угла обзора FOV: [30.0, 105.0] градусов
        let clamp_fov = |deg: f32, delta: f32| (deg + delta).clamp(30.0, 105.0);
        assert_eq!(clamp_fov(45.0, 5.0), 50.0);
        assert_eq!(clamp_fov(32.0, -5.0), 30.0);
        assert_eq!(clamp_fov(102.0, 10.0), 105.0);
    }

    #[test]
    fn test_top_hud_tab_toggle_system() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();

        // Окно для запроса Query<&Window>
        app.world_mut().spawn(Window::default());

        // TopHudRoot по умолчанию Hidden
        let hud_entity = app
            .world_mut()
            .spawn((TopHudRoot, Visibility::Hidden))
            .id();
        let tab_text_entity = app
            .world_mut()
            .spawn((TopHudTabText, Text::new("[Tab] ℹ Инфо")))
            .id();

        app.add_systems(Update, toggle_top_hud_system);

        // До нажатия Tab
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(hud_entity).unwrap(),
            Visibility::Hidden
        );

        // Нажимаем Tab
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();

        // Панель стала видимой, текст обновился на скрыть
        assert_eq!(
            *app.world().get::<Visibility>(hud_entity).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            app.world().get::<Text>(tab_text_entity).unwrap().as_str(),
            "[Tab] ✕ Скрыть инфо"
        );

        // Отпускаем и снова нажимаем Tab
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();

        // Снова скрылась
        assert_eq!(
            *app.world().get::<Visibility>(hud_entity).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            app.world().get::<Text>(tab_text_entity).unwrap().as_str(),
            "[Tab] ℹ Инфо"
        );
    }

    #[test]
    fn test_game_menu_toggle_system() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<GameMenuState>();

        app.world_mut().spawn(Window::default());

        let menu_root_entity = app
            .world_mut()
            .spawn((GameMenuRoot, Visibility::Hidden))
            .id();

        app.add_systems(Update, toggle_game_menu_system);

        // Начальное состояние — закрыто
        app.update();
        assert!(!app.world().resource::<GameMenuState>().is_open);
        assert_eq!(
            *app.world().get::<Visibility>(menu_root_entity).unwrap(),
            Visibility::Hidden
        );

        // Нажимаем Escape
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();

        // Меню открылось
        assert!(app.world().resource::<GameMenuState>().is_open);
        assert_eq!(
            *app.world().get::<Visibility>(menu_root_entity).unwrap(),
            Visibility::Inherited
        );

        // Нажимаем KeyM для закрытия
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyM);
        app.update();

        // Меню закрылось
        assert!(!app.world().resource::<GameMenuState>().is_open);
        assert_eq!(
            *app.world().get::<Visibility>(menu_root_entity).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn test_game_menu_camera_actions_and_hotkeys() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<GameMenuState>();
        app.init_resource::<PlayerInventory>();
        app.init_resource::<DetectiveEnergy>();
        app.init_resource::<DefenseWaveState>();
        app.init_resource::<Time>();

        app.world_mut().spawn(Window::default());

        let cam_entity = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                CameraController {
                    distance: 24.0,
                    ..default()
                },
                Projection::Perspective(PerspectiveProjection {
                    fov: 45.0_f32.to_radians(),
                    ..default()
                }),
            ))
            .id();

        app.world_mut().spawn((
            Transform::default(),
            Player {
                speed: 5.0,
                current_floor: 0,
                path: default(),
                keyboard_vel: Vec3::ZERO,
                is_moving: false,
            },
        ));

        app.add_systems(Update, game_menu_interaction_system);

        // Открываем меню
        app.world_mut().resource_mut::<GameMenuState>().is_open = true;

        // Тестируем горячую клавишу минус (уменьшить дистанцию камеры на 3м)
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Minus);
        app.update();

        let ctrl = app.world().get::<CameraController>(cam_entity).unwrap();
        assert_eq!(ctrl.distance, 21.0);

        // Тестируем горячую клавишу плюс (увеличить дистанцию камеры на 3м)
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Equal);
        app.update();

        let ctrl = app.world().get::<CameraController>(cam_entity).unwrap();
        assert_eq!(ctrl.distance, 24.0);

        // Тестируем F5 (сохранение в файл)
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F5);
        app.update();

        let status = &app.world().resource::<GameMenuState>().status_message;
        assert!(status.contains("✓ Успешно сохранено"));
        assert!(std::path::Path::new(SAVE_FILE_NAME).exists());
        let _ = std::fs::remove_file(SAVE_FILE_NAME);
    }

    #[test]
    fn test_menu_toggle_and_tab_button_mouse_clicks() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<GameMenuState>();

        // Окно с курсором
        let mut window = Window::default();
        window.set_cursor_position(Some(Vec2::new(20.0, 20.0)));
        let win_entity = app.world_mut().spawn(window).id();

        // 1. Тестируем TopHudTabButton
        let hud_entity = app
            .world_mut()
            .spawn((TopHudRoot, Visibility::Hidden))
            .id();
        let tab_text_entity = app
            .world_mut()
            .spawn((TopHudTabText, Text::new("[Tab] ℹ Инфо")))
            .id();
        let tab_btn_entity = app
            .world_mut()
            .spawn((
                TopHudTabButton,
                ComputedNode {
                    size: Vec2::new(60.0, 30.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(20.0, 20.0),
                MenuHoverable {
                    base_color: Color::BLACK,
                    hover_color: Color::WHITE,
                },
                BackgroundColor(Color::BLACK),
            ))
            .id();

        app.add_systems(Update, (toggle_top_hud_system, toggle_game_menu_system));

        // Курсор над кнопкой Tab, нажимаем ЛКМ
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();

        // Проверяем, что клик сработал: HUD показан, текст изменился, цвет подсветки hover_color
        assert_eq!(
            *app.world().get::<Visibility>(hud_entity).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            app.world().get::<Text>(tab_text_entity).unwrap().as_str(),
            "[Tab] ✕ Скрыть инфо"
        );
        assert_eq!(
            app.world().get::<BackgroundColor>(tab_btn_entity).unwrap().0,
            Color::WHITE
        );

        // 2. Тестируем MenuToggleButton
        let menu_root_entity = app
            .world_mut()
            .spawn((GameMenuRoot, Visibility::Hidden))
            .id();
        let toggle_btn_entity = app
            .world_mut()
            .spawn((
                MenuToggleButton,
                ComputedNode {
                    size: Vec2::new(80.0, 30.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(200.0, 20.0),
                MenuHoverable {
                    base_color: Color::srgb(0.1, 0.1, 0.1),
                    hover_color: Color::srgb(0.9, 0.9, 0.9),
                },
                BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
            ))
            .id();

        // Перемещаем курсор на MenuToggleButton (200.0, 20.0) и кликаем ЛКМ
        app.world_mut()
            .get_mut::<Window>(win_entity)
            .unwrap()
            .set_cursor_position(Some(Vec2::new(200.0, 20.0)));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();

        // Проверяем, что меню открылось
        assert!(app.world().resource::<GameMenuState>().is_open);
        assert_eq!(
            *app.world().get::<Visibility>(menu_root_entity).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            app.world().get::<BackgroundColor>(toggle_btn_entity).unwrap().0,
            Color::srgb(0.9, 0.9, 0.9)
        );
    }

    #[test]
    fn test_game_menu_modal_action_button_mouse_clicks() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<GameMenuState>();
        app.init_resource::<PlayerInventory>();
        app.init_resource::<DetectiveEnergy>();
        app.init_resource::<DefenseWaveState>();
        app.init_resource::<Time>();

        let mut window = Window::default();
        window.set_cursor_position(Some(Vec2::new(100.0, 100.0)));
        let win_entity = app.world_mut().spawn(window).id();

        let cam_entity = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                CameraController {
                    distance: 24.0,
                    ..default()
                },
                Projection::Perspective(PerspectiveProjection {
                    fov: 45.0_f32.to_radians(),
                    ..default()
                }),
            ))
            .id();

        app.world_mut().spawn((
            Transform::default(),
            Player {
                speed: 5.0,
                current_floor: 0,
                path: default(),
                keyboard_vel: Vec3::ZERO,
                is_moving: false,
            },
        ));

        let menu_root_entity = app
            .world_mut()
            .spawn((GameMenuRoot, Visibility::Inherited))
            .id();

        // Создаем кнопку изменения дистанции камеры (-3м)
        app.world_mut().spawn((
            MenuButtonAction::CameraDistanceStep(-3.0),
            ComputedNode {
                size: Vec2::new(100.0, 30.0),
                ..default()
            },
            UiGlobalTransform::from_xy(100.0, 100.0),
            MenuHoverable {
                base_color: Color::BLACK,
                hover_color: Color::WHITE,
            },
            BackgroundColor(Color::BLACK),
        ));

        // Создаем кнопку закрытия меню
        app.world_mut().spawn((
            MenuButtonAction::CloseMenu,
            ComputedNode {
                size: Vec2::new(100.0, 30.0),
                ..default()
            },
            UiGlobalTransform::from_xy(300.0, 100.0),
            MenuHoverable {
                base_color: Color::BLACK,
                hover_color: Color::WHITE,
            },
            BackgroundColor(Color::BLACK),
        ));

        app.add_systems(Update, game_menu_interaction_system);

        // Открываем меню
        app.world_mut().resource_mut::<GameMenuState>().is_open = true;

        // Кликаем по первой кнопке (100.0, 100.0): дистанция камеры
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();

        let ctrl = app.world().get::<CameraController>(cam_entity).unwrap();
        assert_eq!(ctrl.distance, 21.0);

        // Перемещаем курсор на кнопку CloseMenu (300.0, 100.0) и кликаем
        app.world_mut()
            .get_mut::<Window>(win_entity)
            .unwrap()
            .set_cursor_position(Some(Vec2::new(300.0, 100.0)));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();

        // Меню должно закрыться
        assert!(!app.world().resource::<GameMenuState>().is_open);
        assert_eq!(
            *app.world().get::<Visibility>(menu_root_entity).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn test_game_menu_time_of_day_actions_and_labels() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<GameMenuState>();
        app.init_resource::<PlayerInventory>();
        app.init_resource::<DetectiveEnergy>();
        app.init_resource::<DefenseWaveState>();
        app.init_resource::<Time>();
        app.insert_resource(WorldClock {
            time_of_day: 17.5,
            time_speed: 0.0,
            is_paused: false,
        });

        let mut window = Window::default();
        window.set_cursor_position(Some(Vec2::new(100.0, 100.0)));
        let win_entity = app.world_mut().spawn(window).id();

        app.world_mut().spawn((
            Camera3d::default(),
            CameraController::default(),
            Projection::default(),
        ));
        app.world_mut().spawn((
            Transform::default(),
            Player {
                speed: 5.0,
                current_floor: 0,
                path: default(),
                keyboard_vel: Vec3::ZERO,
                is_moving: false,
            },
        ));

        // Метка времени
        let time_label_entity = app
            .world_mut()
            .spawn((MenuTimeLabel, Text::new("")))
            .id();

        // 4 кнопки времени: 00:00 (12 ночи), 06:00 (6 утра), 12:00 (12 дня), 18:00 (6 вечера)
        let times = [
            (0.0, Vec2::new(50.0, 50.0)),
            (6.0, Vec2::new(150.0, 50.0)),
            (12.0, Vec2::new(250.0, 50.0)),
            (18.0, Vec2::new(350.0, 50.0)),
        ];

        for (tod, pos) in times {
            app.world_mut().spawn((
                MenuButtonAction::SetTimeOfDay(tod),
                ComputedNode {
                    size: Vec2::new(80.0, 30.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(pos.x, pos.y),
                MenuHoverable {
                    base_color: Color::BLACK,
                    hover_color: Color::WHITE,
                },
                BackgroundColor(Color::BLACK),
            ));
        }

        app.add_systems(
            Update,
            (game_menu_interaction_system, update_game_menu_labels_system).chain(),
        );

        // Открываем меню
        app.world_mut().resource_mut::<GameMenuState>().is_open = true;

        // 1. Клик по 00:00 (12 ночи)
        app.world_mut()
            .get_mut::<Window>(win_entity)
            .unwrap()
            .set_cursor_position(Some(Vec2::new(50.0, 50.0)));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<WorldClock>().time_of_day, 0.0);
        assert_eq!(
            app.world().get::<Text>(time_label_entity).unwrap().as_str(),
            "Время суток: 00:00 (Ночь Древних)"
        );

        // 2. Клик по 06:00 (6 утра)
        app.world_mut()
            .get_mut::<Window>(win_entity)
            .unwrap()
            .set_cursor_position(Some(Vec2::new(150.0, 50.0)));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<WorldClock>().time_of_day, 6.0);
        assert_eq!(
            app.world().get::<Text>(time_label_entity).unwrap().as_str(),
            "Время суток: 06:00 (Туманный Рассвет)"
        );

        // 3. Клик по 12:00 (12 дня)
        app.world_mut()
            .get_mut::<Window>(win_entity)
            .unwrap()
            .set_cursor_position(Some(Vec2::new(250.0, 50.0)));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<WorldClock>().time_of_day, 12.0);
        assert_eq!(
            app.world().get::<Text>(time_label_entity).unwrap().as_str(),
            "Время суток: 12:00 (Пасмурный День)"
        );

        // 4. Клик по 18:00 (6 вечера)
        app.world_mut()
            .get_mut::<Window>(win_entity)
            .unwrap()
            .set_cursor_position(Some(Vec2::new(350.0, 50.0)));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<WorldClock>().time_of_day, 18.0);
        assert_eq!(
            app.world().get::<Text>(time_label_entity).unwrap().as_str(),
            "Время суток: 18:00 (Кровавый Закат)"
        );
    }
}
