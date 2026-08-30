//! Adaptadores de protocolo.
//!
//! A superfície do `SshManager` é maior do que a interface consome hoje; o
//! `allow` acompanha a migração e sai com a última seção portada.
#![allow(dead_code)]

pub mod sftp;
pub mod ssh;
