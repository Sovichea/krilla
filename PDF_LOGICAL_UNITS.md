# PDF Logical Text Units

This document describes the complete Version 4 architecture of Krilla's
authoritative logical-text path. The version-specific documents record how the
design evolved:

- [Version 1](PDF_LOGICAL_UNITS_V1.md)
- [Version 2](PDF_LOGICAL_UNITS_V2.md)
- [Version 3](PDF_LOGICAL_UNITS_V3.md)
- [Version 4](PDF_LOGICAL_UNITS_V4.md)

## Purpose

Text shaping and PDF text extraction describe different things. A shaping
engine may reorder glyphs, combine several Unicode scalars into one glyph, or
draw one source sequence with several positioned glyphs. PDF viewers, however,
need a stable character-code sequence and `/ToUnicode` mapping for extraction,
search, selection, and accessibility.

The logical-text path therefore keeps these identities separate:

```text
source Unicode
    != shaping cluster
    != visual glyph sequence
    != PDF character code / CID
    != embedded TrueType GID
```

The caller supplies authoritative Unicode and already-shaped geometry. Krilla
does not reshape, normalize, reorder, or apply script-specific rules.

## End-to-end architecture

```text
caller / text shaper
    |
    | PdfLogicalUnit[] in logical Unicode order
    | - exact text
    | - shaped glyph IDs
    | - advances and offsets
    | - independent visual origins
    v
logical planning
    |
    | LogicalUnitPlan
    | - authoritative text
    | - VisualUnitKey
    | - visual_x / visual_y
    v
LogicalFontMapper
    |
    | SemanticUnitKey(text + visual)
    | - reuses identical semantic units
    | - chooses a logical-font shard
    v
LogicalCIDFont
    |                         |
    | semantic namespace      | visual namespace
    | CID -> text             | VisualUnitKey -> compact GID
    |                         |
    +------------+------------+
                 v
compact derived TrueType font
    |
    | used source glyphs + composite dependencies
    | + synthetic glyphs only where required
    v
PDF Type 0 font
    |
    | /Encoding /Identity-H
    | /ToUnicode: CID -> exact Unicode
    | /CIDToGIDMap: CID -> compact embedded GID
    v
content stream
    |
    | Tj for contiguous groups
    | TJ for safely positioned groups
    v
searchable, selectable, extractable, tagged PDF text
```

## Public API and caller contract

The public representation is:

```rust
pub struct PdfLogicalUnit<'a, G: Glyph> {
    pub text: &'a str,
    pub glyphs: &'a [G],
    pub visual_x: f32,
    pub visual_y: f32,
    pub location: Option<Location>,
}
```

The caller passes a slice to `Surface::draw_pdf_logical_units` together with a
run origin, font, size, fill, and stroke.

The contract is:

- Units are ordered by authoritative Unicode source order.
- `text` is the exact Unicode represented by that unit. Krilla does not
  normalize it.
- `glyphs` are already shaped and remain in visual glyph order within the unit.
- Glyph IDs refer to the supplied source font.
- Glyph metrics and `visual_x` / `visual_y` are normalized to a font size of
  `1.0`.
- The visual origin is independent from the unit's position in the logical
  slice. This is required for bidirectional text.
- `location` is optional diagnostic information used by validation errors.

The path does not add an invisible duplicate text layer and does not use
`/ActualText` as its normal semantic mechanism.

## Logical planning

Each `PdfLogicalUnit` is converted into a `LogicalUnitPlan` before font
allocation.

The planner walks the shaped glyphs and records every component as:

```rust
LogicalComponent {
    glyph_id,
    x,
    y,
}
```

Component coordinates and the unit advance are rounded into source-font units.
The planner also accounts for negative or positive pen movement so the visual
key has a stable origin and width.

The resulting visual identity is:

```rust
VisualUnitKey {
    advance_width,
    components,
}
```

`VisualUnitKey` contains no Unicode. It describes only the appearance and
metrics needed to paint one logical unit.

`LogicalUnitPlan` keeps the two sides together without conflating them:

```text
LogicalUnitPlan
    semantic: text + optional diagnostic location
    visual:   advance + positioned source components
    placement: visual_x + visual_y
```

## Semantic and visual identity

The font mapper uses two different keys.

### Semantic identity

```rust
SemanticUnitKey {
    text,
    visual,
}
```

An identical `text + visual` pair reuses the same PDF character code and CID.
The Unicode string is authoritative for `/ToUnicode`.

The visual key remains part of semantic identity because the same Unicode may
legitimately have different shaped forms. Conversely, different Unicode
strings may have exactly the same visual form. Those strings receive different
CIDs but may share one compact embedded GID.

### Visual identity

Within each logical-font shard, a `VisualUnitKey` maps to one visual record.
Unicode does not participate in this lookup.

This makes the following mapping valid:

```text
CID 41 -> /ToUnicode "semantic A" -> embedded GID 12
CID 42 -> /ToUnicode "semantic B" -> embedded GID 12
```

