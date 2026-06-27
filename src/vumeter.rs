//! Analog VU-meter widget drawn with Cairo.
//!
//! Anti-jump philosophy: iperf3 provides a new reading ~every second, but the
//! needle redraws at ~60 fps. We store the *target* value on one side and the
//! *displayed* value on the other; in each frame the displayed value chases the
//! target with exponential smoothing (critically damped spring), so movement is
//! continuous and smooth even when data arrives in jumps. The scale range also
//! auto-scales smoothly.

use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use gtk::cairo;
use gtk::prelude::*;

const START_DEG: f64 = 150.0; // arc start (bottom-left)
const SWEEP_DEG: f64 = 240.0; // total needle sweep
const NEEDLE_TAU: f64 = 0.10; // needle time constant (s); smaller = faster
const SCALE_GROW_TAU: f64 = 0.25; // range grows fast...
const SCALE_SHRINK_TAU: f64 = 1.20; // ...and shrinks slowly (avoids scale flicker)
const MIN_SCALE: f64 = 10.0; // minimum scale baseline (Mbit/s)

struct State {
    target: f64,        // Mbit/s we want to reach
    displayed: f64,     // Mbit/s the needle paints now
    peak: f64,          // retained peak
    peak_age: f64,      // seconds since last peak (to decay)
    scale_max: f64,     // current scale baseline
    last_frame_us: i64, // timestamp of previous frame
    active: bool,       // si hay una medición en curso
}

#[derive(Clone)]
pub struct VuMeter {
    area: gtk::DrawingArea,
    state: Rc<RefCell<State>>,
}

impl VuMeter {
    pub fn new() -> Self {
        let area = gtk::DrawingArea::new();
        area.set_content_width(360);
        area.set_content_height(240);
        area.set_hexpand(true);
        area.set_vexpand(true);

        let state = Rc::new(RefCell::new(State {
            target: 0.0,
            displayed: 0.0,
            peak: 0.0,
            peak_age: 0.0,
            scale_max: MIN_SCALE,
            last_frame_us: 0,
            active: false,
        }));

        // --- Dibujo ---
        {
            let state = state.clone();
            area.set_draw_func(move |area, cr, w, h| {
                let st = state.borrow();
                draw_gauge(area, cr, w as f64, h as f64, &st);
            });
        }

        // --- Animación a 60 fps sincronizada al reloj de frames ---
        {
            let state = state.clone();
            area.add_tick_callback(move |area, clock| {
                let now = clock.frame_time();
                let mut st = state.borrow_mut();

                let dt = if st.last_frame_us == 0 {
                    0.0
                } else {
                    ((now - st.last_frame_us) as f64 / 1_000_000.0).clamp(0.0, 0.1)
                };
                st.last_frame_us = now;

                // Suavizado exponencial de la aguja hacia el objetivo.
                let a = 1.0 - (-dt / NEEDLE_TAU).exp();
                st.displayed += (st.target - st.displayed) * a;
                if (st.target - st.displayed).abs() < 0.01 {
                    st.displayed = st.target;
                }

                // Auto-escalado del fondo de la esfera.
                let want = nice_ceil(st.target.max(st.peak) * 1.15).max(MIN_SCALE);
                let tau = if want > st.scale_max {
                    SCALE_GROW_TAU
                } else {
                    SCALE_SHRINK_TAU
                };
                let b = 1.0 - (-dt / tau).exp();
                st.scale_max += (want - st.scale_max) * b;

                // Decaimiento lento del pico retenido.
                st.peak_age += dt;
                if st.peak_age > 2.0 {
                    st.peak *= 1.0 - (-dt / 1.5_f64).exp();
                    if st.peak < st.target {
                        st.peak = st.target;
                    }
                }

                area.queue_draw();
                glib::ControlFlow::Continue
            });
        }

        Self { area, state }
    }

