param(
    [string]$InstallDir,
    [string]$Version = "latest",
    [switch]$AddToPath,
    [switch]$Quiet
)

$RepoOwner = "aryamanw"  
$RepoName  = "obsidian-mcp"
$Repo      = "$RepoOwner/$RepoName"

$ErrorActionPreference = "Stop"

function Write-Info  { if (-not $Quiet) { Write-Host "[INFO] $args" -ForegroundColor Cyan } }
function Write-Ok    { if (-not $Quiet) { Write-Host "[OK]   $args" -ForegroundColor Green } }
function Write-Warn  { if (-not $Quiet) { Write-Host "[WARN] $args" -ForegroundColor Yellow } }
function Write-Err   { Write-Host "[ERR]  $args" -ForegroundColor Red; exit 1 }

if (-not $InstallDir) {
    $InstallDir = Join-Path $env:USERPROFILE ".local\bin"
}

$BinaryName = "obsidian-mcp.exe"
$BinaryPath = Join-Path $InstallDir $BinaryName

if (Test-Path -LiteralPath $BinaryPath) {
    $existing = (Get-Item -LiteralPath $BinaryPath).VersionInfo.FileVersion
    Write-Info "Already installed at $BinaryPath (version: $existing)"
}

Write-Info "Detecting platform..."

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64"   { "x86_64-pc-windows-msvc" }
    "ARM64"   { "aarch64-pc-windows-msvc" }
    default   { Write-Err "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
}
Write-Info "Architecture: $arch"

if ($Version -eq "latest") {
    Write-Info "Fetching latest release..."
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $Version = $release.tag_name -replace '^v', ''
    Write-Info "Latest version: $Version"
}

$archiveName = "obsidian-mcp-$Version-$arch.zip"
$downloadUrl = "https://github.com/$Repo/releases/download/v$Version/$archiveName"
$downloadPath = Join-Path $env:TEMP $archiveName

Write-Info "Downloading $downloadUrl ..."
$progressPreference = 'silentlyContinue'
Invoke-WebRequest -Uri $downloadUrl -OutFile $downloadPath -UseBasicParsing
$progressPreference = 'continue'

Write-Info "Extracting..."
if (-not (Test-Path -LiteralPath $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

try {
    Expand-Archive -Path $downloadPath -DestinationPath $InstallDir -Force
} catch {
    Write-Err "Extraction failed: $_"
}

Remove-Item -LiteralPath $downloadPath -Force

if (-not (Test-Path -LiteralPath $BinaryPath)) {
    Write-Err "Binary not found after extraction at $BinaryPath"
}

if ($AddToPath -or (-not $Quiet)) {
    $userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($userPath -split ';' -notcontains $InstallDir) {
        if ($Quiet -or $AddToPath) {
            $addToPath = $true
        } else {
            $choice = Read-Host "Add $InstallDir to your PATH? (Y/n)"
            $addToPath = ($choice -ne 'n')
        }
        if ($addToPath) {
            $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
            [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
            $env:PATH = "$env:PATH;$InstallDir"
            Write-Ok "Added $InstallDir to PATH (restart terminals to apply)"
        }
    } else {
        Write-Info "$InstallDir already in PATH"
    }
}

Write-Ok "Installation complete!"
Write-Info ""
Write-Info "Next steps:"
Write-Info "  1. Set your vault path:"
Write-Info "     `$env:OBSIDIAN_VAULT = `"C:\path\to\your\vault`""
Write-Info ""
Write-Info "  2. Run the server:"
Write-Info "     obsidian-mcp.exe"
Write-Info ""
Write-Info "  3. Or configure your MCP client (Claude, VS Code, etc.) to use:"
Write-Info "     $BinaryPath"
