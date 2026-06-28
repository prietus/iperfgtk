//! Wrapper around the system's `iperf3` binary.
//!
//! We don't embed iperf3: we launch it as a subprocess (must be in PATH)
//! and parse its readable output in *streaming*. We use `--forceflush` so
//! each 1-second interval is emitted as it occurs; `-J/--json` does NOT work
//! here because iperf3 accumulates all JSON and prints it at the end, which
//! would prevent real-time needle movement.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Events that the reader thread sends to the GUI.
#[derive(Debug, Clone)]
pub enum Event {
    /// Throughput reading from an interval (Mbit/s).
    Interval { mbps: f64 },
    /// Final test summary (send and receive Mbit/s).
    Summary { sender_mbps: f64, receiver_mbps: f64 },
    /// (Server) a client just connected.
    ClientConnected { peer: String },
    /// (Server) the client's test ended / disconnected.
    ClientDisconnected,
    /// The server is listening and free.
    Listening,
    /// Raw log line (reserved for a future details panel).
    #[allow(dead_code)]
    Log(String),
    /// Error message (stderr or launch failure).
    Error(String),
    /// Process terminated with the given code.
    Finished(i32),
}

/// IP version preference (general option, `-4`/`-6`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpVersion {
    Auto,
    V4,
    V6,
}

/// Client configuration for a test.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub host: String,
    pub port: u16,
    pub duration: u32,
    pub parallel: u32,
    /// `true` = reverse (descarga: el servidor envía hacia nosotros).
    pub reverse: bool,
    /// `true` = bidireccional (`--bidir`): mide en ambos sentidos a la vez.
    pub bidir: bool,
    /// `true` = UDP en lugar de TCP.
    pub udp: bool,
    /// Bitrate objetivo (`-b`), p.ej. "10M", "0" (sin límite). Vacío = por
    /// defecto de iperf3 (ilimitado en TCP; en UDP forzamos `-b 0`).
    pub bitrate: String,
    /// Omite los primeros N segundos de los resultados (`-O`). 0 = ninguno.
    pub omit: u32,
    /// Preferencia de versión de IP.
    pub ip_version: IpVersion,
    /// Dirección/interfaz local a la que enlazar (`-B`). Vacío = ninguna.
    pub bind: String,
    /// Tamaño de ventana TCP / buffer de socket (`-w`), p.ej. "256K". Vacío = ninguno.
    pub window: String,
    /// Longitud del buffer de lectura/escritura (`-l`), p.ej. "128K". Vacío = ninguno.
    pub length: String,
    /// MSS de TCP (`-M`). 0 = ninguno.
    pub mss: u32,
    /// Algoritmo de control de congestión TCP (`-C`). Vacío = ninguno.
    pub congestion: String,
    /// Desactiva el algoritmo de Nagle (`-N`).
    pub no_delay: bool,
    /// Envío con zero-copy (`-Z`).
    pub zerocopy: bool,
}

/// Server mode configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    /// Dirección/interfaz local a la que enlazar (`-B`). Vacío = ninguna.
    pub bind: String,
    /// Atiende un único cliente y termina (`-1`).
    pub one_off: bool,
}

/// Test session in progress. On `drop` or `stop()` the iperf3 subprocess is killed.
pub struct Session {
    child: Arc<Mutex<Option<Child>>>,
}

