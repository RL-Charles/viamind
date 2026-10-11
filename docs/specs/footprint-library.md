# Native rectangular SMD footprint libraries

## Public references and scope

KiCad's public [footprint format](https://dev-docs.kicad.org/en/file-formats/sexpr-footprint/index.html)
documents a standalone named footprint and its pads. Its
[coordinate conventions](https://dev-docs.kicad.org/en/file-formats/sexpr-intro/)
describe local footprint geometry. These establish the behavioral distinction
between reusable local geometry and a placed component. This implementation uses
an independently specified native JSON format; it does not read KiCad files or
claim IPC footprint compliance.

## Contract

- Envelope: `format: "viamind-footprint"`, `version: 1`, `footprint` with
  `name` and ordered `pads`. Each pad has `number`, integer Nm `position`,
  `width`, `height`. Unknown fields, unsupported versions, repeated numbers,
  invalid names/dimensions and more than 1024 pads are rejected.
- Names are trimmed, nonempty, <=128 characters, without controls. Local pad
  origins are bounded to +/-10000 mm; sizes are >0 and <=10000 mm. Empty explicit
  pad collections are valid. Only rectangular SMD pads are represented.
- Files are UTF-8 JSON, bounded to 1 MiB on read. The library carries no UUIDs,
  net assignments, component placement, rotation or board side.
- Applying requires exact project identity and part snapshot. Matching pad
  numbers retain their identities and net assignments; new numbers receive new
  identities and no net. Removed numbers disappear. Name and geometry change in
  one undoable transaction; all component placement fields remain unchanged.
- Desktop and MCP use the same application command. File reads happen before
  guarded application. Export uses an immutable snapshot of committed pads;
  legacy visual preview pads cannot be exported as electrical geometry.
- Exports atomically publish a new file, refuse destination symlinks/conflicts,
  and protect the active native project path (including canonical aliases).
  Users may choose a new path; no implicit overwrite or library installation.
- Desktop library controls live in the electrical pads editor, explain committed
  geometry export, and reload after successful import. A concurrent part edit
  rejects import without changing either edit.

Courtyard/paste, arbitrary shapes, through-hole pads, library browsing and
parametric generation remain separate roadmap work.
