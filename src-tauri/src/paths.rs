//! 便携目录约定：一切都在 exe 所在目录下，绝不写 %APPDATA%。
use std::path::PathBuf;

/// 程序根目录（exe 所在目录）
pub fn base_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("定位程序目录失败：{e}"))?;
    Ok(exe
        .parent()
        .ok_or_else(|| "定位程序目录失败".to_string())?
        .to_path_buf())
}

pub fn config_dir() -> Result<PathBuf, String> {
    Ok(base_dir()?.join("config"))
}

pub fn projects_dir() -> Result<PathBuf, String> {
    Ok(base_dir()?.join("projects"))
}

pub fn versions_dir() -> Result<PathBuf, String> {
    Ok(base_dir()?.join("versions"))
}

/// 项目 id 只允许字母、数字、下划线、短横线，避免路径穿越
pub fn safe_id(id: &str) -> Result<String, String> {
    let id = id.trim();
    if id.is_empty() || id.len() > 80 {
        return Err("项目 id 非法".into());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("项目 id 含非法字符：{id}"));
    }
    Ok(id.to_string())
}

/// 版本号只允许 数字.数字.数字
pub fn safe_ver(v: &str) -> Result<String, String> {
    let v = v.trim();
    let ok = !v.is_empty()
        && v.len() <= 20
        && v.chars().all(|c| c.is_ascii_digit() || c == '.')
        && v.split('.').count() == 3
        && v.split('.').all(|p| !p.is_empty());
    if !ok {
        return Err(format!("版本号非法：{v}"));
    }
    Ok(v.to_string())
}

pub fn project_dir(id: &str) -> Result<PathBuf, String> {
    Ok(projects_dir()?.join(safe_id(id)?))
}

pub fn project_toml(id: &str) -> Result<PathBuf, String> {
    Ok(project_dir(id)?.join("frpc.toml"))
}

pub fn project_log(id: &str) -> Result<PathBuf, String> {
    Ok(project_dir(id)?.join("frpc.log"))
}

pub fn service_exe() -> Result<PathBuf, String> {
    base_dir()?.canonicalize().map_err(|e| format!("解析程序目录失败：{e}"))
}

/// 某个版本下 frpc.exe 的路径
pub fn frpc_exe(version: &str) -> Result<PathBuf, String> {
    Ok(versions_dir()?.join(safe_ver(version)?).join("frpc.exe"))
}
