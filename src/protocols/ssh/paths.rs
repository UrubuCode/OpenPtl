pub(crate) fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        "/".to_string()
    } else if trimmed.starts_with('/') {
        trimmed
    } else {
        format!("/{}", trimmed)
    }
}

pub(crate) fn join_remote_path(base: &str, child: &str) -> String {
    let base = normalize_remote_path(base);
    let child = child.trim().trim_start_matches('/');
    if base == "/" {
        format!("/{}", child)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), child)
    }
}

pub(crate) fn normalize_chunk_size(chunk_size: usize) -> usize {
    chunk_size.clamp(64 * 1024, 8 * 1024 * 1024)
}
