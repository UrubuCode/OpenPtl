// Mobile (Android/iOS) stub for the desktop global-input-capture service.
//
// The real `key_actions` module relies on `rdev` to hook OS-wide keyboard/mouse
// events so the SSH terminal can receive input while a native surface has focus.
// `rdev` has no Android/iOS backend, and global capture is meaningless on mobile
// (the WebView already owns all input). This stub mirrors the public API as
// no-ops so the rest of the backend compiles unchanged; it reports a "disabled"
// status to the frontend.
use tauri::{AppHandle, Emitter};

const STATUS_EVENT: &str = "key_actions:status";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyActionsStatusKind {
    Ready,
    Disabled,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct KeyActionsStatusPayload {
    pub status: KeyActionsStatusKind,
    pub reason: Option<String>,
    pub platform: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SurfaceRectInput {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

// Kept byte-for-byte compatible with the desktop enum so the Tauri command
// deserializes identical payloads from the frontend.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KeyActionsActiveTargetInput {
    Ssh {
        session_id: String,
        tab_id: String,
        block_id: String,
        surface_rect: SurfaceRectInput,
        #[serde(default)]
        dpi_scale: Option<f64>,
        cols: u16,
        rows: u16,
    },
}

#[derive(Default)]
pub struct KeyActionsService;

impl KeyActionsService {
    pub fn new() -> Self {
        Self
    }

    pub fn start(&self, app: AppHandle) {
        self.emit_status(&app);
    }

    pub fn set_active_target(
        &self,
        _target: Option<KeyActionsActiveTargetInput>,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn set_window_focused(&self, _focused: bool) {}

    pub fn set_window_origin(&self, _x: f64, _y: f64) {}

    pub fn emit_status(&self, app: &AppHandle) {
        let payload = KeyActionsStatusPayload {
            status: KeyActionsStatusKind::Disabled,
            reason: Some("unsupported_platform".to_string()),
            platform: std::env::consts::OS.to_string(),
            details: Some("Captura global indisponivel no mobile".to_string()),
        };
        let _ = app.emit(STATUS_EVENT, payload);
    }
}
