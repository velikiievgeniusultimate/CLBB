# CLBB — Computer Legends Bang Bang

**Русский** · [English](README.en.md)

Экспериментальный Linux-клиент Mobile Legends с маппером на Rust.
Запуск через ATL и QEMU, без Waydroid. Проект независим от MOONTON.

## Установка

Первая тестовая ветка: **Arch Linux, Manjaro и EndeavourOS, x86_64**.
Нужны графическая сессия X11 либо XWayland и актуальные зависимости дистрибутива.
Установщик проверяет требования конкретного релиза, включая версию glibc.
Windows пока не поддерживается.

Одна команда в терминале обычного пользователя, **без `sudo` перед командой**:

```sh
curl -fsSL https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/install.sh | sh
```

Пакетный менеджер может запросить пароль для установки системных зависимостей.
Сам CLBB устанавливается в пользовательскую папку и появляется в меню приложений.
Скрипт выбирает последний опубликованный тестовый или стабильный релиз, проверяет
SHA256 загрузок и оставляет предыдущие версии на месте.

**При чистой установке APK Mobile Legends скачивается автоматически со StarFive**
и проверяется по SHA256 перед импортом. При обрыве соединения загрузка
повторяется с докачкой сохранённой части. Выбирать или копировать файл вручную не нужно.
При повторной установке существующие APK и данные аккаунта сохраняются.
Сам APK не входит в Git-репозиторий и GitHub-релизы; это отдельная загрузка
проприетарной игры, на которую лицензия CLBB не распространяется.
Файл не изменяется и не переподписывается. Вход в свой аккаунт выполняется внутри игры.

Для установки только CLBB без загрузки игры передайте `CLBB_INSTALL_GAME=0`:
при первом запуске появится выбор собственного APK.

Установка с уже имеющимся APK:

```sh
curl -fsSL https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/install.sh | CLBB_APK_FILE="$HOME/mobile-legends.apk" sh
```

Автозагрузка APK с выбранного вами разрешённого HTTPS-источника поддерживается
через `CLBB_APK_URL` и обязательную `CLBB_APK_SHA256`. Подробности:
[установка и распространение](distribution/README.md).

## Управление и обновления

- **F7** — настройки управления, героя, звука, изображения и обновлений.
- **F11** — оконный / полноэкранный режим.
- **F12** — ограничение курсора игровой областью.
- Клавиши переназначаются в F7. Движение и навыки используют независимые контакты.
- **F7 → Обновления → Проверить обновления → Установить обновление**.

Загрузка обновления идёт в фоне. Новая версия включается при следующем запуске;
текущий матч, данные игры и настройки не заменяются. В копии, запущенной непосредственно
из исходников, автоматическая установка отключена: обновляйте код через Git и пересобирайте.

Команды установленной версии:

```sh
~/.local/bin/clbb --version
~/.local/bin/clbb --check-updates
~/.local/bin/clbb --update
~/.local/bin/clbb --import-game /путь/base.apk /путь/split_config.apk
```

## Данные и диагностика

По умолчанию:

| Данные | Путь |
|---|---|
| Версии CLBB | `~/.local/share/clbb/releases/` |
| Игра, APK, кэши и журналы | `~/.local/share/clbb/state/` |
| Настройки | `~/.config/clbb/settings.json` |
| Фрагменты отладки | `~/.local/state/clbb/aim-debug/` |

Поддерживаются переменные XDG. Корень установки можно изменить через
`CLBB_INSTALL_ROOT`. Обновление CLBB не удаляет эти данные.

В F7 → «Горячие клавиши» можно включить запись отладки. **Ctrl+S** сохраняет
фрагмент с пятью секундами до и после метки. Без меток остаётся короткий буфер
в памяти. Запись по умолчанию выключена; максимальный объём фрагментов — 256 МиБ.
Не публикуйте пароли, cookies, токены, данные аккаунта или полные дампы процесса.

Лимит 30–240 FPS — ограничение сверху, не гарантия частоты кадров.

## Исходники и сборка

Репозиторий содержит наш код и патчи; внешние исходники закреплены в `sources.json`
и `.gitmodules`. Для точного исходного дерева релиза проще скачать его архив
`clbb-<версия>-sources.tar.gz`: там уже есть внешние исходники и применённые патчи.
Rust-зависимости включены в `vendor/`.

```sh
git clone https://github.com/velikiievgeniusultimate/CLBB.git
cd CLBB
python3 distribution/bootstrap-sources.py
bash distribution/build-from-source.sh --install-deps
```

Начальная сборка тяжёлая; инструкция и требования находятся в
[distribution/README.md](distribution/README.md). Для уже подготовленного дерева:

```sh
bash build-runtime.sh --check
bash build-runtime.sh
cargo test --manifest-path mapper/runtime/Cargo.toml --lib --offline
cargo test --manifest-path distribution/manager/Cargo.toml --offline
python3 -m unittest discover -s distribution/tests
```

`mapper/` — управление и интерфейс; `launcher/` — запуск; `distribution/` —
установка, обновления и упаковка; `patches/` — изменения внешних компонентов.
Старый эксперимент ETA не является частью этого публичного проекта.

## Лицензии

Новый оригинальный код — **GPL-3.0-or-later**, кроме файлов с явно указанными
другими условиями. Чужие компоненты сохраняют свои лицензии и авторство.
См. [LICENSE_POLICY.md](LICENSE_POLICY.md), [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt)
и [legal/README.md](legal/README.md). К каждому бинарному релизу прилагаются
соответствующие исходники. Бесплатная раздача и будущие добровольные пожертвования
не отменяют обязанности лицензий.

