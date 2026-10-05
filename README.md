# kansha
Personal finance app

## Build setup

Needs Rust (MSRV 1.93), Node 22, and [`just`](https://just.systems).
The SQLCipher build compiles OpenSSL from source, so it also needs Perl
and a C toolchain.

### Windows

1. Visual Studio Build Tools with the "Desktop development with C++"
   workload. WebView2 ships with Windows 11.
2. `winget install StrawberryPerl.StrawberryPerl Casey.Just`
   (Git's bundled Perl does not work). Open a new terminal afterwards.
3. `perl -v` must report Strawberry Perl; `just --version` must work.
4. If the linker picks up Cygwin's `link.exe`, use "Developer PowerShell
   for VS" or put the MSVC tools before Cygwin on `PATH`.

### Kubuntu

```
sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev libssl-dev libxdo-dev
```

### Then

```
npm install
just check   # fmt, clippy, tests, svelte-check
just dev     # run the app
```
