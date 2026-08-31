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
        with_block(&handle, &id, |block| {
            block.x = (block.x + dx).max(0.0);
            block.y = (block.y + dy).max(0.0);
        });
    });

    let handle = window.as_weak();
    window.on_block_resized(move |id, dx, dy| {
        with_block(&handle, &id, |block| {
            block.width = (block.width + dx).max(MIN_WIDTH);
            block.height = (block.height + dy).max(MIN_HEIGHT);
        });
    });

    let handle = window.as_weak();
    window.on_block_raised(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        // Trazer para a frente é mover para o fim da lista: o Slint desenha na
        // ordem do modelo, então o último fica por cima.
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(tab) = active_tab_mut(&mut state) {
                if let Some(index) = tab.blocks.iter().position(|block| block.id == id.as_str()) {
                    let block = tab.blocks.remove(index);
                    tab.blocks.push(block);
                }
            }
        });
        publish(&window);
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
