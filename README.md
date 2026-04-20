# TOAPIPROXY

TOAPIPROXY 是一款基于 Tauri 构建的跨平台桌面应用，用于代理和统一管理多个 AI 服务的 API 请求。

**仅限个人学习使用，不得商用，bug自负**

## 功能特性

### 核心功能
- **多服务代理** - 支持 Claude、Codex、Gemini、Copilot、Qwen、Kiro、Antigravity 等主流 AI 服务
- **OAuth 认证管理** - 简化的浏览器认证流程，安全存储认证信息
- **多账户轮询** - 支持多账户负载均衡和自动故障转移
- **用量监控** - 实时查看各账户的 API 使用情况和配额

### 特色功能
- **Thinking 代理** - 自动处理 Claude 模型的 `thinking` 参数，支持 `claude-*-thinking-*` 格式的模型名称转换
- **Cloudflare Tunnel** - 一键创建公网访问地址，远程也能使用本地代理
- **开机自启动** - 支持系统启动时自动运行
- **系统托盘** - 最小化到托盘，不占用任务栏

### 跨平台
- Windows (x64)
- macOS (Intel & Apple Silicon)
- Linux

## 界面预览

```
┌─────────────────────────────────────────────────────────┐
│ [Logo] TOAPIPROXY                                       │
├────────────┬────────────────────────────────────────────┤
│            │                                            │
│  ⚙️ 常规   │  服务器状态                                │
│            │  ┌──────────────────────────────────────┐  │
│  ☁️ 服务   │  │ ● 运行中                    [停止]  │  │
│            │  └──────────────────────────────────────┘  │
│  👤 Kiro   │                                            │
│            │  设置                                       │
│  💻 Codex  │  ┌──────────────────────────────────────┐  │
│            │  │ ☑ 开机自启动                          │  │
│  ℹ️ 关于   │  │ [复制地址]  [打开认证文件夹]          │  │
│            │  └──────────────────────────────────────┘  │
│            │                                            │
└────────────┴────────────────────────────────────────────┘
```

## 系统要求

- Windows 10/11 (x64)
- macOS 10.15+ (Intel 或 Apple Silicon)
- Linux (主流发行版)

## 安装

### Windows

下载 `TOAPIPROXY_x.x.x_x64-setup.exe` 安装包，双击运行即可。

### macOS / Linux

```bash
# 下载对应平台的安装包
# macOS: TOAPIPROXY_x.x.x_x64.dmg 或 .app.tar.gz
# Linux: TOAPIPROXY_x.x.x_amd64.deb 或 .AppImage
```

## 快速开始

### 1. 启动应用

安装完成后运行 TOAPIPROXY，代理服务器会自动启动（默认端口 8317）。

### 2. 连接服务

在 **服务** 页面，点击对应服务的「连接」按钮：

| 服务 | 说明 |
|------|------|
| Claude | Anthropic Claude AI |
| Codex | OpenAI Codex / ChatGPT |
| Gemini | Google Gemini |
| Copilot | GitHub Copilot (Claude/GPT/Gemini) |
| Qwen | 通义千问 |
| Kiro | AWS CodeWhisperer / Kiro |
| Antigravity | Gemini & Claude 组合服务 |

### 3. 使用代理

连接成功后，将 AI 客户端的 API 地址设置为：

```
http://127.0.0.1:8317
```

### 4. 模型名称格式

对于 Claude 的 thinking 功能，可以使用以下格式：

```
claude-sonnet-4-20250514-thinking-15000
```

系统会自动提取 `claude-sonnet-4-20250514` 并添加 `thinking: { type: "enabled", budget_tokens: 14999 }` 参数。

## 配置说明

### 认证文件位置

认证信息存储在 `~/.cli-proxy-api/` 目录下，每个服务一个 JSON 文件。

### 端口说明

| 端口 | 用途 |
|------|------|
| 8317 | TOAPIPROXY 代理端口（对外） |
| 8318 | CLIProxyAPIPlus 后端端口（内部） |

### Cloudflare Tunnel

在「常规」页面可以启动 Cloudflare Tunnel，将本地服务暴露到公网：

1. 点击「启动 Tunnel」
2. 等待获取公网 URL
3. 点击「复制」分享给其他人

## 构建开发

### 环境要求

- Node.js 18+
- Rust 1.77+
- Go 1.21+（用于构建 CLIProxyAPIPlus）

### 安装依赖

```bash
# 克隆项目
git clone <repo-url>
cd TOAPIPROXY

# 安装 Rust 依赖
cargo fetch

# 安装 Node 依赖
npm install
```

### 开发模式

```bash
# 使用 Makefile（推荐）
make dev

# 或直接使用 Tauri
cargo tauri dev
```

### 构建安装包

```bash
# Windows
make build

# macOS / Linux
make build
```

### 单独构建 CLIProxyAPIPlus

```bash
make build-cli-proxy
```

## 技术栈

- **桌面框架**: Tauri 2.x
- **后端语言**: Rust
- **后端服务**: CLIProxyAPIPlus (Go)
- **前端**: HTML, CSS, JavaScript (原生)

## 目录结构

```
TOAPIPROXY/
├── src/                      # 前端代码
│   ├── index.html            # 主页面
│   ├── main.js               # 前端逻辑
│   ├── styles.css            # 样式文件
│   └── assets/               # 静态资源
├── src-tauri/                # Rust 后端
│   ├── src/
│   │   ├── main.rs           # 程序入口
│   │   ├── lib.rs            # 应用主逻辑
│   │   ├── auth/             # 认证管理
│   │   ├── commands/         # Tauri 命令
│   │   ├── server/            # 服务器管理
│   │   ├── thinking_proxy/    # Thinking 代理
│   │   ├── tunnel/           # Tunnel 管理
│   │   ├── usage/            # 用量获取
│   │   ├── codex/            # Codex API
│   │   └── watcher/          # 文件监控
│   ├── resources/            # 打包资源
│   ├── Cargo.toml            # Rust 依赖
│   └── tauri.conf.json       # Tauri 配置
├── tmp/
│   └── CLIProxyAPIPlus/      # 临时文件
├── third_party/
│   └── CLIProxyAPIPlus/      # 代理核心 (Go subtree)
├── scripts/                  # 构建脚本
├── package.json
└── Makefile
```

## 常见问题

### Q: 代理无法连接

1. 检查服务器是否启动（状态显示「运行中」）
2. 检查端口 8317 是否被占用
3. 重启应用

### Q: 认证失败

1. 删除 `~/.cli-proxy-api/` 目录下的认证文件
2. 重新连接服务进行认证

### Q: macOS 无法打开

```bash
xattr -rd com.apple.quarantine /Applications/TOAPIPROXY.app
```

## 更新日志

详见 [CHANGELOG.md](./CHANGELOG.md)

## 许可证

MIT License

## 联系方式

- 问题反馈: [GitHub Issues](https://github.com/your-repo/issues)

---

© 2026 TOAPIPROXY
