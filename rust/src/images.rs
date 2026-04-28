use std::io::Cursor;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::{imageops, DynamicImage, ImageFormat, Rgba, RgbaImage};
use reqwest::Client;

// ── Helpers ────────────────────────────────────────────────────────────────

fn http_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("HTTP client")
}

fn to_base64_png(img: &DynamicImage) -> Result<String, String> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", B64.encode(buf.into_inner())))
}

// Shared return type: base64 PNG data URI string
pub type PngB64 = String;

// ── Avatar ────────────────────────────────────────────────────────────────

/// Download a Twitch avatar, resize to 144×144 and optionally greyscale.
pub async fn avatar(url: &str, greyscale: bool) -> Result<PngB64, String> {
    let bytes = http_client()
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let src = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let resized = src.resize_exact(144, 144, imageops::FilterType::Lanczos3);
    let img = if greyscale {
        DynamicImage::ImageRgba8(DynamicImage::ImageLuma8(resized.to_luma8()).to_rgba8())
    } else {
        resized
    };

    to_base64_png(&img)
}

// ── Placeholder ───────────────────────────────────────────────────────────

/// Circular placeholder: purple when live, grey when offline.
pub async fn placeholder(is_live: bool) -> Result<PngB64, String> {
    let fill: Rgba<u8> = if is_live {
        Rgba([0x91, 0x46, 0xFF, 0xFF])
    } else {
        Rgba([0x55, 0x55, 0x55, 0xFF])
    };
    let text_col: Rgba<u8> = if is_live {
        Rgba([0xFF, 0xFF, 0xFF, 0xFF])
    } else {
        Rgba([0xAA, 0xAA, 0xAA, 0xFF])
    };

    let mut img = RgbaImage::new(144, 144);

    // Draw filled circle
    let cx = 72i32;
    let cy = 72i32;
    let r  = 68i32;
    for y in 0..144i32 {
        for x in 0..144i32 {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= r * r {
                img.put_pixel(x as u32, y as u32, fill);
            }
        }
    }

    // Draw a simple "T" using rectangles (no font dependency)
    // Horizontal bar: y=44..62, x=36..108
    // Vertical bar:   y=62..112, x=64..80
    let t_col = text_col;
    for y in 44u32..62 {
        for x in 36u32..108 { img.put_pixel(x, y, t_col); }
    }
    for y in 62u32..112 {
        for x in 64u32..80 { img.put_pixel(x, y, t_col); }
    }

    to_base64_png(&DynamicImage::ImageRgba8(img))
}

// ── LIVE badge ────────────────────────────────────────────────────────────

/// Fill a rounded rectangle with `color`. Corners use a quarter-circle of radius `r`.
fn fill_rounded_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, r: u32, color: Rgba<u8>) {
    let r = r.min(w / 2).min(h / 2);
    let (x1, y1) = (x as i32, y as i32);
    let (x2, y2) = ((x + w) as i32, (y + h) as i32);
    let ri = r as i32;

    for py in y1..y2 {
        for px in x1..x2 {
            // Determine if the pixel falls inside the rounded rect.
            let in_rect = {
                let cx = if px < x1 + ri { x1 + ri } else if px >= x2 - ri { x2 - ri - 1 } else { px };
                let cy = if py < y1 + ri { y1 + ri } else if py >= y2 - ri { y2 - ri - 1 } else { py };
                let dx = px - cx;
                let dy = py - cy;
                dx * dx + dy * dy <= ri * ri
            };
            if in_rect {
                let pu = px as u32;
                let qu = py as u32;
                if pu < img.width() && qu < img.height() {
                    img.put_pixel(pu, qu, color);
                }
            }
        }
    }
}

/// Composite a rounded red "LIVE" badge at bottom-right of a 144×144 PNG.
pub async fn add_live_badge(input_b64: &str) -> Result<PngB64, String> {
    let data = decode_b64_png(input_b64)?;
    let mut base = image::load_from_memory(&data).map_err(|e| e.to_string())?
        .to_rgba8();

    // Rounded badge 46×24 at (94, 116), corner radius 5
    let badge_x = 94u32;
    let badge_y = 116u32;
    let badge_w = 46u32;
    let badge_h = 24u32;
    let radius  = 5u32;
    let red = Rgba([0xE9, 0x19, 0x16, 0xFF]);

    fill_rounded_rect(&mut base, badge_x, badge_y, badge_w, badge_h, radius, red);

    // Draw "LIVE" text centred within the badge (bold)
    draw_text_centered_bold(&mut base, "LIVE", badge_x, badge_w, badge_y + 4, Rgba([0xFF, 0xFF, 0xFF, 0xFF]), 18.0);

    to_base64_png(&DynamicImage::ImageRgba8(base))
}

