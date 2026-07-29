//! Animation frames: loading, and resampling to whatever size the terminal is.
//!
//! Source art is stored as plain text, one file per frame, all frames sharing a
//! common canvas so poses stay registered. We read it as a *coverage* grid
//! (ink / no ink) rather than keeping the original characters, which is what
//! lets us rescale to any size: downscaling averages the ink in each source
//! block and picks a glyph of matching density, so thin strokes survive instead
//! of being dropped by nearest-neighbour sampling.

use std::fs;
use std::io;
use std::path::Path;

/// The animation used when nothing else is configured.
pub const DEFAULT_NAME: &str = "cat-run";

/// Animations compiled into the binary, so a fresh install has art without
/// needing any files on disk. Frames are ordered as listed.
const BUNDLED: &[(&str, &[&str])] = &[
    (
        "cat-run",
        &[
            include_str!("../assets/anim/cat-run/01.txt"),
            include_str!("../assets/anim/cat-run/02.txt"),
            include_str!("../assets/anim/cat-run/03.txt"),
            include_str!("../assets/anim/cat-run/04.txt"),
            include_str!("../assets/anim/cat-run/05.txt"),
        ],
    ),
    (
        "cat-tail",
        &[
            include_str!("../assets/anim/cat-tail/01.txt"),
            include_str!("../assets/anim/cat-tail/02.txt"),
            include_str!("../assets/anim/cat-tail/03.txt"),
            include_str!("../assets/anim/cat-tail/04.txt"),
            include_str!("../assets/anim/cat-tail/05.txt"),
            include_str!("../assets/anim/cat-tail/06.txt"),
            include_str!("../assets/anim/cat-tail/07.txt"),
            include_str!("../assets/anim/cat-tail/08.txt"),
        ],
    ),
    (
        "fox-run",
        &[
            include_str!("../assets/anim/fox-run/01.txt"),
            include_str!("../assets/anim/fox-run/02.txt"),
            include_str!("../assets/anim/fox-run/03.txt"),
            include_str!("../assets/anim/fox-run/04.txt"),
            include_str!("../assets/anim/fox-run/05.txt"),
            include_str!("../assets/anim/fox-run/06.txt"),
            include_str!("../assets/anim/fox-run/07.txt"),
            include_str!("../assets/anim/fox-run/08.txt"),
        ],
    ),
];

/// One frame as a binary coverage mask on a `width * height` grid.
pub struct Frame {
    width: usize,
    height: usize,
    ink: Vec<bool>,
}

impl Frame {
    fn parse(text: &str) -> Self {
        let rows: Vec<Vec<bool>> = text
            .lines()
            .map(|line| line.chars().map(|c| !c.is_whitespace()).collect())
            .collect();

        let height = rows.len();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);

        // Flatten into a rectangle; short lines pad with blanks.
        let mut ink = vec![false; width * height];
        for (y, row) in rows.iter().enumerate() {
            ink[y * width..y * width + row.len()].copy_from_slice(row);
        }

        Self { width, height, ink }
    }

    fn covered(&self, x: usize, y: usize) -> bool {
        self.ink[y * self.width + x]
    }

    /// Resample to `out_w * out_h` cells, mapping the ink coverage of each
    /// source block onto `ramp` (index 0 = empty, last = solid).
    ///
    /// Works for both directions: when upscaling, each output cell maps to a
    /// single source cell and coverage is simply 0 or 1.
    pub fn scale(&self, out_w: usize, out_h: usize, ramp: &[char]) -> Vec<String> {
        debug_assert!(!ramp.is_empty());
        if out_w == 0 || out_h == 0 || self.width == 0 || self.height == 0 {
            return Vec::new();
        }

        let top = (ramp.len() - 1) as f32;

        (0..out_h)
            .map(|oy| {
                let (y0, y1) = span(oy, out_h, self.height);
                let mut line = String::with_capacity(out_w);

                for ox in 0..out_w {
                    let (x0, x1) = span(ox, out_w, self.width);

                    let mut hits = 0usize;
                    for y in y0..y1 {
                        for x in x0..x1 {
                            hits += self.covered(x, y) as usize;
                        }
                    }

                    let total = (y1 - y0) * (x1 - x0);
                    let coverage = hits as f32 / total as f32;
                    line.push(ramp[(coverage * top).round() as usize]);
                }

                // Trailing blanks cost bytes on every repaint and render
                // identically to nothing.
                line.truncate(line.trim_end().len());
                line
            })
            .collect()
    }
}

