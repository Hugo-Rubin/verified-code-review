# Run specified arms for N trials into an existing or new trials root.
#
# Complements run_trials.ps1, which runs the standard three arms. This one
# takes an explicit arm list so a single configuration can be re-measured, or
# a new ablation added to an existing set of trials, without re-running
# everything.
#
#     pwsh scripts/run_arms.ps1 -Trials 3 -Root results-trials-v6 -Arms advanced
#     pwsh scripts/run_arms.ps1 -Trials 3 -Root results-trials -Arms advanced:candidates-only

param(
    [int]$Trials = 3,
    [string]$Root = "results-trials",
    [string]$Benchmark = "benchmark/cases",
    # Each entry is "agent" or "agent:ablation".
    [string[]]$Arms = @("advanced")
)

$ErrorActionPreference = "Stop"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# `pwsh -File script.ps1 -Arms a,b,c` hands the whole list over as one string,
# so split again on commas and drop blanks. Harmless when the caller already
# passed a real array.
$Arms = $Arms | ForEach-Object { $_ -split "," } | Where-Object { $_ -ne "" }

Write-Output "Building..."
cargo build --release --quiet

$overall = [Diagnostics.Stopwatch]::StartNew()

for ($t = 1; $t -le $Trials; $t++) {
    $out = Join-Path $Root "t$t"
    Write-Output ""
    Write-Output "===================== TRIAL $t of $Trials -> $out ====================="

    foreach ($spec in $Arms) {
        $parts = $spec.Split(":")
        $agent = $parts[0]
        $abl = if ($parts.Length -gt 1) { $parts[1] } else { "none" }

        $sw = [Diagnostics.Stopwatch]::StartNew()
        Write-Output ""
        Write-Output "--- [$spec] running ---"

        cargo run --release --quiet --bin vcr -- run `
            --agent $agent --ablation $abl --benchmark $Benchmark --out $out |
            Select-Object -Last 1

        cargo run --release --quiet --bin vcr -- evaluate `
            --agent $agent --ablation $abl --benchmark $Benchmark --out $out |
            Select-String "precision|recall|  F1|false positives/case|cost/case|evidence accuracy|RealIssue|Trap|Challenging"

        Write-Output "--- [$spec] done in $([math]::Round($sw.Elapsed.TotalMinutes,1)) min ---"
    }
}

Write-Output ""
Write-Output "===================== COMPLETE in $([math]::Round($overall.Elapsed.TotalMinutes,1)) min ====================="
cargo run --release --quiet --bin vcr -- variance --root $Root
