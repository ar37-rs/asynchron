use asynchron::{Futurize, ITaskHandle, Progress, SyncState};
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

    let mut text_frame = Frame::default().with_label("Try hit Fetch button and let's see what happens...");
    text_frame.set_pos(100, 60);
    text_frame.set_size(200, 200);

    let mut button_fetch = Button::default().with_label("Fetch");
    button_fetch.set_pos(120, 210);
    button_fetch.set_size(80, 40);
    let mut button_cancel = Button::default()
        .with_label("Cancel")
        .right_of(&button_fetch, 10);
    button_cancel.set_size(80, 40);

    wind.show_with_args(&["-nokbd"]);

    let url = SyncState::new("https://www.rust-lang.org");
    let _url = url.clone();

    let reqwest = Futurize::task(
        0,
        move |_task: ITaskHandle<String>| -> Progress<String, String, String> {
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

                let url = *_url.get();
                let request = match client.get(url).build() {
                    Ok(request) => request,
                    Err(e) => return Progress::Error(e.to_string()),
                };

                let response = match client.execute(request).await {
                    Ok(response) => response,
                    Err(e) => return Progress::Error(e.to_string()),
                };

                for i in 0..5 {
                    _task.send(format!("checking status... {}", i));
                    std::thread::sleep(Duration::from_millis(100))
                }

                if response.status().is_success() {
                    let status = response.status().to_string();

                    for _ in 0..5 {
                        _task.send(status.clone());
                        std::thread::sleep(Duration::from_millis(100))
                    }

                    match response.text().await {
                        Ok(text) => {
                            // cancel at the end of the tunnel.
                            if _task.is_canceled() {
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
        },
    );

    let reqwest_fetch = reqwest.handle();
    let reqwest_cancel = reqwest.cancelation_handle();

    button_fetch.set_callback(move |_| reqwest_fetch.try_do());

    button_cancel.set_callback(move |_| {
        if reqwest_cancel.is_canceled() {
            println!("canceled")
        } else {
            reqwest_cancel.cancel()
        }
    });

    let mut label = String::new();

    let mut timer = 0;

    while app.wait() {
        std::thread::sleep(Duration::from_millis(10));
        reqwest.try_resolve(|progress, resolved| {
            match progress {
                Progress::Current(task_receiver) => {
                    button_fetch.set_label("Fetching...");
                    if let Some(value) = task_receiver {
                        text_frame.set_label(&value)
                    }
                }
                Progress::Canceled => label = "Request canceled.".to_owned(),
                Progress::Completed(value) => label = value,
                Progress::Error(e) => {
                    eprintln!("error {}", &e);
                    label = e
                }
            }
            
            if resolved {
                text_frame.set_label(&label);
                button_fetch.set_label("Fetch")
            }
        });

        if timer % 2 == 0 {
            url.set("https://hyper.rs")
        } else {
            url.set("https://www.rust-lang.org")
        }

        timer += 1;

        timer_frame.set_label(timer.to_string().as_ref());
        wind.redraw()
    }
    Ok(())
}
