use crate::{
    app_state::{AppStateManager, Progress},
    github::ReleaseAndTag,
    t, Result, SDCARD_ROOT,
};
use bytes::Bytes;
use fetching::{download, fetch_latest_release, fetch_releases, fetch_tags};
use regex::Regex;

use std::{
    fs::File,
    io::{Cursor, Read, Write},
    path::PathBuf,
    process::exit,
    thread,
};

mod fetching;

fn extract_zip<T: Fn(&str) -> bool>(
    bytes: Bytes,
    filter: T,
    progress_cb: impl Fn(f32),
) -> Result<()> {
    pub fn file_write_all_bytes(path: &PathBuf, bytes: &[u8]) -> Result<usize> {
        let mut file = File::create(path)?;
        file.set_len(0)?;
        Ok(file.write(bytes)?)
    }

    // Extract the update package
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let target_directory = PathBuf::from(SDCARD_ROOT);
    let archive_len = archive.len();

    for file_number in 0..archive_len {
        let mut next = archive.by_index(file_number)?;

        let sanitized_name = next.mangled_name();

        if !filter(sanitized_name.as_os_str().to_string_lossy().as_ref()) {
            println!("Skipping file: {}", sanitized_name.display());
            continue;
        }

        if next.is_dir() {
            let extracted_folder_path = target_directory.join(sanitized_name);
            std::fs::create_dir_all(&extracted_folder_path)?;
            println!("Created directory: {}", extracted_folder_path.display());
        } else if next.is_file() {
            let mut buffer: Vec<u8> = Vec::new();
            let _bytes_read = next.read_to_end(&mut buffer)?;
            let extracted_file_path = target_directory.join(sanitized_name);
            file_write_all_bytes(&extracted_file_path, buffer.as_ref())?;
            println!("Extracted file: {}", extracted_file_path.display());
        }

        progress_cb(file_number as f32 / (archive_len - 1) as f32);
    }

    Ok(())
}

pub fn self_update(app_state: &AppStateManager) -> Result<()> {
    // Fetch latest release information
    app_state.start_operation(&t!("op.fetch_updater"));

    println!("Fetching latest updater release...");

    let release = fetch_latest_release("LoveRetro/nextui-updater-pak")?;

    println!("Latest updater release: {release:?}");

    let available = semver::Version::parse(&release.tag_name)?;
    let installed = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;

    if available > installed {
        println!("New version available: {available} (current: {installed})");
        app_state.set_current_operation(Some(t!("op.download_updater")));
    } else {
        println!("No updates available");
        return Ok(());
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with(".pakz"))
        .ok_or_else(|| t!("err.no_pakz_asset"))?;

    let bytes = download(&asset.url, |pr| {
        app_state.update_progress(pr);
    })?;

    app_state.set_current_operation(
        Some(t!("op.extract_updater").replace("{tag}", &release.tag_name)),
    );
    app_state.set_progress(Some(Progress::Indeterminate));

    // Move the current binary to a backup location
    let current_binary = std::env::current_exe()?;
    std::fs::rename(&current_binary, current_binary.with_extension("bak"))?;

    // Extract the update package
    let result = extract_zip(
        bytes,
        |_| true,
        |pr| {
            app_state.update_progress(pr);
        },
    );

    println!("Extraction complete!");
    app_state.set_progress(Some(Progress::Indeterminate));

    if result.is_err() {
        // Move the backup back
        std::fs::rename(current_binary.with_extension("bak"), current_binary)?;

        return Err(t!("err.extract_failed").into());
    }

    app_state.set_current_operation(Some(t!("op.self_update_done")));

    // Give the user a moment to see the completion message
    thread::sleep(std::time::Duration::from_secs(1));

    // "5" is the exit code for "restart required"
    exit(5);
}

pub fn do_nextui_release_check(app_state: &AppStateManager) {
    // Fetch latest release information
    app_state.start_operation(&t!("op.fetch_release"));
    let repo = "LoveRetro/NextUI";

    // Fetch latest releases information
    app_state.start_operation(&t!("op.fetch_releases"));
    let latest_releases = match fetch_releases(repo) {
        Ok(releases) => releases,
        Err(err) => {
            // Failed connection
            println!("Fetching releases failed: {err:?}");
            app_state.set_operation_failed(
                &t!("err.fetch_releases_failed").replace("{err}", &err.to_string()),
            );
            return;
        }
    };
    if latest_releases.is_empty() {
        // Connected, but no results
        println!("Fetching releases returned 0 releases");
        app_state.set_operation_failed(&t!("err.fetch_releases_zero"));
        return;
    }

    // Fetch latest tag information
    app_state.start_operation(&t!("op.fetch_tags"));
    let mut latest_tags = match fetch_tags(repo) {
        Ok(tags) => tags,
        Err(err) => {
            // Failed connection
            println!("Fetching tags failed: {err:?}");
            app_state.set_operation_failed(
                &t!("err.fetch_tags_failed").replace("{err}", &err.to_string()),
            );
            return;
        }
    };
    if latest_tags.is_empty() {
        // Connected, but no results
        println!("Fetching tags returned 0 tags");
        app_state.set_operation_failed(&t!("err.fetch_tags_zero"));
        return;
    }

    // Build ReleaseAndTag list for app state
    let mut releases_and_tags: Vec<ReleaseAndTag> = vec![];
    let mut check_latest_release = true;
    let current_tag = app_state.current_version().unwrap_or_default();
    let mut current_tag_found = false;
    for release in &latest_releases {
        if let Some(tag_index) = latest_tags
            .iter()
            .position(|tag| tag.name == release.tag_name)
        {
            releases_and_tags.push(ReleaseAndTag {
                release: (release.clone()),
                tag: (latest_tags[tag_index].clone()),
            });
            if latest_tags[tag_index].commit.sha.starts_with(&current_tag) {
                // set release selector starting index
                app_state.set_nextui_releases_and_tags_index(Some(releases_and_tags.len() - 1));
                current_tag_found = true;
            }
            latest_tags.remove(tag_index);
            if check_latest_release {
                check_latest_release = false;
            }
            continue;
        }
        if check_latest_release {
            // Failed to find a match for the first release
            println!("Latest release has no matching tag: {:?}", release.tag_name);
            app_state.set_operation_failed(
                &t!("err.no_matching_tag").replace("{tag}", &release.tag_name),
            );
            return;
        }
    }

    // Save collected values to app state
    app_state.set_nextui_release(Some(releases_and_tags[0].release.clone()));
    app_state.set_nextui_tag(Some(releases_and_tags[0].tag.clone()));
    app_state.set_nextui_releases_and_tags(Some(releases_and_tags));
    if !current_tag_found {
        app_state.set_nextui_releases_and_tags_index(Some(0_usize));
    }

    app_state.finish_operation();
}

