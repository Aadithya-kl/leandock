# LeanDock Windows Installer (PowerShell)
$ErrorActionPreference = "Stop"

$Repo = if ($env:LEANDOCK_REPO) { $env:LEANDOCK_REPO } else { "pvbang/leandock" }
$InstallDir = if ($env:LEANDOCK_INSTALL_DIR) { $env:LEANDOCK_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "LeanDock\bin" }
$Version = if ($env:LEANDOCK_VERSION) { $env:LEANDOCK_VERSION } else { "latest" }

if ($Version -eq "latest") {
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $Tag = $Release.tag_name
} else {
    $Tag = $Version
}

$Target = "x86_64-pc-windows-msvc"
$Archive = "leandock-$Tag-$Target.zip"
$BaseUrl = "https://github.com/$Repo/releases/download/$Tag"

$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("leandock-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $TmpDir | Out-Null

try {
    $ZipPath = Join-Path $TmpDir $Archive
    $ShaPath = Join-Path $TmpDir "$Archive.sha256"

    Write-Host "Downloading LeanDock $Tag ($Target)..."
    Invoke-WebRequest -Uri "$BaseUrl/$Archive" -OutFile $ZipPath
    Invoke-WebRequest -Uri "$BaseUrl/$Archive.sha256" -OutFile $ShaPath

    Write-Host "Verifying SHA256 checksum..."
    $ExpectedHash = (Get-Content $ShaPath).Split(" ")[0].Trim().ToUpper()
    $ActualHash = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToUpper()
    if ($ExpectedHash -ne $ActualHash) {
        throw "SHA256 checksum mismatch! Expected $ExpectedHash, got $ActualHash."
    }

    Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path (Join-Path $TmpDir "leandock.exe") -Destination (Join-Path $InstallDir "leandock.exe") -Force

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
        Write-Host "Added $InstallDir to your user PATH. Open a new terminal to use `leandock`."
    }

    Write-Host "Installed LeanDock $Tag to $InstallDir\leandock.exe"
    & (Join-Path $InstallDir "leandock.exe") --version
} finally {
    Remove-Item -Path $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
