//! Adaptadores de protocolo.
//!
//! O `SshManager` implementa mais do que a interface alcança hoje: shell local,
//! redimensionamento de PTY, renomear e copiar arquivos entre duas sessões.
#![allow(dead_code)]

pub mod sftp;
pub mod ssh;
