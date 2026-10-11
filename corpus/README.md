# Regression corpus

Real, openly licensed KiCad projects that Viamind's importers, renderers and
checks are tested against. The corpus spans KiCad 6 to 10 file formats, 2 to 14
copper layers, flat and hierarchical schematics (including repeated sheet
instances), impedance-controlled differential pairs, fine-pitch BGAs and
blind/micro vias.

`manifest.toml` is the only thing committed. Design files are fetched on demand
and never checked in to this repository.

## Fetching

```sh
scripts/fetch-corpus.sh               # every entry
scripts/fetch-corpus.sh glasgow-revd1 # just the named entries
```

Needs `git` and `python3` (3.11+, for `tomllib`). Each project lands in
`corpus/cache/<name>/` (gitignored) at its pinned commit. Only KiCad sources are
checked out (`*.kicad_pro/sch/pcb/sym/mod/dru/wks` and library tables) using a
shallow, blob-filtered, sparse fetch. The full set is about 700 MB, most of it
in a few large high-layer-count boards. Entries already at their pinned commit
are skipped. The script fails if a fetch fails or a listed project's
`.kicad_pro`, `.kicad_sch` or `.kicad_pcb` is missing. Git submodules are not
fetched. KiCad boards and schematics embed their footprints and symbols, so
they load without them.

## Licences

Each project keeps its own licence. The `license` field records what the
upstream repository states (GitHub licence field or LICENSE file) as an SPDX
id, and `license_note` adds detail where the repo's terms are path-specific
or carry extra conditions. Check the upstream LICENSE before redistributing
anything from `corpus/cache/`. Fetching a project for local testing does not
relicense it, and nothing fetched may be copied into Viamind's own sources (see
`CLEAN_ROOM.md`).

## Adding an entry

1. Pick a repo with a clear, redistributable licence and at least one KiCad 6+
   project (`.kicad_pro` with its `.kicad_sch` and `.kicad_pcb`). Skip repos with
   no licence or a non-commercial / no-derivatives licence.
2. Pin the current commit: `gh api repos/<owner>/<repo>/commits/<branch> --jq .sha`.
3. Append a `[[project]]` block:

   | Field | Meaning |
   | --- | --- |
   | `name` | Unique kebab-case id; also the cache directory name |
   | `repo`, `commit` | Clone URL and full 40-char SHA |
   | `license`, `license_note` | SPDX id as the repo states it; optional detail |
   | `projects` | `.kicad_pro` path(s) relative to the repo root |
   | `kicad_version`, `copper_layers`, `sheets` | Major version that saved the board; counts |
   | `note` | One line on what the design exercises |

4. Run `scripts/fetch-corpus.sh <name>` and make sure it passes.
5. Prefer designs that add something the corpus lacks, such as a new file
   version, a stackup, a hierarchy shape or a routing feature.