/// Half-open source range covering output index `i`, guaranteed non-empty.
fn span(i: usize, out_len: usize, src_len: usize) -> (usize, usize) {
    let start = i * src_len / out_len;
    let end = ((i + 1) * src_len).div_ceil(out_len).min(src_len);
    (start, end.max(start + 1))
}

/// A loaded animation: every frame on a shared canvas.
pub struct Animation {
    pub frames: Vec<Frame>,
    /// Canvas size, used to preserve aspect ratio when fitting to the terminal.
    pub width: usize,
    pub height: usize,
}

impl Animation {
    fn from_texts(texts: impl IntoIterator<Item = String>) -> Option<Self> {
        let frames: Vec<Frame> = texts.into_iter().map(|t| Frame::parse(&t)).collect();
        if frames.is_empty() {
            return None;
        }

        // Frames are authored on one canvas, but be forgiving if they are not.
        let width = frames.iter().map(|f| f.width).max().unwrap_or(0);
        let height = frames.iter().map(|f| f.height).max().unwrap_or(0);

        Some(Self { frames, width, height })
    }

    /// Load `name`, preferring a directory on disk over the bundled copy.
    ///
    /// A user folder shadows a bundled animation of the same name, so the
    /// built-in art can be replaced without touching the binary. An unknown
    /// name is an error rather than a fallback: silently substituting different
    /// art would hide a typo, which is easy to miss once several are installed.
    pub fn load(dir: Option<&Path>, name: &str) -> io::Result<Self> {
        if let Some(dir) = dir {
            let path = dir.join(name);
            if path.is_dir()
                && let Some(anim) = Self::from_texts(read_frame_dir(&path)?)
            {
                return Ok(anim);
            }
        }

        if let Some((_, frames)) = BUNDLED.iter().find(|(n, _)| *n == name) {
            return Self::from_texts(frames.iter().map(|s| s.to_string()))
                .ok_or_else(|| io::Error::other("bundled animation is empty"));
        }

        let where_ = dir.map_or_else(
            || "no animation directory is configured".to_string(),
            |d| format!("looked in {}", d.join(name).display()),
        );
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no animation named {name:?} ({where_}); try --list"),
        ))
    }

    /// Largest `(width, height)` that fits inside the given box while keeping
    /// the canvas aspect ratio. Terminal cells are already accounted for by the
    /// art itself, which is authored for a character grid.
    pub fn fit(&self, max_w: usize, max_h: usize) -> (usize, usize) {
        let by_width = max_w as f32 / self.width as f32;
        let by_height = max_h as f32 / self.height as f32;
        // Capped at 1:1 — upscaling a coverage mask only produces chunkier
        // blocks, never more detail, so a huge terminal gets the art at its
        // authored size rather than a blown-up version of it.
        let scale = by_width.min(by_height).min(1.0);

        let w = ((self.width as f32 * scale).round() as usize).clamp(1, max_w.max(1));
        let h = ((self.height as f32 * scale).round() as usize).clamp(1, max_h.max(1));
        (w, h)
    }
}

/// One installed animation, as reported by `--list`.
pub struct Entry {
    pub name: String,
    pub frames: usize,
    /// Where it came from, or `None` when it is compiled into the binary.
    pub path: Option<std::path::PathBuf>,
}

