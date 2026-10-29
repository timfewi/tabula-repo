param(
  [Parameter(Mandatory=$true)][string]$Version,
  [Parameter(Mandatory=$true)][string]$Archive,
  [Parameter(Mandatory=$true)][string]$Sha256,
  [string]$Prefix = (Join-Path $env:LOCALAPPDATA 'Tabula')
)
$ErrorActionPreference = 'Stop'
$created = $false
$bundle = $null
try {
  if ($Version -notmatch '^\d+\.\d+\.\d+$' -or $Sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Invalid version or checksum' }
  if ([Environment]::Is64BitProcess -eq $false -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'Requires Windows x86-64' }
  $archiveItem = Get-Item -LiteralPath $Archive
  if ($archiveItem.PSIsContainer -or ($archiveItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Archive must be a regular file' }
  if ((Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Sha256) { throw 'Checksum mismatch' }
  if (-not [IO.Path]::IsPathRooted($Prefix)) { throw 'Prefix must be absolute' }
  $Prefix = [IO.Path]::GetFullPath($Prefix)
  $path = $Prefix
  while ($path) {
    $item = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    if ($null -ne $item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Reparse point in prefix' }
    $parent = [IO.Path]::GetDirectoryName($path)
    if ($parent -eq $path) { break }; $path = $parent
  }
  if (Test-Path -LiteralPath $Prefix) { throw 'Choose an unused installation directory' }
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $bundle = [IO.Compression.ZipFile]::OpenRead($archiveItem.FullName)
  $top = "tabula-$Version-x86_64-pc-windows-msvc"
  $expected = @("$top/tabula.exe", "$top/LICENSE", "$top/README.md", "$top/REFERENCE.md", "$top/VERSION", "$top/THIRD_PARTY_LICENSES")
  $seen = @{}
  foreach ($entry in $bundle.Entries) {
    if ($entry.FullName -notin $expected -or $seen.ContainsKey($entry.FullName) -or $entry.Length -gt 67108864) { throw 'Unexpected, duplicate or oversized archive member' }
    $mode = ($entry.ExternalAttributes -shr 16) -band 61440
    if ($mode -ne 0 -and $mode -ne 32768) { throw 'Non-regular archive member' }
    $seen[$entry.FullName] = $true
  }
  if ($seen.Count -ne 6) { throw 'Missing archive member' }
  # CreateNew is the overwrite boundary for every extracted member.
  New-Item -ItemType Directory -Path $Prefix -ErrorAction Stop | Out-Null
  $created = $true
  foreach ($entry in $bundle.Entries) {
    $destination = Join-Path $Prefix ([IO.Path]::GetFileName($entry.FullName))
    $output = [IO.File]::Open($destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $inputStream = $entry.Open()
    try { $inputStream.CopyTo($output) } finally { $inputStream.Dispose(); $output.Dispose() }
  }
  if (([IO.File]::ReadAllText((Join-Path $Prefix 'VERSION'))).Trim() -cne $Version) { throw 'Version mismatch' }
  $binary = Join-Path $Prefix 'tabula.exe'
  if ((& $binary --version) -cne "Tabula $Version" -or $LASTEXITCODE -ne 0) { throw 'Binary version mismatch' }
  $created = $false
  Write-Output 'Installed Tabula. Add the installation directory to PATH.'
} catch {
  if ($created) { Remove-Item -LiteralPath $Prefix -Recurse -Force }
  Write-Error "tabula: installation refused: $($_.Exception.Message)" -ErrorAction Continue
  exit 2
} finally { if ($bundle) { $bundle.Dispose() } }
