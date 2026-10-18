//! Web links in the terminal grid: OSC 8 hyperlinks, and `http://`, `https://`
//! and `www.` written as text. A click opens the URL in the browser.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point as GridPoint};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;

/// Where one grid cell sits in the logical line's text.
struct Piece {
    byte_start: usize,
    byte_end: usize,
    /// Viewport row, once the cell is on screen.
    row: Option<usize>,
    col: usize,
    width: usize,
}

/// The URL under a viewport cell, when a click there should open it.
pub(super) fn url_at<T: EventListener>(
    term: &Term<T>,
    col: usize,
    viewport_row: usize,
) -> Option<String> {
    let grid = term.grid();
    let cols = grid.columns();
    let rows = grid.screen_lines();
    if col >= cols || viewport_row >= rows {
        return None;
    }
    let line = Line(viewport_row as i32 - grid.display_offset() as i32);
    let from_cell = grid[GridPoint::new(line, Column(col))]
        .hyperlink()
        .map(|h| h.uri().to_string())
        .filter(|u| openable(u));
    if let Some(uri) = from_cell {
        return Some(uri);
    }
    let (text, pieces) = logical_line(term, line);
    let byte = pieces.iter().find_map(|p| {
        (p.row == Some(viewport_row) && col >= p.col && col < p.col + p.width)
            .then_some(p.byte_start)
    })?;
    urls_in(&text)
        .into_iter()
        .find(|(start, end, _)| byte >= *start && byte < *end)
        .map(|(_, _, url)| url)
}

/// Viewport cells that are part of a link. Index is `row * columns + col`.
pub(super) fn cells<T: EventListener>(term: &Term<T>) -> Vec<bool> {
    let grid = term.grid();
    let cols = grid.columns();
    let rows = grid.screen_lines();
    let mut mask = vec![false; rows.saturating_mul(cols)];
    if cols == 0 || rows == 0 {
        return mask;
    }
    let offset = grid.display_offset() as i32;
    let mut row = 0;
    while row < rows {
        let line = Line(row as i32 - offset);
        if row > 0 && wrapped(term, Line(line.0 - 1)) {
            row += 1;
            continue;
        }
        let (text, pieces) = logical_line(term, line);
        for (start, end, _) in urls_in(&text) {
            for piece in &pieces {
                if piece.byte_end <= start || piece.byte_start >= end {
                    continue;
                }
                mark(&mut mask, piece, cols, rows);
            }
        }
        for piece in &pieces {
            let Some(vrow) = piece.row else { continue };
            let Some(cell_line) = (vrow < rows).then_some(Line(vrow as i32 - offset)) else {
                continue;
            };
            let linked = term.grid()[GridPoint::new(cell_line, Column(piece.col))]
                .hyperlink()
                .is_some_and(|h| openable(h.uri()));
            if linked {
                mark(&mut mask, piece, cols, rows);
            }
        }
        row += 1;
    }
    mask
}

fn mark(mask: &mut [bool], piece: &Piece, cols: usize, rows: usize) {
    let Some(row) = piece.row else { return };
    if row >= rows {
        return;
    }
    for col in piece.col..piece.col + piece.width {
        if col < cols {
            mask[row * cols + col] = true;
        }
    }
}

