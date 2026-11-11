# Диагностическая запись / Diagnostic flight recording

F7 → Горячие клавиши → Запись отладки. Ctrl+S (сочетание настраивается)
ставит метку. Сохраняются пять секунд до и после; близкие метки объединяются.
Выключение сохраняет незавершённый фрагмент с `truncated=true`.
Файлы: `$XDG_STATE_HOME/clbb/aim-debug/clip-*.jsonl` (обычно ~/.local/state).

Схема 2 связывает:
- событие клавиши/мыши до обработки окном, фокус, панель настроек, блокировку курсора;
- решение маршрутизации, очередь навыков, настройки и полное состояние контроллера;
- запрошенную и переданную координату каждого пальца, результат транспортного вызова;
- реальные MotionEvent: номер пакета, Android ID всех пальцев, action/index,
  координаты, downTime/eventTime, результат dispatch и поток чтения MotionEvent;
- типизированные исходные данные: центр/радиус/камера, источник и возраст геометрии,
  направление, отмену, варианты кнопок и откаты, включая общие кнопки;
- числовые поля объектов управления и кнопок, блокировки, привязки пальцев;
- точную причину отклонения обратной связи и доступный результат применения навыка.

Снимок источника сопровождает каждое успешное чтение; полные типизированные кадры
и состояния сохраняются при переходах и раз в 32 мс, аудит объектов — раз в 20 мс.
У каждого аудита есть интервал чтения и результат проверки согласованности.
Пакеты ввода и команды пальцев не прореживаются. Повторяющаяся одинаковая причина
отклонения направления записывается с интервалом до 50 мс, переходы — сразу.
Отключённая запись не включает аудит или JNI-вызовы из геттеров MotionEvent.

Метаданные каждого фрагмента содержат сборку, настройки, список каналов,
фактический охват и признак обрезки. Ошибки чтения остаются в записи.
Автометки включают долгое ожидание отпускания, расхождение, устаревший источник,
нулевой native touch при активном навыке и отказ транспорта.

Проверка записи:
`python3 tools/analyze-flight.py [путь-к-фрагменту]`

Отсутствующий канал явно показывается анализатором, но само по себе не означает
ошибку: в меню нет результатов навыков. Чтение геттера MotionEvent подтверждает
чтение Java-события указанным потоком, а не завершение игровой симуляции.
Это локальная телеметрия управления, а не полный дамп памяти процесса.
Неизвестная будущая механика может потребовать новых наблюдений; запись не должна
скрывать отсутствие данных или выдавать предположение за доказанную причину.

## English

Enable recording in F7 → Hotkeys. Ctrl+S (configurable) retains five seconds
before and after a mark. Disabling flushes partial clips with `truncated=true`.
Schema 2 correlates host routing, controller decisions/queue/settings, requested
and encoded contacts, transport results, numbered Android MotionEvents and their
reader threads, complete typed input snapshots, cooldowns/variants/cancellation,
control-object numeric audits, and available skill results. Snapshot transitions
are preserved; periodic full snapshots are 32 ms and object audits 20 ms.
Headers identify build, settings, observed channels, coverage and truncation.
Repeated identical direction rejection reasons have a 50 ms heartbeat; individual
touch commands and Android packets are not thinned. Disabled recording skips the
extra audits and MotionEvent getter callbacks. The analyzer lists absent channels
and separates transport, dispatch, reader and controller failures. A Java getter
read is evidence of event access, not proof of completed game simulation.

## Направленные навыки и отложенное применение / Direction gestures and buffered casts

Обычный направленный навык завершает палец по физическому отпусканию: свежая
геометрия → конечный MOVE → UP в том же порядке. Обновление игрового индикатора
может прекращаться во время отложенного применения другого навыка. Его отсутствие
не должно удерживать уже отпущенную клавишу и блокировать следующие нажатия.
До отпускания направление продолжает следовать курсору. Отмена остаётся отдельным
действием; навыки с дальностью и моментальный режим сохраняют свои проверки.
Событие диагностики: `physical_up_after_final_direction_move`.

An ordinary direction gesture commits its final MOVE then UP on physical release,
using fresh geometry and successful transport. Buffered game actions may stop
indicator updates; such missing visual feedback must not hold a released key or
block subsequent presses. Held aiming, explicit cancellation, range gestures and
instant-mode receipt checks remain separate. Diagnostic reason:
`physical_up_after_final_direction_move`.

Если игра не открыла индикатор после DOWN, обычный направленный навык всё равно
завершается по физическому UP: конечный угол вычисляется из текущей принадлежащей
этой кнопке геометрии и камеры (кадр не старше 100 мс). Это не подтверждение прицела
игрой. Путь запрещён для удержания, моментального режима и сменившейся формы с
дальностью. Диагностика: `final_geometry_from_fresh_button`.

When DOWN did not open an indicator, ordinary direction UP may use the owned
current button geometry/camera from a frame no older than 100 ms. It still sends
final MOVE then UP and is not a native aim receipt. Held/instant gestures and a
changed range form cannot use this path. Trace: `final_geometry_from_fresh_button`.
