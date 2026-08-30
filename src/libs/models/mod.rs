//! Modelos serializados do domínio, divididos por área.
//!
//! A ordem dos campos e das variantes de enum define o formato binário do
//! vault: alterá-la invalida vaults existentes.

pub mod base;
pub mod notes;
pub mod runtime;
pub mod settings;
pub mod storage;

pub use base::*;
pub use notes::*;
pub use runtime::*;
pub use settings::*;
pub use storage::*;

#[cfg(test)]
mod tests;
