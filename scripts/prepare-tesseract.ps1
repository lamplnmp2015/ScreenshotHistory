<#
.SYNOPSIS
  Stage the bundled Tesseract OCR engine + Chinese/English models that the
  installer ships, into src-tauri/tesseract/.

.DESCRIPTION
  The Tesseract binaries (~123 MB) are intentionally NOT committed to git.
  Run this once before `npm run tauri:build` to (re)create the bundle so the
  produced MSI/NSIS installer carries OCR that works out of the box — end users
  don't need to install Tesseract themselves.

  Steps:
    1. Locate a Tesseract 5.x install (Program Files / LocalAppData). If none
       is found, install it via winget (UB-Mannheim.TesseractOCR).
    2. Copy tesseract.exe + the 26 runtime DLLs it actually loads.
    3. Download the "fast" chi_sim + eng language models from tessdata_fast.

  Re-running is safe (idempotent): existing files are overwritten.
#>

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$Dst         = Join-Path $ProjectRoot "src-tauri\tesseract"
$DstTessdata = Join-Path $Dst "tessdata"

# Runtime DLLs that tesseract.exe loads (determined empirically, not the full
# training-tool set). Keep in sync if the upstream build changes.
$RuntimeDlls = @(
  "libarchive-13", "libb2-1", "libbz2-1", "libcrypto-3-x64", "libdeflate",
  "libexpat-1", "libgcc_s_seh-1", "libgif-7", "libiconv-2", "libjbig-0",
  "libjpeg-8", "libleptonica-6", "libLerc", "liblz4", "liblzma-5",
  "libopenjp2-7", "libpng16-16", "libsharpyuv-0", "libstdc++-6",
  "libtesseract-5", "libtiff-6", "libwebp-7", "libwebpmux-3",
  "libwinpthread-1", "libzstd", "zlib1"
)

function Find-TesseractDir {
  $candidates = @(
    "C:\Program Files\Tesseract-OCR",
    "C:\Program Files (x86)\Tesseract-OCR",
    (Join-Path $env:LOCALAPPDATA "Programs\Tesseract-OCR")
  )
  foreach ($c in $candidates) {
    if (Test-Path (Join-Path $c "tesseract.exe")) { return $c }
  }
  return $null
}

Write-Host "==> Locating Tesseract install..."
$TessDir = Find-TesseractDir
if (-not $TessDir) {
  Write-Host "    Not found. Installing via winget (UB-Mannheim.TesseractOCR)..."
  winget install --id UB-Mannheim.TesseractOCR --accept-source-agreements --accept-package-agreements
  $TessDir = Find-TesseractDir
  if (-not $TessDir) { throw "Tesseract still not found after winget install." }
}
Write-Host "    Using: $TessDir"

Write-Host "==> Staging engine into $Dst"
if (Test-Path $Dst) { Remove-Item -Recurse -Force $Dst }
New-Item -ItemType Directory -Force -Path $DstTessdata | Out-Null

Copy-Item (Join-Path $TessDir "tesseract.exe") $Dst
$missing = @()
foreach ($d in $RuntimeDlls) {
  $src = Join-Path $TessDir "$d.dll"
  if (Test-Path $src) { Copy-Item $src $Dst } else { $missing += "$d.dll" }
}
if ($missing.Count -gt 0) {
  Write-Warning ("These expected DLLs were absent (engine may still work): " + ($missing -join ", "))
}

Write-Host "==> Downloading fast language models (chi_sim + eng)..."
$base = "https://github.com/tesseract-ocr/tessdata_fast/raw/main"
foreach ($lang in @("chi_sim", "eng")) {
  $out = Join-Path $DstTessdata "$lang.traineddata"
  Write-Host "    $lang..."
  # --ssl-no-revoke avoids CRL-check failures on restricted networks.
  curl.exe -L --ssl-no-revoke -o $out "$base/$lang.traineddata"
  if ((Get-Item $out).Length -lt 100000) {
    throw "Download of $lang.traineddata looks too small — check network / URL."
  }
}

$total = (Get-ChildItem -Recurse $Dst | Measure-Object Length -Sum).Sum
Write-Host ("==> Done. Bundle size: {0:N1} MB at {1}" -f ($total/1MB), $Dst)
Write-Host "    Now run: npm run tauri:build"
