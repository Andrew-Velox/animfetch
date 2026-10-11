//! The kitty graphics protocol, as far as a border needs it: put a picture
//! somewhere, and take it away again.
//!
//! Kitty, WezTerm and Ghostty speak it. A picture is sent once and then placed
//! by id, so repainting the pane never resends it. Every command carries
//! `q=2`: without it the terminal answers each one, into our own input.

/// Characters of base64 per chunk. The protocol's limit, and a multiple of 4.
const CHUNK: usize = 4096;

/// Whether the terminal named by `var` draws pictures. Taken as a function so
/// it can be tested without touching the process environment.
pub fn supported_in(var: impl Fn(&str) -> Option<String>) -> bool {
    let set = |k: &str| var(k).is_some_and(|v| !v.is_empty());

    // A multiplexer swallows the escape unless it is told to pass it on.
    if set("TMUX") || set("STY") {
        return false;
    }

    let term = var("TERM").unwrap_or_default();
    let program = var("TERM_PROGRAM").unwrap_or_default();

    set("KITTY_WINDOW_ID")
        || set("GHOSTTY_RESOURCES_DIR")
        || set("WEZTERM_EXECUTABLE")
        || term.contains("kitty")
        || term.contains("ghostty")
        || matches!(program.as_str(), "ghostty" | "WezTerm")
}

pub fn supported() -> bool {
    supported_in(|k| std::env::var(k).ok())
}

/// The pixels, zlib-compressed. The border pictures are mostly transparent, so
/// this is a large saving on the way to a terminal that may be across a link.
fn deflate(bytes: &[u8]) -> Vec<u8> {
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write as _;

    let mut enc = ZlibEncoder::new(Vec::new(), Compression::fast());
    // Writing to a Vec cannot fail.
    let _ = enc.write_all(bytes);
    enc.finish().unwrap_or_default()
}

/// Send RGBA pixels as image `id`, replacing any earlier one of that id.
/// Nothing is shown until it is placed.
pub fn transmit(out: &mut String, id: u32, w: usize, h: usize, rgba: &[u8]) {
    let data = base64(&deflate(rgba));
    let pieces: Vec<&str> = data
        .as_bytes()
        .chunks(CHUNK)
        // Base64 is ASCII, so any cut is on a character boundary.
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();

    for (n, piece) in pieces.iter().enumerate() {
        let more = u8::from(n + 1 < pieces.len());
        if n == 0 {
            out.push_str(&format!(
                "\x1b_Ga=t,f=32,o=z,s={w},v={h},i={id},q=2,m={more};{piece}\x1b\\"
            ));
        } else {
            out.push_str(&format!("\x1b_Gq=2,m={more};{piece}\x1b\\"));
        }
    }
}

/// Show image `id` with its top-left at the cursor, filling `cols` by `rows`
/// cells, under the text, without moving the cursor. Placing again with the
/// same ids replaces the placement rather than adding one.
///
/// `y` pushes it that many pixels down inside its first cell, which must be
/// less than a cell high. With the origin a row higher, that is how a picture
/// is raised by part of a row: its size is the same, only where it starts.
pub fn place(out: &mut String, id: u32, cols: usize, rows: usize, x: usize, y: usize) {
    let mut offset = String::new();
    if x > 0 {
        offset.push_str(&format!(",X={x}"));
    }
    if y > 0 {
        offset.push_str(&format!(",Y={y}"));
    }
    out.push_str(&format!(
        "\x1b_Ga=p,i={id},p=1,c={cols},r={rows}{offset},z=-1,C=1,q=2\x1b\\"
    ));
}

