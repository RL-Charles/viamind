# Linux AppImage

T-0093 packages the current locked optimized x86_64 GNU/Linux application as a
Type-2 AppImage. CI builds on Ubuntu 24.04. Local builds record their actual host;
a newer Arch build carries newer glibc requirements. This does not promise
arbitrary Linux distribution compatibility.

## Build

Use Python 3.12+, Rust, Docker, GNU binutils, and the Linux development libraries
listed in the Linux bundle workflow. Docker builds only the public runtime
sources; it receives no home directory, application checkout or credentials.
The existing application bundle builds with Cargo on the host.

From the repository root, choose new output directories whose parents exist:

```sh
python3 scripts/build-appimage-runtime.py /path/to/NEW-runtime
python3 scripts/package-appimage.py /path/to/NEW-appimage --runtime /path/to/NEW-runtime
```

The second command builds and verifies a fresh optimized Linux bundle. To reuse
an explicitly supplied bundle from `scripts/package-linux.py`, add
`--bundle /path/to/existing-verified-bundle`. Its archive, recorded executable SHA,
host and existing headless verification are checked before packaging; the actual
resulting image is independently executed and checked again.

The scripts cache finite, SHA256-pinned HTTPS inputs in
`~/.cache/viamind/appimage-inputs`; `--cache` changes that directory. `--offline`
requires every selected input and the exact prepared Docker image to be cached,
and uses locked offline Cargo dependencies when building the application.
Source URLs, archive hashes and explicit byte caps live in
[inputs.json](../packaging/appimage/inputs.json).

All output publication is atomic and refuses existing destinations, including
files, directories and dangling symlinks. Failures remove only task-owned staging.
No desktop files, associations, system libraries or release assets are installed.
The runtime build uses two compile jobs, and image compression uses two workers.

## Run

Keep the image executable. Run it from a terminal or your file manager:

```sh
chmod +x ./viamind-*-x86_64.AppImage
./viamind-VERSION-REVISION-x86_64.AppImage --open ./board.viamind.json
```

When FUSE mounting is unavailable, the runtime's extraction route works without
installing FUSE or changing desktop settings:

```sh
./viamind-VERSION-REVISION-x86_64.AppImage --appimage-extract-and-run --open './my board.viamind.json'
./viamind-VERSION-REVISION-x86_64.AppImage --appimage-extract-and-run --headless
```

AppRun preserves argument boundaries, the caller's current directory, standard
streams and exit status. It adds no application environment overrides. The
runtime owns `APPIMAGE`, `APPDIR` and its documented extraction variables.
Relative project and output paths resolve against the caller's directory.
MCP configuration remains the application's existing configuration, including
`VIAMIND_MCP_PORT` for the native process. Headless MCP uses clean JSON-RPC stdio.

System GPU drivers, Wayland/X11 services and the application libraries listed in
`build-info.json` remain host requirements. The static AppImage runtime does not
make the Viamind application or its GPU dependencies statically linked.

## Contents and verification

The new output directory contains the executable image, a runtime source/relink
archive, `appimage-info.json`, and `SHA256SUMS`. Check all distributed files:

```sh
cd /path/to/appimage-output
sha256sum -c SHA256SUMS
```

Inside the image, `usr/lib/viamind` retains the original application's complete
license/build inventory. `usr/share/viamind-runtime` retains exact runtime sources,
libfuse patches, upstream notices, Alpine source recipes/patches and installed
package metadata, plus a static relink kit. The external runtime companion
contains the same inventory. GCC source and Runtime Library Exception notices
accompany the GCC CRT/libgcc objects. No build reproducibility claim is made for
resolved mutable Alpine package repositories.

On x86_64 Linux with GNU `ld`, Python 3 and a POSIX shell, extract the companion
and relink independently of the original Docker build:

```sh
tar xf viamind-VERSION-REVISION-x86_64-runtime-source-relink.tar.gz
./viamind-runtime/relink/relink.sh /absolute/path/to/NEW-runtime
/absolute/path/to/NEW-runtime --appimage-version
```

Replace `relink/lib/libfuse3.a` with a compatible rebuilt library to relink a
modified libfuse. The original source archives, applied patch and recipes are
retained. Relink outputs also refuse existing destinations atomically.

Packaging verifies the actual image's executable hashes, help/version, no-display
MCP discovery/add/query/undo, relative input/output arguments from a directory
containing spaces, and relinking from both embedded and external companions
alone. Meaningful Python tests cover corrupted inputs, archive paths and links,
finite expansion, publication races, process argument/environment/stream/exit
semantics and failed-stage cleanup. CI publishes downloadable Ubuntu 24.04
artifacts with 14-day retention; this workflow does not publish a GitHub release.

See [native verification and exact artifact provenance](linux-appimage-verification.md)
for the actual Ubuntu image, shared native MCP checks and rendering limitations.
