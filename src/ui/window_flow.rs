//! Controles de uma janela sem moldura do sistema.
//!
//! Com `no-frame`, arrastar, minimizar, maximizar, fechar e redimensionar
//! deixam de ser do gerenciador de janelas e passam a ser responsabilidade da
//! aplicação. As bordas de redimensionamento vivem em `ui/layout/resize-edges`.

use std::cell::Cell;

use slint::{ComponentHandle, LogicalPosition, LogicalSize};

use super::AppWindow;

/// Abaixo disto a barra de título e a sidebar deixam de caber.
const MIN_WIDTH: f32 = 880.0;
const MIN_HEIGHT: f32 = 560.0;

/// Geometria em coordenadas lógicas.
#[derive(Clone, Copy, Default)]
struct Frame {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

thread_local! {
    /// Geometria pretendida enquanto o gesto acontece.
    ///
    /// Ela é mantida aqui em vez de relida da janela a cada evento: `set_size`
    /// e `set_position` não têm efeito imediato, então reler logo em seguida
    /// devolvia o valor antigo e o deslocamento entrava duas vezes — era o que
    /// fazia o redimensionamento tremer.
    static GESTURE: Cell<Option<Frame>> = const { Cell::new(None) };
}

pub fn bind(window: &AppWindow) {
    bind_move(window);
    bind_resize(window);
    bind_buttons(window);
}

fn bind_move(window: &AppWindow) {
    let handle = window.as_weak();
    window.on_window_drag_started(move || {
        if let Some(window) = handle.upgrade() {
            GESTURE.with(|slot| slot.set(Some(frame_of(&window))));
        }
    });

    let handle = window.as_weak();
    window.on_window_dragged(move |dx, dy| {
        let Some(window) = handle.upgrade() else {
            return;
        };

        // Uma janela maximizada volta ao tamanho normal ao ser arrastada, como
        // manda o comportamento habitual do sistema.
        if window.window().is_maximized() {
            window.window().set_maximized(false);
            window.set_window_maximized(false);
            GESTURE.with(|slot| slot.set(Some(frame_of(&window))));
            return;
        }

        let Some(mut frame) = GESTURE.with(|slot| slot.get()) else {
            return;
        };

        frame.x += dx;
        frame.y += dy;
        window
            .window()
            .set_position(LogicalPosition::new(frame.x, frame.y));
        GESTURE.with(|slot| slot.set(Some(frame)));
    });
}

fn bind_resize(window: &AppWindow) {
    let handle = window.as_weak();
    window.on_window_resize_started(move || {
        if let Some(window) = handle.upgrade() {
            GESTURE.with(|slot| slot.set(Some(frame_of(&window))));
        }
    });

    let handle = window.as_weak();
    window.on_window_resized(move |dx, dy, from_left, from_top| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some(frame) = GESTURE.with(|slot| slot.get()) else {
            return;
        };

        let next = resize(frame, dx, dy, from_left, from_top);

        window
            .window()
            .set_size(LogicalSize::new(next.width, next.height));
        if from_left || from_top {
            window
                .window()
                .set_position(LogicalPosition::new(next.x, next.y));
        }
        GESTURE.with(|slot| slot.set(Some(next)));
    });
}

fn bind_buttons(window: &AppWindow) {
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

/// Centraliza a janela na tela. Chamado uma vez na abertura, para o aplicativo
/// não nascer no canto ou fora da área visível.
pub fn center(window: &AppWindow) {
    let size = window.window().size();
    let scale = window.window().scale_factor();
    let width = size.width as f32 / scale;
    let height = size.height as f32 / scale;

    // Sem API de tamanho de tela no Slint, a referência é a resolução lógica
    // mais comum; a posição é corrigida pelo gerenciador se não couber.
    let position = LogicalPosition::new(
        ((1920.0 / scale) - width).max(0.0) / 2.0,
        ((1080.0 / scale) - height).max(0.0) / 2.0,
    );
    window.window().set_position(position);
}

fn frame_of(window: &AppWindow) -> Frame {
    let handle = window.window();
    let scale = handle.scale_factor();
    let position = handle.position();
    let size = handle.size();

    Frame {
        x: position.x as f32 / scale,
        y: position.y as f32 / scale,
        width: size.width as f32 / scale,
        height: size.height as f32 / scale,
    }
}

/// Aplica o deslocamento de uma borda. Arrastar a esquerda ou o topo encolhe no
/// sentido inverso e move a janela junto; o limite mínimo é respeitado antes do
/// deslocamento, senão a janela continuaria andando depois de parar de encolher.
fn resize(frame: Frame, dx: f32, dy: f32, from_left: bool, from_top: bool) -> Frame {
    let width = if from_left {
        frame.width - dx
    } else {
        frame.width + dx
    }
    .max(MIN_WIDTH);

    let height = if from_top {
        frame.height - dy
    } else {
        frame.height + dy
    }
    .max(MIN_HEIGHT);

    Frame {
        x: frame.x + if from_left { frame.width - width } else { 0.0 },
        y: frame.y + if from_top { frame.height - height } else { 0.0 },
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        Frame {
            x: 100.0,
            y: 100.0,
            width: 1000.0,
            height: 700.0,
        }
    }

    #[test]
    fn dragging_the_right_edge_only_changes_the_width() {
        let next = resize(frame(), 50.0, 0.0, false, false);
        assert_eq!(next.width, 1050.0);
        assert_eq!(next.x, 100.0, "a janela não deve andar");
        assert_eq!(next.height, 700.0);
    }

    #[test]
    fn dragging_the_left_edge_moves_and_resizes_together() {
        let next = resize(frame(), 50.0, 0.0, true, false);
        assert_eq!(next.width, 950.0);
        assert_eq!(next.x, 150.0, "a borda esquerda acompanha o ponteiro");
    }

    #[test]
    fn the_minimum_stops_the_window_from_sliding() {
        // Encolher muito além do mínimo pela esquerda: a largura trava e a
        // posição precisa travar junto.
        let next = resize(frame(), 5000.0, 0.0, true, false);
        assert_eq!(next.width, MIN_WIDTH);
        assert_eq!(next.x, 100.0 + (1000.0 - MIN_WIDTH));

        let further = resize(next, 5000.0, 0.0, true, false);
        assert_eq!(further.width, MIN_WIDTH);
        assert_eq!(further.x, next.x, "no mínimo a janela para de andar");
    }

    #[test]
    fn the_top_edge_behaves_like_the_left_one() {
        let next = resize(frame(), 0.0, 40.0, false, true);
        assert_eq!(next.height, 660.0);
        assert_eq!(next.y, 140.0);
        assert_eq!(next.x, 100.0);
    }

    #[test]
    fn the_bottom_edge_never_moves_the_window() {
        let next = resize(frame(), 0.0, -5000.0, false, false);
        assert_eq!(next.height, MIN_HEIGHT);
        assert_eq!(next.y, 100.0);
    }
}
