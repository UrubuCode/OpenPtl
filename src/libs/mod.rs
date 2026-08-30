//! Camada de domínio.
//!
//! O vault é portado por inteiro, mas a interface Slint consome sua API aos
//! poucos, uma fase de migração por vez. O `allow` abaixo cobre esse intervalo
//! e sai quando a última seção deixar de exibir `PendingPage`.
#![allow(dead_code)]

pub mod models;
pub mod secret_store;
pub mod terminal;
pub mod vault;