/// The line `start` belongs to, joining wrapped rows, and each cell's place in it.
fn logical_line<T: EventListener>(term: &Term<T>, mut start: Line) -> (String, Vec<Piece>) {
    let grid = term.grid();
    let cols = grid.columns();
    let offset = grid.display_offset() as i32;
    let rows = grid.screen_lines();
    while start.0 > grid.topmost_line().0 && line_wrapped(grid, Line(start.0 - 1)) {
        start = Line(start.0 - 1);
    }
    let mut text = String::new();
    let mut pieces = Vec::new();
    let mut line = start;
    loop {
        let viewport = line.0 + offset;
        let row = (viewport >= 0 && (viewport as usize) < rows).then_some(viewport as usize);
        for col in 0..cols {
            let cell = &grid[line][Column(col)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let width = if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
            let byte_start = text.len();
            text.push(cell.c);
            pieces.push(Piece {
                byte_start,
                byte_end: text.len(),
                row,
                col,
                width,
            });
        }
        if !line_wrapped(grid, line) || line.0 >= grid.bottommost_line().0 {
            break;
        }
        line = Line(line.0 + 1);
    }
    (text, pieces)
}

fn wrapped<T: EventListener>(term: &Term<T>, line: Line) -> bool {
    line_wrapped(term.grid(), line)
}

fn line_wrapped(
    grid: &alacritty_terminal::grid::Grid<alacritty_terminal::term::cell::Cell>,
    line: Line,
) -> bool {
    grid.columns() > 0
        && grid[line][Column(grid.columns() - 1)]
            .flags
            .contains(Flags::WRAPLINE)
}

/// `http` and `https` URLs only: the click opens a browser.
fn openable(uri: &str) -> bool {
    let uri = uri.trim();
    if uri.bytes().any(|b| b.is_ascii_whitespace() || b < 0x20) {
        return false;
    }
    let body = scheme_body(uri);
    matches!(body, Some(b) if !b.is_empty() && !b.starts_with('/'))
}

fn scheme_body(uri: &str) -> Option<&str> {
    let b = uri.as_bytes();
    if b.len() >= 8 && b[..8].eq_ignore_ascii_case(b"https://") {
        Some(&uri[8..])
    } else if b.len() >= 7 && b[..7].eq_ignore_ascii_case(b"http://") {
        Some(&uri[7..])
    } else {
        None
    }
}

/// Byte ranges of links in `text`, and the URL to open (a leading `www.`
/// becomes `https://`).
fn urls_in(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut byte = 0;
    while byte < text.len() {
        if !boundary(text, byte) {
            byte += text[byte..].chars().next().map(char::len_utf8).unwrap_or(1);
            continue;
        }
        if let Some(body) = scheme_body(&text[byte..]) {
            let scheme = text.len() - byte - body.len();
            let (end, url) = take_url(&text[byte..], scheme);
            if end > scheme {
                out.push((byte, byte + end, url));
                byte += end;
                continue;
            }
        } else if text[byte..].len() > 4
            && text.as_bytes()[byte..byte + 4].eq_ignore_ascii_case(b"www.")
            && text
                .as_bytes()
                .get(byte + 4)
                .is_some_and(u8::is_ascii_alphanumeric)
        {
            let (end, host) = take_url(&text[byte..], 0);
            if end > 4 {
                out.push((byte, byte + end, format!("https://{host}")));
                byte += end;
                continue;
            }
        }
        byte += text[byte..].chars().next().map(char::len_utf8).unwrap_or(1);
    }
    out
}

/// The URL body starting at `text`, with `skip` bytes of scheme already
/// accepted. Returns the byte length to keep and the URL text.
fn take_url(text: &str, skip: usize) -> (usize, String) {
    let mut end = skip;
    for (i, c) in text[skip..].char_indices() {
        if !is_url_char(c) {
            break;
        }
        end = skip + i + c.len_utf8();
    }
    end = trim_trailing(text, end);
    (end, text[..end].to_string())
}

fn is_url_char(c: char) -> bool {
    !c.is_whitespace()
        && !matches!(c, '<' | '>' | '"' | '\'' | '`')
        && c != '\u{7f}'
        && (c as u32) >= 0x20
}

fn boundary(text: &str, byte: usize) -> bool {
    let Some(prev) = text[..byte].chars().next_back() else {
        return true;
    };
    !prev.is_ascii_alphanumeric() && !matches!(prev, '_' | '-' | '.' | '@')
}

fn trim_trailing(text: &str, mut end: usize) -> usize {
    let bytes = text.as_bytes();
    while end > 0 {
        let c = bytes[end - 1] as char;
        if matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '>') {
            end -= 1;
            continue;
        }
        if matches!(c, ')' | ']' | '}') {
            let open = match c {
                ')' => b'(',
                ']' => b'[',
                '}' => b'{',
                _ => break,
            };
            let opens = bytes[..end].iter().filter(|b| **b == open).count();
            let closes = bytes[..end].iter().filter(|b| **b == c as u8).count();
            if closes > opens {
                end -= 1;
                continue;
            }
        }
        break;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    #[test]
    fn finds_web_urls() {
        let cases = [
            (
                "see https://example.com/a. next",
                vec!["https://example.com/a"],
            ),
            (
                "https://example.com/a?x=1&y=2",
                vec!["https://example.com/a?x=1&y=2"],
            ),
            ("(https://example.com/a).", vec!["https://example.com/a"]),
            (
                "https://en.wikipedia.org/wiki/Rust_(programming_language)",
                vec!["https://en.wikipedia.org/wiki/Rust_(programming_language)"],
            ),
            ("www.rust-lang.org,", vec!["https://www.rust-lang.org"]),
            ("notwww.example.com", vec![]),
            (
                "http://localhost:8080/health",
                vec!["http://localhost:8080/health"],
            ),
            ("javascript:alert(1)", vec![]),
            ("HTTPS://Example.com/Path", vec!["HTTPS://Example.com/Path"]),
            (
                "one https://a.example then https://b.example.",
                vec!["https://a.example", "https://b.example"],
            ),
        ];
        for (text, want) in cases {
            let got: Vec<String> = urls_in(text).into_iter().map(|(_, _, u)| u).collect();
            assert_eq!(got, want, "{text}");
        }
    }

    struct Size(usize, usize);

    impl Dimensions for Size {
        fn total_lines(&self) -> usize {
            self.1
        }
        fn screen_lines(&self) -> usize {
            self.1
        }
        fn columns(&self) -> usize {
            self.0
        }
    }

    fn feed(cols: usize, rows: usize, bytes: &[u8]) -> Term<VoidListener> {
        let mut term = Term::new(Config::default(), &Size(cols, rows), VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, bytes);
        term
    }

    #[test]
    fn a_click_on_the_url_finds_it() {
        let term = feed(80, 6, b"see https://example.com/a.\n");
        assert_eq!(
            url_at(&term, 4, 0).as_deref(),
            Some("https://example.com/a")
        );
        assert_eq!(url_at(&term, 3, 0), None);
        let period = "see https://example.com/a.".len() - 1;
        assert_eq!(url_at(&term, period, 0), None);
        let mask = cells(&term);
        assert!(mask[4]);
        assert!(!mask[3]);
        assert!(!mask[period]);
    }

    #[test]
    fn an_osc8_link_uses_its_target() {
        let term = feed(
            80,
            4,
            b"\x1b]8;;https://example.com/docs\x1b\\docs\x1b]8;;\x1b\\",
        );
        assert_eq!(
            url_at(&term, 0, 0).as_deref(),
            Some("https://example.com/docs")
        );
        assert_eq!(
            url_at(&term, 3, 0).as_deref(),
            Some("https://example.com/docs")
        );
        assert_eq!(url_at(&term, 4, 0), None);
        let mask = cells(&term);
        assert!(mask[..4].iter().all(|c| *c));
        assert!(!mask[4]);
    }

    #[test]
    fn a_wrapped_url_is_one_link() {
        let url = "https://example.com/some/long/path";
        assert!(url.len() > 20);
        let term = feed(20, 6, url.as_bytes());
        assert_eq!(url_at(&term, 0, 0).as_deref(), Some(url));
        assert_eq!(url_at(&term, 0, 1).as_deref(), Some(url));
        let mask = cells(&term);
        assert!(mask[0]);
        assert!(mask[20]);
    }
}
