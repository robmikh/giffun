use clap::{App, Arg};
use robmikh_common::desktop::displays::get_display_handle_from_index;
use windows::{
    core::Result,
    Win32::{Foundation::HWND, Graphics::Gdi::HMONITOR},
};

use crate::util::window_info::get_window_from_query;

pub struct CliOptions {
    pub capture_type: CaptureType,
    pub output_file: String,
    pub disable_frame_diff: bool,
}

pub enum CaptureType {
    Window(HWND),
    Monitor(HMONITOR),
}

pub fn parse_cli() -> Result<CliOptions> {
    let mut app = build_cli_app();

    // Handle /?
    let args: Vec<_> = std::env::args().collect();
    if args.contains(&"/?".to_owned()) {
        app.print_help().unwrap();
        std::process::exit(0);
    }

    let matches = app.get_matches();

    let capture_type = if let Some(value) = matches.value_of("display") {
        let display_index: usize = value.parse().expect("Invalid display index value!");
        let display_handle = get_display_handle_from_index(display_index)
            .expect("Could not find a monitor with a matching index.");
        CaptureType::Monitor(display_handle)
    } else if let Some(window_query) = matches.value_of("window") {
        let window_info = get_window_from_query(window_query)?;
        CaptureType::Window(window_info.handle)
    } else {
        // Default to recording the primary monitor
        let display_handle = get_display_handle_from_index(0).expect("No monitors detected!");
        CaptureType::Monitor(display_handle)
    };

    let disable_frame_diff = if cfg!(feature = "debug") {
        matches.is_present("nodiff")
    } else {
        false
    };

    let output_file = matches.value_of("OUTPUT FILE").unwrap();

    Ok(CliOptions {
        capture_type,
        output_file: output_file.to_owned(),
        disable_frame_diff,
    })
}

fn build_cli_app() -> App<'static, 'static> {
    let mut app = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("display")
                .short("d")
                .long("display")
                .value_name("display index")
                .help("The index of the display you'd like to record.")
                .takes_value(true)
                .conflicts_with_all(&["window", "primary"]),
        )
        .arg(
            Arg::with_name("window")
                .short("w")
                .long("window")
                .value_name("window query")
                .help("A partial string that matches the title of the window you'd like to record.")
                .takes_value(true)
                .conflicts_with_all(&["display", "primary"]),
        )
        .arg(
            Arg::with_name("primary")
                .short("p")
                .long("primary")
                .help("A shortcut to record the primary display.")
                .takes_value(false)
                .conflicts_with_all(&["window", "display"]),
        );
    if cfg!(feature = "debug") {
        app = app.arg(
            Arg::with_name("nodiff")
                .long("nodiff")
                .help("Disable frame diffing. (DEBUG)")
                .takes_value(false)
                .required(false),
        );
    }
    app = app.arg(
        Arg::with_name("OUTPUT FILE")
            .help("The output file that will contain the gif.")
            .default_value("recording.gif")
            .required(false),
    );

    app
}
