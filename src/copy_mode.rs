use crate::vt::{Point, Row, TrackedPoint, Vt};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction { Forward, Backward }

pub struct Prompt { pub direction: Direction, pub query: String }

pub enum Selection {
    Chars { anchor: TrackedPoint },
    Lines { anchor: TrackedPoint },
}

pub struct CopyMode {
    pub cursor: TrackedPoint,
    pub selection: Option<Selection>,
    pub prompt: Option<Prompt>,
    pub last_search: Option<Direction>,
    pub entry_offset: usize,
    pub pending_search: bool,
    pub current_match: Option<(TrackedPoint, TrackedPoint)>,
}


pub trait Rows {
    fn rows(&self) -> u32;
    fn cols(&self) -> u16;
    fn row(&self, y: u32) -> Option<Row>;
}

impl Rows for Vt {
    fn rows(&self) -> u32 { self.screen_rows() }
    fn cols(&self) -> u16 { self.cols }
    fn row(&self, y: u32) -> Option<Row> { Vt::row(self, y) }
}

#[derive(Clone, Copy)]
pub enum Motion {
    Left, Right, Up(u32), Down(u32), Start, FirstNonblank, End,
    FirstRow, LastRow, ParagraphBack, ParagraphForward,
    WordForward, WordBackward, WordEnd, BigWordForward, BigWordBackward, BigWordEnd,
}

