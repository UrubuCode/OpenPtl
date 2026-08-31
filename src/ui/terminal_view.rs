//! Estado de apresentação das sessões de terminal.
//!
//! Mantém um emulador por sessão, drena a saída num temporizador e traduz a
//! grade para o modelo que `TerminalView` desenha. A drenagem acontece fora da
//! thread de desenho; aqui só chega texto já pronto.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};

use super::keymap::{self, Modifiers};
use super::{AppWindow, SessionRow, TermLine, TermSpan};
use crate::backend::Backend;
use crate::libs::terminal::{Color, Span, Terminal};

/// Intervalo de drenagem. Curto o bastante para o eco da digitação parecer
/// imediato, longo o bastante para não acordar o executor à toa.
const POLL_INTERVAL: Duration = Duration::from_millis(40);

/// Índice usado no modelo Slint para "sem cor própria, use a do tema".
const THEME_COLOR: i32 = -1;

#[derive(Default)]
struct Sessions {
    emulators: HashMap<String, Terminal>,
}

pub fn bind(window: &AppWindow, backend: Arc<Backend>) -> Timer {
    let sessions = Arc::new(Mutex::new(Sessions::default()));

    bind_selection(window, Arc::clone(&sessions), Arc::clone(&backend));
    bind_resize(window, Arc::clone(&sessions), Arc::clone(&backend));
    bind_input(window, Arc::clone(&backend));
    start_polling(window, sessions, backend)
}

fn bind_selection(window: &AppWindow, sessions: Arc<Mutex<Sessions>>, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_session_selected(move |id| {
        if let Some(window) = handle.upgrade() {
            window.set_active_session(id);
        }
    });

    let handle = window.as_weak();
    window.on_session_closed(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        backend.disconnect(&id);
        if let Ok(mut state) = sessions.lock() {
            state.emulators.remove(id.as_str());
        }

        let kept: Vec<SessionRow> = window
            .get_sessions()
            .iter()
            .filter(|row| row.id != id)
            .collect();
        let next = kept.first().map(|row| row.id.clone()).unwrap_or_default();
        window.set_sessions(ModelRc::new(VecModel::from(kept)));
        window.set_active_session(next);
    });
}

fn bind_input(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_terminal_input_sent(move |text, control, alt| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let session = window.get_active_session();
        if session.is_empty() {
            return;
        }

        let bytes = keymap::to_bytes(&text, Modifiers { control, alt });
        if !bytes.is_empty() {
            backend.send_input(&session, bytes);
        }
    });
}

fn start_polling(
    window: &AppWindow,
    sessions: Arc<Mutex<Sessions>>,
    backend: Arc<Backend>,
) -> Timer {
    let timer = Timer::default();
    let handle = window.as_weak();

    timer.start(TimerMode::Repeated, POLL_INTERVAL, move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let session = window.get_active_session();
        if session.is_empty() {
            return;
        }

        let deliver = handle.clone();
        let target = session.to_string();
        let emulators = Arc::clone(&sessions);

        backend.poll_output(&session, move |output| {
            let Ok(chunk) = output else {
                return;
            };
            if chunk.is_empty() {
                return;
            }
            let _ = deliver.upgrade_in_event_loop(move |window| {
                let Ok(mut state) = emulators.lock() else {
                    return;
                };
                let terminal = state.emulators.entry(target).or_default();
                terminal.feed(chunk.as_bytes());
                window.set_terminal_lines(to_model(terminal));
            });
        });
    });

    timer
}

fn to_model(terminal: &Terminal) -> ModelRc<TermLine> {
    let lines: Vec<TermLine> = terminal
        .visible_spans()
        .iter()
        .map(|spans| TermLine {
            spans: ModelRc::new(VecModel::from(
                spans.iter().map(to_span).collect::<Vec<_>>(),
            )),
        })
        .collect();

    ModelRc::new(VecModel::from(lines))
}

fn to_span(span: &Span) -> TermSpan {
    TermSpan {
        text: span.text.as_str().into(),
        foreground: to_index(span.style.foreground),
        background: to_index(span.style.background),
        bold: span.style.bold,
        inverse: span.style.inverse,
        // Contagem em celulas, nao em bytes: acentos e caracteres de caixa
        // ocupam uma celula cada, mas mais de um byte.
        cells: span.text.chars().count() as i32,
    }
}

/// A paleta da interface tem 16 entradas; cores fora dela caem no tema em vez
/// de virarem um índice inválido.
fn to_index(color: Color) -> i32 {
    match color {
        Color::Indexed(index) if index < 16 => index as i32,
        _ => THEME_COLOR,
    }
}

/// Largura e altura aproximadas de uma célula com a fonte monoespaçada do tema.
/// Servem para converter a área do bloco em colunas e linhas.
const CELL_WIDTH: f32 = 7.0;
const CELL_HEIGHT: f32 = 17.0;
/// Menor grade utilizável; abaixo disso o shell remoto começa a se atrapalhar.
const MIN_COLUMNS: usize = 20;
const MIN_ROWS: usize = 4;

/// Converte a área disponível em colunas e linhas.
fn grid_for(width: f32, height: f32) -> (usize, usize) {
    let columns = ((width - 2.0 * PADDING) / CELL_WIDTH).floor().max(0.0) as usize;
    let rows = ((height - 2.0 * PADDING) / CELL_HEIGHT).floor().max(0.0) as usize;
    (columns.max(MIN_COLUMNS), rows.max(MIN_ROWS))
}

/// Recuo interno da superfície, igual ao do componente que a desenha.
const PADDING: f32 = 12.0;

/// O bloco mudou de tamanho: o emulador e o servidor precisam saber, senão o
/// conteúdo remoto continua desenhando para a grade antiga.
fn bind_resize(window: &AppWindow, sessions: Arc<Mutex<Sessions>>, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_terminal_resized(move |session, width, height| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        if session.is_empty() {
            return;
        }

        let (columns, rows) = grid_for(width, height);

        let Ok(mut state) = sessions.lock() else {
            return;
        };
        let terminal = state.emulators.entry(session.to_string()).or_default();
        if terminal.size().columns == columns && terminal.size().rows == rows {
            return;
        }

        terminal.resize(columns, rows);
        window.set_terminal_lines(to_model(terminal));
        drop(state);

        backend.resize_pty(&session, columns as u32, rows as u32);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wider_block_gets_more_columns() {
        let (narrow, _) = grid_for(400.0, 300.0);
        let (wide, _) = grid_for(800.0, 300.0);
        assert!(wide > narrow);
    }

    #[test]
    fn a_taller_block_gets_more_rows() {
        let (_, short) = grid_for(600.0, 200.0);
        let (_, tall) = grid_for(600.0, 600.0);
        assert!(tall > short);
    }

    #[test]
    fn a_tiny_block_still_reports_a_usable_grid() {
        let (columns, rows) = grid_for(10.0, 10.0);
        assert_eq!(columns, MIN_COLUMNS);
        assert_eq!(rows, MIN_ROWS);
    }

    #[test]
    fn the_grid_never_reports_zero() {
        let (columns, rows) = grid_for(0.0, 0.0);
        assert!(columns > 0 && rows > 0);
    }
}
