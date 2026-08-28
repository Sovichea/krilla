use rustc_hash::FxHashMap;

use crate::chunk_container::ChunkContainer;
use crate::error::KrillaResult;
use crate::serialize::SerializeContext;
use crate::surface::Location;
use crate::text::cid::{serialize_logical_font, Cid};
use crate::text::logical::VisualUnitKey;
use crate::text::truetype_logical::{CompactGlyphAddition, CompactGlyphTracker};
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

pub(crate) struct LogicalSemanticRecord {
    pub(crate) text: String,
    pub(crate) location: Option<Location>,
    pub(crate) visual: usize,
}

pub(crate) struct LogicalCIDFont {
    font: Font,
    identifier: FontIdentifier,
    visuals: Vec<VisualUnitKey>,
    visual_map: FxHashMap<VisualUnitKey, usize>,
    semantics: Vec<LogicalSemanticRecord>,
    compact_glyphs: CompactGlyphTracker,
}

enum VisualAddition {
    Existing,
    New(CompactGlyphAddition),
}

impl LogicalCIDFont {
    fn new(font: Font, index: usize) -> Self {
        Self {
            identifier: FontIdentifier::Logical(LogicalFontIdentifier(font.clone(), index)),
            font,
            visuals: Vec::new(),
            visual_map: FxHashMap::default(),
            semantics: Vec::new(),
            compact_glyphs: CompactGlyphTracker::new(),
        }
    }

    pub(crate) fn identifier(&self) -> FontIdentifier {
        self.identifier.clone()
    }

    fn plan_addition(
        &self,
        visual: &VisualUnitKey,
        semantic_limit: usize,
        embedded_glyph_limit: usize,
    ) -> Option<VisualAddition> {
        if self.semantics.len() >= semantic_limit || self.semantics.len() >= usize::from(u16::MAX) {
            return None;
        }
        if self.visual_map.contains_key(visual) {
            return Some(VisualAddition::Existing);
        }

        let addition = self
            .compact_glyphs
            .plan(&self.font, visual)
            .expect("logical visual was validated before allocation");
        self.compact_glyphs
            .can_commit(&addition, embedded_glyph_limit)
            .then_some(VisualAddition::New(addition))
    }

    fn add(
        &mut self,
        text: String,
        visual: VisualUnitKey,
        location: Option<Location>,
        addition: VisualAddition,
    ) -> PDFGlyph {
        let visual_index = match addition {
            VisualAddition::Existing => *self
                .visual_map
                .get(&visual)
                .expect("existing logical visual is missing from its map"),
            VisualAddition::New(addition) => {
                self.compact_glyphs.commit(addition);
                let index = self.visuals.len();
                self.visuals.push(visual.clone());
                self.visual_map.insert(visual, index);
                index
            }
        };

        let cid = Cid::try_from(self.semantics.len() + 1)
            .expect("logical font capacity was checked before allocation");
        self.semantics.push(LogicalSemanticRecord {
            text,
            location,
            visual: visual_index,
        });
        PDFGlyph::Cid(cid)
    }

    pub(crate) fn serialize(
        &self,
        sc: &mut SerializeContext,
        chunk_container: &mut ChunkContainer,
        root_ref: pdf_writer::Ref,
    ) -> KrillaResult<()> {
        let shard = match &self.identifier {
            FontIdentifier::Logical(LogicalFontIdentifier(_, shard)) => *shard,
            _ => unreachable!("logical font has a non-logical identifier"),
        };
        serialize_logical_font(
            &self.font,
            &self.visuals,
            &self.semantics,
            shard,
            sc,
            chunk_container,
            root_ref,
        )
    }

    #[cfg(test)]
    fn semantic_count(&self) -> usize {
        self.semantics.len()
    }

    #[cfg(test)]
    fn visual_count(&self) -> usize {
        self.visuals.len()
    }

    #[cfg(test)]
    fn embedded_glyph_count(&self) -> usize {
        self.compact_glyphs.glyph_count()
    }
}

