//! 生成 Chatters 应用图标（512x512 PNG → Windows .ico 所需尺寸）。
//!
//! 图案：深蓝圆角背景 + 白色聊天气泡，中间一个绿色三角（P2P 传输意象）。
//! 使用 image crate 程序化绘制，无需外部素材。

use std::fs;
use std::path::Path;

fn main() {
    let size = 512u32;
    let mut img = image::RgbaImage::from_pixel(size, size, image::Rgba([0, 0, 0, 0]));

    let bg = image::Rgba([24, 60, 108, 255]); // deep blue
    let bubble = image::Rgba([255, 255, 255, 255]);
    let accent = image::Rgba([40, 200, 120, 255]); // green accent

    let cx = size as f64 / 2.0;
    let cy = size as f64 / 2.0;
    let r = size as f64 * 0.46;

    // 圆角背景（圆形简化）
    for (x, y, p) in img.enumerate_pixels_mut() {
        let dx = x as f64 - cx;
        let dy = y as f64 - cy;
        if dx * dx + dy * dy <= r * r {
            *p = bg;
        }
    }

    // 聊天气泡：圆角矩形主体（260x200，居中偏上）+ 左下尾巴
    let bx0 = 126.0; let by0 = 146.0; let bx1 = 386.0; let by1 = 346.0; let rad = 40.0;
    for y in (by0 as u32)..(by1 as u32) {
        for x in (bx0 as u32)..(bx1 as u32) {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let in_rx = rounded(px, py, bx0, by0, bx1, by1, rad);
            if in_rx { img.put_pixel(x, y, bubble); }
        }
    }
    // 尾巴：三角
    for t in 0..60u32 {
        let y = 346 + t;
        if y >= size { break; }
        let x_start = 150 + t / 2;
        for x in x_start..(x_start + 46 - t / 3) {
            if x < size { img.put_pixel(x, y, bubble); }
        }
    }

    // 绿色对勾（传输成功意象），粗线段
    draw_thick_line(&mut img, (170.0, 250.0), (232.0, 312.0), 26.0, accent);
    draw_thick_line(&mut img, (232.0, 312.0), (342.0, 192.0), 26.0, accent);

    let out_dir = Path::new("src-tauri/icons");
    fs::create_dir_all(out_dir).unwrap();

    // 512 主图
    image::save_buffer(
        out_dir.join("chatters-512.png"),
        &img,
        size,
        size,
        image::ExtendedColorType::Rgba8,
    )
    .unwrap();

    // 各尺寸缩放
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

    // ICO：32 与 128 两帧
    let ico32 = image::imageops::resize(&img, 32, 32, image::imageops::FilterType::Lanczos3);
    let ico128 = image::imageops::resize(&img, 128, 128, image::imageops::FilterType::Lanczos3);
    let entries = vec![ico::Entry::from_rgba(32, 32, &ico32), ico::Entry::from_rgba(128, 128, &ico128)];
    ico::write_ico(out_dir.join("icon.ico").as_path(), &entries).unwrap();

    println!("icons written to {:?}", out_dir);
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
