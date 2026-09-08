# 在 Windows x64 上准备完整离线资源、运行测试并构建 NSIS 安装包。
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $projectDir

if ($env:OS -ne "Windows_NT" -or -not [Environment]::Is64BitOperatingSystem) {
    throw "Windows 安装包必须在 Windows x64 环境构建。"
}

if (-not (Get-Command py -ErrorAction SilentlyContinue)) {
    throw "未找到 Python Launcher，请先安装 Python 3.12 x64。"
}
& py -3.12 -c "import struct,sys; assert sys.version_info[:2] == (3,12) and struct.calcsize('P') == 8"
if ($LASTEXITCODE -ne 0) {
    throw "需要 Python 3.12 x64。"
}

$venvDir = Join-Path $projectDir "mortal/.venv"
$python = Join-Path $venvDir "Scripts/python.exe"
if (-not (Test-Path $python -PathType Leaf)) {
    & py -3.12 -m venv $venvDir
    if ($LASTEXITCODE -ne 0) { throw "创建 Mortal 虚拟环境失败。" }
}

$runtimeDir = Join-Path $projectDir "mortal/runtime"
$runtimeCommit = "0cff2b52982be5b1163aa9a62fb01f03ce91e0d2"
if (-not (Test-Path $runtimeDir -PathType Container)) {
    & git clone https://github.com/Equim-chan/Mortal.git $runtimeDir
    if ($LASTEXITCODE -ne 0) { throw "下载 Mortal 源码失败。" }
    & git -C $runtimeDir checkout --detach $runtimeCommit
    if ($LASTEXITCODE -ne 0) { throw "切换 Mortal 固定版本失败。" }
}
if ((& git -C $runtimeDir rev-parse HEAD) -ne $runtimeCommit) {
    throw "Mortal 源码版本不匹配：$runtimeDir"
}
$runtimeStatus = (& git -C $runtimeDir status --porcelain=v1 --untracked-files=all) -join "`n"
if ($runtimeStatus) {
    throw "Mortal 源码存在本地修改，请先处理：`n$runtimeStatus"
}

& $python -m pip install --disable-pip-version-check "torch==2.14.0" "numpy==2.5.2"
if ($LASTEXITCODE -ne 0) { throw "安装固定 Python 依赖失败。" }

$previousPyo3Python = $env:PYO3_PYTHON
try {
    $env:PYO3_PYTHON = $python
    & cargo build --manifest-path (Join-Path $runtimeDir "Cargo.toml") -p libriichi --lib --release --locked
    if ($LASTEXITCODE -ne 0) { throw "编译 libriichi.pyd 失败。" }
} finally {
    $env:PYO3_PYTHON = $previousPyo3Python
}

$modelDir = Join-Path $projectDir "mortal/models"
$model = Join-Path $modelDir "mortal_582500.pth"
$modelSha = "738e0d6e3c0ce9671629554ad39abd147d2ffbac676e80b194c83f2acc0fea20"
New-Item -ItemType Directory -Force $modelDir | Out-Null
if (-not (Test-Path $model -PathType Leaf)) {
    $part = "$model.part"
    Invoke-WebRequest -Uri "https://huggingface.co/Yuchen1457/mortal-582500/resolve/7386c9f5c751a3ea75efea99737cef5a5ef950f1/mortal_582500.pth" -OutFile $part
    if ((Get-FileHash -Algorithm SHA256 $part).Hash.ToLowerInvariant() -ne $modelSha) {
        Remove-Item $part -ErrorAction SilentlyContinue
        throw "Mortal 权重 SHA-256 不匹配。"
    }
    Move-Item $part $model
}
if ((Get-FileHash -Algorithm SHA256 $model).Hash.ToLowerInvariant() -ne $modelSha) {
    throw "Mortal 权重 SHA-256 不匹配。"
}

& $python scripts/prepare-windows.py
if ($LASTEXITCODE -ne 0) { throw "准备 Windows 内置运行资源失败。" }

& npm --prefix desktop ci
if ($LASTEXITCODE -ne 0) { throw "安装桌面前端依赖失败。" }
& cargo test --locked
if ($LASTEXITCODE -ne 0) { throw "Rust 测试失败。" }
& npm --prefix desktop test
if ($LASTEXITCODE -ne 0) { throw "桌面前端测试失败。" }
& npm --prefix services/majsoul test
if ($LASTEXITCODE -ne 0) { throw "雀魂组件测试失败。" }
& cargo test --manifest-path desktop/src-tauri/Cargo.toml --locked
if ($LASTEXITCODE -ne 0) { throw "桌面 Rust 测试失败。" }

& npm --prefix desktop run tauri -- build --target x86_64-pc-windows-msvc --config src-tauri/tauri.windows.conf.json
if ($LASTEXITCODE -ne 0) { throw "构建 Windows NSIS 安装包失败。" }

$bundleDir = Join-Path $projectDir "desktop/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis"
$installers = @(Get-ChildItem $bundleDir -Filter "*-setup.exe" -File)
if ($installers.Count -ne 1) {
    throw "未找到唯一的 Windows NSIS 安装包：$bundleDir"
}
$installer = $installers[0]
$hash = (Get-FileHash -Algorithm SHA256 $installer.FullName).Hash.ToLowerInvariant()
Set-Content -Encoding ascii -NoNewline -Path "$($installer.FullName).sha256" -Value "$hash  $($installer.Name)`n"
Write-Host "Windows 安装包已生成：$($installer.FullName)"
Write-Host "SHA-256：$hash"
