# План оптимизации проекта FarmLoveCraft (`OPTIMIZATION_PLAN.md`)

Данный документ описывает детальный, поэтапный технический план оптимизации **FarmLoveCraft** (Bevy 0.20, Rust Edition 2024, Desktop + WebAssembly/WebGL2/WebGPU). План составлен на основе полного аудита текущей кодовой базы (`src/*.rs`, `Cargo.toml`, `assets/`).

---

## 1. Сводная матрица выявленных узких мест (Bottlenecks)

| ID | Подсистема / Файл | Проблема в текущей реализации | Сложность сейчас | Целевая сложность | Приоритет |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **OPT-01** | Навигация и коллизии (`src/navigation.rs`) | Плоский `Vec<ColliderRect>` в `NavWorld`. При каждом шаге A* (до 64 000 итераций × 8 соседей) и проверке видимости `has_line_of_sight_on_floor` происходит линейный перебор всех коллайдеров этажа. | $O(N_{\text{steps}} \cdot N_{\text{colliders}})$ | $O(N_{\text{steps}} \cdot 1)$ через Spatial Grid | **P0 (Критично)** |
| **OPT-02** | Срезание стен (`src/building.rs`) | `update_wall_cutaway_system` выполняется **каждый кадр** для всех стен (`CutawayWall`), колонн (`CutawayPillar`) и мебели (`CutawayAttachedProp`), вызывая `children_query.iter_descendants(entity)` по дереву GLB-сцены. | $O(N_{\text{pieces}} \cdot D_{\text{hierarchy}})$ каждый кадр | $O(1)$ в покое; $O(N_{\text{active\_building}})$ при смене угла/этажа | **P0 (Критично)** |
| **OPT-03** | Стриминг мира (`src/terrain.rs`) | `stream_terrain_chunks_system` только догружает чанки в радиусе `CHUNK_STREAM_RADIUS = 5`, но **никогда не выгружает** далекие чанки, их `Mesh`, процедурные объекты (`Destructible`) и коллайдеры в `NavWorld`. | Неограниченный рост памяти и числа `Entity` | Константное окно $(2R+1)^2$ активных чанков | **P0 (Критично)** |
| **OPT-04** | Целостность построек (`src/destruction.rs`) | `recompute_structural_integrity_system` клонирует все `StructuralPiece` в `Vec` и выполняет 10 проходов вложенного цикла `for i in 0..len { for j in 0..len }` по всем зданиям мира сразу. | $O(10 \cdot N^2)$ | $O(V + E)$ локальный BFS/Dijkstra по 3D-сетке/зданию | **P1 (Высокий)** |
| **OPT-05** | RTS-рейкастинг (`src/rts_command.rs`) | При клике ПКМ `rts_right_click_menu_system` перебирает все `Destructible` в мире и проверяет по 9 горизонтальных срезов `ray_cylinder_distance` для каждого объекта. | $O(9 \cdot N_{\text{destructibles}})$ | $O(K)$ через Spatial Hash вдоль луча + аналитический тест | **P1 (Высокий)** |
| **OPT-06** | Частицы и лут (`src/destruction.rs`) | `spawn_debris_burst` и `spawn_loot_drops` на каждый удар/снос вызывают `meshes.add(Cuboid::new(...))` и `materials.add(...)`, засоряя `Assets<Mesh>` и `Assets<StandardMaterial>` и ломая GPU batching. | Аллокация GPU-буферов на каждый удар | $O(1)$ через кэшированные `Handle` и пул частиц | **P1 (Высокий)** |
| **OPT-07** | UI и HUD (`src/rts_command.rs`) | `update_inventory_hud_system` каждый кадр вызывает `format!(...)` и перезаписывает `Text`, вызывая пересчет глифов и верстки в Bevy UI даже когда ресурсы не менялись. | Переверстка текста 60+ раз/сек | Обновление только по `Changed` / `is_changed()` | **P1 (Высокий)** |
| **OPT-08** | Освещение и рендер (`src/world.rs`, `src/rts_command.rs`) | Десятки постоянных `PointLight` (уличные фонари, канделябры, алтари, каждое выращенное щупальце `TentacleCrop`) перегружают кластерный пайплайн освещения (`ClusteredForward`), особенно в WebGL2/WASM. | Все источники света активны всегда | Дистанционный Culling света + Emissive материалы | **P2 (Средний)** |
| **OPT-09** | Планировщик ECS (`src/main.rs`) | Все 15 систем в `Update` объединены единым `.chain()`, что полностью отключает многопоточный параллелизм планировщика Bevy. | Однопоточное последовательное выполнение | Группировка в `SystemSet` с параллельным чтением | **P2 (Средний)** |
| **OPT-10** | Сборка и WASM (`Cargo.toml`) | Отсутствуют профили оптимизации (`[profile.dev.package."*"]`, `[profile.release]`, `wasm-release`), LTO, `codegen-units = 1` и сжатие GLB-ассетов. | Медленный debug и крупный `.wasm` бинарник | Быстрый dev-профиль и минимальный размер WASM | **P2 (Средний)** |

