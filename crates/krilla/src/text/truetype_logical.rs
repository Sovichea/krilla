use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::error::{KrillaError, KrillaResult};
use crate::text::logical::LogicalUnitKey;
use crate::text::Font;

const HEAD: [u8; 4] = *b"head";
const HHEA: [u8; 4] = *b"hhea";
const HMTX: [u8; 4] = *b"hmtx";
const LOCA: [u8; 4] = *b"loca";
const GLYF: [u8; 4] = *b"glyf";
const MAXP: [u8; 4] = *b"maxp";
const POST: [u8; 4] = *b"post";
const CHECKSUM_MAGIC: u32 = 0xB1B0_AFBA;

const ARG_WORDS: u16 = 0x0001;
const ARGS_XY: u16 = 0x0002;
const ROUND_XY: u16 = 0x0004;
const HAVE_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const HAVE_XY_SCALE: u16 = 0x0040;
const HAVE_2X2: u16 = 0x0080;

#[derive(Clone, Debug)]
pub(crate) struct SyntheticLogicalGlyph {
    pub(crate) virtual_gid: u16,
    pub(crate) key: LogicalUnitKey,
}

#[derive(Clone, Copy, Debug, Default)]
struct BBox {
    x_min: i16,
    y_min: i16,
    x_max: i16,
    y_max: i16,
}

impl BBox {
    fn union(self, other: Self) -> Self {
        Self {
            x_min: self.x_min.min(other.x_min),
            y_min: self.y_min.min(other.y_min),
            x_max: self.x_max.max(other.x_max),
            y_max: self.y_max.max(other.y_max),
        }
    }

