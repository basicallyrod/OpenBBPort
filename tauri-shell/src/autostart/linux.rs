//! Linux XDG autostart via `~/.config/autostart/<package>.desktop`.

use std::path::PathBuf;
use tauri::AppHandle;

fn package_name(app: &AppHandle) -> String {
    app.package_info().name.clone()
}

fn desktop_path(app: &AppHandle) -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("autostart").join(format!("{}.desktop", package_name(app))))
}

pub fn is_enabled(app: &AppHandle) -> Result<bool, String> {
    Ok(desktop_path(app).map(|p| p.exists()).unwrap_or(false))
}

pub fn enable(app: &AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .into_owned();
    let name = package_name(app);
    let path = desktop_path(app).ok_or("no config dir")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Exec=\"{exe}\"\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    );
    std::fs::write(&path, content).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn disable(app: &AppHandle) -> Result<(), String> {
    if let Some(p) = desktop_path(app) {
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
