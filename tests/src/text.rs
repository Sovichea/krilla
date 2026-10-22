use std::collections::BTreeMap;
use std::ops::Range;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use harfrust::{Direction, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use krilla::geom::Point;
use krilla::page::{Page, PageSettings};
use krilla::paint::{Fill, LinearGradient, Paint, SpreadMethod, Stroke};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph, PdfLogicalUnit, Tag, TextDirection};
use krilla::{Data, Document};
use krilla_macros::{snapshot, visreg};

use crate::{
    blue_fill, blue_stroke, red_fill, red_stroke, stops_with_3_solid_1, CANTARELL_VAR,
    LATIN_MODERN_ROMAN, LIBERTINUS_SERIF, NOTO_COLOR_EMOJI_COLR, NOTO_SANS, NOTO_SANS_ARABIC,
    NOTO_SANS_CJK, NOTO_SANS_DEVANAGARI, NOTO_SANS_VAR, TWITTER_COLOR_EMOJI,
};

fn text_gradient(spread_method: SpreadMethod) -> LinearGradient {
    LinearGradient {
        x1: 50.0,
        y1: 0.0,
        x2: 150.0,
        y2: 0.0,
        transform: Default::default(),
        spread_method,
        stops: stops_with_3_solid_1(),
        anti_alias: false,
    }
}

fn text_with_fill_impl(surface: &mut Surface, outlined: bool) {
    let font = Font::new(NOTO_SANS.clone(), 0).unwrap();
    surface.set_fill(Some(red_fill(0.5)));
    surface.draw_text(
        Point::from_xy(0.0, 80.0),
        font.clone(),
        20.0,
        "red outlined text",
        outlined,
        TextDirection::Auto,
    );

    surface.set_fill(Some(blue_fill(0.8)));
    surface.draw_text(
        Point::from_xy(0.0, 100.0),
        font.clone(),
        20.0,
        "blue outlined text",
        outlined,
        TextDirection::Auto,
    );

    let grad_fill = Fill {
        paint: Paint::from(text_gradient(SpreadMethod::Pad)),
        ..Default::default()
    };

    surface.set_fill(Some(grad_fill));
    surface.draw_text(
        Point::from_xy(0.0, 120.0),
        font.clone(),
        20.0,
        "gradient text",
        outlined,
        TextDirection::Auto,
    );

    let noto_font = Font::new(NOTO_COLOR_EMOJI_COLR.clone(), 0).unwrap();

    surface.set_fill(Some(blue_fill(0.8)));
    surface.draw_text(
        Point::from_xy(0.0, 140.0),
        noto_font.clone(),
        20.0,
        "😄😁😆",
        outlined,
        TextDirection::Auto,
    );

    let grad_fill = Fill {
        paint: Paint::from(text_gradient(SpreadMethod::Reflect)),
        ..Default::default()
    };

    surface.set_fill(Some(grad_fill));
    surface.draw_text(
        Point::from_xy(0.0, 160.0),
        font,
        20.0,
        "longer gradient text with repeat",
        outlined,
        TextDirection::Auto,
    );
}

#[visreg]
fn text_outlined_with_fill(surface: &mut Surface) {
    text_with_fill_impl(surface, true)
}

fn text_with_stroke_impl(surface: &mut Surface, outlined: bool) {
    let font = Font::new(NOTO_SANS.clone(), 0).unwrap();
    surface.set_stroke(Some(red_stroke(0.5, 1.0)));
    surface.draw_text(
        Point::from_xy(0.0, 80.0),
        font.clone(),
        20.0,
        "red outlined text",
        outlined,
        TextDirection::Auto,
    );

    surface.set_stroke(Some(blue_stroke(0.8)));
    surface.draw_text(
        Point::from_xy(0.0, 100.0),
        font.clone(),
        20.0,
        "blue outlined text",
        outlined,
        TextDirection::Auto,
    );

    let grad_stroke = Stroke {
        paint: Paint::from(text_gradient(SpreadMethod::Pad)),
        ..Default::default()
    };

    surface.set_stroke(Some(grad_stroke));
    surface.draw_text(
        Point::from_xy(0.0, 120.0),
        font,
        20.0,
        "gradient text",
        outlined,
        TextDirection::Auto,
    );

    let font = Font::new(NOTO_COLOR_EMOJI_COLR.clone(), 0).unwrap();

    surface.set_stroke(Some(blue_stroke(0.8)));
    surface.draw_text(
        Point::from_xy(0.0, 140.0),
        font,
        20.0,
        "😄😁😆",
        outlined,
        TextDirection::Auto,
    );
}

