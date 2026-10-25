use peek::{
    Application,
    git::Mode,
    service::{GitCommand, GitService},
};
use plico_dev::services::HostService;
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{ServiceView, Source};
use snafu::Snafu;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(usage_rs::Cli)]
#[usage(
    bin = "peek",
    about = "Peek, implemented with Plico. Read-only Git diff viewer."
)]
struct Options {
    /// Repository directory (defaults to the current directory).
    path: Option<PathBuf>,
    #[usage(long)]
    capture: Option<PathBuf>,
    #[usage(long)]
    dark: bool,
    /// Commit hash/prefix, or HEAD. Omit to inspect the working tree.
    #[usage(long)]
    commit: Option<String>,
    #[usage(long, default = "changes")]
    mode: String,
    #[usage(long)]
    file: Option<String>,
    #[usage(long, default = "1280")]
    width: u32,
    #[usage(long, default = "800")]
    height: u32,
    /// Measure forced declarations and warm layout-cache builds; no GPU timings.
    #[usage(long)]
    benchmark: bool,
    #[usage(long, default = "1")]
    scale: u32,
}
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("{source}"))]
    Io { source: std::io::Error },
    #[snafu(display("{source}"))]
    Window { source: plico_vello::DesktopError },
    #[snafu(display("{message}"))]
    Input { message: String },
}
fn run() -> Result<(), Error> {
    let options = Options::parse();
    if !(900..=4096).contains(&options.width) || !(500..=4096).contains(&options.height) {
        return Err(Error::Input {
            message: "viewport must be 900..4096 by 500..4096".into(),
        });
    }
    if !(1..=2).contains(&options.scale) {
        return Err(Error::Input {
            message: "--scale must be 1 or 2".into(),
        });
    }
    let mode = match options.mode.as_str() {
        "changes" => Mode::Changes,
        "files" => Mode::Files,
        _ => {
            return Err(Error::Input {
                message: "--mode must be changes or files".into(),
            });
        }
    };
    let path = options
        .path
        .unwrap_or(std::env::current_dir().map_err(|source| Error::Io { source })?);
    let mut services = GitService::new(&path);
    if options.capture.is_some()
        || options.benchmark
        || options.commit.is_some()
        || mode == Mode::Files
    {
        services
            .wait_ready(Duration::from_secs(30))
            .map_err(|message| Error::Input { message })?;
        let snapshot = services.snapshot();
        let revision = if let Some(commit) = &options.commit {
            snapshot
                .commits
                .iter()
                .find(|c| commit == "HEAD" || c.hash.starts_with(commit))
                .ok_or_else(|| Error::Input {
                    message: format!("commit not found in the latest 100: {commit}"),
                })?
                .hash
                .clone()
        } else {
            String::new()
        };
        services.send(GitCommand::Browse {
            revision,
            mode,
            file: options.file.unwrap_or_default(),
        });
        services
            .wait_ready(Duration::from_secs(30))
            .map_err(|message| Error::Input { message })?;
    }
    if options.capture.is_some()
        && let Some(email) = services.snapshot().commits.first().map(|c| c.email.clone())
    {
        services.wait_avatar(&email, Duration::from_secs(16));
    }
    let app = Application::new(options.dark, mode);
    let view = ServiceView::new(app, services);
    if options.capture.is_some() || options.benchmark {
        let clock = plico_core::VirtualClock::default();
        let mut source = Source::with_clock(view, clock.clone()).with_font_family("Noto Sans");
        let mut text = TextContext::new();
        source.build_frame(&mut text, options.width, options.height);
        clock.advance(Duration::from_secs(1)).unwrap();
        let nodes = source.build_frame(&mut text, options.width, options.height);
        if let Some(error) = &source.last_error {
            return Err(Error::Input {
                message: error.to_string(),
            });
        }
        if let Some(path) = &options.capture {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent).map_err(|source| Error::Io { source })?;
            }
            let mut scene = Scene::new();
            source.paint_scene(
                &mut scene,
                &nodes,
                &mut text,
                (options.width, options.height),
                f64::from(options.scale),
            );
            plico_shell::renderer::render_to_png(
                &scene,
                options.width * options.scale,
                options.height * options.scale,
                source.base_color(),
                &path.to_string_lossy(),
            )
            .map_err(|source| Error::Window { source })?;
            println!("wrote {}", path.display());
        }
        if options.benchmark {
            const FRAMES: usize = 200;
            for _ in 0..5 {
                source.request_rebuild();
                source.build_frame(&mut text, options.width, options.height);
            }
            let start = Instant::now();
            let mut declaration_us = 0;
            for _ in 0..FRAMES {
                source.request_rebuild();
                source.build_frame(&mut text, options.width, options.height);
                declaration_us += source.declaration_micros;
            }
            println!(
                "{}",
                serde_json::json!({"frames":FRAMES,"warm_forced_snapshot_build_us":start.elapsed().as_micros() as f64 / FRAMES as f64,
                "declaration_us":declaration_us as f64 / FRAMES as f64,"viewport":[options.width,options.height],"gpu_included":false,"git_io_included":false,"idle_measurement":false})
            );
        }
        return Ok(());
    }
    plico_vello::run_source_with_options(
        Source::new(view).with_font_family("Noto Sans"),
        plico_vello::WindowOptions::new()
            .title("Peek · Plico")
            .initial_inner_size(options.width, options.height)
            .min_inner_size(900, 500),
    )
    .map_err(|source| Error::Window { source })
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("peek: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
