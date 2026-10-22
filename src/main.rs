use visor_tui::app::VisorApp;

fn main() -> std::io::Result<()> {
    let mut app = VisorApp::new()?;
    app.run()
}