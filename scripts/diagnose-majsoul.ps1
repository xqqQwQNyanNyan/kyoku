[CmdletBinding()]
param([string]$InstallDir, [switch]$Login, [switch]$Replay)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try {
    if (-not $InstallDir) {
        $key = Get-Item 'HKCU:\Software\kyoku\Kyoku' -ErrorAction SilentlyContinue
        if ($key) { $InstallDir = $key.GetValue('') }
        if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Kyoku' }
    }
    $node = Join-Path $InstallDir 'majsoul\node\node.exe'
    $service = Join-Path $InstallDir 'majsoul\service'
    $probe = Join-Path $PSScriptRoot 'diagnose-majsoul.cjs'
    if (-not (Test-Path -LiteralPath $node -PathType Leaf)) {
        throw 'Bundled node.exe not found. Run again with -InstallDir followed by the Kyoku installation folder.'
    }
    if (-not (Test-Path -LiteralPath $probe -PathType Leaf)) {
        throw 'diagnose-majsoul.cjs is missing. Extract both files from the ZIP first.'
    }
    $reportPath = Join-Path $PSScriptRoot ('majsoul-diagnostic-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.txt')
    if ($Login -or $Replay) {
        $replayLink = $null
        if ($Replay) {
            if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'trace-majsoul.cjs') -PathType Leaf)) {
                throw 'trace-majsoul.cjs is missing. Extract all three scripts from the ZIP first.'
            }
            $replayLink = Read-Host 'Replay link'
        }
        Write-Host 'This sends ONE login to the official Traditional Chinese server, then closes the session.'
        Write-Host 'Close other game clients first. Unofficial login can trigger account security checks.'
        Write-Host 'Your account and password stay in local process memory and are excluded from the report.'
        $userName = Read-Host 'Account'
        $password = Read-Host 'Password' -AsSecureString
        $pointer = [IntPtr]::Zero
        $process = $null
        $started = $false
        $writer = $null
        $payload = $null
        try {
            $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($password)
            $payload = @{
                username = $userName
                password = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
                accept_risk = $true
                replay = $replayLink
            } | ConvertTo-Json -Compress
            $start = New-Object System.Diagnostics.ProcessStartInfo
            $start.FileName = $node
            $start.Arguments = '"{0}" "{1}" --login-probe' -f $probe, $service
            if ($Replay) { $start.Arguments = '"{0}" "{1}" --desktop-probe' -f $probe, $service }
            $start.UseShellExecute = $false
            $start.CreateNoWindow = $true
            $start.RedirectStandardInput = $true
            $start.RedirectStandardOutput = $true
            $start.RedirectStandardError = $true
            $start.StandardOutputEncoding = [Text.Encoding]::UTF8
            $start.EnvironmentVariables.Clear()
            foreach ($name in @('PATH', 'SystemRoot', 'WINDIR', 'TEMP', 'TMP', 'USERPROFILE')) {
                $value = [Environment]::GetEnvironmentVariable($name)
                if ($null -ne $value) { $start.EnvironmentVariables[$name] = $value }
            }
            $process = New-Object System.Diagnostics.Process
            $process.StartInfo = $start
            [void]$process.Start()
            $started = $true
            $stderr = $process.StandardError.ReadToEndAsync()
            # 文本写入器固定 UTF-8，无需手动构造字节缓冲区。
            if ([string]::IsNullOrEmpty($payload)) { throw 'Credential input could not be prepared.' }
            $encoding = New-Object System.Text.UTF8Encoding($false)
            $writer = New-Object System.IO.StreamWriter($process.StandardInput.BaseStream, $encoding)
            $writer.WriteLine($payload)
            $writer.Dispose()
            $writer = $null
            $payload = $null
            while ($null -ne ($line = $process.StandardOutput.ReadLine())) {
                Write-Host $line
                Add-Content -LiteralPath $reportPath -Value $line -Encoding UTF8
            }
            $process.WaitForExit()
            $exitLine = '{"event":"LOGIN_PROBE_EXIT","exitCode":' + $process.ExitCode + '}'
            Write-Host $exitLine
            Add-Content -LiteralPath $reportPath -Value $exitLine -Encoding UTF8
        } finally {
            if ($writer) { $writer.Dispose() }
            if ($pointer -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
            if ($process) {
                if ($started -and -not $process.HasExited) { $process.Kill() }
                $process.Dispose()
            }
            $payload = $null
            $userName = $null
            $password.Dispose()
        }
    } else {
        Write-Host 'Testing the installed component. No account login will be sent.'
        & $node $probe $service | Tee-Object -FilePath $reportPath
    }
    Write-Host "Report saved: $reportPath"
} catch {
    Write-Host $_.Exception.Message
}
Read-Host 'Press Enter to close'
