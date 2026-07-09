#![windows_subsystem = "windows"]

mod app;
mod cevio;
mod dictionary;
mod gemini;
mod settings;
mod text_filter;
mod youtube;
mod web_server;

fn main() -> eframe::Result<()> {
    let rt = tokio::runtime::Runtime::new().expect("tokioランタイムの作成に失敗");
    let _guard = rt.enter();

    // アイコンの埋め込み
    let icon_bytes = include_bytes!("../assets/icon.png");
    let icon_data = if let Ok(image) = image::load_from_memory(icon_bytes) {
        let image = image.to_rgba8();
        let (width, height) = image.dimensions();
        Some(egui::IconData {
            rgba: image.into_raw(),
            width,
            height,
        })
    } else {
        None
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("YouTubeLiveChatViewer")
        .with_inner_size([960.0, 640.0])
        .with_min_inner_size([700.0, 400.0]);

    if let Some(icon) = icon_data {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "YouTubeLiveChatViewer",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
