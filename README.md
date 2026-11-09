# Glint

A Minecraft: Java Edition client: a desktop launcher and a Fabric client mod.

| Folder | What it is |
|---|---|
| `launcher/` | Desktop launcher. Tauri 2 with a Rust backend and a React + TypeScript UI. |
| `launcher/crates/glint-core/` | All launcher logic (sign-in, metadata, install, Java, instances, launch). No UI dependency. |
| `launcher/src-tauri/` | The Tauri shell: thin commands over `glint-core`. |
| `client-mod/` | The Fabric mod, `glintclient`. Not started yet. |
| `branding/` | Logo and `tokens.json`, the colours shared by the launcher and the mod. |

## Status

Built so far:

- **Launcher core:** Microsoft sign-in, instances with presets, automatic Java, installing
  and launching Minecraft with Fabric, and a live game log.
- **Mod browser:** search Modrinth and CurseForge together, install with dependencies,
  enable and disable per instance, and update checks. Presets install their mods when an
  instance is created.

The client mod and onboarding/skins come next.

## Requirements

- Windows 10 or 11 (the code is written to be portable; only Windows is tested)
- [Rust](https://rustup.rs) (stable, MSVC toolchain) and the Visual Studio C++ build tools
- Node.js 22 or newer

## Running it

```sh
cd launcher
npm install
npm run tauri dev      # run the launcher
npm run tauri build    # build the installer
cargo test -p glint-core
cargo test -p glint-core --test live_mods -- --ignored   # checks against the real Modrinth API
```

`glint-core` also has a headless end-to-end check that installs and starts the game
without the UI:

```sh
cargo run -p glint-core --example smoke -- <some-empty-folder>
```

## Signing in

Minecraft sign-in needs an Azure app registration, and Mojang has to approve it.

1. In the [Azure portal](https://portal.azure.com), go to **App registrations → New
   registration**. Choose **Personal Microsoft accounts only** and leave the redirect URI
   empty.
2. Under **Authentication**, set **Allow public client flows** to **Yes**.
3. Copy the **Application (client) ID** into the launcher's Settings page, or set
   `GLINT_AZURE_CLIENT_ID`.
4. Submit the client ID to Mojang at <https://aka.ms/mce-reviewappid>. Until it's approved,
   sign-in fails at the final step with a message saying so.

No client secret is involved. Refresh tokens are stored in the operating system's
credential store, not in a file.

Development builds have a **Play offline** button on each instance, for testing the launch
pipeline without an approved app. It is compiled out of release builds.

## Configuration

Settings live in `%APPDATA%\dev.glintclient.launcher\config.json`, edited through the
Settings page. Two values can also come from the environment, which takes precedence:

| Variable | Purpose |
|---|---|
| `GLINT_AZURE_CLIENT_ID` | Azure application (client) ID |
| `GLINT_CURSEFORGE_API_KEY` | CurseForge API key for the mod browser. Without one, the browser shows Modrinth only. |

Setting either variable at build time bakes it into the binary as the default, which is
how a public build would ship them. Nothing is hardcoded in the source.

## Theme

`branding/tokens.json` is the single source for colours and radii. To change the accent,
edit `color.accent` there; the launcher regenerates its CSS variables on the next
`dev` or `build`.
