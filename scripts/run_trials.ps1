# Run the full experiment sweep: N trials of each arm, evaluated.
#
# LLM output is nondeterministic even at temperature 0, so a single run of an
# arm is a sample rather than a measurement. This runs each arm repeatedly so
# `vcr variance` can report the spread and name the cases that move.
#
#     pwsh scripts/run_trials.ps1 -Trials 3 -Root results-trials
#
# Each trial writes into <Root>/t<N>/ with the same layout as a normal run, so
# every artifact stays inspectable on its own.

param(
    [int]$Trials = 3,
    [string]$Root = "results-trials",
    [string]$Benchmark = "benchmark/cases"
)

$ErrorActionPreference = "Stop"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# arm label -> arguments. The ablations exist to measure what each stage of the
# pipeline contributes, rather than asserting it.
$arms = @(
    @{ Name = "baseline";                     Agent = "baseline"; Ablation = "none" },
    @{ Name = "advanced";                     Agent = "advanced"; Ablation = "none" },
    @{ Name = "advanced-no-falsification";    Agent = "advanced"; Ablation = "no-falsification" }
)

Write-Output "Building..."
cargo build --release --quiet

$overall = [Diagnostics.Stopwatch]::StartNew()

for ($t = 1; $t -le $Trials; $t++) {
    $out = Join-Path $Root "t$t"
    Write-Output ""
    Write-Output "=============================================================="
    Write-Output " TRIAL $t of $Trials  ->  $out"
    Write-Output "=============================================================="

    foreach ($arm in $arms) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        Write-Output ""
        Write-Output "--- [$($arm.Name)] running ---"

        cargo run --release --quiet --bin vcr -- run `
            --agent $arm.Agent `
            --ablation $arm.Ablation `
            --benchmark $Benchmark `
            --out $out | Select-Object -Last 2

        cargo run --release --quiet --bin vcr -- evaluate `
            --agent $arm.Agent `
            --ablation $arm.Ablation `
            --benchmark $Benchmark `
            --out $out | Select-String "precision|recall|F1|cost/case|evidence accuracy"

        Write-Output "--- [$($arm.Name)] done in $([math]::Round($sw.Elapsed.TotalMinutes,1)) min ---"
    }
}

Write-Output ""
Write-Output "=============================================================="
Write-Output " ALL TRIALS COMPLETE in $([math]::Round($overall.Elapsed.TotalMinutes,1)) min"
Write-Output "=============================================================="
cargo run --release --quiet --bin vcr -- variance --root $Root
