pub mod app;
pub mod contracts;
pub mod cursor;
pub mod db;
pub mod executor;
pub mod intake;
pub mod packaging;
pub mod planner;
pub mod process;
pub mod recovery;
pub mod scheduler;
pub mod security;
pub mod verification;
pub mod workspace;

use app::commands::{self, AppState};
use app::promote_commands;
use std::sync::Mutex;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Fail second desktop instance cleanly before opening DB / scheduling work.
    let _single_instance = match app::single_instance::acquire_single_instance_mutex() {
        Ok(guard) => guard,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let store = commands::init_store(app.handle())?;
            let _ = crate::db::ensure_demo_run(&store);
            // Full startup recovery before accepting new work (includes process reconcile).
            let recovery = crate::recovery::run_startup_recovery(&store, None)
                .map_err(|e| format!("startup recovery failed: {e}"))?;
            if let Some(ref offer) = recovery.offer {
                eprintln!(
                    "Tiamat startup recovery offer: {} (resume_allowed={})",
                    offer.reason, offer.resume_allowed
                );
            }
            if recovery
                .process_reconcile
                .as_ref()
                .map(|r| r.hard_failure)
                .unwrap_or(false)
            {
                eprintln!(
                    "Tiamat startup cleanup hard failure: {:?}",
                    recovery.messages
                );
            }

            let mut abort_settings = store
                .get_abort_settings()
                .map_err(|e| e.to_string())?;

            let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::F12);
            let registered = match app.global_shortcut().on_shortcut(shortcut, {
                let handle = app.handle().clone();
                move |_app, _shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    let state = handle.state::<AppState>();
                    let store = match state.store.lock() {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let run_id = store.list_runs().ok().and_then(|runs| {
                        runs.into_iter()
                            .find(|r| {
                                !matches!(
                                    r.status.as_str(),
                                    "completed" | "failed" | "cancelled" | "created"
                                )
                            })
                            .map(|r| r.run_id)
                    });
                    let active = run_id.is_some() || state.process_host.active_live_count() > 0;
                    if let Ok(result) = state.abort.handle_press(
                        &store,
                        &state.process_host,
                        run_id,
                        active,
                        false,
                    ) {
                        drop(store);
                        let _ = handle.emit("tiamat://abort", result);
                    }
                }
            }) {
                Ok(()) => true,
                Err(err) => {
                    abort_settings.degraded = true;
                    abort_settings.registered = false;
                    abort_settings.collision_reason = Some(format!("shortcut registration failed: {err}"));
                    abort_settings.updated_at_utc = chrono::Utc::now().to_rfc3339();
                    let _ = store.save_abort_settings(&abort_settings);
                    false
                }
            };

            if registered {
                let _ = crate::process::mark_shortcut_registered(&store, true, None);
            }

            // Tray fallback for emergency stop when shortcut is degraded or window unfocused.
            let tray_ok = setup_tray(app.handle()).is_ok();
            let recovery_for_state = recovery.clone();
            app.manage(AppState {
                store: Mutex::new(store),
                last_preflight: Mutex::new(None),
                last_cursor: Mutex::new(None),
                last_workspace: Mutex::new(None),
                last_plan: Mutex::new(None),
                last_architect: Mutex::new(None),
                last_scheduler: Mutex::new(None),
                last_executor: Mutex::new(None),
                last_recovery: Mutex::new(Some(recovery_for_state)),
                workspace_parent: Mutex::new(None),
                process_host: crate::process::ProcessHost::new(),
                abort: {
                    let ctrl = crate::process::AbortController::new();
                    ctrl.set_tray_available(tray_ok);
                    ctrl
                },
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.abort.keep_running() {
                    return;
                }
                let store = match state.store.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let has_active = state.process_host.active_live_count() > 0
                    || store.active_process_count(None).unwrap_or(0) > 0;
                drop(store);
                if has_active {
                    api.prevent_close();
                    let _ = window.emit(
                        "tiamat://close-policy",
                        serde_json::json!({
                            "message": "Active work detected. Choose Keep Tiamat running or Stop all and exit.",
                            "choices": ["keep_running", "stop_all_and_exit"]
                        }),
                    );
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::validate_contract_json,
            commands::orchestrator_status,
            commands::ensure_demo_run,
            commands::list_runs,
            commands::replay_events,
            commands::list_artifacts,
            commands::transition_run_status,
            commands::pick_intake_paths,
            commands::run_intake_preflight,
            commands::confirm_intake_trust,
            commands::get_intake_preflight,
            commands::probe_cursor_capability,
            commands::get_cursor_capability,
            commands::get_app_settings,
            commands::set_cursor_cli_path,
            commands::list_cursor_models,
            commands::preview_cursor_command,
            commands::materialize_workspace,
            commands::get_workspace_manifest,
            commands::validate_workspace_roots,
            commands::create_workspace_checkpoint,
            commands::run_architect_pipeline,
            commands::get_project_plan,
            commands::get_graph_projection,
            commands::get_architect_result,
            commands::start_scheduler,
            commands::scheduler_tick,
            commands::scheduler_complete_attempt,
            commands::scheduler_pause,
            commands::scheduler_resume,
            commands::get_scheduler_snapshot,
            commands::emergency_abort,
            commands::get_process_registry,
            commands::get_abort_settings,
            commands::acknowledge_degraded_abort,
            commands::rebind_abort_shortcut,
            commands::apply_close_policy,
            commands::reconcile_processes,
            commands::run_process_fixture,
            commands::execute_phase_fixture,
            commands::get_executor_outcome,
            commands::seed_perf_events,
            commands::emit_event_burst,
            commands::export_run_report,
            commands::scheduler_retry_phase,
            commands::open_run_output,
            commands::run_startup_recovery,
            commands::get_recovery_offer,
            commands::recovery_resume,
            commands::recovery_cancel,
            commands::probe_disk_pressure,
            commands::set_fault_injection,
            commands::clear_fault_injection,
            commands::run_fault_injection_fixture,
            commands::get_retention_settings,
            commands::cleanup_managed_workspace,
            commands::scan_prompt_injection,
            commands::redact_text,
            commands::apply_output_limits_fixture,
            commands::plan_uninstall_retention,
            commands::simulate_upgrade_preserve,
            commands::create_long_path_fixture,
            commands::prove_packaged_cleanup,
            commands::materialize_testbench,
            promote_commands::export_workspace_project,
            promote_commands::promote_workspace,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let emergency = MenuItemBuilder::with_id("emergency_stop", "Emergency stop")
        .build(app)
        .map_err(|e| e.to_string())?;
    let show = MenuItemBuilder::with_id("show", "Show Tiamat")
        .build(app)
        .map_err(|e| e.to_string())?;
    let menu = MenuBuilder::new(app)
        .item(&emergency)
        .item(&show)
        .build()
        .map_err(|e| e.to_string())?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Tiamat")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "emergency_stop" => {
                let state = app.state::<AppState>();
                let store = match state.store.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let run_id = store
                    .list_runs()
                    .ok()
                    .and_then(|runs| runs.into_iter().next().map(|r| r.run_id));
                let active = state.process_host.active_live_count() > 0;
                if let Ok(result) =
                    state
                        .abort
                        .handle_press(&store, &state.process_host, run_id, active, false)
                {
                    drop(store);
                    let _ = app.emit("tiamat://abort", result);
                }
            }
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)
        .map_err(|e| e.to_string())?;
    Ok(())
}
