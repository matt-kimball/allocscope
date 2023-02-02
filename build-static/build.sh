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

BUILD_DIR=$(readlink -f $(dirname $0))
ALLOCSCOPE_DIR=$(dirname $BUILD_DIR)

docker pull rust:1.67-buster
docker build -t allocscope-static $BUILD_DIR
docker run -ti -v $ALLOCSCOPE_DIR:/mnt/src allocscope-static sh /mnt/src/build-static/build-inside-container.sh
