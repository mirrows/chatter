//! 生成 Chatters 应用图标（512x512 PNG → Windows .ico 所需尺寸）。
//!
//! 图案：渐变圆角方形背景 + 后方浅色小气泡（对端设备）+ 白色主聊天气泡 + 绿色对勾（传输完成）。
//! 使用 image crate 程序化绘制，无需外部素材；改动后重新运行即可得到一致的图标。

use std::fs;
use std::path::Path;

fn main() {
    let size = 512u32;
    let mut img = image::RgbaImage::from_pixel(size, size, image::Rgba([0, 0, 0, 0]));
    let cx = size as f64 / 2.0;
    let cy = size as f64 / 2.0;

    // 1) 圆角方形背景 + 垂直渐变（深蓝 → 青）。
    let half = size as f64 * 0.47;
    let radius = 104.0;
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let px = x as f64 + 0.5;
        let py = y as f64 + 0.5;
        if !rounded(px, py, cx - half, cy - half, cx + half, cy + half, radius) {
            continue;
        }
        let t = (py / size as f64).clamp(0.0, 1.0);
        let top = [18.0_f64, 52.0, 96.0];
        let bottom = [20.0_f64, 112.0, 140.0];
        *pixel = image::Rgba([
            (top[0] + (bottom[0] - top[0]) * t) as u8,
            (top[1] + (bottom[1] - top[1]) * t) as u8,
            (top[2] + (bottom[2] - top[2]) * t) as u8,
            255,
        ]);
    }

    // 2) 后方小气泡：代表对端设备（P2P 意象）。
    let peer = image::Rgba([120, 224, 200, 255]);
    fill_rounded(&mut img, 296.0, 96.0, 412.0, 196.0, 36.0, peer);
    for t in 0..34u32 {
        let y = 196 + t;
        let x_start = 356i32 - (t as i32 / 2);
        for x in x_start..(x_start + 30 - (t as i32) / 3) {
            put_checked(&mut img, x, y as i32, peer);
        }
    }

    // 3) 主气泡（白色）+ 左下尾巴。
    let bubble = image::Rgba([255, 255, 255, 255]);
    fill_rounded(&mut img, 96.0, 168.0, 372.0, 360.0, 44.0, bubble);
    for t in 0..52u32 {
        let y = 360 + t;
        let x_start = 132i32 + (t as i32 / 2);
        for x in x_start..(x_start + 44 - (t as i32) / 2) {
            put_checked(&mut img, x, y as i32, bubble);
        }
    }

    // 4) 绿色对勾（传输完成意象）。
    let accent = image::Rgba([40, 190, 120, 255]);
    draw_thick_line(&mut img, (150.0, 258.0), (210.0, 318.0), 28.0, accent);
    draw_thick_line(&mut img, (210.0, 318.0), (324.0, 204.0), 28.0, accent);

    let out_dir = Path::new("src-tauri/icons");
    fs::create_dir_all(out_dir).unwrap();

    // 主图与各尺寸缩放。
    image::save_buffer(
        out_dir.join("chatters-512.png"),
        &img,
        size,
        size,
        image::ExtendedColorType::Rgba8,
    )
    .unwrap();
    for s in [32u32, 128u32] {
        let resized = image::imageops::resize(&img, s, s, image::imageops::FilterType::Lanczos3);
        image::save_buffer(
            out_dir.join(format!("chatters-{s}.png")),
            &resized,
            s,
            s,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    }

    // ICO：32 / 128 / 256（Windows 大图标需要 256 帧）。
    let mut entries = Vec::new();
    for s in [32u32, 128u32, 256u32] {
        let frame = image::imageops::resize(&img, s, s, image::imageops::FilterType::Lanczos3);
        entries.push(ico::Entry::from_rgba(s, s, &frame));
    }
    ico::write_ico(out_dir.join("icon.ico").as_path(), &entries).unwrap();

    println!("icons written to {:?}", out_dir);
}

