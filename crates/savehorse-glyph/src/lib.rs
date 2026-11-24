//! glyph — documents.
//!
//! Ported from `glyph` 1.0.0 (generation 1). The PDF writer, the extractor, the
//! Markdown renderer and the text measurements are the original algorithms,
//! kept because they are correct and small. What changed is the shape: nothing
//! here prints, everything returns data, and errors are the workspace type.
//!
//! `glyph.search` from the original is deliberately *not* ported: it was a
//! substring scan over one file, which `sift.search` already does better. It
//! stays in the manifest marked planned rather than becoming a second, weaker
//! implementation of the same thing.

use serde::{Deserialize, Serialize};
use savehorse_core::{Error, Result};
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PdfReport {
    pub file_path: String,
    pub bytes_written: u64,
    pub title: String,
    pub lines: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PdfExtractReport {
    pub file_path: String,
    pub total_pages: usize,
    pub extracted_text: String,
    pub lines_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CodeMetricsReport {
    pub file_path: String,
    pub total_lines: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub blank_lines: usize,
    pub character_count: usize,
    pub comment_ratio_percent: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EncodingReport {
    pub file_path: String,
    pub detected_encoding: String,
    pub has_bom: bool,
    pub line_terminator: String,
    pub crlf_count: usize,
    pub lf_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WordCountReport {
    pub total_words: usize,
    pub total_characters: usize,
    pub total_sentences: usize,
    pub average_word_length: f64,
    pub estimated_reading_time_mins: f64,
}

// ---------------------------------------------------------------------------
// PDF
// ---------------------------------------------------------------------------

/// Escape a string for a PDF literal string: `\`, `(` and `)`.
fn pdf_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// Write a self-contained PDF 1.4 document: one page, one text column.
///
/// The file is assembled by hand — catalog, page tree, page, content stream,
/// one standard Helvetica font — with a real cross-reference table so readers
/// accept it.
pub fn create_pdf(title: &str, lines: &[String], out_path: &Path) -> Result<PdfReport> {
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut stream = String::new();
    stream.push_str("BT\n/F1 16 Tf\n50 740 Td\n");
    stream.push_str(&format!("({}) Tj\n", pdf_escape(title)));
    stream.push_str("/F1 11 Tf\n0 -24 Td\n");
    for line in lines {
        stream.push_str(&format!("({}) Tj\n0 -16 Td\n", pdf_escape(line)));
    }
    stream.push_str("ET\n");

    let stream_bytes = stream.as_bytes();

    let mut pdf: Vec<u8> = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");

    // Offsets of every object, indexed from 1.
    let mut offsets: Vec<usize> = Vec::new();

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<</Type /Catalog /Pages 2 0 R>>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"2 0 obj\n<</Type /Pages /Kids [3 0 R] /Count 1>>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(
        b"3 0 obj\n<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources <</Font <</F1 5 0 R>>>>>>\nendobj\n",
    );

    offsets.push(pdf.len());
    pdf.extend_from_slice(format!("4 0 obj\n<</Length {}>>\nstream\n", stream_bytes.len()).as_bytes());
    pdf.extend_from_slice(stream_bytes);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(
        b"5 0 obj\n<</Type /Font /Subtype /Type1 /BaseFont /Helvetica>>\nendobj\n",
    );

    let start_xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<</Size {} /Root 1 0 R>>\nstartxref\n{}\n%%EOF\n",
            offsets.len() + 1,
            start_xref
        )
        .as_bytes(),
    );

    fs::write(out_path, &pdf)?;

    Ok(PdfReport {
        file_path: savehorse_core::display_path(&out_path),
        bytes_written: pdf.len() as u64,
        title: title.to_string(),
        lines: lines.len(),
    })
}

/// Pull the text out of a PDF written with literal strings and `Tj`/`TJ`.
///
/// This is a reader for the common case, not a general PDF parser: it does not
/// decompress streams, resolve fonts or handle every encoding. It reads what
/// `create_pdf` writes, and most simple PDFs besides.
pub fn extract_pdf_text(path: &Path) -> Result<PdfExtractReport> {
    let data = fs::read(path)?;

    // Count pages by counting /Type /Page without the trailing s.
    let as_text = String::from_utf8_lossy(&data);
    let page_count = as_text
        .split("/Type")
        .filter(|chunk| {
            let t = chunk.trim_start();
            t.starts_with("/Page") && !t.starts_with("/Pages")
        })
        .count()
        .max(1);

    let mut extracted: Vec<String> = Vec::new();
    let len = data.len();
    let mut i = 0usize;

    while i < len {
        if data[i] != b'(' {
            i += 1;
            continue;
        }

        // Walk to the matching close paren, honouring backslash escapes.
        let start = i + 1;
        let mut end = start;
        let mut escaped = false;
        while end < len {
            if escaped {
                escaped = false;
            } else if data[end] == b'\\' {
                escaped = true;
            } else if data[end] == b')' {
                break;
            }
            end += 1;
        }

        if end >= len || data[end] != b')' {
            break;
        }

        // Only keep it if a text operator follows.
        let lookahead = &data[end + 1..(end + 10).min(len)];
        let is_text = lookahead.windows(2).any(|w| w == b"Tj" || w == b"TJ");

        if is_text {
            let raw = String::from_utf8_lossy(&data[start..end]);
            let unescaped = raw
                .replace("\\(", "(")
                .replace("\\)", ")")
                .replace("\\\\", "\\");
            let trimmed = unescaped.trim();
            if !trimmed.is_empty() {
                extracted.push(trimmed.to_string());
            }
        }

        i = end + 1;
    }

    Ok(PdfExtractReport {
        file_path: savehorse_core::display_path(&path),
        total_pages: page_count,
        lines_count: extracted.len(),
        extracted_text: extracted.join("\n"),
    })
}

// ---------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------

fn format_inline(text: &str) -> String {
    let mut s = text.to_string();

    while let Some(start) = s.find("**") {
        match s[start + 2..].find("**") {
            Some(rel_end) => {
                let bold = s[start + 2..start + 2 + rel_end].to_string();
                s.replace_range(start..start + 2 + rel_end + 2, &format!("<strong>{}</strong>", bold));
            }
            None => break,
        }
    }

    while let Some(start) = s.find('`') {
        match s[start + 1..].find('`') {
            Some(rel_end) => {
                let code = s[start + 1..start + 1 + rel_end].to_string();
                s.replace_range(start..start + 1 + rel_end + 1, &format!("<code>{}</code>", code));
            }
            None => break,
        }
    }

    s
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Render Markdown to a complete, styled HTML5 page.
///
/// Handles headings, unordered lists, fenced code blocks, bold and inline
/// code. It is not CommonMark and does not pretend to be.
pub fn compile_markdown_to_html(md: &str, title: &str) -> String {
    let clean = md.strip_prefix('\u{feff}').unwrap_or(md);
    let mut body = String::new();
    let mut in_code_block = false;
    let mut in_list = false;

    for line in clean.lines() {
        let trimmed = line.trim_start_matches('\u{feff}').trim();

        if trimmed.starts_with("```") {
            if in_code_block {
                body.push_str("</code></pre>\n");
                in_code_block = false;
            } else {
                if in_list {
                    body.push_str("</ul>\n");
                    in_list = false;
                }
                body.push_str("<pre><code>");
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            body.push_str(&html_escape(line));
            body.push('\n');
            continue;
        }

        let heading = if let Some(rest) = trimmed.strip_prefix("### ") {
            Some((3, rest))
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            Some((2, rest))
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            Some((1, rest))
        } else {
            None
        };

        if let Some((level, rest)) = heading {
            if in_list {
                body.push_str("</ul>\n");
                in_list = false;
            }
            body.push_str(&format!("<h{0}>{1}</h{0}>\n", level, format_inline(rest)));
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            if !in_list {
                body.push_str("<ul>\n");
                in_list = true;
            }
            body.push_str(&format!("  <li>{}</li>\n", format_inline(&trimmed[2..])));
        } else if trimmed.is_empty() {
            if in_list {
                body.push_str("</ul>\n");
                in_list = false;
            }
        } else {
            if in_list {
                body.push_str("</ul>\n");
                in_list = false;
            }
            body.push_str(&format!("<p>{}</p>\n", format_inline(trimmed)));
        }
    }

    if in_code_block {
        body.push_str("</code></pre>\n");
    }
    if in_list {
        body.push_str("</ul>\n");
    }

    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>{}</title>\n  <style>\n    body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; line-height: 1.6; max-width: 800px; margin: 40px auto; padding: 0 20px; color: #1E293B; }}\n    pre {{ background: #0F172A; color: #F8FAFC; padding: 16px; border-radius: 6px; overflow-x: auto; }}\n    h1, h2, h3 {{ color: #0F172A; border-bottom: 1px solid #E2E8F0; padding-bottom: 8px; }}\n    code {{ font-family: Consolas, monospace; }}\n  </style>\n</head>\n<body>\n{}\n</body>\n</html>",
        html_escape(title),
        body
    )
}

// ---------------------------------------------------------------------------
// Measurements
// ---------------------------------------------------------------------------

/// Count lines, comments, blanks and character density.
pub fn analyze_code_metrics(path: &Path) -> Result<CodeMetricsReport> {
    let content = fs::read_to_string(path)?;
    let clean = content.strip_prefix('\u{feff}').unwrap_or(&content);

    let mut total_lines = 0usize;
    let mut code_lines = 0usize;
    let mut comment_lines = 0usize;
    let mut blank_lines = 0usize;
    let mut in_block_comment = false;

    for line in clean.lines() {
        total_lines += 1;
        let trimmed = line.trim_start_matches('\u{feff}').trim();

        if trimmed.is_empty() {
            blank_lines += 1;
            continue;
        }

        if in_block_comment {
            comment_lines += 1;
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }

        if trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("--")
            || trimmed.starts_with("<!--")
        {
            comment_lines += 1;
        } else if trimmed.starts_with("/*") {
            comment_lines += 1;
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
        } else {
            code_lines += 1;
        }
    }

    let ratio = if total_lines > 0 {
        (comment_lines as f64 / total_lines as f64) * 100.0
    } else {
        0.0
    };

    Ok(CodeMetricsReport {
        file_path: savehorse_core::display_path(&path),
        total_lines,
        code_lines,
        comment_lines,
        blank_lines,
        character_count: content.chars().count(),
        comment_ratio_percent: (ratio * 10.0).round() / 10.0,
    })
}

/// Detect byte-order mark, and whether the file uses CRLF or LF.
pub fn inspect_encoding(path: &Path) -> Result<EncodingReport> {
    let buf = fs::read(path)?;

    let mut has_bom = false;
    let mut encoding = "UTF-8 / ASCII".to_string();

    if buf.len() >= 3 && buf[0] == 0xEF && buf[1] == 0xBB && buf[2] == 0xBF {
        has_bom = true;
        encoding = "UTF-8 with BOM".to_string();
    } else if buf.len() >= 2 && buf[0] == 0xFF && buf[1] == 0xFE {
        has_bom = true;
        encoding = "UTF-16 LE with BOM".to_string();
    } else if buf.len() >= 2 && buf[0] == 0xFE && buf[1] == 0xFF {
        has_bom = true;
        encoding = "UTF-16 BE with BOM".to_string();
    }

    let mut crlf = 0usize;
    let mut lf = 0usize;
    for i in 0..buf.len() {
        if buf[i] == b'\n' {
            if i > 0 && buf[i - 1] == b'\r' {
                crlf += 1;
            } else {
                lf += 1;
            }
        }
    }

    let term = if crlf > lf {
        "CRLF (Windows)"
    } else if lf > 0 {
        "LF (Unix)"
    } else {
        "none — single line"
    };

    Ok(EncodingReport {
        file_path: savehorse_core::display_path(&path),
        detected_encoding: encoding,
        has_bom,
        line_terminator: term.to_string(),
        crlf_count: crlf,
        lf_count: lf,
    })
}

/// Words, characters, sentences and an estimated reading time at 200 wpm.
pub fn analyze_word_count(text: &str) -> WordCountReport {
    let words: Vec<&str> = text.split_whitespace().collect();
    let total_words = words.len();
    let total_characters = text.chars().count();

    let sentences = text.matches('.').count() + text.matches('!').count() + text.matches('?').count();

    let average_word_length = if total_words > 0 {
        let total: usize = words.iter().map(|w| w.chars().count()).sum();
        ((total as f64 / total_words as f64) * 10.0).round() / 10.0
    } else {
        0.0
    };

    WordCountReport {
        total_words,
        total_characters,
        total_sentences: sentences.max(1),
        average_word_length,
        // Two decimals, not one: rounding a short document to one decimal
        // reports 0.0 minutes, which reads as "no time at all" rather than
        // "a few seconds".
        estimated_reading_time_mins: ((total_words as f64 / 200.0) * 100.0).round() / 100.0,
    }
}

/// Read a file and measure it.
pub fn analyze_file_readability(path: &Path) -> Result<WordCountReport> {
    let text = fs::read_to_string(path)
        .map_err(|e| Error::Invalid(format!("{}: {}", savehorse_core::display_path(&path), e)))?;
    Ok(analyze_word_count(&text))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("savehorse-glyph-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn writes_a_pdf_with_the_right_skeleton() {
        let dir = scratch("pdf");
        let out = dir.join("doc.pdf");
        let lines = vec!["first line".to_string(), "second line".to_string()];

        let report = create_pdf("Test Title", &lines, &out).unwrap();
        assert!(report.bytes_written > 0);
        assert_eq!(report.lines, 2);

        let bytes = fs::read(&out).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Type /Catalog"));
        assert!(text.contains("startxref"));
        assert!(text.trim_end().ends_with("%%EOF"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pdf_round_trips_through_the_extractor() {
        let dir = scratch("roundtrip");
        let out = dir.join("doc.pdf");
        let lines = vec!["alpha beta".to_string(), "gamma (parenthesised)".to_string()];
        create_pdf("Heading", &lines, &out).unwrap();

        let report = extract_pdf_text(&out).unwrap();
        assert_eq!(report.total_pages, 1);
        assert!(report.extracted_text.contains("Heading"));
        assert!(report.extracted_text.contains("alpha beta"));
        // The escaped paren must come back as a real paren.
        assert!(report.extracted_text.contains("gamma (parenthesised)"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn markdown_renders_headings_lists_and_code() {
        let md = "# Title\n\nSome **bold** text.\n\n- one\n- two\n\n```\nlet x = 1 < 2;\n```\n";
        let html = compile_markdown_to_html(md, "Doc");

        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>one</li>"));
        assert!(html.contains("let x = 1 &lt; 2;"), "code must be escaped");
        assert!(html.trim_end().ends_with("</html>"));
    }

    #[test]
    fn markdown_escapes_the_title() {
        let html = compile_markdown_to_html("body", "A & B <C>");
        assert!(html.contains("A &amp; B &lt;C&gt;"));
    }

    #[test]
    fn counts_words_and_reading_time() {
        let report = analyze_word_count("one two three four five. six seven!");
        assert_eq!(report.total_words, 7);
        assert_eq!(report.total_sentences, 2);
        assert!(report.estimated_reading_time_mins > 0.0);
    }

    #[test]
    fn detects_crlf_and_bom() {
        let dir = scratch("encoding");

        let crlf = dir.join("crlf.txt");
        fs::write(&crlf, b"a\r\nb\r\nc").unwrap();
        let report = inspect_encoding(&crlf).unwrap();
        assert_eq!(report.crlf_count, 2);
        assert_eq!(report.lf_count, 0);
        assert!(report.line_terminator.starts_with("CRLF"));
        assert!(!report.has_bom);

        let bom = dir.join("bom.txt");
        fs::write(&bom, b"\xef\xbb\xbfhello\n").unwrap();
        let report = inspect_encoding(&bom).unwrap();
        assert!(report.has_bom);
        assert_eq!(report.detected_encoding, "UTF-8 with BOM");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn code_metrics_separate_code_comments_and_blanks() {
        let dir = scratch("metrics");
        let file = dir.join("a.rs");
        fs::write(
            &file,
            "// a comment\nfn main() {\n\n    let x = 1;\n}\n/* block\nstill block */\n",
        )
        .unwrap();

        let report = analyze_code_metrics(&file).unwrap();
        assert_eq!(report.total_lines, 7);
        assert_eq!(report.blank_lines, 1);
        assert_eq!(report.comment_lines, 3, "one line comment plus two block lines");
        assert_eq!(report.code_lines, 3);
        let _ = fs::remove_dir_all(&dir);
    }
}
