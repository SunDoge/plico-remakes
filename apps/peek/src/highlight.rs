//! Syntax work runs with Git IO, never during a UI declaration.
use crate::git::{Document, LineKind, Token};
use std::sync::LazyLock;
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};
static SYNTAX: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

pub fn highlight(document: &mut Document) {
    let mut file = String::new();
    let plain = SYNTAX.find_syntax_plain_text();
    let mut light_old = HighlightLines::new(plain, &THEMES.themes["InspiredGitHub"]);
    let mut light_new = HighlightLines::new(plain, &THEMES.themes["InspiredGitHub"]);
    let mut dark_old = HighlightLines::new(plain, &THEMES.themes["base16-ocean.dark"]);
    let mut dark_new = HighlightLines::new(plain, &THEMES.themes["base16-ocean.dark"]);
    for row in &mut document.rows {
        if row.file != file {
            file.clone_from(&row.file);
            let syntax = std::path::Path::new(&file)
                .extension()
                .and_then(|e| e.to_str())
                .and_then(|ext| SYNTAX.find_syntax_by_extension(ext))
                .unwrap_or(plain);
            light_old = HighlightLines::new(syntax, &THEMES.themes["InspiredGitHub"]);
            light_new = HighlightLines::new(syntax, &THEMES.themes["InspiredGitHub"]);
            dark_old = HighlightLines::new(syntax, &THEMES.themes["base16-ocean.dark"]);
            dark_new = HighlightLines::new(syntax, &THEMES.themes["base16-ocean.dark"]);
        }
        let input = format!("{}\n", row.text);
        let (light, dark) = match row.kind {
            LineKind::Added => (&mut light_new, &mut dark_new),
            LineKind::Removed => (&mut light_old, &mut dark_old),
            LineKind::Context => {
                let _ = light_old.highlight_line(&input, &SYNTAX);
                let _ = dark_old.highlight_line(&input, &SYNTAX);
                (&mut light_new, &mut dark_new)
            }
            _ => continue,
        };
        let collect = |highlighter: &mut HighlightLines<'_>| {
            let mut offset = 0;
            highlighter
                .highlight_line(&input, &SYNTAX)
                .unwrap_or_default()
                .into_iter()
                .map(|(style, token)| {
                    let start = offset;
                    offset += token.len();
                    let c = style.foreground;
                    (
                        start,
                        offset.min(row.text.len()),
                        (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b),
                    )
                })
                .filter(|(start, end, _)| start < end)
                .collect::<Vec<_>>()
        };
        let l = collect(light);
        let d = collect(dark);
        let mut edges = l
            .iter()
            .chain(&d)
            .flat_map(|(a, b, _)| [*a, *b])
            .collect::<Vec<_>>();
        edges.sort_unstable();
        edges.dedup();
        row.spans = edges
            .windows(2)
            .filter_map(|edge| {
                let (start, end) = (edge[0], edge[1]);
                let light = l.iter().find(|(a, b, _)| *a <= start && *b >= end)?.2;
                let dark = d.iter().find(|(a, b, _)| *a <= start && *b >= end)?.2;
                Some(Token {
                    start,
                    end,
                    light,
                    dark,
                })
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_line_comment_does_not_color_the_remaining_document() {
        let mut document = Document {
            rows: ["// comment", "fn main() {}"]
                .into_iter()
                .map(|text| crate::git::Line {
                    text: text.into(),
                    file: "main.rs".into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        highlight(&mut document);
        let colors: std::collections::BTreeSet<_> = document.rows[1]
            .spans
            .iter()
            .map(|span| span.light)
            .collect();
        assert!(colors.len() > 1);
        assert!(
            document
                .rows
                .iter()
                .all(|row| row.spans.iter().all(|span| span.end <= row.text.len()))
        );
    }
}
