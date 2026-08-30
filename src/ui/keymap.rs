//! Tradução de teclas para as sequências que um shell espera.
//!
//! O `FocusScope` do Slint entrega o caractere da tecla mais os modificadores;
//! um terminal precisa dos bytes correspondentes. Teclas sem representação
//! definida não enviam nada, em vez de mandarem lixo para o servidor.

use slint::platform::Key;

/// Modificadores relevantes para o terminal. Shift já vem aplicado ao texto.
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
}

pub fn to_bytes(text: &str, modifiers: Modifiers) -> Vec<u8> {
    let Some(character) = text.chars().next() else {
        return Vec::new();
    };

    if let Some(sequence) = special_sequence(character) {
        return prefix_alt(sequence.to_vec(), modifiers);
    }

    if modifiers.control {
        if let Some(byte) = control_byte(character) {
            return prefix_alt(vec![byte], modifiers);
        }
    }

    prefix_alt(text.as_bytes().to_vec(), modifiers)
}

/// Sequências ANSI das teclas que não têm caractere próprio.
fn special_sequence(character: char) -> Option<&'static [u8]> {
    let sequence: &[u8] = match character {
        c if c == char::from(Key::UpArrow) => b"\x1b[A",
        c if c == char::from(Key::DownArrow) => b"\x1b[B",
        c if c == char::from(Key::RightArrow) => b"\x1b[C",
        c if c == char::from(Key::LeftArrow) => b"\x1b[D",
        c if c == char::from(Key::Home) => b"\x1b[H",
        c if c == char::from(Key::End) => b"\x1b[F",
        c if c == char::from(Key::PageUp) => b"\x1b[5~",
        c if c == char::from(Key::PageDown) => b"\x1b[6~",
        c if c == char::from(Key::Delete) => b"\x1b[3~",
        c if c == char::from(Key::Insert) => b"\x1b[2~",
        c if c == char::from(Key::Escape) => b"\x1b",
        c if c == char::from(Key::Backspace) => b"\x7f",
        c if c == char::from(Key::Return) => b"\r",
        c if c == char::from(Key::Tab) => b"\t",
        _ => return None,
    };
    Some(sequence)
}

/// Ctrl+letra vira o código de controle correspondente: Ctrl+C = 0x03.
fn control_byte(character: char) -> Option<u8> {
    match character.to_ascii_uppercase() {
        letter @ 'A'..='Z' => Some(letter as u8 - b'A' + 1),
        '[' => Some(0x1b),
        c if c == 0x5c as char => Some(0x1c),
        ']' => Some(0x1d),
        ' ' => Some(0x00),
        _ => None,
    }
}

/// Alt é enviado como ESC antes da sequência, a convenção do xterm.
fn prefix_alt(mut bytes: Vec<u8>, modifiers: Modifiers) -> Vec<u8> {
    if modifiers.alt && !bytes.starts_with(b"\x1b") {
        bytes.insert(0, 0x1b);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> Vec<u8> {
        to_bytes(text, Modifiers::default())
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(plain("a"), b"a");
        assert_eq!(plain("ç"), "ç".as_bytes());
    }

    #[test]
    fn arrows_become_ansi_sequences() {
        let up: String = char::from(Key::UpArrow).to_string();
        assert_eq!(plain(&up), b"\x1b[A");
    }

    #[test]
    fn control_letters_become_control_bytes() {
        let modifiers = Modifiers {
            control: true,
            alt: false,
        };
        assert_eq!(to_bytes("c", modifiers), vec![0x03]);
        assert_eq!(to_bytes("d", modifiers), vec![0x04]);
    }

    #[test]
    fn alt_prefixes_with_escape() {
        let modifiers = Modifiers {
            control: false,
            alt: true,
        };
        assert_eq!(to_bytes("b", modifiers), b"\x1bb");
    }

    #[test]
    fn escape_is_never_doubled_by_alt() {
        let modifiers = Modifiers {
            control: false,
            alt: true,
        };
        let left: String = char::from(Key::LeftArrow).to_string();
        assert_eq!(to_bytes(&left, modifiers), b"\x1b[D");
    }

    #[test]
    fn empty_input_sends_nothing() {
        assert!(plain("").is_empty());
    }
}
