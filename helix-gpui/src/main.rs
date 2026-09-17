mod backend;
mod clipboard;
mod design;
mod input;
mod surface;
mod view;
mod widgets;

use anyhow::{Context as _, Result};
use backend::{Frame, SurfaceBackend};
use gpui::{prelude::*, *};
use helix_term::{
    application::Application,
    args::Args,
    config::{Config, ConfigLoadError},
};
use helix_view::graphics::Rect;
use std::sync::{Arc, Mutex};

fn main() -> Result<()> {
    std::process::exit(run()?);
}

fn run() -> Result<i32> {
    let args = Args::parse_args()?;
    helix_loader::initialize_config_file(args.config_file.clone());
    helix_loader::initialize_log_file(args.log_file.clone());
    if args.display_help {
        println!("hx-gpui {}\n\nUsage: hx-gpui [OPTIONS] [files]...\n\nHelix in a GPUI window. Accepts Helix's file[:row[:col]], +line,\n--vsplit, --hsplit, --tutor, --working-dir, --config, --log,\n--health, --grammar, --strict, --version and verbosity options.\n\nSee helix-gpui/README.md for desktop shortcuts and build requirements.", helix_loader::VERSION_AND_GIT_HASH);
        return Ok(0);
    }
    if args.display_version {
        println!("hx-gpui {}", helix_loader::VERSION_AND_GIT_HASH);
        return Ok(0);
    }
    if args.health {
        helix_term::health::print_health(args.health_arg)?;
        return Ok(0);
    }
    if args.fetch_grammars {
        helix_loader::grammar::fetch_grammars(args.strict)?;
        return Ok(0);
    }
    if args.build_grammars {
        helix_loader::grammar::build_grammars(None, args.strict)?;
        return Ok(0);
    }
    if let Some(path) = args
        .working_directory
        .as_ref()
        .or_else(|| args.files.first().map(|(p, _)| p).filter(|p| p.is_dir()))
    {
        helix_stdx::env::set_current_working_dir(path)?;
    }
    helix_term::logging::init_file(
        if args.verbosity == 0 && std::env::var_os("HELIX_GPUI_TRACE_LATENCY").is_some() {
            log::LevelFilter::Info
        } else if args.verbosity == 0 {
            log::LevelFilter::Warn
        } else {
            log::LevelFilter::Debug
        },
        &helix_loader::log_file(),
    )?;
    let mut config = match Config::load_default() {
        Ok(config) => config,
        Err(ConfigLoadError::Error(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            Config::default()
        }
        Err(err) => anyhow::bail!("Cannot load configuration: {err}"),
    };
    if config.theme.is_none() {
        config.theme = Some(helix_view::theme::Config::Constant("helix_gpui".into()));
    }
    let trust =
        helix_loader::workspace_trust::WorkspaceTrust::new((&config.editor.workspace_trust).into());
    let languages = helix_core::config::user_lang_loader(&trust)?;
    let area = Rect::new(0, 0, 120, 40);
    let (frames_tx, frames_rx) = tokio::sync::watch::channel(Arc::new(Frame::new(area)));
    let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
    let (clipboard_tx, clipboard_rx) = tokio::sync::mpsc::unbounded_channel();
    let backend = SurfaceBackend::new(area, frames_tx.clone());
    // The editor and its non-Send compositor stay on this thread. Tokio workers
    // continue servicing subprocesses, language servers and background jobs.
    let worker = std::thread::Builder::new()
        .name("helix-editor".into())
        .spawn(move || {
            let native_frames = frames_tx.clone();
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<i32> {
                    let runtime = tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()?;
                    runtime.block_on(async move {
                        let mut app = Application::new_with_backend(
                            args, config, languages, trust, backend, false,
                        )?;
                        app.set_frontend_renderer(Box::new(move |frame| {
                            native_frames.send_replace(Arc::new(Frame::from_native(frame)));
                        }));
                        app.editor.registers.set_clipboard_provider(
                            helix_view::clipboard::ClipboardProvider::Native(
                                helix_view::clipboard::NativeClipboardProvider(Arc::new(
                                    clipboard::Clipboard(clipboard_tx),
                                )),
                            ),
                        );
                        app.run_frontend(&mut tokio_stream::wrappers::UnboundedReceiverStream::new(
                            events_rx,
                        ))
                        .await
                    })
                }))
                .map_err(|_| anyhow::anyhow!("Editor thread panicked; see the Helix log"))
                .and_then(|result| result)
                .map_err(|error| format!("{error:#}"));
            let mut frame = (**frames_tx.borrow()).clone();
            frame.exit = Some(result.clone());
            frames_tx.send_replace(Arc::new(frame));
            result
        })
        .context("Cannot start editor thread")?;

    let window_error = Arc::new(Mutex::new(None));
    let startup_error = window_error.clone();
    gpui::Application::new().run(move |cx| {
        view::install_shortcuts(cx);
        clipboard::connect(clipboard_rx, cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Helix".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| view::EditorView::new(events_tx, frames_rx, area, window, cx)),
        );
        if let Err(error) = result {
            eprintln!("Cannot open Helix window: {error}");
            *startup_error.lock().unwrap() = Some(error.to_string());
            cx.quit();
        }
        cx.activate(true);
    });
    let result = worker
        .join()
        .map_err(|_| anyhow::anyhow!("Editor thread panicked"))?
        .map_err(anyhow::Error::msg);
    if let Some(error) = window_error.lock().unwrap().take() {
        anyhow::bail!("Cannot open Helix window: {error}");
    }
    result
}
