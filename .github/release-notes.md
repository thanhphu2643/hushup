Stop OS updates and silence the "update available" nags.

## macOS (13 or later, Apple silicon + Intel)

1. Download **HushUp-x.y.z.dmg**, open it and drag **HushUp** to **Applications**.
2. HushUp isn't notarized yet, so macOS blocks the first launch. Either open it once,
   then go to **System Settings → Privacy & Security → Open Anyway**, or run
   `xattr -dr com.apple.quarantine /Applications/HushUp.app`.
3. Click the bell in the menu bar → **Block Updates** (asks for your password).

Updates macOS had already found before you block stay in its SIP-protected cache
and keep the badge until installed; see Caveats in the README.

## Windows (10 / 11, x64)

1. Download **HushUp-x.y.z-windows-x64-setup.exe** and run it. It isn't code-signed
   yet, so SmartScreen may warn: **More info → Run anyway**.
2. Open HushUp → **Block Updates** and accept the administrator prompt.

This turns off automatic updates, pins Windows to the installed feature release and
hides update notifications, through Windows Update policies (restored exactly by
**Restore Updates**). Windows Home may ignore some of these policies.

`hushup-x.y.z-windows-x64.exe` is the command-line tool alone (`hushup status`,
`hushup apply --daemon` from an administrator terminal).

## Linux (Ubuntu 22.04+, Linux Mint 21+, x64)

1. Download **hushup_x.y.z_amd64.deb** and install it:
   `sudo apt install ./hushup_x.y.z_amd64.deb` (or double-click it).
2. Open HushUp from the app menu → **Block Updates** and enter your password.

Uses only official switches: apt periodic updates and unattended-upgrades off, snap
refreshes held, new-release prompts off, Software Updater / Update Manager
autostart hidden, GNOME Software auto-downloads off. Notifiers that are already
running stop right away; everything is back after **Restore Updates** and a re-login.

`hushup-x.y.z-linux-x64` is the command-line tool alone (`sudo ./hushup-… apply --daemon`).
