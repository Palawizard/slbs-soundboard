use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use tauri::{AppHandle, Manager};

const HARDWARE_ID: &str = "ROOT\\SLBVIRTUALMICROPHONE";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverStatus {
    pub installed: bool,
    pub package_available: bool,
    pub requires_restart: bool,
}

pub fn status(app: &AppHandle) -> DriverStatus {
    DriverStatus {
        installed: installed(),
        package_available: package_root(app).join("SlbVirtualAudio.inf").is_file(),
        requires_restart: false,
    }
}

pub fn request_install(app: &AppHandle) -> Result<(), String> {
    let root = package_root(app);
    for file in [
        "SlbVirtualAudio.inf",
        "SlbVirtualAudio.sys",
        "SlbVirtualAudio.cat",
    ] {
        if !root.join(file).is_file() {
            return Err(
                "Le paquet signé du microphone virtuel n’est pas inclus dans cette version."
                    .to_owned(),
            );
        }
    }
    elevate("--driver-install", Some(&root))
}

pub fn request_remove() -> Result<(), String> {
    elevate("--driver-remove", None)
}

fn package_root(app: &AppHandle) -> PathBuf {
    let bundled = app.path().resource_dir().unwrap_or_default().join("driver");
    if bundled.join("SlbVirtualAudio.inf").is_file() {
        return bundled;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../native/driver/package/x64/Release")
}

fn elevate(action: &str, path: Option<&Path>) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|_| "L’exécutable de l’application est introuvable.".to_owned())?;
    let mut arguments = format!("'{}'", action.replace('\'', "''"));
    if let Some(path) = path {
        arguments.push_str(&format!(
            ",'{}'",
            path.to_string_lossy().replace('\'', "''")
        ));
    }
    let command = format!(
        "$process = Start-Process -FilePath '{}' -ArgumentList @({arguments}) -Verb RunAs -Wait -PassThru; exit $process.ExitCode",
        executable.to_string_lossy().replace('\'', "''")
    );
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .status()
        .map_err(|_| "L’élévation Windows n’a pas pu démarrer.".to_owned())?;
    if status.success() {
        Ok(())
    } else {
        Err("L’opération sur le microphone virtuel a été annulée ou a échoué.".to_owned())
    }
}

#[cfg(windows)]
fn installed() -> bool {
    Command::new("pnputil.exe")
        .args([
            "/enum-devices",
            "/instanceid",
            "ROOT\\SLBVIRTUALMICROPHONE\\0000",
        ])
        .output()
        .map(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .to_ascii_uppercase()
                    .contains(HARDWARE_ID)
        })
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn installed() -> bool {
    false
}

#[cfg(windows)]
pub fn run_cli_action(arguments: &[String]) -> Option<i32> {
    match arguments.get(1).map(String::as_str) {
        Some("--driver-install") => Some(
            arguments
                .get(2)
                .map(PathBuf::from)
                .ok_or(())
                .and_then(|path| install(&path).map_err(|_| ()))
                .map(|_| 0)
                .unwrap_or(1),
        ),
        Some("--driver-remove") => Some(remove().map(|_| 0).unwrap_or(1)),
        _ => None,
    }
}

#[cfg(not(windows))]
pub fn run_cli_action(_arguments: &[String]) -> Option<i32> {
    None
}

#[cfg(windows)]
fn install(package: &Path) -> Result<(), String> {
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{w, BOOL, GUID, PCWSTR};
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiCallClassInstaller, SetupDiCreateDeviceInfoList, SetupDiCreateDeviceInfoW,
        SetupDiDestroyDeviceInfoList, SetupDiSetDeviceRegistryPropertyW,
        UpdateDriverForPlugAndPlayDevicesW, DICD_GENERATE_ID, DIF_REGISTERDEVICE,
        INSTALLFLAG_FORCE, SPDRP_HARDWAREID, SP_DEVINFO_DATA,
    };

    let inf = package
        .join("SlbVirtualAudio.inf")
        .canonicalize()
        .map_err(|_| "INF absent".to_owned())?;
    if !installed() {
        let class_guid = GUID::from_u128(0x4d36e96c_e325_11ce_bfc1_08002be10318);
        let mut device = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        let info = unsafe { SetupDiCreateDeviceInfoList(Some(&class_guid), None) }
            .map_err(|error| error.to_string())?;
        let creation = (|| {
            unsafe {
                SetupDiCreateDeviceInfoW(
                    info,
                    w!("SLB Virtual Microphone"),
                    &class_guid,
                    PCWSTR::null(),
                    None,
                    DICD_GENERATE_ID,
                    Some(&mut device),
                )
            }
            .map_err(|error| error.to_string())?;
            let hardware: Vec<u16> = format!("{HARDWARE_ID}\0\0").encode_utf16().collect();
            let bytes = unsafe {
                std::slice::from_raw_parts(hardware.as_ptr().cast::<u8>(), hardware.len() * 2)
            };
            unsafe {
                SetupDiSetDeviceRegistryPropertyW(info, &mut device, SPDRP_HARDWAREID, Some(bytes))
            }
            .map_err(|error| error.to_string())?;
            unsafe { SetupDiCallClassInstaller(DIF_REGISTERDEVICE, info, Some(&device)) }
                .map_err(|error| error.to_string())
        })();
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(info);
        }
        creation?;
    }
    let inf: Vec<u16> = inf.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut reboot = BOOL::default();
    unsafe {
        UpdateDriverForPlugAndPlayDevicesW(
            None,
            w!("ROOT\\SLBVirtualMicrophone"),
            PCWSTR(inf.as_ptr()),
            INSTALLFLAG_FORCE,
            Some(&mut reboot),
        )
    }
    .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn remove() -> Result<(), String> {
    for index in 0..16 {
        let id = format!("ROOT\\SLBVIRTUALMICROPHONE\\{index:04}");
        let _ = Command::new("pnputil.exe")
            .args(["/remove-device", &id])
            .status();
    }
    let script = "$drivers = Get-WindowsDriver -Online | Where-Object { $_.OriginalFileName -like '*SlbVirtualAudio.inf' }; foreach ($driver in $drivers) { & pnputil.exe /delete-driver $driver.Driver /uninstall; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE } }";
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Driver removal failed".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_regular_application_arguments() {
        assert_eq!(run_cli_action(&["app.exe".to_owned()]), None);
    }
}
