#[cfg(target_os = "windows")]
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(target_os = "windows")]
const HOST_NAME: &str = "com.nununuuuu.conest";
#[cfg(target_os = "windows")]
const EXTENSION_ID: &str = "hdcbkkfljbmldjcgnfdmdjjkchekaggp";

#[cfg(target_os = "windows")]
fn registry_add(key: &str, manifest: &Path) -> anyhow::Result<()> {
    let status = Command::new("reg")
        .args([
            "add",
            key,
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            &manifest.to_string_lossy(),
            "/f",
        ])
        .status()?;

    anyhow::ensure!(status.success(), "reg add failed for {key}");
    Ok(())
}

#[cfg(target_os = "windows")]
fn local_manifest_path() -> anyhow::Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| anyhow::anyhow!("LOCALAPPDATA is unavailable"))?;
    Ok(PathBuf::from(local)
        .join("CONest")
        .join("NativeMessaging")
        .join(format!("{HOST_NAME}.json")))
}

#[cfg(target_os = "windows")]
pub fn register() -> anyhow::Result<()> {
    let app_exe = std::env::current_exe()?;
    let app_dir = app_exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("CONest executable has no parent directory"))?;

    // Portable uses the unsuffixed filename. Tauri build/install can retain
    // the target-triple filename depending on where the executable is launched.
    let host_exe = [
        app_dir.join("conest-native-host.exe"),
        app_dir.join("conest-native-host-x86_64-pc-windows-msvc.exe"),
        app_dir.join("resources").join("conest-native-host.exe"),
        app_dir
            .join("resources")
            .join("conest-native-host-x86_64-pc-windows-msvc.exe"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .ok_or_else(|| anyhow::anyhow!(
        "Native Messaging host is missing beside {}",
        app_exe.display()
    ))?;

    let manifest_path = local_manifest_path()?;
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let manifest = serde_json::json!({
        "name": HOST_NAME,
        "description": "CONest Native Messaging Host",
        "path": host_exe.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [
            format!("chrome-extension://{EXTENSION_ID}/"),
            "chrome-extension://aifecpakbelclcbccfaookeanfcmif/"
        ]
    });

    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;

    // Chromium browsers use vendor-specific registry roots on Windows.
    // Register the same host manifest for the common browsers so an unpacked
    // CONest extension can connect regardless of which supported Chromium
    // browser is being used for testing.
    let registry_keys = [
        r"HKCU\Software\Google\Chrome\NativeMessagingHosts\com.nununuuuu.conest",
        r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.nununuuuu.conest",
        r"HKCU\Software\Chromium\NativeMessagingHosts\com.nununuuuu.conest",
        r"HKCU\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.nununuuuu.conest",
        r"HKCU\Software\Vivaldi\NativeMessagingHosts\com.nununuuuu.conest",
    ];

    let mut registered = 0usize;
    let mut last_error = None;
    for key in registry_keys {
        match registry_add(key, &manifest_path) {
            Ok(()) => registered += 1,
            Err(error) => {
                log::warn!("Native Messaging registry write failed for {key}: {error:#}");
                last_error = Some(error);
            }
        }
    }

    anyhow::ensure!(
        registered > 0,
        "Failed to register Native Messaging in any browser registry root: {}",
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown error".into())
    );

    log::info!(
        "Native Messaging registered in {registered} browser roots: {} -> {}",
        manifest_path.display(),
        host_exe.display()
    );
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn register() -> anyhow::Result<()> {
    Ok(())
}
