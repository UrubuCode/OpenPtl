use serde::{Deserialize, Serialize};

/// Cor de destaque da nota. A ordem define o formato binário de `notes.bin`:
/// variantes novas entram no fim.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NoteColor {
    #[default]
    Default,
    Yellow,
    Blue,
    Green,
    Pink,
    Purple,
    Red,
    Orange,
    Cyan,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub color: NoteColor,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub pinned: bool,
}

/// Conteúdo de `notes.bin`. Arquivo próprio, como o de known_hosts: assim as
/// notas não alteram o layout posicional de profile.bin, que invalidaria os
/// vaults já existentes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotesBinPayload {
    pub version: u32,
    #[serde(default)]
    pub notes: Vec<Note>,
}
