# Initial Linux application bundle

## Public references

- [GitHub artifact action](https://github.com/actions/upload-artifact): workflow
  artifacts retain a build's downloadable files without publishing a release.
- [Desktop Entry specification](https://specifications.freedesktop.org/desktop-entry-spec/latest/):
  launcher keys and executable arguments.
- [Cargo build](https://doc.rust-lang.org/cargo/commands/cargo-build.html): locked,
  optimized native application compilation.

## Behavior

Build the `viamind` desktop and headless executable on Linux with the locked
dependency graph. Package a new output directory containing a version/revision
named tar.gz and SHA256 manifest. Refuse an existing destination before building;
publish completed files together and clean temporary files on failure.

The archive contains executable `viamind`, launch instructions, a desktop-entry
template, repository license/notice/attribution, dependency license texts and an
index of the resolved application dependency closure. Include normal and build
dependencies; exclude dev-only dependencies. Record compiler, host target, source
revision, glibc and binary hash in build metadata. Validate help/version without
a display and verify the archive's executable and checksum after extraction.
Exercise actual stdio MCP initialization, discovery, add/query/undo and clean EOF
shutdown on the packaged binary with no display variables.

CI runs on Ubuntu 24.04 x86_64 and uploads the tarball/checksum after verification.
This bundle uses host system graphics/input libraries and glibc. Document that
local builds inherit the local glibc baseline; it is not a universal static
binary or an AppImage/Flatpak. Desktop registration is an explicit user action.
MCP defaults remain the application's documented loopback/stdio behavior.