pub fn motion(rows: &impl Rows, from: Point, motion: Motion) -> Point {
    let count = rows.rows();
    if count == 0 { return from; }
    let from = Point { row: from.row.min(count - 1), col: from.col.min(rows.cols().saturating_sub(1)) };
    let Some(row) = rows.row(from.row) else { return from };
    let cell = |y: u32, col: u16| -> Point {
        let Some(r) = rows.row(y) else { return from };
        let x = r.spots.iter().rev().find(|s| s.x <= col).or_else(|| r.spots.first()).map_or(0, |s| s.x);
        Point { row: y, col: x }
    };
    let vertical = |y: u32| {
        let last = rows.row(y).and_then(|r| r.spots.iter().rev()
            .find(|s| !r.text[s.byte..].starts_with(char::is_whitespace)).map(|s| s.x)).unwrap_or(0);
        cell(y, from.col.min(last))
    };
    match motion {
        Motion::Up(n) => vertical(from.row.saturating_sub(n)),
        Motion::Down(n) => vertical(from.row.saturating_add(n).min(count - 1)),
        Motion::FirstRow => cell(0, from.col),
        Motion::LastRow => cell(count - 1, from.col),
        Motion::Start => cell(from.row, 0),
        Motion::Left => row.spots.iter().rev().find(|s| s.x < from.col)
            .map(|s| Point { row: from.row, col: s.x })
            .or_else(|| from.row.checked_sub(1).map(|y| cell(y, rows.cols().saturating_sub(1))))
            .unwrap_or(from),
        Motion::Right => row.spots.iter().find(|s| s.x > from.col)
            .map(|s| Point { row: from.row, col: s.x })
            .or_else(|| (from.row + 1 < count).then(|| cell(from.row + 1, 0)))
            .unwrap_or(from),
        Motion::FirstNonblank | Motion::End => {
            let nonblank = |s: &&crate::vt::Spot| !row.text[s.byte..].starts_with(char::is_whitespace);
            let spot = if matches!(motion, Motion::End) { row.spots.iter().rev().find(nonblank) } else { row.spots.iter().find(nonblank) };
            cell(from.row, spot.map_or(0, |s| s.x))
        }
        Motion::ParagraphBack | Motion::ParagraphForward => {
            let backward = matches!(motion, Motion::ParagraphBack);
            let mut y = from.row;
            loop {
                let Some(next) = (if backward { y.checked_sub(1) } else { (y + 1 < count).then_some(y + 1) }) else { break };
                y = next;
                if rows.row(y).is_some_and(|r| r.text.trim().is_empty()) { break; }
            }
            cell(y, 0)
        }
        _ => {
            let backward = matches!(motion, Motion::WordBackward | Motion::BigWordBackward);
            let mut reach = 4u32;
            loop {
            let start = if backward { from.row.saturating_sub(reach) } else { from.row };
            let end = if backward { from.row + 1 } else { from.row.saturating_add(reach).min(count) };
            let mut atoms = Vec::new();
            for y in start..end {
                let Some(r) = rows.row(y) else { break };
                for s in &r.spots {
                    let c = r.text[s.byte..].chars().next().unwrap_or(' ');
                    let class = if c.is_whitespace() { 0 } else if c.is_ascii_punctuation() && c != '_' { 1 } else { 2 };
                    atoms.push((Point { row: y, col: s.x }, class));
                }
                if !r.wraps && y + 1 < count { atoms.push((Point { row: y, col: rows.cols().saturating_sub(1) }, 0)); }
            }
            let Some(mut i) = atoms.iter().position(|(p, _)| *p == cell(from.row, from.col)) else { return from };
            let big = matches!(motion, Motion::BigWordForward | Motion::BigWordBackward | Motion::BigWordEnd);
            let class = |i: usize| if big && atoms[i].1 != 0 { 1 } else { atoms[i].1 };
            match motion {
                Motion::WordBackward | Motion::BigWordBackward => {
                    if i > 0 { i -= 1; }
                    while i > 0 && class(i) == 0 { i -= 1; }
                    while i > 0 && class(i - 1) == class(i) { i -= 1; }
                }
                Motion::WordForward | Motion::BigWordForward => {
                    let initial = class(i);
                    if i + 1 < atoms.len() { i += 1; }
                    while i + 1 < atoms.len() && initial != 0 && class(i) == initial { i += 1; }
                    while i + 1 < atoms.len() && class(i) == 0 { i += 1; }
                }
                Motion::WordEnd | Motion::BigWordEnd => {
                    if i + 1 < atoms.len() { i += 1; }
                    while i + 1 < atoms.len() && class(i) == 0 { i += 1; }
                    while i + 1 < atoms.len() && class(i + 1) == class(i) { i += 1; }
                }
                _ => {}
            }
            let needs_more = if backward { i == 0 && start > 0 } else { i + 1 == atoms.len() && end < count };
            if !needs_more { break atoms[i].0; }
            reach = reach.saturating_mul(2);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vt::Vt;

    #[test]
    fn vertical_motion_clamps_to_end_of_shorter_line() {
        let mut vt = Vt::new(10, 2);
        vt.write(b"abcdefgh\r\nxy");
        assert_eq!(motion(&vt, Point { row: 0, col: 7 }, Motion::Down(1)), Point { row: 1, col: 1 });
    }

    #[test]
    fn vertical_and_paragraph_motions_respect_screen_bounds() {
        let mut vt = Vt::new(8, 4);
        vt.write(b"first\r\n\r\nthird\r\nlast");
        assert_eq!(motion(&vt, Point { row: 0, col: 3 }, Motion::Down(10)), Point { row: 3, col: 3 });
        assert_eq!(motion(&vt, Point { row: 2, col: 3 }, Motion::ParagraphBack), Point { row: 1, col: 0 });
        assert_eq!(motion(&vt, Point { row: 0, col: 0 }, Motion::ParagraphForward), Point { row: 1, col: 0 });
        assert_eq!(motion(&vt, Point { row: 0, col: 0 }, Motion::Up(1)), Point { row: 0, col: 0 });
    }

    #[test]
    fn word_motions_distinguish_punctuation_and_big_words() {
        let mut vt = Vt::new(20, 2);
        vt.write(b"abc.def ghi");
        let p = Point { row: 0, col: 0 };
        assert_eq!(motion(&vt, p, Motion::WordForward), Point { row: 0, col: 3 });
        assert_eq!(motion(&vt, p, Motion::BigWordForward), Point { row: 0, col: 8 });
        assert_eq!(motion(&vt, p, Motion::WordEnd), Point { row: 0, col: 2 });
        assert_eq!(motion(&vt, Point { row: 0, col: 8 }, Motion::WordBackward), Point { row: 0, col: 4 });
    }

    #[test]
    fn motions_skip_wide_spacers_and_follow_wrapped_words() {
        let mut vt = Vt::new(5, 3);
        vt.write("ab日本cd".as_bytes());
        assert_eq!(motion(&vt, Point { row: 0, col: 2 }, Motion::Right), Point { row: 1, col: 0 });
        assert_eq!(motion(&vt, Point { row: 0, col: 0 }, Motion::WordEnd), Point { row: 1, col: 3 });
    }
}
