use asynchron::{
    Futurize,
    Futurized::{self, OnComplete, OnError, OnProgress},
    State,
};
use fltk::{app::*, button::*, frame::*, prelude::WidgetExt, window::*};
use reqwest::Client;
use std::{io::Result, time::Duration};
use tokio::runtime::Builder;
fn main() -> Result<()> {
    let mut app = App::default();
    app.set_scheme(Scheme::Gtk);
    let mut wind = Window::default().with_size(400, 300);
    wind.set_label("Hello from rust");
    let mut timer_frame = Frame::default().with_label("");
    timer_frame.set_pos(0, 0);
    timer_frame.set_size(400, 100);

    let mut loading_frame = Frame::default().with_label("");
    loading_frame.set_pos(80, 60);
    loading_frame.set_size(200, 200);

    let mut but: Button = Button::default().with_label("Fetch");
    but.set_pos(160, 210);
    but.set_size(80, 40);

    wind.show_with_args(&["-nokbd"]);

    let url = State::new("https://www.rust-lang.org");
    let set_url = url.clone();

    let (tx, rx) = std::sync::mpsc::sync_channel(0);
    let reqwest = Futurize::task(move || -> Futurized<String, String> {
        let rt = match Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => return OnError(e.to_string()),
        };

        rt.block_on(async {
            // timeout connection for 5 seconds, so there's a noise if something goes wrong.
            let time_out = Duration::from_secs(5);
            let client = match Client::builder().timeout(time_out).build() {
                Ok(respose) => respose,
                Err(e) => return OnError(e.to_string()),
            };

            let url = url.async_get().await;
            let request = match client.get(url).build() {
                Ok(request) => request,
                Err(e) => return OnError(e.to_string()),
            };

            let respose = match client.execute(request).await {
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

    let reqwest_clone = reqwest.clone();
    let mut loading_frame_clone = loading_frame.clone();

    but.set_callback(move |_| {
        let reqwest = reqwest_clone.to_owned();
        if !reqwest.awake() {
            loading_frame_clone.set_label("loading...");
        }
        reqwest.try_wake();
    });

    let mut label = String::new();

    let mut timer = 0;

    while app.wait() {
        std::thread::sleep(Duration::from_millis(10));

        if reqwest.awake() {
            match reqwest.try_get() {
                OnError(e) => {
                    eprintln!("error {}", &e);
                    label = e.clone();
                }
                OnProgress => {
                    if let Ok(rx) = rx.try_recv() {
                        loading_frame.set_label(&rx);
                    }
                }
                OnComplete(value) => {
                    label = value;
                }
            }
            loading_frame.show();
        } else {
            loading_frame.set_label(&label);
        }

        if timer % 2 == 0 {
            set_url.set("https://hyper.rs");
        } else {
            set_url.set("https://www.rust-lang.org");
        }

        timer += 1;

        timer_frame.set_label(timer.to_string().as_ref());
        wind.redraw();
    }
    Ok(())
}
