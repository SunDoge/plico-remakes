//! Fixed text sheet for comparing registry Glifo with Plico's optional patch.
use plico_core::prelude::*;
use plico_renderer::{FrameSource, Scene, TextContext};
use plico_runtime::{Source, View};
use snafu::Snafu;
use std::path::PathBuf;

#[derive(usage_rs::Cli)]
#[usage(bin = "text-rendering")]
struct Options {
    #[usage(long)]
    output: PathBuf,
    #[usage(long, default = "1")]
    scale: u32,
    #[usage(long)]
    dark: bool,
}

struct Sheet {
    dark: bool,
}
impl View for Sheet {
    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).height_percent(100.).padding(20).gap(12).background(Color::Panel) + |cx| {
            text(cx, "中文笔画与混排 / Chinese and Latin").font_size(24).weight(600);
            text(cx, "Noto Sans CJK SC · 12 / 14 / 16 / 24 px · 400 / 500 / 700").font_size(14);
            for size in [12, 14, 16, 24] {
                for weight in [400, 500, 700] {
                    cx.keyed(u64::from(size) * 1000 + u64::from(weight), |cx| {
                    row(cx).gap(16).align(Align::Center) + |cx| {
                        text!(cx, "{size}px / {weight}").width(90.).font_size(12);
                        column(cx).gap(4) + |cx| {
                            text(cx, "中文渲染：永国器藏赢，横竖撇捺，输入设置与文件选择。").font_size(size).weight(weight);
                            text(cx, "中英混排 Rust GUI 2026 · s S 8 B · ffi fl · 你好，世界！").font_size(size).weight(weight);
                        };
                    };
                    });
                }
            }
            text(cx, "代码与中文注释 / monospace fallback").font_size(16).weight(600);
            for size in [12, 14, 16] {
                cx.keyed(u64::from(size), |cx| {
                    text(cx, "let message = \"你好，世界\"; // 检查笔画粗细 ss SS 0123456789").font_family(FontFamily::Monospace).font_size(size);
                });
            }
        };
    }
}

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("{source}"))]
    Io { source: std::io::Error },
    #[snafu(display("{source}"))]
    Render { source: plico_vello::DesktopError },
    #[snafu(display("scale must be 1 or 2"))]
    Scale,
}
fn main() -> Result<(), Error> {
    let options = Options::parse();
    if !(1..=2).contains(&options.scale) {
        return Err(Error::Scale);
    }
    if let Some(parent) = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io { source })?;
    }
    let mut source = Source::new(Sheet { dark: options.dark }).with_font_family("Noto Sans CJK SC");
    let mut text = TextContext::new();
    let viewport = (1100, 1150);
    let nodes = source.build_frame(&mut text, viewport.0, viewport.1);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
    let mut scene = Scene::new();
    source.paint_scene(
        &mut scene,
        &nodes,
        &mut text,
        viewport,
        f64::from(options.scale),
    );
    plico_shell::renderer::render_to_png(
        &scene,
        viewport.0 * options.scale,
        viewport.1 * options.scale,
        source.base_color(),
        &options.output.to_string_lossy(),
    )
    .map_err(|source| Error::Render { source })
}
