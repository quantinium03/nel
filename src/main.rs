use atomic_float::AtomicF64;
use device_query::{DeviceEvents, DeviceState};
use reqwest::{self, Client};
use serde_json::json;
use std::{
    error::Error,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread, time::Duration,
};
use tokio::{spawn, time::interval};

#[derive(Debug)]
struct Keyboard {
    keypress: AtomicU64,
}

#[derive(Debug)]
struct Mouse {
    right_click: AtomicU64,
    left_click: AtomicU64,
}

#[derive(Debug)]
struct MouseDistance {
    distance: AtomicF64,
}

const PIXEL_M_CONVERSION: f64 = 0.0002645833;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let keyboard_counter = Arc::new(Keyboard {
        keypress: AtomicU64::new(0),
    });
    let mouse_counter = Arc::new(Mouse {
        right_click: AtomicU64::new(0),
        left_click: AtomicU64::new(0),
    });
    let mouse_distance = Arc::new(MouseDistance {
        distance: AtomicF64::new(0.0),
    });

    let kb_counter = Arc::clone(&keyboard_counter);
    thread::spawn(move || {
        let device_state = DeviceState::new();
        let _guard = device_state.on_key_down(move |_| {
            kb_counter.keypress.fetch_add(1, Ordering::Relaxed);
        });
        thread::park();
    });

    let mouse_counter_clone = Arc::clone(&mouse_counter);
    thread::spawn(move || {
        let device_state = DeviceState::new();
        let _guard = device_state.on_mouse_down(move |button| {
            if *button == 1 {
                mouse_counter_clone
                    .left_click
                    .fetch_add(1, Ordering::Relaxed);
            }
            if *button == 3 {
                mouse_counter_clone
                    .right_click
                    .fetch_add(1, Ordering::Relaxed);
            }
        });
        thread::park();
    });

    let mouse_dist = Arc::clone(&mouse_distance);
    thread::spawn(move || {
        let device_state = DeviceState::new();
        let last_pos = (AtomicF64::new(0.0), AtomicF64::new(0.0));
        let _guard = device_state.on_mouse_move(move |pos| {
            let dx = pos.0 as f64 - last_pos.0.load(Ordering::Relaxed);
            let dy = pos.1 as f64 - last_pos.1.load(Ordering::Relaxed);
            let d = f64::sqrt(dx * dx + dy * dy);
            last_pos.0.store(pos.0 as f64, Ordering::Relaxed);
            last_pos.1.store(pos.1 as f64, Ordering::Relaxed);
            mouse_dist.distance.fetch_add(d, Ordering::Relaxed);
        });
        thread::park();
    });

    let kb_counter = Arc::clone(&keyboard_counter);
    let report_task = spawn(async move {
        let mut interval = interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            let keypress = kb_counter.keypress.load(Ordering::Relaxed);
            if let Err(e) = send_keypress(keypress).await {
                eprintln!("Failed to send keypress: {}", e);
            } else {
                kb_counter.keypress.swap(0, Ordering::Relaxed);
            }
        }
    });

    let mouse_counter = Arc::clone(&mouse_counter);
    let mouse_dist = Arc::clone(&mouse_distance);
    let mouse_stats_task = spawn(async move {
        let mut interval = interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            let right_clicks = mouse_counter.right_click.load(Ordering::Relaxed);
            let left_clicks = mouse_counter.left_click.load(Ordering::Relaxed);
            let distance = mouse_dist.distance.load(Ordering::Relaxed);

            if let Err(e) = send_mouse_stats(right_clicks, left_clicks, distance).await {
                eprintln!("Failed to send mouse stats: {}", e);
            } else {
                mouse_counter.right_click.swap(0, Ordering::Relaxed);
                mouse_counter.left_click.swap(0, Ordering::Relaxed);
                mouse_dist.distance.swap(0.0, Ordering::Relaxed);
            }
        }
    });

    tokio::try_join!(mouse_stats_task, report_task)?;
    Ok(())
}

async fn send_mouse_stats(
    right_click: u64,
    left_clicks: u64,
    distance: f64,
) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let body = json!({
        "password": "",
        "rightClick": right_click,
        "leftClick": left_clicks,
        "mouseTravel": distance * PIXEL_M_CONVERSION,
    });

    client
        .put("")
        .json(&body)
        .send()
        .await?;

    Ok(())
}

async fn send_keypress(keypress: u64) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let body = json!({
        "password": "",
        "keypress": keypress,
    });

    client
        .put("")
        .json(&body)
        .send()
        .await?;

    Ok(())
}
