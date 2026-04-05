param(
    [string]$Command = ""
)

$ErrorActionPreference = "Stop"

function Show-Menu {
    Write-Host ""
    Write-Host "=== ToneDock Dev Menu ===" -ForegroundColor Cyan
    Write-Host "  1. run        - Build and run" -ForegroundColor White
    Write-Host "  2. watch      - Watch and auto-rebuild" -ForegroundColor White
    Write-Host "  3. check      - Type check only" -ForegroundColor White
    Write-Host "  4. clippy     - Lint with clippy" -ForegroundColor White
    Write-Host "  5. fmt        - Format code" -ForegroundColor White
    Write-Host "  6. test       - Run tests" -ForegroundColor White
    Write-Host "  0. exit       - Quit" -ForegroundColor Gray
    Write-Host ""
}

function Invoke-DevCommand {
    param([string]$Cmd)

    switch ($Cmd) {
        { $_ -in "1", "run" } {
            Write-Host "[dev] cargo run ..." -ForegroundColor Green
            cargo run $args
        }
        { $_ -in "2", "watch" } {
            $watchExists = Get-Command cargo-watch -ErrorAction SilentlyContinue
            if (-not $watchExists) {
                Write-Host "[dev] cargo-watch not found. Installing ..." -ForegroundColor Yellow
                cargo install cargo-watch
            }
            Write-Host "[dev] cargo watch (check + run) ..." -ForegroundColor Green
            cargo watch -x check -x "run"
        }
        { $_ -in "3", "check" } {
            Write-Host "[dev] cargo check ..." -ForegroundColor Green
            cargo check
        }
        { $_ -in "4", "clippy" } {
            Write-Host "[dev] cargo clippy ..." -ForegroundColor Green
            cargo clippy -- -W clippy::all
        }
        { $_ -in "5", "fmt" } {
            Write-Host "[dev] cargo fmt ..." -ForegroundColor Green
            cargo fmt
        }
        { $_ -in "6", "test" } {
            Write-Host "[dev] cargo test ..." -ForegroundColor Green
            cargo test
        }
        { $_ -in "0", "exit", "q" } {
            Write-Host "Bye." -ForegroundColor Gray
            exit 0
        }
        default {
            Write-Host "Unknown command: $_" -ForegroundColor Red
            return
        }
    }
}

if ($Command -ne "") {
    Invoke-DevCommand $Command
    exit $LASTEXITCODE
}

while ($true) {
    Show-Menu
    $choice = Read-Host "Select"
    if ([string]::IsNullOrWhiteSpace($choice)) { continue }
    Invoke-DevCommand $choice.Trim().ToLower()
    Write-Host ""
}
