# Voidspace: дизайн Windows-анализатора дискового пространства

Дата: 2026-08-25  
Статус: утверждённый дизайн, готов к планированию реализации  
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
- локальные, съёмные и UNC-пути в обычном режиме;
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

Эталонный макет проверен на 1440×900, 1280×720 и 1024×768: пересечений соседних элементов и выходов дочерних блоков за границы нет.

## 6. Основные пользовательские потоки

### 6.1 Запуск и выбор области

Стартовый экран показывает доступные тома с ёмкостью, занятым и свободным местом. Пользователь выбирает том, папку, UNC-путь или сохранённый snapshot. Доступны кнопки Scan и Turbo Scan. Turbo Scan объясняет необходимость UAC и доступен только для NTFS.

### 6.2 Сканирование

После запуска сразу открывается рабочая вкладка. Карта, счётчики и список крупнейших объектов обновляются потоково. Пользователь может поставить задачу на паузу, отменить её или повторно просканировать выбранную ветку. Несколько вкладок сканирования независимы.

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

### 6.5 Файловые действия

Для выбранного объекта доступны:

- Open и Open in Explorer;
- Copy path;
- Move to Recycle Bin;
- Delete permanently;
- Properties;
- Rescan branch.

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

Turbo Scan не парсит сырые структуры диска вручную и не пишет на том.

### 8.5 `voidspace-watch`

Поддерживает живой индекс.

- В обычном режиме регистрирует `ReadDirectoryChangesW` для сканируемого корня и буферизует события.
- В NTFS Turbo-режиме хранит USN cursor и читает Change Journal.
- Cursor/наблюдение запускается до baseline scan. После baseline накопленные события воспроизводятся через reducer, что закрывает race между сканированием и изменениями.
- События группируются окнами 30–60 мс и нормализуются в create/delete/modify/rename/move.
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

### 8.8 `voidspace-fileops`

Выполняет Open/Explorer/Properties, корзину и permanent delete на отдельном worker. Для корзины используется Windows Shell `IFileOperation`. Permanent delete удаляет файлы и пустеющие каталоги без перехода в корзину.

Reparse point удаляется как ссылка; target никогда не обходится. Результат возвращается по каждому объекту отдельно. Успешные операции преобразуются в `FsDelta`; watcher затем подтверждает фактическое состояние.

### 8.9 `voidspace-export`

Сериализует snapshot и экспортные форматы потоково, чтобы не копировать всё дерево. Template parser создаёт безопасный AST разрешённых полей и форматтеров.

### 8.10 `voidspace-app`

Содержит eframe lifecycle, вкладки, command routing, keyboard shortcuts, настройки и композицию экранов. UI не мутирует индекс напрямую: он отправляет команды и получает immutable snapshots/events.

### 8.11 `voidspace-elevated`

Минимальный helper с манифестом `requireAdministrator`. Принимает строго типизированные запросы Turbo Scan и защищённых операций по локальному authenticated IPC. Проверяет вызывающий процесс, нормализует пути и не предоставляет универсальную командную оболочку.

## 9. Поток данных и конкурентность

1. UI создаёт `ScanRequest`.
2. Scan coordinator фиксирует watch cursor, запускает scanner и передаёт события в bounded channel.
3. Index reducer применяет события пакетами и публикует immutable snapshot/version.
4. Layout worker получает snapshot и dirty-set, создаёт новый layout snapshot.
5. UI атомарно заменяет отображаемый snapshot и renderer интерполирует старые/новые прямоугольники.
6. Watcher продолжает выдавать `FsDelta` после baseline scan.
7. File operation возвращает per-item result; успешный результат применяется немедленно, затем сверяется с watcher.

UI thread никогда не ждёт disk I/O, UAC helper, экспорт или полное построение layout. Bounded channels предотвращают неограниченный рост памяти. Cancellation проходит от вкладки к scan/watch/layout/export tasks.

## 10. Размеры и идентичность файлов

