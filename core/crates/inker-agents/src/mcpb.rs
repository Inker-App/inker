//! The Claude Desktop Extension: a `.mcpb` bundle (a zip holding
//! `manifest.json` and the `inker` binary) that the Claude app installs on a
//! double-click or from Settings > Extensions > Advanced > Install Extension.
//!
//! The manifest follows the MCPB manifest format for a `binary` server. The
//! binary inside is the same stdio shim `inker mcp` that the config-file path
//! registers; it forwards to the running INKER app. For release the binary
//! must be signed with a Developer ID and notarized on a Mac before bundling.

use std::io::Write;

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Value, json};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::Result;

/// The MCPB manifest format version the manifest is written against.
pub const MANIFEST_VERSION: &str = "0.3";

/// Default for the private-folder picker. MCPB expands `${HOME}`.
pub const DEFAULT_PRIVATE_FOLDER: &str = "${HOME}/Private";

/// The binary's name inside the bundle.
pub const BINARY_NAME: &str = "inker";

/// Environment variable the manifest uses to hand the chosen private folder
/// to the shim.
pub const PRIVATE_FOLDER_ENV: &str = "INKER_PRIVATE_FOLDER";

pub const DESCRIPTION: &str = "Work on tax forms and other sensitive documents with Claude \
without Claude seeing the identifying details. INKER keeps the originals in a private folder \
on your Mac and gives Claude a copy where names, numbers and addresses are inked out as stable \
tokens like [SSN_1], with amounts, boxes and dates kept. Needs the INKER app running.";

const README: &str = "# INKER for Claude\n\n\
This extension connects the Claude app to INKER, the menu bar app that lets Claude work on \
sensitive documents without seeing the identifying details.\n\n\
1. Install and open INKER.\n\
2. Drop a document on the octopus in the menu bar and paste the handle it gives you \
(like `ink/w2-acme-2025`) into a chat.\n\
3. Claude reads the inked copy through INKER's tools. Real values stay on your Mac.\n\n\
The `inker` program in this bundle only forwards Claude's requests to the INKER app; the app \
holds your documents.\n";

/// Fixed entry timestamp, so two builds of the same inputs are identical.
const FIXED_MTIME: (u16, u8, u8) = (2026, 1, 1);

/// `manifest.json` for a bundle whose binary is `binary_name`.
pub fn manifest(version: &str, binary_name: &str, private_folder_default: &str) -> Value {
    json!({
        "manifest_version": MANIFEST_VERSION,
        "name": "inker",
        "display_name": "INKER",
        "version": version,
        "description": DESCRIPTION,
        "author": { "name": "INKER", "url": "https://inker.sh" },
        "homepage": "https://inker.sh",
        "server": {
            "type": "binary",
            "entry_point": binary_name,
            "mcp_config": {
                "command": format!("${{__dirname}}/{binary_name}"),
                "args": ["mcp"],
                "env": { PRIVATE_FOLDER_ENV: "${user_config.private_folder}" }
            }
        },
        "user_config": {
            "private_folder": {
                "type": "directory",
                "title": "Private folder",
                "description": "Where INKER keeps your original documents. Leave it as is unless \
    you chose a different folder in the INKER app.",
                "default": private_folder_default,
                "required": false
            }
        },
        "compatibility": { "platforms": ["darwin"] }
    })
}

/// Write the `.mcpb` bundle at `out_path` with `binary_path` as `inker`.
/// Written to a temporary file and renamed, so a failure leaves no bundle.
pub fn build_bundle(
    out_path: &Utf8Path,
    binary_path: &Utf8Path,
    version: &str,
) -> Result<Utf8PathBuf> {
    let binary = std::fs::read(binary_path)?;
    let manifest = manifest(version, BINARY_NAME, DEFAULT_PRIVATE_FOLDER);
    let mut manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    manifest_text.push('\n');

    let dir = out_path.parent().unwrap_or(Utf8Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = out_path.file_name().unwrap_or("bundle.mcpb");
    let tmp = dir.join(format!(".{name}.inker-{}", std::process::id()));
    let written = write_zip(&tmp, &manifest_text, &binary);
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, out_path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    tracing::info!(bundle = %out_path, "built");
    Ok(out_path.to_path_buf())
}

fn write_zip(path: &Utf8Path, manifest: &str, binary: &[u8]) -> Result<()> {
    let (y, m, d) = FIXED_MTIME;
    let mtime = DateTime::from_date_and_time(y, m, d, 0, 0, 0)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let opts = |mode: u32| {
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(mtime)
            .unix_permissions(mode)
    };
    let mut zip = ZipWriter::new(std::fs::File::create(path)?);
    zip.start_file("manifest.json", opts(0o644))?;
    zip.write_all(manifest.as_bytes())?;
    zip.start_file(BINARY_NAME, opts(0o755))?;
    zip.write_all(binary)?;
    zip.start_file("README.md", opts(0o644))?;
    zip.write_all(README.as_bytes())?;
    zip.finish()?.sync_all()?;
    Ok(())
}
