//! Windows 服务（SCM）管理：内部调用 sc.exe，服务宿主是本程序自身（--service-host 模式）。
use crate::paths;
use serde::Serialize;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize)]
pub struct SvcResult {
    pub ok: bool,
    pub output: String,
}

/// sc.exe 在中日文系统上输出 GBK，这里按 GBK 解码，失败再退 UTF-8
fn decode(bytes: &[u8]) -> String {
    let (cow, _, had_errors) = encoding_rs::GBK.decode(bytes);
    if had_errors {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        cow.into_owned()
    }
}

fn sc(args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("sc.exe");
    cmd.args(args);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().map_err(|e| format!("调用 sc.exe 失败：{e}"))?;
    let text = decode(&out.stdout) + &decode(&out.stderr);
    if !out.status.success() {
        let body = text.trim();
        let hint = if body.contains("拒绝访问") || body.to_lowercase().contains("access is denied") {
            "（需要管理员权限：先关闭本程序，再右键 → 以管理员身份运行）"
        } else {
            ""
        };
        return Err(format!(
            "sc {} 失败（退出码 {:?}）：{}{}",
            args.first().unwrap_or(&""),
            out.status.code(),
            body,
            hint
        ));
    }
    Ok(text)
}

fn host_bin(id: &str, name: &str, version: &str, toml: &str, log: &str) -> Result<String, String> {
    let host = std::env::current_exe().map_err(|e| format!("定位程序路径失败：{e}"))?;
    let frpc = paths::frpc_exe(version)?;
    Ok(format!(
        "\"{}\" --service-host --project \"{}\" --name \"{}\" --frpc \"{}\" --config \"{}\" --log \"{}\"",
        host.display(),
        id,
        name,
        frpc.display(),
        toml,
        log
    ))
}

#[tauri::command]
pub fn service_install(
    name: String,
    display: String,
    description: String,
    id: String,
    version: String,
) -> Result<SvcResult, String> {
    let id = paths::safe_id(&id)?;
    let toml = paths::project_toml(&id)?;
    if !toml.exists() {
        return Err("还没有生成 frpc.toml，请先点一次「启动」或保存配置".into());
    }
    let log = paths::project_log(&id)?;
    if let Some(dir) = log.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let bin = host_bin(&id, &name, &version, &toml.display().to_string(), &log.display().to_string())?;

    // 已存在则先删掉，保证幂等
    let _ = sc(&["stop", &name]);
    let _ = sc(&["delete", &name]);

    let mut out = String::new();
    out += &sc(&[
        "create",
        &name,
        "binPath=",
        &bin,
        "start=",
        "auto",
        "DisplayName=",
        &display,
    ])?;
    let _ = sc(&["description", &name, &description]);
    let start = sc(&["start", &name]);
    out += &start.unwrap_or_else(|e| format!("\n启动返回：{e}"));
    Ok(SvcResult { ok: true, output: out })
}

#[tauri::command]
pub fn service_uninstall(name: String) -> Result<SvcResult, String> {
    let mut out = String::new();
    if let Ok(t) = sc(&["stop", &name]) {
        out += &t;
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    match sc(&["delete", &name]) {
        Ok(t) => out += &t,
        Err(e) if e.contains("1060") || e.contains("指定的服务未安装") => {
            out += "[SC] 服务不存在，无需删除";
        }
        Err(e) => return Err(e),
    }
    Ok(SvcResult { ok: true, output: out })
}

#[tauri::command]
pub fn service_control(name: String, action: String) -> Result<SvcResult, String> {
    let a = match action.as_str() {
        "start" => "start",
        "stop" => "stop",
        other => return Err(format!("未知操作：{other}")),
    };
    let out = sc(&[a, &name])?;
    Ok(SvcResult { ok: true, output: out })
}

#[tauri::command]
pub fn service_restart(name: String) -> Result<SvcResult, String> {
    let mut out = String::new();
    if let Ok(t) = sc(&["stop", &name]) {
        out += &t;
    }
    std::thread::sleep(std::time::Duration::from_millis(800));
    out += &sc(&["start", &name])?;
    Ok(SvcResult { ok: true, output: out })
}

/// 查询服务状态（SERVICE_NAME 不存在时返回 not-installed）
#[tauri::command]
pub fn service_query(name: String) -> Result<SvcResult, String> {
    match sc(&["query", &name]) {
        Ok(text) => Ok(SvcResult { ok: true, output: text }),
        Err(e) => {
            if e.contains("1060") || e.contains("指定的服务未安装") {
                Ok(SvcResult {
                    ok: false,
                    output: "not-installed".into(),
                })
            } else {
                Err(e)
            }
        }
    }
}
