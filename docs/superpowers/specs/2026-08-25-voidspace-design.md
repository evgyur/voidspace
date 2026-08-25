# Voidspace: дизайн Windows-анализатора дискового пространства

Дата: 2026-08-25  
Статус: утверждённый дизайн, проходит финальную проверку перед планированием реализации
Рабочее название: Voidspace

## 1. Контекст

Voidspace — быстрый Windows-анализатор дискового пространства на Rust. Продукт сохраняет сильные стороны SpaceSniffer: живую treemap-карту, построение результатов во время сканирования, навигацию по иерархии, фильтры, теги, снимки, экспорт и реакцию на изменения файловой системы. Интерфейс и внутренняя архитектура проектируются заново.

Исходный SpaceSniffer 1.3.0.2 был изучен статически и интерактивно. Зафиксированы его основные экраны и функции: выбор диска/пути/снимка, несколько окон сканирования, treemap, навигация, фильтры, теги, экспортные шаблоны, настройки геометрии и анимации, файловые классы, журнал, справка и файловые действия.

## 2. Цели

1. Быстро показывать первые результаты и не блокировать интерфейс до завершения сканирования.
2. Постоянно поддерживать карту в актуальном состоянии после создания, удаления, перемещения, переименования и изменения размера файлов.
3. Предоставить обычный режим без повышения прав и опциональный Turbo Scan для NTFS с правами администратора.
4. Сохранить функциональную полноту SpaceSniffer, но заменить устаревший MDI-интерфейс современным Windows-first рабочим пространством.
5. Обеспечить безопасные файловые действия: Explorer, копирование пути, корзина и безвозвратное удаление.
6. Поддерживать миллионы файлов при предсказуемом потреблении памяти и плавной GPU-отрисовке.
7. Работать локально без телеметрии и сетевой зависимости.

## 3. Не входит в первую версию

- macOS, Linux и мобильные платформы;
- облачные хранилища и удалённый индексирующий сервис;
- поиск дубликатов по содержимому и хеширование всех файлов;
- автоматические рекомендации по очистке;
- фоновой Windows Service;
- планировщик сканирований;
- встроенный просмотрщик или редактор файлов;
- криптографически гарантированное стирание данных. «Безвозвратное удаление» означает обход корзины; на SSD и современных файловых системах это не гарантирует невозможность лабораторного восстановления;
- полноценный язык сценариев в экспортных шаблонах. Поддерживается безопасная подстановка документированных полей и форматирование без выполнения кода.

## 4. Поддерживаемая среда

- Windows 10 22H2 и Windows 11, x86-64;
- NTFS, ReFS, exFAT и FAT для обычного сканирования;
- Turbo Scan и USN Journal только для NTFS;
- локальные, съёмные и UNC-пути в обычном режиме. Для UNC гарантирован baseline scan; live-режим включается только если сервер подтверждает recursive change notifications, иначе вкладка явно показывает `POLLING` и сверяет открытую ветку раз в 5 секунд;
- reparse points по умолчанию не раскрываются. Опция `Follow links within selected volume` разрешает traversal только в пределах исходного `VolumeId`, с visited-set по file identity и пределом 64 перехода; cross-volume targets и любые циклы остаются отдельными link-nodes;
- DPI 100–250%, несколько мониторов, светлая системная тема не меняет выбранную тёмную тему приложения;
- стабильный Rust toolchain, зависимости фиксируются в `Cargo.lock`.

## 5. Визуальная система

Выбрано направление Spectral Workbench с палитрой Spectral Signal.

- Основа: почти чёрные поверхности `#070809`–`#111316`.
- Главный акцент: Signal Orange `#FF5A2F` для активного действия, выбранного объекта, административного режима и прогресса.
- Цвета данных: Electric Cyan `#19D3FF`, Acid Lime `#C9F65A`, Hot Magenta `#FF4ECD`, Ultraviolet `#8B5CF6`.
- Оранжевый не используется как декоративный фон всего интерфейса: он сохраняет роль сигнала.
- Карта — визуальный центр; chrome и панели остаются тихими.
- Геометрия прямоугольника всегда разделяет заголовок и содержимое. Текст не позиционируется поверх дочерней сетки абсолютными координатами.
- Уровень подписей зависит от площади: полная подпись, сокращённая подпись, только цвет, затем объединённый узел «мелкие объекты».
- Все строки используют ellipsis и `min-width: 0`; ни один дочерний элемент не может выйти за границы tile.

Нормативная композиция рабочего экрана:

```text
┌ top bar: scope | breadcrumbs | search/filter | scan status | ADMIN ┐
├ tabs ────────────────────────────────────────────────────────────────┤
├ treemap canvas (flex, min 560×360) ───────────┬ inspector (320 px) ┤
│                                               │ selection/details  │
│                                               │ actions/classes    │
├ status: files | used/free/unknown | watch health | errors ─────────┤
└─────────────────────────────────────────────────────────────────────┘
```

При ширине меньше 1180 px inspector превращается в overlay drawer; меньше 900 px вторичные кнопки top bar уходят в меню, но breadcrumbs, фильтр и scan state остаются видимыми. Минимальный размер окна — 800×600. При недостатке высоты inspector скроллится независимо, treemap не перекрывается. Геометрический prototype проверен на 1440×900, 1280×720 и 1024×768: пересечений соседних элементов и выходов дочерних блоков за границы нет.

## 6. Основные пользовательские потоки

### 6.1 Запуск и выбор области

Стартовый экран показывает доступные тома с ёмкостью, занятым и свободным местом. Пользователь выбирает том, папку, UNC-путь или сохранённый snapshot. Доступны кнопки Scan и Turbo Scan. Turbo Scan объясняет необходимость UAC и доступен только для NTFS.

### 6.2 Сканирование

После запуска сразу открывается рабочая вкладка. Карта, счётчики и список крупнейших объектов обновляются потоково. Пользователь может поставить задачу на паузу, отменить её или повторно просканировать выбранную ветку. Несколько вкладок сканирования независимы.

Empty state содержит выбор тома/папки и последние пять scopes. Loading state сразу показывает skeleton только в inspector, а canvas принимает реальные узлы по мере поступления. Paused, Offline и Error отображаются persistent banner в пределах вкладки; данные не скрываются. Одновременно активно не более двух baseline scans и одного Turbo scan; дополнительные вкладки стоят в видимой очереди, live-watcher уже завершённых вкладок продолжает работать.

### 6.3 Навигация

- один клик выбирает объект;
- двойной клик погружает в папку;
- Backspace и кнопки Back/Forward работают по истории;
- Ctrl+Up и breadcrumb переходят к родителю;
- колесо или Ctrl+Plus/Ctrl+Minus меняют уровень детализации;
- Ctrl+Home возвращает к корню текущего сканирования;
- hover синхронно подсвечивает tile и строку в инспекторе/списке.

### 6.4 Фильтры, классы и теги

