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
