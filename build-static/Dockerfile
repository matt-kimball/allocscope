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

FROM rust:1.67-buster
RUN apt-get update
RUN apt-get install -y musl-tools libclang-13-dev tcl
RUN rustup target add x86_64-unknown-linux-musl

ENV CC=musl-gcc
ENV CXX=musl-gcc
ENV CFLAGS="-I/usr/include/x86_64-linux-musl"

RUN mkdir -p /extern/src

RUN git clone -b v1.6.2 https://github.com/libunwind/libunwind.git /extern/src/libunwind
WORKDIR /extern/src/libunwind
RUN autoreconf -i
RUN ./configure --prefix=/extern --disable-shared
RUN make install

RUN git clone -b version-3.40.1 https://github.com/sqlite/sqlite.git /extern/src/sqlite
RUN mkdir /extern/src/sqlite/build
WORKDIR /extern/src/sqlite/build
RUN ../configure --prefix=/extern --disable-readline --disable-shared
RUN make install

RUN wget https://ftp.gnu.org/gnu/gcc/gcc-12.2.0/gcc-12.2.0.tar.gz -P /extern/src
RUN tar xf /extern/src/gcc-12.2.0.tar.gz -C /extern/src
WORKDIR /extern/src/gcc-12.2.0/libiberty
RUN ./configure --prefix=/extern --enable-install-libiberty
RUN make install

RUN wget https://ftp.gnu.org/pub/gnu/ncurses/ncurses-6.2.tar.gz -P /extern/src
RUN tar xf /extern/src/ncurses-6.2.tar.gz -C /extern/src
WORKDIR /extern/src/ncurses-6.2
RUN ./configure --libdir=/extern/lib --includedir=/extern/include --datarootdir=/usr/local/share/allocscope --without-cxx-binding
RUN make install.libs install.includes install.data

WORKDIR /
