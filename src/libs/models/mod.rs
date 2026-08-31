#![allow(dead_code)]

//! Modelos serializados do domínio, divididos por área.
//!
//! A ordem dos campos e das variantes de enum define o formato binário dos
//! arquivos locais do cofre: alterá-la invalida vaults existentes. O que
//! trafega entre dispositivos é o log de mutações, em JSON cifrado, e esse
//! não depende de posição.

pub mod base;
pub mod notes;
pub mod runtime;
pub mod settings;
pub mod storage;
pub mod sync;

pub use base::*;
pub use notes::*;
pub use runtime::*;
pub use settings::*;
pub use storage::*;
pub use sync::*;

#[cfg(test)]
mod tests;
