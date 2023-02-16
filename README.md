![allocscope banner](https://allocscope.com/banner.png)

# allocscope
### a memory tracking tool

allocscope is a tool for tracking down where the most egregiously large allocations are occurring
in a C, C++ or Rust codebase.  It is particilarly intendend to be useful for developers who want to
get a handle on excessive allocations and are working in a large codebase with multiple
contributors with allocations occuring in many modules or libraries.

It is composed of two commands:

`allocscope-trace` attaches to another process as a debugger.  By using breakpoints on memory
allocation functions such as `malloc` it tracks allocations made by that process.  During the
trace, the callstack of all allocations are recorded to an `.atrace` file.  Tracing programs
which spawn multiple threads and tracing calls through shared libraries are supported.  You can
spawn a process to trace by specifying a full commandline to `allocscope-trace`, or you can
attach to an existing running process.

`allocscope-view` reads the `.atrace` file produced by `allocscope-trace`.  It presents a summary
of all allocations made in a call tree format, which can be sorted by largest concurrent
allocation, total number of blocks, number of unfreed allocation blocks, or the sequence of
the allocation.  The summary can be navigated interactively through a curses-based terminal user
interface, or a text report suitable for non-interactive use can be generated.

## Installing prebuilt binaries

The easiest way to get started with allocscope is to install prebuilt binaries.

To install the latest version:

`curl -s https://allocscope.com/install.sh | sudo sh`

Currently only Linux on x86_64 processors is supported, but I'd like to support more operating systems
and processors in the future.

## Getting started

While it will likely be most useful to use allocscope on a program with symbols, which you
have compiled yourself, you can verify that it functions correctly by performing a trace on a 
standard system command, such as `ls`:

```
allocscope-trace ls -l
allocscope-view ls.atrace
```

## Building from source

On recent Ubuntu releases, allocscope can be built from source with the following sequence
of commands:

```
apt-get update
apt-get install cargo git libclang-dev libiberty-dev libncurses-dev libsqlite3-dev libunwind-dev
git clone https://github.com/matt-kimball/allocscope.git
cd allocscope
cargo install --path allocscope-trace
cargo install --path allocscope-view
```

Statically linked binaries can also be built using the `build-static/build.sh` script, though this requires
Docker installed on the build system.

## Support development

If you find allocscope useful, please consider supporting development.

Visit https://allocscope.com/support

## License

allocscope is licensed GNU General Public License version 3.
