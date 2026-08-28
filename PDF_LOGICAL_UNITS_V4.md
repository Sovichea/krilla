# PDF Logical Units — Version 4 Architecture

> **Enhanced Unicode Engine equivalent:** v0.4.0  
> **Implementation commit:** `26360e8604e4d69f7506c1c0d186c482942264d2`

## Overview

Version 4 preserves Version 3's public API, semantic ordering, visual
positioning, `TJ` batching, and standard two-byte `Identity-H` character
codes. It changes the physical font representation behind each logical CID.

Versions 1 through 3 append a synthetic glyph for every unique visual logical
unit after the source font's complete glyph namespace. Version 4 instead builds
a compact derived TrueType font for each logical-font shard:

```text
authoritative Unicode
    -> semantic CID and /ToUnicode
    -> visual unit
         -> exact single source glyph: compact source GID
         -> otherwise: compact synthetic GID
    -> /CIDToGIDMap
    -> embedded compact glyph
```

The central invariant is:

> Every logical unit uses the smallest glyph representation that preserves its
> exact Unicode semantics, advance, components, and visual placement.

## What Version 3 got wrong

Version 3 uses one synthetic glyph for every unique visual unit, even when the
source font already contains an exact visual representation. It also allocates
those synthetic glyphs after the source font's original GID range.

This creates two unnecessary costs:

1. Ordinary one-to-one glyphs are copied into synthetic glyphs.
2. A source font with a large GID namespace leaves little or no room for
   synthetic glyphs, even when the document uses only a small subset.

For example, a source font may contain almost 65,535 glyphs while a document
uses only 100 of them. Version 3 still treats the full source namespace as
occupied. Version 4 embeds and counts only the glyphs the document actually
uses.

## Independent namespaces

Version 4 separates three identities:

```text
PDF character code = semantic CID
semantic CID        -> authoritative Unicode through /ToUnicode
semantic CID        -> compact embedded GID through /CIDToGIDMap
compact GID         -> visual outline
```

The PDF character code remains the semantic CID. `/Encoding /Identity-H`
therefore remains unchanged and content streams continue to use standard
two-byte codes.

The descendant `CIDFontType2` no longer uses `/CIDToGIDMap /Identity`. It
contains an explicit map from each semantic CID to the corresponding compact
embedded GID. This permits multiple CIDs with different `/ToUnicode` strings to
share one visual glyph without making extraction ambiguous.

No four-byte character codes or custom Encoding CMap are introduced.

## Source-backed and synthetic visuals

A visual logical unit reuses a source glyph only when all of these conditions
hold:

- it contains exactly one source component;
- the component has zero horizontal and vertical offset; and
- the logical advance equals the source glyph's nominal advance.

```text
one source component + exact position + exact advance
    -> compact source-backed glyph
```

All other units retain the synthetic representation:

```text
multiple components
or positioned component
or changed advance
    -> compact synthetic composite glyph
```

The choice is based only on shaped geometry and metrics. It does not inspect or
special-case the script, language, or Unicode content.

## Compact derived font

Each logical-font shard embeds only:

- `.notdef`;
- source glyphs referenced by its visual units;
- transitive component dependencies of referenced composite glyphs; and
- one synthetic glyph for every unique visual construction that cannot reuse a
  source glyph exactly.

Source GIDs are remapped densely during subsetting. Synthetic glyphs are then
appended to that compact source subset. Consequently, a high source GID does
not become a high embedded GID.

```text
source GIDs used by the document:  19, 4200, 65000
compact embedded GIDs:              1,    2,     3
synthetic visual:                   4
```

`VisualUnitKey` controls visual deduplication. Unicode text never participates
in the compact glyph identity. `SemanticUnitKey`, containing both text and the
visual key, continues to control CID reuse and `/ToUnicode` identity.

## Capacity and sharding

Version 4 tracks two independent per-shard limits:

1. semantic CIDs; and
2. compact embedded GIDs.

Embedded-GID accounting follows the transitive closure of TrueType composite
components and counts genuinely synthetic visuals. It does not use the source
font's original glyph count.

When either namespace reaches 65,535 entries, the mapper creates another
logical-font shard. A nearly full CJK source font therefore does not force
immediate sharding when the document uses only a small set of glyphs.

A single synthetic visual and all of its source dependencies must still fit in
one physical TrueType font. One composite glyph cannot be split across font
resources.

## What is unchanged from Version 3

- The public `PdfLogicalUnit` API.
- Authoritative Unicode and logical source order.
- Shaped component geometry and visual origins.
- Standard two-byte `Identity-H` content codes.
- `/ToUnicode` as the semantic mapping authority.
- `Tf` state retention and safe `TJ` positioning.
- Tagged-content and marked-content boundaries.
- The ordinary `draw_glyphs` path.
- Script-independent behavior.

## Properties summary

- Exact one-to-one visuals reuse compact source glyphs.
- Complex visuals retain synthetic composite glyphs.
- Different semantic CIDs may safely share one embedded GID.
- Source and embedded GID numbers are independent.
- Large source fonts consume capacity only for glyphs actually embedded.
- Sharding remains a standards-compatible fallback rather than requiring wider
  PDF character codes.

## Validation

The implementation is covered by unit tests for:

- repeated semantic-unit CID reuse;
- distinct semantic CIDs sharing one visual glyph;
- distinct visuals receiving distinct compact GIDs;
- exact source-backed reuse;
- synthetic fallback for changed advances;
- synthetic construction from multiple source components;
- semantic-CID capacity sharding; and
- compact embedded-GID capacity sharding.

The controlled Typst corpus covers English, Khmer, Arabic, Devanagari, and
mixed multilingual prose. Against Version 3:

- all 85 rendered pages are pixel-identical;
- Poppler raw extraction is byte-identical for every fixture;
- every generated PDF passes qpdf syntax validation;
- the combined PDF/A-2b + PDF/UA-1 mixed document passes both veraPDF profiles;
- embedded glyph counts fall from 2,707 to 2,279; and
- each fixture's PDF size remains within 1.01% of Version 3.

The PDF-size result is intentionally secondary. The primary benefit is that
logical text no longer consumes synthetic or embedded glyph capacity when an
exact source-backed representation already exists.
