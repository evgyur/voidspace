#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    let _ = voidspace_app::diagnostics::log_line("Voidspace startup");
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/voidspace.png"))
        .expect("embedded Voidspace window icon must be valid PNG");
    let options = eframe::NativeOptions {
        viewport: voidspace_app::main_viewport().with_icon(icon),
        centered: true,
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Voidspace",
        options,
        Box::new(|context| Ok(Box::new(voidspace_app::VoidspaceApp::new(context)))),
    )
}