---

## 2. Детальный план работ по этапам

### Этап 1. Пространственная индексация, коллизии и A* навигация (`src/navigation.rs`)

#### 1.1. Внедрение пространственной хэш-сетки (`SpatialGrid`) в `NavWorld`
* **Проблема**: Поле `pub colliders: Vec<ColliderRect>` в `NavWorld` сканируется целиком при каждом вызове `is_walkable_pos_on_floor` и `resolve_collisions_on_floor`. По мере генерации новых чанков в `populate_wild_chunk` число коллайдеров растет до тысяч, что приводит к фризам при поиске пути A* (особенно когда цель далеко или заблокирована).
* **Решение**:
  1. Заменить хранение коллайдеров на `HashMap<ColliderId, ColliderRect>` + пространственный индекс по ячейкам фиксированного шага (например, чанк `4.0 × 4.0 м` и этаж `floor: i32`):
     `spatial_buckets: HashMap<(i32, i32, i32), SmallVec<[ColliderId; 8]>>`.
  2. При вызове `add_box_on_floor` регистрировать `ColliderId` во всех корзинах `(bx, bz, floor)`, которые пересекает AABB коллайдера (с запасом `PLAYER_RADIUS`).
  3. При вызове `remove_colliders` удалять `ColliderId` только из соответствующих корзин за $O(1)$.
  4. В `is_walkable_pos_on_floor(pos, floor, radius)` и `resolve_collisions_on_floor` проверять только 1 корзину `(bx, bz, floor)`, в которую попадает `pos`.

#### 1.2. Оптимизация алгоритма A* и трассировки видимости (`find_path`)
* **Проблема**:
  * В `find_path` на каждом вызове создаются новые `BinaryHeap`, `HashMap<NavCell, i32>` (`g_score`) и `HashMap<NavCell, NavCell>` (`came_from`).
  * Сглаживание пути в конце `find_path` вызывает `has_line_of_sight_on_floor` в цикле `((anchor_idx + 2)..raw_pts.len()).rev()`, делая квадратичное число проверок коллизий.
  * Лимит итераций `64_000` слишком велик для одного кадра при клике в недостижимую область.
* **Решение**:
  1. **Переиспользование буферов (Scratchpad)**: Вынести рабочие структуры A* (`open`, `g_score`, `came_from`) в отдельный ресурс или поле `PathfinderScratch` с очисткой `.clear()` без переаллокации памяти в куче.
  2. **Быстрый Hash (`FxHashMap` / `AHashMap`)**: Заменить стандартный криптостойкий `std::collections::HashMap` (SipHash) на `bevy::platform::collections::HashMap` (или `rustc_hash::FxHashMap`) для ключей `NavCell` и `IVec2`.
  3. **Иерархическое/жадное сглаживание пути**: Вместо проверки каждого узла `raw_pts` с конца проверять видимость методом бинарного поиска или шагом по ключевым точкам изгиба (corner points).
  4. **Адаптивный бюджет итераций**: Снизить верхний порог `iterations` до `8_000..12_000` для мгновенного отклика (не более 1–2 мс на кадр) с возвратом частичного пути к ближайшей исследованной точке (`best_fallback_cell` с минимальным `heuristic`), если цель замурована.

---

### Этап 2. Оптимизация системы срезания стен и иерархии зданий (`src/building.rs`)

