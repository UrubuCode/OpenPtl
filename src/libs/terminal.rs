//! Adaptador do emulador de terminal.
//!
//! A emulação em si é do `alacritty_terminal`: ele mantém a grade, o histórico
//! e a interpretação das sequências de escape. Este módulo existe só para
//! traduzir a grade em trechos estilizados que a apresentação sabe desenhar, e
//! para manter `vte` e `alacritty_terminal` fora do resto do código.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Point;
use alacritty_terminal::term::{cell::Flags, Config, Term};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Processor, Rgb};

const DEFAULT_COLUMNS: usize = 120;
const DEFAULT_ROWS: usize = 32;
const SCROLLBACK_LINES: usize = 4000;

/// Cor pedida pelo servidor. A resolução para pixels é da apresentação, que
/// aplica o tema; o domínio guarda apenas a intenção.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub foreground: Color,
    pub background: Color,
    pub bold: bool,
    pub inverse: bool,
}

/// Trecho contíguo de uma linha com o mesmo estilo, pronto para virar um nó de
/// texto na interface. Agrupar aqui evita um elemento por célula na tela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub columns: usize,
    pub rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

pub struct Terminal {
    term: Term<VoidListener>,
    processor: Processor,
    size: Size,
}

impl Terminal {
    pub fn new() -> Self {
        let size = Size {
            columns: DEFAULT_COLUMNS,
            rows: DEFAULT_ROWS,
        };
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Default::default()
        };

        Self {
            term: Term::new(config, &size, VoidListener),
            processor: Processor::new(),
            size,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, columns: usize, rows: usize) {
        self.size = Size {
            columns: columns.max(1),
            rows: rows.max(1),
        };
        self.term.resize(self.size);
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn cursor(&self) -> Point<usize> {
        let point = self.term.grid().cursor.point;
        Point::new(point.line.0.max(0) as usize, point.column)
    }

    /// Linhas visíveis da janela, cada uma já agrupada por estilo. A célula do
    /// cursor sai invertida: é assim que um terminal desenha o bloco, e evita
    /// que a interface precise medir a fonte para posicionar um retângulo.
    pub fn visible_spans(&self) -> Vec<Vec<Span>> {
        let cursor = self.term.grid().cursor.point;
        let mut lines: Vec<Vec<Span>> = Vec::with_capacity(self.size.rows);
        let mut current_line = None;

        for indexed in self.term.grid().display_iter() {
            if current_line != Some(indexed.point.line) {
                current_line = Some(indexed.point.line);
                lines.push(Vec::new());
            }
            // A segunda metade de um glifo largo nao carrega texto proprio.
            if indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }

            let mut style = style_of(indexed.cell);
            if indexed.point == cursor {
                style.inverse = !style.inverse;
            }
            let spans = lines.last_mut().expect("uma linha foi aberta acima");
            match spans.last_mut() {
                Some(last) if last.style == style => last.text.push(indexed.cell.c),
                _ => spans.push(Span {
                    text: indexed.cell.c.to_string(),
                    style,
                }),
            }
        }

        lines
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

fn style_of(cell: &alacritty_terminal::term::cell::Cell) -> Style {
    Style {
        foreground: color_of(cell.fg),
        background: color_of(cell.bg),
        bold: cell.flags.contains(Flags::BOLD),
        inverse: cell.flags.contains(Flags::INVERSE),
    }
}

fn color_of(color: AnsiColor) -> Color {
    match color {
        AnsiColor::Named(NamedColor::Foreground | NamedColor::Background) => Color::Default,
        AnsiColor::Named(named) => Color::Indexed(named as u8),
        AnsiColor::Indexed(index) => Color::Indexed(index),
        AnsiColor::Spec(Rgb { r, g, b }) => Color::Rgb(r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(terminal: &Terminal) -> Vec<String> {
        terminal
            .visible_spans()
            .iter()
            .map(|line| {
                line.iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn cursor_cell_is_marked_inverse() {
        let mut terminal = Terminal::new();
        terminal.feed(b"ab");

        let line = terminal.visible_spans().remove(0);
        let cursor = line
            .iter()
            .find(|span| span.style.inverse)
            .expect("a célula do cursor deve sair invertida");
        assert_eq!(cursor.text.chars().count(), 1);
    }

    #[test]
    fn writes_plain_text() {
        let mut terminal = Terminal::new();
        terminal.feed("olá mundo".as_bytes());
        assert_eq!(rendered(&terminal)[0], "olá mundo");
    }

    #[test]
    fn carriage_return_overwrites_the_current_line() {
        let mut terminal = Terminal::new();
        terminal.feed(b"progresso: 10%\rprogresso: 90%");
        assert_eq!(rendered(&terminal)[0], "progresso: 90%");
    }

    #[test]
    fn line_feed_starts_a_new_line() {
        let mut terminal = Terminal::new();
        terminal.feed(b"primeira\r\nsegunda");
        let lines = rendered(&terminal);
        assert_eq!(lines[0], "primeira");
        assert_eq!(lines[1], "segunda");
    }

    #[test]
    fn sgr_colors_split_the_line_into_spans() {
        let mut terminal = Terminal::new();
        terminal.feed(b"normal\x1b[31mvermelho\x1b[0m");
        let line = terminal.visible_spans().remove(0);
        assert_eq!(line[0].style.foreground, Color::Default);
        assert_eq!(line[1].text, "vermelho");
        assert_eq!(
            line[1].style.foreground,
            Color::Indexed(NamedColor::Red as u8)
        );
    }

    #[test]
    fn unsupported_sequences_never_leak_as_text() {
        let mut terminal = Terminal::new();
        terminal.feed(b"\x1b]0;titulo da janela\x07visivel");
        assert_eq!(rendered(&terminal)[0], "visivel");
    }

    #[test]
    fn erase_to_end_of_line_drops_the_tail() {
        let mut terminal = Terminal::new();
        terminal.feed(b"abcdef\r\x1b[3C\x1b[K");
        assert_eq!(rendered(&terminal)[0], "abc");
    }

    #[test]
    fn viewport_is_limited_to_the_configured_rows() {
        let mut terminal = Terminal::new();
        for index in 0..5000 {
            terminal.feed(format!("linha {index}\r\n").as_bytes());
        }
        assert_eq!(terminal.visible_spans().len(), DEFAULT_ROWS);
    }
}