Лицензия CLBB не даёт прав на Mobile Legends и сервисы MOONTON. Использование
стороннего клиента и чтение состояния игры необходимо отдельно сопоставлять
с правилами издателя; открытый код не гарантирует одобрения игры или отсутствия блокировок.

## Прогресс, сбои и чистая переустановка

Установщик показывает этапы и настоящую полосу скачивания. Для проверок и распаковки,
где процент неизвестен, отображается активность и время. Журнал установки находится
в `~/.local/state/clbb/install/` (либо соответствующем XDG_STATE_HOME).

При ошибке запуска создаётся технический JSON-отчёт и появляется окно с причиной,
путём к файлу и кнопкой **«Отправить отчёт на сервер»**. Отправка выполняется только
после нажатия; StarFive возвращает номер отчёта. Технический отчёт не включает
пароли, cookies, файлы аккаунта, дампы памяти или запись игрового процесса.
Локальные отчёты: `~/.local/share/clbb/state/logs/reports/`.

Проверка вручную: `~/.local/bin/clbb --diagnose`. Для старой или не запускающейся
установки доступен независимый инструмент (игру не запускает):

```sh
curl -fL --progress-bar https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/diagnose.sh | sh
```

Полный сброс: сначала закройте CLBB и установщик. Команда удаляет установленный
CLBB, APK, локальные данные аккаунта, настройки, кэши, отчёты и ярлык.
Системные пакеты Linux сохраняются. Исходный checkout проекта не удаляется.

```sh
curl -fL --progress-bar https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/uninstall.sh | sh
```

### Чистая установка и ARM64

Установщик и первый запуск отдельно подготавливают ARM64-библиотеки из base/split
APK. Архитектура ПК не используется для выбора этих файлов. Установщик показывает
отдельный этап; повторные запуски используют проверенный кэш соответствующего APK.
Это исправляет ошибку первых alpha-сборок, где чистая копия могла не найти
`libmain.so`/`libEncryptorP.so`, хотя APK был скачан правильно.

### Аудиоустройства / Audio devices

F7 → **Звук**: выбирайте аудиовывод и микрофон. Выбор сохраняется; «Системное
устройство по умолчанию» следует настройке рабочего стола. Смена устройства
перенаправляет только потоки процесса CLBB. Мониторы выходов скрыты из списка
микрофонов. При отключении выбранного устройства его имя остаётся в списке как
недоступное. Включение голосового чата выполняется в самой игре.

F7 → **Sound** lets you select the output and microphone. Choices persist;
“System default” follows the desktop setting. Routing affects CLBB streams only.
Output monitors are excluded from the microphone picker. Disconnected selections
remain visible as unavailable. Enable voice chat inside the game itself.

Громкость разделена на **Общий уровень**, **Игра** и **Голосовой чат**.
Общий уровень умножается на уровень каждого канала. Голосовой регулятор меняет
громкость собеседников, а не микрофона. Настройки сохраняются; новые каналы по
умолчанию имеют 100%, прежняя общая громкость сохраняется.

Volume has separate **Master**, **Game** and **Voice chat** sliders. Master gain
multiplies both channel gains. Voice controls received speech, not microphone
capture. Channel settings persist and default to 100%; existing master volume is
preserved. Android VOICE_CALL/BLUETOOTH_SCO playback is routed to voice gain;
other AudioTrack streams and the game's OpenSL ES playback use game gain.

Все источники игры и войсчата смешиваются в Rust и выводятся через один системный
поток CLBB. Регулятор в системном микшере общий. Захват микрофона остаётся отдельным
входом. Частоты источников приводятся к общему стереовыходу 48 кГц.

Game and voice sources share one Rust mixer and one desktop playback stream.
The system mixer controls their combined output; microphone capture stays a
separate input. Source sample rates are converted to 48 kHz stereo internally.

AudioRecord PCM16 capture is implemented in Rust with a bounded 200 ms queue and
10 ms capture fragments. Java/JNI adapters support short arrays, byte arrays and
direct buffers, blocking/nonblocking reads, and explicit initialization failures.
Diagnostic counters contain no saved voice recordings. Requires a local
PulseAudio-compatible server (including PipeWire-Pulse) and `libpulse`/`pactl`.

Checks: `python3 smoke/check-audiorecord-jni.py`,
`python3 smoke/check-microphone.py` (local synthetic source),
`bash mapper/ui/check-preview.sh` (isolated settings UI).

### Частота кадров / Frame rate

F7 → «Основное» → «Показывать FPS»: счётчик новых игровых кадров, включённых GTK
в отрисовку. Он не считает повторные перерисовки HUD и не измеряет физическую
развёртку монитора. Частота передачи текстур и частота отрисовки доступны отдельно
в `diagnostics.render` (`gtk_handoff` / `gtk_snapshot`).

Для GL/EGL-окна CLBB управляет интервалом вывода согласно переключателю VSync,
устраняя отдельное ограничение GTK/X11 в 60 Гц. Настройки рабочего стола и драйвера
не меняются. Явные альтернативные `GSK_RENDERER=cairo/vulkan` сохраняют штатную
политику часов GTK; они не проверены как высокочастотный путь этой сборки.

The FPS overlay counts unique game textures included in GTK snapshots, rather
than all texture handoffs or physical monitor scan-out. GL/EGL presentation follows
CLBB's VSync setting without changing desktop or driver-wide settings. Explicit
alternative renderer overrides retain their standard GTK frame-clock policy.

Runtime profiling: `python3 tools/profile-live-fps.py --seconds 15 --output REPORT.json`.
Opt-in `CLBB_PROFILE_EGL=1` logs aggregate producer, queue and buffer-wait timings.
