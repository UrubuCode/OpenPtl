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
/// Índice que manda a interface usar o RGB que vem junto, em vez da paleta.
const DIRECT_COLOR: i32 = -2;

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
    let (foreground, foreground_rgb) = resolve(span.style.foreground);
    let (background, background_rgb) = resolve(span.style.background);

    TermSpan {
        text: span.text.as_str().into(),
        foreground,
        foreground_rgb,
        background,
        background_rgb,
        bold: span.style.bold,
        inverse: span.style.inverse,
        // Largura em colunas, calculada pelo emulador: um glifo largo vale
        // duas, e contar caracteres desalinhava o resto da linha.
        cells: span.cells as i32,
    }
}

/// Traduz a cor pedida pelo servidor para o que a interface sabe desenhar.
///
/// As 16 primeiras entradas continuam saindo pela paleta do tema, para o
/// terminal não destoar do resto da interface. O resto — o cubo de 256 e o
/// truecolor — vira RGB direto. Antes tudo acima de 15 caía na cor padrão, e
/// era por isso que programas como o `btop`, que usam a paleta estendida quase
/// inteira, apareciam sem cor nenhuma.
fn resolve(color: Color) -> (i32, slint::Color) {
    match color {
        Color::Default => (THEME_COLOR, slint::Color::default()),
        Color::Indexed(index) if index < 16 => (index as i32, slint::Color::default()),
        Color::Indexed(index) => {
            let (r, g, b) = xterm_256(index);
            (DIRECT_COLOR, slint::Color::from_rgb_u8(r, g, b))
        }
        Color::Rgb(r, g, b) => (DIRECT_COLOR, slint::Color::from_rgb_u8(r, g, b)),
    }
}

/// Cor de um índice da paleta estendida do xterm.
///
/// 16..=231 formam um cubo 6x6x6 e 232..=255 uma rampa de cinzas. Índices
/// abaixo de 16 não chegam aqui: são resolvidos pelo tema.
fn xterm_256(index: u8) -> (u8, u8, u8) {
    if index >= 232 {
        let level = 8 + (index as u16 - 232) * 10;
        let level = level.min(255) as u8;
        return (level, level, level);
    }

    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let value = index as usize - 16;
    (
        STEPS[(value / 36) % 6],
        STEPS[(value / 6) % 6],
        STEPS[value % 6],
    )
}

/// Menor grade utilizável; abaixo disso o shell remoto começa a se atrapalhar.
const MIN_COLUMNS: usize = 20;
const MIN_ROWS: usize = 4;

/// Converte a área disponível em colunas e linhas.
///
/// O tamanho da célula vem medido da interface, não de uma constante: só lá se
/// sabe qual fonte o sistema entregou de fato, e um avanço chutado desalinhava
/// a grade progressivamente ao longo da linha.
fn grid_for(width: f32, height: f32, cell_width: f32, cell_height: f32) -> (usize, usize) {
    if cell_width <= 0.0 || cell_height <= 0.0 {
        return (MIN_COLUMNS, MIN_ROWS);
    }

    let columns = ((width - 2.0 * PADDING) / cell_width).floor().max(0.0) as usize;
    let rows = ((height - 2.0 * PADDING) / cell_height).floor().max(0.0) as usize;
    (columns.max(MIN_COLUMNS), rows.max(MIN_ROWS))
}

/// Recuo interno da superfície, igual ao do componente que a desenha.
const PADDING: f32 = 12.0;

/// O bloco mudou de tamanho: o emulador e o servidor precisam saber, senão o
/// conteúdo remoto continua desenhando para a grade antiga.
fn bind_resize(window: &AppWindow, sessions: Arc<Mutex<Sessions>>, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_terminal_resized(move |session, width, height, cell_width, cell_height| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        if session.is_empty() {
            return;
        }

        let (columns, rows) = grid_for(width, height, cell_width, cell_height);

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

    /// Métricas típicas de uma monoespaçada de 13px.
    const CELL: (f32, f32) = (7.15, 17.0);

    fn grid(width: f32, height: f32) -> (usize, usize) {
        grid_for(width, height, CELL.0, CELL.1)
    }

    #[test]
    fn a_wider_block_gets_more_columns() {
        let (narrow, _) = grid(400.0, 300.0);
        let (wide, _) = grid(800.0, 300.0);
        assert!(wide > narrow);
    }

    #[test]
    fn a_taller_block_gets_more_rows() {
        let (_, short) = grid(600.0, 200.0);
        let (_, tall) = grid(600.0, 600.0);
        assert!(tall > short);
    }

    #[test]
    fn a_tiny_block_still_reports_a_usable_grid() {
        let (columns, rows) = grid(10.0, 10.0);
        assert_eq!(columns, MIN_COLUMNS);
        assert_eq!(rows, MIN_ROWS);
    }

    #[test]
    fn the_grid_never_reports_zero() {
        let (columns, rows) = grid(0.0, 0.0);
        assert!(columns > 0 && rows > 0);
    }

    #[test]
    fn a_cell_that_was_never_measured_does_not_divide_by_zero() {
        let (columns, rows) = grid_for(800.0, 600.0, 0.0, 0.0);
        assert_eq!((columns, rows), (MIN_COLUMNS, MIN_ROWS));
    }

    #[test]
    fn a_narrower_cell_fits_more_columns() {
        let (wide_cell, _) = grid_for(800.0, 400.0, 10.0, 17.0);
        let (narrow_cell, _) = grid_for(800.0, 400.0, 7.0, 17.0);
        assert!(narrow_cell > wide_cell);
    }

    #[test]
    fn the_first_sixteen_colors_still_come_from_the_theme_palette() {
        let (index, _) = resolve(Color::Indexed(4));
        assert_eq!(index, 4, "a paleta do tema continua mandando em 0..15");
    }

    #[test]
    fn the_extended_palette_is_resolved_instead_of_discarded() {
        // Era aqui que o btop perdia a cor: tudo acima de 15 virava a cor
        // padrao do tema.
        let (index, rgb) = resolve(Color::Indexed(196));
        assert_eq!(index, DIRECT_COLOR);
        assert_eq!((rgb.red(), rgb.green(), rgb.blue()), (255, 0, 0));
    }

    #[test]
    fn truecolor_reaches_the_screen_unchanged() {
        let (index, rgb) = resolve(Color::Rgb(18, 52, 86));
        assert_eq!(index, DIRECT_COLOR);
        assert_eq!((rgb.red(), rgb.green(), rgb.blue()), (18, 52, 86));
    }

    #[test]
    fn the_grayscale_ramp_stays_gray() {
        for index in 232u8..=255 {
            let (_, rgb) = resolve(Color::Indexed(index));
            assert_eq!(rgb.red(), rgb.green());
            assert_eq!(rgb.green(), rgb.blue());
        }
    }

    #[test]
    fn the_color_cube_covers_its_whole_range() {
        assert_eq!(xterm_256(16), (0, 0, 0));
        assert_eq!(xterm_256(231), (255, 255, 255));
    }

    #[test]
    fn a_default_color_defers_to_the_theme() {
        let (index, _) = resolve(Color::Default);
        assert_eq!(index, THEME_COLOR);
    }
}