/// Every animation available, sorted by name.
///
/// A directory shadows a bundled animation of the same name, matching what
/// `load` does — the listing must agree with what you would actually get.
pub fn list(dir: Option<&Path>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = BUNDLED
        .iter()
        .map(|(name, frames)| Entry {
            name: (*name).to_string(),
            frames: frames.len(),
            path: None,
        })
        .collect();

    if let Some(dir) = dir
        && let Ok(read) = fs::read_dir(dir)
    {
        for path in read.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()) {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let frames = read_frame_dir(&path).map_or(0, |f| f.len());
            if frames == 0 {
                continue;
            }

            let entry = Entry { name: name.to_string(), frames, path: Some(path.clone()) };
            match entries.iter_mut().find(|e| e.name == name) {
                Some(existing) => *existing = entry,
                None => entries.push(entry),
            }
        }
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Read every `.txt` in `dir` in filename order. Sorting is what defines frame
/// order, so zero-padded names (`01.txt`) matter.
fn read_frame_dir(dir: &Path) -> io::Result<Vec<String>> {
    let mut paths: Vec<_> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    paths.sort();

    paths.iter().map(fs::read_to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAMP: &[char] = &[' ', '░', '▒', '▓', '█'];

    #[test]
    fn spans_are_never_empty() {
        // Upscaling maps several output cells onto one source cell; each must
        // still resolve to a real sample rather than a zero-width range.
        for out in 1..40usize {
            for src in 1..40usize {
                for i in 0..out {
                    let (a, b) = span(i, out, src);
                    assert!(b > a, "empty span for i={i} out={out} src={src}");
                    assert!(b <= src);
                }
            }
        }
    }

    #[test]
    fn solid_block_stays_solid_at_any_size() {
        let frame = Frame::parse("####\n####\n####\n####\n");
        for (w, h) in [(2, 2), (4, 4), (9, 7), (1, 1)] {
            let out = frame.scale(w, h, RAMP);
            assert_eq!(out.len(), h);
            assert!(
                out.iter().all(|l| l.chars().all(|c| c == '█')),
                "{w}x{h} produced {out:?}"
            );
        }
    }

    #[test]
    fn thin_stroke_survives_downscaling() {
        // A one-cell diagonal on an 8x8 canvas. Nearest-neighbour sampling
        // would drop most of it; coverage averaging must keep every row inked.
        let mut rows = vec![vec!['.'; 8]; 8];
        for (i, row) in rows.iter_mut().enumerate() {
            row[i] = '#';
        }
        let text: String = rows
            .iter()
            .map(|r| r.iter().collect::<String>() + "\n")
            .collect();

        // '.' is not whitespace, so build the mask from a space-padded version.
        let text = text.replace('.', " ");
        let frame = Frame::parse(&text);

        let out = frame.scale(4, 4, RAMP);
        assert_eq!(out.len(), 4);
        assert!(
            out.iter().all(|l| l.chars().any(|c| c != ' ')),
            "stroke lost: {out:?}"
        );
    }

    #[test]
    fn blank_canvas_renders_empty_lines() {
        let frame = Frame::parse("    \n    \n");
        let out = frame.scale(2, 2, RAMP);
        assert_eq!(out, vec!["".to_string(), "".to_string()]);
    }

    #[test]
    fn embedded_frames_share_one_canvas() {
        let anim = Animation::load(None, DEFAULT_NAME).expect("bundled art must load");
        assert_eq!(anim.frames.len(), 5);
        // Registration between poses depends on this.
        assert!(anim.frames.iter().all(|f| f.height == anim.height));
    }

    #[test]
    fn every_bundled_animation_loads_and_is_listed() {
        let listed = list(None);
        assert_eq!(listed.len(), BUNDLED.len());

        for (name, frames) in BUNDLED {
            let anim = Animation::load(None, name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(anim.frames.len(), frames.len(), "{name}");
            // Poses have to be registered against a shared canvas, or the art
            // jitters as it plays.
            assert!(anim.frames.iter().all(|f| f.height == anim.height), "{name}");
            assert!(listed.iter().any(|e| e.name == *name && e.path.is_none()));
        }
    }

    #[test]
    fn an_unknown_animation_is_an_error_not_a_silent_cat() {
        // Deliberately a name no animation will ever have — an earlier version
        // of this test used "fox-run", which then got bundled and made it pass
        // for the wrong reason.
        let missing = "no-such-animation";
        let Err(err) = Animation::load(None, missing) else {
            panic!("a typo must not silently resolve to the bundled art");
        };
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains(missing), "{err}");
    }
}