Фильтр применяется к готовому индексу без повторного обхода диска. Синтаксис поддерживает имя, расширение, путь, тип, размер, allocated size, дату, возраст, атрибуты и пользовательский тег. Есть включающие и исключающие выражения, логические AND/OR и сохранённые пресеты.

Файловые классы сопоставляют расширения с названием и базовым цветом. Четыре быстрых тега доступны с клавиатуры `1`–`4`; пользователь может переименовать теги и изменить цвета. Тег рисуется отдельным маркером и не разрушает цветовое кодирование типа данных.

Фильтр скрывает несовпавшие tiles, но агрегат в status bar отдельно показывает `visible / total`; ancestor сохраняется как context node, если совпал его потомок. Unknown не удовлетворяет числовому сравнению, кроме явного `is:unknown`. Полная грамматика и defaults закреплены в разделе 24.

### 6.5 Файловые действия

Для выбранного объекта доступны:

- Open и Open in Explorer;
- Copy path;
- Move to Recycle Bin;
- Delete permanently;
- Properties;
- Rescan branch.

Ctrl/Shift создают multi-selection только в списке и среди siblings текущего treemap root. Context menu действует на текущий selection; если объект под курсором не выбран, он сначала становится единственным выбранным. Open/Properties при multi-selection недоступны, Copy path и операции удаления применяются ко всем элементам после дедупликации вложенных путей.

Snapshot открывается в read-only режиме: операции над файлами отключены до запуска нового сканирования соответствующего пути.

### 6.6 Снимки и экспорт

Snapshot содержит версию схемы, идентификатор тома, корневой путь, время, настройки измерения и компактный индекс. Он сжимается и проверяется контрольной суммой.

Экспорт поддерживает CSV, JSON, HTML и текстовый шаблон. Профиль задаёт сортировку, Files first/Folders first, порядок, header/detail/footer и документированные поля: path, name, logical size, allocated size, timestamps, attributes, counts, class и tag. Шаблоны не выполняют произвольный код.

## 7. Решение по UI-стеку

Выбраны `eframe/egui` и backend `wgpu` с DirectX 12. Это pure-Rust стек, подходящий для часто меняющегося состояния и пользовательской GPU-отрисовки. Панели, ввод, accessibility и оконная интеграция используют egui; treemap рисуется специализированным пакетным renderer-слоем.

Отклонённые варианты:

- чистые `winit + wgpu`: максимальный контроль, но неоправданная реализация собственного UI toolkit;
- Slint: хороший декларативный UI, но менее прямой путь к специализированной карте и дополнительные лицензионные ограничения для закрытой сборки.

## 8. Архитектурные единицы

Реализация организуется как Cargo workspace. Каждая единица имеет отдельный публичный интерфейс и тестируется без UI.

### 8.1 `voidspace-model`

Владеет типами `NodeId`, `VolumeId`, `FileId`, `NodeKind`, `SizeMetrics`, `TimestampSet`, `NodeFlags`, `TagId`, `ScanId` и событиями между подсистемами. Не зависит от Win32, renderer или egui.

### 8.2 `voidspace-index`

Хранит дерево в компактной arena. Узел содержит стабильный идентификатор, родителя, диапазон/список детей, interned name, file identity, logical/allocated sizes, времена, flags и агрегаты. Полные пути не дублируются в каждом узле и собираются по цепочке родителей.

Публичный контракт:

```text
apply_scan_event(ScanEvent) -> DirtySet
apply_fs_delta(FsDelta) -> DirtySet
snapshot(ReadScope) -> IndexSnapshot
resolve_path(NodeId) -> PathBuf
```

Все изменения проходят через один reducer, поэтому агрегаты и dirty-набор обновляются атомарно с точки зрения UI.

### 8.3 `voidspace-scan`

Выполняет обычный параллельный обход каталогов. Использует Win32 directory enumeration, bounded work-stealing queue, cancellation token и backpressure. Не следует через reparse point по умолчанию. Накапливает permission и I/O ошибки как данные, а не завершает всю задачу.

Контракт:

```text
start(ScanRequest, EventSink) -> ScanHandle
ScanHandle::{pause, resume, cancel, stats}
```

### 8.4 `voidspace-ntfs`

Открывает NTFS volume handle после UAC, перечисляет записи через документированные FSCTL/USN-механизмы, связывает parent file reference и file reference, а затем передаёт те же `ScanEvent`, что и обычный scanner. Модуль изолирован за trait, чтобы обычный режим не зависел от NTFS.

Код модуля компонуется только в `voidspace-elevated`; основной процесс знает лишь IPC-типы. Helper владеет volume handle, преобразует NTFS references в wire-события и не передаёт raw handle основному процессу. Turbo Scan не парсит сырые структуры диска вручную и не пишет на том.

### 8.5 `voidspace-watch`

Поддерживает живой индекс.

- В обычном режиме регистрирует `ReadDirectoryChangesW` для сканируемого корня и буферизует события.
- В NTFS Turbo-режиме хранит USN cursor и читает Change Journal.
- Cursor/наблюдение запускается до baseline scan. После baseline накопленные события воспроизводятся через reducer, что закрывает race между сканированием и изменениями.
- События собираются фиксированным окном 40 мс; настройка `UI coalescing` 30–250 мс управляет только частотой публикации snapshot в UI и не меняет reconciliation semantics.
- При переполнении буфера помечается минимальная достоверно известная ветка и запускается targeted rescan.
- При смене USN Journal ID или недоступной истории запускается сверка тома; полный перескан используется только когда восстановление невозможно.

Контракт:

```text
watch(WatchRequest, DeltaSink) -> WatchHandle
WatchHandle::{stop, health, cursor}
```

### 8.6 `voidspace-layout`

Строит стабильную squarified treemap для текущего viewport и уровня детализации. При `DirtySet` пересчитывает изменённые поддеревья и их предков, а не весь индекс. Стабильный secondary key уменьшает визуальные скачки при малых изменениях.

Контракт:

```text
layout(IndexSnapshot, ViewState, DirtySet) -> LayoutSnapshot
hit_test(Point) -> Option<NodeId>
```

Layout гарантирует непересечение прямоугольников, нахождение каждого ребёнка внутри родителя и сохранение суммарной площади с учётом пиксельного округления.

### 8.7 `voidspace-render`

Преобразует видимые layout nodes в небольшое число batched meshes для `wgpu`. Текст и мелкие маркеры получают отдельный LOD. При активной анимации renderer запрашивает кадры до 60 FPS; в idle перерисовка событийная.

Renderer не читает файловую систему и работает только с immutable `LayoutSnapshot` и theme tokens.

Контракт:

```text
prepare(LayoutSnapshot, Theme, RenderBudget) -> PreparedFrame
paint(&PreparedFrame, SurfaceTarget) -> RenderStats
invalidate(DeviceLost | ThemeChanged | DpiChanged)
```