#### 2.1. Кэширование дочерних мешей и отказ от ежекадрового `iter_descendants`
* **Проблема**: В `update_wall_cutaway_system` для каждой стены, колонны и шкафа вызывается `apply_entity_transparency`, которая выполняет `children_query.iter_descendants(root_entity)`. Поскольку GLB-модели в Bevy имеют вложенность `WorldAssetRoot -> SceneRoot -> Node -> MeshEntity`, обход дерева выполняется сотни раз каждый кадр.
* **Решение**:
  1. Добавить на сущности `CutawayWall`, `CutawayPillar`, `CutawayAttachedProp` компонент кэша мешей и текущего состояния:
     ```rust
     #[derive(Component, Default)]
     pub struct CutawayStateCache {
         pub is_transparent: bool,
         pub mesh_entities: SmallVec<[Entity; 4]>,
         pub initialized: bool,
     }
     ```
  2. Заполнять `mesh_entities` один раз (при срабатывании триггера `WorldInstanceReady` или лениво при первом кадре, когда дочерние сущности уже созданы).
  3. В `apply_entity_transparency` сначала проверять `if cache.is_transparent == should_cut { return; }`. Если состояние прозрачности стены не изменилось — **вообще не трогать дочерние сущности и материалы**.

#### 2.2. Фильтрация по активному зданию и углу камеры
* **Проблема**: Сейчас циклы в `update_wall_cutaway_system` перебирают стены **всех** зданий мира (`Замок`, `Особняк Уэйтли`, `Особняк Марша`, обе `Гостиницы`), даже когда игрок гуляет по улице или находится в другом доме.
* **Решение**:
  1. Ввести отслеживание изменений: пересчитывать cutaway только если:
     * изменился `ActiveBuildingState` (`state.is_changed()`),
     * ИЛИ игрок/камера сдвинулись больше чем на порог ($\Delta \text{pos} > 0.05\text{ м}$ или $\Delta \text{yaw} > 0.01\text{ рад}$).
  2. При смене `active_building` со старого `Some(prev_id)` на `None` (или другое здание) однократно сбрасывать в непрозрачное состояние только стены `prev_id`, а в остальных кадрах обрабатывать **только** сущности текущего `active_building` и `viewed_floor`.

#### 2.3. Оптимизация `update_building_floor_visibility_system`
* **Проблема**: Система каждый кадр проходит по всем `BuildingRoof`, `BuildingExteriorShell`, `BuildingFloorRoot`, `BuildingSpanFloors`.
* **Решение**: Добавить условие раннего выхода `if !state.is_changed() { return; }` в начало `update_building_floor_visibility_system` (так же, как это уже сделано в `update_building_hud_system`).

---

### Этап 3. Полноценный стриминг чанков рельефа и процедурных объектов (`src/terrain.rs`)

#### 3.1. Выгрузка далеких чанков (Chunk Unloading / Despawning)
* **Проблема**: `stream_terrain_chunks_system` спавнит чанки в радиусе `CHUNK_STREAM_RADIUS = 5`, но при движении игрока в одну сторону старые чанки остаются в `spawned_chunks`, `Assets<Mesh>` и `nav_world.terrain_heights` навсегда.
* **Решение**:
  1. Ввести радиус выгрузки `CHUNK_UNLOAD_RADIUS: i32 = CHUNK_STREAM_RADIUS + 2` (гистерезис в 2 чанка предотвращает дребезг при ходьбе по границе чанка).
  2. Привязать процедурно сгенерированные объекты в `populate_wild_chunk` к родительской сущности `TerrainChunkEntity` (через `.add_child(prop_entity)` или компонент `ChunkMember(ChunkCoord)`), а их `ColliderId` сохранять в списке коллайдеров чанка.
  3. Когда расстояние Чебышева $\max(|cx - player\_cx|, |cz - player\_cz|) > \text{CHUNK\_UNLOAD\_RADIUS}$:
     * Удалять коллайдеры диких объектов этого чанка из `NavWorld`;
     * Вызывать `commands.entity(chunk_entity).despawn()` (вместе с дочерними нетронутыми объектами);
     * Удалять запись из `terrain.spawned_chunks`.
  4. **Сохранение дельт игрока**: Если игрок срубил дерево или построил стену в диком чанке, фиксировать модификацию в ресурсе `ModifiedChunkData` (чтобы при повторном возвращении срубленное дерево не появлялось заново).

#### 3.2. Переиспользование буферов меша и отказ от лишнего хэширования высот
* **Проблема**:
  * `nav_world.terrain_heights: HashMap<IVec2, f32>` дублирует высоты рельефа, которые уже вычисляются в `WorldTerrain::grid_height(gx, gz)`.
  * В `rebuild_dirty_chunks_system` пересобираются все грязные чанки за один кадр.
* **Решение**:
  1. Убрать дублирующуюся хэш-таблицу `nav_world.terrain_heights` или хранить в чанке плоский массив `[[f32; 9]; 9]` (81 число `f32` = 324 байта на чанк вместо 81 записи в `HashMap`), обращаясь к нему по индексу за $O(1)$.
  2. В `build_chunk_mesh` использовать `RenderAssetUsages::RENDER_WORLD` для неизменяемых чанков или обновлять атрибуты существующего `Mesh` in-place при терраформинге.

