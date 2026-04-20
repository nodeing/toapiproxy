# TOAPIPROXY Build Guide

这份文档对应当前仓库已经加好的两个聚合命令：

- `make build-win`
- `make build-mac`

执行完成后的最终交付物都在 `build/dist/` 目录下。

当前命名规则如下：

- `TOAPIPROXY_1.0.0_windows_x64_setup.exe`
- `TOAPIPROXY_1.0.0_windows_arm64_setup.exe`
- `TOAPIPROXY_1.0.0_macos_intel.dmg`
- `TOAPIPROXY_1.0.0_macos_apple_silicon.dmg`

说明：

- 本文默认你已经安装好了 `Go` 和 `Rust`
- 如果某台机器还没有 `cargo tauri`，文档里会补安装命令
- Windows 和 macOS 请分别在对应系统上执行

## Windows 机器

目标：

- 一次构建出 `Windows x64` 和 `Windows ARM64` 两个安装包

### 1. 仅首次执行：安装 Visual Studio C++ Build Tools，并补 ARM64 编译组件

请用 `管理员 PowerShell` 执行下面整段命令：

```powershell
$vsconfig = @'
{
  "version": "1.0",
  "components": [
    "Microsoft.Component.MSBuild",
    "Microsoft.VisualStudio.Component.CoreEditor",
    "Microsoft.VisualStudio.Component.NuGet",
    "Microsoft.VisualStudio.Component.Roslyn.Compiler",
    "Microsoft.VisualStudio.Component.TextTemplating",
    "Microsoft.VisualStudio.Component.VC.ASAN",
    "Microsoft.VisualStudio.Component.VC.ATL.ARM64.Spectre",
    "Microsoft.VisualStudio.Component.VC.ATL.ARM64",
    "Microsoft.VisualStudio.Component.VC.ATL.Spectre",
    "Microsoft.VisualStudio.Component.VC.ATL",
    "Microsoft.VisualStudio.Component.VC.ATLMFC.Spectre",
    "Microsoft.VisualStudio.Component.VC.ATLMFC",
    "Microsoft.VisualStudio.Component.VC.CoreIde",
    "Microsoft.VisualStudio.Component.VC.MFC.ARM64.Spectre",
    "Microsoft.VisualStudio.Component.VC.MFC.ARM64",
    "Microsoft.VisualStudio.Component.VC.Redist.14.Latest",
    "Microsoft.VisualStudio.Component.VC.Runtimes.ARM64.Spectre",
    "Microsoft.VisualStudio.Component.VC.Runtimes.ARM64EC.Spectre",
    "Microsoft.VisualStudio.Component.VC.Runtimes.x86.x64.Spectre",
    "Microsoft.VisualStudio.Component.VC.Tools.ARM64",
    "Microsoft.VisualStudio.Component.VC.Tools.ARM64EC",
    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
    "Microsoft.VisualStudio.Component.Windows10SDK",
    "Microsoft.VisualStudio.ComponentGroup.NativeDesktop.Core",
    "Microsoft.VisualStudio.Workload.CoreEditor",
    "Microsoft.VisualStudio.Workload.NativeDesktop"
  ]
}
'@

$vsconfigPath = "$env:TEMP\toapiproxy-windows-build.vsconfig"
Set-Content -Path $vsconfigPath -Value $vsconfig -Encoding UTF8

winget install --source winget --exact --id Microsoft.VisualStudio.2022.Community --override "--passive --config `"$vsconfigPath`""
```

执行完后，关闭当前终端，再开一个新的 PowerShell。

### 2. 补齐 Rust 目标架构

```powershell
rustup default stable-msvc
rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
```

### 3. 安装 LLVM Clang

这一步是为了 `Windows ARM64` 构建。

当前项目依赖树里的 `ring` 在 `aarch64-pc-windows-msvc` 目标上会调用 `clang`。  
如果没有这一步，`make build-win` 在 ARM64 那个包上会报错：

- `failed to find tool "clang"`

执行：

```powershell
winget install --source winget --exact --id LLVM.LLVM
```

执行完后，关闭当前终端，再开一个新的 PowerShell，然后检查：

```powershell
clang --version
```

### 4. 如果没装过 Tauri CLI，再执行一次安装

先检查：

```powershell
cargo tauri -V
```

如果提示命令不存在，再执行：

```powershell
cargo install tauri-cli --version "^2.0.0" --locked
```

### 5. 开始构建

```powershell
cd d:\workspace\app\toapiproxy
make build-win
```

### 6. 构建结果

成功后，产物在：

```powershell
d:\workspace\app\toapiproxy\build\dist
```

你会拿到这两个文件：

- `TOAPIPROXY_1.0.0_windows_x64_setup.exe`
- `TOAPIPROXY_1.0.0_windows_arm64_setup.exe`

### 7. 可选检查命令

```powershell
rustc -V
go version
cargo tauri -V
clang --version
```

## macOS 机器

目标：

- 一次构建出 `macOS Intel` 和 `macOS Apple Silicon` 两个 `dmg`

### 1. 仅首次执行：安装 Xcode Command Line Tools

```bash
xcode-select --install
```

如果系统提示已经安装，可以直接跳过。

### 2. 补齐 Rust 目标架构

```bash
rustup target add x86_64-apple-darwin aarch64-apple-darwin
```

### 3. 如果没装过 Tauri CLI，再执行一次安装

先检查：

```bash
cargo tauri -V
```

如果提示命令不存在，再执行：

```bash
cargo install tauri-cli --version "^2.0.0" --locked
```

### 4. 开始构建

不签名内测版：

```bash
cd /path/to/toapiproxy
make build-mac
```

如果你想做 `ad-hoc` 签名内测版：

```bash
cd /path/to/toapiproxy
make build-mac MACOS_SIGNING_IDENTITY=-
```

### 5. 构建结果

成功后，产物在：

```bash
build/dist
```

你会拿到这两个文件：

- `TOAPIPROXY_1.0.0_macos_intel.dmg`
- `TOAPIPROXY_1.0.0_macos_apple_silicon.dmg`

## 常见说明

### `ad-hoc` 是什么

`ad-hoc` 是 macOS 的临时签名方式：

- 不需要 Apple Developer Program 会员
- 不需要 Developer ID
- 适合个人内测
- 但不等于公证，测试用户首次打开时仍可能需要去“隐私与安全性”里手动放行

### 为什么 Windows x64 机器也能编 Windows ARM64

因为当前仓库的构建脚本已经支持：

- Rust 切换到 `aarch64-pc-windows-msvc`
- Go 后端切换到 `windows/arm64`

所以你不需要单独再找一台 ARM Windows 机器。

### 当前仓库里实际使用的命令

- Windows：`make build-win`
- macOS：`make build-mac`

如果以后只想单独编某一个架构，也可以继续用：

- `make build-windows-x64`
- `make build-windows-arm64`
- `make build-macos-intel`
- `make build-macos-arm64`