// ── Viewer count overlay ──────────────────────────────────────────────────

/// Composite a semi-transparent viewer count banner at the top.
/// Text is rendered at 2× scale and horizontally centred on the 144-wide image.
pub async fn add_viewer_count(input_b64: &str, count: u32) -> Result<PngB64, String> {
    let data = decode_b64_png(input_b64)?;
    let mut base = image::load_from_memory(&data).map_err(|e| e.to_string())?
        .to_rgba8();

    // Font metrics for Segoe UI at 28px: ascent ≈21px, full height ≈28px.
    // TOP_PAD  = 2px gap from raw image edge to banner.
    // TEXT_Y   = 10px — clears the hardware bezel (~8–9px).
    // BANNER_H = TEXT_Y + 28px glyph + 4px bottom pad − TOP_PAD = 40px.
    const FONT_SIZE: f32 = 28.0;
    const TOP_PAD:   u32 = 0;
    const TEXT_Y:    u32 = 10;
    const BANNER_H:  u32 = 40;
    const IMG_W:     u32 = 144;

    // Dark semi-transparent strip
    let overlay_color = Rgba([0u8, 0, 0, 179]); // ~70% opacity
    for y in TOP_PAD..(TOP_PAD + BANNER_H) {
        for x in 0..IMG_W {
            let pixel = base.get_pixel_mut(x, y);
            let a = overlay_color[3] as f32 / 255.0;
            pixel[0] = (a * overlay_color[0] as f32 + (1.0 - a) * pixel[0] as f32) as u8;
            pixel[1] = (a * overlay_color[1] as f32 + (1.0 - a) * pixel[1] as f32) as u8;
            pixel[2] = (a * overlay_color[2] as f32 + (1.0 - a) * pixel[2] as f32) as u8;
        }
    }

    let label = format_viewer_count(count);
    draw_text_centered_ttf(&mut base, &label, 0, IMG_W, TEXT_Y, Rgba([0xFF, 0xFF, 0xFF, 0xFF]), FONT_SIZE);

    to_base64_png(&DynamicImage::ImageRgba8(base))
}

// ── Follows count image ───────────────────────────────────────────────────

/// Build the follows counter key image: base icon + count + "Live" label.
/// The base images live in `images/` relative to the plugin root.
/// Falls back to a generated placeholder if the files are missing.
pub async fn follows_count_image(count: u32) -> Result<PngB64, String> {
    let plugin_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let icon_name = if count > 0 { "key_online.png" } else { "key_offline.png" };
    let icon_path = plugin_dir.join("images").join(icon_name);

    let mut base = if icon_path.exists() {
        image::open(&icon_path)
            .map(|i| i.resize_exact(72, 72, imageops::FilterType::Lanczos3).to_rgba8())
            .unwrap_or_else(|_| RgbaImage::new(72, 72))
    } else {
        RgbaImage::new(72, 72)
    };

    // Semi-transparent overlay
    let ov = Rgba([0, 0, 0, 140]);
    for y in 0u32..72 {
        for x in 0u32..72 {
            let p = base.get_pixel_mut(x, y);
            let a = ov[3] as f32 / 255.0;
            p[0] = (a * ov[0] as f32 + (1.0 - a) * p[0] as f32) as u8;
            p[1] = (a * ov[1] as f32 + (1.0 - a) * p[1] as f32) as u8;
            p[2] = (a * ov[2] as f32 + (1.0 - a) * p[2] as f32) as u8;
        }
    }

    // Count number — centred, top half of the 72px image.
    let count_str = count.to_string();
    draw_text_centered_ttf(&mut base, &count_str, 0, 72, 8, Rgba([0xFF, 0xFF, 0xFF, 0xFF]), 22.0);

    // "Live" label — centred, bottom half.
    let live_col: Rgba<u8> = if count > 0 {
        Rgba([0x91, 0x46, 0xFF, 0xFF])
    } else {
        Rgba([0x88, 0x88, 0x88, 0xFF])
    };
    draw_text_centered_ttf(&mut base, "Live", 0, 72, 46, live_col, 13.0);

    to_base64_png(&DynamicImage::ImageRgba8(base))
}

// ── Internal helpers ──────────────────────────────────────────────────────

fn decode_b64_png(b64: &str) -> Result<Vec<u8>, String> {
    let raw = b64.trim_start_matches("data:image/png;base64,");
    B64.decode(raw).map_err(|e| e.to_string())
}

