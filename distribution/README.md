# Установка, обновления и релизы / Installation, updates and releases

## Русский

Первый пакет рассчитан на Arch-family Linux x86_64 с X11/XWayland. Минимальная
glibc вычисляется из импортируемых символов ELF при упаковке. На неподходящей
системе установка завершается до переключения версии. Системные GTK, WebKit,
драйверы и шрифты устанавливаются через pacman, а не копируются с рабочей машины.

Установщик и менеджер используют публичный GitHub Releases API. Каждый релиз содержит:

- `clbb-<версия>-linux-x86_64.tar.gz` — runtime без APK и личных данных;
- `clbb-manager-linux-x86_64` — загрузчик первоначальной установки;
- `clbb-<версия>-sources.tar.gz` — соответствующие исходники с патчами;
- `SHA256SUMS` — контрольные суммы загрузок.

Контрольные суммы получаются с того же доверенного GitHub-источника через HTTPS;
отдельной криптографической подписи релизов пока нет. Файлы runtime дополнительно
проверяются по `release.json`. Это не резервная копия аккаунта.

`install.sh` запускает менеджер, устанавливает недостающие системные зависимости
с подтверждением pacman, затем добавляет ярлык. Кнопка обновления в игре не выполняет
sudo: если новой версии нужны дополнительные пакеты, она предлагает повторить
установщик в терминале. Обновления сериализованы файловой блокировкой ядра;
новый каталог версии проверяется до атомарного переключения `current`.
Работающая игра продолжает использовать прежний каталог. Предыдущие версии
не удаляются автоматически.

### APK

Обычная установка CLBB с автоматическим скачиванием и импортом APK со StarFive:

```sh
curl -fsSL https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/install.sh | sh
```

Автоматический импорт своего локального APK:

```sh
curl -fsSL https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/install.sh | CLBB_APK_FILE="$HOME/mobile-legends.apk" sh
```

Автозагрузка из разрешённого вам источника:

```sh
curl -fsSL https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/install.sh | CLBB_APK_URL="https://your-authorized-source.example/game.apk" CLBB_APK_SHA256="64_HEX_CHARACTERS" sh
```

Последний URL — пример, а не действующий сервер. Менеджер проверяет SHA256,
пакет `com.mobile.legends` и одинаковую версию выбранных split APK. Импорт не
переподписывает APK, не копирует аккаунты и не удаляет `state/data`. Предыдущий
набор APK сохраняется в `state/game-backups/`. Импорт при работающей игре запрещён.

`game-apk.json` содержит HTTPS-адрес APK на StarFive, версию, размер и SHA256.
Установщик использует его автоматически, если игра ещё не установлена и не заданы
`CLBB_APK_FILE`/`CLBB_APK_URL`. `CLBB_INSTALL_GAME=0` отключает этот шаг.
Установленные APK и аккаунт по умолчанию не заменяются. Файл предоставлен владельцем;
APK остаётся проприетарным, не входит в исходники CLBB и не перелицензируется.
Размещение файла не является заявлением о разрешении MOONTON на сторонние клиенты.

### Сборка релиза

`build-from-source.sh --install-deps` подготавливает исходное дерево и локальный
prefix по upstream-инструкциям. Первичный native bootstrap не является лёгкой
операцией; для изменения готового runtime используйте `build-runtime.sh`.
Образцы установки и обновления проходят изолированные проверки без реального аккаунта.

Рабочая копия исходников и запускаемый пакет теперь различаются явно. Ярлык
разработчика запускает выбранный неизменный пакет из `~/.local/share/clbb-workbench/`,
а его игровые данные остаются в прежней папке проекта. Сборочный `prefix/` не
подменяет этот пакет. Локальная проверочная копия переключается через workbench;
обновление через F7 предназначено для обычной установки тестера.

Проверить исходники и выбранную локальную версию:

```sh
python3 distribution/release.py status
```

Порядок следующего выпуска:

1. Изменить код и `VERSION` на новый номер. Выгрузить патчи и зафиксировать
   исходники: `python3 tools/export-runtime-patches.py --write`, затем Git commit.
   Убедиться, что нужные лицензии и уведомления обновлены.
2. Собрать кандидата одной командой ниже. Перед сборкой закрыть локальную игру:
   существующий сборочный скрипт проверяет использование библиотек. Первичная
   подготовка зависимостей по-прежнему выполняется `build-from-source.sh`;
   после смены upstream-версий сначала нужен полный bootstrap.
3. Переключить локальный ярлык на кандидата и проверить реальную игру. Запись
   `accept` — это описание реально проведённых проверок, а не замена тестированию.
4. Опубликовать те же архивы. Повторная сборка между проверкой и публикацией не нужна.

```sh
python3 distribution/release.py prepare "$HOME/.local/share/clbb-candidates/v0.1.0-alpha.8"
python3 distribution/release.py use-local "$HOME/.local/share/clbb-candidates/v0.1.0-alpha.8"
# После реальных проверок указать их фактический результат:
python3 distribution/release.py accept "$HOME/.local/share/clbb-candidates/v0.1.0-alpha.8" --note "Описать проверенные экраны и управление"
python3 distribution/release.py publish "$HOME/.local/share/clbb-candidates/v0.1.0-alpha.8" --notes /absolute/path/release-notes.md
```

`prepare` требует чистые зафиксированные исходники и совпадение сохранённых патчей
с внешними деревьями, собирает runtime, запускает проверки, создаёт бинарный пакет,
соответствующие исходники и `SHA256SUMS`. `candidate.json` фиксирует коммит и хеши.
Новые пакеты содержат `REVISION` и `source_commit` в `release.json`; номер коммита
виден в F7 и через `clbb --build-info`. Старые выпуски этих полей не имели.

