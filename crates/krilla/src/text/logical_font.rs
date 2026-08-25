use rustc_hash::FxHashMap;

use crate::surface::Location;
use crate::text::cid::CIDFont;
use crate::text::logical::VisualUnitKey;
use crate::text::{Font, FontIdentifier, LogicalFontIdentifier, PDFGlyph};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SemanticUnitKey {
    text: String,
    visual: VisualUnitKey,
}

#[derive(Clone)]
pub(crate) struct LogicalPdfGlyph {
    pub(crate) identifier: FontIdentifier,
    glyph: PDFGlyph,
}

impl LogicalPdfGlyph {
    pub(crate) fn encode_into(&self, output: &mut Vec<u8>) {
        self.glyph.encode_into(output);
    }
}

pub(crate) struct LogicalCIDFont {
    identifier: FontIdentifier,
    cid_font: CIDFont,
    logical_count: usize,
}

impl LogicalCIDFont {
    fn new(font: Font, index: usize) -> Self {
        Self {
            identifier: FontIdentifier::Logical(LogicalFontIdentifier(font.clone(), index)),
            cid_font: CIDFont::new_logical(font, index),
            logical_count: 0,
        }
    }

    pub(crate) fn identifier(&self) -> FontIdentifier {
        self.identifier.clone()
    }

    pub(crate) fn cid_font(&self) -> &CIDFont {
        &self.cid_font
    }

    fn has_capacity(&self, limit: usize) -> bool {
        self.logical_count < limit && self.cid_font.remaining_logical_glyph_capacity() > 0
    }

    fn add(
        &mut self,
        text: String,
        visual: VisualUnitKey,
        location: Option<Location>,
    ) -> Option<PDFGlyph> {
        let cid = self.cid_font.add_logical_visual(visual)?;
        self.cid_font.set_codepoints(cid, text, location);
        self.logical_count += 1;
        Some(PDFGlyph::Cid(cid))
    }
}

pub(crate) struct LogicalFontMapper {
    font: Font,
    semantic_records: FxHashMap<SemanticUnitKey, LogicalPdfGlyph>,
    shards: Vec<LogicalCIDFont>,
    shard_capacity: usize,
}

impl LogicalFontMapper {
    pub(crate) fn new(font: Font) -> Self {
        Self::with_shard_capacity(font, usize::MAX)
    }

    fn with_shard_capacity(font: Font, shard_capacity: usize) -> Self {
        assert!(shard_capacity > 0);
        Self {
            font,
            semantic_records: FxHashMap::default(),
            shards: Vec::new(),
            shard_capacity,
        }
    }

    pub(crate) fn fonts(&self) -> &[LogicalCIDFont] {
        &self.shards
    }

    pub(crate) fn add(
        &mut self,
        text: String,
        visual: VisualUnitKey,
        location: Option<Location>,
    ) -> LogicalPdfGlyph {
        let key = SemanticUnitKey { text, visual };
        if let Some(record) = self.semantic_records.get(&key) {
            return record.clone();
        }

        let shard = match self.shards.last() {
            Some(shard) if shard.has_capacity(self.shard_capacity) => self.shards.len() - 1,
            _ => {
                let index = self.shards.len();
                self.shards
                    .push(LogicalCIDFont::new(self.font.clone(), index));
                index
            }
        };
        let identifier = self.shards[shard].identifier();
        let glyph = self.shards[shard]
            .add(key.text.clone(), key.visual.clone(), location)
            .expect("new logical font shard must have glyph capacity");
        let result = LogicalPdfGlyph { identifier, glyph };
        self.semantic_records.insert(key, result.clone());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::LogicalFontMapper;
    use crate::text::logical::{LogicalComponent, VisualUnitKey};
    use crate::text::Font;

    fn font() -> Font {
        let data = include_bytes!("../../../../assets/fonts/NotoSans-Regular.ttf");
        Font::new(data.as_slice().into(), 0).unwrap()
    }

    fn visual(glyph_id: u32) -> VisualUnitKey {
        VisualUnitKey {
            advance_width: 600,
            components: vec![LogicalComponent {
                glyph_id,
                x: 0,
                y: 0,
            }],
        }
    }

    #[test]
    fn repeated_semantic_units_reuse_the_same_cid() {
        let mut mapper = LogicalFontMapper::new(font());
        let first = mapper.add("A".into(), visual(36), None);
        let second = mapper.add("A".into(), visual(36), None);

        assert_eq!(first.identifier, second.identifier);
        assert_eq!(mapper.fonts()[0].logical_count, 1);
    }

    #[test]
    fn different_semantics_use_distinct_cids() {
        let mut mapper = LogicalFontMapper::new(font());
        mapper.add("A".into(), visual(36), None);
        mapper.add("different semantics".into(), visual(36), None);

        assert_eq!(mapper.fonts()[0].logical_count, 2);
        assert_ne!(
            mapper.fonts()[0].cid_font.get_codepoints(1),
            mapper.fonts()[0].cid_font.get_codepoints(2)
        );
    }

    #[test]
    fn capacity_exhaustion_creates_a_new_identity_shard() {
        let mut mapper = LogicalFontMapper::with_shard_capacity(font(), 2);
        let first = mapper.add("A".into(), visual(36), None);
        mapper.add("B".into(), visual(36), None);
        let third = mapper.add("C".into(), visual(36), None);

        assert_ne!(first.identifier, third.identifier);
        assert_eq!(mapper.fonts().len(), 2);
        assert_eq!(mapper.fonts()[0].logical_count, 2);
        assert_eq!(mapper.fonts()[1].logical_count, 1);
    }

    #[test]
    fn true_type_glyph_limit_creates_a_new_identity_shard() {
        let font = font();
        let capacity = usize::from(u16::MAX) + 1 - font.num_glyphs() as usize;
        let mut mapper = LogicalFontMapper::new(font);
        let visual = visual(36);
        for index in 0..=capacity {
            mapper.add(index.to_string(), visual.clone(), None);
        }

        assert_eq!(mapper.fonts().len(), 2);
        assert_eq!(mapper.fonts()[0].logical_count, capacity);
        assert_eq!(mapper.fonts()[1].logical_count, 1);
    }
}
