// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 服务模式：被 SCM 拉起时不创建界面，直接作为服务宿主运行
    if std::env::args().any(|a| a == "--service-host") {
        #[cfg(windows)]
        {
            frp_tunnel_manager_lib::svc_host::run_host();
            return;
        }
    }
    // 自检模式：--verify-service <out.json>，跑服务全生命周期并写报告（需要管理员）
    if let Some(pos) = std::env::args().position(|a| a == "--verify-service") {
        let out = std::env::args()
            .nth(pos + 1)
            .unwrap_or_else(|| "_verify_service.json".into());
        frp_tunnel_manager_lib::verify_service(&out);
        return;
    }
    frp_tunnel_manager_lib::run()
}
