//! Estado de apresentação das sessões de terminal.
//!
//! Mantém um emulador por sessão, drena a saída num temporizador e traduz a
//! grade para o modelo que `TerminalView` desenha. A drenagem acontece fora da
//! thread de desenho; aqui só chega texto já pronto.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};

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
    window.on_terminal_input_sent(move |text| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let session = window.get_active_session();
        if !session.is_empty() && !text.is_empty() {
            backend.send_input(&session, text.as_bytes().to_vec());
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
