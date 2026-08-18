use crate::surface::Location;
use crate::text::{Font, Glyph};

/// One logical PDF text unit with authoritative Unicode and its shaped visual representation.
///
/// The units passed to [`crate::surface::Surface::draw_pdf_logical_units`] must be stored in
/// logical Unicode order. `visual_x` and `visual_y` keep that semantic order independent from
/// the position at which the unit is painted. This is particularly important for bidirectional
/// text, where logical and visual order can differ.
///
/// All coordinates are normalized to a font size of `1.0`, just like the metrics returned by
/// [`Glyph`]. `visual_x` and `visual_y` identify the pen position of the first glyph in this unit
/// relative to the run origin supplied to `draw_pdf_logical_units`.
#[derive(Debug, Clone, Copy)]
pub struct PdfLogicalUnit<'a, G: Glyph> {
    /// Exact Unicode represented by this PDF text unit.
    pub text: &'a str,
    /// Shaped glyphs that visually render this unit, in visual glyph order.
    pub glyphs: &'a [G],
    /// Horizontal pen position of the first glyph, normalized to a font size of `1.0`.
    pub visual_x: f32,
    /// Vertical pen position of the first glyph, normalized to a font size of `1.0`.
    pub visual_y: f32,
    /// Optional location associated with this logical unit for validation diagnostics.
    pub location: Option<Location>,
}

impl<'a, G: Glyph> PdfLogicalUnit<'a, G> {
    /// Create a logical PDF text unit.
    pub fn new(text: &'a str, glyphs: &'a [G], visual_x: f32, visual_y: f32) -> Self {
        Self {
            text,
            glyphs,
            visual_x,
            visual_y,
            location: None,
        }
    }

    /// Associate a validation location with this logical unit.
    pub fn with_location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct LogicalComponent {
    pub(crate) glyph_id: u32,
    pub(crate) x: i32,
    pub(crate) y: i32,
}

/// Identity of one reusable logical PDF character.
///
/// Unicode is intentionally part of the key. Two uses of the same source glyph can therefore
/// receive different PDF CIDs when they carry different extraction semantics. Conversely, a
/// multi-glyph shaped cluster is represented by one key and ultimately one CID.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct LogicalUnitKey {
    pub(crate) text: String,
    pub(crate) advance_width: i32,
    pub(crate) components: Vec<LogicalComponent>,
}

#[derive(Clone, Debug)]
pub(crate) struct LogicalUnitPlan {
    pub(crate) key: LogicalUnitKey,
    /// Absolute visual origin of the synthetic glyph, normalized to font size 1.
    pub(crate) visual_x: f32,
    /// Baseline displacement of the unit, normalized to font size 1.
    pub(crate) visual_y: f32,
    pub(crate) location: Option<Location>,
}

impl<'a, G: Glyph> PdfLogicalUnit<'a, G> {
    pub(crate) fn plan(&self, font: &Font) -> LogicalUnitPlan {
        let upem = font.units_per_em();
        let mut pen_x = 0.0_f32;
        let mut pen_y = 0.0_f32;
        let mut min_x = 0.0_f32;
        let mut max_x = 0.0_f32;
        let mut raw_components = Vec::with_capacity(self.glyphs.len());

        for glyph in self.glyphs {
            let x_advance = glyph.x_advance(1.0);
            let y_advance = glyph.y_advance(1.0);
            min_x = min_x.min(pen_x).min(pen_x + x_advance);
            max_x = max_x.max(pen_x).max(pen_x + x_advance);

            raw_components.push((
                glyph.glyph_id().to_u32(),
                pen_x + glyph.x_offset(1.0),
                pen_y + glyph.y_offset(1.0),
            ));

            pen_x += x_advance;
            pen_y += y_advance;
        }

        let components = raw_components
            .into_iter()
            .map(|(glyph_id, x, y)| LogicalComponent {
                glyph_id,
                x: ((x - min_x) * upem).round() as i32,
                y: (y * upem).round() as i32,
            })
            .collect();

        LogicalUnitPlan {
            key: LogicalUnitKey {
                text: self.text.to_owned(),
                advance_width: ((max_x - min_x) * upem).round() as i32,
                components,
            },
            visual_x: self.visual_x + min_x,
            visual_y: self.visual_y,
            location: self.location,
        }
    }
}
