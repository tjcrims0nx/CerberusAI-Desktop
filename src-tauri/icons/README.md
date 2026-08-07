# Icons

This directory must contain Tauri's required icon set before `tauri build` will succeed.

Generate them once from `helix-icon-master.png`:

```powershell
npm install
npx tauri icon helix-icon-master.png
```

This produces `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico`, and the Microsoft Store `Square*` set. Commit the generated files.
