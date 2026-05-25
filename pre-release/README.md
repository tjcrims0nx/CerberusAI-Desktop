# Cerberus Pre-Release

This folder tracks the beta channel source notes and installer entry points.

## One-Line Beta Install

```powershell
irm https://cerberusai.dev/get-beta | iex
```

GitHub prereleases are built by `.github/workflows/prerelease.yml`. Each run creates a prerelease tag, uploads the Windows installers, and keeps the beta installer flow pointed at the latest prerelease.

## Manual Build Trigger

Open GitHub Actions, run **Pre-Release Beta**, and optionally provide a tag like:

```text
v0.3.1-beta.1
```

If no tag is provided, the workflow creates one with the current UTC timestamp.