`verify` проверяет архивы и извлечённый runtime, включая отсутствие лишних файлов.
`use-local` сохраняет проверенную копию пакета в постоянную папку workbench,
поэтому удаление временного каталога не ломает ярлык. `accept` привязывает запись
проверки к хешам. `publish` отказывается публиковать изменившиеся файлы или другой
коммит под существующим тегом; загружает все четыре файла в черновик, сверяет
серверные хеши и требует успешные проверки GitHub. Прерванную загрузку черновика
можно продолжить повторным `publish`.

Для выбора уже опубликованного выпуска сначала скачать четыре его файла в одну
папку, затем выполнить `import-release <папка> --tag <версия>` и `use-local <папка>`.
Импорт сверяет хеши с GitHub и коммит тега. Соответствующие исходники нельзя удалять,
пока раздаётся связанная с ними бинарная сборка. Инструмент не выполняет очистку
игровых данных и никогда не добавляет их в публикуемые файлы.

## English

The initial package targets Arch-family Linux x86_64 with X11/XWayland. Packaging
calculates minimum glibc from ELF imports. Incompatible systems are rejected before
activation. GTK, WebKit, drivers and fonts come from pacman, not copies of host libraries.

Each GitHub release contains a runtime archive without the game or private data,
a bootstrap manager, a corresponding-source archive, and `SHA256SUMS`. Checksums
are fetched over HTTPS from the same trusted GitHub origin; separate cryptographic
release signatures are not implemented yet. Payload files are additionally checked
against `release.json`.

The installer can install missing system packages through an interactive pacman
confirmation. The in-game updater never invokes sudo: new dependencies require
rerunning the terminal installer. A kernel file lock serializes updates. Releases
are staged separately, verified, then activated through an atomic `current` link.
The running game keeps using its old release directory. Previous releases are retained.

The commands above cover normal installation, automatic local APK import through
`CLBB_APK_FILE`, and an authorized HTTPS download through `CLBB_APK_URL` plus mandatory
`CLBB_APK_SHA256`. The example download hostname is not a live service. APK import
checks the package and matching split versions, keeps original bytes, preserves
account data, backs up old APKs and refuses to run while CLBB is active.

`game-apk.json` records the StarFive HTTPS URL, version, size and SHA256 for the
owner-provided APK. With no explicit APK override, the installer downloads and
imports it when the game is absent. `CLBB_INSTALL_GAME=0` disables this step.
Existing APKs and account data are preserved by default. The game remains
proprietary and is not included in the CLBB repository or GitHub releases.
CLBB licensing does not grant game rights or imply MOONTON service approval.

`build-from-source.sh --install-deps` prepares pinned sources and a local prefix
following upstream build instructions. Native bootstrap is substantial. Use
`build-runtime.sh` for an existing prepared runtime. Maintainers must commit the
exact sources, export patches and notices, build runtime and source companions,
and produce SHA256 sums with basenames from the output directory. Upload every
asset to a draft release before publishing it. Preserve corresponding sources
for every binary version still distributed.

### Active developer runtime / Запущенная рабочая копия

Do not install native build output over a running game. Build in `src/atl/build`
without installing and stage new artifacts using package-release.py flags
`--main-executable src/atl/build/android-translation-layer` and
`--main-library src/atl/build/libtranslation_layer_main.so`.
Не заменяйте бинарники активной рабочей копии: эти флаги включают новые сборочные
артефакты только в релизный архив. Обычная сборка из исходников устанавливает
актуальные артефакты в prefix и в этих флагах не нуждается.

### Maintainer workbench

Use `python3 distribution/release.py status` to distinguish the source checkout
from the selected local runtime. Developer shortcuts run an immutable package in
`~/.local/share/clbb-workbench/` while keeping the existing project game profile.
The development build prefix remains separate. Tester installations keep the
normal F7 update flow; developer packages are selected through the workbench.

Commit the changed code, exported patches and a new VERSION, then use the
`prepare`, `use-local`, `accept --note`, and `publish --notes` commands shown above.
Close the local game before rebuilding the prefix. Upstream dependency changes
require the full source bootstrap first. Preparation runs checks and creates both
binary and corresponding-source archives once. Candidates contain revision and
asset hashes; local selection survives removal of temporary build directories.
Actual game testing is required before recording acceptance. Publication uploads
the exact accepted files as a draft, checks remote digests and successful GitHub
checks, then publishes. Draft uploads can be resumed; existing published versions
cannot be replaced. `import-release` verifies an existing release before local use.
New packages expose the source revision in F7 and `clbb --build-info`.

## Окно обновления / Update window

F7 → Основное → «Обновить CLBB» завершает игру и открывает отдельное окно.
До подтверждения «Обновиться» загрузка и установка не начинаются. Полоса сверху
показывает загрузку, строка состояния — текущий этап (проверка, распаковка,
активация). После успеха предлагается запуск CLBB; ошибка оставляет возможность
повторить попытку. При установке окно нельзя закрыть посередине активации.
Реальные игровые данные и настройки хранятся отдельно от обновляемой версии.
Локальная рабочая сборка (`CLBB_MANAGED_INSTALL=0`) не перезаписывается менеджером:
это окно объясняет ограничение; обычные установки тестеров обновляются через него.

F7 → Main → Update CLBB closes the game and opens a separate Rust/GTK window.
No installation starts until Update is confirmed. Download progress and the
current stage remain visible; success offers a launch of the newly selected
release, failure allows retry. Busy activation cannot be interrupted by closing
the window. Development workbench builds retain their explicit no-overwrite
policy; managed tester installations use the normal updater.

Validation without game/account/network access:
```sh
bash launcher/check-update-handoff.sh
bash mapper/ui/check-update-window.sh
bash mapper/ui/check-preview.sh
```
