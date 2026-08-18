use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use harfrust::{Direction, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use krilla::geom::Point;
use krilla::page::PageSettings;
use krilla::text::{Font, GlyphId, KrillaGlyph, PdfLogicalUnit};
use krilla::Document;

const PAGE_WIDTH: f32 = 1400.0;
const PAGE_HEIGHT: f32 = 520.0;
const FONT_SIZE: f32 = 22.0;
const LTR_X: f32 = 50.0;
const RTL_X: f32 = 50.0;
const FIRST_LINE_Y: f32 = 90.0;
const LINE_GAP: f32 = 72.0;

struct OwnedUnit {
    range: Range<usize>,
    glyphs: Vec<KrillaGlyph>,
    visual_x: f32,
    visual_y: f32,
}

struct LoadedFont {
    bytes: Vec<u8>,
    font: Font,
}

impl LoadedFont {
    fn load(path: &str) -> Self {
        let bytes = std::fs::read(path)
            .unwrap_or_else(|err| panic!("failed to read font {path}: {err}"));
        let font = Font::new(Arc::new(bytes.clone()).into(), 0)
            .unwrap_or_else(|| panic!("failed to load font {path}"));
        Self { bytes, font }
    }
}

fn shape_units(text: &str, bytes: &[u8], upem: f32, direction: Direction) -> Vec<OwnedUnit> {
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

    // HarfRust clusters are byte offsets into `text`. Build one source range for
    // every distinct cluster so each PdfLogicalUnit keeps the original Unicode.
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

    // BTreeMap keeps the final units in logical source order. visual_x/visual_y
    // keep the positions produced by shaping, which is important for RTL text.
    let mut groups = BTreeMap::<usize, OwnedUnit>::new();
    let (mut pen_x, mut pen_y) = (0.0_f32, 0.0_f32);

    for (info, pos) in infos.iter().zip(positions.iter()) {
        let cluster = info.cluster as usize;
        let range = ranges[&cluster].clone();
        let x_advance = pos.x_advance as f32 / upem;
        let y_advance = pos.y_advance as f32 / upem;
        let x_offset = pos.x_offset as f32 / upem;
        let y_offset = pos.y_offset as f32 / upem;

        let group = groups.entry(cluster).or_insert_with(|| OwnedUnit {
            range: range.clone(),
            glyphs: Vec::new(),
            visual_x: pen_x,
            visual_y: pen_y,
        });

        group.glyphs.push(KrillaGlyph::new(
            GlyphId::new(info.glyph_id),
            x_advance,
            x_offset,
            y_offset,
            y_advance,
            range,
            None,
        ));

        pen_x += x_advance;
        pen_y += y_advance;
    }

    groups.into_values().collect()
}

fn draw_line(
    surface: &mut krilla::surface::Surface<'_>,
    loaded: &LoadedFont,
    text: &str,
    direction: Direction,
    x: f32,
    y: f32,
) {
    let owned = shape_units(text, &loaded.bytes, loaded.font.units_per_em(), direction);
    let units: Vec<_> = owned
        .iter()
        .map(|unit| {
            PdfLogicalUnit::new(
                &text[unit.range.clone()],
                &unit.glyphs,
                unit.visual_x,
                unit.visual_y,
            )
        })
        .collect();

    surface.draw_pdf_logical_units(
        Point::from_xy(x, y),
        &units,
        loaded.font.clone(),
        FONT_SIZE,
        false,
    );
}

fn add_page(
    document: &mut Document,
    loaded: &LoadedFont,
    lines: &[&str],
    direction: Direction,
) {
    let mut page = document.start_page_with(
        PageSettings::from_wh(PAGE_WIDTH, PAGE_HEIGHT).expect("valid page settings"),
    );
    let mut surface = page.surface();
    let x = if direction == Direction::RightToLeft {
        RTL_X
    } else {
        LTR_X
    };

    for (index, line) in lines.iter().enumerate() {
        let y = FIRST_LINE_Y + index as f32 * LINE_GAP;
        draw_line(&mut surface, loaded, line, direction, x, y);
    }

    surface.finish();
    page.finish();
}

fn main() {
    let mut args = std::env::args().skip(1);
    let output_path = args.next().unwrap_or_else(|| {
        panic!(
            "usage: pdf_logical_demo OUTPUT KHMER_FONT DEVANAGARI_FONT ARABIC_FONT THAI_FONT"
        )
    });
    let khmer_path = args.next().expect("missing KHMER_FONT");
    let devanagari_path = args.next().expect("missing DEVANAGARI_FONT");
    let arabic_path = args.next().expect("missing ARABIC_FONT");
    let thai_path = args.next().expect("missing THAI_FONT");

    if args.next().is_some() {
        panic!(
            "usage: pdf_logical_demo OUTPUT KHMER_FONT DEVANAGARI_FONT ARABIC_FONT THAI_FONT"
        );
    }

    let khmer = LoadedFont::load(&khmer_path);
    let devanagari = LoadedFont::load(&devanagari_path);
    let arabic = LoadedFont::load(&arabic_path);
    let thai = LoadedFont::load(&thai_path);

    let mut document = Document::new();

    // Khmer: multi-glyph clusters, split vowels, subscript forms, and marks.
    let khmer_lines = [
        "កម្ពុជាមានអក្សរខ្មែរដែលត្រូវការការរៀបចំសញ្ញា និងជើងអក្សរច្រើន។",
        "ខ្ញុំសរសេរភាសាខ្មែរ ហើយចង់ឱ្យអត្ថបទក្នុង PDF ចម្លងបានត្រឹមត្រូវ។",
        "ការបង្ហាញត្រូវតែត្រឹមត្រូវ ហើយ Unicode ដែលចម្លងចេញក៏ត្រូវរក្សាដូចដើម។",
        "កម្ពុជា ខ្ញុំសរសេរភាសាខ្មែរ។",
    ];
    add_page(
        &mut document,
        &khmer,
        &khmer_lines,
        Direction::LeftToRight,
    );

    // Devanagari: conjuncts, reordered matras, and combining marks.
    let devanagari_lines = [
        "हिन्दी और अन्य भारतीय लिपियों में अक्षरों की आकृति संदर्भ के अनुसार बदल सकती है।",
        "संयुक्त अक्षर, मात्राएँ और पुनःक्रमित चिह्न PDF में सही दिखने के साथ सही तरह से कॉपी भी होने चाहिए।",
        "यह परीक्षण मूल यूनिकोड पाठ को आकार देने के बाद भी सुरक्षित रखने की कोशिश करता है।",
        "कर्म क्षेत्र दृष्टि शक्ति क्षमा हिन्दी संस्कृति परीक्षण।",
    ];
    add_page(
        &mut document,
        &devanagari,
        &devanagari_lines,
        Direction::LeftToRight,
    );

    // Arabic: logical order is kept in PdfLogicalUnit while shaped visual
    // positions run right-to-left.
    let arabic_lines = [
        "هذا هو سطر عربي بسيط",
        "النص العربي يكتب من اليمين لليسار",
        "نريد نسخ النص العربي بشكل صحيح",
        "هذه جملة عربية للتجربة مع كلمات كثيرة",
    ];   add_page(
        &mut document,
        &arabic,
        &arabic_lines,
        Direction::RightToLeft,
    );

    // Thai / Krilla #411.
    //
    // The original reproducer compares standalone sara-aa (ส + า) with
    // sara-am (ต + ำ). Sara-am shapes to nikhahit + sara-aa and can reuse the
    // same sara-aa source glyph. Viewers that ignore /ActualText can therefore
    // copy an extra า with Krilla's current glyph-based mapping.
    //
    // Issue: https://github.com/LaurenzV/krilla/issues/411
    let thai_lines = [
        "ภาษาไทยมีสระและวรรณยุกต์ที่ต้องจัดวางร่วมกับพยัญชนะอย่างถูกต้อง",
        "สา : standalone sara-aa (ส + า)",
        "ตำ : issue #411 sara-am (ต + ำ)",
        "ตำ ตำรา น้ำ ทำ กำลัง สำคัญ คำ และสำหรับ เป็นคำที่ใช้สระอำหลายครั้ง",
        "ข้อความควรดูถูกต้องใน PDF และคัดลอกกลับมาเป็น Unicode เดิมได้",
    ];
    add_page(
        &mut document,
        &thai,
        &thai_lines,
        Direction::LeftToRight,
    );

    std::fs::write(&output_path, document.finish().expect("failed to finish PDF"))
        .unwrap_or_else(|err| panic!("failed to write {output_path}: {err}"));

    println!("wrote {output_path}");
}
