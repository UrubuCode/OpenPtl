//! Controles de uma janela sem moldura do sistema.
//!
//! Com `no-frame`, arrastar, minimizar, maximizar e fechar deixam de ser do
//! gerenciador de janelas e passam a ser responsabilidade da barra de título.

use std::cell::Cell;

use slint::{ComponentHandle, LogicalPosition, PhysicalPosition};

use super::AppWindow;

thread_local! {
    /// Posição da janela quando o arrasto começou. O deslocamento reportado
    /// pelo Slint é relativo ao ponto de pressão, então precisa de uma âncora.
    static DRAG_ORIGIN: Cell<Option<PhysicalPosition>> = const { Cell::new(None) };
}

pub fn bind(window: &AppWindow) {
    let handle = window.as_weak();
    window.on_window_drag_started(move || {
        if let Some(window) = handle.upgrade() {
            DRAG_ORIGIN.with(|origin| origin.set(Some(window.window().position())));
        }
    });

    let handle = window.as_weak();
    window.on_window_dragged(move |dx, dy| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some(origin) = DRAG_ORIGIN.with(|origin| origin.get()) else {
            return;
        };

        // Uma janela maximizada volta ao tamanho normal ao ser arrastada, como
        // manda o comportamento habitual do sistema.
        if window.window().is_maximized() {
            window.window().set_maximized(false);
            window.set_window_maximized(false);
            DRAG_ORIGIN.with(|slot| slot.set(Some(window.window().position())));
            return;
        }

        let scale = window.window().scale_factor();
        let moved =
            LogicalPosition::new(origin.x as f32 / scale + dx, origin.y as f32 / scale + dy);
        window.window().set_position(moved);
        DRAG_ORIGIN.with(|slot| slot.set(Some(window.window().position())));
    });

    let handle = window.as_weak();
    window.on_window_minimize(move || {
        if let Some(window) = handle.upgrade() {
            window.window().set_minimized(true);
        }
    });

    let handle = window.as_weak();
    window.on_window_maximize_toggle(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let next = !window.window().is_maximized();
        window.window().set_maximized(next);
        window.set_window_maximized(next);
    });

    let handle = window.as_weak();
    window.on_window_close(move || {
        if let Some(window) = handle.upgrade() {
            let _ = window.hide();
        }
    });
}
