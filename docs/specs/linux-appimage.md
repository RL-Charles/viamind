# Verified Linux AppImage distribution

T-0093, S-094. Public-source specification before implementation.
Initial base: `cd19c8476c3b326c6fd3409eafbe6f60212cf1bb`.
An independent agent reviews this specification before packaging changes.

## Public contracts

The official [manual packaging guide](https://docs.appimage.org/packaging-guide/manual.html)
defines an AppDir with an executable AppRun, desktop entry, icon and application
payload. The [architecture reference](https://docs.appimage.org/reference/architecture.html)
describes a runtime followed by a filesystem image. The official
[appimagetool documentation](https://github.com/AppImage/appimagetool/blob/8c8c91f762b412a19f4e8d2c4b35afb98f2d7c81/README.md)
documents an explicit runtime file, architecture and output destination.
The [runtime build guide](https://github.com/AppImage/type2-runtime/blob/dd6cebedcbddde9c82f89b011e8e1d40b6e43868/BUILD.md)
provides an isolated Docker build. The
[runtime license](https://github.com/AppImage/type2-runtime/blob/dd6cebedcbddde9c82f89b011e8e1d40b6e43868/LICENSE)
lists its static library dependencies; its Makefile also links mimalloc.
The [FUSE troubleshooting guide](https://docs.appimage.org/user-guide/troubleshooting/fuse.html)
documents extraction when mounting is unavailable.

The independent application name, desktop artwork and existing license inventory
remain Viamind's. Packaging tools are identified by their actual upstream names.
No external EDA implementation or artwork is used.

## Deliverable and platform

Produce an executable x86_64 Type-2 AppImage from a fresh locked optimized Viamind
Linux bundle, preserving its complete application/license/provenance inventory.
AppRun resolves its own location and execs the application with every argument
unchanged, preserving the caller's working directory, standard streams, exit
status and caller-provided environment. It must work from paths containing spaces.
Runtime-owned `APPIMAGE`/`APPDIR` and documented extraction variables are explicit
environment exceptions; AppRun adds no application-specific environment overrides.
Headless MCP and native desktop launch use the same packaged application.

The image includes a valid desktop entry and a repository-native SVG icon. No
desktop files, application associations or host configuration are installed by
the packaging script or validation. Optional integration instructions are explicit.
No auto-update, signing key, credential or publication to a release is introduced.

Ubuntu 24.04 x86_64 CI is the initial distribution baseline. Record the actual
compiler, build host, glibc requirements and dynamic/runtime assumptions. System
GPU drivers, Wayland/X11 services and the documented host libraries remain runtime
requirements. A local Arch build must identify its actual newer host assumptions;
it cannot claim the CI baseline merely because it uses the same script. This
increment does not promise arbitrary-distribution portability or finish non-Linux
installer acceptance.

## Pinned tools and runtime source

Use appimagetool 1.9.1, source commit
`8c8c91f762b412a19f4e8d2c4b35afb98f2d7c81`, official x86_64 asset
SHA256 `ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0`.
Verify size and SHA before execution. Supply an explicit runtime file so the tool
cannot select a moving runtime release. Run its extraction path when FUSE is
unavailable. Download helpers use explicit HTTPS URLs, timeouts, finite byte caps
and NEW-only cache publication; cached files are rehashed before reuse.

Build the runtime from public source commit
`dd6cebedcbddde9c82f89b011e8e1d40b6e43868` in an isolated container with bounded
CPU resources. Pin and verify the source archive and base image digest before
execution. Its libfuse 3.15.0 and squashfuse 0.5.2 source archives already have
upstream-published SHA256 checks; verify those exact values. Record all actual
container package versions and hashes rather than claiming reproducibility from
mutable package repositories. Do not edit the host's package or system settings.

Retain and distribute the exact runtime source, patches, dependency source/license
texts and build instructions. Include a relink kit containing the runtime object,
the exact necessary static library/CRT objects and explicit link instructions,
allowing replacement of the statically linked LGPL libfuse. Retain a pinned
Alpine aports snapshot with matching actual static-package revisions and their
patches/build recipes, plus GCC source and Runtime Library Exception notices for
the libgcc/CRT objects. Relink outputs also use atomic NEW-only publication. Verify that relinking
from the distributed companion alone produces a functional runtime before publishing;
the verification cannot depend on files left in the original build tree. Preserve
the MIT, BSD, LGPL and other actual component notices, including mimalloc. Runtime
source/build and relink evidence are required gates, not a URL-only source promise.
The application itself remains covered by its existing license inventory.

## Publication and provenance

Use a new output directory and stage the complete image, checksum file, source/
relink companion and JSON manifest beside it. Publish the complete directory with
the existing Linux NEW-only atomic publication helper. Refuse every existing
destination, including directories and dangling symlinks. Failures remove only
the task's own staging files and preserve sources and existing artifacts.

Record exact application executable SHA, AppImage SHA, runtime SHA, tool/source
pins, source checkout revision/dirt, companion SHA, actual runtime assumptions
and observed verification results. Retain honest provenance after integration.
No bit-for-bit reproducibility claim is made. Packaging limits and archive reads
are bounded; extraction rejects escaping paths, links and special files.

## Required verification

Meaningful Python tests cover existing-destination protection, tool/cache digest
refusal before execution, wrong platform/architecture inputs, argument and working
directory preservation, archive path/link refusal, provenance consistency and
failed-stage cleanup. Existing Linux tar bundle tests and bytes remain valid.

Build the real optimized image. Verify checksum readback, image magic, extracted
application and license/source inventory, exact executable SHA and native host
requirements. With display variables removed, run actual image help/version and
MCP discovery/add/query/undo through its runtime extraction path. Assert exit
status and clean protocol stdout. Test explicit relative project/output arguments
from an unrelated working directory containing spaces.

Launch the actual image in the native Linux desktop and retain a readable capture
with precise runtime/application/image/process/window provenance. Verify two MCP
sessions share the actual packaged native application's design and undo state.
If the compositor allocates a tiny production window, report it honestly and use
a separate explicitly identified inspection host for readable rendering evidence.
No operating-system input or cadence claim follows from metadata-only snapshots.

CI produces downloadable image/checksums/provenance/source companions on Ubuntu
24.04, exercising the actual image without a display. Existing fmt, strict
all-target Clippy, workspace tests/build and final combined CI remain required.
Independent review precedes merge; no ticket-done transition is performed.
