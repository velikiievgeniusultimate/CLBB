# Проверки персонажей / Hero validation

Профиль, распознанный тип и успешный выход скрипта не означают, что герой готов.
Сначала прочитайте **полные описания в установленной игре**, включая пассивку и
другие формы. Из них составьте сценарии: нужная цель, попадание, накопление,
смена формы, повторное применение, дополнительная кнопка. Затем воспроизведите
условия в тренировке и проверьте применение, отпускание, отмену и движение.
Окончательную отметку пользователь ставит сам. Запрещённые к изменению герои
передаются инструментам через явный frozen-manifest.

## Описания из интерфейса

Откройте панель описаний стрелкой справа и включите «Полный». Для стандартной
панели с тремя навыками и пассивкой:

```sh
sudo -n python3 tools/read-training-descriptions.py --hero HERO_ID --frozen-manifest MANIFEST.json --output NEW_DIRECTORY
```

Инструмент выбирает только наблюдаемые значки, читает локализованные `UILabel`
из `UIPlayerInfo`, сохраняет текст, исходные дампы и скриншоты. Цифры из
`CData_Skill.m_SkillDescription` могут быть ключами локализации, а не описанием.
У `UIImageableLabel` диагностический флаг видимости иногда отличается от
видимого текста: сверяйте скриншоты. Не переносите строки невидимых старых
виджетов на новый навык. Неизвестная панель останавливает чтение.

После чтения инструмент закрывает панель наблюдаемой кнопкой гаечного ключа
и проверяет, что закрыты и описания, и GM. Сохранён `panel-closed.png`.
Переключатель формы внутри описаний **не превращает героя в игре**.
Отменённые или перехваченные панелью нажатия не являются тестом навыка.

## Текущие цели и призванные существа

```sh
cargo build --manifest-path mapper/runtime/Cargo.toml --locked --offline --bin training-actors-audit
sudo -n mapper/runtime/target/debug/training-actors-audit PID NEW_OUTPUT.json
sudo -n mapper/runtime/target/debug/training-actors-audit PID NEW_OUTPUT.jsonl --watch-ms 10000
```

Rust-инструмент только читает память выбранной собственной сборки. Он проверяет
личность процесса и текущего матча, типы полей, список и массив ссылок на
`ShowEntity`, повторно проверяет позицию и идентичность существа. Читает
`m_ShowPlayers` и `m_ShowPlayerSummons`: GUID, игровой ID, сторону, здоровье,
отрисовываемые координаты и время наблюдения. Ошибочная/устаревшая строка не
является целью. Это диагностические данные, не разрешение для production-ввода.

В режиме наблюдения первая строка — снимок метаданных, затем строки `actors`.
Строка `READY` в stdout относится только к подключению наблюдателя. Полное
наблюдение заканчивается JSONL-строкой `summary` с `complete: true`. При анализе
сохраняйте отказы и неполные записи, не подменяйте их успешным повтором.

После страха/отталкивания используйте свежую позицию и свежие матрицы камеры.
Счётчик применения подтверждает применение, но сам по себе не доказывает
попадание. Для повторной кнопки подтвердите текущий вариант, его доступность,
удержание и результат. Сопоставляйте GUID призванного существа до/после действия.
`cast_locked: false` не означает отсутствия КД: отдельно проверяйте
`VirtualButton.m_CdCoolDown`. `ready: true` также может сопровождаться КД.

Все игровые действия выполняются одним координатором последовательно, через
приватный сокет собственной игры. Физический курсор и фокус пользователя не
перемещаются. Не запускайте одновременно два инструмента ввода. Менять код
Rust-маппера можно через `tools/reload-mapper.py`; требования и ограничения
описаны в `LIVE-RELOAD.md`. Диагностические бинарники не требуют замены игры.

## English

Read the installed game's expanded skill/passive descriptions first. Derive
and reproduce target, hit, stack, form and recast conditions before calling a
hero ready for user review. Successful scripts or inferred templates alone are
insufficient. User confirmation remains separate.

`read-training-descriptions.py` reads localized UI labels with screenshots and
checks panel closure; it supports the observed three-skill panel only.
`training-actors-audit` provides bounded read-only snapshots/JSONL watches of
owned live players and summons, with identity, type and coherence checks.
Use fresh positions after displacement. Application counters alone do not prove
hits. Verify active variants, actual cooldowns, held input, cancellation and
observable effects. Serialize gameplay input; never move the physical cursor.
