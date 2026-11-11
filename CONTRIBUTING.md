# Участие / Contributing

## Русский

Документация репозитория поддерживается на русском и английском. Обновляйте
`README.md` и `README.en.md` вместе. Сохраняйте лицензии и уведомления upstream.
Новые оригинальные изменения принимаются на условиях `LICENSE_POLICY.md`;
изменения чужих файлов сохраняют применимые условия этих файлов.

В отчёте об ошибке укажите дистрибутив и его версию, CPU/GPU, версию CLBB,
героя, кнопку и последовательность действий. Для управления полезен фрагмент
записи с Ctrl+S. Сначала проверьте его содержимое; не прикладывайте пароли,
cookies, токены, APK, данные аккаунта или полный дамп памяти.

Перед PR запустите Rust/Python проверки из README. Игровые проверки выполняйте
последовательно: одна сессия и один аккаунт. Переписывание на Rust само по себе
не считается доказательством ускорения или точности; указывайте проверенное поведение.

## English

Maintain both Russian and English documentation. Update `README.md` and
`README.en.md` together. Preserve upstream licenses and notices. New original
contributions follow `LICENSE_POLICY.md`; modified upstream files retain their
applicable terms.

Bug reports should include distribution/version, CPU/GPU, CLBB version, hero,
binding and reproduction steps. A Ctrl+S debug clip can help with input issues.
Inspect it before sharing. Do not attach passwords, cookies, tokens, APKs,
account data or full memory dumps.

Run the README Rust/Python checks before a PR. Run game checks serially with
one session/account. A Rust rewrite alone does not demonstrate performance or
accuracy; report actual verified behavior.
