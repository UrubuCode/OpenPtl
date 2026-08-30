pub(crate) fn update_mouse_sgr_mode(output: &str, enabled: &mut bool) {
    let bytes = output.as_bytes();
    let mut index = 0usize;

    while index + 3 < bytes.len() {
        if bytes[index] == 0x1b && bytes[index + 1] == b'[' && bytes[index + 2] == b'?' {
            let mut cursor = index + 3;
            let mut modes = Vec::<u16>::new();
            loop {
                let start = cursor;
                while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                    cursor += 1;
                }
                if start == cursor {
                    break;
                }
                if let Ok(value) = std::str::from_utf8(&bytes[start..cursor]) {
                    if let Ok(number) = value.parse::<u16>() {
                        modes.push(number);
                    }
                }
                if cursor >= bytes.len() || bytes[cursor] != b';' {
                    break;
                }
                cursor += 1;
            }

            if cursor < bytes.len() {
                let command = bytes[cursor];
                if command == b'h' || command == b'l' {
                    let next_value = command == b'h';
                    if modes.into_iter().any(|mode| mode == 1006) {
                        *enabled = next_value;
                    }
                }
            }
        }
        index += 1;
    }
}
#[cfg(test)]
mod tests {
    use super::update_mouse_sgr_mode;

    #[test]
    fn should_toggle_sgr_mouse_mode_from_terminal_output() {
        let mut enabled = false;
        update_mouse_sgr_mode("\x1b[?1006h", &mut enabled);
        assert!(enabled);

        update_mouse_sgr_mode("\x1b[?1006l", &mut enabled);
        assert!(!enabled);
    }

    #[test]
    fn should_ignore_non_sgr_mouse_sequences() {
        let mut enabled = false;
        update_mouse_sgr_mode("\x1b[?1000h", &mut enabled);
        assert!(!enabled);

        enabled = true;
        update_mouse_sgr_mode("\x1b[?25l", &mut enabled);
        assert!(enabled);
    }
}
