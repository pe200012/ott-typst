use std::path::PathBuf;

use clap::{Parser, Subcommand};
use miette::IntoDiagnostic;

#[derive(Parser)]
#[command(name = "ott", version, about = "Next-gen Ott (Rust + WASM + Typst)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and run basic semantic checks.
    Check {
        /// Input .ott file
        input: PathBuf,
    },

    /// Render the spec into a Typst-oriented document IR and print it as JSON.
    RenderJson {
        /// Input .ott file
        input: PathBuf,
    },

    /// Render the spec into a Typst-oriented document IR and write it as CBOR bytes.
    RenderCbor {
        /// Input .ott file
        input: PathBuf,

        /// Output file path
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check { input } => {
            let src = std::fs::read_to_string(&input).into_diagnostic()?;
            let spec = ott_core::parse_spec(&src).map_err(|e| miette::miette!(e.to_string()))?;
            let _checked = ott_core::check_spec(spec, &ott_core::OttOptions::default())
                .map_err(|e| miette::miette!(e.to_string()))?;
            println!("ok");
        }
        Command::RenderJson { input } => {
            let src = std::fs::read_to_string(&input).into_diagnostic()?;
            let spec = ott_core::parse_spec(&src).map_err(|e| miette::miette!(e.to_string()))?;
            let checked = ott_core::check_spec(spec, &ott_core::OttOptions::default())
                .map_err(|e| miette::miette!(e.to_string()))?;
            let doc = ott_render::render_for_typst(&checked);
            println!(
                "{}",
                serde_json::to_string_pretty(&doc).into_diagnostic()?
            );
        }
        Command::RenderCbor { input, output } => {
            let src = std::fs::read_to_string(&input).into_diagnostic()?;
            let spec = ott_core::parse_spec(&src).map_err(|e| miette::miette!(e.to_string()))?;
            let checked = ott_core::check_spec(spec, &ott_core::OttOptions::default())
                .map_err(|e| miette::miette!(e.to_string()))?;
            let doc = ott_render::render_for_typst(&checked);
            let bytes = ott_render::to_cbor_bytes(&doc)
                .map_err(|e| miette::miette!("CBOR encode failed: {e}"))?;
            std::fs::write(&output, bytes).into_diagnostic()?;
        }
    }

    Ok(())
}
