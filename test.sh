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

cargo build
if [ $? != 0 ]
then
    exit $?
fi

export ALLOCSCOPE_ROOT=$(dirname $(realpath $0))
export BUILD_PATH=$ALLOCSCOPE_ROOT/target/debug
export TEST_ALLOCSCOPE_TRACE=$BUILD_PATH/allocscope-trace
export TEST_ALLOCSCOPE_VIEW=$BUILD_PATH/allocscope-view
export TEST_TRACEE_PATH=$ALLOCSCOPE_ROOT/integration-test/tracee
export CC=cc
export CXX=c++
export RUSTC=rustc

cargo test -p integration-test $*