#[visreg]
fn text_outlined_with_stroke(surface: &mut Surface) {
    text_with_stroke_impl(surface, true);
}

#[visreg]
fn text_zalgo(surface: &mut Surface) {
    let font = Font::new(NOTO_SANS.clone(), 0).unwrap();
    surface.draw_text(
        Point::from_xy(0.0, 100.0),
        font,
        32.0,
        "z͈̤̭͖̉͑́a̳ͫ́̇͑̽͒ͯlͨ͗̍̀̍̔̀ģ͔̫̫̄o̗̠͔̦͆̏̓͢",
        false,
        TextDirection::Auto,
    );
}

#[visreg]
fn text_direction_ltr(surface: &mut Surface) {
    let font = Font::new(NOTO_SANS_CJK.clone(), 0).unwrap();
    surface.draw_text(
        Point::from_xy(0.0, 100.0),
        font,
        20.0,
        "你好这是一段则是文字",
        false,
        TextDirection::LeftToRight,
    );
}

#[visreg]
fn text_direction_rtl(surface: &mut Surface) {
    let font = Font::new(NOTO_SANS_CJK.clone(), 0).unwrap();
    surface.draw_text(
        Point::from_xy(0.0, 100.0),
        font,
        20.0,
        "你好这是一段则是文字",
        false,
        TextDirection::RightToLeft,
    );
}

#[visreg]
fn text_direction_ttb(surface: &mut Surface) {
    let font = Font::new(NOTO_SANS_CJK.clone(), 0).unwrap();
    surface.draw_text(
        Point::from_xy(100.0, 0.0),
        font,
        20.0,
        "你好这是一段则是文字",
        false,
        TextDirection::TopToBottom,
    );
}

#[visreg]
fn text_direction_btt(surface: &mut Surface) {
    let font = Font::new(NOTO_SANS_CJK.clone(), 0).unwrap();
    surface.draw_text(
        Point::from_xy(100.0, 0.0),
        font,
        20.0,
        "你好这是一段则是文字",
        false,
        TextDirection::BottomToTop,
    );
}

#[snapshot]
fn text_direction_auto(page: &mut Page) {
    let font = Font::new(NOTO_SANS_ARABIC.clone(), 0).unwrap();
    let mut surface = page.surface();
    surface.draw_text(
        Point::from_xy(0.0, 100.0),
        font,
        32.0,
        "مرحبا بالعالم",
        false,
        TextDirection::Auto,
    );
}

pub(crate) fn simple_text_impl(page: &mut Page, font_data: Data) {
    let mut surface = page.surface();
    surface.draw_text(
        Point::from_xy(0.0, 50.0),
        Font::new(font_data, 0).unwrap(),
        16.0,
        "A line of text.",
        false,
        TextDirection::Auto,
    );
}

#[snapshot]
fn text_simple_cff(page: &mut Page) {
    simple_text_impl(page, LATIN_MODERN_ROMAN.clone());
}

#[snapshot]
fn text_simple_ttf(page: &mut Page) {
    simple_text_impl(page, NOTO_SANS.clone());
}

#[snapshot]
fn text_complex(page: &mut Page) {
    let mut surface = page.surface();
    surface.draw_text(
        Point::from_xy(0.0, 50.0),
        Font::new(NOTO_SANS_DEVANAGARI.clone(), 0).unwrap(),
        16.0,
        "यह कुछ जटिल पाठ है.",
        false,
        TextDirection::Auto,
    );
}

