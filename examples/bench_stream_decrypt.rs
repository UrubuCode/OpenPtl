//! Streaming real: cifra 1 GB em frames num arquivo e decifra lendo do disco.
//!
//! cargo run --profile fastbench --example bench_stream_decrypt -- [MB] [DIR]
//!
//! Formato do frame: [nonce 24][len u32 LE][ciphertext len]

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::time::Instant;

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

fn rate(bytes: usize, secs: f64) -> String {
    format!("{:.0} MB/s", bytes as f64 / 1_048_576.0 / secs)
}

fn nonce_for(epoch: u64, index: u64) -> [u8; 24] {
    // Nonce por frame: epoch da chave + contador. Nunca derivado do conteudo,
    // para que reescrever um bloco no lugar nao repita (nonce, chave).
    let mut n = [0u8; 24];
    n[..8].copy_from_slice(&epoch.to_le_bytes());
    n[8..16].copy_from_slice(&index.to_le_bytes());
    n
}

fn write_encrypted(path: &PathBuf, size: usize, chunk: usize, cipher: &XChaCha20Poly1305) -> f64 {
    let file = File::create(path).unwrap();
    let mut out = BufWriter::with_capacity(1 << 20, file);
    let mut block = vec![0u8; chunk];
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;

    let start = Instant::now();
    let mut written = 0usize;
    let mut index = 0u64;
    while written < size {
        let take = chunk.min(size - written);
        for slot in block[..take].chunks_mut(8) {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let bytes = state.to_le_bytes();
            slot.copy_from_slice(&bytes[..slot.len()]);
        }
        let nonce = nonce_for(1, index);
        let ct = cipher
            .encrypt(XNonce::from_slice(&nonce), &block[..take])
            .unwrap();
        out.write_all(&nonce).unwrap();
        out.write_all(&(ct.len() as u32).to_le_bytes()).unwrap();
        out.write_all(&ct).unwrap();
        written += take;
        index += 1;
    }
    let mut file = out.into_inner().unwrap();
    file.flush().unwrap();
    file.sync_all().unwrap();
    start.elapsed().as_secs_f64()
}

fn stream_decrypt(path: &PathBuf, chunk: usize, cipher: &XChaCha20Poly1305) -> (f64, usize, u64) {
    let file = File::open(path).unwrap();
    let mut input = BufReader::with_capacity(1 << 20, file);
    let mut header = [0u8; 28];
    let mut ct = vec![0u8; chunk + 16];
    let mut total = 0usize;
    // Consome o plaintext para o compilador nao descartar o trabalho.
    let mut checksum = 0u64;

    let start = Instant::now();
    let mut index = 0u64;
    loop {
        match input.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => panic!("leitura falhou: {error}"),
        }
        let len = u32::from_le_bytes(header[24..28].try_into().unwrap()) as usize;
        input.read_exact(&mut ct[..len]).unwrap();

        let expected = nonce_for(1, index);
        assert_eq!(
            &header[..24],
            &expected,
            "nonce fora de ordem no frame {index}"
        );

        let plain = cipher
            .decrypt(XNonce::from_slice(&header[..24]), &ct[..len])
            .expect("frame corrompido");
        checksum = checksum
            .wrapping_add(plain.len() as u64)
            .wrapping_add(plain[0] as u64);
        total += plain.len();
        index += 1;
    }
    (start.elapsed().as_secs_f64(), total, checksum)
}

fn read_only(path: &PathBuf) -> (f64, usize) {
    let file = File::open(path).unwrap();
    let mut input = BufReader::with_capacity(1 << 20, file);
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0usize;
    let start = Instant::now();
    loop {
        let n = input.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        total += n;
    }
    (start.elapsed().as_secs_f64(), total)
}

fn main() {
    let mb: usize = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024);
    let dir = PathBuf::from(
        std::env::args()
            .nth(2)
            .unwrap_or_else(|| std::env::temp_dir().to_string_lossy().to_string()),
    );
    let size = mb * 1024 * 1024;
    let key = [0x5au8; 32];
    let cipher = XChaCha20Poly1305::new((&key).into());
    println!("payload: {mb} MB  dir: {}", dir.display());

    for chunk in [4 * 1024usize, 64 * 1024, 1024 * 1024] {
        let path = dir.join(format!("openptl-bench-{chunk}.bin"));
        let enc = write_encrypted(&path, size, chunk, &cipher);
        let bytes_on_disk = std::fs::metadata(&path).unwrap().len() as usize;

        let (io, io_bytes) = read_only(&path);
        let (dec, total, _sum) = stream_decrypt(&path, chunk, &cipher);
        assert_eq!(total, size);
        assert_eq!(io_bytes, bytes_on_disk);

        println!(
            "chunk {:>5} KiB | overhead {:.2}% | gerar+cifrar+gravar {:.2}s ({}) | ler cru {:.2}s ({}) | ler+decifrar {:.2}s ({})",
            chunk / 1024,
            (bytes_on_disk as f64 / size as f64 - 1.0) * 100.0,
            enc,
            rate(size, enc),
            io,
            rate(size, io),
            dec,
            rate(size, dec),
        );
        let _ = std::fs::remove_file(&path);
    }
}
