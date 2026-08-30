//! Navegação de arquivos remotos por SFTP.
//!
//! Toda operação vai para o runtime e volta pelo event loop; a interface nunca
//! espera a rede. O caminho corrente vive na janela, então voltar e recarregar
//! não dependem de estado escondido aqui.

use std::sync::Arc;

use chrono::{Local, TimeZone};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{AppWindow, FileRow};
use crate::backend::Backend;
use crate::libs::models::SftpEntry;

const ROOT: &str = "/";

pub fn bind(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    let navigating = Arc::clone(&backend);
    window.on_file_path_requested(move |path| {
        if let Some(window) = handle.upgrade() {
            list(&window, &navigating, &path);
        }
    });

    let handle = window.as_weak();
    let ascending = Arc::clone(&backend);
    window.on_file_parent_requested(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let parent = parent_of(window.get_files_path().as_str());
        list(&window, &ascending, &parent);
    });

    let handle = window.as_weak();
    let reloading = Arc::clone(&backend);
    window.on_file_reload_requested(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let current = window.get_files_path().to_string();
        list(&window, &reloading, &current);
    });

    let handle = window.as_weak();
    let creating = Arc::clone(&backend);
    window.on_file_folder_created(move |name| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some(session) = active_session(&window) else {
            return;
        };

        let current = window.get_files_path().to_string();
        let target = join(&current, name.trim());
        let deliver = window.as_weak();
        let after = Arc::clone(&creating);

        creating.sftp_mkdir(&session, &target, move |outcome| {
            let _ = deliver.upgrade_in_event_loop(move |window| match outcome {
                Ok(()) => list(&window, &after, &current),
                Err(error) => window.set_files_message(format!("{error}").into()),
            });
        });
    });

    let handle = window.as_weak();
    window.on_file_delete_requested(move |path, is_dir| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some(session) = active_session(&window) else {
            return;
        };

        let current = window.get_files_path().to_string();
        let deliver = window.as_weak();
        let after = Arc::clone(&backend);

        backend.sftp_delete(&session, &path, is_dir, move |outcome| {
            let _ = deliver.upgrade_in_event_loop(move |window| match outcome {
                Ok(()) => list(&window, &after, &current),
                Err(error) => window.set_files_message(format!("{error}").into()),
            });
        });
    });
}

/// Primeira listagem de uma sessão recém-aberta.
pub fn open_root(window: &AppWindow, backend: &Arc<Backend>) {
    list(window, backend, ROOT);
}

fn list(window: &AppWindow, backend: &Arc<Backend>, path: &str) {
    let Some(session) = active_session(window) else {
        return;
    };

    window.set_files_message("Carregando…".into());
    let deliver = window.as_weak();
    let requested = path.to_owned();

    backend.sftp_list(&session, path, move |outcome| {
        let _ = deliver.upgrade_in_event_loop(move |window| match outcome {
            Ok(entries) => {
                window.set_files_path(requested.as_str().into());
                window.set_files_message(SharedString::new());
                let rows = entries.iter().map(to_row).collect::<Vec<_>>();
                window.set_files(ModelRc::new(VecModel::from(rows)));
            }
            Err(error) => window.set_files_message(format!("{error}").into()),
        });
    });
}

fn active_session(window: &AppWindow) -> Option<String> {
    let session = window.get_active_session();
    (!session.is_empty()).then(|| session.to_string())
}

fn to_row(entry: &SftpEntry) -> FileRow {
    FileRow {
        name: entry.name.as_str().into(),
        path: entry.path.as_str().into(),
        is_dir: entry.is_dir,
        size: if entry.is_dir {
            SharedString::new()
        } else {
            human_size(entry.size).into()
        },
        modified_at: entry
            .modified_at
            .map(format_timestamp)
            .unwrap_or_default()
            .into(),
    }
}

/// Diretórios sobem um nível; a raiz não tem pai.
fn parent_of(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => ROOT.to_owned(),
        Some(index) => trimmed[..index].to_owned(),
    }
}

fn join(base: &str, child: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    format!("{trimmed}/{child}")
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_walks_up_one_level() {
        assert_eq!(parent_of("/var/log/nginx"), "/var/log");
        assert_eq!(parent_of("/var/log/"), "/var");
    }

    #[test]
    fn root_has_no_parent() {
        assert_eq!(parent_of("/"), "/");
        assert_eq!(parent_of("/etc"), "/");
    }

    #[test]
    fn join_never_doubles_the_separator() {
        assert_eq!(join("/var/log", "nginx"), "/var/log/nginx");
        assert_eq!(join("/", "etc"), "/etc");
    }

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }
}