#[snapshot]
fn text_complex_2(page: &mut Page) {
    let mut surface = page.surface();
    surface.draw_text(
        Point::from_xy(0.0, 50.0),
        Font::new(NOTO_SANS_DEVANAGARI.clone(), 0).unwrap(),
        16.0,
        "यु॒धा नर॑ ऋ॒ष्वा",
        false,
        TextDirection::Auto,
    );
}

#[snapshot]
fn text_complex_3(page: &mut Page) {
    let mut surface = page.surface();
    surface.draw_text(
        Point::from_xy(0.0, 50.0),
        Font::new(NOTO_SANS_DEVANAGARI.clone(), 0).unwrap(),
        12.0,
        "आ रु॒क्मैरा यु॒धा नर॑ ऋ॒ष्वा ऋ॒ष्टीर॑सृक्षत ।",
        false,
        TextDirection::Auto,
    );
}

#[snapshot]
fn text_complex_4(page: &mut Page) {
    let mut surface = page.surface();
    surface.draw_text(
        Point::from_xy(0.0, 50.0),
        Font::new(NOTO_SANS_DEVANAGARI.clone(), 0).unwrap(),
        10.0,
        "अन्वे॑नाँ॒ अह॑ वि॒द्युतो॑ म॒रुतो॒ जज्झ॑तीरव भनर॑र्त॒ त्मना॑ दि॒वः ॥",
        false,
        TextDirection::Auto,
    );
}

#[snapshot]
// Tests https://github.com/typst/typst/issues/5654
fn text_small_caps(page: &mut Page) {
    let glyphs = vec![
        KrillaGlyph {
            glyph_id: GlyphId::new(2464),
            text_range: 0..1,
            x_advance: 0.529,
            x_offset: 0.0,
            y_offset: 0.0,
            y_advance: 0.0,
            location: None,
        },
        KrillaGlyph {
            glyph_id: GlyphId::new(2464),
            text_range: 1..2,
            x_advance: 0.529,
            x_offset: 0.0,
            y_offset: 0.0,
            y_advance: 0.0,
            location: None,
        },
    ];

    let mut surface = page.surface();
    surface.draw_glyphs(
        Point::from_xy(0.0, 50.0),
        &glyphs,
        Font::new(LIBERTINUS_SERIF.clone(), 0).unwrap(),
        "Tt",
        12.0,
        false,
    );
}

#[visreg]
fn text_zalgo_outlined(surface: &mut Surface) {
    let font = Font::new(NOTO_SANS.clone(), 0).unwrap();
    surface.draw_text(
        Point::from_xy(0.0, 100.0),
        font,
        32.0,
        "z͈̤̭͖̉͑́a̳ͫ́̇͑̽͒ͯlͨ͗̍̀̍̔̀ģ͔̫̫̄o̗̠͔̦͆̏̓͢",
        true,
        TextDirection::Auto,
    );
}

#[snapshot]
fn text_fill(page: &mut Page) {
    let mut surface = page.surface();
    surface.draw_text(
        Point::from_xy(0.0, 50.0),
        Font::new(NOTO_SANS.clone(), 0).unwrap(),
        16.0,
        "hi there",
        false,
        TextDirection::Auto,
    );
}

#[snapshot]
fn text_stroke(page: &mut Page) {
    let mut surface = page.surface();
    surface.set_stroke(Some(Stroke::default()));
    surface.draw_text(
        Point::from_xy(0.0, 50.0),
        Font::new(NOTO_SANS.clone(), 0).unwrap(),
        16.0,
        "hi there",
        false,
        TextDirection::Auto,
    );
}

// This would be nicer as a snapshot test, but since it's a system font
// we can't include it in the repository.
// The point of the test is to check that fonts that do have a bitmap table
// will still embed a CID font for glyphs that don't have an entry in the
// bitmap table instead of falling back to a Type3 font.
#[cfg(target_os = "macos")]
#[visreg]
fn text_mixed_ttf_ebdt_font(surface: &mut Surface) {
    let data = std::fs::read("/System/Library/Fonts/Supplemental/PTSans.ttc").unwrap();
    let font = Font::new(data.into(), 0).unwrap();
    surface.draw_text(
        Point::from_xy(0.0, 100.0),
        font,
        32.0,
        "Hi!",
        false,
        TextDirection::Auto,
    );
}

