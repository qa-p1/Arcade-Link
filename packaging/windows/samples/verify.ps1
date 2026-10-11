# Runs only on a disposable Windows CI account. Uses stable per-user paths,
# restores the pre-test user Path and removes every sample in finally.
$ErrorActionPreference = 'Stop'
$key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
$hadPath = $key.GetValueNames() -contains 'Path'
$oldPath = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
$oldKind = if ($hadPath) { $key.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
$root = Join-Path $env:LOCALAPPDATA 'Arcade'
$bin = Join-Path $root 'bin'
$cases = @(
    @{ Id='arcade.ci-inno'; Name='Arcade Link CI Inno'; Cli='arcade-ci-inno'; Setup='sample-inno.exe'; Args=@('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/MERGETASKS=arcade_startup,arcade_desktop'); Uninstaller='unins000.exe'; UninstallArgs=@('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') },
    @{ Id='arcade.ci-nsis'; Name='Arcade Link CI NSIS'; Cli='arcade-ci-nsis'; Setup='sample-nsis.exe'; Args=@('/S','/STARTATLOGIN=1','/DESKTOPSHORTCUT=1'); Uninstaller='uninstall.exe'; UninstallArgs=@('/S') }
)
function Run([string]$Exe, [string[]]$Arguments) {
    $process = Start-Process -FilePath $Exe -ArgumentList $Arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        foreach ($case in $cases) {
            $log = Join-Path (Installed $case) 'arcade-integration-error.log'
            if (Test-Path -LiteralPath $log) { Get-Content -Raw -LiteralPath $log | Write-Host }
        }
        throw "$Exe exited $($process.ExitCode)"
    }
}
function Assert([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function PathCount {
    $raw = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    return @($raw.Split(';') | Where-Object { [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') -ieq $bin.TrimEnd('\') }).Count
}
function Installed($Case) {
    if ($Case.ActiveDir) { return $Case.ActiveDir }
    return Join-Path $env:LOCALAPPDATA ('Programs\' + $Case.Name)
}
function Uninstall($Case, [string]$Directory = '') {
    $dir = if ($Directory) { $Directory } else { Installed $Case }
    $exe = Join-Path $dir $Case.Uninstaller
    if (-not (Test-Path -LiteralPath $exe) -or -not (Test-Path -LiteralPath (Join-Path $dir 'arcade-integration.ps1'))) { return }
    # NSIS normally forks a temporary uninstaller and returns early. _?=
    # runs it in place so Start-Process -Wait observes completion. It must
    # be last and unquoted, even when INSTDIR contains spaces.
    $arguments = @($Case.UninstallArgs)
    if ($Case.Id -eq 'arcade.ci-nsis') { $arguments += ('_?=' + $dir) }
    Run $exe $arguments
}
try {
    foreach ($case in $cases) {
        $setup = Join-Path $PSScriptRoot $case.Setup
        Run $setup $case.Args
        Run $setup $case.Args  # repeated installation must not duplicate Path
        Assert ((PathCount) -eq 1) 'Arcade bin must occur once in user Path'
        $shim = Join-Path $bin ($case.Cli + '.cmd')
        Assert (Test-Path -LiteralPath $shim) 'missing CLI shim'
        Assert ((Get-Content -Raw -LiteralPath $shim) -match '%\*') 'shim does not forward arguments'
        $receipt = Join-Path $root ('installs\' + $case.Id + '.json')
        $r = Get-Content -Raw -LiteralPath $receipt -Encoding UTF8 | ConvertFrom-Json
        Assert ($r.schema -eq 1 -and $r.id -eq $case.Id -and $r.method -eq 'windows-installer') 'invalid receipt'
        Assert ($r.managedBy -eq 'self' -and $r.version -eq '0.3.0') 'wrong receipt version/manager'
        Assert (Test-Path -LiteralPath $r.path) 'missing installed executable'
        Assert (Test-Path -LiteralPath $r.integration.desktopEntry) 'missing Start menu entry'
        Assert (Test-Path -LiteralPath $r.integration.autostart) 'missing start-at-login entry'
        Assert (Test-Path -LiteralPath (Join-Path ([Environment]::GetFolderPath('Desktop')) ($case.Name + '.lnk'))) 'missing optional desktop shortcut'
        $metadataPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $case.Id
        $metadata = Get-ItemProperty $metadataPath
        Assert ($metadata.Publisher -eq 'qa-p1' -and $metadata.DisplayVersion -eq '0.3.0') 'invalid Apps & features metadata'
        Write-Host "$($case.Id): silent install, repeat install, receipt, shim, tasks and metadata passed"
        # Regression: a newer copy in a different directory replaces shared
        # integration, then the *old* native uninstaller must leave it intact.
        $case.OldDir = Installed $case
        $successor = $case.OldDir + ' successor'
        $nextArgs = @($case.Args)
        if ($case.Id -eq 'arcade.ci-inno') { $nextArgs += ('/DIR="' + $successor + '"') }
        else { $nextArgs += ('/D=' + $successor) } # NSIS /D must be last.
        $newerSetup = $case.Setup.Replace('.exe','-newer.exe')
        Run (Join-Path $PSScriptRoot $newerSetup) $nextArgs
        $case.ActiveDir = $successor
        $newReceipt = Get-Content -Raw -LiteralPath $receipt -Encoding UTF8 | ConvertFrom-Json
        Assert ($newReceipt.version -eq '0.3.1' -and $newReceipt.path -eq (Join-Path $successor 'sample.exe')) 'successor receipt was not installed'
        $shared = @($receipt,$shim,$newReceipt.integration.desktopEntry,$newReceipt.integration.autostart,(Join-Path ([Environment]::GetFolderPath('Desktop')) ($case.Name + '.lnk')))
        $before = @{}
        foreach ($entry in $shared) { $before[$entry] = [Convert]::ToBase64String([IO.File]::ReadAllBytes($entry)) }
        Uninstall $case $case.OldDir
        foreach ($entry in $shared) {
            Assert (Test-Path -LiteralPath $entry) ('old uninstall removed successor entry: ' + $entry)
            Assert ([Convert]::ToBase64String([IO.File]::ReadAllBytes($entry)) -eq $before[$entry]) ('old uninstall changed successor entry: ' + $entry)
        }
        $metadata = Get-ItemProperty $metadataPath
        Assert ($metadata.DisplayVersion -eq '0.3.1' -and $metadata.InstallLocation -eq $successor) 'old uninstall removed or changed successor metadata'
        Assert ((PathCount) -eq 1) 'old uninstall changed shared Path'
        Write-Host "$($case.Id): uninstall old copy after newer install preserved all successor integration"
    }
    Uninstall $cases[0]
    Assert ((PathCount) -eq 1) 'uninstall removed shared Path while another shim exists'
    Uninstall $cases[1]
    Assert ((PathCount) -eq 0) 'last uninstall left Arcade bin in Path'
    foreach ($case in $cases) {
        Assert (-not (Test-Path -LiteralPath (Join-Path $bin ($case.Cli + '.cmd')))) 'shim remained after uninstall'
        Assert (-not (Test-Path -LiteralPath (Join-Path $root ('installs\' + $case.Id + '.json')))) 'receipt remained after uninstall'
        Assert (-not (Test-Path -LiteralPath (Join-Path ([Environment]::GetFolderPath('Startup')) ($case.Name + '.lnk')))) 'autostart remained after uninstall'
        Assert (-not (Test-Path -LiteralPath ('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $case.Id))) 'Apps & features metadata remained after uninstall'
        Assert (-not (Test-Path -LiteralPath (Join-Path ([Environment]::GetFolderPath('Desktop')) ($case.Name + '.lnk')))) ($case.Id + ': desktop shortcut remained after uninstall')
    }
    Write-Host 'Both silent uninstalls and last-one-out Path removal passed'
} finally {
    foreach ($case in $cases) {
        try { Uninstall $case } catch { Write-Warning $_ }
        Remove-Item -LiteralPath (Installed $case) -Recurse -Force -ErrorAction SilentlyContinue
        if ($case.OldDir) { Remove-Item -LiteralPath $case.OldDir -Recurse -Force -ErrorAction SilentlyContinue }
    }
    if ($hadPath) { $key.SetValue('Path', $oldPath, $oldKind) } else { $key.DeleteValue('Path', $false) }
    $key.Dispose()
}
