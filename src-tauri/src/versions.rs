//! frp 版本下载 / 校验 / 解压 / 本地列表。
//! 下载 URL 支持镜像前缀（如 https://gh-proxy.com/），也支持 HTTP 代理。
use crate::paths;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
pub struct VersionProgress {
    pub version: String,
    pub phase: String,
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Serialize)]
pub struct LocalVersion {
    pub version: String,
    pub size: u64,
}

fn agent(proxy: Option<&str>) -> Result<ureq::Agent, String> {
    let mut b = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(180));
    if let Some(p) = proxy.map(str::trim).filter(|p| !p.is_empty()) {
        let parsed = ureq::Proxy::new(p).map_err(|e| format!("代理地址无效：{e}"))?;
        b = b.proxy(parsed);
    }
    Ok(b.build())
}

fn with_mirror(raw: &str, mirror: Option<&str>) -> String {
    match mirror.map(str::trim).filter(|m| !m.is_empty()) {
        None => raw.to_string(),
        Some(m) if m.ends_with('/') => format!("{m}{raw}"),
        Some(m) => format!("{m}/{raw}"),
    }
}

fn asset_url(version: &str, file: &str, mirror: Option<&str>) -> String {
    let raw = format!(
        "https://github.com/fatedier/frp/releases/download/v{version}/{file}"
    );
    with_mirror(&raw, mirror)
}

/// 当前平台对应的 frp 资产标签（GitHub release 压缩包按它区分平台）
const PLATFORM_TAG: &str = if cfg!(target_os = "macos") {
    if cfg!(target_arch = "aarch64") {
        "darwin_arm64"
    } else {
        "darwin_amd64"
    }
} else if cfg!(windows) {
    "windows_amd64"
} else if cfg!(target_arch = "aarch64") {
    "linux_arm64"
} else {
    "linux_amd64"
};

/// frp 官方 release 压缩包名，如 `frp_0.62.1_darwin_arm64.zip`
fn zip_name(version: &str) -> String {
    format!("frp_{version}_{PLATFORM_TAG}.zip")
}

fn emit(app: &AppHandle, p: VersionProgress) {
    let _ = app.emit("version-progress", p);
}

fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("打开文件失败：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("读取失败：{e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 尝试用官方 checksums 文件校验下载的 zip；拿不到校验文件时跳过（返回 None）
fn verify_checksum(
    ver: &str,
    zip: &std::path::Path,
    agent: &ureq::Agent,
    mirror: Option<&str>,
) -> Result<Option<String>, String> {
    let url = asset_url(ver, "frp_sha256_checksums.txt", mirror);
    let resp = match agent
        .get(&url)
        .set("User-Agent", "FRP-Tunnel-Manager")
        .call()
    {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };
    let text = resp.into_string().map_err(|e| format!("读取校验文件失败：{e}"))?;
    let want_name = zip_name(ver);
    let expected = text
        .lines()
        .find(|l| l.trim_end().ends_with(&want_name))
        .and_then(|l| l.split_whitespace().next())
        .map(|s| s.to_ascii_lowercase());
    let Some(expected) = expected else {
        return Ok(None);
    };
    let actual = sha256_file(zip)?;
    if actual != expected {
        return Err(format!(
            "校验失败：期望 {expected}，实际 {actual}（可能是镜像返回了错误内容）"
        ));
    }
    Ok(Some(actual))
}

fn install_blocking(
    app: &AppHandle,
    version: &str,
    proxy: Option<&str>,
    mirror: Option<&str>,
) -> Result<String, String> {
    let ver = paths::safe_ver(version)?;
    let dir = paths::versions_dir()?.join(&ver);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建版本目录失败：{e}"))?;
    let zip_path = dir.join("download.zip");
    let agent = agent(proxy)?;
    let url = asset_url(&ver, &zip_name(&ver), mirror);
    emit(
        app,
        VersionProgress {
            version: ver.clone(),
            phase: "connect".into(),
            downloaded: 0,
            total: 0,
        },
    );
    let resp = agent
        .get(&url)
        .set("User-Agent", "FRP-Tunnel-Manager")
        .call()
        .map_err(|e| format!("下载失败（{url}）：{e}"))?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&zip_path).map_err(|e| format!("创建临时文件失败：{e}"))?;
    let mut buf = vec![0u8; 65536];
    let mut done: u64 = 0;
    let mut last_emit: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("读取下载流失败：{e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| format!("写入失败：{e}"))?;
        done += n as u64;
        if done - last_emit >= 256 * 1024 || (total > 0 && done >= total) {
            last_emit = done;
            emit(
                app,
                VersionProgress {
                    version: ver.clone(),
                    phase: "download".into(),
                    downloaded: done,
                    total,
                },
            );
        }
    }
    file.flush().map_err(|e| format!("写入失败：{e}"))?;
    drop(file);

    emit(
        app,
        VersionProgress {
            version: ver.clone(),
            phase: "verify".into(),
            downloaded: done,
            total,
        },
    );
    let _hash = verify_checksum(&ver, &zip_path, &agent, mirror)?;

    emit(
        app,
        VersionProgress {
            version: ver.clone(),
            phase: "extract".into(),
            downloaded: done,
            total,
        },
    );
    let exe_path = dir.join(paths::frpc_name());
    let mut found = false;
    let want = paths::frpc_name();
    {
        let f = std::fs::File::open(&zip_path).map_err(|e| format!("打开压缩包失败：{e}"))?;
        let mut archive = zip::ZipArchive::new(f).map_err(|e| format!("解析压缩包失败：{e}"))?;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("读取压缩包条目失败：{e}"))?;
            let name = entry.name().replace('\\', "/").to_ascii_lowercase();
            if name.ends_with(&format!("/{want}")) || name == want {
                let mut out = std::fs::File::create(&exe_path)
                    .map_err(|e| format!("释放 frpc.exe 失败：{e}"))?;
                std::io::copy(&mut entry, &mut out).map_err(|e| format!("解压失败：{e}"))?;
                found = true;
                break;
            }
        }
    }
    if !found {
        return Err(format!("压缩包里没有找到 {}", paths::frpc_name()));
    }
    // zip 解压不带可执行位，macOS / Linux 上必须补 +x，否则 spawn 会报 Permission denied
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("设置 frpc 可执行权限失败：{e}"))?;
    }
    let _ = std::fs::remove_file(&zip_path);

    emit(
        app,
        VersionProgress {
            version: ver.clone(),
            phase: "done".into(),
            downloaded: done,
            total,
        },
    );
    Ok(exe_path.display().to_string())
}

#[tauri::command]
pub async fn version_install(
    app: AppHandle,
    version: String,
    proxy: Option<String>,
    mirror: Option<String>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        install_blocking(&app, &version, proxy.as_deref(), mirror.as_deref())
    })
    .await
    .map_err(|e| format!("下载任务异常：{e}"))?
}

#[tauri::command]
pub fn version_list_local() -> Result<Vec<LocalVersion>, String> {
    let dir = paths::versions_dir()?;
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Ok(out);
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let exe = p.join(paths::frpc_name());
        if !exe.exists() {
            continue;
        }
        let size = std::fs::metadata(&exe).map(|m| m.len()).unwrap_or(0);
        out.push(LocalVersion {
            version: e.file_name().to_string_lossy().to_string(),
            size,
        });
    }
    out.sort_by(|a, b| b.version.cmp(&a.version));
    Ok(out)
}

