use asynchron::{
    Futurize,
    Futurized::{self, OnComplete, OnError, OnProgress},
};
use fltk::{app::*, button::*, frame::*, window::*};
use std::{
    io::Result,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::runtime::Builder;

fn main() -> Result<()> {
    let mut app = App::default();
    app.set_scheme(Scheme::Gtk);
    let mut wind = Window::new(100, 100, 400, 300, "Hello from rust");
    let mut timer_frame = Frame::new(0, 0, 400, 100, "");
    let loading_frame = Arc::new(Mutex::new(Frame::new(80, 60, 200, 200, "")));

    let mut but: Button = Button::new(160, 210, 80, 40, "Fetch");
    wind.end();
    wind.show_with_args(&["-nokbd"]);

    let (tx, rx) = std::sync::mpsc::sync_channel(0);
    let request = Futurize::task(move || -> Futurized<String, String> {
        let rt = match Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => return OnError(e.to_string()),
        };
        rt.block_on(async {
            let respose = match reqwest::get("https://www.rust-lang.org/").await {
                Ok(respose) => respose,
                Err(e) => return OnError(e.to_string()),
            };

            for i in 0..5 {
                let _ = tx.send(format!("checking status... {}", i));
                std::thread::sleep(Duration::from_millis(100));
            }

            if respose.status().is_success() {
                let status = respose.status().to_string();

                for _ in 0..5 {
                    let _ = tx.send(status.clone());
                    std::thread::sleep(Duration::from_millis(100));
                }

                match respose.text().await {
                    Ok(text) => {
                        return OnComplete(text[0..100].to_string());
                    }
                    Err(e) => return OnError(e.to_string()),
                }
            } else {
                return OnError(respose.status().to_string());
            }
        })
    });

    let request_clone = request.clone();

    let loading_frame_clone = loading_frame.clone();

    but.set_callback(move || {
        let request = request_clone.to_owned();
        if !request.awake() {
            let mut loading_frame = loading_frame_clone.lock().unwrap();
            loading_frame.set_label("loading...");
        }
        request.try_wake();
    });

    let mut label = String::new();

    let mut timer = 0;

    while app.wait() {
        std::thread::sleep(Duration::from_millis(10));

        let mut loading_frame = loading_frame.lock().unwrap();
        if request.awake() {
            loading_frame.show();
            match request.try_get() {
                OnError(e) => {
                    label = e;
                }
                OnProgress => {
                    if let Ok(msg) = rx.try_recv() {
                        loading_frame.set_label(&msg);
                    }
                }
                OnComplete(value) => label = value,
            }
        } else {
            loading_frame.set_label(&label);
        }
        timer += 1;
        timer_frame.set_label(timer.to_string().as_ref());
        wind.redraw();
    }
    Ok(())
}