Extraction remains unambiguous because `/ToUnicode` is keyed by CID, while the
outline is safely shared through `/CIDToGIDMap`.

## Logical fonts are separate from ordinary text fonts

Each Krilla `FontContainer` owns both:

- the existing ordinary `CIDFont` used by `draw_glyphs`; and
- a dedicated `LogicalFontMapper` used by `draw_pdf_logical_units`.

The ordinary path retains its existing GID-to-CID mapping, subsetting, and
serialization behavior. Version 4 changes only the logical path.

Each physical logical shard has its own `LogicalFontIdentifier`, semantic CID
table, visual table, compact capacity tracker, and PDF font resource.

## Compact source-backed and synthetic glyphs

Version 4 chooses the simplest exact representation for every unique visual.

### Source-backed visual

A visual can reuse a source glyph when:

1. it has exactly one component;
2. the component offset is `(0, 0)`;
3. the source GID is valid; and
4. the logical advance equals the source glyph's nominal advance.

The source GID is still remapped into the compact embedded namespace. Source
GID 65,000 can therefore become embedded GID 3.

### Synthetic visual

A synthetic TrueType composite is used when the unit has:

- multiple components;
- a positioned component; or
- an advance different from the source glyph's nominal advance.

The synthetic glyph references compactly remapped source components and stores
the logical unit's required advance in `hmtx`. Its outline geometry is built
from the already-shaped positions; no shaping is repeated.

This decision is independent of script. Latin, Arabic, Khmer, Devanagari,
Thai, Lao, CJK, and other scripts follow the same geometric test.

## Building the compact derived TrueType font

`build_compact_logical_font` performs these steps for one shard:

1. Seed a `GlyphRemapper` with every source component referenced by the
   shard's visual records.
2. Subset the source font. The subsetter includes transitive dependencies of
   source composite glyphs.
3. Record the compact embedded GID for every exact source-backed visual.
4. Remap every component of a synthetic visual into the compact source subset.
5. Append synthetic composite glyphs after the compact source glyphs.
6. Update the TrueType `glyf`, `loca`, `hmtx`, `hhea`, `maxp`, `head`, and
   `post` data required by the derived font.
7. Return the derived font and one embedded GID for every visual record.

For example:

```text
source font GIDs used:       19, 4200, 65000
compact source GIDs:          1,    2,     3
synthetic visual GID:         4
```

The complete source namespace is never copied merely to preserve its original
GID numbers.

## Exact compact-capacity accounting

`CompactGlyphTracker` predicts the physical glyph cost before a new visual is
assigned to a shard.

For every source component, it walks the `glyf` / `loca` data and computes the
transitive closure of composite dependencies. It then counts:

```text
.notdef
+ unique source glyphs and dependencies
+ unique visuals that require synthesis
```

This count matches the namespace that will be built during serialization.
Already-known source dependencies and visual records do not consume capacity
again.

As a result, a source font containing nearly 65,535 glyphs does not force an
immediate shard when the document uses only a few glyphs. Only the compact
derived namespace matters.

## Sharding

Each physical logical font has two standard 16-bit limits:

- at most 65,535 allocated semantic CIDs, with CID 0 reserved for `.notdef`;
- at most 65,535 compact embedded TrueType GIDs.

Before adding a new semantic unit, `LogicalFontMapper` asks the active shard
whether both allocations fit. If not, it creates another shard and assigns the
unit there.

```text
LogicalFontMapper
    +-- shard 0: semantic CIDs + compact GIDs
    +-- shard 1: semantic CIDs + compact GIDs
    +-- ...
```

Repeated semantic keys continue to reuse their original shard. Content
serialization switches PDF font resources when adjacent units belong to
different shards.

One visual construction and all source dependencies it references must fit in
one physical font. A TrueType composite cannot reference glyphs stored in a
different PDF font resource.

Sharding preserves standard two-byte PDF codes. Version 4 does not require
non-standard wider CIDs, four-byte content codes, or a custom Encoding CMap.

## PDF font serialization

Every logical shard is serialized as a Type 0 font with a `CIDFontType2`
descendant:

```text
Type 0 font
    /Encoding /Identity-H
    /DescendantFonts [CIDFontType2]
    /ToUnicode <stream>

CIDFontType2
    /W <semantic CID widths>
    /CIDToGIDMap <explicit stream>
    /FontDescriptor
        /FontFile2 <compact derived TrueType font>
        /CIDSet <when required by the selected PDF version>
```

The mappings have independent responsibilities:

```text
content bytes -> Identity-H -> semantic CID
semantic CID  -> /ToUnicode -> exact Unicode string
semantic CID  -> /CIDToGIDMap -> compact embedded GID
```

`/CIDToGIDMap` contains one big-endian `u16` GID per CID, beginning with the
CID 0 mapping to embedded GID 0.

Widths are indexed by semantic CID and converted from source-font units to PDF
text units. `/ToUnicode` entries may map one CID to multiple Unicode scalars.
The compact `FontFile2`, descriptor metrics, bounding box, CID set, widths, and
maps are emitted as one coherent logical font resource.

