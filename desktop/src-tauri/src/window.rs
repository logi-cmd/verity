use tauri::{LogicalSize, Manager};

fn main_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Main window is not available.".to_string())
}

#[tauri::command]
pub(crate) fn window_minimize(app: tauri::AppHandle) -> Result<(), String> {
    let window = main_window(&app)?;
    window
        .minimize()
        .map_err(|error| format!("Unable to minimize window: {error}"))
}

#[tauri::command]
pub(crate) fn window_toggle_maximize(app: tauri::AppHandle) -> Result<bool, String> {
    let window = main_window(&app)?;
    if window
        .is_maximized()
        .map_err(|error| format!("Unable to read window state: {error}"))?
    {
        window
            .unmaximize()
            .map_err(|error| format!("Unable to restore window: {error}"))?;
        Ok(false)
    } else {
        window
            .maximize()
            .map_err(|error| format!("Unable to maximize window: {error}"))?;
        Ok(true)
    }
}

#[tauri::command]
pub(crate) fn window_is_maximized(app: tauri::AppHandle) -> Result<bool, String> {
    main_window(&app)?
        .is_maximized()
        .map_err(|error| format!("Unable to read window state: {error}"))
}

#[tauri::command]
pub(crate) fn window_restore(app: tauri::AppHandle) -> Result<(), String> {
    let window = main_window(&app)?;
    window
        .unmaximize()
        .map_err(|error| format!("Unable to restore window: {error}"))
}

#[tauri::command]
pub(crate) fn window_maximize(app: tauri::AppHandle) -> Result<(), String> {
    let window = main_window(&app)?;
    window
        .maximize()
        .map_err(|error| format!("Unable to maximize window: {error}"))
}

#[tauri::command]
pub(crate) fn window_close(app: tauri::AppHandle) -> Result<(), String> {
    let window = main_window(&app)?;
    window
        .close()
        .map_err(|error| format!("Unable to close window: {error}"))
}

pub(crate) fn center_main_window_for_startup(app: &tauri::AppHandle) -> Result<(), String> {
    let window = main_window(app)?;
    let (min_width, min_height, width, height) = (1100.0, 640.0, 1280.0, 820.0);
    window
        .set_min_size(Some(LogicalSize::new(min_width, min_height)))
        .map_err(|error| format!("Unable to update window minimum size: {error}"))?;
    if !window
        .is_maximized()
        .map_err(|error| format!("Unable to read window state: {error}"))?
    {
        window
            .set_size(LogicalSize::new(width, height))
            .map_err(|error| format!("Unable to update window size: {error}"))?;
        window
            .center()
            .map_err(|error| format!("Unable to center window: {error}"))?;
    }
    window
        .show()
        .map_err(|error| format!("Unable to show window: {error}"))?;
    window
        .set_focus()
        .map_err(|error| format!("Unable to focus window: {error}"))?;
    Ok(())
}
