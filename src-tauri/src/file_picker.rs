use std::path::PathBuf;

#[cfg(not(target_os = "windows"))]
use std::process::Command;

pub fn pick_rest_overlay_image() -> Result<Option<PathBuf>, String> {
    pick_file(
        "Select rest screen image",
        FileFilter::new(
            "Image files",
            &[
                "*.png", "*.jpg", "*.jpeg", "*.webp", "*.gif", "*.bmp", "*.svg",
            ],
        ),
    )
}

pub fn pick_rest_overlay_html() -> Result<Option<PathBuf>, String> {
    pick_file(
        "Select rest screen HTML",
        FileFilter::new("HTML files", &["*.html", "*.htm"]),
    )
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct FileFilter {
    name: &'static str,
    #[cfg(not(target_os = "windows"))]
    unix_patterns: &'static [&'static str],
}

impl FileFilter {
    fn new(
        name: &'static str,
        #[allow(unused_variables)] unix_patterns: &'static [&'static str],
    ) -> Self {
        Self {
            name,
            #[cfg(not(target_os = "windows"))]
            unix_patterns,
        }
    }
}

#[cfg(target_os = "windows")]
fn pick_file(title: &'static str, filter: FileFilter) -> Result<Option<PathBuf>, String> {
    std::thread::spawn(move || pick_file_sta(title, filter))
        .join()
        .map_err(|_| "File picker closed unexpectedly.".to_string())?
}

#[cfg(target_os = "windows")]
fn pick_file_sta(title: &str, _filter: FileFilter) -> Result<Option<PathBuf>, String> {
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::{
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
            },
            UI::Shell::{
                FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
                FOS_PATHMUSTEXIST, SIGDN_FILESYSPATH,
            },
        },
    };

    const HRESULT_ERROR_CANCELLED: i32 = 0x800704C7u32 as i32;

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|error| format!("Failed to initialize file picker: {error}"))?;

        let result = (|| {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(|error| format!("Failed to open file picker: {error}"))?;
            let title = wide_null(title);

            dialog
                .SetTitle(PCWSTR::from_raw(title.as_ptr()))
                .map_err(|error| format!("Failed to prepare file picker: {error}"))?;
            dialog
                .SetOptions(FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
                .map_err(|error| format!("Failed to prepare file picker: {error}"))?;

            if let Err(error) = dialog.Show(None) {
                if error.code().0 == HRESULT_ERROR_CANCELLED {
                    return Ok(None);
                }

                return Err(format!("Failed to show file picker: {error}"));
            }

            let item = dialog
                .GetResult()
                .map_err(|error| format!("Failed to read selected file: {error}"))?;
            let path: PWSTR = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|error| format!("Failed to read selected file: {error}"))?;
            let selected = path.to_string();

            CoTaskMemFree(Some(path.as_ptr() as *const core::ffi::c_void));

            let selected = selected
                .map_err(|error| format!("Selected file path is not valid Unicode: {error}"))?;

            Ok(Some(PathBuf::from(selected)))
        })();

        CoUninitialize();
        result
    }
}

#[cfg(target_os = "windows")]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "macos")]
fn pick_file(title: &'static str, _filter: FileFilter) -> Result<Option<PathBuf>, String> {
    let output = Command::new("osascript")
        .args([
            "-e",
            &format!("POSIX path of (choose file with prompt \"{}\")", title),
        ])
        .output()
        .map_err(|error| format!("Failed to open file picker: {error}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    selected_path_from_output(&output.stdout)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn pick_file(title: &'static str, filter: FileFilter) -> Result<Option<PathBuf>, String> {
    let mut command = Command::new("zenity");
    command
        .arg("--file-selection")
        .arg(format!("--title={}", title))
        .arg(format!(
            "--file-filter={} | {}",
            filter.name,
            filter.unix_patterns.join(" ")
        ));
    let output = command
        .output()
        .map_err(|error| format!("Failed to open file picker: {error}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    selected_path_from_output(&output.stdout)
}

#[cfg(not(target_os = "windows"))]
fn selected_path_from_output(stdout: &[u8]) -> Result<Option<PathBuf>, String> {
    let selected = String::from_utf8_lossy(stdout).trim().to_string();
    if selected.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(selected)))
    }
}