---

### Этап 4. Граф структурной целостности зданий (`src/destruction.rs`)

#### 4.1. Переход от $O(N^2)$ полного перебора к локальному графу смежности
* **Проблема**: В `recompute_structural_integrity_system` все `StructuralPiece` со всей карты собираются в один `Vec`, после чего выполняется 10 итераций двойного цикла `for i in 0..len { for j in 0..len }`. При 500 строительных блоках это $10 \times 250\,000 = 2.5\text{ млн}$ проверок дистанций за один кадр.
* **Решение**:
  1. **Локализация по зданиям / чанкам**: Разрушение стены в `Особняке Уэйтли` никак не влияет на `Замок Исследователя`. Присваивать каждому `StructuralPiece` идентификатор `BuildingId` или `ChunkCoord` и помечать в `StructuralIntegrityDirty` только затронутую группу (`dirty_buildings: HashSet<BuildingId>`).
  2. **Пространственная сетка / Граф смежности**:
     * Помещать блоки в 3D-корзины с шагом `4.0 м` (размер тайла стены/пола).
     * Для каждого блока искать соседей только в его собственной и 26 соседних 3D-корзинах (это снижает внутренний цикл с $N$ до $k \le 12$ соседей).
  3. **Волновой алгоритм (Dijkstra / Max-Heap Propagation)**:
     * Инициализировать очередь с приоритетом (`BinaryHeap`) всеми блоками-фундаментами (`floor_index == 0`, опирающимися на грунт, `support = max_support`).
     * Распространять опору от узлов с максимальным `support` к соседям за один проход $O(V \log V + E)$ вместо 10 слепых итераций релаксации.

---

### Этап 5. Оптимизация частиц, лута, RTS-рейкастинга и UI (`src/destruction.rs`, `src/rts_command.rs`)

#### 5.1. Кэширование `Handle<Mesh>` и `Handle<StandardMaterial>` для эффектов
* **Проблема**:
  * В `spawn_debris_burst` на **каждый удар** топором вызывается `meshes.add(Cuboid::new(0.22, 0.22, 0.22))` и `materials.add(...)`.
  * В `spawn_loot_drops` при каждом разрушении создается `meshes.add(Cuboid::new(0.32, 0.32, 0.32))`, даже если используется `WorldAssetRoot`!
  * В `spawn_staircase` (`src/building.rs`) для каждой лестницы заново создаются одинаковые меши ступеней и материалы.
* **Решение**:
  1. Создать ресурс `SharedVfxAssets` при старте приложения (`Startup`):
     * Единый `debris_cube_mesh: Handle<Mesh>`;
     * Единый `fallback_loot_mesh: Handle<Mesh>`;
     * Предсозданные `Handle<StandardMaterial>` для каждого варианта `DestructibleMaterial` (`Wood`, `Stone`, `Foliage`, `Occult`) и `ResourceType`.
  2. Это полностью устранит создание новых GPU-буферов во время боя/рубки и позволит рендереру Bevy автоматически объединять все осколки в **1–4 инстансированных draw call**.

#### 5.2. Ускорение RTS-выбора объекта (`rts_right_click_menu_system`)
* **Проблема**: Функция `ray_cylinder_distance` делает 9 проверок пересечения с горизонтальными плоскостями для каждого `Destructible` в мире.
* **Решение**:
  1. Заменить пошаговый цикл `for step in 0..=8` на **аналитическое пересечение луча и вертикального цилиндра / AABB** (решение квадратного уравнения в плоскости XZ + проверка $y \in [\text{min\_y}, \text{max\_y}]$ — выполняется за ~10 арифметических операций без циклов).
  2. Отсекать объекты по дистанции от точки пересечения луча с плоскостью земли/этажа ($R \le 12\text{ м}$ от точки клика).

#### 5.3. Реактивное обновление UI и HUD (`update_inventory_hud_system`)
* **Проблема**: `update_inventory_hud_system` форматирует двухстрочную строку каждый кадр.
* **Решение**:
  1. Проверять изменения ресурсов и позиции биома:
     ```rust
     let current_biome = terrain.sample_biome(player_tf.translation.x, player_tf.translation.z);
     if !inventory.is_changed()
         && !active_order.is_changed()
         && *last_biome == Some(current_biome)
         && *last_chunk_count == terrain.spawned_chunks.len()
     {
         return;
     }
     ```
  2. В `rts_menu_interaction_system` перенести кнопки контекстного меню на стандартные компоненты `Button` + `Interaction` из `bevy::ui`, убрав ручной расчет экранных прямоугольников `screen_rect`.

