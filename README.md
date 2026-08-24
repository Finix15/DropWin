# DropWin

DropWin is a lightweight desktop utility for temporarily holding files and folders while you work. It provides floating Drop windows that make moving or copying items between applications and locations faster.

Built with Tauri 2, React, TypeScript, and Rust.

## Features

- Create a temporary Drop by shaking the mouse or using a global shortcut.
- Drag files and folders into a Drop and drag them out when needed.
- Copy items by default or hold Shift while dropping to request a move.
- Manage multiple independent Drop windows.
- Generate native file thumbnails on Windows and macOS.
- Configure opacity, Drop size, shake sensitivity, language, process blacklist, and autostart.

## Download

Published builds are available from [GitHub Releases](https://github.com/Finix15/DropWin/releases).

## Build from source

Prerequisites:

- Node.js 20 or newer
- pnpm 9
- Rust stable
- Native Tauri build prerequisites for your operating system

```bash
git clone https://github.com/Finix15/DropWin.git
cd DropWin/app
pnpm install --frozen-lockfile
pnpm build:tauri
```

See [BUILD.md](BUILD.md) for platform-specific instructions.

## Development

```bash
cd app
pnpm install
pnpm build
```

The frontend lives in `app/src`; the Tauri backend lives in `app/src-tauri`.

## Releases

The workflow in `.github/workflows/tauri.yml` builds Windows x64 and macOS Apple Silicon artifacts when a tag beginning with `v` is pushed.

## License

DropWin is distributed under the [MIT License](LICENSE). The repository includes required notices for incorporated MIT-licensed components.
