#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Arch Linux / Manjaro / EndeavourOS, x86_64. Game APK is downloaded separately.
set -eu
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = x86_64 ] || { echo 'CLBB: Linux x86_64 required / требуется Linux x86_64' >&2; exit 1; }
command -v pacman >/dev/null || { echo 'CLBB: this installer currently supports Arch, Manjaro and EndeavourOS.' >&2; exit 1; }
[ "$(id -u)" != 0 ] || { echo 'CLBB: run as your normal desktop user, without sudo / запускайте без sudo.' >&2; exit 1; }
# A path on the developer's PC is not available on another tester's PC.
if [ -n "${CLBB_APK_FILE:-}" ] && [ ! -f "$CLBB_APK_FILE" ]; then
    printf '%s\n' "CLBB: APK file not found / APK не найден: $CLBB_APK_FILE" >&2
    printf '%s\n' 'Use an APK path on this computer, or omit CLBB_APK_FILE and select an APK on first launch / Укажите файл на этом ПК либо уберите CLBB_APK_FILE и выберите APK при первом запуске.' >&2
    exit 1
fi
for tool in python3 curl tar sha256sum; do
    if ! command -v "$tool" >/dev/null; then
        sudo pacman -S --needed -- python curl tar coreutils < /dev/tty
        break
    fi
done
CLBB_TMP=$(mktemp -d)
trap 'rm -rf -- "$CLBB_TMP"' EXIT HUP INT TERM
CLBB_REPO=velikiievgeniusultimate/CLBB
curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' \
    "https://api.github.com/repos/$CLBB_REPO/releases?per_page=20" -o "$CLBB_TMP/releases.json"
python3 - "$CLBB_TMP/releases.json" > "$CLBB_TMP/tag" <<'PY'
import json,re,sys
rows=json.load(open(sys.argv[1]));pattern=re.compile(r'^v(\d+)\.(\d+)\.(\d+)(?:-alpha\.(\d+))?$')
found=[]
for row in rows:
 m=pattern.fullmatch(row.get('tag_name',''))
 if m and not row.get('draft',True):found.append(((int(m[1]),int(m[2]),int(m[3]),m[4] is None,int(m[4] or 0)),row['tag_name']))
if not found:raise SystemExit('No CLBB release published / Нет опубликованной сборки CLBB')
print(max(found)[1])
PY
CLBB_TAG=$(cat "$CLBB_TMP/tag")
CLBB_URL="https://github.com/$CLBB_REPO/releases/download/$CLBB_TAG"
for file in clbb-manager-linux-x86_64 SHA256SUMS; do
    curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' \
        "$CLBB_URL/$file" -o "$CLBB_TMP/$file"
done
python3 - "$CLBB_TMP" <<'PY'
import hashlib,pathlib,sys
root=pathlib.Path(sys.argv[1]);name='clbb-manager-linux-x86_64'
entries=[line.split()[0] for line in (root/'SHA256SUMS').read_text().splitlines() if len(line.split())==2 and line.split()[1].lstrip('*')==name]
if len(entries)!=1 or hashlib.sha256((root/name).read_bytes()).hexdigest()!=entries[0]:raise SystemExit('CLBB: SHA256 mismatch')
PY
chmod 700 "$CLBB_TMP/clbb-manager-linux-x86_64"
"$CLBB_TMP/clbb-manager-linux-x86_64" --install
if [ -n "${CLBB_APK_FILE:-}" ]; then
    "$CLBB_TMP/clbb-manager-linux-x86_64" --import-game "$CLBB_APK_FILE"
elif [ -n "${CLBB_APK_URL:-}" ]; then
    [ -n "${CLBB_APK_SHA256:-}" ] || { echo 'CLBB_APK_SHA256 is required for an APK download' >&2; exit 1; }
    "$CLBB_TMP/clbb-manager-linux-x86_64" --import-game-url "$CLBB_APK_URL" "$CLBB_APK_SHA256"
elif [ "${CLBB_INSTALL_GAME:-1}" != 0 ]; then
    CLBB_GAME_STATE="${CLBB_STATE_ROOT:-${CLBB_INSTALL_ROOT:-${XDG_DATA_HOME:-$HOME/.local/share}/clbb}/state}"
    if [ -f "$CLBB_GAME_STATE/game/base.apk" ]; then
        printf '%s\n' 'Игра уже установлена; APK и аккаунт сохранены / Game already installed; preserving APK and account.'
    else
        curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' \
            --connect-timeout 15 --max-time 60 --retry 2 \
            "https://raw.githubusercontent.com/$CLBB_REPO/main/distribution/game-apk.json" -o "$CLBB_TMP/game-apk.json"
        python3 - "$CLBB_TMP/game-apk.json" > "$CLBB_TMP/game-source" <<'PYAPK'
import json,re,sys,urllib.parse
with open(sys.argv[1]) as stream: info=json.load(stream)
url=info.get('download_url');digest=info.get('sha256','')
if info.get('package')!='com.mobile.legends' or not isinstance(url,str):raise SystemExit('Invalid game download manifest')
parsed=urllib.parse.urlsplit(url)
if parsed.scheme!='https' or not parsed.hostname or parsed.username or parsed.password or any(c.isspace() for c in url):raise SystemExit('Invalid game download URL')
if not isinstance(digest,str) or not re.fullmatch('[0-9a-fA-F]{64}',digest):raise SystemExit('Invalid game SHA256')
print(url);print(digest.lower())
PYAPK
        {
            IFS= read -r CLBB_GAME_URL
            IFS= read -r CLBB_GAME_SHA256
        } < "$CLBB_TMP/game-source"
        printf '%s\n' 'Скачивается APK Mobile Legends со StarFive, затем проверяется SHA256 / Downloading the game APK from StarFive and verifying SHA256…'
        "$CLBB_TMP/clbb-manager-linux-x86_64" --import-game-url "$CLBB_GAME_URL" "$CLBB_GAME_SHA256"
    fi
fi
printf '%s\n' 'CLBB установлен / installed. Запустите CLBB из меню приложений / Launch CLBB from your application menu.'
