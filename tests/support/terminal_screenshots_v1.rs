//! Faithful cell export of a real completed PTY frame, not an application mock.
//! Default/indexed terminal colors use the documented xterm palette; RGB is exact.
use std::fmt::Write;

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn color(value: vt100::Color, foreground: bool) -> String {
    let rgb = match value {
        vt100::Color::Default => {
            if foreground {
                [229, 229, 229]
            } else {
                [0, 0, 0]
            }
        }
        vt100::Color::Rgb(r, g, b) => [r, g, b],
        vt100::Color::Idx(index) => match index {
            0..=15 => [
                [0, 0, 0],
                [205, 0, 0],
                [0, 205, 0],
                [205, 205, 0],
                [0, 0, 238],
                [205, 0, 205],
                [0, 205, 205],
                [229, 229, 229],
                [127, 127, 127],
                [255, 0, 0],
                [0, 255, 0],
                [255, 255, 0],
                [92, 92, 255],
                [255, 0, 255],
                [0, 255, 255],
                [255, 255, 255],
            ][usize::from(index)],
            16..=231 => {
                let n = index - 16;
                let component = |n| [0, 95, 135, 175, 215, 255][usize::from(n)];
                [component(n / 36), component((n / 6) % 6), component(n % 6)]
            }
            _ => [8 + 10 * (index - 232); 3],
        },
    };
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

pub fn svg(frame: &vt100::Screen) -> String {
    let (rows, cols) = frame.size();
    let width = u32::from(cols) * 10;
    let height = u32::from(rows) * 20;
    let mut out = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><title>Actual ghtui PTY frame {cols}x{rows}; offline test fixture; xterm indexed palette</title><rect width=\"100%\" height=\"100%\" fill=\"#000000\"/><g font-family=\"JetBrains Mono,monospace\" font-size=\"16\" xml:space=\"preserve\">");
    for row in 0..rows {
        for column in 0..cols {
            let cell = frame.cell(row, column).expect("actual terminal dimensions");
            if cell.is_wide_continuation() {
                continue;
            }
            let x = u32::from(column) * 10;
            let y = u32::from(row) * 20;
            let cell_width = if cell.is_wide() { 20 } else { 10 };
            let mut fg = color(cell.fgcolor(), true);
            let mut bg = color(cell.bgcolor(), false);
            if cell.inverse() {
                std::mem::swap(&mut fg, &mut bg);
            }
            write!(
                out,
                "<rect x=\"{x}\" y=\"{y}\" width=\"{cell_width}\" height=\"20\" fill=\"{bg}\"/>"
            )
            .unwrap();
            let contents = cell.contents();
            if !contents.is_empty() && contents != " " {
                write!(out, "<text x=\"{x}\" y=\"{}\" fill=\"{fg}\" textLength=\"{cell_width}\" lengthAdjust=\"spacingAndGlyphs\" font-weight=\"{}\" font-style=\"{}\" text-decoration=\"{}\">{}</text>", y + 16, if cell.bold() { "bold" } else { "normal" }, if cell.italic() { "italic" } else { "normal" }, if cell.underline() { "underline" } else { "none" }, escape(&contents)).unwrap();
            }
        }
    }
    out.push_str("</g></svg>\n");
    out
}