impl Session {
    /// Stops the session by killing the iperf3 process.
    pub fn stop(&self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
            }
            *guard = None;
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Launches iperf3 in client mode. Returns the session and an event receiver.
pub fn run_client(cfg: ClientConfig) -> (Session, async_channel::Receiver<Event>) {
    let mut args: Vec<String> = vec![
        "-c".into(),
        cfg.host.clone(),
        "-p".into(),
        cfg.port.to_string(),
        "-t".into(),
        cfg.duration.to_string(),
        "-i".into(),
        "1".into(),
        "--forceflush".into(),
    ];
    if cfg.parallel > 1 {
        args.push("-P".into());
        args.push(cfg.parallel.to_string());
    }
    // `--bidir` y `-R` son excluyentes; si el usuario pide bidireccional,
    // ignoramos reverse.
    if cfg.bidir {
        args.push("--bidir".into());
    } else if cfg.reverse {
        args.push("-R".into());
    }
    if cfg.udp {
        args.push("-u".into());
    }
    // Bitrate: valor explícito si se indica. En UDP iperf3 limita a 1 Mbit/s
    // por defecto, así que sin valor forzamos `-b 0` para medir el máximo.
    let bitrate = cfg.bitrate.trim();
    if !bitrate.is_empty() {
        args.push("-b".into());
        args.push(bitrate.to_string());
    } else if cfg.udp {
        args.push("-b".into());
        args.push("0".into());
    }
    if cfg.omit > 0 {
        args.push("-O".into());
        args.push(cfg.omit.to_string());
    }
    match cfg.ip_version {
        IpVersion::Auto => {}
        IpVersion::V4 => args.push("-4".into()),
        IpVersion::V6 => args.push("-6".into()),
    }
    let bind = cfg.bind.trim();
    if !bind.is_empty() {
        args.push("-B".into());
        args.push(bind.to_string());
    }
    let window = cfg.window.trim();
    if !window.is_empty() {
        args.push("-w".into());
        args.push(window.to_string());
    }
    let length = cfg.length.trim();
    if !length.is_empty() {
        args.push("-l".into());
        args.push(length.to_string());
    }
    if cfg.mss > 0 {
        args.push("-M".into());
        args.push(cfg.mss.to_string());
    }
    let congestion = cfg.congestion.trim();
    if !congestion.is_empty() {
        args.push("-C".into());
        args.push(congestion.to_string());
    }
    if cfg.no_delay {
        args.push("-N".into());
    }
    if cfg.zerocopy {
        args.push("-Z".into());
    }
    spawn(args, false)
}

/// Launches iperf3 in server mode. Returns the session and an event receiver.
pub fn run_server(cfg: ServerConfig) -> (Session, async_channel::Receiver<Event>) {
    let mut args: Vec<String> = vec![
        "-s".into(),
        "-p".into(),
        cfg.port.to_string(),
        "-i".into(),
        "1".into(),
        "--forceflush".into(),
    ];
    let bind = cfg.bind.trim();
    if !bind.is_empty() {
        args.push("-B".into());
        args.push(bind.to_string());
    }
    if cfg.one_off {
        args.push("-1".into());
    }
    spawn(args, true)
}

fn spawn(args: Vec<String>, is_server: bool) -> (Session, async_channel::Receiver<Event>) {
    let (tx, rx) = async_channel::unbounded::<Event>();
    let child_slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

    let child_for_thread = Arc::clone(&child_slot);
    thread::spawn(move || {
        let mut cmd = Command::new("iperf3");
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // En Linux pedimos al kernel que mate al hijo iperf3 si este proceso
        // (la app) muere, para no dejar servidores huérfanos ocupando el puerto.
        #[cfg(target_os = "linux")]
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let spawned = cmd.spawn();

        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let msg = if e.kind() == std::io::ErrorKind::NotFound {
                    "iperf3 not found in PATH. Install it (e.g. \
                     'sudo apt install iperf3' or 'sudo dnf install iperf3')."
                        .to_string()
                } else {
                    format!("Could not launch iperf3: {e}")
                };
                let _ = tx.send_blocking(Event::Error(msg));
                let _ = tx.send_blocking(Event::Finished(-1));
                return;
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        if let Ok(mut guard) = child_for_thread.lock() {
            *guard = Some(child);
        }

        // Thread for stderr (logs/errors from iperf3).
        let tx_err = tx.clone();
        let err_handle = stderr.map(|stderr| {
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    let t = line.trim();
                    // "iperf3: exiting" es la coletilla final tras un error real;
                    // la descartamos para no tapar el motivo verdadero.
                    if t.is_empty() || t == "iperf3: exiting" {
                        continue;
                    }
                    // Limpiamos el prefijo ruidoso para mostrar algo legible.
                    let msg = t
                        .strip_prefix("iperf3: error - ")
                        .or_else(|| t.strip_prefix("iperf3: "))
                        .unwrap_or(t)
                        .to_string();
                    let _ = tx_err.send_blocking(Event::Error(msg));
                }
            })
        });

        // Lectura principal: stdout línea a línea.
        let mut parser = OutputParser::new(is_server);
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx.send_blocking(Event::Log(line.clone()));
                for ev in parser.feed(&line) {
                    let _ = tx.send_blocking(ev);
                }
            }
        }

        if let Some(h) = err_handle {
            let _ = h.join();
        }

        // Esperamos al proceso para recoger el código de salida.
        let code = {
            let mut guard = child_for_thread.lock().unwrap();
            match guard.as_mut() {
                Some(c) => c.wait().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1),
                None => 0, // ya fue matado por stop()
            }
        };
        let _ = tx.send_blocking(Event::Finished(code));
    });

    (Session { child: child_slot }, rx)
}

/// Máquina de estados que convierte líneas de texto de iperf3 en `Event`s.
struct OutputParser {
    is_server: bool,
    /// Se activa al ver la primera línea `[SUM]` de la sesión. A partir de ahí
    /// preferimos esas líneas agregadas e ignoramos las individuales para no
    /// contar doble. Si nunca aparece (cliente de un solo flujo), usamos las
    /// líneas individuales. Se reinicia con cada nueva conexión en el servidor.
    seen_sum: bool,
    /// (Servidor) si ya hay un cliente en curso.
    client_active: bool,
    /// Mejor estimación del resumen (envío/recepción) para emitir al final.
    pending_sender: Option<f64>,
    pending_receiver: Option<f64>,
}

impl OutputParser {
    fn new(is_server: bool) -> Self {
        Self {
            is_server,
            seen_sum: false,
            client_active: false,
            pending_sender: None,
            pending_receiver: None,
        }
    }

