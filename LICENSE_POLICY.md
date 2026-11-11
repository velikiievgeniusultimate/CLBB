# Лицензирование / Licensing

## Русский

Новый оригинальный код CLBB распространяется по **GPL-3.0-or-later**,
кроме файлов, где явно указана другая лицензия. Copyright © 2026 CLBB contributors.
Полный текст GPLv3: `LICENSE`. Файлы с ранее указанной MIT и другими лицензиями
сохраняют эти условия; чужой код не объявляется нашей собственностью.

ATL, QEMU, ART, wolfSSL, AOSP, библиотеки и другие внешние компоненты сохраняют
собственные лицензии, авторство и уведомления. `sources.json` закрепляет исходники,
`.gitmodules` — внешние репозитории, `patches/` — наши изменения. Перенесённые из
QEMU вычислительные функции в `execution-core/` и `translator-rust/` сохраняют
происхождение и условия соответствующих исходных файлов QEMU (GPLv2 или новее);
этот документ не отменяет исходные разрешения.

Добровольные пожертвования и коммерческое использование допускаются при соблюдении
применимых лицензий. Получатели GPL-сборок сохраняют право получить соответствующие
исходники, изменять и распространять их. У релизов с бинарниками публикуется архив
соответствующих исходников, патчей и сборочных инструкций. Архивы и патчи нужно
сохранять для каждой распространяемой версии.

`legal/third-party-inventory.json` — широкий реестр исходников и зависимостей
рабочей машины, а не утверждение, что каждый перечисленный системный пакет входит
в установщик. Точный состав бинарного архива записывается в его `release.json`.
Системные GTK/WebKit, драйверы и шрифты устанавливаются пакетным менеджером дистрибутива;
в бинарный архив CLBB их копии не включаются.

APK Mobile Legends, ресурсы игры, аккаунты и записи диагностики **не входят**
в репозиторий или релизы. Пользователь предоставляет свою неизменённую копию игры.
Лицензия CLBB не предоставляет прав на игру, товарные знаки или сервис MOONTON.
CLBB — независимый экспериментальный проект; необходимо отдельно учитывать правила игры.

## English

New original CLBB code is licensed under **GPL-3.0-or-later**, except where a file
explicitly specifies another license. Copyright © 2026 CLBB contributors.
The full GPLv3 text is in `LICENSE`. Existing MIT and other file-level licenses
remain in effect. Upstream work is not claimed as project-authored code.

ATL, QEMU, ART, wolfSSL, AOSP and other upstream components retain their own
licenses, copyrights and notices. `sources.json` pins sources, `.gitmodules`
identifies upstream repositories, and `patches/` contains project changes.
QEMU-derived computation code in `execution-core/` and `translator-rust/` retains
its provenance and the applicable QEMU source terms (GPLv2 or later); this policy
does not remove original permissions.

Voluntary donations and commercial use are permitted subject to the applicable
licenses. GPL binary recipients retain their rights to corresponding source,
modification and redistribution. Each binary release is accompanied by an archive
of its corresponding sources, patches and build instructions. Preserve source
companions for every distributed version.

`legal/third-party-inventory.json` is a broad development-machine/source inventory,
not a claim that all listed system packages ship in the installer. The exact binary
payload is recorded in its `release.json`. System GTK/WebKit, drivers and fonts
are installed through the distribution package manager, not copied into CLBB archives.

Mobile Legends APKs, game assets, accounts and diagnostic recordings are excluded
from the repository and releases. Users supply their own unmodified game files.
CLBB licensing grants no rights to the game, trademarks or MOONTON service.
CLBB is an independent experimental project; game rules must be considered separately.
