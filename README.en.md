# CLBB — Computer Legends Bang Bang

[Русский](README.md) · **English**

An experimental Linux Mobile Legends client with a Rust input mapper.
Runs through ATL and QEMU without Waydroid. Independent of MOONTON.

## Install

The first testing branch supports **Arch Linux, Manjaro and EndeavourOS, x86_64**.
An X11 or XWayland desktop session and compatible distribution packages are required.
The installer checks each release's requirements, including its minimum glibc version.
Windows is not supported yet.

Run as your normal desktop user, **without prefixing the command with sudo**:

```sh
curl -fsSL https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/install.sh | sh
```

The package manager may request your password to install system dependencies.
CLBB itself is installed in your user directory and added to the application menu.
The installer selects the latest published testing or stable version, verifies
SHA256 downloads and preserves previous application releases.

**A clean installation automatically downloads the Mobile Legends APK from StarFive**
and verifies its SHA256 before importing it. Interrupted transfers are retried
with the partial download resumed. No manual APK selection or copying is needed.
Re-running the installer preserves existing APKs and account data.
The APK is not included in the Git repository or GitHub releases; it is a separate
proprietary game download and is not covered by the CLBB license.
The file is not modified or re-signed. Sign in to your account inside the game.

To install CLBB without downloading the game, set `CLBB_INSTALL_GAME=0`:
the first launch will offer a picker for your own APK.

Install with an existing APK:

```sh
curl -fsSL https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/install.sh | CLBB_APK_FILE="$HOME/mobile-legends.apk" sh
```

Automatic APK download from an HTTPS source you are authorized to use is supported
through `CLBB_APK_URL` and mandatory `CLBB_APK_SHA256`.
See [installation and distribution](distribution/README.md).

## Controls and updates

- **F7**: input, hero, audio, display and update settings.
- **F11**: windowed / fullscreen mode.
- **F12**: confine the cursor to the game area.
- Hotkeys can be changed in F7. Movement and skills have independent contacts.
- **F7 → Updates → Check for updates → Install update** (current UI labels are Russian).

Updates download in the background. The new version is activated on the next launch;
current gameplay, game data and settings are not replaced. Automatic installation is
disabled in a development checkout: update its source through Git and rebuild instead.

Installed command-line entry points:

```sh
~/.local/bin/clbb --version
~/.local/bin/clbb --check-updates
~/.local/bin/clbb --update
~/.local/bin/clbb --import-game /path/base.apk /path/split_config.apk
```

## Data and diagnostics

Default paths:

| Data | Path |
|---|---|
| CLBB releases | `~/.local/share/clbb/releases/` |
| Game, APKs, caches and logs | `~/.local/share/clbb/state/` |
| Settings | `~/.config/clbb/settings.json` |
| Debug clips | `~/.local/state/clbb/aim-debug/` |

XDG environment variables are supported. Set `CLBB_INSTALL_ROOT` to change the
installation root. CLBB updates preserve these directories.

F7 → Hotkeys offers an optional debug recorder. **Ctrl+S** saves five seconds before
and after the mark. Without a mark, only a short memory buffer is retained.
Recording is off by default; saved clips have a 256 MiB limit. Never publish passwords,
cookies, tokens, account data or full process-memory dumps.

The 30–240 FPS setting is a cap, not a performance guarantee.

## Source and builds

This repository contains project code and patches. Upstream source revisions are
pinned in `sources.json` and `.gitmodules`. A release's `clbb-<version>-sources.tar.gz`
contains the complete upstream trees with project patches applied. Rust dependencies
are included in `vendor/`.

```sh
git clone https://github.com/velikiievgeniusultimate/CLBB.git
cd CLBB
python3 distribution/bootstrap-sources.py
bash distribution/build-from-source.sh --install-deps
```

The initial native build is substantial. See [distribution/README.md](distribution/README.md)
for requirements. For an already prepared source tree:

```sh
bash build-runtime.sh --check
bash build-runtime.sh
cargo test --manifest-path mapper/runtime/Cargo.toml --lib --offline
cargo test --manifest-path distribution/manager/Cargo.toml --offline
python3 -m unittest discover -s distribution/tests
```

`mapper/` contains controls and settings, `launcher/` launches the runtime,
`distribution/` handles installation, updates and packaging, and `patches/` records
upstream changes. The archived ETA experiment is not part of this public project.

## Licensing

New original code is **GPL-3.0-or-later**, except where another license is explicitly
stated. Third-party components retain their licenses and attribution. See
[LICENSE_POLICY.md](LICENSE_POLICY.md), [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt)
and [legal/README.md](legal/README.md). Each binary release has corresponding sources.
Free distribution and future voluntary donations do not remove license obligations.

CLBB licensing grants no rights to Mobile Legends or MOONTON services. Publisher rules
must be considered separately for third-party clients and game-state reading; open-source
availability does not guarantee game approval or protection against account restrictions.

## Progress, failures and clean reinstallation

The installer shows stages and real download progress. Verification and extraction
show activity and elapsed time when their percentage is unknown. Installation logs
are stored in `~/.local/state/clbb/install/` (or the corresponding XDG_STATE_HOME).

A failed launch creates a technical JSON report and opens a dialog with a reason,
local file path and **Send report to server** button. Reports are sent only after
the button is pressed; StarFive returns a report identifier. Reports exclude
passwords, cookies, account files, memory dumps and gameplay recordings.
Local reports: `~/.local/share/clbb/state/logs/reports/`.

Run `~/.local/bin/clbb --diagnose` for a manual check. The independent diagnostic
command above also works with old installations or an unloadable Rust launcher;
it does not launch the game. Run the following independent diagnostic tool with old or unloadable installations:

```sh
curl -fL --progress-bar https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/diagnose.sh | sh
```

The reset command below removes CLBB, the APK, local account state, settings,
caches, reports and shortcut. Close CLBB and the installer first.
Distribution-managed system packages and source checkouts are preserved.

```sh
curl -fL --progress-bar https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/uninstall.sh | sh
```

### Clean installs and ARM64

Installation and startup explicitly prepare ARM64 libraries from base/split APKs,
independent of the host CPU architecture. A separate installation stage is shown;
subsequent launches use the APK-matched preparation cache. This fixes initial
alpha builds where a clean state could lack `libmain.so`/`libEncryptorP.so` despite
a successful APK download.
