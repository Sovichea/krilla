use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use harfrust::{BufferClusterLevel, Direction, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use krilla::geom::Point;
use krilla::page::PageSettings;
use krilla::text::{Font, Glyph, GlyphId, KrillaGlyph, PdfLogicalUnit};
use krilla::Document;

const PAGE_WIDTH: f32 = 1800.0;
const PAGE_HEIGHT: f32 = 1800.0;
const FONT_SIZE: f32 = 18.0;
const LINES_PER_PAGE: usize = 20;
const FIRST_LINE_Y: f32 = 60.0;
const LINE_GAP: f32 = 70.0;

#[derive(Clone)]
struct OwnedUnit {
    range: Range<usize>,
    glyphs: Vec<KrillaGlyph>,
    visual_x: f32,
    visual_y: f32,
}

struct ShapedLine {
    units: Vec<OwnedUnit>,
    glyphs: Vec<KrillaGlyph>,
}

fn load_font(path: &str) -> (Vec<u8>, Font) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("failed to read font {path}: {e}"));
    let font = Font::new(Arc::new(bytes.clone()).into(), 0)
        .unwrap_or_else(|| panic!("failed to load font {path}"));
    (bytes, font)
}

fn shape_line(text: &str, bytes: &[u8], upem: f32) -> ShapedLine {
    let font_ref = FontRef::from_index(bytes, 0).expect("invalid font");
    let shaper_data = ShaperData::new(&font_ref);
    let shaper = shaper_data.shaper(&font_ref).build();

    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    buffer.set_direction(Direction::LeftToRight);
    buffer.set_cluster_level(BufferClusterLevel::MonotoneCharacters);
    let output = shaper.shape(buffer, ShapeOptions::default());

    let infos = output.glyph_infos();
    let positions = output.glyph_positions();

    let mut starts: Vec<usize> = infos.iter().map(|info| info.cluster as usize).collect();
    starts.sort_unstable();
    starts.dedup();
    let ranges: BTreeMap<usize, Range<usize>> = starts
        .iter()
        .enumerate()
        .map(|(i, start)| {
            let end = starts.get(i + 1).copied().unwrap_or(text.len());
            (*start, *start..end)
        })
        .collect();

    let mut groups = BTreeMap::<usize, OwnedUnit>::new();
    let mut flat = Vec::with_capacity(infos.len());
    let (mut pen_x, mut pen_y) = (0.0_f32, 0.0_f32);

    for (info, pos) in infos.iter().zip(positions.iter()) {
        let cluster = info.cluster as usize;
        let range = ranges[&cluster].clone();
        let x_advance = pos.x_advance as f32 / upem;
        let y_advance = pos.y_advance as f32 / upem;
        let x_offset = pos.x_offset as f32 / upem;
        let y_offset = pos.y_offset as f32 / upem;

        let glyph = KrillaGlyph::new(
            GlyphId::new(info.glyph_id),
            x_advance,
            x_offset,
            y_offset,
            y_advance,
            range.clone(),
            None,
        );
        flat.push(glyph.clone());

        let group = groups.entry(cluster).or_insert_with(|| OwnedUnit {
            range: range.clone(),
            glyphs: Vec::new(),
            visual_x: pen_x,
            visual_y: pen_y,
        });
        group.glyphs.push(glyph);

        pen_x += x_advance;
        pen_y += y_advance;
    }

    ShapedLine {
        units: groups.into_values().collect(),
        glyphs: flat,
    }
}

fn realistic_unique_count(text: &str, shaped: &ShapedLine, upem: f32) -> usize {
    let mut set = HashSet::new();
    for unit in &shaped.units {
        let mut s = text[unit.range.clone()].to_owned();
        s.push('|');
        let mut pen_x = 0.0_f32;
        let mut pen_y = 0.0_f32;
        for g in &unit.glyphs {
            let x = ((pen_x + g.x_offset(1.0)) * upem).round() as i32;
            let y = ((pen_y + g.y_offset(1.0)) * upem).round() as i32;
            s.push_str(&format!("{}:{x}:{y};", g.glyph_id().to_u32()));
            pen_x += g.x_advance(1.0);
            pen_y += g.y_advance(1.0);
        }
        set.insert(s);
    }
    set.len()
}

fn build_realistic(mode: &str, pages: usize, text: &str, font: &Font, shaped: &ShapedLine) -> (Vec<u8>, f64, f64) {
    let build_start = Instant::now();
    let mut document = Document::new();

    for _ in 0..pages {
        let mut page = document.start_page_with(
            PageSettings::from_wh(PAGE_WIDTH, PAGE_HEIGHT).expect("valid page settings"),
        );
        let mut surface = page.surface();
        for line in 0..LINES_PER_PAGE {
            let y = FIRST_LINE_Y + line as f32 * LINE_GAP;
            match mode {
                "normal-khmer" => surface.draw_glyphs(
                    Point::from_xy(40.0, y),
                    &shaped.glyphs,
                    font.clone(),
                    text,
                    FONT_SIZE,
                    false,
                ),
                "logical-khmer" => {
                    let units: Vec<_> = shaped
                        .units
                        .iter()
                        .map(|u| PdfLogicalUnit::new(&text[u.range.clone()], &u.glyphs, u.visual_x, u.visual_y))
                        .collect();
                    surface.draw_pdf_logical_units(
                        Point::from_xy(40.0, y),
                        &units,
                        font.clone(),
                        FONT_SIZE,
                        false,
                    );
                }
                _ => unreachable!(),
            }
        }
        surface.finish();
        page.finish();
    }
    let build_ms = build_start.elapsed().as_secs_f64() * 1000.0;

    let finish_start = Instant::now();
    let pdf = document.finish().expect("finish PDF");
    let finish_ms = finish_start.elapsed().as_secs_f64() * 1000.0;
    (pdf, build_ms, finish_ms)
}

