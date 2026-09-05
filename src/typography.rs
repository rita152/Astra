//! Process-local rendering policy for ChatGPT's `-webkit-font-smoothing: antialiased`.

struct Typography {
    brand_family: gpui::SharedString,
}
impl gpui::Global for Typography {}

pub fn initialize_fonts(cx: &mut gpui::App) {
    let family = installed_brand_font()
        .and_then(|bytes| {
            cx.text_system()
                .add_fonts(vec![std::borrow::Cow::Owned(bytes)])
                .ok()
        })
        .map_or(crate::theme::UI_FONT_FAMILY, |_| "OpenAI Sans");
    cx.set_global(Typography {
        brand_family: family.into(),
    });
}

pub fn brand_font(cx: &gpui::App) -> gpui::Font {
    let mut font = crate::theme::ui_font();
    if let Some(typography) = cx.try_global::<Typography>() {
        font.family = typography.brand_family.clone();
    }
    font.weight = gpui::FontWeight::SEMIBOLD;
    font
}

// OpenAI Sans is not redistributed with this repository. CoreGraphics accepts
// the unmodified WOFF2 resource directly from the user's installed desktop app.
fn installed_brand_font() -> Option<Vec<u8>> {
    let mut applications = vec![std::path::PathBuf::from("/Applications/ChatGPT.app")];
    if let Some(home) = std::env::var_os("HOME") {
        applications.push(std::path::PathBuf::from(home).join("Applications/ChatGPT.app"));
    }
    applications
        .into_iter()
        .find_map(|app| read_brand_font_from_asar(&app.join("Contents/Resources/app.asar")).ok())
}

fn read_brand_font_from_asar(path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let mut prefix = [0; 16];
    file.read_exact(&mut prefix)?;
    let header_size = u32::from_le_bytes(prefix[4..8].try_into()?) as u64;
    let json_size = u32::from_le_bytes(prefix[12..16].try_into()?) as usize;
    anyhow::ensure!(
        header_size <= 16 * 1024 * 1024 && json_size as u64 + 8 <= header_size,
        "invalid ASAR header"
    );
    let mut json = vec![0; json_size];
    file.read_exact(&mut json)?;
    let header: serde_json::Value = serde_json::from_slice(&json)?;
    let assets = header
        .pointer("/files/webview/files/assets/files")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("ASAR assets missing"))?;
    let entry = assets
        .iter()
        .find(|(name, _)| name.starts_with("OpenAISans-Medium-") && name.ends_with(".woff2"))
        .map(|(_, entry)| entry)
        .ok_or_else(|| anyhow::anyhow!("brand font missing"))?;
    let offset = entry["offset"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("font offset missing"))?
        .parse::<u64>()?;
    let size = entry["size"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("font size missing"))?;
    let start = (8u64 + header_size)
        .checked_add(offset)
        .ok_or_else(|| anyhow::anyhow!("font offset overflow"))?;
    anyhow::ensure!(
        size <= 4 * 1024 * 1024
            && start
                .checked_add(size)
                .is_some_and(|end| end <= file.metadata().map(|m| m.len()).unwrap_or(0)),
        "invalid font range"
    );
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = vec![0; size as usize];
    file.read_exact(&mut bytes)?;
    anyhow::ensure!(bytes.starts_with(b"wOF2"), "invalid WOFF2 font");
    Ok(bytes)
}

/// GPUI's macOS backend adds luminance-dependent stroke dilation by default.
/// Chromium's antialiased CSS explicitly opts out of that CoreGraphics smoothing.
/// Set the volatile argument domain before GPUI initializes its cached policy;
/// this does not write preferences or change font rendering in other apps.
#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub fn configure() {
    use cocoa::{base::nil, foundation::NSString};
    use objc::{class, msg_send, rc::autoreleasepool, runtime::Object, sel, sel_impl};

    autoreleasepool(|| unsafe {
        let defaults: *mut Object = msg_send![class!(NSUserDefaults), standardUserDefaults];
        let domain = NSString::alloc(nil).init_str("NSArgumentDomain");
        let key = NSString::alloc(nil).init_str("AppleFontSmoothing");
        let existing: *mut Object = msg_send![defaults, volatileDomainForName: domain];
        let arguments: *mut Object = msg_send![existing, mutableCopy];
        let zero: *mut Object = msg_send![class!(NSNumber), numberWithInt: 0i32];
        let _: () = msg_send![arguments, setObject: zero forKey: key];
        let _: () = msg_send![defaults, setVolatileDomain: arguments forName: domain];
        let _: () = msg_send![arguments, release];
        let _: () = msg_send![key, release];
        let _: () = msg_send![domain, release];
    });
}

#[cfg(not(target_os = "macos"))]
pub fn configure() {}

/// A native, live-text fixture. It exercises the same text system as the app,
/// including CJK fallback and emoji; no reference pixels enter the renderer.
#[cfg(feature = "screenshot")]
pub fn capture_specimen(args: &[String]) -> bool {
    use gpui::{
        App, AppContext, Bounds, Context, FontWeight, Render, Window, WindowBounds, WindowOptions,
        div, prelude::*, px, rgba, size,
    };

    if !args.iter().any(|arg| arg == "--typography-specimen") {
        return false;
    }
    if !args
        .iter()
        .any(|arg| arg == "--typography-native-smoothing")
    {
        configure();
    }
    #[derive(serde::Deserialize)]
    struct Sample {
        text: String,
        size: f32,
        weight: f32,
        height: f32,
        #[serde(default)]
        mono: bool,
        #[serde(default)]
        brand: bool,
    }
    struct Specimen(Vec<Sample>);
    impl Render for Specimen {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .relative()
                .children([false, true].map(|dark| {
                    div()
                        .absolute()
                        .left(px(if dark { 500.0 } else { 0.0 }))
                        .top(px(0.0))
                        .w(px(500.0))
                        .h_full()
                        .bg(rgba(if dark { 0x181818ff } else { 0xffffffff }))
                        .text_color(rgba(if dark { 0xdfdfdfff } else { 0x1a1c1fff }))
                        .children(self.0.iter().enumerate().map(|(index, sample)| {
                            let mut font = crate::theme::ui_font();
                            if sample.brand {
                                font = brand_font(cx);
                            }
                            font.weight = FontWeight(sample.weight);
                            if sample.mono {
                                font.family = crate::theme::UI_MONOSPACE_FONT_FAMILY.into();
                            }
                            div()
                                .absolute()
                                .left(px(24.0))
                                .top(px(40.0 + index as f32 * 36.0))
                                .font(font)
                                .text_size(px(sample.size))
                                .line_height(px(sample.height))
                                .child(sample.text.clone())
                        }))
                }))
        }
    }
    let output = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--screenshot="))
        .map(str::to_owned);
    let display_index = args.iter().find_map(|arg| {
        arg.strip_prefix("--typography-display=")?
            .parse::<usize>()
            .ok()
    });
    gpui_platform::application().run(move |cx: &mut App| {
        initialize_fonts(cx);
        let display_id = display_index.map(|index| {
            cx.displays()
                .get(index)
                .expect("typography display index")
                .id()
        });
        let bounds = Bounds::centered(display_id, size(px(1000.0), px(620.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id,
                titlebar: None,
                ..Default::default()
            },
            move |window, cx| {
                if let Some(output) = output {
                    crate::schedule_screenshot(window, output, 3);
                }
                cx.new(|_| {
                    Specimen(
                        serde_json::from_str(include_str!("../scripts/typography_samples.json"))
                            .expect("typography samples"),
                    )
                })
            },
        )
        .expect("open typography specimen");
        cx.activate(true);
    });
    true
}
