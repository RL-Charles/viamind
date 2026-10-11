# Linux application bundle

Extract the tar.gz into a directory you own. From the extracted directory:

```sh
sha256sum -c SHA256SUMS
./viamind
./viamind --open /path/to/design.viamind.json
./viamind --headless
```

The desktop opens a shared design and serves MCP at
`http://127.0.0.1:7878/mcp`. Set `VIAMIND_MCP_PORT` to choose another port, or pass
`--no-mcp` for a desktop without the HTTP endpoint. `--headless` serves MCP over
stdin/stdout without a display. Run `./viamind --help` for all current options.

## Runtime requirements

The CI bundle targets x86_64 Linux and is built on Ubuntu 24.04. It requires glibc
and system graphics/input libraries, a Vulkan-capable driver for the desktop,
and a Wayland or X11 session. `build-info.json` records the actual compiler,
glibc, host target, revision, binary hash and linked libraries. Locally generated
bundles inherit that machine's runtime baseline; a newer glibc build may fail on
older distributions. The bundle includes Rust dependency license texts and an
index in `licenses/`. It does not bundle system graphics drivers or libraries.

For optional desktop-menu registration, first put the extracted `viamind`
executable on your PATH, then copy `viamind.desktop` into
`~/.local/share/applications/`. Launching from the extracted directory requires
no installation. [AppImage packaging and extraction](linux-appimage.md) are also
available. Flatpak and macOS/Windows remain future work.

## Build a bundle from source

Requires a native GNU/Linux Rust toolchain, Python 3.12+, Cargo dependencies and
the normal Linux build libraries. Choose a new output directory whose parent
exists:

```sh
python3 scripts/package-linux.py /tmp/viamind-linux-build
cd /tmp/viamind-linux-build
sha256sum -c SHA256SUMS
```

Add `--offline` when dependencies are already cached. The script builds the
locked optimized application, verifies help/version without a display, reads
the archive back, checks headless MCP discovery/edit/query/undo and clean exit,
and publishes the complete output directory without replacing
existing paths. GitHub's **Linux bundle** workflow retains the tarball and
checksum as a downloadable build artifact; this does not create a public release.

## Artifact source provenance

`build-info.json` records the exact checked-out Git `revision` used to build the
binary. For pull-request workflow runs, GitHub checks out a synthetic merge commit
combining the branch with its base; that revision can differ from the PR branch
head. Runs on `main` and manually selected refs record their actual checkout.
Use the workflow run and its checkout revision when identifying an artifact.

`source_dirty` reports tracked edits and untracked source files at packaging time.
The workflow disables Python bytecode generation with
`PYTHONDONTWRITEBYTECODE=1` and checks that its unit tests leave the checkout clean.
The packaging script still reports real source dirt, including untracked files;
it does not hide generated files through new ignore rules. For the same clean
local test workflow, run:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -p test_package_linux.py
git status --porcelain
```
