# Отладочная вторая игра / Private debug instance

В локальной рабочей сборке обычный ярлык запускает выбранный профиль.
Переключение профилей и второе окно доступны в разделе «Аккаунты» настроек F7.
Для отдельного отладочного запуска из терминала используйте команды ниже.

In the local workbench, the desktop shortcut starts the selected profile.
The Accounts section in F7 supports switching profiles and opening a second
window. The commands below start a separate debug instance from the terminal.

```sh
python tools/debug-instance.py start secondary
python tools/debug-instance.py status secondary
python tools/debug-instance.py restart secondary
python tools/debug-instance.py hide secondary
python tools/debug-instance.py show secondary
python tools/debug-instance.py command secondary read hero.id
python tools/debug-instance.py stop secondary
```

Первый запуск создаёт чистый профиль. В дополнительный аккаунт нужно войти
вручную один раз. Остановка сохраняет его собственные данные входа.

The first launch creates a clean profile. Sign in to the additional account once;
stopping the instance preserves its private login data.

* Общие: проверенная сборка, APK, исходные игровые ресурсы.
* Отдельные: Android-данные, аккаунт, cookies, настройки, кэши, временные файлы,
  PID, управляющий сокет и отчёты.
* `fuse-overlayfs` направляет изменения ресурсов в личный слой второй копии.
  Исходные ресурсы и Android-данные основного аккаунта она не перезаписывает.
* `bubblewrap` скрывает основной каталог проекта, делает остальной хост доступным
  только для чтения, оставляет запись в отдельном профиле и предоставляет свой
  `/tmp`. Служебный ART/QEMU `code_cache` также отдельный.
* Внутренняя песочница WebKit отключена только у отладочной копии, уже работающей
  в общей внешней песочнице; иначе вложенный запуск WebKit здесь аварийно завершался.
* Для второй копии: 60 FPS, выключенный звук, пониженный приоритет CPU/IO,
  запуск без запроса фокуса. Показ/скрытие не двигает физический курсор.
* Ошибки второй копии записываются в файлы, без всплывающих окон Zenity.

Shared: verified runtime, APKs and base assets. Private: all writable Android,
authentication, configuration, cache, temporary, control and report state.
Resource writes use a private copy-on-write overlay. Only the debug instance
uses the outer filesystem sandbox with its redundant nested WebKit sandbox
turned off. CPU and GPU work and process memory remain separate for each game.

Профили / Profiles: `~/.local/share/clbb-debug-instances/<name>`.

Системные инструменты / System tools: `bubblewrap`, `fuse-overlayfs`,
`fusermount3`, `systemd-run`. Они вызываются как отдельные установленные утилиты;
их исходники и бинарники этим изменением не включаются в CLBB.

Для разработческих проб `Probe(..., state_root=profile)` выбирает именно вторую
копию. Захват её окна требует явного `--state-root profile --pid PID`.
Без этих параметров прежние инструменты продолжают выбирать основной запуск.

Developer probes require an explicit debug state root. Existing default tools
continue targeting the primary runtime; there is no fallback between accounts.
