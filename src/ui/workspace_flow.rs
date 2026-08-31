//! Áreas de trabalho com blocos flutuantes.
//!
//! Cada aba é um canvas com seus próprios blocos — terminal, arquivos e editor
//! lado a lado, arrastáveis e redimensionáveis. A geometria vive aqui e não na
//! interface: a UI manda deslocamentos e recebe posições já contidas nos
//! limites, o que mantém a regra de contenção num lugar só.

use std::cell::RefCell;
use std::sync::Arc;

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use super::{AppWindow, BlockKind, BlockRow, Section, WorkspaceTab};
use crate::backend::Backend;

/// Tamanho mínimo de um bloco. Abaixo disso o cabeçalho não cabe e a janelinha
/// deixa de ser utilizável.
const MIN_WIDTH: f32 = 260.0;
const MIN_HEIGHT: f32 = 160.0;
const DEFAULT_WIDTH: f32 = 480.0;
const DEFAULT_HEIGHT: f32 = 320.0;
/// Deslocamento em cascata para um bloco novo não cobrir o anterior.
const CASCADE: f32 = 28.0;

#[derive(Default)]
struct Workspaces {
    tabs: Vec<Tab>,
    active: String,
    /// Bloco em foco. Guardado a parte da ordem de desenho para que focar nao
    /// precise reordenar a lista durante um gesto.
    focused: Option<String>,
    /// Tamanho util do canvas, informado pela interface.
    canvas: (f32, f32),
    next_id: u32,
}

struct Tab {
    id: String,
    name: String,
    blocks: Vec<Block>,
}

struct Block {
    id: String,
    kind: BlockKind,
    title: String,
    subtitle: String,
    session: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    minimized: bool,
}

thread_local! {
    static STATE: RefCell<Workspaces> = RefCell::new(Workspaces::default());
}

pub fn bind(window: &AppWindow, backend: Arc<Backend>) {
    ensure_first_tab();

    bind_tabs(window);
    bind_blocks(window);
    bind_canvas(window);
    let _ = backend;

    publish(window);
}

fn bind_tabs(window: &AppWindow) {
    let handle = window.as_weak();
    window.on_tab_created(move || {
        if let Some(window) = handle.upgrade() {
            STATE.with(|state| {
                let mut state = state.borrow_mut();
                let tab = new_tab(&mut state);
                state.active = tab.id.clone();
                state.tabs.push(tab);
            });
            publish(&window);
        }
    });

    let handle = window.as_weak();
    window.on_tab_selected(move |id| {
        if let Some(window) = handle.upgrade() {
            STATE.with(|state| state.borrow_mut().active = id.to_string());
            publish(&window);
        }
    });

    let handle = window.as_weak();
    window.on_tab_closed(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.tabs.retain(|tab| tab.id != id.as_str());
            // Fechar a última aba deixa uma vazia no lugar: um canvas sem aba
            // nenhuma não teria onde receber o próximo bloco.
            if state.tabs.is_empty() {
                let tab = new_tab(&mut state);
                state.tabs.push(tab);
            }
            if state.active == id.as_str() {
                state.active = state.tabs[0].id.clone();
            }
        });
        publish(&window);
    });
}

