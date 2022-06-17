// Copyright 2022 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use super::{
  super::common::CommandExt,
  debian::{generate_desktop_file, generate_icon_files},
};
use crate::Settings;

use anyhow::Context;
use handlebars::Handlebars;
use log::info;
use serde::Serialize;

use std::{fs, path::PathBuf, process::Command};

#[derive(Serialize)]
struct ManifestMap {
  app_id: String,
  app_name: String,
  main_binary: String,
  binary: Vec<String>,
}

pub fn bundle_project(settings: &Settings) -> crate::Result<Vec<PathBuf>> {
  // TODO: Sidecar/external binaries? (These might be working already)
  // TODO: Don't hardcode source path (in the manifest)
  // TODO: Don't hardcode `src-tauri` (in the manifest)
  // TODO: metainfo file? (we should have most of the necessary data for a basic one)
  // TODO: Don't write desktop file and icons to `flatpak/usr`
  // TODO: Allow specifying extra permissions in config

  //
  // Step 1a: Generate the Flapak manifest file
  //

  // Location for build artifacts (Flatpak manifest and bundle)
  let output_dir = settings.project_out_directory().join("bundle/flatpak");
  // Location for build files (Flatpak repo, build directory, etc.)
  let build_dir = settings
    .project_out_directory()
    .join("bundle/flatpak_build");

  // Location of Flatpak manifest file
  let manifest_path = output_dir.join(format!("{}.json", settings.bundle_identifier()));
  // Location of flatpak-cargo-generator.py script
  let flatpak_cargo_generator_path = build_dir.join("flatpak-cargo-generator.py");
  // Location of Flatpak repository
  let flatpak_repository_dir = build_dir.join("repo");
  // Name of Flatpak single-file bundle
  let bundle_name = format!("{}.flatpak", settings.bundle_identifier());
  // Location of Flatpak single-file bundle
  let flatpak_bundle_path = output_dir.join(&bundle_name);

  // Start with clean build and output directories
  if output_dir.exists() {
    fs::remove_dir_all(&output_dir)?;
  }
  if build_dir.exists() {
    fs::remove_dir_all(&build_dir)?;
  }
  fs::create_dir_all(&output_dir)?;
  fs::create_dir_all(&build_dir)?;

  // Generate the desktop and icon files on the host
  // It's easier to do it here than in the sandbox
  generate_desktop_file(&settings, &output_dir)?;
  generate_icon_files(&settings, &output_dir)?;

  // Get the name of each binary that should be installed
  let mut binary_installs = Vec::new();
  for binary in settings.binaries() {
    binary_installs.push(binary.name().to_string());
  }

  for external_binary in settings.external_binaries() {
    dbg!(&external_binary);
  }

  // Create the map for the manifest template file
  let data = ManifestMap {
    app_id: settings.bundle_identifier().to_string(),
    app_name: settings.product_name().to_string(),
    main_binary: settings.main_binary_name().to_string(),
    binary: binary_installs,
  };

  // Render the manifest template
  let mut handlebars = Handlebars::new();
  handlebars
    .register_template_string("flatpak", include_str!("flatpak/manifest-template"))
    .expect("Failed to register template for handlebars");
  handlebars.set_strict_mode(true);
  let manifest = handlebars.render("flatpak", &data)?;

  info!(action = "Bundling"; "{} ({})", bundle_name, manifest_path.display());

  // Write the manifest
  fs::write(&manifest_path, manifest)?;

  //
  // Step 1b: Generate the Cargo sources file for the manifest
  //

  fs::write(
    &flatpak_cargo_generator_path,
    include_str!("flatpak/flatpak-cargo-generator.py"),
  )?;

  Command::new("python3")
    .arg(&flatpak_cargo_generator_path)
    .arg("-o")
    .arg(&output_dir.join("generated-sources.json"))
    .arg("Cargo.lock") // TODO: Use absolute path to `Cargo.lock`
    .output_ok()
    .context("failed to generate Cargo sources file for Flatpak manifest")?;

  //
  // Step 2: Build the Flatpak to a temporary repository
  //

  Command::new("flatpak-builder")
    .arg(format!(
      "--state-dir={}/.flatpak-builder",
      &build_dir.display()
    ))
    .arg(format!("--repo={}", &flatpak_repository_dir.display()))
    .arg(format!("{}/build", &build_dir.display()))
    .arg(&manifest_path)
    .output_ok()
    .context("failed to build Flatpak")?;

  //
  // Step 3: Export the Flatpak bundle from the temporary repository
  //

  Command::new("flatpak")
    .arg("build-bundle")
    .arg(&flatpak_repository_dir)
    .arg(&flatpak_bundle_path)
    .arg(&settings.bundle_identifier())
    .output_ok()
    .context("failed to export Flatpak bundle")?;

  Ok(vec![flatpak_bundle_path])
}
