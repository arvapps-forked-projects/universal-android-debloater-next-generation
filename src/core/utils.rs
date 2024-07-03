use crate::core::sync::{hashset_system_packages, list_all_system_packages, User};
use crate::core::theme::Theme;
use crate::core::uad_lists::{PackageHashMap, PackageState, Removal, UadList};
use crate::gui::widgets::package_row::PackageRow;
use chrono::offset::Utc;
use chrono::{DateTime, Local};
use csv::Writer;
use std::path::PathBuf;
use std::process::Command;
use std::{fmt, fs};

/// Global environment variable to keep
/// track of the current device serial.
pub const ANDROID_SERIAL: &str = "ANDROID_SERIAL";
pub const EXPORT_FILE_NAME: &str = "selection_export.txt";
pub const UNINSTALLED_PACKAGES_FILE_NAME: &str = "uninstalled_packages";

#[derive(Debug, Clone)]
pub enum Error {
    DialogClosed,
}

pub fn fetch_packages(uad_lists: &PackageHashMap, user_id: Option<&User>) -> Vec<PackageRow> {
    let all_system_packages = list_all_system_packages(user_id); // installed and uninstalled packages
    let enabled_system_packages = hashset_system_packages(PackageState::Enabled, user_id);
    let disabled_system_packages = hashset_system_packages(PackageState::Disabled, user_id);
    let mut description;
    let mut uad_list;
    let mut state;
    let mut removal;
    let mut user_package: Vec<PackageRow> = Vec::new();

    for p_name in all_system_packages.lines() {
        state = PackageState::Uninstalled;
        description = "[No description]: CONTRIBUTION WELCOMED";
        uad_list = UadList::Unlisted;
        removal = Removal::Unlisted;

        if let Some(package) = uad_lists.get(p_name) {
            if !package.description.is_empty() {
                description = &package.description
            }
            uad_list = package.list;
            removal = package.removal;
        }

        if enabled_system_packages.contains(p_name) {
            state = PackageState::Enabled;
        } else if disabled_system_packages.contains(p_name) {
            state = PackageState::Disabled;
        }

        let package_row =
            PackageRow::new(p_name, state, description, uad_list, removal, false, false);
        user_package.push(package_row);
    }
    user_package.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    user_package
}

pub fn string_to_theme(theme: &str) -> Theme {
    match theme {
        "Dark" => Theme::Dark,
        "Light" => Theme::Light,
        "Lupin" => Theme::Lupin,
        _ => Theme::Dark,
    }
}

pub fn setup_uad_dir(dir: Option<PathBuf>) -> PathBuf {
    let dir = dir.unwrap().join("uad");
    fs::create_dir_all(&dir).expect("Can't create cache directory");
    dir
}

pub fn open_url(dir: PathBuf) {
    #[cfg(target_os = "windows")]
    let output = Command::new("explorer").args([dir]).output();

    #[cfg(target_os = "macos")]
    let output = Command::new("open").args([dir]).output();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let output = Command::new("xdg-open").args([dir]).output();

    match output {
        Ok(o) => {
            if !o.status.success() {
                let stderr = String::from_utf8(o.stderr).unwrap().trim_end().to_string();
                error!("Can't open the following URL: {}", stderr);
            }
        }
        Err(e) => error!("Failed to run command to open the file explorer: {}", e),
    }
}

#[rustfmt::skip]
#[allow(clippy::option_if_let_else)]
pub fn last_modified_date(file: PathBuf) -> DateTime<Utc> {
    fs::metadata(file).map_or_else(|_| Utc::now(), |metadata| match metadata.modified() {
        Ok(time) => time.into(),
        Err(_) => Utc::now(),
    })
}

pub fn format_diff_time_from_now(date: DateTime<Utc>) -> String {
    let now: DateTime<Utc> = Utc::now();
    let last_update = now - date;
    if last_update.num_days() == 0 {
        if last_update.num_hours() == 0 {
            last_update.num_minutes().to_string() + " min(s) ago"
        } else {
            last_update.num_hours().to_string() + " hour(s) ago"
        }
    } else {
        last_update.num_days().to_string() + " day(s) ago"
    }
}

/// Export selected packages.
/// File will be saved in same directory where UAD-ng is located.
pub async fn export_selection(packages: Vec<PackageRow>) -> Result<bool, String> {
    let selected = packages
        .iter()
        .filter(|p| p.selected)
        .map(|p| p.name.clone())
        .collect::<Vec<String>>()
        .join("\n");

    match fs::write(EXPORT_FILE_NAME, selected) {
        Ok(_) => Ok(true),
        Err(err) => Err(err.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayablePath {
    pub path: PathBuf,
}

impl fmt::Display for DisplayablePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stem = self.path.file_stem().map_or_else(
            || {
                error!("[PATH STEM]: No file stem found");
                "[File steam not found]".to_string()
            },
            |p| match p.to_os_string().into_string() {
                Ok(stem) => stem,
                Err(e) => {
                    error!("[PATH ENCODING]: {:?}", e);
                    "[PATH ENCODING ERROR]".to_string()
                }
            },
        );

        write!(f, "{stem}")
    }
}

/// Can be used to choose any folder.
pub async fn open_folder() -> Result<PathBuf, Error> {
    let picked_folder = rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .ok_or(Error::DialogClosed)?;

    Ok(picked_folder.path().to_owned())
}

/// Export uninstalled packages in a csv file.
/// Exported information will contain package name and description.
pub async fn export_packages(
    user: Option<User>,
    phone_packages: Vec<Vec<PackageRow>>,
) -> Result<bool, String> {
    let uninstalled_packages: Vec<&PackageRow> = phone_packages[user.unwrap().index]
        .iter()
        .filter(|p| p.state.to_string() == "Uninstalled")
        .collect();

    let backup_file = format!(
        "{}_{}.csv",
        UNINSTALLED_PACKAGES_FILE_NAME,
        Local::now().format("%Y%m%d")
    );

    let file = fs::File::create(backup_file).map_err(|err| err.to_string())?;
    let mut wtr = Writer::from_writer(file);

    wtr.write_record(["Package Name", "Description"])
        .map_err(|err| err.to_string())?;

    for package in uninstalled_packages {
        wtr.write_record([&package.name, &package.description.replace('\n', " ")])
            .map_err(|err| err.to_string())?;
    }

    wtr.flush().map_err(|err| err.to_string())?;

    Ok(true)
}
