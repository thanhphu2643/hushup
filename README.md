<p align="center"><img src="docs/icon.png" width="128" alt="HushUp icon"></p>

# HushUp

Stop OS updates and silence the "update available" nags: the red badge on
System Settings in the Dock and the **Software Update Available** row in its
sidebar. Your machine, your schedule.

- `hushup`: Rust CLI + background guard (does all the work)
- `HushUp.app`: Swift menu bar app on top of the CLI (macOS)
- `HushUp.exe` / `HushUp` (.deb): Tauri 2 app on top of the same core (Windows, Linux)

Platforms: **macOS**, **Windows**, **Linux** (Ubuntu, Linux Mint and other apt-based desktops).

## How it works (macOS)

Every change is recorded and undone by `hushup restore`.

| Layer | What it does |
|---|---|
| Preferences | `AutomaticCheckEnabled`, `AutomaticDownload`, `AutomaticallyInstallMacOSUpdates` → `false` in `/Library/Preferences/com.apple.SoftwareUpdate` |
| Badges | Clears `RecommendedUpdates` (what System Settings counts) plus per-user `AvailableUpdatesNotificationCountKey` / `AttentionPrefBundleIDs`, then restarts the Dock |
| Hosts | Points `swscan`, `swdist`, `swcdn`, `updates.cdn-apple.com`, `gdmf.apple.com` at `0.0.0.0` / `::` so a scan can't refill the list |
| Guard | launchd daemon re-runs `hushup enforce` every 15 min and whenever softwareupdated rewrites its prefs |

Security data (XProtect, `ConfigDataInstall`/`CriticalUpdateInstall`) is left
on by default. Pass `--block-security-data` only if you really mean it.

To update manually later: `sudo hushup restore`, update, then `apply` again.

## How it works (Windows)

Windows Update group policies in `HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate`,
backed up first and put back exactly by `restore`:

| Policy | Effect |
|---|---|
| `AU\NoAutoUpdate=1`, `AU\AUOptions=2` | No automatic download/install; notify-only if re-enabled |
| `TargetReleaseVersion` + `ProductVersion` + `TargetReleaseVersionInfo` | Pinned to the installed feature release (e.g. Windows 11 24H2) |
| `SetUpdateNotificationLevel=1`, `UpdateNotificationLevel=2` | Update notifications and restart warnings hidden |
| `AU\NoAutoRebootWithLoggedOnUsers=1` | No forced restart while you're signed in |

A `HushUp` scheduled task (SYSTEM, hourly) re-applies them. No hosts-file block on
Windows: Defender treats that as `HostsFileHijack`. Windows Home may ignore some policies.

## How it works (Linux)

Only the distro's own switches, each undone by `restore`:

| Piece | Ubuntu | Mint | How |
|---|---|---|---|
| apt auto-update / unattended-upgrades | ✓ | ✓ | `APT::Periodic::* "0"` in `/etc/apt/apt.conf.d/99hushup` |
| Snap auto-refresh | ✓ | | `snap refresh --hold` |
| New-release prompts | ✓ | | `Prompt=never` in `/etc/update-manager/release-upgrades` |
| Software Updater pop-ups / Update Manager tray & reminders | ✓ | ✓ | per-user autostart override with `Hidden=true` |
| GNOME Software auto-download | ✓ | | `gsettings set org.gnome.software download-updates false` |
| Ubuntu Pro ads in apt | ✓ | | `pro config set apt_news=false` |
| Mint automatic upgrades (if enabled) | | ✓ | its systemd timers disabled |

A `hushup.timer` systemd unit re-applies the system-wide parts hourly.

## Install

Download from [Releases](https://github.com/thanhphu2643/hushup/releases/latest).

- **macOS**: open the DMG, drag HushUp to Applications, then click the bell in the
  menu bar → **Block Updates**. The app isn't notarized yet: allow it once in
  System Settings → Privacy & Security (or `xattr -dr com.apple.quarantine /Applications/HushUp.app`).
- **Windows**: run `HushUp-…-windows-x64-setup.exe` (SmartScreen: More info → Run anyway),
  open HushUp → **Block Updates**, accept the administrator prompt.
- **Linux**: `sudo apt install ./hushup_…_amd64.deb`, open HushUp from the app menu →
  **Block Updates**, enter your password.

## Build from source

```sh
cargo build --release
./target/release/hushup status
sudo ./target/release/hushup apply --dry-run   # preview every change
sudo ./target/release/hushup apply             # --no-hosts to skip /etc/hosts
sudo ./target/release/hushup install-daemon    # copies itself to /usr/local/bin
sudo hushup restore --daemon                   # undo everything, guard included
```

On Windows, run the same commands from an administrator terminal (no `sudo`).

Menu bar app (macOS):

```sh
./scripts/bundle-app.sh    # → dist/HushUp.app (ad-hoc signed)
./scripts/make-dmg.sh      # → dist/HushUp-<version>.dmg
open dist/HushUp.app
```

Desktop app (Windows, Linux; Tauri 2):

```sh
cd desktop
npx @tauri-apps/cli@2 dev      # run
npx @tauri-apps/cli@2 build    # Windows → target/release/bundle/nsis/*-setup.exe
npx @tauri-apps/cli@2 build --bundles deb   # Linux → target/release/bundle/deb/*.deb
```

## Layout

```
src/cli.rs                  CLI (shared by `hushup` and the Windows app)
src/platform/mod.rs         shared types and helpers, picks the OS module
src/platform/macos.rs       macOS implementation
src/platform/windows.rs     Windows implementation
src/platform/linux.rs       Linux (apt-based) implementation
src/platform/unsupported.rs placeholder for other OSes
app/                        Swift menu bar app (macOS)
desktop/                    Tauri 2 app (Windows, Linux); its exe doubles as the CLI
scripts/bundle-app.sh       builds HushUp.app
scripts/make-dmg.sh         packs it into a DMG
.github/workflows/release.yml  tag v* → DMG + Windows installer + .deb on GitHub Releases
```

Adding an OS = a new `src/platform/<os>.rs` with the same functions.

## Roadmap

- Windows: tray icon, code signing, list pending updates
- Linux: Fedora (dnf-automatic, PackageKit), KDE Discover, Flatpak auto-updates
- macOS: SMAppService privileged helper instead of the AppleScript password prompt; notarized release

## Caveats

- Updates macOS found **before** you ran `apply` stay in softwareupdated's
  cache (`/Library/Updates`, protected by SIP) and keep the badge until they're
  installed. `hushup status` lists them. Install them once, e.g. for a
  Command Line Tools update:
  `sudo hushup restore && softwareupdate -i "<label>" && sudo hushup apply`
  (`softwareupdate --list --no-scan` shows the label). New ones won't show up.

- Skipping updates means skipping security fixes. Re-enable regularly.
- Apple changes where the badge comes from between macOS versions; open an
  issue with `hushup status` output if a badge survives.
- MDM-managed Macs and Intune/domain-managed PCs may enforce updates regardless.

## License

MIT
