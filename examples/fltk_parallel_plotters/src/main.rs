// main source code taken from: https://github.com/fltk-rs/demos/tree/master/plotters

use asynchron::{Futurize, Progress};
use fltk::{prelude::*, *};
use plotters::prelude::*;
use plotters::style::Color;
use plotters_bitmap::bitmap_pixel::RGBPixel;
use plotters_bitmap::BitMapBackend;
use std::error::Error;
use std::time::SystemTime;

const SAMPLE_RATE: f64 = 10_000.0;
const FREAME_RATE: f64 = 30.0;
const WIN_W: i32 = 420;
const WIN_H: i32 = 480;

fn main() -> Result<(), Box<dyn Error>> {
    const W: usize = 420;
    const H: usize = 240;
    let fx: f64 = 1.0;
    let fy: f64 = 1.1;
    let xphase: f64 = 0.0;
    let yphase: f64 = 0.1;

    let plot1 = Futurize::task(0, move |_task| -> Progress<Vec<u8>, ()> {
        let mut buf = vec![0u8; W * H * 3];
        let root =
            BitMapBackend::<RGBPixel>::with_buffer_and_format(&mut buf, (W as u32, H as u32))
                .unwrap()
                .into_drawing_area();
        root.fill(&BLACK).unwrap();

        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .set_all_label_area_size(30)
            .build_cartesian_2d(-1.2..1.2, -1.2..1.2)
            .unwrap();

        chart
            .configure_mesh()
            .label_style(("sans-serif", 15).into_font().color(&GREEN))
            .axis_style(&GREEN)
            .draw()
            .unwrap();

        let cs = chart.into_chart_state();
        drop(root);

        let mut data = Vec::new();
        let start_ts = SystemTime::now();
        let mut last_flushed = 0.0;

        loop {
            let epoch = SystemTime::now()
                .duration_since(start_ts)
                .unwrap()
                .as_secs_f64();

            if let Some((ts, _, _)) = data.pop() {
                if epoch - ts < 1.0 / SAMPLE_RATE {
                    std::thread::sleep(std::time::Duration::from_secs_f64(epoch - ts));
                    continue;
                }
                let mut ts = ts;
                while ts < epoch {
                    ts += 1.0 / SAMPLE_RATE;
                    let phase_x: f64 = 2.0 * ts * std::f64::consts::PI * fx + xphase;
                    let phase_y: f64 = 2.0 * ts * std::f64::consts::PI * fy + yphase;
                    data.push((ts, phase_x.sin(), phase_y.sin()));
                }
            }

            let phase_x = 2.0 * epoch * std::f64::consts::PI * fx + xphase;
            let phase_y = 2.0 * epoch * std::f64::consts::PI * fy + yphase;
            data.push((epoch, phase_x.sin(), phase_y.sin()));

            if epoch - last_flushed > 1.0 / FREAME_RATE {
                let root = BitMapBackend::<RGBPixel>::with_buffer_and_format(
                    &mut buf,
                    (W as u32, H as u32),
                )
                .unwrap()
                .into_drawing_area();
                let mut chart = cs.clone().restore(&root);
                chart.plotting_area().fill(&BLACK).unwrap();

                chart
                    .configure_mesh()
                    .bold_line_style(&GREEN.mix(0.2))
                    .light_line_style(&TRANSPARENT)
                    .draw()
                    .unwrap();

                chart
                    .draw_series(data.iter().zip(data.iter().skip(1)).map(
                        |(&(e, x0, y0), &(_, x1, y1))| {
                            PathElement::new(
                                vec![(x0, y0), (x1, y1)],
                                &GREEN.mix(((e - epoch) * 20.0).exp()),
                            )
                        },
                    ))
                    .unwrap();

                std::mem::forget(root);
                std::mem::forget(chart);
                last_flushed = epoch;
            }

            _task.send(buf.to_owned());

            while let Some((e, _, _)) = Some(data[0]) {
                if ((e - epoch) * 20.0).exp() > 0.1 {
                    break;
                }
                std::mem::forget(data.remove(0));
            }
        }
    });

    let plot2 = Futurize::task(1, move |_task| -> Progress<Vec<u8>, ()> {
        let mut buf = vec![0u8; W * H * 3];
        let root =
            BitMapBackend::<RGBPixel>::with_buffer_and_format(&mut buf, (W as u32, H as u32))
                .unwrap()
                .into_drawing_area();
        root.fill(&BLACK).unwrap();

        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .set_all_label_area_size(30)
            .build_cartesian_2d(-1.2..1.2, -1.2..1.2)
            .unwrap();

        chart
            .configure_mesh()
            .label_style(("sans-serif", 15).into_font().color(&RED))
            .axis_style(&RED)
            .draw()
            .unwrap();

        let cs = chart.into_chart_state();
        drop(root);

        let mut data = Vec::new();
        let start_ts = SystemTime::now();
        let mut last_flushed = 0.0;

        loop {
            let epoch = SystemTime::now()
                .duration_since(start_ts)
                .unwrap()
                .as_secs_f64();

            if let Some((ts, _, _)) = data.pop() {
                if epoch - ts < 1.0 / SAMPLE_RATE {
                    std::thread::sleep(std::time::Duration::from_secs_f64(epoch - ts));
                    continue;
                }
                let mut ts = ts;
                while ts < epoch {
                    ts += 1.0 / SAMPLE_RATE;
                    let phase_x: f64 = 2.0 * ts * std::f64::consts::PI * fx + xphase;
                    let phase_y: f64 = 2.0 * ts * std::f64::consts::PI * fy + yphase;
                    data.push((ts, phase_x.sin(), phase_y.sin()));
                }
            }

            let phase_x = 2.0 * epoch * std::f64::consts::PI * fx + xphase;
            let phase_y = 2.0 * epoch * std::f64::consts::PI * fy + yphase;
            data.push((epoch, phase_x.sin(), phase_y.sin()));

            if epoch - last_flushed > 1.0 / FREAME_RATE {
                let root = BitMapBackend::<RGBPixel>::with_buffer_and_format(
                    &mut buf,
                    (W as u32, H as u32),
                )
                .unwrap()
                .into_drawing_area();
                let mut chart = cs.clone().restore(&root);
                chart.plotting_area().fill(&BLACK).unwrap();

                chart
                    .configure_mesh()
                    .bold_line_style(&RED.mix(0.2))
                    .light_line_style(&TRANSPARENT)
                    .draw()
                    .unwrap();

                chart
                    .draw_series(data.iter().zip(data.iter().skip(1)).map(
                        |(&(e, x0, y0), &(_, x1, y1))| {
                            PathElement::new(
                                vec![(x0, y0), (x1, y1)],
                                &RED.mix(((e - epoch) * 20.0).exp()),
                            )
                        },
                    ))
                    .unwrap();

                std::mem::forget(root);
                std::mem::forget(chart);
                last_flushed = epoch;
            }

            _task.send(buf.to_owned());

            while let Some((e, _, _)) = Some(data[0]) {
                if ((e - epoch) * 20.0).exp() > 0.1 {
                    break;
                }
                std::mem::forget(data.remove(0));
            }

            // delay for 180 ms.
            std::thread::sleep(std::time::Duration::from_millis(180));
        }
    });

    let app = app::App::default();
    let mut win = window::Window::default().with_size(WIN_W, WIN_H);
    let mut frame = frame::Frame::default().with_size(420, 240);
    let mut frame2 = frame::Frame::default()
        .with_size(420, 240)
        .below_of(&frame, 0);
    win.end();
    win.show();

    plot1.try_do();
    plot2.try_do();

    let mut count = 0;
    let mut assume_failure = false;

    let mut retry_plot2 = false;
    let mut retry_plot1 = false;

    while app.wait() {
        plot1.try_resolve(|prog, _| match prog {
            Progress::Current(recv) => {
                if let Some(_buf) = recv {
                    draw::draw_rgb(&mut frame, &_buf).unwrap();
                    if !assume_failure {
                        count += 1
                    }
                }
            }
            Progress::Error(e) => {
                println!("{}\n", e);
                // unwrapping all over the place potentially panicked, retry?
                retry_plot1 = true
            }
            _ => (),
        });

        if retry_plot1 {
            plot1.try_do();
            retry_plot1 = false
        }

        if count >= 60 {
            count = 60;
            assume_failure = true;
            plot2.try_resolve(|prog, _| match prog {
                Progress::Current(recv) => {
                    if let Some(_buf) = recv {
                        draw::draw_rgb(&mut frame2, &_buf).unwrap();
                    }
                }
                Progress::Error(e) => {
                    println!("{}\n", e);
                    // unwrapping all over the place potentially panicked, retry?
                    retry_plot2 = true
                }
                _ => (),
            });

            if retry_plot2 {
                plot2.try_do();
                retry_plot2 = false
            }
        }
        win.redraw();
        app::sleep(0.017);
        app::awake();
    }
    Ok(())
}
