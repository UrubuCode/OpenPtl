//! Benchmark do custo de cifrar/decifrar um payload grande com o esquema do vault.
//!
//! cargo run --profile bench --example bench_vault_crypto -- [MB]

use std::time::Instant;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

const CHUNK: usize = 64 * 1024;

fn derive_key(password: &str, salt: &[u8; 16]) -> [u8; 32] {
    let params = Params::new(19_456, 3, 1, Some(32)).unwrap();
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .unwrap();
    key
}

fn rate(bytes: usize, secs: f64) -> String {
    format!("{:.2} MB/s", bytes as f64 / 1_048_576.0 / secs)
}

fn main() {
    let mb: usize = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024);
    let size = mb * 1024 * 1024;
    println!("payload: {} MB ({} bytes)", mb, size);

    let start = Instant::now();
    let key = derive_key("senha-mestra-de-teste", &[7u8; 16]);
    println!("argon2id (19456 KiB, t=3, p=1): {:?}", start.elapsed());

    // Plaintext pseudo-aleatorio barato, sem custo de RNG criptografico.
    let start = Instant::now();
    let mut plaintext = vec![0u8; size];
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    for chunk in plaintext.chunks_mut(8) {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let bytes = state.to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    println!("geracao do plaintext: {:?}", start.elapsed());

    let cipher = XChaCha20Poly1305::new((&key).into());

    // --- Cenario A: one-shot, exatamente como encrypt_bin_payload/decrypt_bin_payload ---
    let nonce = XNonce::from_slice(&[3u8; 24]);
    let start = Instant::now();
    let ciphertext = cipher.encrypt(nonce, plaintext.as_ref()).unwrap();
    let enc = start.elapsed();
    println!(
        "[one-shot] encrypt: {:?} ({})",
        enc,
        rate(size, enc.as_secs_f64())
    );

    let start = Instant::now();
    let out = cipher.decrypt(nonce, ciphertext.as_ref()).unwrap();
    let dec = start.elapsed();
    println!(
        "[one-shot] decrypt: {:?} ({})",
        dec,
        rate(size, dec.as_secs_f64())
    );
    assert_eq!(out.len(), size);
    drop(out);
    drop(ciphertext);

    // --- Cenario B: blocos de 64 KiB, o que um DB cifrado exigiria ---
    let start = Instant::now();
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(size / CHUNK + 1);
    for (i, block) in plaintext.chunks(CHUNK).enumerate() {
        let mut n = [0u8; 24];
        n[..8].copy_from_slice(&(i as u64).to_le_bytes());
        frames.push(cipher.encrypt(XNonce::from_slice(&n), block).unwrap());
    }
    let enc = start.elapsed();
    println!(
        "[64KiB]    encrypt: {:?} ({})",
        enc,
        rate(size, enc.as_secs_f64())
    );

    let start = Instant::now();
    let mut total = 0usize;
    for (i, frame) in frames.iter().enumerate() {
        let mut n = [0u8; 24];
        n[..8].copy_from_slice(&(i as u64).to_le_bytes());
        total += cipher
            .decrypt(XNonce::from_slice(&n), frame.as_ref())
            .unwrap()
            .len();
    }
    let dec = start.elapsed();
    println!(
        "[64KiB]    decrypt: {:?} ({})",
        dec,
        rate(size, dec.as_secs_f64())
    );
    assert_eq!(total, size);

    // --- Custo extra do hash de conteudo/nonce derivado (SHA-256 sobre o plaintext) ---
    use sha2::{Digest, Sha256};
    let start = Instant::now();
    let mut hasher = Sha256::new();
    hasher.update(&plaintext);
    let _ = hasher.finalize();
    let h = start.elapsed();
    println!(
        "sha256 do plaintext (derive_nonce/content_hash): {:?} ({})",
        h,
        rate(size, h.as_secs_f64())
    );
}
