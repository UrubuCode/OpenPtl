//! Áreas de trabalho com blocos flutuantes.
//!
//! Cada aba é um canvas com seus próprios blocos — terminal, arquivos e editor
//! lado a lado, arrastáveis e redimensionáveis. A geometria vive aqui e não na
//! interface: a UI manda deslocamentos e recebe posições já contidas nos
//! limites, o que mantém a regra de contenção num lugar só.

use std::cell::RefCell;
use std::sync::Arc;

use slint::{ComponentHandle, ModelRc, VecModel};

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
                subtitle: String::new(),
                session,
                x: 32.0 + offset,
                y: 24.0 + offset,
                width: DEFAULT_WIDTH,
                height: DEFAULT_HEIGHT,
                minimized: false,
            });
        }
    });
    publish(window);
}

fn with_block(handle: &slint::Weak<AppWindow>, id: &str, apply: impl FnOnce(&mut Block)) {
    let Some(window) = handle.upgrade() else {
        return;
    };
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(tab) = active_tab_mut(&mut state) {
            if let Some(block) = tab.blocks.iter_mut().find(|block| block.id == id) {
                apply(block);
            }
        }
    });
    publish(&window);
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
        let last = state
            .tabs
            .iter()
            .find(|tab| tab.id == active)
            .and_then(|tab| tab.blocks.last())
            .map(|block| block.id.clone());

        let blocks: Vec<BlockRow> = state
            .tabs
            .iter()
            .find(|tab| tab.id == active)
            .map(|tab| {
                tab.blocks
                    .iter()
                    .map(|block| BlockRow {
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
                        // O bloco no topo da pilha é o que está em foco.
                        active: Some(&block.id) == last.as_ref(),
                    })
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
