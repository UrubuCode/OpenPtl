//! Fluxo de notas: listar, criar, editar, fixar e excluir.
//!
//! As notas passam a viver criptografadas no cofre. A lista carrega apenas um
//! resumo; o conteúdo completo só sai do cofre para preencher o editor.

use std::sync::Arc;

use chrono::{Local, TimeZone};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{AppWindow, NoteDraft, NoteRow};
use crate::backend::Backend;
use crate::libs::models::{Note, NoteColor};

/// Quantos caracteres do conteúdo aparecem na lista.
const PREVIEW_LIMIT: usize = 90;

pub fn bind(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_note_create_requested(move || {
        if let Some(window) = handle.upgrade() {
            open_form(&window, NoteDraft::default());
        }
    });

    let handle = window.as_weak();
    let editing = Arc::clone(&backend);
    window.on_note_edit_requested(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match editing.note(&id) {
            Ok(note) => open_form(&window, to_draft(&note)),
            Err(error) => window.set_note_error(format!("{error}").into()),
        }
    });

    let handle = window.as_weak();
    let pinning = Arc::clone(&backend);
    window.on_note_pin_toggled(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let outcome = pinning.note(&id).and_then(|mut note| {
            note.pinned = !note.pinned;
            pinning.note_save(note)
        });
        match outcome {
            Ok(_) => refresh(&window, &pinning),
            Err(error) => window.set_note_error(format!("{error}").into()),
        }
    });

    let handle = window.as_weak();
    let deleting = Arc::clone(&backend);
    window.on_note_delete_requested(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match deleting.note_delete(&id) {
            Ok(()) => refresh(&window, &deleting),
            Err(error) => window.set_note_error(format!("{error}").into()),
        }
    });

    let handle = window.as_weak();
    window.on_note_form_dismissed(move || {
        if let Some(window) = handle.upgrade() {
            close_form(&window);
        }
    });

    let handle = window.as_weak();
    window.on_note_saved(move |draft| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match backend.note_save(from_draft(&draft)) {
            Ok(_) => {
                close_form(&window);
                refresh(&window, &backend);
            }
            Err(error) => window.set_note_error(format!("{error}").into()),
        }
    });
}

pub fn refresh(window: &AppWindow, backend: &Backend) {
    match backend.notes() {
        Ok(notes) => {
            let rows = notes.iter().map(to_row).collect::<Vec<_>>();
            window.set_notes(ModelRc::new(VecModel::from(rows)));
        }
        Err(error) => window.set_note_error(format!("{error}").into()),
    }
}

fn open_form(window: &AppWindow, draft: NoteDraft) {
    window.set_note_draft(draft);
    window.set_note_error(SharedString::new());
    window.set_note_form_open(true);
}

fn close_form(window: &AppWindow) {
    window.set_note_form_open(false);
    window.set_note_draft(NoteDraft::default());
    window.set_note_error(SharedString::new());
}

fn to_row(note: &Note) -> NoteRow {
    NoteRow {
        id: note.id.as_str().into(),
        title: note.title.as_str().into(),
        preview: preview_of(&note.content).into(),
        color: color_index(note.color),
        pinned: note.pinned,
        updated_at: format_timestamp(note.updated_at).into(),
    }
}

fn to_draft(note: &Note) -> NoteDraft {
    NoteDraft {
        id: note.id.as_str().into(),
        title: note.title.as_str().into(),
        content: note.content.as_str().into(),
        color: color_index(note.color),
        pinned: note.pinned,
    }
}

fn from_draft(draft: &NoteDraft) -> Note {
    Note {
        id: draft.id.to_string(),
        title: draft.title.trim().to_string(),
        content: draft.content.to_string(),
        color: color_of(draft.color),
        created_at: 0,
        updated_at: 0,
        pinned: draft.pinned,
    }
}

/// Primeira linha do conteúdo, truncada: a lista não deve revelar a nota toda.
fn preview_of(content: &str) -> String {
    let first_line = content.lines().find(|line| !line.trim().is_empty());
    let Some(line) = first_line else {
        return String::new();
    };

    if line.chars().count() <= PREVIEW_LIMIT {
        return line.trim().to_owned();
    }
    let cut: String = line.chars().take(PREVIEW_LIMIT).collect();
    format!("{}…", cut.trim_end())
}

fn color_index(color: NoteColor) -> i32 {
    match color {
        NoteColor::Default => 0,
        NoteColor::Yellow => 1,
        NoteColor::Blue => 2,
        NoteColor::Green => 3,
        NoteColor::Pink => 4,
        NoteColor::Purple => 5,
        NoteColor::Red => 6,
        NoteColor::Orange => 7,
        NoteColor::Cyan => 8,
    }
}

fn color_of(index: i32) -> NoteColor {
    match index {
        1 => NoteColor::Yellow,
        2 => NoteColor::Blue,
        3 => NoteColor::Green,
        4 => NoteColor::Pink,
        5 => NoteColor::Purple,
        6 => NoteColor::Red,
        7 => NoteColor::Orange,
        8 => NoteColor::Cyan,
        _ => NoteColor::Default,
    }
}

fn format_timestamp(seconds: i64) -> String {
    if seconds <= 0 {
        return String::new();
    }
    Local
        .timestamp_opt(seconds, 0)
        .single()
        .map(|moment| moment.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default()
}
