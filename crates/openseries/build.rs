//! Embed the generated app icon into the Windows executable.

#[path = "src/art.rs"]
#[allow(dead_code)]
mod art;

fn main() {
    println!("cargo:rerun-if-changed=src/art.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = out.join("openseries.ico");
    std::fs::write(&ico, build_ico(&[16, 20, 24, 32, 40, 48, 64, 128, 256])).unwrap();
    let rc = out.join("openseries.rc");
    std::fs::write(&rc, format!("1 ICON \"{}\"\n", ico.display().to_string().replace('\\', "\\\\"))).unwrap();
    embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().unwrap();
}

/// ICO with one PNG-compressed image per size.
fn build_ico(sizes: &[u32]) -> Vec<u8> {
    let pngs: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&s| {
            let mut buf = Vec::new();
            let mut enc = png::Encoder::new(&mut buf, s, s);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&art::logo(s)).unwrap();
            buf
        })
        .collect();
    let mut ico = vec![0, 0, 1, 0];
    ico.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (s, png) in sizes.iter().zip(&pngs) {
        let dim = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend([dim, dim, 0, 0, 1, 0, 32, 0]);
        ico.extend((png.len() as u32).to_le_bytes());
        ico.extend(offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in pngs {
        ico.extend(png);
    }
    ico
}
