//! A real pinned backend's end-of-cursorless-draw marker, not App state.
//! Ratatui applies all cell differences before Crossterm execute!(Hide).
const END: &[u8] = b"\x1b[?25l";

#[derive(Default)]
pub struct CursorlessFrames {
    matched: usize,
    pub last: Option<vt100::Screen>,
    pub end: usize,
}

impl CursorlessFrames {
    pub fn process(&mut self, parser: &mut vt100::Parser, bytes: &[u8], base: usize) {
        let mut parsed = 0;
        for (index, byte) in bytes.iter().copied().enumerate() {
            self.matched = if byte == END[self.matched] {
                self.matched + 1
            } else {
                usize::from(byte == END[0])
            };
            if self.matched == END.len() {
                parser.process(&bytes[parsed..=index]);
                parsed = index + 1;
                self.last = Some(parser.screen().clone());
                self.end = base + parsed;
                self.matched = 0;
            }
        }
        parser.process(&bytes[parsed..]);
    }

    pub fn invalidate_size(&mut self) {
        self.last = None;
    }
}
