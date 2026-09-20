# Windows release gate

Production releases are built only from a clean `dev` commit on a disposable Windows build machine. The release command requires an HTTPS community endpoint, an Authenticode application certificate and the signed x64 driver package.

```powershell
$env:SLB_COMMUNITY_API_URL = "https://community.example.com"
$env:SLB_SIGN_CERT_SHA1 = "0123456789ABCDEF0123456789ABCDEF01234567"
npm run release:windows
```

The gate refuses missing or invalid driver signatures. The release MSI uses a stable upgrade code, bundles the driver package, creates or updates `SLB Virtual Microphone` during installation and removes it only on a real uninstall—not during an upgrade. User data under `%APPDATA%\fr.slb.soundboard` is outside the installation directory and is intentionally preserved.

The driver must pass Microsoft Hardware Lab Kit testing and be signed through the Hardware Developer Program before distribution. An attestation-signed testing package is not accepted as the retail release artifact. Keep certificates and Hardware Dashboard credentials outside the repository.

Run `scripts/test-installer-lifecycle.ps1` first without `-Execute` for structural validation, then with `-Execute` on a disposable elevated VM. Complete the Google OAuth and Cloudflare Tunnel staging checks as the final project validation, as previously agreed.
