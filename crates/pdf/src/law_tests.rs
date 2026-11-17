//! LawCraft-owned PDF searchability tests (kept out of WordCraft's tests.rs).

use wordcraft_doc::para::InlineObject;
use wordcraft_doc::{Block, CharProps, Document, ListKind, Paragraph, Table, Watermark, para_block};
use wordcraft_doc::props::NumRef;
use std::sync::Arc;

use crate::*;

fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(16, 8, |x, y| image::Rgba([(x * 15) as u8, (y * 30) as u8, 200, 255]));
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
    buf
}

/// Headings, a bold run, a hyperlink, an internal link, a list, a table and a picture.
fn sample() -> Document {
    let mut d = Document::new();
    d.core.title = "PDF Sample".into();
    d.core.creator = "Ada Writer".into();
    d.core.subject = "Testing".into();
    d.core.keywords = "pdf, export".into();
    d.core.created = "2024-05-06T07:08:09Z".into();
    let mut b = Vec::new();
    let mut h1 = Paragraph::with_text("Introduction", CharProps::default()).styled("Heading1");
    let n = h1.len();
    h1.insert_object(n, InlineObject::BookmarkStart { name: "intro".into() }, &CharProps::default()).unwrap();
    b.push(para_block(h1));
    let mut p = Paragraph::with_text("Hello WordCraft, ", CharProps::default());
    let n = p.len();
    p.insert_text(n, "bold words", &CharProps { bold: Some(true), ..Default::default() }).unwrap();
    let n = p.len();
    p.insert_text(n, " and a website", &CharProps { link: Some("https://example.com/".into()), ..Default::default() }).unwrap();
    let n = p.len();
    p.insert_text(n, " back to top", &CharProps { link: Some("#intro".into()), italic: Some(true), ..Default::default() }).unwrap();
    b.push(para_block(p));
    b.push(para_block(Paragraph::with_text("Details", CharProps::default()).styled("Heading2")));
    let list = Arc::make_mut(&mut d.numbering).add_list(ListKind::Numbered);
    for t in ["alpha item", "beta item"] {
        let mut q = Paragraph::with_text(t, CharProps::default()).styled("ListParagraph");
        q.props.numbering = Some(NumRef { num: list, level: 0 });
        b.push(para_block(q));
    }
    let mut t = Table::new(2, 2, 400.0);
    for (r, c, s) in [(0, 0, "Cell00"), (0, 1, "Cell01"), (1, 0, "Cell10"), (1, 1, "Cell11")] {
        t.rows[r].cells[c].blocks = vec![para_block(Paragraph::with_text(s, CharProps::default()))];
    }
    b.push(Arc::new(Block::Table(t)));
    let key = d.add_media(png(), "png");
    let mut ip = Paragraph::with_text("Figure: ", CharProps::default());
    let n = ip.len();
    ip.insert_object(
        n,
        InlineObject::Image {
            media: key,
            w: 80.0,
            h: 40.0,
            alt: "gradient".into(),
            float: Default::default(),
            crop: [0.1, 0.0, 0.0, 0.0],
            fx: Default::default(),
            ole: None,
        },
        &CharProps::default(),
    )
    .unwrap();
    b.push(para_block(ip));
    let mut big = Paragraph::with_text("Second page", CharProps::default()).styled("Heading1");
    big.props.page_break_before = Some(true);
    b.push(para_block(big));
    b.push(para_block(Paragraph::with_text(
        "Dotted and wavy",
        CharProps { underline: Some(wordcraft_doc::props::Underline::Wave), ..Default::default() },
    )));
    d.body = b;
    d.settings.watermark = Some(Watermark::default());
    d.settings.page_color = Some(Rgb(0xFF, 0xFE, 0xF0));
    d
}

