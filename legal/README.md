# Лицензии и упоминания / Licenses and notices

## Русский

`../THIRD_PARTY_NOTICES.txt` — черновой список сторонних компонентов.
`third-party-inventory.json` содержит версии, происхождение, ссылки на
собранные тексты и контрольные суммы. Оригиналы текстов находятся в `third-party/`.
Оригинальные лицензии не переводятся и не заменяются лицензией CLBB.

Обновление реестра без запуска или остановки игры:

```sh
python3 tools/collect-license-notices.py
```

Скрипт учитывает закреплённые исходники, разрешённые Cargo зависимости,
системные библиотеки, замеченные в работающем runtime или найденные по ELF,
и уведомления из подготовленного prefix. Сборочные зависимости помечены
отдельно. Это широкий реестр окружения разработки, а не точный состав установщика.
Состав конкретной сборки перечислен в её release.json. Системные пакеты
ставятся через pacman и не включаются в архив CLBB.

GPL не запрещает коммерческое распространение и добровольные пожертвования.
Обязанности лицензий возникают и при бесплатной раздаче сборок. Одного
перечисления авторов недостаточно: для релиза нужно проверить совместимость
лицензий, сохранить обязательные уведомления и обеспечить предусмотренную
лицензиями передачу соответствующих исходников с нашими изменениями и сборочными
инструкциями. Конкретные незакрытые пункты перечислены в `remaining_review`
в JSON-реестре. Новый оригинальный код по умолчанию лицензирован GPL-3.0-or-later; существующие
лицензии отдельных файлов и компонентов продолжают действовать. См. `../LICENSE_POLICY.md`.

APK, ресурсы игры, аккаунты, cookies и игровые записи не входят в этот набор.
Разрешения открытых библиотек не предоставляют разрешений на распространение
Mobile Legends или на использование сервиса MOONTON сторонними инструментами.

## English

`../THIRD_PARTY_NOTICES.txt` is a draft third-party component inventory.
`third-party-inventory.json` records versions, provenance, collected texts and
their hashes. Original license and notice texts are in `third-party/`.
Original licenses are not translated or replaced by a CLBB license.

Regenerate without launching or stopping the game:

```sh
python3 tools/collect-license-notices.py
```

The collector inventories pinned source trees, resolved Cargo dependencies,
system libraries observed in the owned runtime or found through ELF dependencies,
and notices in the prepared prefix. Build dependencies are labelled separately.
This is a broad development-environment inventory, not the installer bill of
materials. Each build lists its exact payload in release.json. System packages
are installed through pacman and are not bundled in the CLBB archive.

The GPL permits commercial distribution and voluntary donations. License
obligations also apply to free binary distribution. Attribution alone is not
enough: a release needs compatibility review, required notices and the applicable
delivery of corresponding source, including project modifications and build
instructions. Open items are listed under `remaining_review` in the JSON inventory.
New original code defaults to GPL-3.0-or-later; existing file-level
and component licenses remain in effect. See `../LICENSE_POLICY.md`.

Game APKs, game resources, accounts, cookies and gameplay recordings are excluded.
Open-source library permissions do not grant permission to redistribute Mobile
Legends or to use MOONTON's service with third-party tools.
