# Icons

Add your app icons here. Tauri requires:

- `32x32.png`
- `128x128.png`
- `128x128@2x.png`
- `icon.icns` (macOS bundle icon)
- `icon.ico` (Windows bundle icon)
- `icon.png` (tray icon, referenced by tauri.conf.json)

Generate them with [`@tauri-apps/cli`](https://tauri.app/distribute/):

```
npx @tauri-apps/cli icon path/to/source.png
```

The shell will not build without these files. They were intentionally not included because they're brand-specific.