fn bind_blocks(window: &AppWindow) {
    let handle = window.as_weak();
    window.on_block_moved(move |id, dx, dy| {
        let canvas = canvas_size();
        with_block(&handle, &id, |block| {
            block.x += dx;
            block.y += dy;
            contain(block, canvas);
        });
    });

    let handle = window.as_weak();
    window.on_block_resized(move |id, dx, dy| {
        let canvas = canvas_size();
        with_block(&handle, &id, |block| {
            block.width = (block.width + dx).max(MIN_WIDTH);
            block.height = (block.height + dy).max(MIN_HEIGHT);
            contain(block, canvas);
        });
    });

    let handle = window.as_weak();
    window.on_block_raised(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        // Só marca o foco. Reordenar aqui trocaria o modelo no exato instante
        // em que a TouchArea capturou o ponteiro: o elemento seria recriado e a
        // captura se perderia, deixando arrastar e redimensionar sem efeito.
        STATE.with(|state| state.borrow_mut().focused = Some(id.to_string()));
        refresh_focus(&window);
    });

    let handle = window.as_weak();
    window.on_block_settled(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        // O gesto terminou: agora dá para trazer o bloco à frente de fato, que
        // no Slint é movê-lo para o fim do modelo, já que o desenho segue essa
        // ordem.
        let reordered = STATE.with(|state| {
            let mut state = state.borrow_mut();
            let focused = state.focused.clone()?;
            let tab = active_tab_mut(&mut state)?;
            let index = tab.blocks.iter().position(|block| block.id == focused)?;
            if index + 1 == tab.blocks.len() {
                return None;
            }
            let block = tab.blocks.remove(index);
            tab.blocks.push(block);
            Some(())
        });

        if reordered.is_some() {
            publish(&window);
        }
    });

    let handle = window.as_weak();
    window.on_block_minimize_toggled(move |id| {
        with_block(&handle, &id, |block| block.minimized = !block.minimized);
    });

    let handle = window.as_weak();
    window.on_block_closed(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(tab) = active_tab_mut(&mut state) {
                tab.blocks.retain(|block| block.id != id.as_str());
            }
        });
        publish(&window);
    });

    let handle = window.as_weak();
    window.on_block_add_terminal(move || {
        if let Some(window) = handle.upgrade() {
            add_block(&window, BlockKind::Terminal, "Terminal");
        }
    });

    let handle = window.as_weak();
    window.on_block_add_files(move || {
        if let Some(window) = handle.upgrade() {
            add_block(&window, BlockKind::Files, "Arquivos");
        }
    });
}

/// Abre um bloco para a sessão recém-conectada e leva o usuário ao canvas.
pub fn open_session_block(window: &AppWindow, session_id: &str, label: &str, has_shell: bool) {
    let (kind, title) = if has_shell {
        (BlockKind::Terminal, "Terminal")
    } else {
        (BlockKind::Files, "Arquivos")
    };

    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let id = next_id(&mut state, "block");
        let count = active_tab_mut(&mut state)
            .map(|tab| tab.blocks.len())
            .unwrap_or(0);
        let offset = CASCADE * count as f32;

        if let Some(tab) = active_tab_mut(&mut state) {
            tab.blocks.push(Block {
                id,
                kind,
                title: title.to_owned(),
                subtitle: label.to_owned(),
                session: session_id.to_owned(),
                x: 32.0 + offset,
                y: 24.0 + offset,
                width: DEFAULT_WIDTH,
                height: DEFAULT_HEIGHT,
                minimized: false,
            });
        }
    });

    window.set_section(Section::Workspace);
    publish(window);
}

fn add_block(window: &AppWindow, kind: BlockKind, title: &str) {
    push_block(window, kind, title, "");
    publish(window);
}

/// Acrescenta um bloco a area corrente, em cascata para nao cobrir o anterior.
fn push_block(window: &AppWindow, kind: BlockKind, title: &str, subtitle: &str) {
    let session = window.get_active_session().to_string();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let id = next_id(&mut state, "block");
        let count = active_tab_mut(&mut state)
            .map(|tab| tab.blocks.len())
            .unwrap_or(0);
        let offset = CASCADE * count as f32;

        if let Some(tab) = active_tab_mut(&mut state) {
            tab.blocks.push(Block {
                id,
                kind,
                title: title.to_owned(),
                subtitle: subtitle.to_owned(),
                session,
                x: 32.0 + offset,
                y: 24.0 + offset,
                width: DEFAULT_WIDTH,
                height: DEFAULT_HEIGHT,
                minimized: false,
            });
        }
    });
}