pub fn do_self_update(app_state: &AppStateManager) {
    // Do self-update
    let result = self_update(app_state);
    match result {
        Ok(()) => {
            app_state.finish_operation();
        }
        Err(err) => {
            println!("Update failed: {err:?}");
            app_state.set_operation_failed(
                &t!("err.self_update_failed").replace("{err}", &err.to_string()),
            );
        }
    }
}

pub fn do_update(app_state: &'static AppStateManager, full: bool) {
    thread::spawn(move || {
        if let Err(err) = update_nextui(app_state, full) {
            println!("Update failed: {:?}", err.source());

            app_state.set_operation_failed(
                &t!("err.update_failed").replace("{err}", &err.to_string()),
            );

            // Try to fetch latest release information again
            do_nextui_release_check(app_state);
        }
    });
}

pub fn update_nextui(app_state: &AppStateManager, full: bool) -> Result<()> {
    let mut release = {
        app_state.start_operation(&t!("op.download_update"));

        app_state
            .nextui_release()
            .clone()
            .ok_or_else(|| t!("err.no_release"))?
    };
    if app_state.release_selection_menu() {
        let index = app_state.nextui_releases_and_tags_index().unwrap_or(0);
        let relase_and_tag_vector = app_state.nextui_releases_and_tags().unwrap_or_default();
        release = relase_and_tag_vector[index].release.clone();
    }

    let assets = release.assets;
    let asset = assets
        .iter()
        .find(|a| a.name.contains(if full { "all" } else { "base" }))
        .or(assets.first())
        .ok_or_else(|| t!("err.no_assets"))?;

    // Download the asset
    app_state.start_determinate_operation(
        &t!("op.download_asset").replace("{asset}", &asset.name),
    );
    println!("Downloading from {}", asset.url);

    let bytes = download(&asset.url, |pr| app_state.update_progress(pr))?;

    app_state.set_current_operation(
        Some(t!("op.extract_archive").replace("{asset}", &asset.name)),
    );
    app_state.set_progress(Some(Progress::Indeterminate));

    // Extract the update package
    if full {
        let emu_tag_re = Regex::new(r"\((?<emu>\w+)\)").expect("Failed to compile regex");
        // Full update, extract all files, except for Roms folders which already exist
        extract_zip(
            bytes,
            |file| {
                if file.starts_with("Roms/") {
                    // Extract the emu tag from the folder name
                    if let Some(captures) = emu_tag_re.captures(file) {
                        if let Some(emu) = captures.name("emu").map(|c| c.as_str()) {
                            // Check if the emu tag already exists in the roms folder
                            if std::fs::read_dir(PathBuf::from(SDCARD_ROOT).join("Roms")).is_ok_and(
                                |d| {
                                    d.filter_map(std::result::Result::ok).any(|e| {
                                        e.file_name()
                                            .to_string_lossy()
                                            .contains(format!("({emu})").as_str())
                                    })
                                },
                            ) {
                                println!("Roms folder for {emu} already exists, skipping");
                                return false;
                            }
                        }
                    }
                }

                true
            },
            |pr| app_state.update_progress(pr),
        )?;
    } else {
        // "Quick" update, just extract MinUI.zip and trimui folder
        extract_zip(
            bytes,
            |file| {
                ["MinUI.zip", "trimui"]
                    .iter()
                    .any(|prefix| file.starts_with(prefix))
            },
            |pr| app_state.update_progress(pr),
        )?;
    }

    println!("Extraction complete!");
    app_state.set_progress(Some(Progress::Indeterminate));

    app_state.set_current_operation(Some(t!("op.update_complete")));

    // Give the user a moment to see the completion message
    thread::sleep(std::time::Duration::from_secs(2));

    app_state.set_current_operation(Some(t!("op.rebooting")));

    // Reboot the system
    match std::process::Command::new("reboot").output() {
        Ok(_) => Ok(()),
        Err(e) => Err(Box::new(e)),
    }
}