// See https://github.com/typst/typst/pull/5420#issuecomment-2768899483.
// Make sure snapshot is stable.
#[test]
fn text_two_fonts_reproducibility() {
    let render_single = || {
        let mut document = Document::new();
        let mut page = document.start_page();
        let mut surface = page.surface();

        surface.draw_text(
            Point::from_xy(0.0, 50.0),
            Font::new(NOTO_SANS.clone(), 0).unwrap(),
            16.0,
            "hi there",
            false,
            TextDirection::Auto,
        );
        surface.draw_text(
            Point::from_xy(0.0, 20.0),
            Font::new(NOTO_SANS_CJK.clone(), 0).unwrap(),
            16.0,
            "你好",
            false,
            TextDirection::Auto,
        );

        surface.finish();
        page.finish();
        document.finish().unwrap()
    };

    let expected = render_single();

    for _ in 0..10 {
        assert_eq!(expected, render_single());
    }
}

fn variable_impl(surface: &mut Surface, coords: Vec<Vec<(Tag, f32)>>, font: Data, text: &str) {
    let mut cur_y = 20.0;

    for coords in coords {
        let font = Font::new_variable(font.clone(), 0, &coords).unwrap();

        surface.draw_text(
            Point::from_xy(0.0, cur_y),
            font,
            16.0,
            text,
            false,
            TextDirection::Auto,
        );

        cur_y += 20.0;
    }
}

#[visreg]
fn text_variable_font(surface: &mut Surface) {
    let coords = vec![
        vec![(Tag::new(b"wght"), 400.0)],
        vec![(Tag::new(b"wght"), 100.0)],
        vec![(Tag::new(b"wght"), 900.0)],
        vec![(Tag::new(b"wght"), 900.0), (Tag::new(b"wdth"), 62.5)],
    ];

    variable_impl(
        surface,
        coords,
        NOTO_SANS_VAR.clone(),
        "I love variable fonts!",
    );
}

#[visreg]
fn text_variable_font_cff2(surface: &mut Surface) {
    let coords = vec![
        vec![(Tag::new(b"wght"), 400.0)],
        vec![(Tag::new(b"wght"), 100.0)],
        vec![(Tag::new(b"wght"), 900.0)],
    ];

    variable_impl(
        surface,
        coords,
        CANTARELL_VAR.clone(),
        "I love variable fonts!",
    );
}

static EXTRACTION_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn extract_logical_pdf(pdf: &[u8], raw: bool) -> Option<String> {
    let id = EXTRACTION_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "krilla-logical-extraction-{}-{id}.pdf",
        std::process::id()
    ));
    std::fs::write(&path, pdf).unwrap();

    let mut command = Command::new("pdftotext");
    command.arg("-enc").arg("UTF-8");
    if raw {
        command.arg("-raw");
    }
    let output = command.arg(&path).arg("-").output();
    let _ = std::fs::remove_file(&path);

    match output {
        Ok(output) if output.status.success() => {
            Some(String::from_utf8(output.stdout).expect("pdftotext returned invalid UTF-8"))
        }
        Ok(output) => panic!(
            "pdftotext failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("failed to run pdftotext: {error}"),
    }
}

fn normalize_logical_text(text: &str) -> String {
    let text = text
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '\u{061C}'
                    | '\u{200E}'
                    | '\u{200F}'
                    | '\u{202A}'..='\u{202E}'
                    | '\u{2066}'..='\u{2069}'
            )
        })
        .collect::<String>();
    text.trim_end_matches(['\n', '\r']).to_owned()
}

fn logical_pages(pdf: &[u8], raw: bool) -> Option<Vec<String>> {
    let text = extract_logical_pdf(pdf, raw)?;
    Some(
        text.split('\u{000C}')
            .map(normalize_logical_text)
            .filter(|page| !page.is_empty())
            .collect(),
    )
}

