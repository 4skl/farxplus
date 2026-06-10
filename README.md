# FARxPlus

A modern tool to manage The Maxis FAR 1a Format Structure, built with Rust and GTK4.

## Requirements to build

Before building the project with Cargo, you must have the native GTK4 development libraries and `pkg-config` installed on your system.

### Windows

Use [MSYS2](https://www.msys2.org/) to install the required MinGW-w64 packages. Open the **MSYS2 MinGW64** (or UCRT64) terminal and run:

```bash
pacman -S mingw-w64-x86_64-gtk4 mingw-w64-x86_64-pkgconf
```
*Note: Make sure your Rust toolchain is configured for the GNU ABI (`x86_64-pc-windows-gnu`) so it can interface with the MSYS2 libraries.*

### Linux

Depending on your distribution, install the GTK4 development headers and build tools:

- Ubuntu / Debian / Pop!_OS / Linux Mint:
    ```bash
    sudo apt update
    sudo apt install libgtk-4-dev build-essential pkg-config
    ```

- Fedora:
    ```bash
    sudo dnf install gtk4-devel gcc pkgconf-pkg-config
    ```

- Arch Linux / Manjaro:
    ```bash
    sudo pacman -S gtk4 pkgconf base-devel
    ```

### MacOS

Use [Homebrew](https://brew.sh/) to install GTK4 and the `pkg-config` tool:

```bash
brew install gtk4 pkg-config
```

## Building and Running

Once the native dependencies are installed for your OS, you can build and run the application using standard Cargo commands:

```bash
# To run directly in debug mode
cargo run

# To compile a finalized release build
cargo build --release
```