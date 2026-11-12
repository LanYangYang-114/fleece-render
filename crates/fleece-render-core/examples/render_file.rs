use std::path::PathBuf;

use anyhow::{Context, Result};
use fleece_render_core::{
    BackendKind, ModelOverride, Parts, RenderEngine, RenderOptions, SkinAssets, Texture,
};

#[derive(Debug)]
struct Args {
    skin: PathBuf,
    output: PathBuf,
    parts: Parts,
    name: Option<String>,
    cape: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let skin_bytes = std::fs::read(&args.skin)
        .with_context(|| format!("failed to read {}", args.skin.display()))?;
    let skin = Texture::from_bytes(&skin_bytes, "skin")?.prepare_skin()?;
    let back = match &args.cape {
        Some(path) => {
            let bytes = std::fs::read(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            Some(Texture::from_bytes(&bytes, "back")?.prepare_back("back")?)
        }
        None => None,
    };
    let assets = SkinAssets {
        skin,
        back,
        display_name: args.name,
    };
    let options = RenderOptions {
        parts: args.parts,
        model: ModelOverride::AutoDetect,
        ..RenderOptions::default()
    };
    options.validate()?;
    let engine = RenderEngine::new(BackendKind::Auto)?;
    let result = engine.render(&assets, &options)?;
    let image = result.primary().context("renderer returned no image")?;
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&args.output, &image.bytes)?;
    println!(
        "wrote {} ({}x{}, model={}, backend={}, {:.1} ms)",
        args.output.display(),
        image.width,
        image.height,
        result.info.model,
        result.info.backend,
        result.info.elapsed_ms
    );
    Ok(())
}

fn parse_args() -> Result<Args> {
    let mut values = std::env::args().skip(1);
    let skin = PathBuf::from(values.next().context(
        "usage: cargo run -p fleece-render-core --example render_file -- <skin.png> <output.png> [front|back|both|all] [name] [cape.png]",
    )?);
    let output = PathBuf::from(values.next().context("missing output path")?);
    let parts = values
        .next()
        .unwrap_or_else(|| "both".to_owned())
        .parse::<Parts>()?;
    let name = values.next();
    let cape = values.next().map(PathBuf::from);
    Ok(Args {
        skin,
        output,
        parts,
        name,
        cape,
    })
}
