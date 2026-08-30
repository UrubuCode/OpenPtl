//! Editor de texto com realce de sintaxe.
//!
//! O `TextEdit` do Slint não aplica estilo por trecho, então realce por ali é
//! impossível. Este módulo usa o `SyntaxEditor` do cosmic-text — que já traz
//! buffer, cursor, seleção, desfazer e realce via syntect — e rasteriza o
//! resultado num buffer de pixels. A apresentação só exibe esse buffer como
//! imagem, sem conhecer cosmic-text.

use std::sync::OnceLock;

use cosmic_text::{
    Action, Attrs, Buffer, Edit, Family, FontSystem, Metrics, Shaping, SwashCache, SyntaxEditor,
    SyntaxSystem,
};

/// Gramáticas e temas do syntect. Imutáveis depois de carregados, e caros de
/// montar, então vivem uma vez só para todo o processo.
static SYNTAX: OnceLock<SyntaxSystem> = OnceLock::new();

const FONT_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 18.0;
const THEME: &str = "base16-eighties.dark";
const BYTES_PER_PIXEL: usize = 4;

/// Quadro rasterizado, em RGBA sem pré-multiplicação.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub struct CodeEditor {
    fonts: FontSystem,
    cache: SwashCache,
    editor: SyntaxEditor<'static, 'static>,
    width: u32,
    height: u32,
}

impl CodeEditor {
    pub fn new(width: u32, height: u32) -> Self {
        let syntax = SYNTAX.get_or_init(SyntaxSystem::new);
        let mut fonts = FontSystem::new();
        let buffer = Buffer::new(&mut fonts, Metrics::new(FONT_SIZE, LINE_HEIGHT));

        let editor = SyntaxEditor::new(buffer, syntax, THEME)
            .expect("o tema embutido do syntect deve existir");

        let mut editor = Self {
            fonts,
            cache: SwashCache::new(),
            editor,
            width: width.max(1),
            height: height.max(1),
        };
        editor.apply_size();
        editor
    }

    /// Carrega o conteúdo e escolhe a gramática pela extensão do arquivo.
    pub fn load(&mut self, text: &str, extension: &str) {
        if !extension.is_empty() {
            self.editor.syntax_by_extension(extension);
        }

        let attrs = Attrs::new().family(Family::Monospace);
        self.editor.with_buffer_mut(|buffer| {
            buffer.set_text(text, &attrs, Shaping::Advanced, None);
        });
        self.editor.set_cursor(Default::default());
    }

    /// Conteúdo atual, com quebras de linha reconstituídas.
    pub fn text(&self) -> String {
        self.editor.with_buffer(|buffer| {
            buffer
                .lines
                .iter()
                .map(|line| line.text())
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    pub fn action(&mut self, action: Action) {
        self.editor.action(&mut self.fonts, action);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width.max(1);
        self.height = height.max(1);
        self.apply_size();
    }

    /// Rasteriza o estado atual. O cosmic-text entrega retângulos coloridos;
    /// preencher aqui evita depender de qualquer API gráfica.
    pub fn render(&mut self, background: (u8, u8, u8)) -> Frame {
        let width = self.width;
        let height = self.height;
        let mut pixels = background_canvas(width, height, background);

        self.editor.shape_as_needed(&mut self.fonts, false);
        self.editor
            .draw(&mut self.fonts, &mut self.cache, |x, y, w, h, color| {
                fill_rect(&mut pixels, width, height, x, y, w, h, color.as_rgba());
            });

        Frame {
            width,
            height,
            pixels,
        }
    }

    fn apply_size(&mut self) {
        let width = self.width as f32;
        let height = self.height as f32;
        self.editor.with_buffer_mut(|buffer| {
            buffer.set_size(Some(width), Some(height));
        });
    }
}

fn background_canvas(width: u32, height: u32, color: (u8, u8, u8)) -> Vec<u8> {
    let (red, green, blue) = color;
    let mut pixels = Vec::with_capacity(width as usize * height as usize * BYTES_PER_PIXEL);
    for _ in 0..(width as usize * height as usize) {
        pixels.extend_from_slice(&[red, green, blue, 255]);
    }
    pixels
}

/// Mistura um retângulo no canvas respeitando o alfa e recortando o que cai
/// fora: o cosmic-text pode devolver coordenadas parcialmente externas.
#[allow(clippy::too_many_arguments)]
fn fill_rect(
    pixels: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    rgba: [u8; 4],
) {
    let [red, green, blue, alpha] = rgba;
    if alpha == 0 {
        return;
    }

    let left = x.max(0) as u32;
    let top = y.max(0) as u32;
    let right = ((x + width as i32).max(0) as u32).min(canvas_width);
    let bottom = ((y + height as i32).max(0) as u32).min(canvas_height);

    for row in top..bottom {
        for column in left..right {
            let offset = (row as usize * canvas_width as usize + column as usize) * BYTES_PER_PIXEL;
            blend(&mut pixels[offset..offset + 3], [red, green, blue], alpha);
        }
    }
}

fn blend(target: &mut [u8], source: [u8; 3], alpha: u8) {
    let weight = alpha as u32;
    let inverse = 255 - weight;
    for channel in 0..3 {
        let mixed = (source[channel] as u32 * weight + target[channel] as u32 * inverse) / 255;
        target[channel] = mixed as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_loaded_text() {
        let mut editor = CodeEditor::new(400, 200);
        editor.load("fn main() {\n    println!(\"olá\");\n}", "rs");
        assert_eq!(editor.text(), "fn main() {\n    println!(\"olá\");\n}");
    }

    #[test]
    fn typing_reaches_the_buffer() {
        let mut editor = CodeEditor::new(400, 200);
        editor.load("ab", "txt");
        editor.action(Action::Insert('c'));
        assert_eq!(editor.text(), "cab");
    }

    #[test]
    fn frame_matches_the_requested_size() {
        let mut editor = CodeEditor::new(120, 60);
        editor.load("teste", "txt");

        let frame = editor.render((0, 0, 0));
        assert_eq!(frame.width, 120);
        assert_eq!(frame.height, 60);
        assert_eq!(frame.pixels.len(), 120 * 60 * BYTES_PER_PIXEL);
    }

    #[test]
    fn rendering_draws_something_over_the_background() {
        let mut editor = CodeEditor::new(200, 60);
        editor.load("texto visível", "txt");

        let frame = editor.render((0, 0, 0));
        assert!(
            frame
                .pixels
                .chunks(BYTES_PER_PIXEL)
                .any(|pixel| pixel[0] > 0 || pixel[1] > 0 || pixel[2] > 0),
            "o texto deve marcar o canvas"
        );
    }

    #[test]
    fn rectangles_outside_the_canvas_are_clipped() {
        let mut pixels = vec![0u8; 4 * 4 * BYTES_PER_PIXEL];
        fill_rect(&mut pixels, 4, 4, -10, -10, 2, 2, [255, 255, 255, 255]);
        assert!(pixels.iter().all(|channel| *channel == 0));

        fill_rect(&mut pixels, 4, 4, 3, 3, 40, 40, [255, 255, 255, 255]);
        assert_eq!(pixels[(3 * 4 + 3) * BYTES_PER_PIXEL], 255);
    }
}