/// 在图像上绘制圆角矩形（越界自动忽略）。
fn fill_rounded(
    img: &mut image::RgbaImage,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    radius: f64,
    color: image::Rgba<u8>,
) {
    for y in (y0 as u32)..(y1 as u32) {
        for x in (x0 as u32)..(x1 as u32) {
            if rounded(x as f64 + 0.5, y as f64 + 0.5, x0, y0, x1, y1, radius) {
                put_checked(img, x as i32, y as i32, color);
            }
        }
    }
}

/// 写入像素，越界静默忽略。
fn put_checked(img: &mut image::RgbaImage, x: i32, y: i32, color: image::Rgba<u8>) {
    if x < 0 || y < 0 {
        return;
    }
    let (width, height) = img.dimensions();
    if (x as u32) < width && (y as u32) < height {
        img.put_pixel(x as u32, y as u32, color);
    }
}

fn rounded(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64, rad: f64) -> bool {
    let cx = px.clamp(x0 + rad, x1 - rad);
    let cy = py.clamp(y0 + rad, y1 - rad);
    let dx = px - cx;
    let dy = py - cy;
    // 拐角圆化：若在角区域则要求圆内，否则在矩形内
    if (px < x0 + rad || px > x1 - rad) && (py < y0 + rad || py > y1 - rad) {
        dx * dx + dy * dy <= rad * rad
    } else {
        px >= x0 && px <= x1 && py >= y0 && py <= y1
    }
}

fn draw_thick_line(
    img: &mut image::RgbaImage,
    (x0, y0): (f64, f64),
    (x1, y1): (f64, f64),
    width: f64,
    color: image::Rgba<u8>,
) {
    let steps = ((x1 - x0).abs().max((y1 - y0).abs()) as u32) * 2 + 1;
    for i in 0..steps {
        let t = i as f64 / steps as f64;
        let px = x0 + (x1 - x0) * t;
        let py = y0 + (y1 - y0) * t;
        let half = width / 2.0;
        for dy in -(half.ceil() as i32)..=(half.ceil() as i32) {
            for dx in -(half.ceil() as i32)..=(half.ceil() as i32) {
                let x = (px as i32 + dx) as u32;
                let y = (py as i32 + dy) as u32;
                if (dx as f64) * (dx as f64) + (dy as f64) * (dy as f64) <= half * half {
                    if x < img.width() && y < img.height() {
                        img.put_pixel(x, y, color);
                    }
                }
            }
        }
    }
}

mod ico {
    use std::fs;
    use std::path::Path;

    pub struct Entry {
        pub width: u32,
        pub height: u32,
        pub data: Vec<u8>,
    }

    impl Entry {
        pub fn from_rgba(w: u32, h: u32, img: &image::RgbaImage) -> Self {
            // ICO PNG 格式条目直接内嵌 PNG
            let mut png = Vec::new();
            {
                let encoder =
                    image::codecs::png::PngEncoder::new(&mut png);
                image::ImageEncoder::write_image(
                    encoder,
                    img.as_raw(),
                    w,
                    h,
                    image::ExtendedColorType::Rgba8,
                )
                .unwrap();
            }
            Entry { width: w, height: h, data: png }
        }
    }

    pub fn write_ico(path: &Path, entries: &[Entry]) -> std::io::Result<()> {
        let mut out = Vec::new();
        // ICONDIR
        out.extend(0u16.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend((entries.len() as u16).to_le_bytes());
        let mut offset = 6 + entries.len() * 16;
        for e in entries {
            let w = if e.width >= 256 { 0 } else { e.width as u8 };
            let h = if e.height >= 256 { 0 } else { e.height as u8 };
            out.extend([w, h]);
            out.extend([0u8, 0u8]); // color count, reserved
            out.extend(1u16.to_le_bytes()); // planes
            out.extend(32u16.to_le_bytes()); // bpp
            out.extend((e.data.len() as u32).to_le_bytes());
            out.extend((offset as u32).to_le_bytes());
            offset += e.data.len();
        }
        for e in entries {
            out.extend(&e.data);
        }
        fs::write(path, &out)
    }
}