/// Remove image `id` and free its data.
pub fn delete(out: &mut String, id: u32) {
    out.push_str(&format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\"));
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        let sextet = |shift: u32| ALPHABET[(n >> shift & 63) as usize] as char;

        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn the_terminals_that_speak_it_are_recognised() {
        assert!(supported_in(env(&[("KITTY_WINDOW_ID", "1")])));
        assert!(supported_in(env(&[("TERM", "xterm-kitty")])));
        assert!(supported_in(env(&[("TERM", "xterm-ghostty")])));
        assert!(supported_in(env(&[("TERM_PROGRAM", "ghostty")])));
        assert!(supported_in(env(&[("TERM_PROGRAM", "WezTerm")])));
        assert!(supported_in(env(&[(
            "GHOSTTY_RESOURCES_DIR",
            "/usr/share"
        )])));
    }

    #[test]
    fn others_are_not_and_a_multiplexer_hides_the_terminal_behind_it() {
        assert!(!supported_in(env(&[("TERM", "xterm-256color")])));
        assert!(!supported_in(env(&[("TERM_PROGRAM", "vscode")])));
        assert!(!supported_in(env(&[])));
        // Inside tmux a kitty variable is inherited and means nothing.
        assert!(!supported_in(env(&[
            ("TMUX", "/tmp/tmux-1/default,1,0"),
            ("KITTY_WINDOW_ID", "1")
        ])));
        assert!(!supported_in(env(&[
            ("STY", "123.pts"),
            ("TERM", "xterm-kitty")
        ])));
        // An empty variable is not a set one.
        assert!(!supported_in(env(&[("KITTY_WINDOW_ID", "")])));
    }

    #[test]
    fn base64_matches_the_standard_vectors() {
        // RFC 4648 section 10.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[test]
    fn a_small_picture_is_one_command_that_says_what_it_is() {
        let mut out = String::new();
        transmit(&mut out, 7, 2, 1, &[1, 2, 3, 4, 5, 6, 7, 8]);

        assert!(
            out.starts_with("\x1b_Ga=t,f=32,o=z,s=2,v=1,i=7,q=2,m=0;"),
            "{out:?}"
        );
        assert!(out.ends_with("\x1b\\"));
        assert_eq!(out.matches("\x1b_G").count(), 1);
    }

    #[test]
    fn a_large_picture_is_chunked_and_only_the_last_says_it_is_last() {
        let mut out = String::new();
        // Noise does not compress, so this stays several chunks long.
        let rgba = noise(4 * 100 * 100);
        transmit(&mut out, 9, 100, 100, &rgba);

        let commands: Vec<&str> = out.split("\x1b_G").skip(1).collect();
        assert!(commands.len() > 2, "{}", commands.len());
        assert!(commands[0].contains("a=t") && commands[0].contains("s=100,v=100,i=9"));
        for (n, c) in commands.iter().enumerate() {
            let last = n + 1 == commands.len();
            assert!(
                c.contains(&format!("m={}", u8::from(!last))),
                "chunk {n}: {c:.40}"
            );
            assert!(c.contains("q=2"), "every command must be quiet");
            if n > 0 {
                assert!(!c.contains("a=t"), "only the first names the action");
            }
            let payload = c.split_once(';').unwrap().1.trim_end_matches("\x1b\\");
            assert!(
                payload.len() <= CHUNK && payload.len() % 4 == 0,
                "chunk {n}"
            );
        }
    }

    #[test]
    fn the_pixels_come_back_out_of_the_chunks() {
        // Undo the chunking, the base64 and the compression: what is left must
        // be the pixels that went in.
        use std::io::Read as _;

        let rgba = noise(4 * 64 * 64);
        let mut out = String::new();
        transmit(&mut out, 1, 64, 64, &rgba);

        let joined: String = out
            .split("\x1b_G")
            .skip(1)
            .map(|c| c.split_once(';').unwrap().1.trim_end_matches("\x1b\\"))
            .collect();
        // Not `assert_eq!`: its failure message would print both strings.
        assert!(
            joined == base64(&deflate(&rgba)),
            "chunks do not join to the payload"
        );

        let mut back = Vec::new();
        flate2::read::ZlibDecoder::new(unbase64(&joined).as_slice())
            .read_to_end(&mut back)
            .unwrap();
        assert!(
            back == rgba,
            "decompressed pixels differ from the originals"
        );
    }

    #[test]
    fn mostly_empty_pictures_are_sent_small() {
        // A 200x400 transparent canvas with a line on it, like the borders.
        let mut rgba = vec![0u8; 4 * 200 * 400];
        for y in 0..400 {
            let i = (y * 200 + 100) * 4;
            rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
        let mut out = String::new();
        transmit(&mut out, 1, 200, 400, &rgba);

        assert!(
            out.len() < rgba.len() / 20,
            "{} bytes for {}",
            out.len(),
            rgba.len()
        );
    }

    /// Bytes that do not compress: xorshift, whose output has no pattern a
    /// deflate stream can find.
    fn noise(n: usize) -> Vec<u8> {
        let mut x = 0x2545_f491_4f6c_dd1du64;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 32) as u8
            })
            .collect()
    }

    /// The inverse of [`base64`], enough to check it.
    fn unbase64(s: &str) -> Vec<u8> {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let val = |c: u8| A.iter().position(|&a| a == c).unwrap_or(0) as u32;
        let mut out = Vec::new();
        for q in s.as_bytes().chunks(4) {
            let n = (val(q[0]) << 18) | (val(q[1]) << 12) | (val(q[2]) << 6) | val(q[3]);
            out.push((n >> 16) as u8);
            if q[2] != b'=' {
                out.push((n >> 8) as u8);
            }
            if q[3] != b'=' {
                out.push(n as u8);
            }
        }
        out
    }

    #[test]
    fn placing_is_under_the_text_quiet_and_does_not_move_the_cursor() {
        let mut out = String::new();
        place(&mut out, 3, 12, 20, 0, 0);

        assert_eq!(out, "\x1b_Ga=p,i=3,p=1,c=12,r=20,z=-1,C=1,q=2\x1b\\");
    }

    #[test]
    fn a_pixel_offset_is_only_sent_when_there_is_one() {
        let mut out = String::new();
        place(&mut out, 3, 12, 20, 0, 7);

        assert!(out.contains(",Y=7,"), "{out:?}");
        // Size is untouched: it is where it starts that moves.
        assert!(out.contains("c=12,r=20"));
        assert_eq!(out.matches("Y=").count(), 1);
    }

    #[test]
    fn deleting_frees_the_picture_by_id() {
        let mut out = String::new();
        delete(&mut out, 3);
        assert_eq!(out, "\x1b_Ga=d,d=I,i=3,q=2\x1b\\");
    }
}