fn extract_text(bytes: &[u8]) -> Vec<String> {
    extract_page_text(bytes).expect("parse")
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The wasm build has no PDF interpreter and passes `None` here; that must never read as a
/// searchable PDF (a bare `%PDF` signature is not text).
#[test]
fn a_build_without_an_extractor_never_claims_a_pdf_is_searchable() {
    assert_eq!(searchability_verdict(None), TextSearchability::Unavailable);
    assert_eq!(searchability_verdict(None).is_searchable(), None);
    assert_eq!(searchability_verdict(Some(false)), TextSearchability::NotSearchable);
    assert_eq!(searchability_verdict(Some(false)).is_searchable(), Some(false));
    assert_eq!(searchability_verdict(Some(true)), TextSearchability::Searchable);
    assert_eq!(searchability_verdict(Some(true)).is_searchable(), Some(true));
}

/// A non-PDF byte string is confidently not searchable (this is what the wasm signature check
/// used to get wrong); a real exported PDF with text is searchable.
#[test]
fn text_searchability_inspects_real_bytes() {
    assert_eq!(text_searchability(b"%PDF-not-really"), TextSearchability::NotSearchable);
    assert_eq!(text_searchability(b"hello"), TextSearchability::NotSearchable);
    let bytes = export(&sample(), &PdfOptions::default()).unwrap();
    assert_eq!(text_searchability(&bytes), TextSearchability::Searchable);
}

/// A scanned filing with a searchable page number is still an unverified picture: the page number
/// extracts, but text on a page a picture dominates never certifies the picture's content. The page
/// is reported as uncertain (a warning naming the page), never certified as not searchable.
#[test]
fn a_scanned_page_with_a_searchable_page_number_is_uncertain() {
    let mut d = Document::new();
    // The "scan": a full-page picture with no text of its own.
    let media = d.add_media(png(), "png");
    let mut p = Paragraph::with_text("", CharProps::default());
    p.insert_object(
        0,
        InlineObject::Image {
            media,
            w: 300.0,
            h: 400.0,
            alt: String::new(),
            float: Default::default(),
            crop: [0.0; 4],
            fx: Default::default(),
            ole: None,
        },
        &CharProps::default(),
    )
    .unwrap();
    d.body = vec![para_block(p)];
    // A page number in the footer: real, extractable text.
    let mut f = Paragraph::with_text("", CharProps::default());
    f.insert_object(
        0,
        InlineObject::Field { instr: "PAGE".into(), result: String::new(), locked: false, dirty: false, code: false, form: String::new() },
        &CharProps::default(),
    )
    .unwrap();
    let id = d.add_part(wordcraft_doc::PartKind::Footer, vec![para_block(f)]);
    d.last_section.footers.default = Some(id);

    let bytes = export(&d, &PdfOptions::default()).unwrap();
    assert!(squash(&extract_text(&bytes).concat()).contains('1'), "the page number is extractable");
    assert_eq!(text_searchability(&bytes), TextSearchability::Uncertain);
    assert_eq!(crate::extract::extract_verdict_detail(&bytes).unverifiable_pages, vec![1]);
}

/// A scan with no extractable text at all is an unverified picture, not a document confidently
/// without a text layer: the verdict preserves the page detail so it can be warned about.
#[test]
fn an_image_only_scan_is_uncertain_not_not_searchable() {
    let mut d = Document::new();
    let media = d.add_media(png(), "png");
    let mut scan = Paragraph::with_text("", CharProps::default());
    scan.insert_object(
        0,
        InlineObject::Image {
            media,
            w: 300.0,
            h: 400.0,
            alt: String::new(),
            float: Default::default(),
            crop: [0.0; 4],
            fx: Default::default(),
            ole: None,
        },
        &CharProps::default(),
    )
    .unwrap();
    d.body = vec![para_block(scan)];
    let bytes = export(&d, &PdfOptions::default()).unwrap();
    assert_eq!(text_searchability(&bytes), TextSearchability::Uncertain);
    let detail = crate::extract::extract_verdict_detail(&bytes);
    assert_eq!(detail.verdict, crate::extract::Verdict::Uncertain);
    assert_eq!(detail.unverifiable_pages, vec![1]);
}

/// Extractable numeric body text is a text layer: a short document that is all digits is
/// searchable, just as alphabetic text is. (A number is only furniture when it is artifact-marked,
/// or when it sits on a page a picture dominates; untagged body digits are the document's text.)
#[test]
fn numeric_body_text_is_a_text_layer() {
    let mut d = Document::new();
    d.body = vec![para_block(Paragraph::with_text("123", CharProps::default()))];
    let bytes = export(&d, &PdfOptions { tagged: false, ..Default::default() }).unwrap();
    assert!(squash(&extract_text(&bytes).concat()).contains("123"));
    assert_eq!(text_searchability(&bytes), TextSearchability::Searchable);
    // A paragraph of alphabetic text certifies it too.
    d.body = vec![para_block(Paragraph::with_text("The quick brown fox jumps over the lazy dog.", CharProps::default()))];
    let bytes = export(&d, &PdfOptions { tagged: false, ..Default::default() }).unwrap();
    assert_eq!(text_searchability(&bytes), TextSearchability::Searchable);
}

/// A searchable cover followed by scanned (image-only) body pages is **not** text-searchable: one
/// page carrying text does not certify a whole PDF that has pages of its own with no text.
#[test]
fn a_searchable_cover_with_scanned_body_pages_is_not_searchable() {
    let mut d = Document::new();
    let media = d.add_media(png(), "png");
    let mut scan = Paragraph::with_text("", CharProps::default());
    scan.insert_object(
        0,
        InlineObject::Image {
            media,
            w: 300.0,
            h: 400.0,
            alt: String::new(),
            float: Default::default(),
            crop: [0.0; 4],
            fx: Default::default(),
            ole: None,
        },
        &CharProps::default(),
    )
    .unwrap();
    scan.props.page_break_before = Some(true);
    d.body = vec![para_block(Paragraph::with_text("Cover page", CharProps::default())), para_block(scan)];
    let bytes = export(&d, &PdfOptions::default()).unwrap();
    let pages = extract_text(&bytes);
    assert_eq!(pages.len(), 2);
    assert!(squash(&pages[0]).contains("Cover page"));
    assert_eq!(text_searchability(&bytes), TextSearchability::Uncertain, "a scanned body page is unverifiable");
    let detail = crate::extract::extract_verdict_detail(&bytes);
    assert_eq!(detail.unverifiable_pages, vec![2]);
}

/// A scan with a caption beside it cannot be certified searchable: the caption extracts, but the
/// scanned writing it labels does not.
#[test]
fn a_scan_with_a_caption_is_uncertain() {
    let mut d = Document::new();
    let media = d.add_media(png(), "png");
    let mut scan = Paragraph::with_text("", CharProps::default());
    scan.insert_object(
        0,
        InlineObject::Image {
            media,
            w: 300.0,
            h: 400.0,
            alt: String::new(),
            float: Default::default(),
            crop: [0.0; 4],
            fx: Default::default(),
            ole: None,
        },
        &CharProps::default(),
    )
    .unwrap();
    // The caption lands on the same page as the scan it labels.
    d.body = vec![para_block(scan), para_block(Paragraph::with_text("Exhibit A", CharProps::default()))];
    let bytes = export(&d, &PdfOptions::default()).unwrap();
    assert_eq!(extract_text(&bytes).len(), 1, "the scan and its caption share a page");
    assert!(squash(&extract_text(&bytes).concat()).contains("Exhibit A"), "the caption is extractable");
    assert_eq!(text_searchability(&bytes), TextSearchability::Uncertain, "a caption beside a scan must not certify the scan as searchable");
    assert_eq!(crate::extract::extract_verdict_detail(&bytes).unverifiable_pages, vec![1]);
}

/// Any caption beside a dominant scan must not pass as searchable: accompanying text never
/// certifies picture content.
#[test]
fn a_dominant_scan_with_a_long_caption_is_uncertain() {
    let mut d = Document::new();
    let media = d.add_media(png(), "png");
    let mut scan = Paragraph::with_text("", CharProps::default());
    scan.insert_object(
        0,
        InlineObject::Image {
            media,
            w: 300.0,
            h: 400.0,
            alt: String::new(),
            float: Default::default(),
            crop: [0.0; 4],
            fx: Default::default(),
            ole: None,
        },
        &CharProps::default(),
    )
    .unwrap();
    let caption = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twentyone twentytwo twentythree twentyfour twentyfive";
    d.body = vec![para_block(scan), para_block(Paragraph::with_text(caption, CharProps::default()))];
    let bytes = export(&d, &PdfOptions::default()).unwrap();
    assert_eq!(text_searchability(&bytes), TextSearchability::Uncertain);
    let detail = crate::extract::extract_verdict_detail(&bytes);
    assert_eq!(detail.verdict, crate::extract::Verdict::Uncertain);
    assert_eq!(detail.unverifiable_pages, vec![1]);
}

/// A page dominated by a picture with text that is neither clearly its content nor clearly a
/// caption cannot be verified: the PDF is reported as uncertain, not as searchable.
#[test]
fn a_dominant_picture_with_some_text_is_uncertain() {
    let mut d = Document::new();
    let media = d.add_media(png(), "png");
    let mut scan = Paragraph::with_text("", CharProps::default());
    scan.insert_object(
        0,
        InlineObject::Image {
            media,
            w: 300.0,
            h: 400.0,
            alt: String::new(),
            float: Default::default(),
            crop: [0.0; 4],
            fx: Default::default(),
            ole: None,
        },
        &CharProps::default(),
    )
    .unwrap();
    // Ten words: more than a caption, less than a page's own text.
    d.body = vec![para_block(scan), para_block(Paragraph::with_text("one two three four five six seven eight nine ten", CharProps::default()))];
    let bytes = export(&d, &PdfOptions::default()).unwrap();
    assert_eq!(text_searchability(&bytes), TextSearchability::Uncertain);
    assert_eq!(text_searchability(&bytes).is_searchable(), None);
}

/// A small figure beside a caption is not a scan: the picture does not dominate the page, so the
/// caption is the page's own content and the PDF stays searchable.
#[test]
fn a_small_figure_with_a_caption_stays_searchable() {
    let mut d = Document::new();
    let media = d.add_media(png(), "png");
    let mut figure = Paragraph::with_text("Figure 1: ", CharProps::default());
    let n = figure.len();
    figure
        .insert_object(
            n,
            InlineObject::Image {
                media,
                w: 80.0,
                h: 40.0,
                alt: String::new(),
                float: Default::default(),
                crop: [0.0; 4],
                fx: Default::default(),
                ole: None,
            },
            &CharProps::default(),
        )
        .unwrap();
    let n = figure.len();
    figure.insert_text(n, "gradient", &CharProps::default()).unwrap();
    d.body = vec![para_block(figure)];
    let bytes = export(&d, &PdfOptions::default()).unwrap();
    assert_eq!(text_searchability(&bytes), TextSearchability::Searchable);
}

/// There is no minimum document length: a fully extractable one-line document (far under any
/// earlier character threshold) is searchable.
#[test]
fn a_short_text_document_is_searchable() {
    let d = Document::from_text("Agreed.");
    let bytes = export(&d, &PdfOptions::default()).unwrap();
    assert_eq!(text_searchability(&bytes), TextSearchability::Searchable);
}

/// Blank and furniture-only pages do not defeat coverage: a text page with an inserted blank page
/// after it is still text-searchable.
#[test]
fn a_blank_page_does_not_defeat_searchability() {
    let mut d = Document::new();
    let mut blank = Paragraph::with_text("", CharProps::default());
    blank.props.page_break_before = Some(true);
    d.body = vec![para_block(Paragraph::with_text("The only real text.", CharProps::default())), para_block(blank)];
    let bytes = export(&d, &PdfOptions::default()).unwrap();
    assert_eq!(extract_text(&bytes).len(), 2, "the blank page is its own page");
    assert_eq!(text_searchability(&bytes), TextSearchability::Searchable);
}

