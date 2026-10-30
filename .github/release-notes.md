Stop macOS updates and silence the "Software Update Available" badge.

## Install

1. Download **HushUp-x.y.z.dmg** below, open it and drag **HushUp** to **Applications**.
2. HushUp isn't notarized yet, so macOS blocks the first launch. Either:
   - open it once, then go to **System Settings → Privacy & Security** and click **Open Anyway**, or
   - run `xattr -dr com.apple.quarantine /Applications/HushUp.app`
3. Click the bell in the menu bar → **Block Updates** (asks for your password).

Universal build (Apple silicon + Intel), macOS 13 or later.

Updates macOS had already found before you block stay in its SIP-protected cache
and keep the badge until installed; see Caveats in the README.
