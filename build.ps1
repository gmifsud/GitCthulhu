# Compile, test, clean, and launch GitCthulhu.
#
#   .\build.ps1              debug build of the app
#   .\build.ps1 debug        same as the default
#   .\build.ps1 release      optimised build
#   .\build.ps1 run          open the debug app; anything after -- is passed to it
#   .\build.ps1 test         workspace tests; anything after -- is passed to cargo test
#   .\build.ps1 check        type-check without writing a binary
#   .\build.ps1 clean        delete target\
#   .\build.ps1 tools        show rustc, cargo, rustup, and git
#
# The debug binary is target\debug\git_cthulhu.exe.
# The release binary is target\release\git_cthulhu.exe.

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('debug', 'release', 'run', 'test', 'check', 'clean', 'tools')]
    [string]$Task = 'debug',

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Rest = @()
)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

function Add-CargoToPath {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    $cargoExe = Join-Path $cargoBin 'cargo.exe'
    if ((Test-Path $cargoExe) -and -not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $env:Path = "$cargoBin;$env:Path"
    }
}

function Require-Cargo {
    Add-CargoToPath
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw @"
cargo is not on PATH.
Install Rust from https://rustup.rs, then open a new PowerShell window.
On Windows the default toolchain also needs the Visual Studio C++ build tools
(the linker link.exe). rustup's installer can install those for you.
"@
    }
}

function Invoke-Cargo {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)
    if ($Rest.Count -gt 0) {
        $Arguments = $Arguments + $Rest
    }
    Write-Host "cargo $($Arguments -join ' ')"
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

Require-Cargo

switch ($Task) {
    'tools' {
        & rustc --version
        & cargo --version
        & rustup show
        if (Get-Command git -ErrorAction SilentlyContinue) {
            & git --version
        }
        else {
            Write-Warning 'git is not on PATH. Fetch, push, pull, stash, and branch commands need it.'
        }
    }
    'check' {
        Invoke-Cargo @('check', '--workspace')
    }
    'debug' {
        Invoke-Cargo @('build', '-p', 'app', '--bin', 'git_cthulhu')
        Write-Host "Debug binary: $(Join-Path $PSScriptRoot 'target\debug\git_cthulhu.exe')"
    }
    'release' {
        Invoke-Cargo @('build', '-p', 'app', '--bin', 'git_cthulhu', '--release')
        Write-Host "Release binary: $(Join-Path $PSScriptRoot 'target\release\git_cthulhu.exe')"
    }
    'run' {
        Invoke-Cargo @('run', '-p', 'app', '--bin', 'git_cthulhu', '--')
    }
    'test' {
        if ($Rest.Count -gt 0) {
            Invoke-Cargo @('test')
        }
        else {
            Invoke-Cargo @('test', '--workspace')
        }
    }
    'clean' {
        Invoke-Cargo @('clean')
    }
}
