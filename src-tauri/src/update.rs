//! 自动更新下载与安装。
//!
//! 只负责把安装包下载到便携目录、报告进度，以及在本机启动安装包；
//! 版本检查由前端请求 GitHub Release API 完成。
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::Emitter;

#[derive(Clone, Serialize)]
struct UpdateProgress {
    downloaded: u64,
    total: u64,
}

/// 安装包文件名只允许字母、数字、点、下划线、短横线，避免路径穿越。
fn safe_file_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 180 || name == "." || name == ".." {
        return Err("安装包文件名非法".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(format!("安装包文件名含非法字符：{name}"));
    }
    Ok(name.to_string())
}

fn build_agent(proxy: Option<&str>) -> Result<ureq::Agent, String> {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout(Duration::from_secs(600));
    if let Some(p) = proxy.map(str::trim).filter(|p| !p.is_empty()) {
        let parsed = ureq::Proxy::new(p).map_err(|e| format!("代理地址无效：{e}"))?;
        builder = builder.proxy(parsed);
    }
    Ok(builder.build())
}

/// 下载更新安装包到 `<exe>/config/updates/<version>/<filename>`。
/// 已存在且大小匹配时直接复用，避免重复下载。
#[tauri::command]
pub async fn update_download(
    app: tauri::AppHandle,
    url: String,
    filename: String,
    version: String,
    expected_size: Option<u64>,
    proxy: Option<String>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        download_blocking(&app, &url, &filename, &version, expected_size, proxy.as_deref())
    })
    .await
    .map_err(|e| format!("下载任务失败：{e}"))?
}

fn download_blocking(
    app: &tauri::AppHandle,
    url: &str,
    filename: &str,
    version: &str,
    expected_size: Option<u64>,
    proxy: Option<&str>,
) -> Result<String, String> {
    let url = url.trim();
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("下载地址必须是 http(s) 链接".into());
    }
    let version = crate::paths::safe_ver(version)?;
    let filename = safe_file_name(filename)?;
    let dir = crate::paths::config_dir()?.join("updates").join(&version);
    fs::create_dir_all(&dir).map_err(|e| format!("创建更新目录失败：{e}"))?;
    let final_path = dir.join(&filename);

    if let Some(expected) = expected_size.filter(|n| *n > 0) {
        if final_path
            .metadata()
            .map(|m| m.is_file() && m.len() == expected)
            .unwrap_or(false)
        {
            return Ok(final_path.display().to_string());
        }
    }

    let tmp_path = dir.join(format!("{filename}.download"));
    let agent = build_agent(proxy)?;
    let resp = agent
        .get(url)
        .set("User-Agent", "FRP-Tunnel-Manager")
        .call()
        .map_err(|e| format!("下载安装包失败：{e}"))?;

    let total = expected_size
        .filter(|n| *n > 0)
        .or_else(|| {
            resp.header("Content-Length")
                .and_then(|s| s.trim().parse::<u64>().ok())
        })
        .unwrap_or(0);

    let mut reader = resp.into_reader();
    let mut file = File::create(&tmp_path).map_err(|e| format!("创建临时文件失败：{e}"))?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut downloaded = 0_u64;
    let mut last_emit = Instant::now() - Duration::from_secs(1);

    let result = (|| -> Result<(), String> {
        loop {
            let n = reader
                .read(&mut buffer)
                .map_err(|e| format!("读取下载数据失败：{e}"))?;
            if n == 0 {
                break;
            }
            file.write_all(&buffer[..n])
                .map_err(|e| format!("写入安装包失败：{e}"))?;
            downloaded += n as u64;
            if last_emit.elapsed() >= Duration::from_millis(100) {
                let _ = app.emit(
                    "update-progress",
                    UpdateProgress {
                        downloaded,
                        total,
                    },
                );
                last_emit = Instant::now();
            }
        }
        file.flush().map_err(|e| format!("写入安装包失败：{e}"))?;
        let _ = app.emit(
            "update-progress",
            UpdateProgress {
                downloaded,
                total,
            },
        );
        if total > 0 && downloaded != total {
            return Err(format!(
                "安装包大小不一致：期望 {total} 字节，实际 {downloaded} 字节"
            ));
        }
        if downloaded == 0 {
            return Err("下载到的安装包为空".into());
        }
        Ok(())
    })();

    drop(file);
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    if final_path.exists() {
        let _ = fs::remove_file(&final_path);
    }
    fs::rename(&tmp_path, &final_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("保存安装包失败：{e}")
    })?;
    Ok(final_path.display().to_string())
}

/// 返回更新包大小；文件不存在时返回 0，供启动时校验上次下载是否仍有效。
#[tauri::command]
pub fn update_file_size(path: String) -> u64 {
    fs::metadata(path)
        .map(|m| if m.is_file() { m.len() } else { 0 })
        .unwrap_or(0)
}

/// 启动安装包。Windows 下会先等安装程序完成 UAC 初始化，再退出当前应用；
/// macOS / Linux 则用系统默认方式打开安装包，应用保持运行。
#[tauri::command]
pub fn update_install(app: tauri::AppHandle, path: String) -> Result<String, String> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err("安装包不存在，请重新下载".into());
    }
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new(&path);
        if let Some(parent) = path.parent() {
            cmd.current_dir(parent);
        }
        cmd.spawn().map_err(|e| format!("启动安装程序失败：{e}"))?;
        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(800));
            handle.exit(0);
        });
        Ok("installer".into())
    }
    #[cfg(not(windows))]
    {
        let _ = &app;
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(opener)
            .arg(path.as_path())
            .spawn()
            .map_err(|e| format!("打开安装包失败：{e}"))?;
        Ok("opened".into())
    }
}
