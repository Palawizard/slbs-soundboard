# SLB Virtual Microphone driver

This directory contains the Windows capture driver used by SLB's Soundboard. It exposes one software microphone endpoint at 48 kHz. The implementation is derived from Microsoft's SysVAD sample at commit `2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89`; the imported files remain covered by the Microsoft Public License in `sysvad/MICROSOFT-LICENSE.txt`.

## Requirements

- Visual Studio 2022 Build Tools with the Windows Driver Kit component
- WDK 10.0.26100 or newer
- MSVC Spectre-mitigated x64 libraries
- Administrator rights only for install and uninstall

## Build and package

Run `scripts/build-driver.ps1 -Configuration Release`. The package is written to `native/driver/package/x64/Release`. Debug builds are test-signed by the WDK. Release distribution still requires Microsoft Hardware Dev Center attestation or WHQL signing.

## Install and uninstall

Use `scripts/install-driver.ps1` from an elevated PowerShell session. Debug installation trusts the package's local WDK test certificate and requires Windows test-signing mode to have been configured separately. The script refuses unsigned release packages. Use `scripts/uninstall-driver.ps1` to remove both the root device and its driver-store package.

The scripts do not modify Secure Boot, BitLocker, or Windows test-signing settings.