    fn translated(self, x: i16, y: i16, font: &Font) -> KrillaResult<Self> {
        Ok(Self {
            x_min: checked_i16(i32::from(self.x_min) + i32::from(x), font)?,
            y_min: checked_i16(i32::from(self.y_min) + i32::from(y), font)?,
            x_max: checked_i16(i32::from(self.x_max) + i32::from(x), font)?,
            y_max: checked_i16(i32::from(self.y_max) + i32::from(y), font)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct GlyphStats {
    points: u32,
    contours: u32,
    depth: u16,
}

#[derive(Debug)]
struct ParsedFont {
    scaler_type: u32,
    tables: BTreeMap<[u8; 4], Vec<u8>>,
}

pub(crate) fn synthesize_logical_glyphs(
    font: &Font,
    logical_glyphs: &[SyntheticLogicalGlyph],
) -> KrillaResult<Font> {
    if logical_glyphs.is_empty() {
        return Ok(font.clone());
    }

    if !font.variation_coordinates().is_empty() {
        return Err(font_error(
            font,
            "PDF logical units are not yet supported for variable TrueType fonts",
        ));
    }

    let source = font.font_data();
    let mut parsed = ParsedFont::parse(source.as_ref(), font)?;
    let maxp = required_table(&parsed.tables, MAXP, font)?;
    if maxp.len() < 6 {
        return Err(font_error(font, "maxp table is too short"));
    }
    let base_glyph_count = read_u16(maxp, 4, font)?;

    for (index, logical) in logical_glyphs.iter().enumerate() {
        let expected = usize::from(base_glyph_count) + index;
        if usize::from(logical.virtual_gid) != expected {
            return Err(font_error(
                font,
                "logical synthetic glyph IDs are not contiguous after the source glyphs",
            ));
        }
    }

    let total_glyphs = usize::from(base_glyph_count)
        .checked_add(logical_glyphs.len())
        .ok_or_else(|| font_error(font, "too many synthetic glyphs"))?;
    let total_glyphs = u16::try_from(total_glyphs)
        .map_err(|_| font_error(font, "TrueType glyph limit exceeded"))?;

    let head = required_table(&parsed.tables, HEAD, font)?;
    if head.len() < 54 {
        return Err(font_error(font, "head table is too short"));
    }
    let loca_format = read_i16(head, 50, font)?;
    if loca_format != 0 && loca_format != 1 {
        return Err(font_error(font, "unsupported loca format"));
    }

    let source_glyf = required_table(&parsed.tables, GLYF, font)?;
    let source_loca = required_table(&parsed.tables, LOCA, font)?;
    let loca = parse_loca(
        source_loca,
        base_glyph_count,
        loca_format,
        source_glyf.len(),
        font,
    )?;
    let mut metrics = parse_hmetrics(&parsed.tables, base_glyph_count, font)?;

    let base_end = usize::try_from(*loca.last().ok_or_else(|| font_error(font, "empty loca"))?)
        .map_err(|_| font_error(font, "glyf offset does not fit usize"))?;
    let mut glyf = source_glyf
        .get(..base_end)
        .ok_or_else(|| font_error(font, "loca exceeds glyf table"))?
        .to_vec();
    let mut new_loca = loca;

    let mut stats_cache = HashMap::<u16, GlyphStats>::new();
    let mut max_points = 0_u32;
    let mut max_contours = 0_u32;
    let mut max_components = 0_u16;
    let mut max_depth = 0_u16;
    let mut synthetic_bbox: Option<BBox> = None;

    for logical in logical_glyphs {
        let (bytes, bbox, stats) = build_composite(
            &logical.key,
            base_glyph_count,
            source_glyf,
            &new_loca[..=usize::from(base_glyph_count)],
            &mut stats_cache,
            font,
        )?;
        glyf.extend_from_slice(&bytes);
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
        new_loca
            .push(u32::try_from(glyf.len()).map_err(|_| font_error(font, "glyf table too large"))?);

        let advance = u16::try_from(logical.key.advance_width)
            .map_err(|_| font_error(font, "logical unit advance does not fit TrueType hmtx"))?;
        metrics.push((advance, bbox.x_min));
        synthetic_bbox = Some(synthetic_bbox.map_or(bbox, |current| current.union(bbox)));
        max_points = max_points.max(stats.points);
        max_contours = max_contours.max(stats.contours);
        max_components =
            max_components.max(u16::try_from(logical.key.components.len()).unwrap_or(u16::MAX));
        max_depth = max_depth.max(stats.depth);
    }

    let mut head = required_table(&parsed.tables, HEAD, font)?.to_vec();
    write_u32(&mut head, 8, 0, font)?;
    write_i16(&mut head, 50, 1, font)?;
    if let Some(bbox) = synthetic_bbox {
        let current = BBox {
            x_min: read_i16(&head, 36, font)?,
            y_min: read_i16(&head, 38, font)?,
            x_max: read_i16(&head, 40, font)?,
            y_max: read_i16(&head, 42, font)?,
        };
        let bbox = current.union(bbox);
        write_i16(&mut head, 36, bbox.x_min, font)?;
        write_i16(&mut head, 38, bbox.y_min, font)?;
        write_i16(&mut head, 40, bbox.x_max, font)?;
        write_i16(&mut head, 42, bbox.y_max, font)?;
    }
    parsed.tables.insert(HEAD, head);

    let mut maxp = required_table(&parsed.tables, MAXP, font)?.to_vec();
    write_u16(&mut maxp, 4, total_glyphs, font)?;
    if maxp.len() >= 32 {
        let old_points = read_u16(&maxp, 10, font)?;
        let old_contours = read_u16(&maxp, 12, font)?;
        let old_components = read_u16(&maxp, 28, font)?;
        let old_depth = read_u16(&maxp, 30, font)?;
        write_u16(
            &mut maxp,
            10,
            old_points.max(saturating_u16(max_points)),
            font,
        )?;
        write_u16(
            &mut maxp,
            12,
            old_contours.max(saturating_u16(max_contours)),
            font,
        )?;
        write_u16(&mut maxp, 28, old_components.max(max_components), font)?;
        write_u16(&mut maxp, 30, old_depth.max(max_depth), font)?;
    }
    parsed.tables.insert(MAXP, maxp);

    let mut hhea = required_table(&parsed.tables, HHEA, font)?.to_vec();
    if hhea.len() < 36 {
        return Err(font_error(font, "hhea table is too short"));
    }
    write_u16(&mut hhea, 34, total_glyphs, font)?;
    parsed.tables.insert(HHEA, hhea);
    parsed.tables.insert(HMTX, serialize_hmetrics(&metrics));
    parsed.tables.insert(GLYF, glyf);
    parsed.tables.insert(LOCA, serialize_loca(&new_loca));

    if let Some(post) = parsed.tables.get_mut(&POST) {
        if post.len() >= 32 {
            post.truncate(32);
            write_u32(post, 0, 0x0003_0000, font)?;
        }
    }

    let data = parsed.build(font)?;
    Font::new(Arc::new(data).into(), 0)
        .ok_or_else(|| font_error(font, "failed to read synthesized logical TrueType font"))
}

fn build_composite(
    key: &LogicalUnitKey,
    base_glyph_count: u16,
    glyf: &[u8],
    loca: &[u32],
    stats_cache: &mut HashMap<u16, GlyphStats>,
    font: &Font,
) -> KrillaResult<(Vec<u8>, BBox, GlyphStats)> {
    if key.components.is_empty() {
        return Ok((Vec::new(), BBox::default(), GlyphStats::default()));
    }

    let mut bbox: Option<BBox> = None;
    let mut stats = GlyphStats {
        points: 0,
        contours: 0,
        depth: 1,
    };
    let mut records = Vec::with_capacity(key.components.len());

    for component in &key.components {
        let gid = u16::try_from(component.glyph_id)
            .map_err(|_| font_error(font, "logical unit glyph ID exceeds TrueType range"))?;
        if gid >= base_glyph_count {
            return Err(font_error(
                font,
                "logical unit references a non-source glyph",
            ));
        }
        let x = checked_i16(component.x, font)?;
        let y = checked_i16(component.y, font)?;
        let child_bbox = glyph_bbox(gid, glyf, loca, font)?.translated(x, y, font)?;
        bbox = Some(bbox.map_or(child_bbox, |current| current.union(child_bbox)));

        let mut visiting = HashSet::new();
        let child_stats = glyph_stats(gid, glyf, loca, stats_cache, &mut visiting, font)?;
        stats.points = stats.points.saturating_add(child_stats.points);
        stats.contours = stats.contours.saturating_add(child_stats.contours);
        stats.depth = stats.depth.max(child_stats.depth.saturating_add(1));
        records.push((gid, x, y));
    }

    let bbox = bbox.unwrap_or_default();
    let mut out = Vec::with_capacity(10 + records.len() * 8);
    push_i16(&mut out, -1);
    push_i16(&mut out, bbox.x_min);
    push_i16(&mut out, bbox.y_min);
    push_i16(&mut out, bbox.x_max);
    push_i16(&mut out, bbox.y_max);

    let last = records.len() - 1;
    for (index, (gid, x, y)) in records.into_iter().enumerate() {
        let mut flags = ARG_WORDS | ARGS_XY | ROUND_XY;
        if index != last {
            flags |= MORE_COMPONENTS;
        }
        push_u16(&mut out, flags);
        push_u16(&mut out, gid);
        push_i16(&mut out, x);
        push_i16(&mut out, y);
    }

    Ok((out, bbox, stats))
}

fn glyph_bbox(gid: u16, glyf: &[u8], loca: &[u32], font: &Font) -> KrillaResult<BBox> {
    let data = glyph_data(gid, glyf, loca, font)?;
    if data.is_empty() {
        return Ok(BBox::default());
    }
    if data.len() < 10 {
        return Err(font_error(font, "glyf record is too short"));
    }
    Ok(BBox {
        x_min: read_i16(data, 2, font)?,
        y_min: read_i16(data, 4, font)?,
        x_max: read_i16(data, 6, font)?,
        y_max: read_i16(data, 8, font)?,
    })
}

fn glyph_stats(
    gid: u16,
    glyf: &[u8],
    loca: &[u32],
    cache: &mut HashMap<u16, GlyphStats>,
    visiting: &mut HashSet<u16>,
    font: &Font,
) -> KrillaResult<GlyphStats> {
    if let Some(stats) = cache.get(&gid) {
        return Ok(*stats);
    }
    if !visiting.insert(gid) {
        return Err(font_error(font, "cyclic composite glyph"));
    }

    let data = glyph_data(gid, glyf, loca, font)?;
    let stats = if data.is_empty() {
        GlyphStats::default()
    } else {
        if data.len() < 10 {
            return Err(font_error(font, "glyf record is too short"));
        }
        let contours = read_i16(data, 0, font)?;
        if contours >= 0 {
            let contours = contours as u16;
            let points = if contours == 0 {
                0
            } else {
                let end_pts_offset = 10 + (usize::from(contours) - 1) * 2;
                u32::from(read_u16(data, end_pts_offset, font)?) + 1
            };
            GlyphStats {
                points,
                contours: u32::from(contours),
                depth: 0,
            }
        } else {
            let mut offset = 10_usize;
            let mut points = 0_u32;
            let mut contours = 0_u32;
            let mut depth = 1_u16;
            loop {
                let flags = read_u16(data, offset, font)?;
                let child = read_u16(data, offset + 2, font)?;
                offset += 4;
                offset += if flags & ARG_WORDS != 0 { 4 } else { 2 };
                if flags & HAVE_SCALE != 0 {
                    offset += 2;
                } else if flags & HAVE_XY_SCALE != 0 {
                    offset += 4;
                } else if flags & HAVE_2X2 != 0 {
                    offset += 8;
                }
                if offset > data.len() {
                    return Err(font_error(font, "malformed composite glyph"));
                }
                let child_stats = glyph_stats(child, glyf, loca, cache, visiting, font)?;
                points = points.saturating_add(child_stats.points);
                contours = contours.saturating_add(child_stats.contours);
                depth = depth.max(child_stats.depth.saturating_add(1));
                if flags & MORE_COMPONENTS == 0 {
                    break;
                }
            }
            GlyphStats {
                points,
                contours,
                depth,
            }
        }
    };

    visiting.remove(&gid);
    cache.insert(gid, stats);
    Ok(stats)
}

fn glyph_data<'a>(gid: u16, glyf: &'a [u8], loca: &[u32], font: &Font) -> KrillaResult<&'a [u8]> {
    let start = usize::try_from(
        *loca
            .get(usize::from(gid))
            .ok_or_else(|| font_error(font, "glyph loca entry missing"))?,
    )
    .map_err(|_| font_error(font, "glyph offset does not fit usize"))?;
    let end = usize::try_from(
        *loca
            .get(usize::from(gid) + 1)
            .ok_or_else(|| font_error(font, "glyph loca end entry missing"))?,
    )
    .map_err(|_| font_error(font, "glyph end offset does not fit usize"))?;
    glyf.get(start..end)
        .ok_or_else(|| font_error(font, "loca exceeds glyf table"))
}

fn parse_loca(
    data: &[u8],
    glyph_count: u16,
    format: i16,
    glyf_len: usize,
    font: &Font,
) -> KrillaResult<Vec<u32>> {
    let count = usize::from(glyph_count) + 1;
    let mut offsets = Vec::with_capacity(count);
    for index in 0..count {
        let offset = if format == 0 {
            u32::from(read_u16(data, index * 2, font)?) * 2
        } else {
            read_u32(data, index * 4, font)?
        };
        if usize::try_from(offset)
            .ok()
            .is_none_or(|offset| offset > glyf_len)
        {
            return Err(font_error(font, "loca offset exceeds glyf table"));
        }
        if offsets.last().is_some_and(|previous| *previous > offset) {
            return Err(font_error(font, "loca offsets are not monotonic"));
        }
        offsets.push(offset);
    }
    Ok(offsets)
}

fn parse_hmetrics(
    tables: &BTreeMap<[u8; 4], Vec<u8>>,
    glyph_count: u16,
    font: &Font,
) -> KrillaResult<Vec<(u16, i16)>> {
    let hhea = required_table(tables, HHEA, font)?;
    if hhea.len() < 36 {
        return Err(font_error(font, "hhea table is too short"));
    }
    let number_of_hmetrics = read_u16(hhea, 34, font)?;
    if number_of_hmetrics == 0 || number_of_hmetrics > glyph_count {
        return Err(font_error(font, "invalid numberOfHMetrics"));
    }
    let hmtx = required_table(tables, HMTX, font)?;
    let mut metrics = Vec::with_capacity(usize::from(glyph_count));
    let mut last_advance = 0_u16;
    for gid in 0..glyph_count {
        if gid < number_of_hmetrics {
            let offset = usize::from(gid) * 4;
            last_advance = read_u16(hmtx, offset, font)?;
            metrics.push((last_advance, read_i16(hmtx, offset + 2, font)?));
        } else {
            let lsb_index = usize::from(gid - number_of_hmetrics);
            let offset = usize::from(number_of_hmetrics) * 4 + lsb_index * 2;
            metrics.push((last_advance, read_i16(hmtx, offset, font)?));
        }
    }
    Ok(metrics)
}

fn serialize_hmetrics(metrics: &[(u16, i16)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(metrics.len() * 4);
    for (advance, lsb) in metrics {
        push_u16(&mut out, *advance);
        push_i16(&mut out, *lsb);
    }
    out
}

fn serialize_loca(offsets: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(offsets.len() * 4);
    for offset in offsets {
        out.extend_from_slice(&offset.to_be_bytes());
    }
    out
}

impl ParsedFont {
    fn parse(data: &[u8], font: &Font) -> KrillaResult<Self> {
        if data.len() < 12 {
            return Err(font_error(font, "font is too short"));
        }
        if data.get(..4) == Some(b"ttcf") {
            return Err(font_error(
                font,
                "PDF logical units do not yet support TrueType collections",
            ));
        }
        let scaler_type = read_u32(data, 0, font)?;
        let count = usize::from(read_u16(data, 4, font)?);
        let directory_len = 12_usize
            .checked_add(count.saturating_mul(16))
            .ok_or_else(|| font_error(font, "table directory overflow"))?;
        if directory_len > data.len() {
            return Err(font_error(font, "table directory exceeds font data"));
        }

        let mut tables = BTreeMap::new();
        for index in 0..count {
            let record = 12 + index * 16;
            let tag: [u8; 4] = data[record..record + 4].try_into().unwrap();
            let offset = usize::try_from(read_u32(data, record + 8, font)?)
                .map_err(|_| font_error(font, "table offset does not fit usize"))?;
            let len = usize::try_from(read_u32(data, record + 12, font)?)
                .map_err(|_| font_error(font, "table length does not fit usize"))?;
            let end = offset
                .checked_add(len)
                .ok_or_else(|| font_error(font, "table range overflow"))?;
            let bytes = data
                .get(offset..end)
                .ok_or_else(|| font_error(font, "table exceeds font data"))?;
            tables.insert(tag, bytes.to_vec());
        }
        Ok(Self {
            scaler_type,
            tables,
        })
    }

    fn build(mut self, font: &Font) -> KrillaResult<Vec<u8>> {
        if let Some(head) = self.tables.get_mut(&HEAD) {
            write_u32(head, 8, 0, font)?;
        }
        let count = u16::try_from(self.tables.len())
            .map_err(|_| font_error(font, "too many font tables"))?;
        let entry_selector = if count == 0 {
            0
        } else {
            (u16::BITS - 1 - count.leading_zeros()) as u16
        };
        let search_range = if count == 0 {
            0
        } else {
            (1_u16 << entry_selector) * 16
        };
        let range_shift = count * 16 - search_range;

        let header_len = 12 + self.tables.len() * 16;
        let mut offsets = Vec::with_capacity(self.tables.len());
        let mut next_offset = header_len;
        for (tag, data) in &self.tables {
            offsets.push((*tag, next_offset, data.len(), checksum(data)));
            next_offset = next_offset
                .checked_add(data.len())
                .ok_or_else(|| font_error(font, "font size overflow"))?;
            next_offset = (next_offset + 3) & !3;
        }

        let mut out = Vec::with_capacity(next_offset);
        out.extend_from_slice(&self.scaler_type.to_be_bytes());
        push_u16(&mut out, count);
        push_u16(&mut out, search_range);
        push_u16(&mut out, entry_selector);
        push_u16(&mut out, range_shift);

        for (tag, offset, len, sum) in &offsets {
            out.extend_from_slice(tag);
            out.extend_from_slice(&sum.to_be_bytes());
            out.extend_from_slice(
                &u32::try_from(*offset)
                    .map_err(|_| font_error(font, "font offset exceeds u32"))?
                    .to_be_bytes(),
            );
            out.extend_from_slice(
                &u32::try_from(*len)
                    .map_err(|_| font_error(font, "font table exceeds u32"))?
                    .to_be_bytes(),
            );
        }

        let mut head_adjustment_offset = None;
        for ((tag, offset, _, _), (_, data)) in offsets.iter().zip(self.tables.iter()) {
            while out.len() < *offset {
                out.push(0);
            }
            if *tag == HEAD {
                head_adjustment_offset = Some(*offset + 8);
            }
            out.extend_from_slice(data);
            while out.len() % 4 != 0 {
                out.push(0);
            }
        }

        if let Some(offset) = head_adjustment_offset {
            let adjustment = CHECKSUM_MAGIC.wrapping_sub(checksum(&out));
            out[offset..offset + 4].copy_from_slice(&adjustment.to_be_bytes());
        }
        Ok(out)
    }
}

fn required_table<'a>(
    tables: &'a BTreeMap<[u8; 4], Vec<u8>>,
    tag: [u8; 4],
    font: &Font,
) -> KrillaResult<&'a [u8]> {
    tables.get(&tag).map(Vec::as_slice).ok_or_else(|| {
        font_error(
            font,
            &format!("missing {} table", String::from_utf8_lossy(&tag)),
        )
    })
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0_u32, |sum, chunk| {
        let mut word = [0_u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

fn checked_i16(value: i32, font: &Font) -> KrillaResult<i16> {
    i16::try_from(value).map_err(|_| font_error(font, "logical component coordinate exceeds i16"))
}

fn saturating_u16(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn read_u16(data: &[u8], offset: usize, font: &Font) -> KrillaResult<u16> {
    let bytes: [u8; 2] = data
        .get(offset..offset + 2)
        .ok_or_else(|| font_error(font, "font read exceeds table"))?
        .try_into()
        .unwrap();
    Ok(u16::from_be_bytes(bytes))
}

fn read_i16(data: &[u8], offset: usize, font: &Font) -> KrillaResult<i16> {
    Ok(i16::from_be_bytes(
        data.get(offset..offset + 2)
            .ok_or_else(|| font_error(font, "font read exceeds table"))?
            .try_into()
            .unwrap(),
    ))
}

fn read_u32(data: &[u8], offset: usize, font: &Font) -> KrillaResult<u32> {
    Ok(u32::from_be_bytes(
        data.get(offset..offset + 4)
            .ok_or_else(|| font_error(font, "font read exceeds table"))?
            .try_into()
            .unwrap(),
    ))
}

fn write_u16(data: &mut [u8], offset: usize, value: u16, font: &Font) -> KrillaResult<()> {
    data.get_mut(offset..offset + 2)
        .ok_or_else(|| font_error(font, "font write exceeds table"))?
        .copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn write_i16(data: &mut [u8], offset: usize, value: i16, font: &Font) -> KrillaResult<()> {
    data.get_mut(offset..offset + 2)
        .ok_or_else(|| font_error(font, "font write exceeds table"))?
        .copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn write_u32(data: &mut [u8], offset: usize, value: u32, font: &Font) -> KrillaResult<()> {
    data.get_mut(offset..offset + 4)
        .ok_or_else(|| font_error(font, "font write exceeds table"))?
        .copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn push_u16(data: &mut Vec<u8>, value: u16) {
    data.extend_from_slice(&value.to_be_bytes());
}

fn push_i16(data: &mut Vec<u8>, value: i16) {
    data.extend_from_slice(&value.to_be_bytes());
}

fn font_error(font: &Font, message: &str) -> KrillaError {
    KrillaError::Font(font.clone(), message.to_owned())
}
