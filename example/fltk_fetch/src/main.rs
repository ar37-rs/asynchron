use asynchron::{
    Futurize,
    Progress,
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
    loading_frame.set_pos(100, 60);
    loading_frame.set_size(200, 200);

    let mut button_fetch: Button = Button::default().with_label("Fetch");
    button_fetch.set_pos(120, 210);
    button_fetch.set_size(80, 40);
    let mut button_cancel: Button = Button::default().with_label("Cancel").right_of(&button_fetch, 10);
    button_cancel.set_size(80, 40);

    wind.show_with_args(&["-nokbd"]);

    let url = State::new("https://www.rust-lang.org");
    let _url = url.clone();

    let (tx, rx) = std::sync::mpsc::sync_channel(0);
    let reqwest = Futurize::task(0, move |canceled| -> Progress<String, String> {
        // Builder::new_current_thread().enable_all().build() also working fine on this context.
        let rt = match Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => return Progress::Error(e.to_string()),
        };

        rt.block_on(async {
            // timeout connection for 10 seconds, so there's a noise if something goes wrong.
            let time_out = Duration::from_secs(10);
            let client = match Client::builder().timeout(time_out).build() {
                Ok(client) => client,
                Err(e) => return Progress::Error(e.to_string()),
            };

            let url = _url.async_get().await;
            let request = match client.get(url).build() {
                Ok(request) => request,
                Err(e) => return Progress::Error(e.to_string()),
            };

            let response = match client.execute(request).await {
                Ok(response) => response,
                Err(e) => return Progress::Error(e.to_string()),
            };

            for i in 0..5 {
                let _ = tx.send(format!("checking status... {}", i));
                std::thread::sleep(Duration::from_millis(100));
            }

            if response.status().is_success() {
                let status = response.status().to_string();

                for _ in 0..5 {
                    let _ = tx.send(status.clone());
                    std::thread::sleep(Duration::from_millis(100));
                }

                match response.text().await {
                    Ok(text) => {
                        // cancel at the end of the tunnel.
                        if canceled.load(std::sync::atomic::Ordering::Relaxed) {
                            return Progress::Canceled;
                        } else {
                            return Progress::Completed(text[0..100].to_string());
                        }   
                    }
                    Err(e) => return Progress::Error(e.to_string()),
                }
            } else {
                return Progress::Error(response.status().to_string());
            }
        })
    });

    let reqwest_fetch = reqwest.clone();
    let reqwest_cancel = reqwest.clone();

    button_fetch.set_callback(move |_| {
        let reqwest = reqwest_fetch.to_owned();
        reqwest.try_do();
    });

    button_cancel.set_callback(move |_| {
        let reqwest = reqwest_cancel.to_owned();
        reqwest.cancel();
    });

    let mut label = String::new();

    let mut timer = 0;

    while app.wait() {
        std::thread::sleep(Duration::from_millis(10));

        if reqwest.is_on_progress() {
            match reqwest.try_get() {
                Progress::Current => {
                    button_fetch.set_label("Fetching...");
                    if let Ok(rx) = rx.try_recv() {
                        loading_frame.set_label(&rx);
                    }
                }
                Progress::Canceled => label = "Request cancled.".to_owned(),
                Progress::Completed(value) => label = value,
                Progress::Error(e) => {
                    eprintln!("error {}", &e);
                    label = e;
                }
            }
            loading_frame.show();
            if reqwest.is_done() {
                button_fetch.set_label("Fetch")
            }
        } else {
            loading_frame.set_label(&label);
        }

        if timer % 2 == 0 {
            url.set("https://hyper.rs");
        } else {
            url.set("https://www.rust-lang.org");
        }

        timer += 1;

        timer_frame.set_label(timer.to_string().as_ref());
        wind.redraw();
    }
    Ok(())
}
