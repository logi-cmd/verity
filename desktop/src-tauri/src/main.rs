// SPDX-License-Identifier: MPL-2.0

mod verification;
mod window;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            window::center_main_window_for_startup(app.handle()).map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            verification::pick_repository,
            verification::inspect_repository,
            verification::runtime_doctor,
            verification::start_docker_desktop,
            verification::create_run_session,
            verification::execute_run_session,
            verification::read_run_session,
            verification::cancel_run_session,
            verification::list_receipts,
            verification::read_receipt,
            verification::verify_receipt,
            verification::export_receipt,
            verification::preview_cleanup,
            verification::start_cleanup,
            verification::read_cleanup_session,
            verification::cancel_cleanup,
            verification::list_cleanup_receipts,
            verification::export_cleanup_patch,
            verification::apply_cleanup,
            verification::list_agent_capabilities,
            verification::test_agent_capability,
            verification::launch_agent_desktop,
            verification::start_agent_repair,
            verification::read_agent_repair,
            verification::cancel_agent_repair,
            verification::apply_agent_repair,
            verification::export_agent_patch,
            verification::copy_agent_task,
            verification::export_agent_task_pack,
            verification::preview_diagnostic_report,
            verification::export_diagnostic_report,
            verification::copy_diagnostic_issue_summary,
            verification::save_project_secret,
            verification::has_project_secret,
            verification::delete_project_secret,
            verification::open_external_url,
            window::window_close,
            window::window_minimize,
            window::window_restore,
            window::window_maximize,
            window::window_is_maximized,
            window::window_toggle_maximize,
        ])
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("failed to run Verity");
}