### 8.8 `voidspace-fileops`

Выполняет Open/Explorer/Properties, корзину и permanent delete на отдельном worker. Для корзины используется Windows Shell `IFileOperation`. Permanent delete удаляет файлы и пустеющие каталоги без перехода в корзину.

Reparse point удаляется как ссылка; target никогда не обходится. Результат возвращается по каждому объекту отдельно. Успешные операции преобразуются в `FsDelta`; watcher затем подтверждает фактическое состояние.

Контракт:

```text
prepare(OperationDraft, IndexSnapshot) -> ConfirmableOperation
execute(ConfirmedOperation, ProgressSink, CancellationToken) -> OperationReport
OperationReport = { operation_id, per_item, reclaimed_logical, reclaimed_allocated, reconciliation_hint }
```

### 8.9 `voidspace-export`

Сериализует snapshot и экспортные форматы потоково, чтобы не копировать всё дерево. Template parser создаёт безопасный AST разрешённых полей и форматтеров.

Контракт:

```text
save_snapshot(IndexSnapshot, SnapshotOptions, AtomicTarget, CancellationToken) -> ArtifactResult
load_snapshot(ReadSource, ResourceLimits) -> ValidatedSnapshot
export(IndexSnapshot, ExportProfile, AtomicTarget, CancellationToken) -> ArtifactResult
```

### 8.10 `voidspace-app`

Содержит eframe lifecycle, вкладки, command routing, keyboard shortcuts, настройки и композицию экранов. UI не мутирует индекс напрямую: он отправляет команды и получает immutable snapshots/events.

Контракт между UI и coordinator: `AppCommand`, `AppEvent`, `TabSnapshot`; команды всегда содержат `tab_id` и `generation`, устаревшие ответы игнорируются.

### 8.11 `voidspace-elevated`

Минимальный helper с манифестом `requireAdministrator`. Принимает строго типизированные запросы Turbo Scan и защищённых операций по локальному authenticated IPC. Проверяет вызывающий процесс, нормализует пути и не предоставляет универсальную командную оболочку.

Контракт: versioned length-prefixed binary messages `Hello/Challenge/Auth/Request/Progress/Result/Error/Cancel/Goodbye`; подробности lifecycle и защиты — раздел 23.

## 9. Поток данных и конкурентность

1. UI создаёт `ScanRequest`.
2. Scan coordinator фиксирует watch cursor, запускает scanner и передаёт события в bounded channel.
3. Index reducer применяет события пакетами и публикует immutable snapshot/version.
4. Layout worker получает snapshot и dirty-set, создаёт новый layout snapshot.
5. UI атомарно заменяет отображаемый snapshot и renderer интерполирует старые/новые прямоугольники.
6. Watcher продолжает выдавать `FsDelta` после baseline scan.
7. File operation возвращает per-item result; успешный результат применяется как optimistic delta с `operation_id`, затем дедуплицируется или опровергается watcher.

UI thread никогда не ждёт disk I/O, UAC helper, экспорт или полное построение layout. Bounded channels предотвращают неограниченный рост памяти. Cancellation проходит от вкладки к scan/watch/layout/export tasks. Формальные схемы, ordering и recovery приведены в разделах 21–23.

## 10. Размеры и идентичность файлов

- Logical size показывает длину файла.
- Allocated size показывает реально выделенное место, включая sparse/compressed semantics.
- Основная карта по умолчанию использует allocated size; режим переключается.
- `VolumeId` локального тома — `(normalized volume GUID path, filesystem serial u64, filesystem kind)` из volume APIs. Equality требует совпадения всех трёх полей; если GUID/serial недоступны, создаётся `SessionVolumeId`, который никогда не переживает reconnect. Для UNC используется `(normalized server/share, server volume serial)`, если share возвращает `FILE_ID_INFO`; иначе identity session-scoped и любой reconnect требует full rescan. Removable device считается прежним только при полном equality, буква диска значения не имеет.
- На NTFS/ReFS `FileId` — 128-bit file ID, полученный через handle; на FAT/exFAT используется volume serial + directory entry identity, если доступна, иначе generation-scoped normalized path hash; на UNC identity считается unstable, пока сервер не вернул file ID. Identity всегда дополнена `scan_generation`, чтобы reuse ID после delete не воскресил старый узел.
- Hard links идентифицируются по `VolumeId + FileId`. Logical size виден у каждой ссылки. Для allocated treemap полная allocated size принадлежит детерминированному owner-link с минимальным нормализованным путём внутри scope, остальные получают нулевую площадь и badge `shared`; inspector показывает физический размер каждой ссылки. При удалении owner ownership атомарно переходит следующей ссылке, total тома не меняется. Перемещение может сменить owner, но не total.
- ADS сканируются только при включённой настройке и отображаются как дочерние stream nodes.
- Для scope, равного корню локального тома, `BoundedUnknown = clamp(used_volume - known_unique_allocated, 0, used_volume)`; он показывает недоступные metadata/ADS/служебные расхождения отдельным weighted system tile. `Restricted` — причина unknown и flag соответствующего узла, не добавочное число.
- Для folder/UNC scope нельзя приписывать пространство вне scope: Free tile отсутствует, а недоступная ветка имеет `UnboundedUnknown` без числового размера. Она показывается hatch-overlay/badge поверх доступного родителя и в inspector, но не получает rectangle и исключена из area-conservation invariant, totals, сортировки и signed deltas.
- Free space берётся из volume API и участвует в площади только для volume-root при включённом `Show free`; `BoundedUnknown` аналогично управляется `Show unknown`. Фильтр не меняет physical totals. ADS включаются в logical/allocated владельца и также доступны как дочерние nodes; при отключённом сканировании ADS их неизвестный вклад может увеличить только volume-root `BoundedUnknown`, а для folder scope отмечается `UnboundedUnknown`.

## 11. Live-пересчёт

Изменение узла создаёт signed size delta. Reducer применяет его к узлу и всем предкам до корня, обновляет сортировочные ключи и формирует `DirtySet`. Layout перестраивает только затронутые ветки текущего viewport.

При удалении из Voidspace успешный результат worker немедленно tombstone-ит узел и вычитает агрегаты. Optimistic delta содержит `operation_id` и captured identity; совпавшее watcher-событие подтверждает tombstone без повторного вычитания. Если watcher сообщает противоречие, targeted rescan восстанавливает истину. При внешнем удалении используется тот же reducer path. Полный ordering описан в разделе 22.

Цель типичной задержки от файлового события до отображения — p95 менее 100 мс без учёта задержки самой файловой системы и сетевого пути.

## 12. Права администратора

Основной executable имеет `asInvoker`. По умолчанию elevated helper запускается через `ShellExecuteW`/`runas` только для Turbo Scan или защищённой операции.

