#!/bin/sh
#
#   allocscope  -  a memory tracking tool
#   Copyright (C) 2023  Matt Kimball
#
#   This program is free software: you can redistribute it and/or modify it
#   under the terms of the GNU General Public License as published by the
#   Free Software Foundation, either version 3 of the License, or (at your
#   option) any later version.
#
#   This program is distributed in the hope that it will be useful, but
#   WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
#   or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License
#   for more details.
#
#   You should have received a copy of the GNU General Public License along
#   with this program. If not, see <https://www.gnu.org/licenses/>.
#

export CFLAGS="-I/extern/include -I/extern/include/libiberty -I/extern/include/ncurses"
export BINDGEN_EXTRA_CLANG_ARGS="-I/extern/include -I/usr/include/x86_64-linux-musl"
export RUSTFLAGS="-C target-feature=+crt-static -L/extern/lib -L/usr/lib/x86_64-linux-musl -l static=ncurses -l static=iberty -l static=sqlite3 -l static=c -l gcc_eh"

mkdir /build
cd /build
git clone /mnt/src .

cd /build
cargo build -r --target x86_64-unknown-linux-musl

BUILDDIR=/build/target/x86_64-unknown-linux-musl/release
cd /
cp $BUILDDIR/allocscope-trace $BUILDDIR/allocscope-view /usr/local/bin
ldd /usr/local/bin/allocscope-trace /usr/local/bin/allocscope-view

VERSION=$(/usr/local/bin/allocscope-trace --version | awk '{ print $2 }')
TARFILE=allocscope-$VERSION-static.tar.gz
mkdir -p /mnt/src/build-static/release
tar zcf /mnt/src/build-static/release/$TARFILE usr/local/bin/allocscope-trace usr/local/bin/allocscope-view usr/local/share/allocscope

echo built build-static/release/$TARFILE
