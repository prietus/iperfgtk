# iperf_rust

A **modern, native Linux** graphical frontend for [`iperf3`](https://iperf.fr/),
written in Rust with **GTK4 + libadwaita**.

It does not bundle the iperf3 binary: it invokes it from your `PATH`. You only
need `iperf3` installed.

## Features

- **Client mode** with an **analog VU-meter** whose needle moves smoothly
  (~60 fps) and **without jumps**: even though iperf3 reports once per second,
  the needle interpolates toward the target with exponential smoothing
  (critically damped spring). The dial **auto-scales** and holds the peak.
- **Server mode** that detects when a client connects ("Accepted connection
  from …"), shows its IP, and reflects the incoming transfer speed live on the
  same VU-meter.
- Native **libadwaita** interface: header with a view switcher, modern
  preference rows, automatic light/dark system theme support.
- Options: host/port, duration, parallel streams (`-P`), download mode
  (`-R`/reverse), and UDP (`-u`).

## Requirements

- `iperf3` in your `PATH` (`sudo apt install iperf3` / `sudo dnf install iperf3` /
  `sudo pacman -S iperf3`).
- GTK4 ≥ 4.10 and libadwaita ≥ 1.4 (the `-devel`/`-dev` packages to build).
- Stable Rust (`cargo`).

### Build dependencies per distro

```sh
# Debian/Ubuntu
sudo apt install build-essential libgtk-4-dev libadwaita-1-dev iperf3
# Fedora
sudo dnf install gcc gtk4-devel libadwaita-devel iperf3
# Arch
sudo pacman -S base-devel gtk4 libadwaita iperf3
```

## Build and run

```sh
cargo run --release
```

## Install (optional)

```sh
cargo build --release
sudo install -Dm755 target/release/iperf_rust /usr/local/bin/iperf_rust
sudo install -Dm644 data/io.github.iperf_rust.desktop \
    /usr/share/applications/io.github.iperf_rust.desktop

# Icon (all hicolor theme sizes)
for s in 16 32 48 64 128 256 512; do
    sudo install -Dm644 "data/icons/hicolor/${s}x${s}/apps/io.github.iperf_rust.png" \
        "/usr/share/icons/hicolor/${s}x${s}/apps/io.github.iperf_rust.png"
done
sudo gtk-update-icon-cache -f /usr/share/icons/hicolor
```

## How it works

iperf3 with `-J` (JSON) does **not** stream: it buffers everything and prints it
at the end, which would make it impossible to animate the needle in real time.
That is why this frontend parses the human-readable output line by line using
`--forceflush`, which flushes the buffer on each 1 s interval. Communication from
the reader thread to the UI uses an async channel (`async-channel`) consumed on
the GLib main loop.

## License

[MIT](LICENSE) © 2026 teraflops
