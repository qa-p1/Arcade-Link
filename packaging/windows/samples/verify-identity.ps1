# No real profile mutation: every file and HKCU key uses the temp ARCADE_HOME.
$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ('Arcade identity ' + [Guid]::NewGuid().ToString('N'))
$previousHome = $env:ARCADE_HOME
$env:ARCADE_HOME = $root
$keyPath = $null
try {
    [IO.Directory]::CreateDirectory($root) | Out-Null
    $exe = Join-Path $root 'sample.exe'
    [IO.File]::WriteAllText($exe,'fixture')
    . (Join-Path $PSScriptRoot '..\arcade-integration.ps1') -Action Install -Id arcade.ci-identity -CliName arcade-ci-identity -Version 1 -ExePath $exe
    $vectors = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot '..\..\..\spec\vectors\windows-path-identity.json') | ConvertFrom-Json
    foreach ($case in $vectors.cases) {
        if ((Isolated-EnvironmentKey $case.root) -cne $case.key) { throw ('Identity mismatch for ' + $case.root) }
    }
    $keyPath = 'HKCU:\' + (Isolated-EnvironmentKey $root)
    if (-not (Test-Path -LiteralPath $keyPath)) { throw 'Install did not use the shared FNV key' }
    $registry = Get-Item -LiteralPath $keyPath
    try {
        $path = [string]$registry.GetValue('Path')
        if ($path -notlike ('*' + (Join-Path $root 'bin') + '*')) { throw 'Missing isolated Path entry' }
    } finally { $registry.Dispose() }
    & (Join-Path $PSScriptRoot '..\arcade-integration.ps1') -Action Uninstall -Id arcade.ci-identity -CliName arcade-ci-identity -ExePath $exe
    if ($LASTEXITCODE -ne 0) { throw 'Isolated uninstall failed' }
    Write-Host 'Rust/Qt/PowerShell fixed Windows path identity vectors passed'
} finally {
    $env:ARCADE_HOME = $previousHome
    if ($keyPath) { Remove-Item -LiteralPath $keyPath -Recurse -Force -ErrorAction SilentlyContinue }
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
