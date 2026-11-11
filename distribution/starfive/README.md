# StarFive APK downloads / Загрузка APK со StarFive

## Русский

Отдельный статический nginx обслуживает только HTTPS `/health` и точный путь APK,
указанный в `../game-apk.json`. APK не коммитится и не включается в GitHub-релизы.
Это неизменённая проприетарная игра; лицензия исходников CLBB на неё не распространяется.

- Хост: `clbb-downloads.velikiievgenius.com`, DNS A указывает на внешний адрес платы.
- На роутере отдельные DNAT-правила `CLBB-Downloads-80/443` ведут на StarFive.
- HTTP 80 используется для ACME и перенаправления на HTTPS; TLS 443 — для APK.
- nginx-конфигурация: `/etc/nginx/sites-available/clbb-downloads`; пример рядом.
- APK: `/srv/clbb/apk/mlbb-2.2.16.12321.apk`; webroot ACME: `/srv/clbb/acme`.
- Сертификат Let's Encrypt: `/etc/letsencrypt/live/clbb-downloads.velikiievgenius.com/`.
- Обновление сертификата: `certbot.timer`; deploy hook `clbb-nginx` проверяет конфигурацию и перезагружает nginx.
- Новое правило `clbb-downloads` в существующей input-цепочке nftables разрешает только TCP 80/443.
- Прежние правила SSH/ANNO и остальные правила роутера сохранены. Сервис не даёт SSH, оболочку или доступ к приватной папке владельца.
- nginx отдаёт файл через sendfile, поддерживает Range/HEAD, ограничивает число соединений с одного IP до четырёх. Access log выключен.

Новая версия APK: разместить файл с новым неизменяемым именем, проверить SHA256 и
Android package/version, добавить соответствующий точный location в nginx,
проверить `nginx -t` и HTTPS-загрузку извне, затем изменить `../game-apk.json`.
Старые файлы и URL не заменять другими байтами. Обновление манифеста не заменяет
APK уже установленной игры: обычный повторный запуск установщика сохраняет её.
При смене внешнего IP обновить DNS A; ACME требует доступный TCP 80.

Проверка с внешней машины:

```sh
curl -fI https://clbb-downloads.velikiievgenius.com/apk/mlbb-2.2.16.12321.apk
```

Пароли, DNS API token, SSH-ключи, router session и закрытый ключ TLS сюда не входят.
Закрытые копии конфигурации до настройки хранятся у владельца, отдельно от репозитория.

## English

An independent static nginx serves only HTTPS `/health` and the exact APK path in
`../game-apk.json`. The unmodified proprietary game is not committed, included in
GitHub releases or relicensed under CLBB's license.

The hostname points directly to StarFive's external IPv4. Dedicated router DNAT
rules forward TCP 80/443 to the board, preserving existing SSH/ANNO rules. The
existing nftables input chain has one additive `clbb-downloads` rule for these web
ports. Port 80 is reserved for ACME and HTTPS redirection; port 443 serves the APK.

Paths, service names and certificate locations are listed above. `certbot.timer`
renews the certificate; a deploy hook validates and reloads nginx. The service uses
sendfile, supports HEAD and Range, permits four simultaneous connections per IP
and disables access logs. It exposes no shell or private owner directory.

For a new APK, use a new immutable filename, verify its SHA256 and Android package/
version, add an exact nginx location, validate nginx and a complete external HTTPS
download, then update `../game-apk.json`. Do not replace bytes at existing URLs.
Existing game installations are preserved when the installer is re-run. If the
public IP changes, update DNS; certificate renewal requires reachable TCP 80.

No administrative credentials, DNS token, SSH keys, router sessions or private TLS
key are included. Owner configuration backups are private and outside this repo.