fn unshaped_logical_pdf(cases: &[(&str, bool)]) -> Vec<u8> {
    let font = Font::new(NOTO_SANS.clone(), 0).unwrap();
    let mut document = Document::new();
    for (text, rtl) in cases {
        let mut page =
            document.start_page_with(PageSettings::from_wh(400.0, 120.0).expect("valid page"));
        let mut surface = page.surface();
        let mut glyphs = Vec::new();
        let mut positions = Vec::new();
        for (index, (start, ch)) in text.char_indices().enumerate() {
            let range = start..start + ch.len_utf8();
            glyphs.push(vec![KrillaGlyph::new(
                GlyphId::new(36),
                0.6,
                0.0,
                0.0,
                0.0,
                range.clone(),
                None,
            )]);
            let x = if *rtl {
                (text.chars().count() - 1 - index) as f32 * 0.6
            } else {
                index as f32 * 0.6
            };
            positions.push((range, x));
        }
        let units = positions
            .iter()
            .enumerate()
            .map(|(index, (range, x))| {
                PdfLogicalUnit::new(&text[range.clone()], &glyphs[index], *x, 0.0)
            })
            .collect::<Vec<_>>();
        surface.draw_pdf_logical_units(
            Point::from_xy(20.0, 60.0),
            &units,
            font.clone(),
            24.0,
            false,
        );
        surface.finish();
        page.finish();
    }
    document.finish().unwrap()
}

fn cluster_logical_pdf(cases: &[&str]) -> Vec<u8> {
    let font = Font::new(NOTO_SANS.clone(), 0).unwrap();
    let mut document = Document::new();
    for text in cases {
        let mut page =
            document.start_page_with(PageSettings::from_wh(400.0, 120.0).expect("valid page"));
        let mut surface = page.surface();
        let glyphs = vec![KrillaGlyph::new(
            GlyphId::new(36),
            0.6,
            0.0,
            0.0,
            0.0,
            0..text.len(),
            None,
        )];
        let units = [PdfLogicalUnit::new(text, &glyphs, 0.0, 0.0)];
        surface.draw_pdf_logical_units(
            Point::from_xy(20.0, 60.0),
            &units,
            font.clone(),
            24.0,
            false,
        );
        surface.finish();
        page.finish();
    }
    document.finish().unwrap()
}

#[test]
fn logical_units_preserve_multi_codepoint_clusters() {
    const CASES: &[&str] = &[
        "e\u{301}a\u{308}",
        "हिन्दी",
        "क्ष",
        "বাংলা",
        "தமிழ்",
        "ಕನ್ನಡ",
        "ഭാഷ",
        "භාෂාව",
        "ພາສາ",
        "ភាសាខ្មែរ",
        "မြန်မာ",
        "བོད་སྐད",
        "😀👨‍👩‍👧‍👦",
    ];

    let pdf = cluster_logical_pdf(CASES);
    let Some(pages) = logical_pages(&pdf, false) else {
        return;
    };

    assert_eq!(pages.len(), CASES.len());
    for (page, expected) in pages.iter().zip(CASES) {
        assert_eq!(page, expected);
    }
}

#[test]
fn logical_units_preserve_unicode_for_all_scripts() {
    const CASES: &[(&str, bool)] = &[
        ("Café naïve coöperate", false),
        ("e\u{301}a\u{308}", false),
        ("مرحبا بالعالم", true),
        ("مُحَمَّد", true),
        ("שלום עולם", true),
        ("हिन्दी और संस्कृत", false),
        ("বাংলা ভাষা", false),
        ("தமிழ் மொழி", false),
        ("తెలుగు భాష", false),
        ("ಕನ್ನಡ ಭಾಷೆ", false),
        ("മലയാളം ഭാഷ", false),
        ("ગુજરાતી ભાષા", false),
        ("ਪੰਜਾਬੀ ਭਾਸ਼ਾ", false),
        ("සිංහල භාෂාව", false),
        ("ภาษาไทย", false),
        ("ພາສາລາວ", false),
        ("ភាសាខ្មែរ", false),
        ("မြန်မာဘာသာ", false),
        ("བོད་སྐད", false),
        ("中文 日本語 한국어", false),
        ("😀👨‍👩‍👧‍👦", false),
    ];

    let pdf = unshaped_logical_pdf(CASES);
    let Some(pages) = logical_pages(&pdf, false) else {
        return;
    };

    assert_eq!(pages.len(), CASES.len());
    for (page, (expected, _)) in pages.iter().zip(CASES) {
        assert_eq!(page, expected);
    }
}