Настройка «Всегда запрашивать права администратора» сохраняется для пользователя. При следующем запуске основной процесс проверяет token elevation и, если нужно, один раз перезапускает себя через `runas`. UAC остаётся обязательным при каждом таком запуске; приложение не обходит и не предлагает отключать UAC. Elevated state отмечается оранжевым badge `ADMIN`.

Пользователь также может включить системную совместимость «Run this program as administrator»; приложение обнаруживает полученный elevated token. Drag-and-drop из non-elevated Explorer может быть заблокирован UIPI, поэтому выбор папки, вставка пути и shell actions остаются полноценными альтернативами.

## 13. Безопасность удаления

### 13.1 Корзина

Перед операцией показываются количество объектов, полный/общий размер и освобождаемое allocated space. Ошибки возвращаются по каждому объекту. Отмена shell operation не трактуется как успех.

### 13.2 Permanent delete

Диалог использует оранжево-красную danger-систему, показывает полный список или сводку, общий размер и явное предупреждение об обходе корзины. Кнопка разблокируется после ввода `DELETE`.

Для корня тома операция запрещена. Для `Windows`, `Program Files`, `Program Files (x86)`, `ProgramData`, каталога текущего executable и других системных known folders нужны elevated helper, ввод полного нормализованного пути и повторное подтверждение. Символические ссылки, junctions и mount points удаляются как объекты и не разрешаются в target.

Перед выполнением пути канонизируются, проверяются на prefix confusion, device namespace, alternate syntax и изменение между подтверждением и операцией. IPC helper принимает конкретный список уже подтверждённых canonical paths, captured file identities и operation nonce. Непустые каталоги удаляются post-order без traversal reparse points; при изменении identity или появлении нового дочернего объекта после подтверждения соответствующий root завершается `ChangedSinceConfirmation` и не удаляется. Подробная permission/failure matrix — раздел 25.

## 14. Ошибки и восстановление

- Access denied на локальном scope: узел `Restricted`, штриховка, счётчик неизвестного размера и действие Retry as administrator через typed `PrivilegedSubtreeScan`; результат атомарно заменяет только эту ветку. Для UNC elevation не меняет серверные права, поэтому показываются Retry и инструкция проверить share/NTFS credentials без UAC.
- Sharing violation/locked file: операция завершается ошибкой только для этого объекта; доступен Retry.
- File disappeared: событие трактуется как delete и не показывается как фатальная ошибка.
- Rename pairing lost: родительская ветка помечается dirty и перепроверяется.
- Watch buffer overflow: targeted rescan минимальной ветки.
- USN journal reset: сверка volume identity и новый baseline/cursor.
- Device disconnected: вкладка переходит в состояние Offline, сохраняя последний snapshot; доступен Resume после возврата тома.
- GPU adapter failure: повторная инициализация с совместимым software adapter; если она невозможна, показывается диагностический экран без потери scan snapshot.
- Corrupt snapshot: файл не открывается частично; ошибка содержит версию, checksum state и безопасное действие удалить/оставить файл.
- Export failure: временный файл удаляется, исходная цель не заменяется; финальный rename атомарный на поддерживаемой файловой системе.

Пользовательские ошибки отображаются рядом с соответствующей вкладкой/операцией. Подробный локальный log drawer доступен отдельно и не открывается автоматически при каждом предупреждении.

## 15. Настройки

- scan: ADS (off), reparse policy (`Never follow`), logical/allocated default (`Allocated`), normal/Turbo preference (`Normal`);
- live updates: enabled, UI coalescing 50 мс в диапазоне 30–250 мс, filesystem watcher health;
- geometry: initial detail 3 (1–8), minimum tile edge 3 px (2–16), sorting `Allocated desc`, Show free on только для volume-root, Show unknown on;
- motion: transition 160 мс (0–500), reduced motion наследует Windows и может быть forced, target FPS 60 (30/60/120);
- appearance: contrast `Normal` (`Normal/High`), border 1 px (0–3), hover halo on, selection shadow on, палитра Spectral Signal, встроенные file classes и четыре пустых пользовательских tag slots;
- behavior: tooltip `Detailed`, timestamps `Absolute + age`, notification on scan complete off, max concurrent normal scans 2 (1–4), recent scopes 5 (0–10);
- privilege: always request administrator rights (off);
- privacy: redact paths in copied diagnostics (on) and export presets (paths разрешены только после явного выбора).

Настройки хранятся в versioned JSON (`settings_version = 1`) через atomic replace. Неизвестные поля сохраняются при round-trip, неизвестные enum values заменяются defaults с warning. Corrupt file переименовывается в `.broken-<timestamp>`, загружаются безопасные defaults. Миграции выполняются последовательно `vN -> vN+1` и покрываются golden tests.

## 16. Производительность и критерии приёмки

- UI остаётся отзывчивым во время scan/watch/export/delete; ни один disk I/O call не выполняется на UI thread.
- Видимая карта поддерживает 60 FPS: p95 CPU frame ≤ 8 мс и p95 GPU frame ≤ 12 мс; `RenderBudget` ограничивает кадр 50 000 rectangles и 2 000 text labels, остальные узлы агрегируются в `small items`.
- Типичное локальное изменение отображается с p95 менее 100 мс.
- Первые nodes появляются до завершения root scan.
- На reference machine (Windows 11 23H2+, Ryzen 5 5600G, 16 GiB DDR4-3200, Radeon Vega 7 DX12, Samsung 970 EVO Plus 1 TB, Defender realtime on без exclusions) normal scan синтетического NTFS-дерева 1M zero-byte files с тёплым filesystem cache достигает ≥150k entries/s; Turbo — ≥300k entries/s и минимум в 1.5 раза быстрее normal на том же fixture. На HDD публикуется измерение без release gate.
- Memory benchmark на синтетических индексах с median UTF-16 name 24 code units: RSS после quiescence ≤220 MiB для 1M, ≤900 MiB для 5M и ≤1.65 GiB для 10M nodes; metadata arena без name pool ≤112 bytes/node. Полные пути не хранятся в каждом node.
- Первый интерактивный кадр стартового экрана появляется ≤500 мс после process start; первый treemap tile — ≤750 мс после первого принятого scan event. Реакция pause/cancel — ≤250 мс, полное завершение workers после cancel — ≤2 с без зависшего I/O, который ОС не позволяет отменить.
- Закрытие/отмена вкладки освобождает workers, handles и watcher registrations без утечки.

Benchmark protocol: release build, plugged-in power profile, Defender state записывается, три прогревочных и пять измеряемых запусков; p95 считается по всем event-to-paint samples после первых 10 секунд, frame p95 — по 60-секундному scripted pan/zoom. Результаты, hardware descriptor и raw samples сохраняются в CI artifacts. Регресс более 10% относительно утверждённого baseline блокирует соответствующий release gate; абсолютные пороги выше остаются обязательными.

## 17. Тестирование

### 17.1 Unit и property tests

