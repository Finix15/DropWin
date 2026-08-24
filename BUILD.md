# Hướng dẫn build DropWin

Tài liệu này hướng dẫn build ứng dụng desktop trong thư mục `app/`. Dự án sử dụng Tauri 2, React, TypeScript và Rust.

## 1. Yêu cầu môi trường

Yêu cầu chung:

- Node.js 20 trở lên.
- pnpm 9.
- Rust stable và Cargo.
- Git và kết nối mạng trong lần cài dependency đầu tiên. Thư viện thumbnail được vendor trực tiếp trong repository.

### Windows

Cài thêm:

- Visual Studio Build Tools 2022 với workload **Desktop development with C++**.
- Windows 10/11 SDK.
- Microsoft Edge WebView2 Runtime. Windows 10 và Windows 11 thường đã có sẵn.

Thêm target Rust nếu chưa có:

```powershell
rustup target add x86_64-pc-windows-msvc
```

### macOS

Cài Xcode Command Line Tools:

```bash
xcode-select --install
```

Thêm target phù hợp với máy:

```bash
# Apple Silicon
rustup target add aarch64-apple-darwin

# Intel
rustup target add x86_64-apple-darwin
```

> Tauri cần toolchain native của hệ điều hành. Hãy build bản Windows trên Windows và bản macOS trên macOS.

## 2. Cài dependency

Từ thư mục gốc của repository:

```bash
cd app
corepack enable
corepack prepare pnpm@9 --activate
pnpm install --frozen-lockfile
```

Nếu máy không có Corepack, có thể cài pnpm bằng cách khác rồi kiểm tra phiên bản:

```bash
pnpm --version
```

## 3. Build

### Chỉ build frontend

Lệnh này chạy TypeScript checker và tạo bản Vite production trong `app/dist/`:

```bash
cd app
pnpm build
```

### Build ứng dụng native trên hệ điều hành hiện tại

```bash
cd app
pnpm build:tauri
```

Tauri sẽ tự chạy script frontend `build` thông qua cấu hình `beforeBuildCommand`, sau đó biên dịch backend Rust và đóng gói installer.

### Build Windows x64

Chạy trên Windows:

```powershell
cd app
pnpm build:windows
```

### Build macOS

Chạy trên macOS và chọn đúng kiến trúc:

```bash
cd app

# Apple Silicon
pnpm build:mac:apple-silicon

# Intel
pnpm build:mac:intel
```

## 4. File đầu ra

Khi build không truyền `--target`, kết quả nằm tại:

```text
app/src-tauri/target/release/
app/src-tauri/target/release/bundle/
```

Khi dùng các script có target cụ thể, kết quả thường nằm tại:

```text
app/src-tauri/target/<target>/release/
app/src-tauri/target/<target>/release/bundle/
```

Ví dụ target Windows là `x86_64-pc-windows-msvc`, Apple Silicon là `aarch64-apple-darwin` và macOS Intel là `x86_64-apple-darwin`. Thư mục `bundle/` chứa installer tương ứng như NSIS/MSI trên Windows hoặc DMG/app trên macOS.

## 5. Kiểm tra trước khi phát hành

Đảm bảo số phiên bản giống nhau trong ba file:

- `app/package.json`
- `app/src-tauri/tauri.conf.json`
- `app/src-tauri/Cargo.toml`

Sau đó chạy:

```bash
cd app
pnpm install --frozen-lockfile
pnpm build
pnpm build:tauri
```

Nên cài thử installer sinh ra và kiểm tra các chức năng chính: mở ứng dụng, tray icon, global shortcut, kéo/thả file, thumbnail và auto-start.

## 6. Phát hành bằng GitHub Actions

Workflow `.github/workflows/tauri.yml` tự build Windows x64 và macOS Apple Silicon khi push tag bắt đầu bằng `v`:

```bash
git tag v3.0.1
git push origin v3.0.1
```

Trước khi tạo tag, hãy cập nhật đồng bộ phiên bản trong ba file ở mục 5 và commit thay đổi. Workflow sẽ tạo GitHub Release và tải các artifact đã build lên release đó.

## 7. Lỗi thường gặp

### Không tìm thấy linker `link.exe` trên Windows

Cài hoặc sửa Visual Studio Build Tools, bảo đảm workload **Desktop development with C++** và Windows SDK đã được chọn. Sau đó mở terminal mới và build lại.

### Lỗi WebView2

Cài Microsoft Edge WebView2 Runtime rồi chạy lại installer hoặc lệnh build.

### `pnpm install --frozen-lockfile` báo lockfile không khớp

Nếu dependency vừa được thay đổi có chủ đích, chạy `pnpm install` để cập nhật `pnpm-lock.yaml`, kiểm tra diff rồi commit cả lockfile. Không bỏ qua lỗi này trong bản phát hành.

### Rust target chưa được cài

Kiểm tra các target hiện có:

```bash
rustup target list --installed
```

Sau đó thêm target bằng lệnh tương ứng ở mục 1.

### Build lần đầu mất nhiều thời gian

Đây là bình thường vì Cargo phải tải và biên dịch toàn bộ dependency Rust. Các lần build sau sẽ tận dụng cache trong `app/src-tauri/target/`.