/// Altera um bloco e atualiza só a linha correspondente do modelo.
///
/// Trocar o modelo inteiro a cada evento destruía e recriava os `BlockFrame`,
/// e junto com eles a `TouchArea` que estava conduzindo o gesto — era o que
/// tornava arrastar e redimensionar um bloco praticamente impossível.
fn with_block(handle: &slint::Weak<AppWindow>, id: &str, apply: impl FnOnce(&mut Block)) {
    let Some(window) = handle.upgrade() else {
        return;
    };

    let updated = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let tab = active_tab_mut(&mut state)?;
        let index = tab.blocks.iter().position(|block| block.id == id)?;
        apply(&mut tab.blocks[index]);
        Some((
            index,
            to_row(&tab.blocks[index], index + 1 == tab.blocks.len()),
        ))
    });

    let Some((index, row)) = updated else {
        return;
    };
    window.get_blocks().set_row_data(index, row);
}

fn active_tab_mut(state: &mut Workspaces) -> Option<&mut Tab> {
    let active = state.active.clone();
    state.tabs.iter_mut().find(|tab| tab.id == active)
}

fn ensure_first_tab() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.tabs.is_empty() {
            let tab = new_tab(&mut state);
            state.active = tab.id.clone();
            state.tabs.push(tab);
        }
    });
}

fn new_tab(state: &mut Workspaces) -> Tab {
    let id = next_id(state, "tab");
    let name = format!("Área {}", state.tabs.len() + 1);
    Tab {
        id,
        name,
        blocks: Vec::new(),
    }
}

fn next_id(state: &mut Workspaces, prefix: &str) -> String {
    state.next_id += 1;
    format!("{prefix}-{}", state.next_id)
}

fn publish(window: &AppWindow) {
    STATE.with(|state| {
        let state = state.borrow();

        let tabs: Vec<WorkspaceTab> = state
            .tabs
            .iter()
            .map(|tab| WorkspaceTab {
                id: tab.id.as_str().into(),
                name: tab.name.as_str().into(),
            })
            .collect();

        let active = state.active.clone();
        let blocks: Vec<BlockRow> = state
            .tabs
            .iter()
            .find(|tab| tab.id == active)
            .map(|tab| {
                let top = tab.blocks.len();
                tab.blocks
                    .iter()
                    .enumerate()
                    .map(|(index, block)| to_row(block, index + 1 == top))
                    .collect()
            })
            .unwrap_or_default();

        window.set_tabs(ModelRc::new(VecModel::from(tabs)));
        window.set_active_tab(active.as_str().into());
        window.set_workspace_hint(
            "Conecte um perfil ou abra um bloco para começar a montar esta área.".into(),
        );
        window.set_blocks(ModelRc::new(VecModel::from(blocks)));
    });
}

/// Converte um bloco para a linha que a interface desenha. O bloco no topo da
/// pilha é o que está em foco.
fn to_row(block: &Block, on_top: bool) -> BlockRow {
    BlockRow {
        id: block.id.as_str().into(),
        kind: block.kind,
        title: block.title.as_str().into(),
        subtitle: block.subtitle.as_str().into(),
        session: block.session.as_str().into(),
        x: block.x,
        y: block.y,
        width: block.width,
        height: block.height,
        minimized: block.minimized,
        active: on_top,
    }
}

/// Abre (ou traz para a frente) o bloco de editor da área corrente. Editar um
/// arquivo deixa de ser uma seção separada e passa a ser mais um bloco.
pub fn open_editor_block(window: &AppWindow, path: &str) {
    let name = path.rsplit('/').next().unwrap_or(path).to_owned();

    let existing = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let tab = active_tab_mut(&mut state)?;
        let index = tab
            .blocks
            .iter()
            .position(|block| block.kind == BlockKind::Editor)?;
        let mut block = tab.blocks.remove(index);
        block.subtitle = name.clone();
        tab.blocks.push(block);
        Some(())
    });

    if existing.is_none() {
        push_block(window, BlockKind::Editor, "Editor", &name);
    }

    window.set_section(Section::Workspace);
    publish(window);
}