    /// The widget to insert into the interface.
    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    /// Sets a new target throughput (Mbit/s).
    pub fn set_target(&self, mbps: f64) {
        let mut st = self.state.borrow_mut();
        st.target = mbps.max(0.0);
        st.active = true;
        if mbps > st.peak {
            st.peak = mbps;
            st.peak_age = 0.0;
        }
    }

    /// Returns the needle to zero and resets the peak (no jump: animated).
    pub fn reset(&self) {
        let mut st = self.state.borrow_mut();
        st.target = 0.0;
        st.peak = 0.0;
        st.peak_age = 0.0;
        st.active = false;
    }
}

impl Default for VuMeter {
    fn default() -> Self {
        Self::new()
    }
}

/// Color verde→ámbar→rojo según la fracción `t` ∈ [0,1] de la escala.
fn color_for(t: f64) -> (f64, f64, f64) {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        let k = t / 0.5;
        (0.20 + 0.75 * k, 0.78, 0.40 - 0.10 * k) // verde → ámbar
    } else {
        let k = (t - 0.5) / 0.5;
        (0.95, 0.78 - 0.48 * k, 0.30 - 0.05 * k) // ámbar → rojo
    }
}

fn draw_gauge(_area: &gtk::DrawingArea, cr: &cairo::Context, w: f64, h: f64, st: &State) {
    let dark = adw::StyleManager::default().is_dark();
    let (fg, track) = if dark {
        ((0.92, 0.93, 0.95), (1.0, 1.0, 1.0, 0.10))
    } else {
        ((0.13, 0.15, 0.18), (0.0, 0.0, 0.0, 0.08))
    };

    // Geometría: el arco (barrido de 240°) más la lectura digital ocupan, en
    // vertical, ~1.55·radio; en horizontal, 2·radio. Elegimos el radio que
    // quepa en ambas dimensiones y centramos el conjunto verticalmente para que
    // nada se recorte por abajo (aguja, eje y lectura incluidos).
    let m = 16.0; // margen
    let radius = (((w - 2.0 * m) / 2.0).min((h - 2.0 * m) / 1.55)).max(40.0);
    let cx = w / 2.0;
    let used_h = radius * 1.55;
    let cy = ((h - used_h) / 2.0).max(m) + radius;
    let arc_w = (radius * 0.13).clamp(8.0, 26.0);

    let start = START_DEG.to_radians();
    let end = (START_DEG + SWEEP_DEG).to_radians();

    // 1) Pista de fondo.
    cr.set_line_width(arc_w);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_source_rgba(track.0, track.1, track.2, track.3);
    cr.arc(cx, cy, radius, start, end);
    let _ = cr.stroke();

    // 2) Arco de valor (relleno coloreado hasta la aguja).
    let frac = (st.displayed / st.scale_max).clamp(0.0, 1.0);
    if frac > 0.001 {
        // Pintamos en segmentos cortos para conseguir el degradado de color.
        let segments = 64;
        let total = end - start;
        for i in 0..segments {
            let f0 = i as f64 / segments as f64;
            let f1 = (i + 1) as f64 / segments as f64;
            if f0 > frac {
                break;
            }
            let f1 = f1.min(frac);
            let (r, g, b) = color_for((f0 + f1) * 0.5);
            cr.set_source_rgb(r, g, b);
            cr.arc(cx, cy, radius, start + total * f0, start + total * f1);
            let _ = cr.stroke();
        }
    }

    // 3) Marcas de escala (ticks) y etiquetas.
    let major = 10;
    cr.set_line_width(2.0);
    for i in 0..=major {
        let f = i as f64 / major as f64;
        let ang = start + (end - start) * f;
        let (ca, sa) = (ang.cos(), ang.sin());
        let r_out = radius - arc_w * 0.5 - 4.0;
        let r_in = r_out - if i % 5 == 0 { 14.0 } else { 8.0 };
        cr.set_source_rgba(fg.0, fg.1, fg.2, 0.55);
        cr.move_to(cx + r_in * ca, cy + r_in * sa);
        cr.line_to(cx + r_out * ca, cy + r_out * sa);
        let _ = cr.stroke();

        if i % 5 == 0 {
            let val = st.scale_max * f;
            let (txt, _) = format_speed(val);
            cr.set_font_size((radius * 0.085).clamp(9.0, 15.0));
            let r_txt = r_in - 14.0;
            let ext = cr.text_extents(&txt).unwrap();
            cr.move_to(
                cx + r_txt * ca - ext.width() / 2.0,
                cy + r_txt * sa + ext.height() / 2.0,
            );
            cr.set_source_rgba(fg.0, fg.1, fg.2, 0.70);
            let _ = cr.show_text(&txt);
        }
    }

    // 4) Marca de pico retenido (línea fina roja).
    if st.peak > 0.0 {
        let pf = (st.peak / st.scale_max).clamp(0.0, 1.0);
        let ang = start + (end - start) * pf;
        let (ca, sa) = (ang.cos(), ang.sin());
        cr.set_line_width(3.0);
        cr.set_source_rgb(0.95, 0.35, 0.35);
        cr.move_to(cx + (radius - arc_w) * ca, cy + (radius - arc_w) * sa);
        cr.line_to(cx + (radius + arc_w * 0.55) * ca, cy + (radius + arc_w * 0.55) * sa);
        let _ = cr.stroke();
    }

    // 5) Aguja.
    let ang = start + (end - start) * frac;
    let (ca, sa) = (ang.cos(), ang.sin());
    let needle_len = radius - arc_w * 0.5;
    let tail = radius * 0.16;
    let (nr, ng, nb) = color_for(frac);
    cr.set_line_width((radius * 0.035).clamp(3.0, 7.0));
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_source_rgb(nr, ng, nb);
    cr.move_to(cx - tail * ca, cy - tail * sa);
    cr.line_to(cx + needle_len * ca, cy + needle_len * sa);
    let _ = cr.stroke();

    // Eje central.
    cr.set_source_rgb(fg.0, fg.1, fg.2);
    cr.arc(cx, cy, (radius * 0.06).clamp(5.0, 11.0), 0.0, 2.0 * PI);
    let _ = cr.fill();

    // 6) Lectura digital central.
    let (val_txt, unit_txt) = format_speed(st.displayed);
    cr.set_source_rgb(fg.0, fg.1, fg.2);
    let big = (radius * 0.30).clamp(18.0, 44.0);
    cr.set_font_size(big);
    cr.select_font_face("Cantarell", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    let ext = cr.text_extents(&val_txt).unwrap();
    let base_y = cy + radius * 0.42;
    cr.move_to(cx - ext.width() / 2.0, base_y);
    let _ = cr.show_text(&val_txt);

    cr.set_font_size((radius * 0.12).clamp(10.0, 18.0));
    cr.select_font_face("Cantarell", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    let uext = cr.text_extents(&unit_txt).unwrap();
    cr.set_source_rgba(fg.0, fg.1, fg.2, 0.65);
    cr.move_to(cx - uext.width() / 2.0, base_y + big * 0.7);
    let _ = cr.show_text(&unit_txt);
}

/// Formats Mbit/s to (value, unit) readable format.
pub fn format_speed(mbps: f64) -> (String, String) {
    if mbps >= 1000.0 {
        (format!("{:.2}", mbps / 1000.0), "Gbit/s".into())
    } else if mbps >= 1.0 {
        (format!("{:.1}", mbps), "Mbit/s".into())
    } else {
        (format!("{:.0}", mbps * 1000.0), "Kbit/s".into())
    }
}

/// Rounds up to a "nice" value (1, 2, 5 × 10ⁿ).
fn nice_ceil(v: f64) -> f64 {
    if v <= 0.0 {
        return MIN_SCALE;
    }
    let k = v.log10().floor();
    let base = 10f64.powf(k);
    let frac = v / base;
    let mult = if frac <= 1.0 {
        1.0
    } else if frac <= 2.0 {
        2.0
    } else if frac <= 5.0 {
        5.0
    } else {
        10.0
    };
    mult * base
}
