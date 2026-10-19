//! Cold plate temperature over the last few minutes.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use adw::prelude::*;

/// Status pushes come once a second.
const SAMPLES: usize = 300;

#[derive(Clone)]
pub struct Chart {
    pub area: gtk::DrawingArea,
    samples: Rc<RefCell<VecDeque<f64>>>,
}

impl Chart {
    pub fn new() -> Chart {
        let area = gtk::DrawingArea::builder().content_height(96).hexpand(true).build();
        let samples: Rc<RefCell<VecDeque<f64>>> = Rc::default();
        let data = samples.clone();
        area.set_draw_func(move |area, cr, w, h| {
            let data = data.borrow();
            if data.len() < 2 {
                return;
            }
            let (w, h) = (w as f64, h as f64);
            let lo = data.iter().copied().fold(f64::INFINITY, f64::min).floor() - 1.0;
            let hi = data.iter().copied().fold(f64::NEG_INFINITY, f64::max).ceil() + 1.0;
            let x = |i: usize| w * i as f64 / (SAMPLES - 1) as f64;
            let y = |t: f64| h - (t - lo) / (hi - lo) * h;
            let offset = SAMPLES - data.len();
            let fg = area.color();
            let accent = adw::StyleManager::default().accent_color_rgba();

            // 0 °C line, where condensation starts to matter.
            if lo < 0.0 && hi > 0.0 {
                cr.set_source_rgba(fg.red().into(), fg.green().into(), fg.blue().into(), 0.25);
                cr.set_line_width(1.0);
                cr.set_dash(&[4.0, 4.0], 0.0);
                cr.move_to(0.0, y(0.0));
                cr.line_to(w, y(0.0));
                let _ = cr.stroke();
                cr.set_dash(&[], 0.0);
            }

            cr.set_source_rgba(accent.red().into(), accent.green().into(), accent.blue().into(), 1.0);
            cr.set_line_width(2.0);
            for (i, &t) in data.iter().enumerate() {
                if i == 0 {
                    cr.move_to(x(offset + i), y(t));
                } else {
                    cr.line_to(x(offset + i), y(t));
                }
            }
            let _ = cr.stroke_preserve();
            cr.line_to(x(SAMPLES - 1), h);
            cr.line_to(x(offset), h);
            cr.close_path();
            cr.set_source_rgba(accent.red().into(), accent.green().into(), accent.blue().into(), 0.15);
            let _ = cr.fill();
        });
        Chart { area, samples }
    }

    pub fn push(&self, temp: f64) {
        let mut s = self.samples.borrow_mut();
        if s.len() == SAMPLES {
            s.pop_front();
        }
        s.push_back(temp);
        drop(s);
        self.area.queue_draw();
    }

    pub fn clear(&self) {
        self.samples.borrow_mut().clear();
        self.area.queue_draw();
    }
}
