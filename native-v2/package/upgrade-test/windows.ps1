# The Windows upgrade test (package/CONTRACT.md): install the latest Electron
# Crundi silently, give it data, a config and launch at login (turned on by
# Electron itself), install the native installer over it, and check every one
# carried over, the uninstall entry and shortcuts are the new install's, and
# the server starts from the new files with the carried config. Then
# uninstall, and check the data stays. Needs nobody (CI windows-latest).
#
#   pwsh windows.ps1 -Old Crundi-Setup-1.21.7.exe -New Crundi-Setup-1.21.8.exe -Edition full -Version 1.21.8
param(
  [Parameter(Mandatory)] [string] $Old,
  [Parameter(Mandatory)] [string] $New,
  [Parameter(Mandatory)] [string] $Version,
  [string] $Edition = 'full',
  [int] $Port = 4861,
  # The staged install the installers were built from (the workflow's): the
  # next version's installer is built from it for the native -> native update.
  [string] $Stage = $(if ($env:RUNNER_TEMP) { Join-Path $env:RUNNER_TEMP "stage\$(if ($Edition -eq 'client') { 'client' } else { 'full' })" } else { '' })
)
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$full = $Edition -ne 'client'
$product = if ($full) { 'Crundi' } else { 'Crundi Client' }
$oldExe = if ($full) { 'Crundi.exe' } else { 'Crundi Client.exe' }
$newExe = if ($full) { 'crundi.exe' } else { 'crundi-client.exe' }
$guid = node (Join-Path $here '..\feed.mjs') guid ($(if ($full) { 'com.crundi.app' } else { 'com.crundi.client' }))
# electron-builder's install folder is the package name (crundi, crundi-client).
$inst = Join-Path $env:LOCALAPPDATA "Programs\$(if ($full) { 'crundi' } else { 'crundi-client' })"
$userData = Join-Path $env:APPDATA $product
$uninstRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$failures = @()
function Check($ok, $what) {
  if ($ok) { Write-Host "ok    $what" } else { Write-Host "FAIL  $what"; $script:failures += $what }
}
function Entries { Get-ChildItem $uninstRoot | ForEach-Object { $p = Get-ItemProperty $_.PSPath; if ($p.DisplayName -match "^$([regex]::Escape($product)) \d") { [pscustomobject]@{ Key = $_.PSChildName; Props = $p } } } }
function LnkTarget($path) { (New-Object -ComObject WScript.Shell).CreateShortcut($path).TargetPath }
function StopTree($procId) {
  Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $procId } | ForEach-Object { StopTree $_.ProcessId }
  Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
}
function UnderInstall { Get-CimInstance Win32_Process | Where-Object { ($_.ExecutablePath -and $_.ExecutablePath.StartsWith("$inst\", 'OrdinalIgnoreCase')) -or ($_.CommandLine -and $_.CommandLine.IndexOf($inst, [StringComparison]::OrdinalIgnoreCase) -ge 0) } }

# An installer or uninstaller still running (NSIS uninstallers relaunch from
# %TEMP%, so -Wait can return before they finish) makes the next setup fail.
function WaitInstallers {
  for ($i = 0; $i -lt 60; $i++) {
    $busy = @(Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(Un_[A-Z]\.exe|Au_\.exe|Uninstall .*\.exe|Crundi.*Setup.*\.exe)$' })
    if (-not $busy.Count) { return }
    if ($i -eq 0) { Write-Host "waiting for: $($busy.Name -join ', ')" }
    Start-Sleep 2
  }
}

Write-Host "== Electron: $Old"
WaitInstallers
for ($try = 1; $try -le 3; $try++) {
  $p = Start-Process $Old -ArgumentList '/S' -Wait -PassThru
  WaitInstallers
  if ($p.ExitCode -eq 0) { break }
  Write-Host "Electron setup exited $($p.ExitCode) (try $try)"; Start-Sleep 5
}
Check ($p.ExitCode -eq 0) "Electron setup exits 0 ($($p.ExitCode))"
Check (Test-Path "$inst\$oldExe") "Electron installed at $inst\$oldExe"
$before = @(Entries)
Check ($before.Count -eq 1) "one Electron uninstall entry ($($before.Count))"
$electronKey = $before[0].Key
Write-Host "Electron's uninstall entry: $electronKey"
$before[0].Props | Select-Object DisplayName, DisplayVersion, Publisher, UninstallString, QuietUninstallString, DisplayIcon, InstallLocation | Format-List | Out-String | Write-Host
Check ($electronKey -eq $guid) "the uninstall key is electron-builder's GUID for the appId ($guid)"

# Launch at login, set by Electron itself through its own API.
$e = Start-Process "$inst\$oldExe" -ArgumentList '--inspect=9230' -PassThru
$login = node (Join-Path $here 'electron-login.mjs') 9230
Write-Host "Electron says: $login"
Start-Sleep 2
StopTree $e.Id
UnderInstall | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep 2
$runBefore = (Get-Item $runKey).Property | Where-Object { (Get-ItemPropertyValue $runKey $_) -like "*$inst\$oldExe*" }
Check (@($runBefore).Count -eq 1) "Electron made one Run entry: $runBefore = $(if ($runBefore) { Get-ItemPropertyValue $runKey $runBefore })"

# Data and config, as a used install has them.
New-Item -ItemType Directory -Force "$userData\data\projects" | Out-Null
Set-Content "$userData\.env" "TELEGRAM_BOT_TOKEN=`nALLOWED_USERNAME=`nPROJECTS_DIR=`nWEB_PORT=0`n# upgrade-test $(Get-Date -Format o)" -NoNewline
Set-Content "$userData\client.json" '{"mode":"local","serverUrl":"","voiceWake":false}' -NoNewline
Set-Content "$userData\update-config.json" '{"enabled":true,"channel":"dev"}' -NoNewline
Set-Content "$userData\worker.json" '{"enabled":false}' -NoNewline
Set-Content "$userData\data\projects\upgrade-marker.json" '{"carried":true}' -NoNewline
$hash = @{}; Get-ChildItem $userData -Recurse -File | ForEach-Object { $hash[$_.FullName] = (Get-FileHash $_.FullName).Hash }
Write-Host "user data: $($hash.Count) files"

Write-Host "== Native: $New"
$t = Get-Date
$p = Start-Process $New -ArgumentList '/S' -Wait -PassThru
WaitInstallers
Check ($p.ExitCode -eq 0) "native setup exits 0 ($($p.ExitCode)) in $([int]((Get-Date) - $t).TotalSeconds) s"
Check (Test-Path "$inst\$newExe") "$newExe installed"
Check ((Get-Item "$inst\$newExe").Name -ceq $newExe) "the executable is the native one ($((Get-Item "$inst\$newExe").VersionInfo.FileDescription))"
Check (-not (Test-Path "$inst\resources\app.asar")) "Electron's app.asar is gone"
Check (-not (Test-Path "$inst\electron.exe") -and -not (Test-Path "$inst\ffmpeg.dll")) "Electron's runtime is gone"
foreach ($f in @('crundi-browser-host.exe', 'libcef.dll', 'crundi-worker.exe')) { Check (Test-Path "$inst\$f") "$f installed" }
if ($full) {
  foreach ($f in @('node\node.exe', 'server\src\index.js', 'server\app\vendor', 'server\skills\crundi', 'server\node_modules\node-pty', 'server\node_modules\cloudflared\bin\cloudflared.exe', 'server\.env.example')) { Check (Test-Path "$inst\$f") "$f installed" }
}

$after = @(Entries)
Check ($after.Count -eq 1) "one uninstall entry after the upgrade ($($after.Count))"
Check ($after[0].Key -eq $electronKey) "it is Electron's key ($($after[0].Key))"
Check ($after[0].Props.DisplayVersion -eq $Version) "DisplayVersion $($after[0].Props.DisplayVersion)"
$un = $after[0].Props.QuietUninstallString
Check ($un -like "*$inst\Uninstall $product.exe*" -and (Test-Path "$inst\Uninstall $product.exe")) "the uninstaller is the new one: $un"
$after[0].Props | Select-Object DisplayName, DisplayVersion, Publisher, UninstallString, QuietUninstallString, DisplayIcon | Format-List | Out-String | Write-Host

foreach ($k in $hash.Keys) { Check ((Test-Path $k) -and (Get-FileHash $k).Hash -eq $hash[$k]) "kept $k" }

$runAfter = (Get-Item $runKey).Property | Where-Object { (Get-ItemPropertyValue $runKey $_) -like "*$inst\*" }
Check (@($runAfter).Count -eq 1 -and $runAfter -eq $runBefore) "launch at login: still one entry, same name ($runAfter)"
if ($runAfter) {
  $v = Get-ItemPropertyValue $runKey $runAfter
  $target = ($v -replace '^"([^"]+)".*$', '$1')
  Check ((Test-Path $target) -and ((Get-Item $target).Name -ceq $newExe)) "it starts the native client: $v"
}

$desk = [Environment]::GetFolderPath('Desktop'); $menu = Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs'
foreach ($l in @("$desk\$product.lnk", "$menu\$product.lnk")) {
  Check ((Test-Path $l) -and ((LnkTarget $l) -ieq "$inst\$newExe")) "shortcut $l -> $(if (Test-Path $l) { LnkTarget $l })"
}

if ($full) {
  # The server starts from the new files, with the carried config and data,
  # as the client starts it (app/main.js startBot): bundled Node, src/index.js.
  $log = Join-Path $env:TEMP 'crundi-upgrade-server.log'
  $env:DOTENV_PATH = "$userData\.env"; $env:DATA_DIR = "$userData\data"; $env:ELECTRON_RUN = '1'
  $env:CRUNDI_DEV = '1'; $env:DEV_WEB_PORT = "$Port"; $env:HOMEPOOL_DISABLE = '1'
  $s = Start-Process "$inst\node\node.exe" -ArgumentList '--no-deprecation', "`"$inst\server\src\index.js`"", '--dev' -WorkingDirectory "$inst\server" -RedirectStandardOutput $log -RedirectStandardError "$log.err" -PassThru -WindowStyle Hidden
  $up = $false
  for ($i = 0; $i -lt 120 -and -not $up; $i++) {
    Start-Sleep 1
    try { $r = Invoke-WebRequest "http://127.0.0.1:$Port/" -UseBasicParsing -TimeoutSec 3; $up = $r.StatusCode -lt 500 } catch { if ($_.Exception.Response) { $up = [int]$_.Exception.Response.StatusCode -lt 500 } }
    if ($s.HasExited) { break }
  }
  Check $up "the server from $inst\server answers on :$Port (node $(& "$inst\node\node.exe" -v))"
  if (-not $up) { Get-Content $log -Tail 40 | Write-Host; Get-Content "$log.err" -Tail 40 | Write-Host }
  StopTree $s.Id
  Remove-Item Env:DOTENV_PATH, Env:DATA_DIR, Env:ELECTRON_RUN, Env:CRUNDI_DEV, Env:DEV_WEB_PORT, Env:HOMEPOOL_DISABLE
}

# The native client itself, started as the shortcut starts it: report whether it
# stays up and, all-in-one, whether it starts the bundled server.
Check ((Test-Path "$inst\version") -and ((Get-Content "$inst\version" -Raw).Trim() -eq $Version)) "$inst\version is $Version"
$clog = Join-Path $env:TEMP 'crundi-upgrade-client.log'
$nlog = Join-Path $env:APPDATA 'Crundi Native\crundi.log'
# Both editions keep their log there: this run reads only its own lines.
Remove-Item $nlog, $clog, "$clog.out" -Force -ErrorAction SilentlyContinue
$c = Start-Process "$inst\$newExe" -PassThru -RedirectStandardError $clog -RedirectStandardOutput "$clog.out"
# The first update check is 8 s after launch (the carried update-config: on, dev).
$upd = $null
for ($i = 0; $i -lt 40 -and -not $upd; $i++) {
  Start-Sleep 1
  if (Test-Path $nlog) { $upd = Select-String -Path $nlog -Pattern '\[update\] (Up to date|Available: .*|check failed.*)' | Select-Object -Last 1 }
}
Check (-not $c.HasExited) "client running after $i s"
Check ((Test-Path $nlog) -and (Select-String -Path $nlog -SimpleMatch "[lifecycle] version=$Version " -Quiet)) "the client reports version $Version"
$u = if ($upd) { $upd.Matches[0].Value } else { '' }
Write-Host "  updater: $u"
$offer = if ($u -match '^\[update\] Available: (.+)$') { $Matches[1].Trim() } else { '' }
# Newer: a release of this prerelease's version (1.21.8 over 1.21.8-dev.8), or higher by number.
$newer = [bool]($offer -and $offer -notmatch '-' -and $Version.StartsWith("$offer-"))
if (-not $newer -and $offer -and $offer -ne $Version) { $newer = ((@($Version, $offer) | Sort-Object { [regex]::Replace($_, '\d+', { $args[0].Value.PadLeft(8, '0') }) })[-1] -eq $offer) }
Check ($u -eq '[update] Up to date' -or $newer) "it offers no update to its own version or older ($u)"
if ($full) {
  # The server the client starts (startBot): its port from the line the client
  # reads ("HTTP server on <host>:<port>"), then it must answer.
  $cport = $null; $cup = $false
  for ($i = 0; $i -lt 120 -and -not $cup; $i++) {
    if (Test-Path $clog) {
      $m = Select-String -Path $clog -Pattern 'HTTP server on \S*:(\d+)' | Select-Object -Last 1
      if ($m) { $cport = $m.Matches[0].Groups[1].Value }
    }
    if ($cport) {
      try { $r = Invoke-WebRequest "http://127.0.0.1:$cport/" -UseBasicParsing -TimeoutSec 3; $cup = $r.StatusCode -lt 500 } catch { if ($_.Exception.Response) { $cup = [int]$_.Exception.Response.StatusCode -lt 500 } }
    }
    if (-not $cup) { Start-Sleep 1 }
  }
  $srv = UnderInstall | Where-Object { $_.CommandLine -like '*server\src\index.js*' }
  Check ([bool]$srv) "the client started the bundled server: $($srv.CommandLine)"
  Check $cup "...and it answers on :$cport within 120 s"
  if (-not $cup) { Write-Host '-- client stderr:'; Get-Content $clog -Tail 60 -ErrorAction SilentlyContinue | Write-Host; Write-Host '-- crundi.log:'; Get-Content $nlog -Tail 40 -ErrorAction SilentlyContinue | Write-Host }
}
StopTree $c.Id
Start-Sleep 2
UnderInstall | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }

# The native -> native update (PARITY-ELECTRON.md line 54), the path every
# Windows update takes from now on: the installed app's own updater, offered
# the next version by a GitHub of the test's own, downloads its Setup .exe
# (sha512 checked); Restart & Install quits the app and its server and runs
# the Setup with --updated /S --force-run (updater.rs installer_command),
# which installs in place and starts the app again.
Write-Host "== Native update: $Version -> next, through the app's updater"
$core = [regex]::Match($Version, '^(\d+)\.(\d+)\.(\d+)')
$next = "$($core.Groups[1].Value).$($core.Groups[2].Value).$([int]$core.Groups[3].Value + 1)"
$feed = if ($full) { 'latest.yml' } else { 'client.yml' }
$nextDir = Join-Path $env:TEMP "crundi-next-$Edition"
Remove-Item $nextDir -Recurse -Force -ErrorAction SilentlyContinue
if (-not $Stage -or -not (Test-Path "$Stage\version")) {
  Check $false "the staged install to build $next from ($Stage)"
} else {
  # The same files, the version file saying $next (lifecycle::version reads it).
  $staged = Get-Content "$Stage\version" -Raw
  Set-Content "$Stage\version" "$next`n" -NoNewline
  try { node (Join-Path $here '..\windows\build.mjs') --stage $Stage --edition $Edition --out $nextDir --version $next | Write-Host }
  finally { Set-Content "$Stage\version" $staged -NoNewline }
  $nextSetup = Join-Path $nextDir "$(if ($full) { 'Crundi-Setup' } else { 'Crundi-Client-Setup' })-$next.exe"
  Check ((Test-Path $nextSetup) -and (Test-Path "$nextDir\$feed")) "built $nextSetup and $feed"

  $portFile = Join-Path $env:TEMP "crundi-fake-gh-$Edition.port"
  Remove-Item $portFile -Force -ErrorAction SilentlyContinue
  $gh = Start-Process node -ArgumentList "`"$(Join-Path $here 'self-update.mjs')`"", 'serve', "`"$nextDir`"", $next, $feed, "`"$portFile`"" -PassThru -WindowStyle Hidden -RedirectStandardOutput "$env:TEMP\crundi-fake-gh-$Edition.log" -RedirectStandardError "$env:TEMP\crundi-fake-gh-$Edition.err"
  for ($i = 0; $i -lt 30 -and -not (Test-Path $portFile); $i++) { Start-Sleep 1 }
  $ghPort = Get-Content $portFile -ErrorAction SilentlyContinue

  $runName = $runAfter
  $lnks = @("$desk\$product.lnk", "$menu\$product.lnk")
  $markerHash = (Get-FileHash "$userData\data\projects\upgrade-marker.json").Hash
  Remove-Item $nlog, $clog, "$clog.out" -Force -ErrorAction SilentlyContinue
  $drv = $Port + 10
  $env:CRUNDI_TEST_PORT = "$drv"; $env:CRUNDI_UPDATE_API = "http://127.0.0.1:$ghPort"
  $c = Start-Process "$inst\$newExe" -PassThru -RedirectStandardError $clog -RedirectStandardOutput "$clog.out"
  Remove-Item Env:CRUNDI_TEST_PORT, Env:CRUNDI_UPDATE_API
  Write-Host "native $Version running (pid $($c.Id)), fake GitHub on :$ghPort offering $next"
  # Its server up first (all-in-one), so the update stops a running one.
  if ($full) {
    for ($i = 0; $i -lt 120 -and -not (UnderInstall | Where-Object { $_.CommandLine -like '*server\src\index.js*' }); $i++) { Start-Sleep 1 }
  }
  Start-Sleep 3
  # Everything of the old version: the app, its browser host, its server.
  $oldPids = @($c.Id) + @(UnderInstall | ForEach-Object { $_.ProcessId })
  Write-Host "old version's processes: $((UnderInstall | ForEach-Object { "$($_.ProcessId) $($_.Name)" }) -join '; ')"
  node (Join-Path $here 'self-update.mjs') drive $drv $next | Write-Host
  $driven = $LASTEXITCODE -eq 0
  Check $driven "the app offered $next, downloaded it and quit on Restart & Install"
  Get-Content "$env:TEMP\crundi-fake-gh-$Edition.log" -ErrorAction SilentlyContinue | Write-Host
  if ($driven) {
    for ($i = 0; $i -lt 60 -and -not $c.HasExited; $i++) { Start-Sleep 1 }
    Check $c.HasExited "the old app ($($c.Id)) is gone"
    # The Setup runs, then starts the app again: a new $newExe under $inst.
    $again = $null
    for ($i = 0; $i -lt 300 -and -not $again; $i++) {
      Start-Sleep 1
      $again = Get-CimInstance Win32_Process | Where-Object { $_.Name -ieq $newExe -and $_.ExecutablePath -and $_.ExecutablePath -ieq "$inst\$newExe" -and $oldPids -notcontains $_.ProcessId } | Select-Object -First 1
    }
    WaitInstallers
    Check ([bool]$again) "the app started again by itself after $i s (pid $($again.ProcessId))"
    Check (((Get-Content "$inst\version" -Raw).Trim()) -eq $next) "installed in place: $inst\version is $next"
    $seen = $false
    for ($i = 0; $i -lt 30 -and -not $seen; $i++) { Start-Sleep 1; $seen = (Test-Path $nlog) -and (Select-String -Path $nlog -SimpleMatch "[lifecycle] version=$next " -Quiet) }
    Check $seen "the started app reports version $next"
    Check ((Test-Path $nlog) -and (Select-String -Path $nlog -SimpleMatch '[update] Running ' -Quiet)) "the old app ran the downloaded Setup: $((Select-String -Path $nlog -SimpleMatch '[update] Running ' | Select-Object -Last 1).Line)"
    $setupRun = Get-CimInstance Win32_Process | Where-Object { $_.Name -like 'Crundi*Setup*' }
    Check (-not $setupRun) "the Setup has finished"
    $left = @(Get-Process -Id $oldPids -ErrorAction SilentlyContinue)
    Check ($left.Count -eq 0) "no process of the old version is left ($($left.Id -join ', '))"
    $apps = @(Get-CimInstance Win32_Process | Where-Object { $_.Name -ieq $newExe -and $_.ExecutablePath -ieq "$inst\$newExe" })
    Check ($apps.Count -eq 1) "one app running ($($apps.Count))"

    $e2 = @(Entries)
    Check ($e2.Count -eq 1 -and $e2[0].Key -eq $electronKey) "still one uninstall entry, the same key ($($e2.Key -join ', '))"
    Check ($e2.Count -eq 1 -and $e2[0].Props.DisplayVersion -eq $next) "DisplayVersion $($e2[0].Props.DisplayVersion)"
    $run2 = @((Get-Item $runKey).Property | Where-Object { (Get-ItemPropertyValue $runKey $_) -like "*$inst\*" })
    Check ($run2.Count -eq 1 -and $run2[0] -eq $runName -and (Get-ItemPropertyValue $runKey $runName) -like "*$inst\$newExe*") "launch at login: still one Run value, the same ($run2)"
    foreach ($l in $lnks) { Check ((Test-Path $l) -and ((LnkTarget $l) -ieq "$inst\$newExe")) "shortcut kept: $l" }
    Check ((Test-Path "$userData\data\projects\upgrade-marker.json") -and (Get-FileHash "$userData\data\projects\upgrade-marker.json").Hash -eq $markerHash) "data kept"
    Check ((Get-Content "$userData\.env" -Raw) -match '# upgrade-test') "the server's .env kept"
    Check ((Get-Content "$userData\client.json" -Raw | ConvertFrom-Json).mode -eq 'local') "client.json kept"
    $uc = Get-Content "$userData\update-config.json" -Raw | ConvertFrom-Json
    Check ($uc.channel -eq 'dev') "update settings kept (channel $($uc.channel))"
    if ($full) {
      $srv = $null
      for ($i = 0; $i -lt 120 -and -not $srv; $i++) { $srv = UnderInstall | Where-Object { $_.CommandLine -like '*server\src\index.js*' }; if (-not $srv) { Start-Sleep 1 } }
      Check ([bool]$srv) "the started app runs the bundled server again"
    }
    if ($again) { StopTree $again.ProcessId }
  } else {
    Write-Host '-- crundi.log:'; Get-Content $nlog -Tail 40 -ErrorAction SilentlyContinue | Write-Host
    Write-Host '-- client stderr:'; Get-Content $clog -Tail 40 -ErrorAction SilentlyContinue | Write-Host
    StopTree $c.Id
  }
  Start-Sleep 2
  UnderInstall | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  if ($gh) { Stop-Process -Id $gh.Id -Force -ErrorAction SilentlyContinue }
}

Write-Host "== Uninstall"
$p = Start-Process "$inst\Uninstall $product.exe" -ArgumentList '/currentuser', '/S' -Wait -PassThru
WaitInstallers
Start-Sleep 5
Check (@(Entries).Count -eq 0) "uninstall removes the entry"
Check (-not (Test-Path "$inst\$newExe")) "uninstall removes the files"
Check ((Test-Path "$userData\.env") -and (Test-Path "$userData\data\projects\upgrade-marker.json")) "uninstall keeps the data (silent = keep)"

$size = (Get-Item $New).Length; $osize = (Get-Item $Old).Length
Write-Host ("size: native {0:N1} MB, Electron {1:N1} MB" -f ($size / 1MB), ($osize / 1MB))
if ($failures.Count) { Write-Host "`n$($failures.Count) FAILED:"; $failures | ForEach-Object { Write-Host "  $_" }; exit 1 }
Write-Host "`nWindows upgrade test passed ($Edition)"
exit 0
