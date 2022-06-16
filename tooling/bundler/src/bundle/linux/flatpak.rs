// Copyright 2022 Owen D'Aprile
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use super::super::common::CommandExt;
use crate::Settings;

use anyhow::Context;
use handlebars::Handlebars;
use log::info;

use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

pub fn bundle_project(settings: &Settings) -> crate::Result<Vec<PathBuf>> {
  //
  // Part 1a: Generate the Flatpak manifest
  //

  // Location for build artifacts (Flatpak manifest and bundle)
  let output_dir = settings.project_out_directory().join("bundle/flatpak");
  // Location for build files (Flatpak repo, build directory, etc.)
  let build_dir = settings
    .project_out_directory()
    .join("bundle/flatpak_build");

  let manifest_path = output_dir.join(format!("{}.json", settings.bundle_identifier()));
  let flatpak_cargo_generator_path = build_dir.join("flatpak-cargo-generator.py");
  let flatpak_repository_dir = build_dir.join("repo");
  let bundle_name = format!("{}.flatpak", settings.bundle_identifier());
  let flatpak_bundle_path = output_dir.join(&bundle_name);

  if output_dir.exists() {
    fs::remove_dir_all(&output_dir)?;
  }
  if build_dir.exists() {
    fs::remove_dir_all(&build_dir)?;
  }

  fs::create_dir_all(&output_dir)?;
  fs::create_dir_all(&build_dir)?;

  let mut manifest_map = BTreeMap::new();
  manifest_map.insert("app_id", settings.bundle_identifier());
  manifest_map.insert("app_name", settings.product_name());

  let mut handlebars = Handlebars::new();
  handlebars
    .register_template_string("flatpak", include_str!("flatpak/manifest-template.json"))
    .expect("Failed to register template for handlebars");
  let manifest = handlebars.render("flatpak", &manifest_map)?;

  info!(action = "Bundling"; "{} ({})", bundle_name, manifest_path.display());

  // Write the manifest
  fs::write(&manifest_path, manifest)?;

  //
  // Part 1b: Generate the cargo sources for the manifest
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
  // Part 2: Build the Flatpak
  //

  Command::new("flatpak-builder")
    .arg(format!("--repo={}", &flatpak_repository_dir.display()))
    .arg(format!("{}/build", &build_dir.display()))
    .arg(&manifest_path)
    .output_ok()
    .context("failed to build Flatpak")?;

  //
  // Part 3: Export the single-file bundle
  //

  Command::new("flatpak")
    .arg("build-bundle")
    .arg(&flatpak_repository_dir)
    .arg(&flatpak_bundle_path)
    .arg(&settings.bundle_identifier())
    .output_ok()
    .context("failed to export Flatpak bundle")?;

  Ok(vec![flatpak_bundle_path, manifest_path])
}