- arena invariants, parent/child relations и size aggregation;
- hard-link allocated accounting;
- filter parser и evaluator;
- template parser без выполнения кода;
- treemap: отсутствие пересечений, containment, неотрицательная площадь, детерминированность;
- reducer для произвольных последовательностей create/delete/modify/rename/move;
- canonical path и protected-root policy.

### 17.2 Integration tests

Тест создаёт временное дерево и параллельно сканированию создаёт, удаляет, изменяет, перемещает и переименовывает файлы. После quiescence индекс сравнивается с независимым filesystem walk.

Отдельно проверяются cancellation, permission denied, locked files, reparse points, hard links, sparse/compressed files, ADS, watcher overflow, USN cursor recovery, отключение тома, snapshot round-trip и export atomicity.

Негативные fixtures проверяют stale/out-of-order/duplicate события, file-ID reuse, IPC spoof/replay/version mismatch/helper crash, UAC refusal, delete TOCTOU, malformed и compression-bomb snapshot, CSV formula injection, HTML escaping, corrupt settings, channel saturation/OOM guard и redaction. Ожидаемое поведение во всех security cases — fail closed без изменения файловой системы; при resource exhaustion вкладка сохраняет последний consistent snapshot и предлагает targeted/full rescan.

### 17.3 Destructive tests

Recycle и permanent delete выполняются только внутри уникального временного каталога на специально созданном test volume/VHDX. Перед каждым destructive test canonical root проверяется на принадлежность test volume. Тест никогда не принимает вычисленный root, домашний каталог, workspace или системный путь.

### 17.4 UI и визуальные тесты

- screenshot tests основных экранов и danger dialogs;
- автоматическая DOM/geometry-подобная проверка layout constraints в egui test harness;
- 1024×768, 1280×720, 1440×900 и DPI 100/125/150/200/250%;
- keyboard-only navigation, screen reader labels и reduced motion;
- long paths, длинные Unicode/RTL имена, emoji и mixed scripts.

### 17.5 Совместимость

Матрица включает Windows 10/11, standard/elevated user, NTFS/ReFS/exFAT, HDD/SSD, removable drive и UNC share. Turbo/USN tests запускаются только на NTFS fixture volume.

## 18. Приватность и диагностика

Телеметрии нет. Сеть не требуется для сканирования, карты, удаления, snapshots или экспорта. Persistent logs находятся в `%LOCALAPPDATA%\\Voidspace\\logs`, имеют ACL только current user + SYSTEM, максимум 7 файлов по 5 MiB и удаляются не позднее 14 дней. Они никогда не содержат file contents, raw paths, search/filter text или export rows: path заменяется session-salted opaque ID, сохраняются только component, event category, OS code, timing и aggregate counts. Salt не сохраняется после session.

Delete result journal находится в `%LOCALAPPDATA%\\Voidspace\\operations`, ACL current user + SYSTEM и шифруется DPAPI CurrentUser. Запись содержит operation/item IDs, `VolumeId/FileId`, encrypted canonical parent/path, started/completed UTC и item outcome; содержимого файла, IPC nonce и security token нет. Journal удаляется сразу после reconciliation, orphan journals — через 24 часа после следующего старта. Его corruption приводит к `OutcomeUnknown` и targeted rescan известных из UI/index parents, но не к повтору операции.

Экспорт diagnostics по умолчанию включает только redacted logs/settings summary; пользователь может отдельным явным checkbox добавить полные пути в одноразовый export. Redaction tests подставляют уникальные sensitive paths и требуют их отсутствия в logs, default diagnostics, crash context и filenames. Приложение не создаёт собственные full-memory dumps; Windows crash-reporting остаётся системной политикой.

## 19. Поставка первой версии

Первая поставка — подписываемый portable x86-64 package из двух бинарников (`voidspace.exe` и `voidspace-elevated.exe`) с конфигурацией в пользовательском профиле. Приложение не требует фонового сервиса. Installer и автообновление не являются условием первой функциональной версии. Release gates приведены в разделе 27; v1 выпускается только после прохождения всех gates, но каждый gate даёт отдельно тестируемый внутренний build.

## 20. UI-состояния и управление ресурсами

Вкладка имеет `TabKind = LiveScan | StaticScan | Snapshot` и явный `TabState`; переходы выполняет coordinator, UI только отправляет команды:

| Состояние | Допустимые переходы | Данные и watcher |
|---|---|---|
| `Queued` | `BaselineActive`, `CancelledPartial`, `Error` | Пустой индекс; watcher создаётся только для `LiveScan` перед baseline |
| `BaselineActive` | `Paused { resume_to: BaselineActive }`, `Reconciling`, `ReadyStatic`, `CancelledPartial`, `Offline`, `Error` | Scan events потоковы; у `LiveScan` watcher буферизует, у `StaticScan` отсутствует |
| `RescanActive` | `Paused { resume_to: RescanActive }`, `Reconciling`, `ReadyStatic`, `Offline`, `Error` | Старая consistent ветка видна до atomic replacement; watcher существует только у `LiveScan` |
| `Paused { resume_to }` | сохранённый `resume_to`, `CancelledPartial`, `Offline`, `Error` | Producers scan/rescan приостановлены; live watcher продолжает bounded buffer |
| `Reconciling` | `Watching`, `RescanActive`, `Offline`, `Error` | Только `LiveScan`: baseline закрыт, buffered deltas воспроизводятся |
| `Watching` | `RescanActive`, `Offline`, `CancelledPartial`, `Error` | Только `LiveScan`: инкрементальные изменения активны |
| `ReadyStatic` | `RescanActive`, `CancelledPartial`, `Error` | Terminal-ready для `StaticScan` или для live updates, отключённых до запуска; watcher отсутствует |
| `SnapshotReady` | close, `Queued` в новой scan-вкладке | Snapshot read-only, никаких scanner/watcher/fileops |
| `Offline` | `Reconciling`, `RescanActive`, `CancelledPartial`, `Error` | Только live scope на недоступном device/share; последний snapshot read-only, reconnect проверяет `VolumeId` и cursor |
| `CancelledPartial` | `Queued` через явный полный Rescan, close | Partial index навигационный/read-only; watcher и workers остановлены |
| `Error` | `RescanActive`, `Queued`, close | Последний consistent snapshot сохраняется; переход зависит от recoverability |

Snapshot loader переходит `Queued -> SnapshotReady` или `Error` и не использует baseline states. `Complete` — краткое UI-событие перед `Reconciling`, `ReadyStatic` или `SnapshotReady`, а не состояние. `live updates` считывается при создании вкладки: включение для `ReadyStatic` требует нового scan, чтобы cursor был зафиксирован до baseline. Pause всегда возобновляет ровно сохранённый вид работы и не останавливает live observation.

Close активной scan-вкладки отменяет её без модального окна; close во время delete/export показывает подтверждение, потому что операция может иметь внешний эффект или незавершённый artifact. При закрытии приложения scans/watchers отменяются; для уже начатого delete приложение ждёт текущий item, максимум 5 секунд, затем helper завершает item и записывает локальный result journal.

