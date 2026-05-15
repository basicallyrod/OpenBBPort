//! Windows autostart via a `.lnk` shortcut in
//! `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\`.
//!
//! Creates the shortcut via COM `IShellLinkW` so the file is a real
//! Windows shortcut rather than a renamed copy of the executable.

use std::path::PathBuf;
use tauri::AppHandle;

fn package_name(app: &AppHandle) -> String {
    app.package_info().name.clone()
}

fn startup_dir() -> Option<PathBuf> {
    dirs::data_dir()
        .map(|p| p.join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("Startup"))
}

fn shortcut_path(app: &AppHandle) -> Option<PathBuf> {
    startup_dir().map(|d| d.join(format!("{}.lnk", package_name(app))))
}

pub fn is_enabled(app: &AppHandle) -> Result<bool, String> {
    Ok(shortcut_path(app).map(|p| p.exists()).unwrap_or(false))
}

pub fn disable(app: &AppHandle) -> Result<(), String> {
    if let Some(p) = shortcut_path(app) {
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn enable(_app: &AppHandle) -> Result<(), String> {
    Err("not on windows".into())
}

#[cfg(target_os = "windows")]
pub fn enable(app: &AppHandle) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::shared::guiddef::GUID;
    use winapi::shared::winerror::SUCCEEDED;
    use winapi::um::combaseapi::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    };
    use winapi::um::objbase::COINIT_APARTMENTTHREADED;
    use winapi::um::shobjidl_core::{IPersistFile, IShellLinkW};

    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    let lnk = shortcut_path(app).ok_or("no startup dir")?;
    if let Some(parent) = lnk.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // CLSID_ShellLink = {00021401-0000-0000-C000-000000000046}
    let clsid_shell_link = GUID {
        Data1: 0x00021401,
        Data2: 0x0000,
        Data3: 0x0000,
        Data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };
    // IID_IShellLinkW = {000214F9-0000-0000-C000-000000000046}
    let iid_shell_link_w = GUID {
        Data1: 0x000214F9,
        Data2: 0x0000,
        Data3: 0x0000,
        Data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };
    // IID_IPersistFile = {0000010B-0000-0000-C000-000000000046}
    let iid_persist_file = GUID {
        Data1: 0x0000010B,
        Data2: 0x0000,
        Data3: 0x0000,
        Data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };

    unsafe {
        let hr = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        if !SUCCEEDED(hr) {
            return Err(format!("CoInitializeEx failed: 0x{hr:08x}"));
        }

        let mut shell_link: *mut IShellLinkW = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &clsid_shell_link,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &iid_shell_link_w,
            &mut shell_link as *mut _ as *mut *mut _,
        );
        if !SUCCEEDED(hr) {
            CoUninitialize();
            return Err(format!("CoCreateInstance failed: 0x{hr:08x}"));
        }

        let target_w: Vec<u16> = target
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let hr = ((*(*shell_link).lpVtbl).SetPath)(shell_link, target_w.as_ptr());
        if !SUCCEEDED(hr) {
            ((*(*shell_link).lpVtbl).parent.Release)(shell_link as *mut _);
            CoUninitialize();
            return Err(format!("SetPath failed: 0x{hr:08x}"));
        }

        // Query IPersistFile
        let mut persist_file: *mut IPersistFile = std::ptr::null_mut();
        let hr = ((*(*shell_link).lpVtbl).parent.QueryInterface)(
            shell_link as *mut _,
            &iid_persist_file,
            &mut persist_file as *mut _ as *mut *mut _,
        );
        if !SUCCEEDED(hr) {
            ((*(*shell_link).lpVtbl).parent.Release)(shell_link as *mut _);
            CoUninitialize();
            return Err(format!("QueryInterface IPersistFile failed: 0x{hr:08x}"));
        }

        let lnk_w: Vec<u16> = lnk
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let hr = ((*(*persist_file).lpVtbl).Save)(persist_file, lnk_w.as_ptr(), 1);

        ((*(*persist_file).lpVtbl).parent.Release)(persist_file as *mut _);
        ((*(*shell_link).lpVtbl).parent.Release)(shell_link as *mut _);
        CoUninitialize();

        if !SUCCEEDED(hr) {
            return Err(format!("IPersistFile::Save failed: 0x{hr:08x}"));
        }
    }

    Ok(())
}