- Logical size показывает длину файла.
- Allocated size показывает реально выделенное место, включая sparse/compressed semantics.
- Основная карта по умолчанию использует allocated size; режим переключается.
- Hard links идентифицируются по `VolumeId + FileId`. Logical size виден у каждой ссылки, а allocated total тома не учитывает одну и ту же запись повторно. В UI ссылка имеет индикатор shared allocation.
- ADS сканируются только при включённой настройке и отображаются как дочерние stream nodes.
- Free space и Unknown space показываются как отдельные системные узлы.

## 11. Live-пересчёт

Изменение узла создаёт signed size delta. Reducer применяет его к узлу и всем предкам до корня, обновляет сортировочные ключи и формирует `DirtySet`. Layout перестраивает только затронутые ветки текущего viewport.

При удалении из Voidspace успешный результат worker немедленно tombstone-ит узел и вычитает агрегаты. Если watcher сообщает противоречие, targeted rescan восстанавливает истину. При внешнем удалении используется тот же reducer path.

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

Перед выполнением пути канонизируются, проверяются на prefix confusion, device namespace, alternate syntax и изменение между подтверждением и операцией. IPC helper принимает конкретный список уже подтверждённых canonical paths и operation nonce.

## 14. Ошибки и восстановление

- Access denied: узел `Restricted`, штриховка, счётчик неизвестного размера и действие Retry as administrator.
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

- scan: ADS, reparse policy, logical/allocated default, normal/Turbo preference;
- live updates: enabled, debounce 30–250 мс, filesystem watcher health;
- geometry: initial detail, minimum tile size, sorting, free/unknown visibility;
- motion: transition duration, reduced motion, target FPS;
- appearance: contrast, border, hover halo, selection shadow, file classes, tags;
- behavior: tooltip details, timestamps/age, notification on scan complete;
- privilege: always request administrator rights;
- privacy: redact paths in copied diagnostics and export presets.

## 16. Производительность и критерии приёмки

- UI остаётся отзывчивым во время scan/watch/export/delete; ни один disk I/O call не выполняется на UI thread.
- Видимая карта поддерживает 60 FPS на целевом GPU; LOD ограничивает число текстовых labels и rendered rectangles.
- Типичное локальное изменение отображается с p95 менее 100 мс.
- Первые nodes появляются до завершения root scan.
- Turbo Scan на NTFS должен быть быстрее обычного режима на одном и том же наборе и машине; точные коэффициенты фиксируются benchmark baseline до оптимизаций.
- Memory benchmark измеряется на синтетических индексах 1M, 5M и 10M nodes; отсутствие копии полного пути в каждом node обязательно.
- Закрытие/отмена вкладки освобождает workers, handles и watcher registrations без утечки.

Численные scan throughput и memory ceilings утверждаются после первого benchmark harness, поскольку они зависят от диска, файловой системы, длины имён и распределения каталогов. Это не разрешает выпуск без регресс-бюджета: baseline сохраняется в CI artifacts и сравнивается на каждом performance milestone.

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

Телеметрии нет. Сеть не требуется для сканирования, карты, удаления, snapshots или экспорта. Логи локальные, с ротацией и без содержимого файлов. Экспорт diagnostics по умолчанию редактирует пользовательские пути; пользователь может явно включить полные пути.

## 19. Поставка первой версии

Первая поставка — подписываемый portable x86-64 executable с локальным elevated helper и конфигурацией в пользовательском профиле. Приложение не требует фонового сервиса. Installer и автообновление не являются условием первой функциональной версии.

## 20. Источники решений

- SpaceSniffer official product, tips, features и release notes: <https://www.uderzo.it/main_products/space_sniffer/>
- Microsoft Change Journals: <https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals>
- Microsoft administrator privilege guidance: <https://learn.microsoft.com/en-us/windows/win32/secbp/running-with-administrator-privileges>
- eframe/egui: <https://docs.rs/eframe/latest/eframe/>
- wgpu: <https://wgpu.rs/doc/wgpu/>
- DaisyDisk: <https://web.daisydiskapp.com/>
- CleanMyMac Space Lens: <https://macpaw.com/support/cleanmymac/knowledgebase/space-lens-results>

