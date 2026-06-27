# iperf_rust

Frontend gráfico **moderno y nativo de Linux** para [`iperf3`](https://iperf.fr/),
escrito en Rust con **GTK4 + libadwaita**.

No incrusta el binario de iperf3: lo invoca desde el `PATH`. Solo necesitas tener
`iperf3` instalado.

## Características

- **Modo cliente** con un **VU-meter analógico** cuya aguja se mueve de forma
  fluida (~60 fps) y **sin saltos**: aunque iperf3 entrega una lectura por
  segundo, la aguja interpola hacia el objetivo con un suavizado exponencial
  (resorte críticamente amortiguado). La esfera se **auto-escala** y retiene el
  pico.
- **Modo servidor** que detecta cuándo un cliente se conecta ("Accepted
  connection from …"), muestra su IP y refleja en vivo la velocidad de la
  transferencia entrante en el mismo VU-meter.
- Interfaz **libadwaita** nativa: cabecera con conmutador de vistas, filas de
  preferencias modernas, soporte automático de tema claro/oscuro del sistema.
- Opciones: host/puerto, duración, flujos paralelos (`-P`), modo descarga
  (`-R`/reverse) y UDP (`-u`).

## Requisitos

- `iperf3` en el `PATH` (`sudo apt install iperf3` / `sudo dnf install iperf3` /
  `sudo pacman -S iperf3`).
- GTK4 ≥ 4.10 y libadwaita ≥ 1.4 (paquetes `-devel`/`-dev` para compilar).
- Rust estable (`cargo`).

### Dependencias de compilación por distro

```sh
# Debian/Ubuntu
sudo apt install build-essential libgtk-4-dev libadwaita-1-dev iperf3
# Fedora
sudo dnf install gcc gtk4-devel libadwaita-devel iperf3
# Arch
sudo pacman -S base-devel gtk4 libadwaita iperf3
```

## Compilar y ejecutar

```sh
cargo run --release
```

## Instalar (opcional)

```sh
cargo build --release
sudo install -Dm755 target/release/iperf_rust /usr/local/bin/iperf_rust
sudo install -Dm644 data/io.github.iperf_rust.desktop \
    /usr/share/applications/io.github.iperf_rust.desktop

# Icono (todos los tamaños del tema hicolor)
for s in 16 32 48 64 128 256 512; do
    sudo install -Dm644 "data/icons/hicolor/${s}x${s}/apps/io.github.iperf_rust.png" \
        "/usr/share/icons/hicolor/${s}x${s}/apps/io.github.iperf_rust.png"
done
sudo gtk-update-icon-cache -f /usr/share/icons/hicolor
```

## Cómo funciona

iperf3 con `-J` (JSON) **no** emite en streaming: acumula todo y lo imprime al
final, lo que impediría animar la aguja en tiempo real. Por eso este frontend
parsea la salida legible línea a línea usando `--forceflush`, que vacía el buffer
en cada intervalo de 1 s. La comunicación hilo-lector → interfaz se hace con un
canal asíncrono (`async-channel`) consumido en el bucle principal de GLib.

## Licencia

[MIT](LICENSE) © 2026 teraflops