fn first_glyph(bytes: &[u8], font: &Font) -> KrillaGlyph {
    let shaped = shape_line("ក", bytes, font.units_per_em());
    shaped.glyphs[0].clone()
}

fn build_unique(mode: &str, count: usize, bytes: &[u8], font: &Font) -> (Vec<u8>, f64, f64) {
    const PER_PAGE: usize = 100;
    let source_glyph = first_glyph(bytes, font);
    let advance = source_glyph.x_advance(1.0).max(0.2);

    let texts: Vec<String> = (0..count)
        .map(|i| {
            // Supplementary-plane scalar, one unique Unicode value per unit.
            let cp = 0x10000 + i as u32;
            char::from_u32(cp).unwrap_or('\u{FFFD}').to_string()
        })
        .collect();

    let build_start = Instant::now();
    let mut document = Document::new();

    for chunk_start in (0..count).step_by(PER_PAGE) {
        let chunk_end = (chunk_start + PER_PAGE).min(count);
        let mut page = document.start_page_with(
            PageSettings::from_wh(PAGE_WIDTH, PAGE_HEIGHT).expect("valid page settings"),
        );
        let mut surface = page.surface();

        match mode {
            "logical-unique" => {
                // Keep one glyph vector per unit so the public API sees a normal borrowed slice.
                let glyph_storage: Vec<Vec<KrillaGlyph>> = (chunk_start..chunk_end)
                    .map(|_| vec![source_glyph.clone()])
                    .collect();
                let units: Vec<_> = (chunk_start..chunk_end)
                    .enumerate()
                    .map(|(local, global)| {
                        PdfLogicalUnit::new(
                            &texts[global],
                            &glyph_storage[local],
                            local as f32 * advance,
                            0.0,
                        )
                    })
                    .collect();
                surface.draw_pdf_logical_units(
                    Point::from_xy(30.0, 100.0),
                    &units,
                    font.clone(),
                    FONT_SIZE,
                    false,
                );
            }
            "normal-unique-baseline" => {
                let count_here = chunk_end - chunk_start;
                let mut source_text = String::new();
                for _ in 0..count_here {
                    source_text.push('ក');
                }
                let mut glyphs = Vec::with_capacity(count_here);
                for (local, (start, ch)) in source_text.char_indices().enumerate() {
                    let end = start + ch.len_utf8();
                    let mut g = source_glyph.clone();
                    g.text_range = start..end;
                    // The glyph's own advance naturally positions each occurrence.
                    let _ = local;
                    glyphs.push(g);
                }
                surface.draw_glyphs(
                    Point::from_xy(30.0, 100.0),
                    &glyphs,
                    font.clone(),
                    &source_text,
                    FONT_SIZE,
                    false,
                );
            }
            _ => unreachable!(),
        }

        surface.finish();
        page.finish();
    }
    let build_ms = build_start.elapsed().as_secs_f64() * 1000.0;

    let finish_start = Instant::now();
    let pdf = document.finish().expect("finish PDF");
    let finish_ms = finish_start.elapsed().as_secs_f64() * 1000.0;
    (pdf, build_ms, finish_ms)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: pdf_logical_bench MODE SCALE FONT");
        eprintln!("modes: normal-khmer logical-khmer normal-unique-baseline logical-unique");
        std::process::exit(2);
    }
    let mode = &args[1];
    let scale: usize = args[2].parse().expect("SCALE must be an integer");
    let font_path = &args[3];

    let total_start = Instant::now();
    let load_start = Instant::now();
    let (bytes, font) = load_font(font_path);
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;

    let khmer_text = "កម្ពុជាមានអក្សរខ្មែរដែលត្រូវការការរៀបចំសញ្ញា និងជើងអក្សរច្រើន។ ខ្ញុំសរសេរភាសាខ្មែរ ហើយចង់ឱ្យអត្ថបទក្នុង PDF ចម្លងបានត្រឹមត្រូវ។";

    let prep_start = Instant::now();
    let (pdf, build_ms, finish_ms, unique_units, occurrences) = match mode.as_str() {
        "normal-khmer" | "logical-khmer" => {
            let shaped = shape_line(khmer_text, &bytes, font.units_per_em());
            let unique = realistic_unique_count(khmer_text, &shaped, font.units_per_em());
            let occurrences = shaped.units.len() * LINES_PER_PAGE * scale;
            let prep_ms = prep_start.elapsed().as_secs_f64() * 1000.0;
            let (pdf, build_ms, finish_ms) = build_realistic(mode, scale, khmer_text, &font, &shaped);
            println!("prep_ms={prep_ms:.3}");
            (pdf, build_ms, finish_ms, unique, occurrences)
        }
        "normal-unique-baseline" | "logical-unique" => {
            let prep_ms = prep_start.elapsed().as_secs_f64() * 1000.0;
            let (pdf, build_ms, finish_ms) = build_unique(mode, scale, &bytes, &font);
            println!("prep_ms={prep_ms:.3}");
            let unique = if mode == "logical-unique" { scale } else { 1 };
            (pdf, build_ms, finish_ms, unique, scale)
        }
        _ => panic!("unknown mode {mode}"),
    };

    let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
    println!("mode={mode}");
    println!("scale={scale}");
    println!("load_ms={load_ms:.3}");
    println!("build_ms={build_ms:.3}");
    println!("finish_ms={finish_ms:.3}");
    println!("total_ms={total_ms:.3}");
    println!("pdf_bytes={}", pdf.len());
    println!("unique_units={unique_units}");
    println!("unit_occurrences={occurrences}");
    std::hint::black_box(pdf);
}