Очереди имеют лимиты: scan events 65 536, watcher raw events 131 072, UI snapshots 2 (latest-wins), operation progress 1 024. При заполнении scan-очереди producer ждёт с cancellation; watcher не блокирует OS callback, а переводит scope в `NeedsRescan`; потеря consumer или panic закрывает producer, сохраняет последний consistent snapshot и переводит вкладку в `Error`. Allocation failure ловится до нового batch через memory budget; новые producers ставятся на pause, UI предлагает закрыть вкладки или снизить detail. Disk-full влияет только на snapshot/export/log и никогда не останавливает live index.

## 21. Схемы событий и immutable snapshots

Каждое межкомпонентное сообщение обёрнуто в:

```text
EventEnvelope {
  schema_version: u16, scan_id: ScanId, generation: u64,
  producer: ProducerId, sequence: u64, observed_at_qpc: u64,
  cause_operation: Option<OperationId>, payload: EventPayload
}
```

`generation` увеличивается при каждом новом baseline одного scope. `sequence` строго возрастает для одного producer и generation. Неизвестная версия, чужой `scan_id/generation` или повторный `(producer, sequence)` отвергаются без мутации.

`ScanEvent` имеет варианты `BaselineStarted`, `UpsertNode`, `RemoveNode`, `DirectoryEnumerated` и `BaselineFinished { captured_cursor, root_fingerprint }`, а `NodeError` является отдельным non-mutating diagnostic event. `UpsertNode` содержит parent identity, file identity, normalized name, kind, sizes, timestamps, attributes, reparse metadata и `SourceRevision`. `DirectoryEnumerated { directory_identity, enumeration_epoch, sorted_child_identities, directory_fingerprint }` закрывает одну authoritative enumeration: только после него reducer знает полный child-set этой директории. `NodeError` содержит stable category, OS code, path/identity, `recoverable` и suggested action; ошибка не завершает stream, кроме `FatalSourceLost`.

`SourceRevision = { producer, generation, sequence }` сравнима только внутри одного producer; порядок между baseline, watcher и fileops определяется фазами раздела 22, а не timestamp. Параллельный `UpsertNode`, пришедший раньше parent, хранится в bounded pending-parent map (максимум 65 536); после появления parent он применяется в sequence order. Переполнение, unresolved parent к `BaselineFinished` или конфликт kind/identity дают `Invalidate(minimal known ancestor)`, не создают synthetic parent.

`directory_fingerprint` — BLAKE3 canonical byte stream отсортированных immediate children: `(FileId bytes, normalized UTF-8 name, kind, logical, allocated, last-write UTC, reparse flag)`; ошибка чтения кодируется отдельным stable marker. `root_fingerprint` — тот же recursive aggregate hash для root после закрытия всех `DirectoryEnumerated`. Он обнаруживает расхождения, но не заменяет event replay. Термин `metadata revision` означает `SourceRevision`; отдельного filesystem revision не предполагается.

`FsDelta` имеет нормализованные варианты `Create`, `Delete`, `ModifyMetadata`, `ModifySize`, `Move { old_parent, new_parent, old_name, new_name }` и `Invalidate { minimal_scope, reason }`. Rename old/new из Win32 объединяются watcher-ом; потерянная половина никогда не угадывается и превращается в `Invalidate(parent)`. USN reason flags могут схлопнуться в один delta на identity в пределах 40 мс, сохраняя последний name/parent и суммируя flags.

`DirtySet { index_version, changed_nodes, changed_ancestors, removed_nodes, layout_roots }` относится ровно к опубликованной версии. `IndexSnapshot { scan_id, generation, index_version, root, arena_pages, aggregate_epoch }` immutable; arena pages copy-on-write, publication atomic. Layout и UI обязаны отбросить результат, если `generation` или `index_version` старше уже показанного.

Ошибки команд используют `CommandError { category, os_code, message_key, retryability, affected_scope }`; пользовательский текст локализуется только в app. Cancellation — отдельный terminal result, не ошибка и не успех.

## 22. Reconciliation и порядок live-событий

1. Coordinator открывает watcher/USN cursor и фиксирует `captured_cursor` до первого baseline event.
2. Baseline получает новую `generation`; watcher складывает raw events после cursor в bounded journal buffer.
3. Reducer применяет baseline events по producer sequence. Повторный upsert одной identity идемпотентен; более новый `SourceRevision` того же producer заменяет старый.
4. После `BaselineFinished` coordinator сверяет root fingerprint и воспроизводит watcher deltas в journal order. Для `ReadDirectoryChangesW`, где нет глобального номера, сохраняется порядок callback batch; конфликт identities/paths ведёт к targeted rescan, а не к предположению.
5. Только после draining buffer публикуется состояние `Watching`. Новые deltas могут идти в следующий atomic reducer batch.
6. Optimistic fileop delta получает `OperationId`, captured identity и expected transition. Совпавшее watcher event в течение 2 секунд подтверждает его и дедуплицируется. Несовпадение, timeout или обратное событие помечают минимальный общий ancestor на rescan.

Stale event с меньшим generation отбрасывается. В текущем generation delete создаёт tombstone с identity epoch; последующий create с тем же numeric file ID считается тем же объектом только если creation timestamp, `VolumeId` и ожидаемый parent/name transition согласованы, иначе создаётся новый `NodeId`. Move атомарен для reducer: один node меняет parent/name, а aggregates вычитаются и добавляются один раз.

Targeted rescan запускается при lost rename pair, локальном buffer overflow, watcher/fileop conflict, unstable identity или permission recovery и заменяет только проверенную ветку. Full rescan обязателен при смене `VolumeId`/USN Journal ID, потере cursor раньше lowest valid USN, overflow без известного ancestor, несовпадении root fingerprint или повреждении index invariants. На UNC/POLLING периодическая сверка сравнивает directory fingerprint и запускает targeted rescan изменившихся веток.

## 23. Elevated helper и IPC

Основной процесс создаёт local-only Windows named pipe (`PIPE_REJECT_REMOTE_CLIENTS`) со случайным 256-bit именем и security descriptor, разрешающим доступ только текущему user SID и SYSTEM; medium mandatory label блокирует low-integrity writer. Helper запускается через `runas` только с pipe name, expected server PID и session ID. Наследуемые handles и передаваемый secret не используются.

После connect основной процесс получает client PID через `GetNamedPipeClientProcessId`, открывает process/token и проверяет: exact path bundled helper, Authenticode publisher, тот же user SID и logon SID, тот же Windows session, elevated token/high integrity. Helper симметрично использует `GetNamedPipeServerProcessId` и проверяет exact path/signature основного binary, user/logon SID, session и expected PID. Pipe name nonce защищает discovery, а peer token/signature/PID защищают от same-user spoofing; несоответствие fail-closed закрывает pipe до принятия request.