    fn feed(&mut self, line: &str) -> Vec<Event> {
        let mut out = Vec::new();

        // --- Detección de conexión de cliente (servidor) ---
        if self.is_server {
            if let Some(idx) = line.find("Accepted connection from") {
                let rest = &line[idx + "Accepted connection from".len()..];
                let peer = rest
                    .trim()
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                self.client_active = true;
                self.seen_sum = false;
                self.pending_sender = None;
                self.pending_receiver = None;
                out.push(Event::ClientConnected { peer });
                return out;
            }
            if line.contains("Server listening") {
                if self.client_active {
                    self.client_active = false;
                    out.push(Event::ClientDisconnected);
                }
                out.push(Event::Listening);
                return out;
            }
        }

        let is_summary = line.contains("sender") || line.contains("receiver");
        let is_sum_line = line.contains("[SUM]");
        if is_sum_line {
            self.seen_sum = true;
        }

        // ¿Es una línea de medición que queremos parsear? Preferimos `[SUM]`
        // cuando la sesión la usa; si no, las líneas individuales `[  N]`.
        let parse_this = if is_sum_line {
            true
        } else {
            line.trim_start().starts_with('[') && !line.contains("[ ID]") && !self.seen_sum
        };

        if !parse_this {
            return out;
        }

        let Some(mbps) = parse_bandwidth_mbps(line) else {
            return out;
        };

        if is_summary {
            if line.contains("sender") {
                self.pending_sender = Some(mbps);
            }
            if line.contains("receiver") {
                self.pending_receiver = Some(mbps);
                // The "receiver" is the last line of the summary → emit it.
                out.push(Event::Summary {
                    sender_mbps: self.pending_sender.unwrap_or(mbps),
                    receiver_mbps: mbps,
                });
                if self.is_server && self.client_active {
                    self.client_active = false;
                    out.push(Event::ClientDisconnected);
                }
            }
        } else {
            // Intervalo en vivo → alimenta la aguja.
            out.push(Event::Interval { mbps });
        }

        out
    }
}

/// Extracts throughput in Mbit/s from an iperf3 line.
///
/// Finds the token `"<prefix>bits/sec"` and takes the number preceding it.
/// E.g.: `"...  985 Mbits/sec ..."` → `985.0`.
fn parse_bandwidth_mbps(line: &str) -> Option<f64> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    for (i, tok) in parts.iter().enumerate() {
        if let Some(prefix) = tok.strip_suffix("bits/sec") {
            if i == 0 {
                return None;
            }
            let val: f64 = parts[i - 1].parse().ok()?;
            let mbps = match prefix {
                "" => val / 1.0e6,
                "K" | "k" => val / 1.0e3,
                "M" => val,
                "G" => val * 1.0e3,
                "T" => val * 1.0e6,
                _ => return None,
            };
            return Some(mbps);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mbits() {
        let l = "[  5]   0.00-1.00   sec   118 MBytes   985 Mbits/sec    0   241 KBytes";
        assert_eq!(parse_bandwidth_mbps(l), Some(985.0));
    }

    #[test]
    fn parses_gbits() {
        let l = "[  5]   0.00-1.00   sec  1.10 GBytes  9.45 Gbits/sec";
        assert_eq!(parse_bandwidth_mbps(l), Some(9450.0));
    }

    #[test]
    fn detects_server_connection() {
        let mut p = OutputParser::new(true);
        let evs = p.feed("Accepted connection from 192.168.1.50, port 40522");
        assert!(matches!(&evs[0], Event::ClientConnected { peer } if peer == "192.168.1.50"));
    }

    #[test]
    fn servidor_un_solo_flujo_emite_intervalo() {
        // Reproduce el bug: cliente de un solo flujo (sin líneas [SUM]).
        let mut p = OutputParser::new(true);
        p.feed("Accepted connection from 127.0.0.1, port 50125");
        let evs = p.feed("[  5]   0.00-1.00   sec  14.3 GBytes   122 Gbits/sec");
        assert!(matches!(&evs[0], Event::Interval { mbps } if (*mbps - 122000.0).abs() < 1.0));
        // Y el resumen + desconexión al recibir la línea "receiver".
        let fin = p.feed("[  5]   0.00-2.00   sec  29.1 GBytes   125 Gbits/sec   receiver");
        assert!(fin.iter().any(|e| matches!(e, Event::Summary { .. })));
        assert!(fin.iter().any(|e| matches!(e, Event::ClientDisconnected)));
    }

    #[test]
    fn multi_flujo_prefiere_sum() {
        let mut p = OutputParser::new(true);
        p.feed("Accepted connection from 10.0.0.2, port 1");
        // Línea individual antes de ver [SUM]: se emite (blip inicial aceptable).
        let _ = p.feed("[  5]   0.00-1.00   sec  1.0 GBytes  8.0 Gbits/sec");
        // Tras ver [SUM], las individuales se ignoran.
        let sum = p.feed("[SUM]   0.00-1.00   sec  2.0 GBytes  16.0 Gbits/sec");
        assert!(matches!(&sum[0], Event::Interval { mbps } if (*mbps - 16000.0).abs() < 1.0));
        let ind = p.feed("[  5]   1.00-2.00   sec  1.0 GBytes  8.0 Gbits/sec");
        assert!(ind.is_empty());
    }
}
