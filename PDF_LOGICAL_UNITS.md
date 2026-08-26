# PDF logical text units

## Purpose

`PdfLogicalUnit` preserves the exact source Unicode of shaped text without changing its
appearance. A unit carries an authoritative Unicode string, one or more already-shaped glyphs,
and an independent visual origin. This keeps four concepts separate:

```text
source Unicode != shaping cluster != glyph sequence != PDF CID
```

Krilla does not reshape, normalize, or apply script-specific rules to logical units. The caller
provides units in logical source order and provides the visual glyph geometry produced by its
text shaper.

## Shared public model

Both implementations use the same caller-facing type:

```rust
pub struct PdfLogicalUnit<'a, G: Glyph> {
    pub text: &'a str,
    pub glyphs: &'a [G],
    pub visual_x: f32,
    pub visual_y: f32,
    pub location: Option<Location>,
}
```

The invariants are:

- `text` is authoritative for extraction, search, and accessibility.
- `glyphs` and the visual origin are authoritative for painting.
- logical units are supplied in source order, even when their visual order differs.
- no invisible duplicate text layer or normal-use `/ActualText` fallback is added.

## Version 1

Version 1 added each logical unit directly to the source font's existing `CIDFont`.

```text
LogicalUnitKey(text + advance + components)
    -> virtual TrueType GID
    -> Identity-H CID
    -> /ToUnicode text
```

Its properties were:

- ordinary positioned glyphs and logical units shared one `CIDFont` per source font;
- Unicode and visual data were combined in `LogicalUnitKey`;
- one unique semantic-and-visual key consumed one synthetic GID and one CID;
- each unit was emitted with its own text matrix and `Tj` operation;
- exact repeated keys reused their CID;
- there was no font sharding.

The practical limit was the remaining TrueType glyph space:

```text
65536 - source font glyph count
```

Exhausting that space caused logical-unit allocation to fail.

## Version 2

Version 2 retains standard two-byte `Identity-H` codes, but moves logical text into a dedicated,
shardable font mapper.

```text
PdfLogicalUnit
    -> SemanticUnitKey(text + VisualUnitKey)
    -> LogicalFontMapper
    -> LogicalCIDFont shard
    -> Identity-H CID / synthetic GID
    -> /ToUnicode text
```

The semantic and visual data now have explicit responsibilities:

- `VisualUnitKey` contains only the advance and positioned glyph components needed to paint the
  synthetic glyph.
- `SemanticUnitKey` combines the exact text with that visual key and controls CID reuse.
- synthetic TrueType glyph construction receives no Unicode data.
- two units reuse a CID only when both their Unicode and visual representation match.
- equal visuals with different Unicode receive different CIDs, so `/ToUnicode` remains
  unambiguous.

Each source font owns a `LogicalFontMapper` separate from Krilla's ordinary `CIDFont`. When a
logical shard reaches its available TrueType glyph capacity, the mapper creates another
`LogicalCIDFont`. Every shard has its own PDF font resource and subset identity. CIDs and GIDs
remain 16-bit and standards-compatible; capacity scales through additional fonts rather than a
non-standard wider CID namespace.

Adjacent units are emitted in one `Tj` only when they use the same shard and their calculated
geometry is exactly contiguous. Units with gaps, vertical displacement, transforms, or reordered
visual positions retain separate text matrices. This reduces unnecessary PDF text operations
without changing shaped placement or assuming a writing system.

The ordinary `draw_glyphs` path is unchanged.

## Version 3

Version 3 keeps version 2's logical-unit, semantic-CID, and font-sharding model unchanged. It
optimizes only how already-planned logical units are written into page content streams.

```text
Version 2:  Tf Tm Tj   Tf Tm Tj   Tf Tm Tj
Version 3:  Tf Tm [text adjustment text adjustment text] TJ
```

Within one PDF text object, Krilla now retains the selected logical-font shard and emits `Tf`
only when the shard changes. Units on a compatible baseline and shard are collected into one
positioned `TJ` array. The array keeps character codes in authoritative logical order and uses
numeric adjustments to reproduce their independently shaped visual positions. This supports
ordinary left-to-right spacing and backward visual movement in right-to-left runs without
reordering the semantic text.

Batching stops at a shard transition, baseline change, invalid coordinate, or other placement
that cannot be expressed safely as a horizontal `TJ` adjustment. Tag and marked-content
boundaries remain outside this operation and are therefore not crossed.

Positioning adjustments are rounded to two decimal places in PDF text space. One text-space
unit is one thousandth of the font size, so the maximum rounding error per adjustment is
0.000005 em. This removes unstable floating-point tails, improves stream compression, and stays
far below display-pixel precision.

Version 3 therefore differs from version 2 in serialization efficiency, not in Unicode
identity, synthetic glyph construction, font capacity, tagging, or the public
`PdfLogicalUnit` API. It remains the universal logical path for supported fonts; no
script-specific or hybrid routing is required.

## Why version 2 does not use 32-bit character codes

A prototype separated a four-byte PDF source code from the visual CID through a custom Encoding
CMap. It allowed different Unicode strings to share one visual CID and greatly enlarged the
semantic code space. The prototype produced valid PDF/A and PDF/UA files and worked in Chromium,
Poppler, and other tested readers, but Adobe Acrobat indexed the real Khmer fixture differently.
It also found almost no useful visual-CID reuse in that document.

The additional mapping layer therefore added interoperability risk without solving a realistic
capacity requirement. Version 2 instead uses ordinary `Identity-H` fonts and reliable font
sharding. There is no `u32` PDF character-code path or custom Encoding CMap in the final design.

## Compatibility and limits

- The current synthetic-glyph path requires a TrueType `glyf` font that Krilla can synthesize.
- Each physical shard is limited by the TrueType 16-bit glyph namespace; the mapper creates a new
  shard before that namespace is exhausted.
- Search result counting and highlight geometry are viewer behaviors. Exact `/ToUnicode` text,
  successful extraction, valid font mappings, and conformance are the producer-side invariants.
- Reader-specific search differences should be recorded as interoperability fixtures, not fixed
  with script-specific text rewriting.

## Validation

The version 2 implementation is covered by unit tests for:

- exact semantic-unit reuse;
- distinct CIDs for identical visuals with different Unicode;
- deterministic shard creation with an artificial capacity;
- sharding at the actual TrueType glyph limit;
- conservative batching of contiguous horizontal units;
- rejection of batching across gaps, vertical changes, and RTL/reordered positions.

Version 3 additionally tests:

- forward and backward (`RTL`) `TJ` adjustment signs;
- rejection across incompatible baselines and invalid coordinates;
- stable two-decimal PDF positioning values; and
- benchmark coverage of font-state reuse and shard transitions.

The real Typst Khmer document is additionally checked for:

- pixel-identical rendering against version 1;
- exact Poppler extraction with no replacement characters;
- standard `Identity-H` font encoding;
- PDF syntax with qpdf;
- PDF/UA-1 and PDF/A-2b conformance with veraPDF.
