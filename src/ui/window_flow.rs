//! Controles de uma janela sem moldura do sistema.
//!
//! Com `no-frame`, arrastar, minimizar, maximizar, fechar e **redimensionar**
//! deixam de ser do gerenciador de janelas e passam a ser responsabilidade da
//! aplicação. As bordas de redimensionamento vivem em `ui/layout/resize-edges`.

use std::cell::Cell;

use slint::{ComponentHandle, LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};

use super::AppWindow;

/// Abaixo disto a barra de título e a sidebar deixam de caber.
const MIN_WIDTH: f32 = 880.0;
const MIN_HEIGHT: f32 = 560.0;

thread_local! {
    /// Posição da janela quando o arrasto começou. O deslocamento reportado
    /// pelo Slint é relativo ao ponto de pressão, então precisa de uma âncora.
    static DRAG_ORIGIN: Cell<Option<PhysicalPosition>> = const { Cell::new(None) };
    /// Posição e tamanho no instante em que o redimensionamento começou.
    static RESIZE_ORIGIN: Cell<Option<(PhysicalPosition, PhysicalSize)>> =
        const { Cell::new(None) };
}

fn bind_resize(window: &AppWindow) {
    let handle = window.as_weak();
    window.on_window_resize_started(move || {
        if let Some(window) = handle.upgrade() {
            let window = window.window();
            RESIZE_ORIGIN.with(|slot| slot.set(Some((window.position(), window.size()))));
        }
    });

    let handle = window.as_weak();
    window.on_window_resized(move |dx, dy, from_left, from_top| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some((origin, size)) = RESIZE_ORIGIN.with(|slot| slot.get()) else {
            return;
        };

        let scale = window.window().scale_factor();
        let width = size.width as f32 / scale;
        let height = size.height as f32 / scale;

        // Arrastar a borda esquerda ou o topo encolhe o lado oposto: o tamanho
        // muda no sentido inverso do deslocamento e a janela anda junto.
        let next_width = (if from_left { width - dx } else { width + dx }).max(MIN_WIDTH);
        let next_height = (if from_top { height - dy } else { height + dy }).max(MIN_HEIGHT);

        // O limite mínimo é aplicado antes de mover: sem isso a janela
        // continuaria deslizando depois de parar de encolher.
        let left = origin.x as f32 / scale + if from_left { width - next_width } else { 0.0 };
        let top = origin.y as f32 / scale + if from_top { height - next_height } else { 0.0 };

        window
            .window()
            .set_size(LogicalSize::new(next_width, next_height));
        if from_left || from_top {
            window
                .window()
                .set_position(LogicalPosition::new(left, top));
        }

        let window = window.window();
        RESIZE_ORIGIN.with(|slot| slot.set(Some((window.position(), window.size()))));
    });
}

pub fn bind(window: &AppWindow) {
    bind_resize(window);

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