pub(crate) struct LogicalFontMapper {
    font: Font,
    semantic_records: FxHashMap<SemanticUnitKey, LogicalPdfGlyph>,
    shards: Vec<LogicalCIDFont>,
    semantic_shard_capacity: usize,
    embedded_glyph_capacity: usize,
}

impl LogicalFontMapper {
    pub(crate) fn new(font: Font) -> Self {
        Self::with_capacities(font, usize::MAX, usize::from(u16::MAX))
    }

    #[cfg(test)]
    fn with_shard_capacity(font: Font, shard_capacity: usize) -> Self {
        Self::with_capacities(font, shard_capacity, usize::from(u16::MAX))
    }

    fn with_capacities(
        font: Font,
        semantic_shard_capacity: usize,
        embedded_glyph_capacity: usize,
    ) -> Self {
        assert!(semantic_shard_capacity > 0);
        assert!(embedded_glyph_capacity > 0);
        Self {
            font,
            semantic_records: FxHashMap::default(),
            shards: Vec::new(),
            semantic_shard_capacity,
            embedded_glyph_capacity,
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

        let (shard, addition) = match self.shards.last().and_then(|shard| {
            shard
                .plan_addition(
                    &key.visual,
                    self.semantic_shard_capacity,
                    self.embedded_glyph_capacity,
                )
                .map(|addition| (self.shards.len() - 1, addition))
        }) {
            Some(planned) => planned,
            None => {
                let index = self.shards.len();
                let shard = LogicalCIDFont::new(self.font.clone(), index);
                let addition = shard
                    .plan_addition(
                        &key.visual,
                        self.semantic_shard_capacity,
                        self.embedded_glyph_capacity,
                    )
                    .expect("one logical visual exceeds the physical font capacity");
                self.shards.push(shard);
                (index, addition)
            }
        };
        let identifier = self.shards[shard].identifier();
        let glyph =
            self.shards[shard].add(key.text.clone(), key.visual.clone(), location, addition);
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
        assert_eq!(mapper.fonts()[0].semantic_count(), 1);
        assert_eq!(mapper.fonts()[0].visual_count(), 1);
    }

    #[test]
    fn different_semantics_share_one_visual_glyph() {
        let mut mapper = LogicalFontMapper::new(font());
        mapper.add("A".into(), visual(36), None);
        mapper.add("different semantics".into(), visual(36), None);

        assert_eq!(mapper.fonts()[0].semantic_count(), 2);
        assert_eq!(mapper.fonts()[0].visual_count(), 1);
    }

    #[test]
    fn different_visuals_get_distinct_embedded_glyphs() {
        let mut mapper = LogicalFontMapper::new(font());
        mapper.add("A".into(), visual(36), None);
        mapper.add("A".into(), visual(37), None);

        assert_eq!(mapper.fonts()[0].semantic_count(), 2);
        assert_eq!(mapper.fonts()[0].visual_count(), 2);
    }

    #[test]
    fn semantic_capacity_exhaustion_creates_a_new_shard() {
        let mut mapper = LogicalFontMapper::with_shard_capacity(font(), 2);
        let first = mapper.add("A".into(), visual(36), None);
        mapper.add("B".into(), visual(36), None);
        let third = mapper.add("C".into(), visual(36), None);

        assert_ne!(first.identifier, third.identifier);
        assert_eq!(mapper.fonts().len(), 2);
        assert_eq!(mapper.fonts()[0].semantic_count(), 2);
        assert_eq!(mapper.fonts()[1].semantic_count(), 1);
    }

    #[test]
    fn compact_embedded_glyph_capacity_creates_a_new_shard() {
        let mut mapper = LogicalFontMapper::with_capacities(font(), usize::MAX, 3);
        let first = mapper.add("A".into(), visual(36), None);
        let second = mapper.add("B".into(), visual(37), None);

        assert_ne!(first.identifier, second.identifier);
        assert_eq!(mapper.fonts().len(), 2);
        assert_eq!(mapper.fonts()[0].embedded_glyph_count(), 3);
        assert_eq!(mapper.fonts()[1].embedded_glyph_count(), 3);
    }
}