fn format_viewer_count(n: u32) -> String {
    if n >= 1_000_000 {
        let v = n as f64 / 1_000_000.0;
        if v.fract() == 0.0 { format!("{:.0}M", v) } else { format!("{:.1}M", v) }
    } else if n >= 1_000 {
        let v = n as f64 / 1_000.0;
        if v.fract() == 0.0 { format!("{:.0}K", v) } else { format!("{:.1}K", v) }
    } else {
        n.to_string()
    }
}

// ── TTF font rendering ──────────────────────────────────────────────────────────────────────────

static FONT_BYTES:      &[u8] = include_bytes!("../assets/sans.ttf");
static FONT_BOLD_BYTES: &[u8] = include_bytes!("../assets/sans_bold.ttf");

fn get_font() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_BYTES).expect("valid embedded font")
}

fn get_font_bold() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_BOLD_BYTES).expect("valid embedded bold font")
}

/// Measure the rendered pixel width of `text` at `size` px using `font`.
fn measure_text_with(font: &FontRef<'_>, text: &str, size: f32) -> f32 {
    let sf = font.as_scaled(PxScale::from(size));
    text.chars().map(|c| sf.h_advance(font.glyph_id(c))).sum()
}

/// Measure the rendered pixel width of `text` at `size` px.
fn measure_text(text: &str, size: f32) -> f32 {
    measure_text_with(&get_font(), text, size)
}

/// Draw `text` at (x, y_top) using the given `font`, with anti-aliased alpha compositing.
fn draw_text_ttf_with(img: &mut RgbaImage, font: &FontRef<'_>, text: &str, x: f32, y_top: f32, color: Rgba<u8>, size: f32) {
    let scale    = PxScale::from(size);
    let sf       = font.as_scaled(scale);
    let baseline = y_top + sf.ascent();
    let mut cx   = x;

    for c in text.chars() {
        let gid   = font.glyph_id(c);
        let glyph = gid.with_scale_and_position(scale, ab_glyph::point(cx, baseline));
        if let Some(og) = font.outline_glyph(glyph) {
            let bb = og.px_bounds();
            og.draw(|gx, gy, cov| {
                let px = bb.min.x as i32 + gx as i32;
                let py = bb.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 {
                    let (px, py) = (px as u32, py as u32);
                    if px < img.width() && py < img.height() {
                        let bg = *img.get_pixel(px, py);
                        let a  = (cov * color[3] as f32 / 255.0).min(1.0);
                        img.put_pixel(px, py, Rgba([
                            (a * color[0] as f32 + (1.0 - a) * bg[0] as f32) as u8,
                            (a * color[1] as f32 + (1.0 - a) * bg[1] as f32) as u8,
                            (a * color[2] as f32 + (1.0 - a) * bg[2] as f32) as u8,
                            255,
                        ]));
                    }
                }
            });
        }
        cx += sf.h_advance(gid);
    }
}

/// Draw `text` at (x, y_top) with anti-aliased alpha compositing onto `img`.
/// `y_top` is the pixel row where the tallest glyph cap begins.
fn draw_text_ttf(img: &mut RgbaImage, text: &str, x: f32, y_top: f32, color: Rgba<u8>, size: f32) {
    draw_text_ttf_with(img, &get_font(), text, x, y_top, color, size);
}

/// Draw `text` horizontally centred within a region starting at `reg_x` of width `reg_w`.
/// `y_top` is where the cap-height of the text begins.
fn draw_text_centered_ttf(
    img:   &mut RgbaImage,
    text:  &str,
    reg_x: u32,
    reg_w: u32,
    y_top: u32,
    color: Rgba<u8>,
    size:  f32,
) {
    let tw = measure_text(text, size);
    let x  = reg_x as f32 + (reg_w as f32 - tw).max(0.0) / 2.0;
    draw_text_ttf(img, text, x, y_top as f32, color, size);
}

/// Draw bold `text` horizontally centred within a region starting at `reg_x` of width `reg_w`.
fn draw_text_centered_bold(
    img:   &mut RgbaImage,
    text:  &str,
    reg_x: u32,
    reg_w: u32,
    y_top: u32,
    color: Rgba<u8>,
    size:  f32,
) {
    let font = get_font_bold();
    let tw   = measure_text_with(&font, text, size);
    let x    = reg_x as f32 + (reg_w as f32 - tw).max(0.0) / 2.0;
    draw_text_ttf_with(img, &font, text, x, y_top as f32, color, size);
}
