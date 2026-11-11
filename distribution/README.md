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

Порядок для сопровождающего:

```sh
python3 tools/export-runtime-patches.py --write
python3 tools/collect-license-notices.py
# Проверьте и зафиксируйте все исходники перед упаковкой соответствующего source archive.
python3 distribution/package-release.py --output /tmp/clbb-release
python3 distribution/package-release.py --source-only --output /tmp/clbb-release
sha256sum /tmp/clbb-release/clbb-* > /tmp/clbb-release/SHA256SUMS
```

В публикуемом `SHA256SUMS` должны быть только базовые имена файлов, а не локальные
пути: выполняйте финальную команду в каталоге релизных файлов. Релиз создаётся
черновиком, загружаются все четыре файла, затем он публикуется. Нельзя удалять
соответствующие исходники, пока раздаётся связанная с ними сборка.

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
