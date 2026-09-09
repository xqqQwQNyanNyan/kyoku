[CmdletBinding()]
param([string]$NodePath = (Get-Command node -ErrorAction Stop).Source)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('kyoku 诊断 ' + [Guid]::NewGuid().ToString('N'))
$account = '诊断用户'
$testPassword = '测试密码-with-"quotes"-and-空格'
try {
    $nodeDir = Join-Path $testRoot 'majsoul/node'
    New-Item -ItemType Directory -Path $nodeDir -Force | Out-Null
    $testNode = Join-Path $nodeDir 'node.exe'
    if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
        Copy-Item -LiteralPath $NodePath -Destination $testNode
    } else {
        New-Item -ItemType SymbolicLink -Path $testNode -Target $NodePath | Out-Null
    }
    $launcher = Join-Path $testRoot 'diagnose-majsoul.ps1'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'diagnose-majsoul.ps1') -Destination $launcher
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'trace-majsoul.cjs') -Destination $testRoot
    $sourceBytes = [IO.File]::ReadAllBytes($launcher)
    if ($sourceBytes[0] -ne 239 -or $sourceBytes[1] -ne 187 -or $sourceBytes[2] -ne 191) {
        throw 'The Windows PowerShell launcher must use UTF-8 with BOM.'
    }
    # 假子进程只检查管道内容和传参，不连接网络或登录。
    $fakeProbe = @'
const fs = require('node:fs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
if (input.username !== '诊断用户' || input.password !== '测试密码-with-"quotes"-and-空格' || input.accept_risk !== true) process.exit(2);
if (process.argv.some(v => v.includes(input.password)) || Object.values(process.env).some(v => v.includes(input.password))) process.exit(3);
if (process.argv[3] === '--desktop-probe' && input.replay !== 'https://game.maj-soul.com/1/?paipu=test') process.exit(4);
process.stdout.write('{"event":"LAUNCHER_TEST_OK"}\n');
'@
    [IO.File]::WriteAllText((Join-Path $testRoot 'diagnose-majsoul.cjs'), $fakeProbe, (New-Object Text.UTF8Encoding($false)))
    function Read-Host {
        param([string]$Prompt, [switch]$AsSecureString)
        if ($AsSecureString) { return ConvertTo-SecureString $testPassword -AsPlainText -Force }
        if ($Prompt -eq 'Account') { return $account }
        if ($Prompt -eq 'Replay link') { return 'https://game.maj-soul.com/1/?paipu=test' }
        return ''
    }
    foreach ($mode in @('Login', 'Replay')) {
    $parameters = @{ InstallDir = $testRoot; $mode = $true }
    $output = (& $launcher @parameters 6>&1 | Out-String)
    $reports = @(Get-ChildItem -LiteralPath $testRoot -Filter 'majsoul-diagnostic-*.txt')
    if ($reports.Count -ne 1) { throw "Expected one report. Output: $output" }
    $report = Get-Content -LiteralPath $reports[0].FullName -Raw -Encoding UTF8
    if (-not $report.Contains('LAUNCHER_TEST_OK') -or -not $report.Contains('"exitCode":0')) {
        throw "Credential pipe did not complete. Output: $output"
    }
    if ($report.Contains($account) -or $report.Contains($testPassword) -or $output.Contains($testPassword)) {
        throw 'Launcher exposed test credentials.'
    }
    Remove-Item -LiteralPath $reports[0].FullName
    }
    Write-Host 'PASS: PowerShell sends Unicode credentials through UTF-8 stdin without logging them.'
} finally {
    Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
}
