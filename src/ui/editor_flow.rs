//! Edição de arquivos remotos.
//!
//! O buffer e o realce vivem em `libs::editor`; aqui só traduzimos eventos do
//! Slint em ações do editor e devolvemos o quadro rasterizado como imagem. A
//! leitura e a gravação passam pela fachada, fora da thread da interface.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use cosmic_text::{Action, Motion};
use slint::platform::Key;
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, SharedString};

use super::AppWindow;
use crate::backend::Backend;
use crate::libs::editor::CodeEditor;

/// Fundo da superfície, alinhado ao `Theme.overlay` da interface.
const BACKGROUND: (u8, u8, u8) = (0x06, 0x0a, 0x11);
const DEFAULT_WIDTH: u32 = 900;
const DEFAULT_HEIGHT: u32 = 600;

/// O editor não é `Send`. Em vez de forçá-lo através da fronteira de threads,
/// ele mora na própria thread da interface: as respostas do runtime chegam pelo
/// event loop, que roda aqui, e alcançam o editor por este acesso local.
/// Só texto atravessa a fronteira.
type Shared = Rc<RefCell<CodeEditor>>;

thread_local! {
    static EDITOR: Shared = Rc::new(RefCell::new(CodeEditor::new(
        DEFAULT_WIDTH,
        DEFAULT_HEIGHT,
    )));
}

fn with_editor<T>(apply: impl FnOnce(&Shared) -> T) -> T {
    EDITOR.with(apply)
}

pub fn bind(window: &AppWindow, backend: Arc<Backend>) {
    bind_open(window, Arc::clone(&backend));
    bind_input(window);
    bind_save(window, backend);
}

fn bind_open(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_editor_open_requested(move |path| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some(session) = active_session(&window) else {
            return;
        };

        window.set_editor_message("Carregando…".into());
        let deliver = handle.clone();
        let target = path.to_string();

        backend.sftp_read(&session, &path, move |outcome| {
            // Só `outcome` e `target` cruzam a fronteira; o editor é alcançado
            // do outro lado, já na thread da interface.
            let _ = deliver.upgrade_in_event_loop(move |window| match outcome {
                Ok(content) => {
                    with_editor(|editor| {
                        editor.borrow_mut().load(&content, &extension_of(&target));
                        window.set_editor_path(target.as_str().into());
                        window.set_editor_dirty(false);
                        window.set_editor_message(SharedString::new());
                        super::workspace_flow::open_editor_block(&window, &target);
                        repaint(&window, editor);
                    });
                }
                Err(error) => window.set_editor_message(format!("{error}").into()),
            });
        });
    });
}

fn bind_input(window: &AppWindow) {
    let handle = window.as_weak();
    window.on_editor_key_pressed(move |text, control, shift| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some(action) = to_action(&text, control, shift) else {
            return;
        };

        with_editor(|editor| {
            editor.borrow_mut().action(action);
            window.set_editor_dirty(true);
            repaint(&window, editor);
        });
    });

    let handle = window.as_weak();
    window.on_editor_clicked(move |x, y| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        with_editor(|editor| {
            editor.borrow_mut().action(Action::Click { x, y });
            repaint(&window, editor);
        });
    });

    let handle = window.as_weak();
    window.on_editor_resized(move |width, height| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        if width <= 0 || height <= 0 {
            return;
        }
        with_editor(|editor| {
            editor.borrow_mut().resize(width as u32, height as u32);
            repaint(&window, editor);
        });
    });
}

fn bind_save(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_editor_save_requested(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some(session) = active_session(&window) else {
            return;
        };

        let path = window.get_editor_path().to_string();
        if path.is_empty() {
            return;
        }

        window.set_editor_message("Salvando…".into());
        let content = with_editor(|editor| editor.borrow().text());
        let deliver = handle.clone();

        backend.sftp_write(&session, &path, content, move |outcome| {
            let _ = deliver.upgrade_in_event_loop(move |window| match outcome {
                Ok(()) => {
                    window.set_editor_dirty(false);
                    window.set_editor_message("Salvo.".into());
                }
                Err(error) => window.set_editor_message(format!("{error}").into()),
            });
        });
    });

    let handle = window.as_weak();
    window.on_editor_close_requested(move || {
        if let Some(window) = handle.upgrade() {
            window.set_editor_path(SharedString::new());
            window.set_editor_dirty(false);
            window.set_editor_message(SharedString::new());
        }
    });
}

fn repaint(window: &AppWindow, editor: &Shared) {
    let frame = editor.borrow_mut().render(BACKGROUND);
    let buffer =
        SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&frame.pixels, frame.width, frame.height);
    window.set_editor_surface(Image::from_rgba8(buffer));
}

/// Traduz a tecla numa ação do editor. Combinações com Ctrl e caracteres de
/// controle são ignorados em vez de entrarem no arquivo como lixo.
fn to_action(text: &SharedString, control: bool, shift: bool) -> Option<Action> {
    let character = text.chars().next()?;

    if control {
        return None;
    }

    let action = match character {
        c if c == char::from(Key::Return) => Action::Enter,
        c if c == char::from(Key::Backspace) => Action::Backspace,
        c if c == char::from(Key::Delete) => Action::Delete,
        c if c == char::from(Key::Tab) && shift => Action::Unindent,
        c if c == char::from(Key::Tab) => Action::Indent,
        c if c == char::from(Key::Escape) => Action::Escape,
        c if c == char::from(Key::LeftArrow) => Action::Motion(Motion::Left),
        c if c == char::from(Key::RightArrow) => Action::Motion(Motion::Right),
        c if c == char::from(Key::UpArrow) => Action::Motion(Motion::Up),
        c if c == char::from(Key::DownArrow) => Action::Motion(Motion::Down),
        c if c == char::from(Key::Home) => Action::Motion(Motion::Home),
        c if c == char::from(Key::End) => Action::Motion(Motion::End),
        c if c == char::from(Key::PageUp) => Action::Motion(Motion::PageUp),
        c if c == char::from(Key::PageDown) => Action::Motion(Motion::PageDown),
        c if (c as u32) < 0x20 => return None,
        c => Action::Insert(c),
    };
    Some(action)
}

/// Gramática do realce vem da extensão do arquivo.
fn extension_of(path: &str) -> String {
    path.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_owned())
        .unwrap_or_default()
}

fn active_session(window: &AppWindow) -> Option<String> {
    let session = window.get_active_session();
    (!session.is_empty()).then(|| session.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_comes_from_the_file_name() {
        assert_eq!(extension_of("/etc/nginx/nginx.conf"), "conf");
        assert_eq!(extension_of("/srv/app/main.rs"), "rs");
    }

    #[test]
    fn files_without_extension_get_no_grammar() {
        assert_eq!(extension_of("/usr/bin/deploy"), "");
        assert_eq!(extension_of("/etc/hosts"), "");
    }

    #[test]
    fn a_dot_in_the_path_is_not_an_extension() {
        assert_eq!(extension_of("/opt/v1.2/README"), "");
    }
}
