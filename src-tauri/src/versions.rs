//! frp 版本下载 / 校验 / 解压 / 本地列表。
//! 下载 URL 支持镜像前缀（如 https://gh-proxy.com/），也支持 HTTP 代理。
use crate::paths;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
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
    let want_name = format!("frp_{ver}_windows_amd64.zip");
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
    let url = asset_url(&ver, &format!("frp_{ver}_windows_amd64.zip"), mirror);
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
    let exe_path = dir.join("frpc.exe");
    let mut found = false;
    {
        let f = std::fs::File::open(&zip_path).map_err(|e| format!("打开压缩包失败：{e}"))?;
        let mut archive = zip::ZipArchive::new(f).map_err(|e| format!("解析压缩包失败：{e}"))?;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("读取压缩包条目失败：{e}"))?;
            let name = entry.name().replace('\\', "/").to_ascii_lowercase();
            if name.ends_with("/frpc.exe") || name == "frpc.exe" {
                let mut out = std::fs::File::create(&exe_path)
                    .map_err(|e| format!("释放 frpc.exe 失败：{e}"))?;
                std::io::copy(&mut entry, &mut out).map_err(|e| format!("解压失败：{e}"))?;
                found = true;
                break;
            }
        }
    }
    if !found {
        return Err("压缩包里没有找到 frpc.exe".into());
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
        let exe = p.join("frpc.exe");
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
