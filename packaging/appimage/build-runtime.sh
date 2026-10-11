#!/bin/bash
# Build the MIT AppImage runtime with its public pinned libfuse/squashfuse sources.
set -euo pipefail
mkdir -p /work /result/relink/lib /result/relink/crt /result/sources
cp /inputs/* /result/sources/
cp -R /runtime/patches /result/sources/runtime-patches
cp /runtime/LICENSE /result/RUNTIME-LICENSE
apk info -v > /result/installed-packages.txt
cp /lib/apk/db/installed /result/installed-package-metadata.txt
cd /work
tar xf /inputs/fuse-3.15.0.tar.xz
cd fuse-3.15.0
patch -p1 < /runtime/patches/libfuse/mount.c.diff
meson setup build --prefix=/usr --default-library=static
ninja -C build -j2 install
cd /work
tar xf /inputs/squashfuse-0.5.2.tar.gz
cd squashfuse-0.5.2
./autogen.sh
CFLAGS='-ffunction-sections -fdata-sections -Os' ./configure LDFLAGS=-static
make -j2
make install
mkdir -p /usr/local/include/squashfuse
install -m644 ./*.h /usr/local/include/squashfuse/
cd /runtime/src/runtime
test -f version
make -j2 runtime
cp runtime /result/runtime-upstream
clang -std=gnu99 -Os -D_FILE_OFFSET_BITS=64 -DGIT_COMMIT="\"$(cat version)\"" \
    -ffunction-sections -fdata-sections -Wall -Werror \
    -I/usr/local/include/squashfuse -I/usr/include/fuse3 \
    -c runtime.c -o /result/relink/runtime.o
cp data_sections.ld /result/relink/
for name in squashfuse squashfuse_ll zstd z fuse3 mimalloc c m; do
    file=$(find /usr/lib /usr/local/lib -name "lib${name}.a" -print -quit)
    test -n "$file"
    cp "$file" "/result/relink/lib/lib${name}.a"
done
for name in gcc gcc_eh; do
    file=$(find /usr/lib/gcc -name "lib${name}.a" -print -quit)
    test -n "$file"
    cp "$file" "/result/relink/lib/lib${name}.a"
done
for name in rcrt1.o crti.o crtn.o; do cp "/usr/lib/$name" /result/relink/crt/; done
for name in crtbeginS.o crtendS.o; do
    file=$(find /usr/lib/gcc -name "$name" -print -quit)
    test -n "$file"
    cp "$file" /result/relink/crt/
done
cat > /result/relink/relink.sh <<'EOF'
#!/bin/sh
set -eu
kit=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
out=${1:?provide a NEW output runtime path}
test ! -e "$out" && test ! -L "$out"
stage=$(mktemp -d "${out}.relink.XXXXXX")
trap 'rm -rf -- "$stage"' EXIT HUP INT TERM
ld -static -pie --no-dynamic-linker --gc-sections -T "$kit/data_sections.ld" \
    -o "$stage/runtime" "$kit/crt/rcrt1.o" "$kit/crt/crti.o" "$kit/crt/crtbeginS.o" \
    "$kit/runtime.o" -L"$kit/lib" -lsquashfuse -lsquashfuse_ll -lzstd -lz \
    -lfuse3 -lmimalloc -lgcc -lgcc_eh -lc -lgcc -lgcc_eh \
    "$kit/crt/crtendS.o" "$kit/crt/crtn.o"
python3 - "$stage/runtime" "$out" <<'PY'
import os,pathlib,sys
p=pathlib.Path(sys.argv[1])
with p.open('r+b') as f:
    f.seek(8);f.write(b'AI\x02');f.flush();os.fsync(f.fileno())
p.chmod(0o755)
os.link(p, sys.argv[2])  # Atomic NEW-only publication, including racing symlinks.
PY
EOF
chmod 755 /result/relink/relink.sh