#[tauri::command]
pub fn version_remove(version: String) -> Result<(), String> {
    let ver = paths::safe_ver(&version)?;
    let dir = paths::versions_dir()?.join(&ver);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("删除失败：{e}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 外部 frpc：扫描系统里已安装的 frpc，以及把外部 frpc「接管」进便携版本库
// ---------------------------------------------------------------------------

/// 系统里已安装的外部 frpc。
/// 扫描只读：绝不修改、移动或删除外部文件。
#[derive(Serialize, Clone)]
pub struct ExternalVersion {
    pub version: String,
    /// 可执行文件绝对路径（Windows 已去掉 canonicalize 的 `\\?\` 前缀）
    pub path: String,
    /// 来源说明，如 `PATH` / `Chocolatey` / `Scoop` / `WinGet` / `Program Files` / `常见目录`
    pub source: String,
}

/// 候选路径：路径 + 来源说明
type Candidate = (PathBuf, &'static str);

/// 最多收集的候选路径数，避免 PATH 很长或目录很大时扫太多
const MAX_CANDIDATES: usize = 40;
/// 最多真正执行 `--version` 探测的数量，避免个别 exe 拖慢整个命令
const MAX_PROBES: usize = 24;
/// 单个探测最长等待时间：外部 exe 万一卡住就强杀，绝不让命令永不返回
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Windows：去掉 canonicalize 产生的 `\\?\` 前缀，界面上显示正常路径
#[cfg(windows)]
fn strip_verbatim(s: &str) -> String {
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    match s.strip_prefix(r"\\?\") {
        Some(rest) => rest.to_string(),
        None => s.to_string(),
    }
}

/// 规范化路径用于比较：Windows 去前缀并转小写，分隔符统一成 `/`
fn norm_path(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    #[cfg(windows)]
    let s = strip_verbatim(&s).to_ascii_lowercase();
    let mut s = s.replace('\\', "/");
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// 对外展示的路径（Windows 去掉 `\\?\` 前缀）
fn display_path(p: &Path) -> String {
    #[cfg(windows)]
    {
        strip_verbatim(&p.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        p.to_string_lossy().to_string()
    }
}

/// p 是否就是 dir，或位于 dir 之下（两者都必须先经 `norm_path` 规范化）
fn under(p: &str, dir: &str) -> bool {
    if dir.is_empty() {
        return false;
    }
    if p == dir {
        return true;
    }
    let mut prefix = String::with_capacity(dir.len() + 1);
    prefix.push_str(dir);
    prefix.push('/');
    p.starts_with(&prefix)
}

/// 需要跳过的目录：本程序自己的便携目录（versions 及程序目录里的东西都不算外部安装）
struct SkipDirs {
    dirs: Vec<String>,
}

impl SkipDirs {
    fn new() -> Self {
        let mut dirs = Vec::new();
        for d in [paths::versions_dir().ok(), paths::base_dir().ok()]
            .into_iter()
            .flatten()
        {
            dirs.push(norm_path(&d));
            if let Ok(canon) = d.canonicalize() {
                dirs.push(norm_path(&canon));
            }
        }
        Self { dirs }
    }

    fn contains(&self, p: &Path) -> bool {
        let p = norm_path(p);
        self.dirs.iter().any(|d| under(&p, d))
    }
}

/// 收集一个候选：必须是存在的文件，canonicalize 去重，跳过本程序自己的目录
fn push_candidate(
    out: &mut Vec<Candidate>,
    seen: &mut HashSet<String>,
    path: PathBuf,
    source: &'static str,
    skip: &SkipDirs,
) {
    if out.len() >= MAX_CANDIDATES || !path.is_file() {
        return;
    }
    let Ok(canon) = path.canonicalize() else {
        return;
    };
    if skip.contains(&canon) || !seen.insert(norm_path(&canon)) {
        return;
    }
    out.push((canon, source));
}

/// 读取一个非空的 Windows 环境变量目录
#[cfg(windows)]
fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// Windows 常见安装位置（含包管理器 shim）
#[cfg(windows)]
fn fixed_candidates() -> Vec<Candidate> {
    let exe = paths::frpc_name();
    let mut v: Vec<Candidate> = Vec::new();
    if let Some(pf) = env_dir("ProgramFiles") {
        v.push((pf.join("frp").join(exe), "Program Files"));
    }
    if let Some(pf) = env_dir("ProgramFiles(x86)") {
        v.push((pf.join("frp").join(exe), "Program Files"));
    }
    v.push((PathBuf::from(r"C:\frp").join(exe), "常见目录"));
    if let Some(home) = env_dir("USERPROFILE") {
        v.push((home.join("frp").join(exe), "用户目录"));
        v.push((home.join("Downloads").join("frp").join(exe), "下载目录"));
        v.push((home.join("scoop").join("shims").join(exe), "Scoop"));
    }
    if let Some(pd) = env_dir("ProgramData") {
        v.push((pd.join("chocolatey").join("bin").join(exe), "Chocolatey"));
    }
    if let Some(la) = env_dir("LOCALAPPDATA") {
        v.push((
            la.join("Microsoft").join("WinGet").join("Links").join(exe),
            "WinGet",
        ));
    }
    v
}

/// macOS / Linux 常见安装位置
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn fixed_candidates() -> Vec<Candidate> {
    let exe = paths::frpc_name();
    let mut v: Vec<Candidate> = Vec::new();
    for dir in [
        "/usr/local/bin",
        "/opt/homebrew/bin",
        "/opt/local/bin",
        "/usr/bin",
        "/opt/frp",
    ] {
        v.push((PathBuf::from(dir).join(exe), "常见目录"));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        v.push((home.join("frp").join(exe), "用户目录"));
        v.push((home.join(".local").join("bin").join(exe), "用户目录"));
    }
    v
}

/// 其他平台（Tauri 移动端等）：没有固定候选
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn fixed_candidates() -> Vec<Candidate> {
    Vec::new()
}

/// Windows 一层目录展开：`frp_0.61.1_windows_amd64/frpc.exe` 这类解压结构。
/// 只枚举一层（总深度 2），不做全盘递归；最多返回 `limit` 个可执行文件路径。
#[cfg(windows)]
fn scan_frp_dirs(base: &Path, limit: usize) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let Ok(rd) = std::fs::read_dir(base) else {
        return out;
    };
    for entry in rd.flatten() {
        if out.len() >= limit {
            break;
        }
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.starts_with("frp") {
            continue;
        }
        // 深度 1：frp_0.61.1_windows_amd64/frpc.exe
        let exe = dir.join(paths::frpc_name());
        if exe.is_file() {
            out.push(exe);
            continue;
        }
        // 深度 2：frp/frp_0.61.1_windows_amd64/frpc.exe、frp/bin/frpc.exe
        let Ok(rd2) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry2 in rd2.flatten() {
            if out.len() >= limit {
                return out;
            }
            let sub = entry2.path();
            if !sub.is_dir() {
                continue;
            }
            let exe2 = sub.join(paths::frpc_name());
            if exe2.is_file() {
                out.push(exe2);
            }
        }
    }
    out
}

/// Windows：在几个基础目录里找 frp* 子目录做一层展开
#[cfg(windows)]
fn expanded_candidates() -> Vec<Candidate> {
    let mut bases: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Some(pf) = env_dir("ProgramFiles") {
        bases.push((pf, "Program Files"));
    }
    if let Some(pf) = env_dir("ProgramFiles(x86)") {
        bases.push((pf, "Program Files"));
    }
    if let Some(home) = env_dir("USERPROFILE") {
        bases.push((home.clone(), "用户目录"));
        bases.push((home.join("Downloads"), "下载目录"));
        bases.push((home.join("Desktop"), "桌面"));
    }
    // 程序 exe 所在目录的父目录（便携版常被解压到 frp* 文件夹旁边）
    if let Ok(base) = paths::base_dir() {
        if let Some(parent) = base.parent() {
            bases.push((parent.to_path_buf(), "程序目录"));
        }
    }
    let mut out: Vec<Candidate> = Vec::new();
    for (base, source) in bases {
        out.extend(
            scan_frp_dirs(&base, MAX_CANDIDATES)
                .into_iter()
                .map(|p| (p, source)),
        );
    }
    out
}

/// 除 Windows 外不做目录展开（固定候选已覆盖常见位置）
#[cfg(not(windows))]
fn expanded_candidates() -> Vec<Candidate> {
    Vec::new()
}

/// 收集全部候选：PATH → 各平台固定位置 → Windows 一层目录展开
fn collect_candidates() -> Vec<Candidate> {
    let skip = SkipDirs::new();
    let mut out: Vec<Candidate> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // a. 环境变量 PATH 的每个目录
    if let Some(raw) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&raw) {
            if out.len() >= MAX_CANDIDATES {
                break;
            }
            if dir.as_os_str().is_empty() {
                continue;
            }
            push_candidate(
                &mut out,
                &mut seen,
                dir.join(paths::frpc_name()),
                "PATH",
                &skip,
            );
        }
    }

    // b / d. 各平台固定候选
    for (path, source) in fixed_candidates() {
        if out.len() >= MAX_CANDIDATES {
            break;
        }
        push_candidate(&mut out, &mut seen, path, source, &skip);
    }

    // c. Windows 一层目录展开（其他平台为空）
    for (path, source) in expanded_candidates() {
        if out.len() >= MAX_CANDIDATES {
            break;
        }
        push_candidate(&mut out, &mut seen, path, source, &skip);
    }

    out
}

/// 从命令输出里取第一个形如 `0.61.1` 的版本号（手写扫描，不引入正则依赖）
fn parse_version(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut p = start;
        let mut ok = true;
        for part in 0..3 {
            let from = p;
            while p < bytes.len() && bytes[p].is_ascii_digit() {
                p += 1;
            }
            if p == from {
                ok = false;
                break;
            }
            if part < 2 {
                if p < bytes.len() && bytes[p] == b'.' {
                    p += 1;
                } else {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return Some(text[start..p].to_string());
        }
        i = start + 1;
    }
    None
}

/// 版本号比较键：`0.61.1` 要大于 `0.9.0`，不能按字符串比
fn version_key(v: &str) -> [u64; 3] {
    let mut parts = v.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    [
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    ]
}

/// 运行 `<exe> --version` 探测版本：只读、不走 shell、Windows 不弹黑框、超时强杀
fn probe_version(exe: &Path) -> Option<String> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW：避免每探测一个 exe 就闪一下控制台黑框
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().ok()?;
    let deadline = std::time::Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    parse_version(&text)
}

/// 扫描系统里已安装的外部 frpc（只读，不修改任何外部文件）。
/// 放到阻塞线程池执行：探测外部 exe 有超时，不能卡住 IPC 线程。
#[tauri::command]
pub async fn version_scan_external() -> Vec<ExternalVersion> {
    tauri::async_runtime::spawn_blocking(scan_external_blocking)
        .await
        .unwrap_or_default()
}

fn scan_external_blocking() -> Vec<ExternalVersion> {
    let mut out: Vec<ExternalVersion> = Vec::new();
    for (path, source) in collect_candidates().into_iter().take(MAX_PROBES) {
        let Some(version) = probe_version(&path) else {
            continue;
        };
        out.push(ExternalVersion {
            version,
            path: display_path(&path),
            source: source.to_string(),
        });
    }
    // 版本从高到低；同版本按路径升序。候选阶段已按 canonical path 去重。
    out.sort_by(|a, b| {
        version_key(&b.version)
            .cmp(&version_key(&a.version))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

/// 校验外部文件确实是名为 frpc 的可执行文件。
fn frpc_source(path: &str) -> Result<PathBuf, String> {
    let src = PathBuf::from(path.trim());
    if !src.is_file() {
        return Err(format!("找不到 frpc 文件：{}", src.display()));
    }
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if !stem.eq_ignore_ascii_case("frpc") {
        return Err(format!("请选择名为 frpc 的可执行文件：{}", src.display()));
    }
    Ok(src)
}

/// 手动选择一个 frpc 文件：只读探测版本，不复制、不改动源文件。
/// 与自动扫描一样放到阻塞线程池：探测最多等 3 秒，不能卡住 IPC 线程。
#[tauri::command]
pub async fn version_probe_file(path: String) -> Result<ExternalVersion, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let src = frpc_source(&path)?;
        let version = probe_version(&src).ok_or_else(|| {
            format!(
                "无法识别版本号：{}。请确认这是可正常运行的 frpc 客户端。",
                display_path(&src)
            )
        })?;
        Ok(ExternalVersion {
            version,
            path: display_path(&src),
            source: "手动选择".into(),
        })
    })
    .await
    .map_err(|e| format!("读取 frpc 失败：{e}"))?
}

/// 接管外部 frpc：复制进便携版本库 `versions/<version>/frpc[.exe]`，
/// 之后就能像内置版本一样使用；只复制，不改动源文件。
#[tauri::command]
pub fn version_take_over(version: String, path: String) -> Result<String, String> {
    let ver = paths::safe_ver(&version)?;
    let src = frpc_source(&path)?;
    let dst = paths::frpc_exe(&ver)?;
    if dst.exists() {
        return Err(format!("frp {ver} 已经在版本库里了"));
    }
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建版本目录失败：{e}"))?;
    }
    std::fs::copy(&src, &dst).map_err(|e| format!("复制 frpc 失败：{e}"))?;
    // macOS / Linux 上复制过来的文件没有可执行位，必须补 +x，否则 spawn 报 Permission denied
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("设置 frpc 可执行权限失败：{e}"))?;
    }
    Ok(dst.display().to_string())
}