Frame: `u32 little-endian length | u16 protocol_version | u16 kind | u64 request_id | u64 sequence | payload`. Максимум frame — 16 MiB, максимальный outstanding request — 8, sequence строго возрастает в каждом направлении; replay/duplicate request ID/unknown kind завершают session. Payload декодируется с depth/collection limits. Разрешены только `TurboStart`, `PrivilegedSubtreeScan`, `ProtectedDelete`, `Cancel` и health handshake. `PrivilegedSubtreeScan` использует обычное read-only enumeration для локальных NTFS/ReFS/FAT/exFAT и не доступен для UNC. Произвольных command line/handle operations вне typed schema нет.

Ответы: `Accepted`, `Progress`, `ScanEvent`, `ItemResult`, `Completed`, `Cancelled`, `Error`. `Cancel` кооперативен и идемпотентен. Helper владеет NTFS volume handles и delete handles; основной процесс получает только значения и результаты. Helper обслуживает одну сессию, завершается через 30 секунд idle или сразу после disconnect без активной destructive операции. При UAC refusal исходный запрос получает `ElevationDenied`, состояние файлов не меняется.

При crash во время Turbo/privileged rescan вкладка сохраняет snapshot и предлагает Normal/Retry. При crash/disconnect во время delete основной процесс читает защищённый result journal по политике раздела 18, затем делает targeted rescan каждого затронутого parent; неполученный результат обозначается `OutcomeUnknown`, никогда автоматически не повторяется.

## 24. Фильтры, даты и пресеты

Грамматика v1:

```text
expr       := or_expr
or_expr    := and_expr (OR and_expr)*
and_expr   := unary ((AND | implicit_whitespace) unary)*
unary      := NOT unary | '(' expr ')' | predicate
predicate  := field operator value | 'is:' identifier | bare_text
field      := name|ext|path|type|size|allocated|modified|created|age|attr|tag|class
operator   := ':'|'='|'!='|'>'|'>='|'<'|'<='|'~'
value      := quoted_string | escaped_token | size_literal | iso_date | duration
```

Precedence: parentheses, NOT, AND, OR. Quoted strings используют `\"` и `\\`; bare text — case-insensitive substring имени, `~` — glob (`*`, `?`), не regex. Size suffixes `B/KB/MB/GB/TB` десятичные и `KiB/MiB/GiB/TiB` двоичные. Absolute date — ISO `YYYY-MM-DD[Thh:mm[:ss][Z|±hh:mm]]`; без offset трактуется в текущей Windows timezone и при сохранении компилируется в UTC. `age` принимает `s/m/h/d/w`; динамический age-filter пересчитывается раз в минуту.

| Поле | Тип | Допустимые операторы | Нормализация/значения |
|---|---|---|---|
| `name`, `ext`, `path` | string | `: = != ~` | Windows `OrdinalIgnoreCase`; path separators приводятся к `\\`, `.`/`..` синтаксически схлопываются без разрешения reparse; `:` substring, `=` полное равенство |
| `type` | enum | `: = !=` | `file`, `folder`, `stream`, `free`, `unknown`, `reparse`; `:` membership одного значения |
| `size`, `allocated` | bytes/unknown | `= != > >= < <=` | unsigned bytes после разбора suffix; overflow — parse error |
| `modified`, `created` | UTC instant/unknown | `= != > >= < <=` | `=` означает ту же календарную секунду UTC |
| `age` | duration/unknown | `> >= < <=` | `now_utc - timestamp`, отрицательный age clamp к zero |
| `attr` | enum set | `: !=` | `readonly`, `hidden`, `system`, `archive`, `compressed`, `encrypted`, `sparse`, `offline`, `temporary` |
| `tag`, `class` | stable ID + display name | `: = !=` | `:` membership по ID или OrdinalIgnoreCase display name; `=` требует один exact ID/name |

Любая комбинация вне таблицы — parse/type error, не implicit conversion. Class/tag rename не меняет сохранённый stable ID; preset, введённый по имени, компилируется и сохраняется с ID плюс display fallback.

Unknown не проходит `>`, `<` или equality чисел; для него используется `is:unknown`. `is:restricted`, `is:file`, `is:folder`, `is:shared`, `is:reparse` явны. Parse error показывает span и ожидаемые tokens, старый валидный фильтр остаётся активным.

Preset — JSON `{ version: 1, name, expression, sort, scope, created_utc }`, максимум 256 presets и 8 KiB expression. `scope` имеет enum `WholeIndex | CurrentTreemapRoot | CurrentSelection`; при отсутствии selection последний даёт пустой result с подсказкой. Settings defaults перечислены в разделе 15; migrations не меняют смысл сохранённого expression без увеличения preset version.

## 25. Контракт destructive operations

Selection нормализуется до unique canonical roots: descendant удаляется из списка, если уже выбран ancestor. Confirmation строит `ConfirmedManifest` через handle-based recursive enumeration без раскрытия reparse: для каждого entry сохраняются relative normalized name, `VolumeId/FileId`, kind, reparse flag, logical/allocated size, creation/last-write UTC и parent identity. Entries сортируются по `(parent identity, normalized name, file identity)`; BLAKE3 этого canonical byte stream — `directory_fingerprint`. Любая access/I/O ошибка делает manifest неполным и блокирует permanent delete данного root.

Непосредственно перед первым destructive call worker снова строит manifest через root handle и требует byte-for-byte identity/metadata equality с подтверждённым manifest; collision BLAKE3 не является trust boundary, сравниваются записи. При расхождении root получает `ChangedSinceConfirmation` без эффекта. Во время удаления каждый manifest entry открывается с `FILE_FLAG_OPEN_REPARSE_POINT`, identity сверяется по полученному handle, и documented `SetFileInformationByHandle(FileDispositionInfoEx)` применяется к этому же handle. Поэтому path swap после проверки не меняет target. Worker удаляет только identities из manifest; новый/unconfirmed child не удаляется и не даст удалить ancestor.

Permanent delete непустого каталога выполняется post-order по manifest. Reparse object — один leaf. Cancellation проверяется между items/directories: завершённые удаления не откатываются, текущий системный вызов заканчивается, остаток получает `Cancelled`. Если новый child появился после preflight, уже удалённые confirmed entries дают partial result, новый child остаётся, root directory не удаляется. Multi-selection возвращает `Succeeded/Failed/Cancelled/ChangedSinceConfirmation/OutcomeUnknown` по каждому root и child errors; общий status — `Complete`, `Partial` или `NoEffect`.

