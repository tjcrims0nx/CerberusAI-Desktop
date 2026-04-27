# Icons

This directory must contain Tauri's required icon set before `tauri build` will succeed.

Generate them once from the SVG in `../../public/cerberus.svg`:

```powershell
npm install
npx tauri icon ..\..\public\cerberus.svg
```

This produces `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico`, and the Microsoft Store `Square*` set. Commit the generated files.
