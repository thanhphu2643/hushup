<p align="center"><img src="docs/icon.png" width="128" alt="HushUp icon"></p>

# HushUp

Stop OS updates and silence the "update available" nags: the red badge on
System Settings in the Dock and the **Software Update Available** row in its
sidebar. Your machine, your schedule.

- `hushup`: Rust CLI + background guard (does all the work)
- `HushUp.app`: Swift menu bar app on top of the CLI (macOS)
- `HushUp.exe` / `HushUp` (.deb): Tauri 2 app on top of the same core (Windows, Linux)
- `ios/`: SwiftUI app + on-device DNS tunnel (iPhone, iPad)

Platforms: **macOS**, **Windows**, **Linux** (Ubuntu, Linux Mint and other apt-based desktops), **iOS / iPadOS**.

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

## How it works (iOS)

iOS has no switch an app can flip for system updates, so HushUp hides Apple's
update servers instead. It installs a VPN profile whose tunnel never leaves the
device: split DNS sends only lookups of `mesu.apple.com`, `gdmf.apple.com`,
`appldnld.apple.com` and `updates(-http).cdn-apple.com` to a resolver inside the
tunnel, which answers "no such domain". All other lookups and traffic use the
normal network. The profile connects on demand, so it survives restarts.
**Restore Updates** removes the profile.

Do once by hand: Settings › General › Software Update › Automatic Updates → off,
and delete an already-downloaded update in iPhone Storage (that clears the badge).

### iOS without the app

Same block through a filtering DNS service, free, no developer account. Example
with [NextDNS](https://nextdns.io) (free tier: 300k lookups/month):

1. Sign up at [my.nextdns.io](https://my.nextdns.io). In **Denylist** add
   `mesu.apple.com`, `gdmf.apple.com`, `appldnld.apple.com`, `updates.cdn-apple.com`,
   `updates-http.cdn-apple.com` (subdomains are included).
2. On the iPhone open [apple.nextdns.io](https://apple.nextdns.io), enter your
   configuration ID (shown in **Setup**), download the profile.
3. Settings › Profile Downloaded › Install. Then Settings › General › VPN & Device
   Management › DNS: make sure NextDNS is selected.
4. Do the two manual steps above (Automatic Updates off, delete a downloaded update).

Check: Settings › General › Software Update should say it can't check for updates.
Every DNS lookup of the phone then goes through NextDNS. Undo: delete the profile
in VPN & Device Management.

## Install

Download from [Releases](https://github.com/thanhphu2643/hushup/releases/latest).

- **macOS**: open the DMG, drag HushUp to Applications, then click the bell in the
  menu bar → **Block Updates**. The app isn't notarized yet: allow it once in
  System Settings → Privacy & Security (or `xattr -dr com.apple.quarantine /Applications/HushUp.app`).
- **Windows**: run `HushUp-…-windows-x64-setup.exe` (SmartScreen: More info → Run anyway),
  open HushUp → **Block Updates**, accept the administrator prompt.
- **Linux**: `sudo apt install ./hushup_…_amd64.deb`, open HushUp from the app menu →
  **Block Updates**, enter your password.
- **iOS**: no App Store build yet. Build it yourself (below); Network Extensions
  need a paid Apple Developer account, a free Apple ID can't sign them.
  No paid account? See [iOS without the app](#ios-without-the-app).

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

iOS app (needs [XcodeGen](https://github.com/yonaskolb/XcodeGen)):

```sh
cd ios
xcodegen                   # → HushUp.xcodeproj
open HushUp.xcodeproj      # set your team in Signing & Capabilities, run on a device
```

Schemes: **HushUp** (with the DNS tunnel, paid account) and **HushUpFree** (no tunnel,
signs with a free Apple ID; walks you through the NextDNS profile below and checks it works).

The tunnel doesn't run in the Simulator; the DNS logic has unit tests that do
(`xcodebuild test -scheme HushUp -destination 'platform=iOS Simulator,name=iPhone 17 Pro'`).

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
ios/                        iOS app (HushUp) + DNS tunnel extension (HushUpTunnel)
scripts/bundle-app.sh       builds HushUp.app
scripts/make-dmg.sh         packs it into a DMG
.github/workflows/release.yml  tag v* → DMG + Windows installer + .deb on GitHub Releases
```

Adding an OS = a new `src/platform/<os>.rs` with the same functions.

## Roadmap

- Windows: tray icon, code signing, list pending updates
- Linux: Fedora (dnf-automatic, PackageKit), KDE Discover, Flatpak auto-updates
- iOS: TestFlight / App Store build
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
