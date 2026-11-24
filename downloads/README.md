# Download Fossil

Download the latest release of **Fossil** directly from this page. Every link below always points to the newest published version.

Need an older release? Browse all versions on the [GitHub Releases](https://github.com/meghrazchi/fossil/releases) page.

---

## Desktop App Installers

Choose the installer for your computer:

| Operating System | Architecture / Format                  | Direct Download                                                                                                            |
| :--------------- | :------------------------------------- | :------------------------------------------------------------------------------------------------------------------------- |
| **macOS**        | Apple Silicon (M1 / M2 / M3 / M4)      | [Fossil-macos-arm64.dmg](https://github.com/meghrazchi/fossil/releases/latest/download/Fossil-macos-arm64.dmg)             |
| **macOS**        | Intel                                  | [Fossil-macos-x64.dmg](https://github.com/meghrazchi/fossil/releases/latest/download/Fossil-macos-x64.dmg)                 |
| **Windows**      | 64-bit Installer (`.exe`, recommended) | [Fossil-windows-x64-setup.exe](https://github.com/meghrazchi/fossil/releases/latest/download/Fossil-windows-x64-setup.exe) |
| **Windows**      | 64-bit Windows Installer (`.msi`)      | [Fossil-windows-x64.msi](https://github.com/meghrazchi/fossil/releases/latest/download/Fossil-windows-x64.msi)             |
| **Linux**        | Universal AppImage (`.AppImage`)       | [Fossil-linux-x64.AppImage](https://github.com/meghrazchi/fossil/releases/latest/download/Fossil-linux-x64.AppImage)       |
| **Linux**        | Debian / Ubuntu (`.deb`)               | [Fossil-linux-x64.deb](https://github.com/meghrazchi/fossil/releases/latest/download/Fossil-linux-x64.deb)                 |
| **Linux**        | Fedora / RHEL (`.rpm`)                 | [Fossil-linux-x64.rpm](https://github.com/meghrazchi/fossil/releases/latest/download/Fossil-linux-x64.rpm)                 |

### Quick Install Notes

- **macOS:** Open the `.dmg` file and drag **Fossil** into your **Applications** folder.
- **Windows:** Double-click `Fossil-windows-x64-setup.exe` (or `Fossil-windows-x64.msi`) and follow the setup steps.
- **Linux (AppImage):** Make the file executable with `chmod +x Fossil-linux-x64.AppImage`, then run `./Fossil-linux-x64.AppImage`.
- **Linux (`.deb` / `.rpm`):** Install with `sudo apt install ./Fossil-linux-x64.deb` on Debian/Ubuntu, or `sudo dnf install ./Fossil-linux-x64.rpm` on Fedora/RHEL.

---

## Standalone CLI & MCP Server Binaries

Prefer the terminal or want to connect Fossil to an AI assistant (like Claude Desktop, Cursor, or Antigravity) using MCP? Download the lightweight single-file binary below:

| Operating System | Architecture  | Direct Download                                                                                                              |
| :--------------- | :------------ | :--------------------------------------------------------------------------------------------------------------------------- |
| **macOS**        | Apple Silicon | [fossil-cli-macos-arm64.tar.gz](https://github.com/meghrazchi/fossil/releases/latest/download/fossil-cli-macos-arm64.tar.gz) |
| **macOS**        | Intel         | [fossil-cli-macos-x64.tar.gz](https://github.com/meghrazchi/fossil/releases/latest/download/fossil-cli-macos-x64.tar.gz)     |
| **Windows**      | 64-bit        | [fossil-cli-windows-x64.zip](https://github.com/meghrazchi/fossil/releases/latest/download/fossil-cli-windows-x64.zip)       |
| **Linux**        | 64-bit        | [fossil-cli-linux-x64.tar.gz](https://github.com/meghrazchi/fossil/releases/latest/download/fossil-cli-linux-x64.tar.gz)     |

---

## Verify Checksums (SHA256)

Every release includes verified SHA256 checksums so you can confirm your download was not altered or corrupted:

- Latest release checksums: [SHA256SUMS.txt](https://github.com/meghrazchi/fossil/releases/latest/download/SHA256SUMS.txt)
- Repository copy: [`./SHA256SUMS.txt`](./SHA256SUMS.txt)

Run the command for your system in the folder containing your downloaded file and compare the output with the matching line in `SHA256SUMS.txt`:

**macOS:**

```bash
shasum -a 256 Fossil-macos-arm64.dmg
```

**Linux:**

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing
```

**Windows (PowerShell):**

```powershell
Get-FileHash .\Fossil-windows-x64-setup.exe -Algorithm SHA256
```