---

### Этап 6. Оптимизация освещения, рендера и планировщика ECS (`src/main.rs`, `src/world.rs`)

#### 6.1. Менеджер источников света (`LightLOD` / Distance Culling)
* **Проблема**: В сцене одновременно светят >30 `PointLight` (фонари улиц, канделябры, камины, алтари + по одному `PointLight` на каждый посаженный куст `TentacleCrop`). В изометрической камере многие из них находятся далеко за пределами экрана или внутри закрытых домов.
* **Решение**:
  1. Для `TentacleCrop`: заменить индивидуальный `PointLight` у каждого ростка на **Emissive-материал** (биолюминесцентное свечение спор в самом шейдере модели, что стоит 0 источников света) либо один общий `PointLight` на грядку/канал.
  2. Для интерьерных `PointLight`: поскольку они уже являются дочерними сущностями `BuildingFloorRoot`, при `Visibility::Hidden` у неактивного этажа или закрытого дома их свет автоматически скрывается (проверить, что все интерьерные источники света вложены в `BuildingFloorRoot`, а не в корень мира).
  3. Для уличных фонарей: добавить систему `cull_distant_lights_system`, отключающую `Visibility` у `PointLight`, находящихся дальше `45.0 м` от фокуса камеры.

#### 6.2. Разделение `.chain()` на параллельные `SystemSet` в `src/main.rs`
* **Проблема**: Единый `.chain()` из 15 систем блокирует многопоточность.
* **Решение**: Разбить `Update` на 4 упорядоченных `SystemSet`, внутри которых независимые системы выполняются параллельно:
  1. `GameSystemSet::Input` (`rts_right_click_menu_system`, `rts_menu_interaction_system`, `player_input_system`)
  2. `GameSystemSet::Simulation` (`execute_rts_order_system`, `player_movement_system`, `stream_terrain_chunks_system`, `recompute_structural_integrity_system`)
  3. `GameSystemSet::WorldSync` (`rebuild_dirty_chunks_system`, `update_destruction_effects_system`, `update_active_building_system`, `update_building_floor_visibility_system`, `update_wall_cutaway_system`)
  4. `GameSystemSet::Presentation` (`camera_controller_system`, `update_building_hud_system`, `update_inventory_hud_system` — работают полностью параллельно!)

#### 6.3. Настройка профилей сборки в `Cargo.toml`
* Добавить оптимизацию зависимостей для быстрой разработки и компактного WASM-билда:
  ```toml
  [profile.dev]
  opt-level = 1

  [profile.dev.package."*"]
  opt-level = 3

  [profile.release]
  opt-level = 3
  lto = "thin"
  codegen-units = 1
  strip = true

  [profile.wasm-release]
  inherits = "release"
  opt-level = "z"
  lto = "fat"
  codegen-units = 1
  ```

---

## 3. Дорожная карта внедрения и верификации

1. **Шаг 1 (Быстрые победы — без изменения логики)**:
   * Настроить профили `Cargo.toml` (**OPT-10**).
   * Добавить `SharedVfxAssets` для осколков, лута и лестниц (**OPT-06**).
   * Добавить проверку `is_changed()` в `update_building_floor_visibility_system` и `update_inventory_hud_system` (**OPT-07**).
2. **Шаг 2 (Ядро производительности CPU)**:
   * Внедрить `CutawayStateCache` для стен и мебели (**OPT-02**).
   * Внедрить `SpatialGrid` в `NavWorld` и аналитический рейкаст цилиндров (**OPT-01**, **OPT-05**).
3. **Шаг 3 (Масштабируемость открытого мира)**:
   * Реализовать выгрузку далеких чанков в `stream_terrain_chunks_system` (**OPT-03**).
   * Перевести `recompute_structural_integrity_system` на локальный волновой алгоритм по 3D-сетке (**OPT-04**).
4. **Шаг 4 (Рендер и параллелизм ECS)**:
   * Настроить `SystemSet` в `main.rs` (**OPT-09**) и дистанционный culling `PointLight` (**OPT-08**).
   * Прогнать все существующие и новые unit-тесты (`cargo test`) и проверить FPS в нативной и WASM сборках.