fn shape_logical_units(
    text: &str,
    bytes: &[u8],
    upem: f32,
    direction: Direction,
) -> Vec<(Range<usize>, Vec<KrillaGlyph>, f32, f32)> {
    let font_ref = FontRef::from_index(bytes, 0).expect("invalid font");
    let shaper_data = ShaperData::new(&font_ref);
    let shaper = shaper_data.shaper(&font_ref).build();

    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    buffer.set_direction(direction);
    let output = shaper.shape(buffer, ShapeOptions::default());

    let infos = output.glyph_infos();
    let positions = output.glyph_positions();

    let mut starts: Vec<usize> = infos.iter().map(|info| info.cluster as usize).collect();
    starts.sort_unstable();
    starts.dedup();
    let ranges: BTreeMap<usize, Range<usize>> = starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts.get(index + 1).copied().unwrap_or(text.len());
            (*start, *start..end)
        })
        .collect();

    let mut groups = BTreeMap::<usize, (Range<usize>, Vec<KrillaGlyph>, f32, f32)>::new();
    let (mut pen_x, mut pen_y) = (0.0_f32, 0.0_f32);
    for (info, pos) in infos.iter().zip(positions.iter()) {
        let cluster = info.cluster as usize;
        let range = ranges[&cluster].clone();
        let group = groups
            .entry(cluster)
            .or_insert_with(|| (range.clone(), Vec::new(), pen_x, pen_y));
        group.1.push(KrillaGlyph::new(
            GlyphId::new(info.glyph_id),
            pos.x_advance as f32 / upem,
            pos.x_offset as f32 / upem,
            pos.y_offset as f32 / upem,
            pos.y_advance as f32 / upem,
            range,
            None,
        ));
        pen_x += pos.x_advance as f32 / upem;
        pen_y += pos.y_advance as f32 / upem;
    }
    groups.into_values().collect()
}

fn shaped_logical_pdf(text: &str, data: Data, direction: Direction) -> Vec<u8> {
    let font = Font::new(data.clone(), 0).unwrap();
    let mut document = Document::new();
    let mut page =
        document.start_page_with(PageSettings::from_wh(1200.0, 400.0).expect("valid page"));
    let mut surface = page.surface();
    let units = shape_logical_units(text, data.as_ref(), font.units_per_em(), direction);
    let logical_units = units
        .iter()
        .map(|(range, glyphs, visual_x, visual_y)| {
            PdfLogicalUnit::new(&text[range.clone()], glyphs, *visual_x, *visual_y)
        })
        .collect::<Vec<_>>();
    surface.draw_pdf_logical_units(
        Point::from_xy(40.0, 100.0),
        &logical_units,
        font.clone(),
        28.0,
        false,
    );
    surface.finish();
    page.finish();
    document.finish().unwrap()
}

#[test]
fn logical_units_preserve_shaped_arabic_and_devanagari() {
    let arabic = "هذا هو سطر عربي بسيط";
    let devanagari = "हिन्दी और अन्य भारतीय लिपियों में अक्षरों की आकृति संदर्भ के अनुसार बदल सकती है।";

    let arabic_pdf = shaped_logical_pdf(
        arabic,
        crate::NOTO_SANS_ARABIC.clone(),
        Direction::RightToLeft,
    );
    let devanagari_pdf = shaped_logical_pdf(
        devanagari,
        crate::NOTO_SANS_DEVANAGARI.clone(),
        Direction::LeftToRight,
    );

    let Some(arabic_pages) = logical_pages(&arabic_pdf, false) else {
        return;
    };
    let Some(devanagari_pages) = logical_pages(&devanagari_pdf, false) else {
        return;
    };

    assert_eq!(arabic_pages, vec![arabic]);
    assert_eq!(devanagari_pages, vec![devanagari]);
}
