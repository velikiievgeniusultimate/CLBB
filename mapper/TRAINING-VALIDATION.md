# Проверки персонажей / Hero validation

Профиль, распознанный тип и успешный выход скрипта не означают, что герой готов.
Сначала прочитайте **полные описания в установленной игре**, включая пассивку и
другие формы. Из них составьте сценарии: нужная цель, попадание, накопление,
смена формы, повторное применение, дополнительная кнопка. Затем воспроизведите
условия в тренировке и проверьте применение, отпускание, отмену и движение.
Окончательную отметку пользователь ставит сам. Запрещённые к изменению герои
передаются инструментам через явный frozen-manifest.

## Описания из интерфейса

Откройте панель описаний стрелкой справа и включите «Полный». Для наблюдаемых
панелей с тремя или четырьмя навыками и пассивкой:

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
sudo -n mapper/runtime/target/debug/training-actors-audit PID NEW_OUTPUT.jsonl --watch-ms 10000 --monsters
```

Rust-инструмент только читает память выбранной собственной сборки. Он проверяет
личность процесса и текущего матча, типы полей, список и массив ссылок на
`ShowEntity`, повторно проверяет позицию и идентичность существа. Читает
`m_ShowPlayers` и `m_ShowPlayerSummons`: GUID, игровой ID, сторону, здоровье,
отрисовываемые координаты и время наблюдения. Ошибочная/устаревшая строка не
является целью. Это диагностические данные, не разрешение для production-ввода.

Опция `--monsters` дополнительно читает `m_ShowMonsters`. В этом списке есть
башни, невидимые маркеры и вспомогательные существа: ближайшая запись сама по
себе не является подходящим монстром. Сверяйте класс, типизированные флаги,
здоровье, свежую позицию и изображение. Сохраняйте GUID цели на всём интервале.
Порождение Заска находится именно в `m_ShowMonsters` с классом `ShowSummons`,
а не обязательно в `m_ShowPlayerSummons`. Для проверки его перемещения нужны
эта опция и свежий GUID, а не только позиция героя.
`local_observables` содержит сырые типизированные поля локального героя.
Название поля не доказывает его смысл: некоторые значения сбрасываются или
убывают, а счётчики попаданий могут не учитывать тренировочные манекены.
Не выдавайте их за накопительные счётчики без отдельной проверки.

В режиме наблюдения первая строка — снимок метаданных, затем строки `actors`.
Строка `READY` в stdout относится только к подключению наблюдателя. Полное
наблюдение заканчивается JSONL-строкой `summary` с `complete: true`. При анализе
сохраняйте отказы и неполные записи, не подменяйте их успешным повтором.

После страха/отталкивания используйте свежую позицию и свежие матрицы камеры. У тренировочного
манекена GUID может сохраниться после смерти и возрождения: отдельно сверяйте
здоровье, флаги смерти/удаления, текущую позицию и время снимка. В проверке Грока
манекен погиб от взрыва и затем появился с тем же GUID на исходной позиции.
Старые координаты нельзя переносить на новую попытку.
Автоматическая пассивная активация может вообще не менять счётчик ручных
применений. У Аргуса автоматическая ульта подтверждена состоянием «1 ОЗ,
жив», появлением перезарядки и отсутствием ввода E; ThirdSkillUseCount при этом
не вырос. Анализируйте результат и состояние отдельно от пользовательского ввода.
Названия и ID боевого навыка берите из текущего персонажа/интерфейса, а не
подставляйте ID, запомненный на предыдущем герое.

Окно описаний не всегда переключается вместе с реальной формой героя.
У Дигги в яйце оно продолжало показывать обычные навыки, хотя текущие
кнопки уже имели ID 4880/4881/4882. Сохраняйте это расхождение и проверяйте
новые кнопки отдельно; повторившийся текст не подтверждает их механику.
В короткой форме подготовьте наблюдатели заранее и проверяйте форму перед
каждым нажатием: тренировочное возрождение может закончить её во время
открытия меню или телепортации.

Для усиления следующей базовой атаки проверяйте всю цепочку. У Хилоса
применение Q меняет атаку 4900 → 4901, попадание усиленной атакой возвращает
4900 и отбрасывает цель. Его W отдельно меняется 4920 → 4921: отмена
повторного нажатия сохраняет круг, применение повторного нажатия выключает
его. Одного начального применения Q/W для проверки этих кнопок недостаточно.

Счётчик применения подтверждает применение, но сам по себе не доказывает
попадание. У заряжаемого навыка счётчик может увеличиваться уже при начале
подготовки, даже при последующей успешной отмене. Например, у Гатоткачи W
переходит 4120 → 4121 во время удержания. Проверяйте отсутствие рывка/эффекта
после отмены и результат после отпускания, а не только итоговый счётчик.
Для повторной кнопки подтвердите текущий вариант, его доступность,
удержание и результат. Сопоставляйте GUID призванного существа до/после действия.
Проверяйте также смену варианта во время одного удержания. У Хелкарта W
может пройти 5120 → 5121 → 5122 → 5123 → 5124 при накоплении клинков:
контакт и направление должны сохраниться, а отмена — сохранить накопленные
клинки. Для изолированной проверки отмены сначала подтвердите завершение
предыдущего применения по счётчику, КД или смене формы. Освободившийся
виртуальный палец сам по себе не доказывает, что игра уже применила навык;
иначе эффект предыдущего применения можно ошибочно приписать новой отмене.
`cast_locked: false` не означает отсутствия КД: отдельно проверяйте
`VirtualButton.m_CdCoolDown`. `ready: true` также может сопровождаться КД.

Все игровые действия выполняются одним координатором последовательно, через
приватный сокет собственной игры. Физический курсор и фокус пользователя не
перемещаются. Не запускайте одновременно два инструмента ввода. Менять код
Rust-маппера можно через `tools/reload-mapper.py`; требования и ограничения
описаны в `LIVE-RELOAD.md`. Диагностические бинарники не требуют замены игры.

В тесте с отслеживанием мировой точки не отправляйте после UP сохранённую
позицию курсора из начала удержания. При панорамировании это другой ввод,
который может изменить цель до завершения отпускания. Сохраняйте последние
когерентные координаты цели игры перед доставкой UP и сравнивайте результат
с ними. Самостоятельная проекция точки на Y=0 не заменяет цель индикатора:
его визуальная плоскость может иметь другую высоту. Эти требования относятся
к измерению; дополнительные задержки в игровое управление из них не следуют.

## English

Read the installed game's expanded skill/passive descriptions first. Derive
and reproduce target, hit, stack, form and recast conditions before calling a
hero ready for user review. Successful scripts or inferred templates alone are
insufficient. User confirmation remains separate.

`read-training-descriptions.py` reads localized UI labels with screenshots and
checks panel closure; it supports the observed three- and four-skill panels.
`training-actors-audit` provides bounded read-only snapshots/JSONL watches of
owned live players and summons, with identity, type and coherence checks.
Use fresh positions after displacement. Application counters alone do not prove
hits. Verify active variants, actual cooldowns, held input, cancellation and
observable effects. Serialize gameplay input; never move the physical cursor.

Description panels can retain base-form text while the actual buttons have
changed (Diggie's egg). Record that limitation rather than treating repeated
text as an alternate-form description. Check the current form before each
input in short-lived states. For next-attack buffs and toggles, verify the
entire arm/hit/reset or enable/cancel-disable/disable sequence; the initial
cast alone does not validate the conditional controls.
Zhask's summons are observed in `m_ShowMonsters` as `ShowSummons`; include that
container and track fresh GUIDs. A world-tracking test must not resend an old
cursor coordinate after UP. Compare the actual outcome with the last coherent
native target before release, respecting the indicator's visual plane.
Exercise form changes within one held input, such as Helcurt's accumulating
knives. Touch/direction must survive and cancellation must retain the resource.
For an isolated cancellation measurement, establish the previous cast's actual
application before attributing a later effect to the next input.

## Отказы диагностического аудита

`hero-contract-audit` сохраняет различия кэшированных проверок перед конечным
снимком. Перестройка списка UI может сменить его версию без смены героя/матча.
Повтор разрешён только при полном и неизменном наборе match/plan-проверок,
читаемом изменении UI, совпадающих владельцах до/после и **том же input epoch**.
Обычный отказ `Match source ownership changed` без этого подтверждения остаётся
окончательным. Исходные неуспешные отчёты сохраняются. Политика production
`Source::sample` не ослаблена; `audit_plan_integrity()` только объясняет различия.

The optional `--monsters` list also includes structures and invisible markers;
validate actual entity type and rendered scene before choosing a target. Local
raw observables are not certified cumulative counters. The contract auditor may
retry a positively identified UI-only cached guard change, but must retain the
same typed match owners and input epoch; production guards remain unchanged.
