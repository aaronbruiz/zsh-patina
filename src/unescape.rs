use anyhow::{Context, Result, bail};

pub trait ZshUnescape {
    fn zsh_unescape_one(&self) -> Result<char>;
}

impl ZshUnescape for str {
    fn zsh_unescape_one(&self) -> Result<char> {
        let bytes = self.as_bytes();
        if bytes.len() < 2 {
            bail!("Escape sequence is too short: {self}");
        }
        if bytes[0] != b'\\' {
            bail!("Escape sequence does not start with a backslash: {self}");
        }

        Ok(match bytes[1] {
            b'a' => '\x07',
            b'b' => '\x08',
            b'f' => '\x0c',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'v' => '\x0b',

            b'x' => {
                if self.len() < 3 {
                    bail!("Hex escape sequence is too short: {self}");
                }
                if self.len() > 4 {
                    bail!("Hex escape sequence is too long: {self}");
                }
                u8::from_str_radix(&self[2..], 16)
                    .with_context(|| format!("Invalid hex escapce sequence: {self}"))?
                    as char
            }

            b'u' => {
                if self.len() < 3 {
                    bail!("Unicode escape sequence is too short: {self}");
                }
                if self.len() > 6 {
                    bail!("Unicode escape sequence is too long: {self}");
                }
                u32::from_str_radix(&self[2..], 16)
                    .map(char::from_u32)
                    .ok()
                    .flatten()
                    .with_context(|| format!("Invalid unicode escapce sequence: {self}"))?
            }

            b'U' => {
                if self.len() < 3 {
                    bail!("Unicode escape sequence is too short: {self}");
                }
                if self.len() > 10 {
                    bail!("Unicode escape sequence is too long: {self}");
                }
                u32::from_str_radix(&self[2..], 16)
                    .map(char::from_u32)
                    .ok()
                    .flatten()
                    .with_context(|| format!("Invalid unicode escapce sequence: {self}"))?
            }

            b if (b'0'..=b'8').contains(&b) => {
                if self.len() > 5 {
                    bail!("Octal escape sequence is too long: {self}");
                }
                u8::from_str_radix(&self[1..], 8)
                    .with_context(|| format!("Invalid octal escapce sequence: {self}"))?
                    as char
            }

            b if self.len() == 2 => b as char,

            _ => bail!("Invalid escape sequence: {self}"),
        })
    }
}

impl ZshUnescape for String {
    fn zsh_unescape_one(&self) -> Result<char> {
        self.as_str().zsh_unescape_one()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn too_short() {
        assert!("".zsh_unescape_one().is_err());
        assert!("\\".zsh_unescape_one().is_err());
    }

    #[test]
    fn no_leading_backslash() {
        assert!("ab".zsh_unescape_one().is_err());
    }

    #[test]
    fn simple_escapes() {
        assert_eq!(r"\a".zsh_unescape_one().unwrap(), '\x07');
        assert_eq!(r"\b".zsh_unescape_one().unwrap(), '\x08');
        assert_eq!(r"\f".zsh_unescape_one().unwrap(), '\x0c');
        assert_eq!(r"\n".zsh_unescape_one().unwrap(), '\n');
        assert_eq!(r"\r".zsh_unescape_one().unwrap(), '\r');
        assert_eq!(r"\t".zsh_unescape_one().unwrap(), '\t');
        assert_eq!(r"\v".zsh_unescape_one().unwrap(), '\x0b');
    }

    #[test]
    fn hex_valid() {
        assert_eq!(r"\x61".zsh_unescape_one().unwrap(), 'a');
        assert_eq!(r"\x0".zsh_unescape_one().unwrap(), '\0');
        assert_eq!(r"\xFF".zsh_unescape_one().unwrap(), '\u{FF}');
    }

    #[test]
    fn hex_invalid() {
        assert!(r"\x".zsh_unescape_one().is_err());
        assert!(r"\x123".zsh_unescape_one().is_err());
        assert!(r"\xZZ".zsh_unescape_one().is_err());
    }

    #[test]
    fn unicode_u_valid() {
        assert_eq!(r"\u2580".zsh_unescape_one().unwrap(), '▀');
        assert_eq!(r"\u61".zsh_unescape_one().unwrap(), 'a');
    }

    #[test]
    fn unicode_u_invalid() {
        assert!(r"\u".zsh_unescape_one().is_err());
        assert!(r"\u12345".zsh_unescape_one().is_err());
        assert!(r"\uZZZZ".zsh_unescape_one().is_err());
        assert!(r"\uD800".zsh_unescape_one().is_err());
    }

    #[test]
    fn unicode_big_u_valid() {
        assert_eq!(r"\U0001F60E".zsh_unescape_one().unwrap(), '😎');
        assert_eq!(r"\U61".zsh_unescape_one().unwrap(), 'a');
    }

    #[test]
    fn unicode_big_u_invalid() {
        assert!(r"\U".zsh_unescape_one().is_err());
        assert!(r"\U123456789".zsh_unescape_one().is_err());
        assert!(r"\UZZZZZZZZ".zsh_unescape_one().is_err());
        assert!(r"\UFFFFFFFF".zsh_unescape_one().is_err());
    }

    #[test]
    fn octal_valid() {
        assert_eq!(r"\141".zsh_unescape_one().unwrap(), 'a');
        assert_eq!(r"\00".zsh_unescape_one().unwrap(), '\0');
        assert_eq!(r"\377".zsh_unescape_one().unwrap(), '\u{FF}');
    }

    #[test]
    fn octal_invalid() {
        assert!(r"\".zsh_unescape_one().is_err());
        assert!(r"\01234".zsh_unescape_one().is_err());
        assert!(r"\09".zsh_unescape_one().is_err());
    }

    #[test]
    fn literal_escape() {
        assert_eq!(r"\\".zsh_unescape_one().unwrap(), '\\');
        assert_eq!(r"\-".zsh_unescape_one().unwrap(), '-');
        assert_eq!(r"\/".zsh_unescape_one().unwrap(), '/');
        assert_eq!(r"\!".zsh_unescape_one().unwrap(), '!');
        assert_eq!(r"\~".zsh_unescape_one().unwrap(), '~');
        assert_eq!(r"\ ".zsh_unescape_one().unwrap(), ' ');
    }

    #[test]
    fn invalid_unknown_escape() {
        assert!(r"\qAB".zsh_unescape_one().is_err());
    }
}
