use asynchron::{Futurize, ITaskHandle, Progress, SyncState};
use fltk::{app::*, button::*, frame::*, prelude::WidgetExt, window::*, *};
use std::{io::Result, time::Duration};
use ureq::{Agent, AgentBuilder};

fn main() -> Result<()> {
    let mut app = App::default();
    app.set_scheme(Scheme::Gtk);
    let mut wind = Window::default().with_size(400, 300);
    wind.set_label("Hello from rust");
    let mut timer_frame = Frame::default().with_label("");
    timer_frame.set_pos(0, 0);
    timer_frame.set_size(400, 100);

    let mut text_frame =
        Frame::default().with_label("Try hit Fetch button and let's see what happens...");
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

    let request = Futurize::task(0, move |_task: ITaskHandle<u16>| -> Progress<u16, String> {
        let url = match _url.load() {
            Some(url) => url,
            _ => return Progress::Error("Unable to load URL, probably empty.".into()),
        };

        let agent: Agent = AgentBuilder::new()
            .timeout_read(Duration::from_secs(3))
            .timeout_write(Duration::from_secs(3))
            .build();

        let res = if let Ok(res) = agent.get(url).call() {
            res
        } else {
            return Progress::Error(
                format!("Network problem, unable to request url: {}", &url).into(),
            );
        };

        // Check if progress is canceled
        if _task.should_cancel() {
            return Progress::Canceled;
        }

        for _ in 0..5 {
            // Check if the task is canceled.
            if _task.should_cancel() {
                return Progress::Canceled;
            }
            _task.send(res.status());
            std::thread::sleep(Duration::from_millis(100))
        }
		
        if res.status() == 200 {
            // And check here also.
            if _task.should_cancel() {
                Progress::Canceled
            } else {
                match res.into_string() {
                    Ok(text) => {
                        // and check here also.
                        if _task.should_cancel() {
                            return Progress::Canceled;
                        }
                        Progress::Completed(text[0..100].to_string())
                    }
                    Err(e) => return Progress::Error(e.to_string().into()),
                }
            }
        } else {
            Progress::Error(format!("Network error, status: {}", res.status()).into())
        }
    });

    let request_fetch = request.handle();
    let request_cancel = request.rt_handle();

    button_fetch.set_callback(move |_| request_fetch.try_do());

    button_cancel.set_callback(move |_| {
        if request_cancel.is_canceled() {
            println!("Canceled")
        } else {
            request_cancel.cancel()
        }
    });

    let mut label = String::new();

    let mut timer = 0;

    while app.wait() {
        request.try_resolve(|progress, done| {
            match progress {
                Progress::Current(task_receiver) => {
                    button_fetch.set_label("Fetching...");
                    if let Some(value) = task_receiver {
                        text_frame.set_label(&format!("status: {}", value))
                    }
                }
                Progress::Canceled => label = "Request canceled.".to_owned(),
                Progress::Completed(value) => label = value,
                Progress::Error(e) => {
                    eprintln!("{}", &e);
                    label = e.into()
                }
            }

            if done {
                text_frame.set_label(&label);
                button_fetch.set_label("Fetch")
            }
        });

        if url.is_empty() {
            let value = if timer % 2 == 0 {
                "https://hyper.rs"
            } else {
                "https://www.rust-lang.org"
            };
            url.store(value);
            println!("url restored.");
        }

        timer += 1;

        timer_frame.set_label(timer.to_string().as_ref());
        wind.redraw();
        app::sleep(0.011);
        app::awake();
    }
    Ok(())
}