## Content-stream serialization

Logical units remain in authoritative source order throughout content writing.
Krilla never reorders the encoded CIDs to match visual order.

Within a text object, the writer:

1. maps each plan to a semantic CID and logical-font shard;
2. retains the active font and emits `Tf` only when the shard changes;
3. groups adjacent units when they share a shard and their horizontal
   displacement can be represented safely;
4. emits one `Tm` at the group's visual origin;
5. emits `Tj` for exactly contiguous codes or `TJ` when adjustments are
   required.

PDF subtracts a `TJ` adjustment from the current horizontal text position. For
two logical plans, Krilla calculates:

```text
expected_x = current visual_x + current advance
displacement = next visual_x - expected_x
TJ adjustment = -displacement * 1000
```

Therefore a forward LTR gap produces a negative adjustment, while backward
visual movement in an RTL run produces a positive adjustment. CID order stays
logical in both cases.

Adjustments are rounded to two decimal places in PDF text space. The maximum
rounding error is `0.000005 em`. A group ends at a shard transition, baseline
change, non-finite coordinate, or displacement that cannot be reconstructed
within that tolerance.

Text operations do not cross Krilla's tagging or marked-content boundaries.
Fill, stroke, opacity, transforms, and bounding-box handling use the same
graphics machinery as other text drawing.

## Unicode, accessibility, and conformance invariants

The producer-side invariants are:

- `/ToUnicode` contains the exact authoritative string for every semantic CID.
- Logical source order is preserved in content codes.
- Visual placement reproduces the caller's shaped geometry.
- Semantic and visual deduplication cannot make `/ToUnicode` ambiguous.
- Logical text remains inside the original marked-content and tagged-content
  operations.
- No duplicate invisible text layer is introduced.
- Ordinary Krilla text rendering remains unchanged.

PDF syntax and standards conformance do not guarantee identical search
highlight geometry or match counts in every viewer. Those are reader behaviors.
Known reader differences should be recorded as interoperability fixtures rather
than addressed by script-specific Unicode rewriting.

## Supported font path and limits

The compact synthetic path currently targets TrueType `glyf` fonts that Krilla
can subset and rewrite.

Important limits are:

- semantic CIDs and compact embedded GIDs remain 16-bit per shard;
- component coordinates must fit the TrueType composite representation;
- synthetic advances must fit `hmtx`;
- one visual and all its dependencies must fit within one shard;
- unsupported font formats must use another rendering path rather than this
  compact synthetic mechanism.

These are physical-font constraints, not script constraints.

## Validation

Unit coverage includes:

- exact semantic-key reuse;
- distinct semantics sharing one visual GID;
- distinct visuals receiving distinct compact GIDs;
- exact source-backed reuse;
- synthetic fallback for changed advances;
- multi-component synthetic construction;
- source composite dependency accounting;
- semantic-CID capacity sharding;
- compact embedded-GID capacity sharding;
- LTR and RTL `TJ` adjustment signs;
- rejection across incompatible baselines and invalid coordinates; and
- stable two-decimal positioning.

The controlled Typst integration corpus covers English, Khmer, Arabic,
Devanagari, and mixed multilingual prose. Compared with Version 3:

- all 85 rendered pages are pixel-identical;
- Poppler raw extraction is byte-identical for every fixture;
- all generated files pass qpdf syntax validation;
- the combined PDF/A-2b + PDF/UA-1 fixture passes both veraPDF profiles;
- embedded glyph counts fall from 2,707 to 2,279; and
- every individual PDF remains within 1.01% of Version 3's size.

The primary v4 result is capacity correctness and a cleaner representation,
not file-size reduction: source-backed units no longer consume synthetic glyphs,
and original source GID numbering no longer consumes compact embedded capacity.

## Source map

The implementation is divided by responsibility:

| File | Responsibility |
|---|---|
| `surface.rs` | Public drawing API, paint/stroke routing, and outline fallback |
| `content.rs` | Planning, semantic allocation, font-state retention, `Tj`/`TJ`, and geometry |
| `text/logical.rs` | Public unit type, visual planning, and positioning calculations |
| `text/logical_font.rs` | Semantic cache, visual cache, shard allocation, and logical font ownership |
| `text/truetype_logical.rs` | Compact dependency tracking, source subsetting, and synthetic TrueType construction |
| `text/cid.rs` | Type 0 font, `/ToUnicode`, `/CIDToGIDMap`, widths, descriptor, and font serialization |
| `serialize.rs` | Registration and final serialization of logical font resources |

## Design summary

Version 4 is a hybrid logical-glyph architecture:

```text
semantic identity != visual identity != embedded glyph identity
```

It preserves exact Unicode through semantic CIDs, preserves shaped appearance
through visual keys, and uses compact embedded GIDs only as physical font
resources. Exact source glyphs are reused; reconstruction is reserved for units
that genuinely need it; and sharding depends on what the PDF embeds rather than
what the source font happens to contain.
