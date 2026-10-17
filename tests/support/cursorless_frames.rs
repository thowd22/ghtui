//! A real pinned backend's end-of-cursorless-draw marker, not App state.
//! Ratatui applies all cell differences before Crossterm execute!(Hide).
const END: &[u8] = b"\x1b[?25l";

#[derive(Default)]
pub struct CursorlessFrames {
    matched: usize,
    pub last: Option<vt100::Screen>,
    pub end: usize,
    /// Latest complete cell diff, including a focused text editor. The pinned
    /// backend executes Show/Hide AFTER all cell writes; Show precedes final
    /// cursor placement, so this is deliberately not a cursor-position claim.
    pub last_cells: Option<vt100::Screen>,
    pub cells_end: usize,
}

impl CursorlessFrames {
    pub fn process(&mut self, parser: &mut vt100::Parser, bytes: &[u8], base: usize) {
        self.process_observed(parser, bytes, base, |_, _| {});
    }

    /// Opt-in observations see EVERY completed cell frame, including multiple
    /// draws coalesced in one receiver chunk. No App state or input is used.
    pub fn process_observed(
        &mut self,
        parser: &mut vt100::Parser,
        bytes: &[u8],
        base: usize,
        mut observe: impl FnMut(&vt100::Screen, usize),
    ) {
        let mut parsed = 0;
        for (index, byte) in bytes.iter().copied().enumerate() {
            let focused_cells_end = self.matched == END.len() - 1 && byte == b'h';
            self.matched = if byte == END[self.matched] {
                self.matched + 1
            } else {
                usize::from(byte == END[0])
            };
            if self.matched == END.len() || focused_cells_end {
                parser.process(&bytes[parsed..=index]);
                parsed = index + 1;
                observe(parser.screen(), base + parsed);
                self.last_cells = Some(parser.screen().clone());
                self.cells_end = base + parsed;
                if !focused_cells_end {
                    self.last = Some(parser.screen().clone());
                    self.end = base + parsed;
                }
                self.matched = 0;
            }
        }
        parser.process(&bytes[parsed..]);
    }

    pub fn invalidate_size(&mut self) {
        self.last = None;
        self.last_cells = None;
    }
}