/// Atualiza só a marca de foco das linhas, sem trocar o modelo.
fn refresh_focus(window: &AppWindow) {
    let rows = window.get_blocks();
    STATE.with(|state| {
        let state = state.borrow();
        let Some(tab) = state.tabs.iter().find(|tab| tab.id == state.active) else {
            return;
        };
        for (index, block) in tab.blocks.iter().enumerate() {
            let focused = state.focused.as_deref() == Some(block.id.as_str());
            rows.set_row_data(index, to_row(block, focused));
        }
    });
}

fn canvas_size() -> (f32, f32) {
    STATE.with(|state| state.borrow().canvas)
}

/// Mantém o bloco dentro do canvas.
///
/// O tamanho é ajustado antes da posição: um bloco maior que a área precisa
/// encolher, senão empurrá-lo para dentro só esconderia a outra ponta. Um
/// canvas ainda sem medida deixa o bloco em paz.
fn contain(block: &mut Block, canvas: (f32, f32)) {
    let (width, height) = canvas;
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    block.width = block.width.min(width).max(MIN_WIDTH);
    block.height = block.height.min(height).max(MIN_HEIGHT);
    block.x = block.x.clamp(0.0, (width - block.width).max(0.0));
    block.y = block.y.clamp(0.0, (height - block.height).max(0.0));
}

/// A interface informa o tamanho do canvas; blocos que ficaram fora voltam.
fn bind_canvas(window: &AppWindow) {
    let handle = window.as_weak();
    window.on_canvas_resized(move |width, height| {
        let Some(window) = handle.upgrade() else {
            return;
        };

        let changed = STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.canvas == (width, height) {
                return false;
            }
            state.canvas = (width, height);

            let canvas = state.canvas;
            let Some(tab) = active_tab_mut(&mut state) else {
                return false;
            };
            for block in tab.blocks.iter_mut() {
                contain(block, canvas);
            }
            true
        });

        if changed {
            publish(&window);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(x: f32, y: f32, width: f32, height: f32) -> Block {
        Block {
            id: "b".into(),
            kind: BlockKind::Terminal,
            title: String::new(),
            subtitle: String::new(),
            session: String::new(),
            x,
            y,
            width,
            height,
            minimized: false,
        }
    }

    #[test]
    fn a_block_cannot_leave_through_the_right_or_the_bottom() {
        let mut moved = block(900.0, 700.0, 400.0, 300.0);
        contain(&mut moved, (1000.0, 800.0));

        assert_eq!(moved.x, 600.0);
        assert_eq!(moved.y, 500.0);
    }

    #[test]
    fn a_block_cannot_leave_through_the_left_or_the_top() {
        let mut moved = block(-50.0, -30.0, 400.0, 300.0);
        contain(&mut moved, (1000.0, 800.0));

        assert_eq!(moved.x, 0.0);
        assert_eq!(moved.y, 0.0);
    }

    #[test]
    fn a_block_larger_than_the_canvas_shrinks_to_fit() {
        let mut oversized = block(0.0, 0.0, 2000.0, 1500.0);
        contain(&mut oversized, (1000.0, 800.0));

        assert_eq!(oversized.width, 1000.0);
        assert_eq!(oversized.height, 800.0);
        assert_eq!(oversized.x, 0.0);
    }

    #[test]
    fn the_minimum_size_wins_over_a_tiny_canvas() {
        let mut squeezed = block(0.0, 0.0, 400.0, 300.0);
        contain(&mut squeezed, (100.0, 90.0));

        assert_eq!(squeezed.width, MIN_WIDTH);
        assert_eq!(squeezed.height, MIN_HEIGHT);
    }

    #[test]
    fn an_unmeasured_canvas_leaves_the_block_alone() {
        let mut untouched = block(900.0, 700.0, 400.0, 300.0);
        contain(&mut untouched, (0.0, 0.0));

        assert_eq!(untouched.x, 900.0);
        assert_eq!(untouched.y, 700.0);
    }
}
