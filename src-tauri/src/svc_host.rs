//! 服务宿主：本程序以 `--service-host` 启动时作为 Windows 服务运行，
//! 负责拉起 frpc 子进程、把输出写进日志、收到停止指令时结束子进程。
#![cfg(windows)]

use std::ffi::OsString;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

define_windows_service!(ffi_service_main, service_main);

fn arg(key: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// 入口：注册服务分发器（阻塞直到服务停止）
pub fn run_host() {
    let name = arg("--name").unwrap_or_else(|| "FRP Tunnel · Service".into());
    if let Err(e) = service_dispatcher::start(&name, ffi_service_main) {
        let _ = std::fs::write(
            std::env::temp_dir().join("frp-service-host-error.log"),
            format!("start dispatcher failed: {e:?}"),
        );
    }
}

fn service_main(_arguments: Vec<OsString>) {
    let _ = run_service();
}

fn status(running: bool, exit: u32) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: if running {
            ServiceState::Running
        } else {
            ServiceState::Stopped
        },
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(exit),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

fn run_service() -> Result<(), String> {
    let name = arg("--name").unwrap_or_else(|| "FRP Tunnel · Service".into());
    let frpc = arg("--frpc").ok_or("缺少 --frpc")?;
    let config = arg("--config").ok_or("缺少 --config")?;
    let log_path = arg("--log").unwrap_or_default();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_handler = stop.clone();
    let handler = move |ev: ServiceControl| -> ServiceControlHandlerResult {
        match ev {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                stop_handler.store(true, Ordering::SeqCst);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };
    let handle = service_control_handler::register(&name, handler)
        .map_err(|e| format!("注册服务控制失败：{e:?}"))?;
    handle
        .set_service_status(status(true, 0))
        .map_err(|e| format!("上报运行状态失败：{e:?}"))?;

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();
    let mut cmd = Command::new(&frpc);
    cmd.arg("-c")
        .arg(&config)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(f) = log_file {
        if let Ok(f2) = f.try_clone() {
            cmd.stdout(Stdio::from(f2));
            cmd.stderr(Stdio::from(f));
        }
    }
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd.spawn().map_err(|e| format!("启动 frpc 失败：{e}"))?;
    let mut code: u32 = 0;
    loop {
        if stop.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        match child.try_wait() {
            Ok(Some(st)) => {
                code = st.code().unwrap_or(0) as u32;
                break;
            }
            Ok(None) => {}
            Err(_) => break,
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = handle.set_service_status(status(false, code));
    Ok(())
}
