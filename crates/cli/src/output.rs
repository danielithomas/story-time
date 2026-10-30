//! Plain-text tables and progress bars for the terminal.

use unicode_width::UnicodeWidthStr;

pub use story_time_core::text::thousands;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

pub struct Table {
    columns: Vec<(String, Align)>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(columns: &[(&str, Align)]) -> Table {
        Table {
            columns: columns
                .iter()
                .map(|(name, align)| (name.to_string(), *align))
                .collect(),
            rows: Vec::new(),
        }
    }

    pub fn row(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }

    /// Prints the table with every line indented by `indent` spaces.
    pub fn print(&self, indent: usize) {
        let widths: Vec<usize> = (0..self.columns.len())
            .map(|i| {
                let header = self.columns[i].0.width();
                self.rows
                    .iter()
                    .map(|row| row.get(i).map_or(0, |cell| cell.width()))
                    .max()
                    .unwrap_or(0)
                    .max(header)
            })
            .collect();
        let header: Vec<String> = self.columns.iter().map(|(name, _)| name.clone()).collect();
        let show_header = header.iter().any(|name| !name.is_empty());
        for cells in show_header.then_some(&header).into_iter().chain(&self.rows) {
            let mut line = " ".repeat(indent);
            for (i, (_, align)) in self.columns.iter().enumerate() {
                let cell = cells.get(i).map_or("", String::as_str);
                let pad = " ".repeat(widths[i] - cell.width());
                match align {
                    Align::Left => line.push_str(&format!("{cell}{pad}")),
                    Align::Right => line.push_str(&format!("{pad}{cell}")),
                }
                line.push_str("  ");
            }
            println!("{}", line.trim_end());
        }
    }
}

/// A bar `width` cells wide, filled to `fraction` (clamped to 0..=1).
pub fn bar(fraction: f64, width: usize) -> String {
    let filled = ((fraction.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// `part` as a whole-number percentage of `whole`.
pub fn percent(part: u64, whole: u64) -> String {
    (part * 100)
        .checked_div(whole)
        .map_or_else(|| "–".to_string(), |p| format!("{p}%"))
}

/// A signed word count: "+1,203" or "-40".
pub fn signed(words: i64) -> String {
    if words > 0 {
        format!("+{}", thousands(words))
    } else {
        thousands(words)
    }
}

pub fn plural(n: u64, one: &str, many: &str) -> String {
    format!(
        "{} {}",
        thousands(n as i64),
        if n == 1 { one } else { many }
    )
}

/// Splits text into lines of at most `width` columns, breaking between words.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.width() + 1 + word.width() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
