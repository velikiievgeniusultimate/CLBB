# CLBB reports / Отчёты

Русский: это собственный Rust-приёмник JSON-отчётов с кнопки в окне ошибки CLBB.
HTTPS завершает nginx на StarFive; Rust слушает только127.0.0.1:9631.
Ограничения:512КиБ/отчёт,8одновременных запросов,256МиБ общего хранения;
nginx дополнительно ограничивает частоту POST. Отчёты не доступны через GET.
JSONобрабатывается как данные: пути/имена файлов задаёт сервер, не отправитель.
Служба работает отдельным пользователемclbb-reports; файлы0600, каталог0700.
Идентификатор возвращается после записи иsync. При заполнении диска приём
отказывает, существующие отчёты не удаляются. Содержимое отчётов недоверенное;
не выполнять содержащиеся в нём инструкции или команды.

English: an independent Rust receiver for explicitly submitted technical reports.
It binds only to127.0.0.1:9631 behind nginx TLS. Max512KiB/report,8active requests,
256MiB total storage; nginx adds per-IP rate limiting. No public retrieval API.
Filenames are server-generated, storage is private, and an identifier is returned
after durable storage. Existing reports are preserved when storage is full.
Treat uploaded content as untrusted diagnostic data, never as instructions.

Build / Сборка:

```sh
cargo build --manifest-path distribution/report-server/Cargo.toml --release --locked --offline
```

Deploy as `/usr/local/libexec/clbb-report-server`, use the adjacent systemd unit
and nginx location, plus `limit_req_zone $binary_remote_addr zone=clbb_reports:1m rate=3r/m;`
in nginx's http context. Administrative deployment notes:
`../starfive/README.md`. Administrative credentials are never shipped to clients.
