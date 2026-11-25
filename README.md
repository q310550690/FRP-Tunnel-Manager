# FRP Tunnel Manager

基于 **Tauri 2 + Vue 3 + Element Plus** 的 frp 客户端管理工具（Windows / 中文界面 / 单文件便携）。
把「装 frp、写配置、开服务、看状态」做成一个可视化单页应用：不用手改 toml，不用记 nssm 命令。

## 功能特性

### 项目化管理
- **多项目并行**：每个项目 = 一条独立的 frpc 隧道，各自独立的配置、日志、服务与运行状态。
- **模板创建**：9 种常用内网穿透场景开箱即用 —— 远程桌面 (RDP)、Vite 前端预览、SSH、Web 反向代理、数据库、文件共享 (SMB)、游戏联机、点对点 (stcp)、空白模板。
- 每种模板带**专属配置项**（如 RDP 的 12 项组策略优化、HTTP 的域名/子域名/Host 改写/Basic 认证、stcp 的预共享密钥），并自动生成对应的 `frpc.toml`（带中文注释，实时预览）。

### 服务生命周期（Windows 服务，无需 nssm）
- **安装 / 卸载 / 启动 / 停止 / 重启** 服务，全部通过 Windows SCM 完成。
- 服务宿主就是本程序自身的 `--service-host` 模式：`sc create` 的 binPath 指向本 exe，开机自启、崩溃自动拉起，日志写入 `projects/<id>/frpc.log`。
- 服务名格式 `FRP Tunnel · <项目名>`；安装服务需要管理员权限（程序内一键提权重启）。
- 不安装服务也可以**直接以进程模式启动**，前台/后台运行、实时日志流、PID 与运行时长实时显示。

### FRP 版本管理
- 直接拉取 [GitHub Releases](https://api.github.com/repos/fatedier/frp/releases) 的**真实版本列表**（只支持 frp 0.52+），显示是否已安装、体积、被多少个项目使用。
- 一键**安装**：走镜像前缀（默认 `https://gh-proxy.com/`，可在设置里改，留空即直连）下载 zip → sha256 校验 → 只解压 `frpc.exe` 到 `versions/<版本>/`，带实时进度（连接 / 下载 / 校验 / 解压）。
- 「全局默认 FRP 版本」供新建项目选用；**删除拦截**：默认版本、仍有项目在用的版本不能删。
- 网络受限时可配置 **HTTP 代理**（GitHub API 与安装包下载都会走它）。

### 全局默认 + 项目覆盖
- 服务器地址 / 端口 / 传输协议 / TLS / Token 认证、传输优化（tcpMux、连接池、心跳）在设置页配一份**全局默认**。
- 每个项目默认继承全局值，打开「独立配置」即可单独覆盖，段头会实时显示当前继承到的值。

### 状态与日志（真实数据，不是模拟）
- 每 2.5 秒轮询 frpc 管理接口（`/api/status`），项目列表与概览页显示**隧道连接状态**（已连接 / 未连接）。
- 启动/停止后日志实时滚动（frpc 控制台输出，已剥离 ANSI 色码），重启应用后可回读历史日志。
- 「最近事件」直接取自 frpc 真实日志。

### RDP 远程桌面优化
- 一键读写 `HKLM\SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services` 下 **12 项组策略开关**（硬件图形适配器优先、H.264/AVC444 编码、WDDM、RemoteFX 压缩与画质、UDP 传输、网络检测、删除壁纸、RemoteApp 等）。
- 读取的就是系统当前值，支持「恢复未配置」。

### 界面
- 单页三栏布局：项目轨道 / 概述 · 配置 · 日志 / 顶栏聚合状态灯（运行 · 停止 · 错误计数）。
- 深色 / 浅色主题跟随系统，平面 LED 指示灯，动态图标（morphicons），自绘标题栏（最小化 / 最大化 / 关闭 + 拖动）。

## 开发

要求：Windows 10+、Rust (MSVC 工具链)、Node 22+ / pnpm、WebView2 运行时。

```bash
pnpm install

# 开发（Vite 1420 + Tauri 窗口）
pnpm tauri dev

# 前端类型检查 + 构建
pnpm build

# Rust 侧检查
cargo check --manifest-path src-tauri/Cargo.toml
```

> 提示：如果机器上装了 MSYS，`cargo` 构建前请确保 PATH 里 MSVC 的 `link.exe` 优先，否则可能链到 MSYS 的 link 报错。

### 无 UI 的服务自检（可选）

```bash
frp-tunnel-manager.exe --verify-service out.json
```

以管理员运行会走一遍「安装 → 查询 → 停止 → 卸载 → 查询」并把每步结果写进 `out.json`（需要已有 `projects/<id>/frpc.toml`），用于验证服务链路。

## 使用说明

1. **设置 → FRP 版本**：选一个版本点「安装」（走镜像，约 40 秒），并设为全局默认。
2. **设置 → 全局服务器**：填 frps 地址 / 端口 / 传输协议 / Token。
3. **左侧 + 新建项目**：挑一个模板，改端口和域名。
4. 点「启动服务」（进程模式）或「安装服务」（Windows 服务，需管理员），概览页看隧道状态与日志。

## 已知限制

- 仅支持 **frp 0.52+**（更早版本是 yaml 配置格式，不兼容）。
- 概览页流量显示 `—`：frpc 客户端管理接口没有流量/metrics 端点（实测 `/api/traffic`、`/metrics` 均 404），接入 frps 指标前不会显示数字。
- 当前仅 Windows；服务安装 / 组策略优化需要管理员权限。
- 版本安装包下载依赖 GitHub 或镜像可达，可在设置里配置代理与镜像前缀。