Volume root и mount root всегда запрещены. Exact protected-root set после `GetFinalPathNameByHandle` normalization: `FOLDERID_Windows`, `FOLDERID_System`, `FOLDERID_SystemX86`, `FOLDERID_ProgramFiles`, `FOLDERID_ProgramFilesX86`, `FOLDERID_ProgramFilesCommon`, `FOLDERID_ProgramFilesCommonX86`, `FOLDERID_ProgramData`, `FOLDERID_Public`, `FOLDERID_UserProfiles`, а также directories обоих binaries. Для любого descendant этих roots обязательны elevated helper, повторный ввод полного canonical path каждого selected root и второй danger-confirm. Device namespaces, alternate data path syntax в качестве root, paths с unresolved reparse component и UNC admin shares (`C$`) запрещены для permanent delete v1.

Recycle Bin использует `IFileOperation`, поддерживает multi-selection и системную отмену. Permanent delete не обещает secure erase. Любая ошибка проверки, IPC или identity работает fail-closed. Результат успешной/частичной операции всегда запускает reconciliation родителей; автоматический retry destructive call запрещён.

## 26. Snapshot и export formats

Snapshot v1 использует little-endian integers и fixed header 128 bytes: offsets `0 magic[4]='VDSP'`, `4 schema_version:u16=1`, `6 header_bytes:u16=128`, `8 flags:u32`, `12 frame_count:u32`, `16 node_count:u64`, `24 uncompressed_total:u64`, `32 metadata_offset:u64=128`, `40 metadata_len:u64`, `48 frame_table_offset:u64`, `56 payload_offset:u64`, `64 created_unix_ns:i64`, `72 file_hash:BLAKE3[32]`, `104 reserved[24]=0`. Unknown nonzero reserved/flag bits отклоняются.

Metadata — RFC 8949 deterministic CBOR map с integer keys: `VolumeId`, root, measurement options, class palette, tag definitions/assignments. Глобальные app settings, history и credentials не сохраняются. После metadata идёт frame table из fixed 64-byte entries: `payload_offset:u64`, `compressed_len:u32`, `uncompressed_len:u32`, `first_node:u64`, `node_count:u32`, `flags:u32`, `frame_hash:BLAKE3[32]`. Каждый payload — один zstd frame с deterministic-CBOR array максимум 65 536 canonical node records; strings UTF-8 NFC, map keys integer и sorted, floating-point отсутствует. `frame_hash` покрывает uncompressed node array; `file_hash` покрывает весь файл с обнулённым диапазоном header `72..104`.

Writer всегда пишет только current schema. В v1 reader принимает только v1: версии 0 нет. Каждый будущий bump обязан добавить ровно именованную deterministic migration `vN-1 -> vN` и frozen golden fixture до расширения compatibility window; более старые/новые версии отклоняются. Migration происходит в памяти и не меняет исходник.

Resource limits по умолчанию: файл ≤4 GiB, uncompressed ≤8 GiB, ≤50M nodes, string ≤32 KiB UTF-8, nesting ≤256, compression ratio ≤100:1. Header и checksum проверяются до публикации; malformed, duplicate IDs, cycles, overflow агрегатов и превышение лимитов отклоняют весь snapshot. Никакой path из snapshot не используется для file operation; snapshot остаётся read-only.

Save/export пишет `.voidspace-tmp-<nonce>` в каталоге назначения, flushes data и directory metadata, повторно проверяет artifact и заменяет target одним atomic replace там, где файловая система это поддерживает. Cancel/error удаляет temp; существующий target не меняется. На filesystem без atomic replace overwrite существующего target запрещён: UI предлагает новое имя, а create-new + verified rename использует fail-if-exists. Поэтому старый файл никогда не перезаписывается частично.

Export scope — один из `Whole scan`, `Selected subtree`, `Current filtered view`, `Selected items`; profile явно сохраняет scope, ordering и поля. CSV — UTF-8 с BOM, RFC 4180 quoting; значения, начинающиеся с `=`, `+`, `-`, `@`, tab или CR, получают leading apostrophe. JSON использует UTF-8 и numeric byte counts. HTML экранирует текст/атрибуты, не включает inline script и добавляет restrictive CSP. Text template AST имеет максимум 64 KiB source, depth 32 и только field/formatter/conditional nodes; paths и names всегда data, не markup/code.

## 27. Release gates

Полный согласованный v1 остаётся единым публичным launch scope, но реализуется проверяемыми внутренними gates:

1. **Foundation:** model/index/reducer/layout, synthetic streaming data, рабочая Spectral UI, navigation и benchmark harness; без доступа к реальным файлам.
2. **Normal Live:** normal scanner, `ReadDirectoryChangesW`, state machine, filters/classes/tags, multi-tab и recovery; read-only файловые действия кроме Open/Explorer/Properties.
3. **Turbo:** signed elevated helper, authenticated IPC, NTFS baseline/USN, UAC flows и adversarial IPC tests.
4. **Safe Actions:** Recycle Bin, permanent delete, protected-root flow, optimistic reconciliation и VHDX destructive suite.
5. **Artifacts & Ship:** snapshots, четыре exports, accessibility, diagnostics/redaction, compatibility/performance matrix и подписанный portable package.

Каждый gate обязан пройти свои unit/integration/negative tests и не может ослаблять инварианты предыдущего. Публичный v1 выпускается только после gate 5; internal builds до него не называются готовым продуктом.

## 28. Traceability и failure gates

| Требование | Владелец | Обязательная проверка |
|---|---|---|
| Потоковый baseline | scan/ntfs → index | concurrent mutation integration + throughput benchmark |
| Постоянная актуальность | watch → reducer | randomized delta, overflow, journal reset, UNC polling |
| Плавная карта | layout/render | property tests + 60-second frame benchmark + DPI screenshots |
| Turbo/UAC | elevated/ntfs | protocol adversarial suite + UAC deny/crash recovery |
| Permanent delete | fileops/elevated | VHDX tests + TOCTOU/reparse/protected-root negatives |
| Snapshot/export | export | malicious fixtures + round-trip + atomicity/injection tests |
| Фильтры/теги | model/app | parser properties + unknown/date/timezone golden tests |
| Privacy | app/export/logging | seeded sensitive paths + automated redaction assertion |

Security, destructive integrity, index invariants, snapshot parser и IPC tests являются P0: любой fail блокирует gate. Accessibility, compatibility и absolute performance thresholds блокируют gate 5. Advisory hardware measurements, не входящие в reference profile, публикуются, но не блокируют release.

## 29. Источники решений

- SpaceSniffer official product, tips, features и release notes: <https://www.uderzo.it/main_products/space_sniffer/>
- Microsoft Change Journals: <https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals>
- Microsoft administrator privilege guidance: <https://learn.microsoft.com/en-us/windows/win32/secbp/running-with-administrator-privileges>
- eframe/egui: <https://docs.rs/eframe/latest/eframe/>
- wgpu: <https://wgpu.rs/doc/wgpu/>
- DaisyDisk: <https://web.daisydiskapp.com/>
- CleanMyMac Space Lens: <https://macpaw.com/support/cleanmymac/knowledgebase/space-lens-results>
